//! The lock screen's event socket: the lock program sends one fixed-form line per event, and the reader says it.

use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncReadExt};
use tokio::sync::mpsc;

pub const MAX_LINE: usize = 64;

pub const LOCK_EXE: &str = "/usr/lib/trindle/bin/trindle";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LockEvent {
    Locked,
    Time(u8, u8),
    Entry(usize),
    Wrong,
    Unlocked,
}

pub fn parse_line(line: &str) -> Option<LockEvent> {
    let line = line.trim_end_matches(['\n', '\r']);
    let (word, arg) = line.split_once(' ').map_or((line, None), |(w, a)| (w, Some(a)));
    match (word, arg) {
        ("locked", None) => Some(LockEvent::Locked),
        ("wrong", None) => Some(LockEvent::Wrong),
        ("unlocked", None) => Some(LockEvent::Unlocked),
        ("entry", Some(n)) if n.len() <= 3 && n.bytes().all(|b| b.is_ascii_digit()) => n.parse().ok().map(LockEvent::Entry),
        ("time", Some(t)) => {
            let (h, m) = t.split_once(':')?;
            let ok = |s: &str| s.len() == 2 && s.bytes().all(|b| b.is_ascii_digit());
            if !ok(h) || !ok(m) {
                return None;
            }
            let (h, m): (u8, u8) = (h.parse().ok()?, m.parse().ok()?);
            (h < 24 && m < 60).then_some(LockEvent::Time(h, m))
        }
        _ => None,
    }
}

pub struct LockSock {
    pub path: PathBuf,
}

async fn read_events(rd: tokio::net::unix::OwnedReadHalf, tx: mpsc::Sender<LockEvent>) {
    let mut rd = tokio::io::BufReader::new(rd);
    loop {
        let mut buf = Vec::new();
        let read = (&mut rd).take(MAX_LINE as u64 + 1).read_until(b'\n', &mut buf).await;
        if !matches!(read, Ok(n) if n > 0) || buf.last() != Some(&b'\n') {
            return;
        }
        if let Some(e) = parse_line(&String::from_utf8_lossy(&buf)) {
            let _ = tx.try_send(e);
        }
    }
}

pub fn listen(dir: &Path, tx: mpsc::Sender<LockEvent>) -> Result<LockSock, String> {
    listen_for(dir, tx, PathBuf::from(LOCK_EXE))
}

fn listen_for(dir: &Path, tx: mpsc::Sender<LockEvent>, lock: PathBuf) -> Result<LockSock, String> {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("events.sock");
    let dir = dir.to_path_buf();
    let _ = std::fs::remove_file(&path);
    let listener = tokio::net::UnixListener::bind(&path).map_err(|e| e.to_string())?;
    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    tokio::task::spawn_local(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let me = rustix::process::geteuid().as_raw();
            let Ok(cred) = stream.peer_cred() else { continue };
            if cred.uid() != me || !crate::keysock::peer_trusted(&cred, &lock, &dir) {
                crate::log::note("lock socket: refused a peer that is not the lock screen");
                continue;
            }
            tokio::task::spawn_local(read_events(stream.into_split().0, tx.clone()));
        }
    });
    Ok(LockSock { path })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;
    use tokio::net::UnixStream;

    fn local<F: std::future::Future>(f: F) -> F::Output {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        tokio::task::LocalSet::new().block_on(&rt, f)
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("trindle-sr-lk-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_five_lines_parse_and_anything_else_does_not() {
        assert_eq!(parse_line("locked\n"), Some(LockEvent::Locked));
        assert_eq!(parse_line("wrong\n"), Some(LockEvent::Wrong));
        assert_eq!(parse_line("unlocked\n"), Some(LockEvent::Unlocked));
        assert_eq!(parse_line("time 09:05\n"), Some(LockEvent::Time(9, 5)));
        assert_eq!(parse_line("entry 0\n"), Some(LockEvent::Entry(0)));
        assert_eq!(parse_line("entry 12\n"), Some(LockEvent::Entry(12)));
        for bad in ["", "locked now", "time 9:05", "time 24:00", "time 12:60", "time ab:cd", "entry", "entry -1", "entry 1234", "entry hunter2", "say hello", "LOCKED"] {
            assert_eq!(parse_line(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn a_line_never_carries_text_that_could_be_a_password() {
        assert_eq!(parse_line("entry hunter2"), None);
        assert_eq!(parse_line("wrong hunter2"), None);
    }

    #[test]
    fn events_from_the_lock_program_arrive() {
        local(async {
            let dir = temp("ok");
            let (tx, mut rx) = mpsc::channel(8);
            let exe = std::env::current_exe().unwrap();
            let ls = listen_for(&dir, tx, exe).unwrap();
            let mut c = UnixStream::connect(&ls.path).await.unwrap();
            c.write_all(b"locked\ntime 10:30\nentry 3\nbogus\nwrong\nunlocked\n").await.unwrap();
            for want in [LockEvent::Locked, LockEvent::Time(10, 30), LockEvent::Entry(3), LockEvent::Wrong, LockEvent::Unlocked] {
                assert_eq!(rx.recv().await, Some(want));
            }
            let _ = std::fs::remove_dir_all(dir);
        });
    }

    #[test]
    fn a_peer_that_is_not_the_lock_program_is_ignored() {
        local(async {
            let dir = temp("refused");
            let (tx, mut rx) = mpsc::channel(8);
            let ls = listen_for(&dir, tx, PathBuf::from("/usr/bin/not-the-lock")).unwrap();
            let mut c = UnixStream::connect(&ls.path).await.unwrap();
            let _ = c.write_all(b"locked\n").await;
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            assert!(rx.try_recv().is_err());
            let _ = std::fs::remove_dir_all(dir);
        });
    }

    #[test]
    fn a_long_line_ends_the_connection() {
        local(async {
            let dir = temp("long");
            let (tx, mut rx) = mpsc::channel(8);
            let ls = listen_for(&dir, tx, std::env::current_exe().unwrap()).unwrap();
            let mut c = UnixStream::connect(&ls.path).await.unwrap();
            c.write_all(&[b'x'; 200]).await.unwrap();
            let mut b = [0u8; 1];
            assert_eq!(tokio::io::AsyncReadExt::read(&mut c, &mut b).await.unwrap(), 0);
            assert!(rx.try_recv().is_err());
            let _ = std::fs::remove_dir_all(dir);
        });
    }
}
