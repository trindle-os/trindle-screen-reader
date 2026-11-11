//! The reader's own sound cues, made from plain tones and played by the voice service.

use std::path::{Path, PathBuf};

use crate::model::{Event, Role};

pub const RATE: u32 = 44_100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cue {
    Window,
    Dialog,
    Alert,
    Close,
    Notification,
    SpeechOn,
    SpeechOff,
    SecretKey,
    Layer,
    Restarted,
    NoVoice,
}

pub const ALL: [Cue; 11] = [
    Cue::Window,
    Cue::Dialog,
    Cue::Alert,
    Cue::Close,
    Cue::Notification,
    Cue::SpeechOn,
    Cue::SpeechOff,
    Cue::SecretKey,
    Cue::Layer,
    Cue::Restarted,
    Cue::NoVoice,
];

impl Cue {
    pub fn file_name(self) -> &'static str {
        match self {
            Cue::Window => "window.wav",
            Cue::Dialog => "dialog.wav",
            Cue::Alert => "alert.wav",
            Cue::Close => "close.wav",
            Cue::Notification => "notification.wav",
            Cue::SpeechOn => "speech-on.wav",
            Cue::SpeechOff => "speech-off.wav",
            Cue::SecretKey => "secret-key.wav",
            Cue::Layer => "layer.wav",
            Cue::Restarted => "restarted.wav",
            Cue::NoVoice => "no-voice.wav",
        }
    }

    fn notes(self) -> &'static [(f32, u32)] {
        match self {
            Cue::Window => &[(660.0, 60), (880.0, 70)],
            Cue::Dialog => &[(523.0, 70), (659.0, 70), (784.0, 90)],
            Cue::Alert => &[(880.0, 90), (0.0, 40), (880.0, 90), (0.0, 40), (1175.0, 140)],
            Cue::Close => &[(660.0, 60), (440.0, 90)],
            Cue::Notification => &[(1047.0, 50), (1319.0, 80)],
            Cue::SpeechOn => &[(900.0, 120)],
            Cue::SpeechOff => &[(220.0, 160)],
            Cue::SecretKey => &[(500.0, 25)],
            Cue::Layer => &[(740.0, 40), (0.0, 20), (740.0, 40)],
            Cue::Restarted => &[(440.0, 90), (554.0, 90), (659.0, 90), (880.0, 150)],
            Cue::NoVoice => &[(196.0, 160), (0.0, 60), (147.0, 240)],
        }
    }
}

pub fn cue_for(e: &Event) -> Option<Cue> {
    match e {
        Event::Alert { object, .. } if !object.is_secret() => Some(Cue::Alert),
        Event::WindowActivated { object } | Event::Focus { object }
            if matches!(object.role, Some(Role::Window | Role::Dialog)) =>
        {
            Some(if object.role == Some(Role::Dialog) { Cue::Dialog } else { Cue::Window })
        }
        Event::WindowClosed => Some(Cue::Close),
        Event::Notification { .. } => Some(Cue::Notification),
        Event::TextInserted { object, .. } | Event::TextRemoved { object, .. } if object.is_secret() => Some(Cue::SecretKey),
        _ => None,
    }
}

pub fn samples(c: Cue) -> Vec<i16> {
    let mut out = Vec::new();
    for &(freq, ms) in c.notes() {
        let n = (RATE as u64 * ms as u64 / 1000) as usize;
        let fade = (RATE as usize / 400).min(n / 2).max(1);
        for i in 0..n {
            let env = if i < fade {
                i as f32 / fade as f32
            } else if i + fade > n {
                (n - i) as f32 / fade as f32
            } else {
                1.0
            };
            let v = if freq == 0.0 { 0.0 } else { (std::f32::consts::TAU * freq * i as f32 / RATE as f32).sin() };
            out.push((v * env * 10_000.0) as i16);
        }
    }
    out
}

pub fn wav(c: Cue) -> Vec<u8> {
    trindle_common::wav::wav(&samples(c), RATE).unwrap_or_default()
}

pub fn path(dir: &Path, c: Cue) -> PathBuf {
    dir.join(c.file_name())
}

pub fn write_all(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    for c in ALL {
        std::fs::write(path(dir, c), wav(c))?;
    }
    Ok(())
}

#[derive(Default)]
pub struct CueGate {
    last: Option<(Cue, u64)>,
}

