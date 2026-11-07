//! The warning for a missing voice service: a low double tone played straight to the speakers.

use crate::cues::{self, Cue};
use std::ffi::{OsStr, OsString};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub const REPEAT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Player {
    PwPlay,
    Paplay,
    Aplay,
}

const ORDER: [Player; 3] = [Player::PwPlay, Player::Paplay, Player::Aplay];

impl Player {
    pub fn program(self) -> &'static str {
        match self {
            Player::PwPlay => "pw-play",
            Player::Paplay => "paplay",
            Player::Aplay => "aplay",
        }
    }

    pub fn args(self, file: &Path) -> Vec<OsString> {
        let mut a = Vec::new();
        if self == Player::Aplay {
            a.push("-q".into());
        }
        a.push(file.as_os_str().to_owned());
        a
    }
}

pub fn find_players(path: &OsStr, is_exec: impl Fn(&Path) -> bool) -> Vec<(Player, PathBuf)> {
    let dirs: Vec<PathBuf> = std::env::split_paths(path).filter(|d| d.is_absolute()).collect();
    ORDER
        .iter()
        .filter_map(|&p| dirs.iter().map(|d| d.join(p.program())).find(|f| is_exec(f)).map(|f| (p, f)))
        .collect()
}

pub fn is_executable(f: &Path) -> bool {
    std::fs::metadata(f).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[derive(Default)]
pub struct Alarm {
    last: Option<Instant>,
}

impl Alarm {
    pub fn wait(&self, now: Instant, voice_present: bool) -> Option<Duration> {
        if voice_present {
            return None;
        }
        Some(self.last.map_or(Duration::ZERO, |t| REPEAT.saturating_sub(now.saturating_duration_since(t))))
    }

    pub fn due(&mut self, now: Instant, voice_present: bool) -> bool {
        if voice_present {
            self.last = None;
            return false;
        }
        if self.last.is_some_and(|t| now.saturating_duration_since(t) < REPEAT) {
            return false;
        }
        self.last = Some(now);
        true
    }
}

pub struct Fallback {
    players: Vec<(Player, PathBuf)>,
    dir: Option<PathBuf>,
    child: Option<Child>,
    at: usize,
    file: Option<PathBuf>,
    good: usize,
    complained: bool,
}

impl Fallback {
    pub fn new() -> Self {
        let players = std::env::var_os("PATH").map(|p| find_players(&p, is_executable)).unwrap_or_default();
        Self::with(players, crate::runtime_dir())
    }

    pub fn with(players: Vec<(Player, PathBuf)>, dir: Option<PathBuf>) -> Self {
        Fallback { players, dir, child: None, at: 0, file: None, good: 0, complained: false }
    }

    pub fn running(&self) -> bool {
        self.child.is_some()
    }

    pub fn busy(&mut self) -> bool {
        let Some(c) = self.child.as_mut() else { return false };
        let status = match c.try_wait() {
            Ok(None) => return true,
            Ok(Some(st)) => st.success(),
            Err(_) => false,
        };
        self.child = None;
        if status {
            self.good = self.at;
            return false;
        }
        match self.file.clone() {
            Some(file) if self.start(&file, self.at + 1) => true,
            _ => {
                self.complain("no sound player (pw-play, paplay or aplay) could play the no-voice tone");
                false
            }
        }
    }

    pub fn sound(&mut self) {
        if self.busy() {
            return;
        }
        let Some(file) = self.dir.as_deref().and_then(ensure_file) else {
            return self.complain("no runtime folder, so the no-voice tone cannot play");
        };
        self.file = Some(file.clone());
        if !self.start(&file, self.good) && !self.start(&file, 0) {
            self.complain("no sound player (pw-play, paplay or aplay) could play the no-voice tone");
        }
    }

    fn start(&mut self, file: &Path, from: usize) -> bool {
        for (i, (p, exe)) in self.players.iter().enumerate().skip(from) {
            let spawned = Command::new(exe)
                .args(p.args(file))
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .process_group(0)
                .spawn();
            if let Ok(c) = spawned {
                self.child = Some(c);
                self.at = i;
                return true;
            }
        }
        false
    }

    fn complain(&mut self, what: &str) {
        if !std::mem::replace(&mut self.complained, true) {
            crate::log::note(what);
        }
    }
}

impl Default for Fallback {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Fallback {
    fn drop(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

fn ensure_file(dir: &Path) -> Option<PathBuf> {
    let f = cues::path(dir, Cue::NoVoice);
    let want = cues::wav(Cue::NoVoice);
    if std::fs::read(&f).is_ok_and(|b| b == want) {
        return Some(f);
    }
    std::fs::create_dir_all(dir).ok()?;
    let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    std::fs::write(&f, want).ok()?;
    Some(f)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("trindle-sr-novoice-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
        let f = dir.join(name);
        std::fs::write(&f, body).unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
        f
    }

    fn wait_for(f: &Path) -> bool {
        for _ in 0..200 {
            if f.exists() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    #[test]
    fn players_come_in_order_and_relative_path_entries_are_skipped() {
        let all = |_: &Path| true;
        let got = find_players(OsStr::new("/b:/a"), all);
        assert_eq!(got.iter().map(|g| g.0).collect::<Vec<_>>(), ORDER);
        assert_eq!(got[0].1, PathBuf::from("/b/pw-play"));
        let only = |f: &Path| f == Path::new("/a/paplay") || f == Path::new("/b/aplay") || f == Path::new("/a/aplay");
        let got = find_players(OsStr::new("/a:/b"), only);
        assert_eq!(got, vec![(Player::Paplay, "/a/paplay".into()), (Player::Aplay, "/a/aplay".into())]);
        assert!(find_players(OsStr::new("bin::."), all).is_empty());
        assert!(find_players(OsStr::new("/a"), |_| false).is_empty());
    }

    #[test]
    fn aplay_is_asked_to_be_quiet() {
        let f = Path::new("/r/x.wav");
        assert_eq!(Player::Aplay.args(f), vec![OsString::from("-q"), "/r/x.wav".into()]);
        assert_eq!(Player::PwPlay.args(f), vec![OsString::from("/r/x.wav")]);
    }

    #[test]
    fn the_alarm_sounds_at_once_repeats_every_30_s_and_stops_when_the_voice_returns() {
        let t0 = Instant::now();
        let at = |s| t0 + Duration::from_secs(s);
        let mut a = Alarm::default();
        assert!(a.due(at(0), false));
        assert!(!a.due(at(1), false));
        assert!(!a.due(at(29), false));
        assert!(a.due(at(30), false));
        assert!(!a.due(at(59), false));
        assert!(a.due(at(60), false));
        assert!(!a.due(at(61), true));
        assert!(!a.due(at(200), true));
        assert!(a.due(at(201), false), "a fresh loss sounds at once");
    }

    #[test]
    fn the_alarm_says_how_long_to_wait() {
        let t0 = Instant::now();
        let at = |s| t0 + Duration::from_secs(s);
        let mut a = Alarm::default();
        assert_eq!(a.wait(at(0), false), Some(Duration::ZERO));
        assert_eq!(a.wait(at(0), true), None);
        assert!(a.due(at(0), false));
        assert_eq!(a.wait(at(10), false), Some(Duration::from_secs(20)));
        assert_eq!(a.wait(at(40), false), Some(Duration::ZERO));
    }

    #[test]
    fn a_found_player_is_run_with_the_tone_file_and_then_reaped() {
        let _g = crate::testenv::LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let bin = scratch("bin");
        let run = scratch("run");
        let out = bin.join("out");
        script(&bin, "pw-play", &format!("#!/bin/sh\necho \"$@\" > '{}'\n", out.display()));
        let mut f = Fallback::with(find_players(bin.as_os_str(), is_executable), Some(run.clone()));
        f.sound();
        assert!(wait_for(&out));
        let tone = run.join("no-voice.wav");
        assert_eq!(std::fs::read(&tone).unwrap(), cues::wav(Cue::NoVoice));
        assert!(std::fs::read_to_string(&out).unwrap().contains(tone.to_str().unwrap()));
        for _ in 0..200 {
            if !f.busy() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!f.busy());
        let _ = std::fs::remove_dir_all(&bin);
        let _ = std::fs::remove_dir_all(&run);
    }

    #[test]
    fn a_tone_still_sounding_is_never_overlapped() {
        let _g = crate::testenv::LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let bin = scratch("slow");
        let run = scratch("slowrun");
        let log = bin.join("log");
        script(&bin, "pw-play", &format!("#!/bin/sh\necho x >> '{}'\nsleep 2\n", log.display()));
        let mut f = Fallback::with(find_players(bin.as_os_str(), is_executable), Some(run.clone()));
        f.sound();
        assert!(wait_for(&log));
        assert!(f.busy());
        f.sound();
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(std::fs::read_to_string(&log).unwrap().lines().count(), 1);
        drop(f);
        let _ = std::fs::remove_dir_all(&bin);
        let _ = std::fs::remove_dir_all(&run);
    }

    #[test]
    fn a_player_that_cannot_start_hands_over_to_the_next() {
        let _g = crate::testenv::LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let bin = scratch("fall");
        let run = scratch("fallrun");
        let out = bin.join("out");
        script(&bin, "pw-play", "#!/nonexistent/interpreter\n");
        script(&bin, "paplay", &format!("#!/bin/sh\necho ok > '{}'\n", out.display()));
        let mut f = Fallback::with(find_players(bin.as_os_str(), is_executable), Some(run.clone()));
        f.sound();
        assert!(wait_for(&out));
        let _ = std::fs::remove_dir_all(&bin);
        let _ = std::fs::remove_dir_all(&run);
    }

    #[test]
    fn a_player_that_starts_but_fails_hands_over_to_the_next() {
        let _g = crate::testenv::LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let bin = scratch("failing");
        let run = scratch("failingrun");
        let out = bin.join("out");
        script(&bin, "pw-play", "#!/bin/sh\nexit 1\n");
        script(&bin, "paplay", &format!("#!/bin/sh\necho ok > '{}'\n", out.display()));
        let mut f = Fallback::with(find_players(bin.as_os_str(), is_executable), Some(run.clone()));
        f.sound();
        for _ in 0..200 {
            f.busy();
            if out.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(out.exists(), "the second player ran after the first failed");
        for _ in 0..200 {
            if !f.busy() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        f.sound();
        assert_eq!(f.at, 1, "the player that worked is tried first next time");
        let _ = std::fs::remove_dir_all(&bin);
        let _ = std::fs::remove_dir_all(&run);
    }

    #[test]
    fn with_no_player_or_no_runtime_folder_nothing_happens_and_nothing_panics() {
        let run = scratch("none");
        let mut f = Fallback::with(vec![], Some(run.clone()));
        f.sound();
        assert!(!f.busy());
        let mut g = Fallback::with(vec![(Player::Aplay, "/bin/true".into())], None);
        g.sound();
        assert!(!g.busy());
        let _ = std::fs::remove_dir_all(&run);
    }
}
