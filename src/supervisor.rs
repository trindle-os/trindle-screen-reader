//! Starting, stopping and watching the reader; the watcher restarts it when it dies.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const RESTARTED_ENV: &str = "TRINDLE_SR_RESTARTED";
const CRASH_BURST: usize = 5;
const CRASH_WINDOW: Duration = Duration::from_secs(10);
const CALM_DOWN: Duration = Duration::from_secs(2);

fn pid_file() -> Option<PathBuf> {
    Some(crate::runtime_dir()?.join("supervisor.pid"))
}

pub fn restart_delay(recent_deaths: usize) -> Duration {
    if recent_deaths >= CRASH_BURST { CALM_DOWN } else { Duration::ZERO }
}

const GOOD_RUN: Duration = Duration::from_secs(5);

pub fn next_restarted(was: bool, ran: Duration) -> bool {
    was || ran >= GOOD_RUN
}

fn is_watcher(cmdline: &[u8]) -> bool {
    let mut parts = cmdline.split(|b| *b == 0).filter(|p| !p.is_empty());
    let prog = parts.next().map(|p| String::from_utf8_lossy(p).into_owned()).unwrap_or_default();
    prog.rsplit('/').next() == Some(crate::APP) && parts.any(|p| p == b"supervise")
}

fn alive(pid: u32) -> bool {
    std::fs::read(format!("/proc/{pid}/cmdline")).is_ok_and(|c| is_watcher(&c))
}

fn read_pid() -> Option<u32> {
    std::fs::read_to_string(pid_file()?).ok()?.trim().parse().ok().filter(|p| alive(*p))
}

pub fn is_running() -> bool {
    read_pid().is_some()
}

fn signal(pid: u32, sig: rustix::process::Signal) {
    if let Some(pid) = i32::try_from(pid).ok().and_then(rustix::process::Pid::from_raw) {
        let _ = rustix::process::kill_process(pid, sig);
    }
}

pub fn start() -> Result<(), String> {
    pid_file().ok_or("no runtime folder (XDG_RUNTIME_DIR), so the reader cannot be started")?;
    if is_running() {
        return Ok(());
    }
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut cmd = Command::new(me);
    cmd.arg("supervise").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    cmd.spawn().map_err(|e| e.to_string())?;
    for _ in 0..30 {
        if is_running() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err("the watcher did not start".into())
}

pub fn stop() -> Result<(), String> {
    let Some(pid) = read_pid() else { return Ok(()) };
    signal(pid, rustix::process::Signal::TERM);
    for _ in 0..50 {
        if !alive(pid) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    signal(pid, rustix::process::Signal::KILL);
    Ok(())
}

fn claim_pid_file() -> Result<Option<File>, String> {
    let path = pid_file().ok_or("no runtime folder (XDG_RUNTIME_DIR)")?;
    std::fs::create_dir_all(path.parent().ok_or("bad path")?).map_err(|e| e.to_string())?;
    let mut f = OpenOptions::new().create(true).truncate(false).write(true).read(true).mode(0o600).open(&path).map_err(|e| e.to_string())?;
    match f.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => return Ok(None),
        Err(TryLockError::Error(e)) => return Err(e.to_string()),
    }
    f.set_len(0).map_err(|e| e.to_string())?;
    f.write_all(std::process::id().to_string().as_bytes()).map_err(|e| e.to_string())?;
    Ok(Some(f))
}

pub async fn supervise() -> Result<(), String> {
    use tokio::signal::unix::{signal as sig, SignalKind};
    let Some(_lock) = claim_pid_file()? else { return Ok(()) };
    let mut term = sig(SignalKind::terminate()).map_err(|e| e.to_string())?;
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut deaths: Vec<Instant> = Vec::new();
    let mut restarted = false;
    loop {
        let mut cmd = tokio::process::Command::new(&me);
        cmd.arg("run");
        if restarted {
            cmd.env(RESTARTED_ENV, "1");
        }
        let began = Instant::now();
        match cmd.spawn() {
            Ok(mut child) => {
                tokio::select! {
                    _ = child.wait() => {}
                    _ = term.recv() => {
                        if let Some(pid) = child.id() {
                            signal(pid, rustix::process::Signal::TERM);
                        }
                        if tokio::time::timeout(Duration::from_secs(3), child.wait()).await.is_err() {
                            let _ = child.kill().await;
                        }
                        break;
                    }
                }
            }
            Err(e) => {
                crate::log::note(&format!("could not start the reader, trying again: {e}"));
                tokio::select! {
                    _ = tokio::time::sleep(CALM_DOWN) => {}
                    _ = term.recv() => break,
                }
                continue;
            }
        }
        restarted = next_restarted(restarted, began.elapsed());
        let now = Instant::now();
        deaths.retain(|t| now.duration_since(*t) < CRASH_WINDOW);
        deaths.push(now);
        let wait = restart_delay(deaths.len());
        if !wait.is_zero() {
            tokio::select! {
                _ = tokio::time::sleep(wait) => {}
                _ = term.recv() => break,
            }
        }
    }
    if let Some(p) = pid_file() {
        let _ = std::fs::remove_file(p);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restarts_at_once_until_it_keeps_dying() {
        assert_eq!(restart_delay(1), Duration::ZERO);
        assert_eq!(restart_delay(4), Duration::ZERO);
        assert_eq!(restart_delay(5), CALM_DOWN);
    }

    #[test]
    fn signal_ends_a_child_and_ignores_pid_zero() {
        let _g = crate::testenv::LOCK.lock().unwrap_or_else(|e| e.into_inner());
        use std::os::unix::process::ExitStatusExt;
        signal(0, rustix::process::Signal::KILL);
        let mut child = std::process::Command::new("sleep").arg("30").spawn().unwrap();
        signal(child.id(), rustix::process::Signal::TERM);
        assert_eq!(child.wait().unwrap().signal(), Some(15));
    }

    #[test]
    fn only_a_real_watcher_counts_as_alive() {
        assert!(is_watcher(b"/usr/bin/trindle-screen-reader\0supervise\0"));
        assert!(!is_watcher(b"/usr/bin/trindle-screen-reader\0run\0"));
        assert!(!is_watcher(b"vim\0trindle-screen-reader\0supervise\0"));
        assert!(!is_watcher(b""));
    }

    #[test]
    fn a_failed_start_does_not_turn_the_welcome_into_restarted() {
        assert!(!next_restarted(false, Duration::from_millis(300)));
        assert!(next_restarted(false, Duration::from_secs(60)));
        assert!(next_restarted(true, Duration::from_millis(300)));
    }

    #[test]
    fn a_second_watcher_cannot_take_the_pid_file() {
        let _g = crate::testenv::LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let old = std::env::var_os("XDG_RUNTIME_DIR");
        let d = std::env::temp_dir().join(format!("trindle-sr-sup-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::set_permissions(&d, std::os::unix::fs::PermissionsExt::from_mode(0o700)).unwrap();
        std::env::set_var("XDG_RUNTIME_DIR", &d);
        let first = claim_pid_file().unwrap();
        assert!(first.is_some());
        assert!(claim_pid_file().unwrap().is_none());
        drop(first);
        assert!(claim_pid_file().unwrap().is_some());
        std::env::remove_var("XDG_RUNTIME_DIR");
        assert!(start().is_err());
        assert!(!is_running());
        if let Some(v) = old { std::env::set_var("XDG_RUNTIME_DIR", v) }
        let _ = std::fs::remove_dir_all(d);
    }
}