impl CueGate {
    pub const WINDOW_MS: u64 = 1000;

    pub fn allow(&mut self, c: Cue, now_ms: u64) -> bool {
        if c == Cue::SecretKey {
            return true;
        }
        if self.last.is_some_and(|(l, t)| l == c && now_ms.saturating_sub(t) < Self::WINDOW_MS) {
            return false;
        }
        self.last = Some((c, now_ms));
        true
    }
}

pub fn incomplete(dir: &Path) -> bool {
    ALL.iter().any(|c| !path(dir, *c).exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_cue_is_short_quiet_and_has_a_valid_header() {
        for c in ALL {
            let s = samples(c);
            let ms = s.len() as u64 * 1000 / RATE as u64;
            assert!((15..=600).contains(&ms), "{c:?} lasts {ms} ms");
            assert!(s.iter().all(|x| x.unsigned_abs() <= 10_000));
            assert!(s.iter().any(|&x| x != 0));
            assert_eq!(s[0], 0, "{c:?} starts with a click");
            let w = wav(c);
            assert_eq!(&w[..4], b"RIFF");
            assert_eq!(u32::from_le_bytes(w[4..8].try_into().unwrap()) as usize, w.len() - 8);
        }
    }

    #[test]
    fn events_pick_their_cues() {
        use crate::model::{ObjectInfo, State};
        let o = |role, secret| ObjectInfo { role: Some(role), states: if secret { vec![State::Secret] } else { vec![] }, ..Default::default() };
        let key = |object| Event::TextInserted { object, before: String::new(), inserted: "a".into() };
        assert_eq!(cue_for(&key(o(Role::PasswordEntry, true))), Some(Cue::SecretKey));
        assert_eq!(cue_for(&key(o(Role::TextEntry, false))), None);
        assert_eq!(cue_for(&Event::WindowActivated { object: o(Role::Dialog, false) }), Some(Cue::Dialog));
        assert_eq!(cue_for(&Event::Focus { object: o(Role::Button, false) }), None);
        assert_eq!(cue_for(&Event::WindowClosed), Some(Cue::Close));
    }

    #[test]
    fn file_names_are_unique() {
        let mut names: Vec<_> = ALL.iter().map(|c| c.file_name()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), ALL.len());
    }

    #[test]
    fn on_is_higher_than_off() {
        let zero_crossings = |c| samples(c).windows(2).filter(|w| (w[0] < 0) != (w[1] < 0)).count() * 1000 / samples(c).len();
        assert!(zero_crossings(Cue::SpeechOn) > zero_crossings(Cue::SpeechOff));
    }

    #[test]
    fn no_voice_is_a_low_double_tone() {
        let rate = |c| samples(c).windows(2).filter(|w| (w[0] < 0) != (w[1] < 0)).count() * 1000 / samples(c).len();
        assert!(ALL.iter().filter(|c| **c != Cue::NoVoice).all(|c| rate(*c) >= rate(Cue::SpeechOff) - 1));
        assert!(rate(Cue::NoVoice) < rate(Cue::SpeechOff));
        let s = samples(Cue::NoVoice);
        let gap = s.windows(RATE as usize / 25).any(|w| w.iter().all(|&x| x == 0));
        assert!(gap, "two tones need a pause between them");
    }

    #[test]
    fn write_all_makes_every_file_and_a_half_set_is_noticed() {
        let d = std::env::temp_dir().join(format!("trindle-sr-cues-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        assert!(incomplete(&d));
        write_all(&d).unwrap();
        assert!(!incomplete(&d));
        for c in ALL {
            assert_eq!(std::fs::read(path(&d, c)).unwrap(), wav(c));
        }
        std::fs::remove_file(path(&d, ALL[ALL.len() - 1])).unwrap();
        assert!(incomplete(&d));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_same_cue_within_a_second_plays_once() {
        let mut g = CueGate::default();
        assert!(g.allow(Cue::Window, 0));
        assert!(!g.allow(Cue::Window, 400));
        assert!(g.allow(Cue::Dialog, 450));
        assert!(g.allow(Cue::Window, 500));
        assert!(g.allow(Cue::Window, 1500));
        assert!(g.allow(Cue::SecretKey, 1600));
        assert!(g.allow(Cue::SecretKey, 1650), "every key typed into a password field clicks");
    }
}
