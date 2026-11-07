//! The reader's key socket: the compositor connects and sends reader keys, one line each.

use crate::keys::{parse_line, Wire};
use std::cell::RefCell;
use std::fs::{File, TryLockError};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, Notify};

pub const MAX_LINE: usize = 256;

pub const COMPOSITOR_EXE: &str = "/usr/lib/trindle/bin/trindle-compose";

pub type Back = Rc<RefCell<Option<mpsc::Sender<String>>>>;

pub struct KeySock {
    _lock: File,
    pub path: PathBuf,
}

pub fn is_compositor_exe(exe: Option<&Path>, expected: &Path) -> bool {
    let Some(exe) = exe else { return false };
    exe == expected || exe.to_str().and_then(|s| s.strip_suffix(" (deleted)")).is_some_and(|s| Path::new(s) == expected)
}

pub fn trusted_peer(pid_visible: bool, exe: Option<&Path>, expected: &Path, folder_private: bool) -> bool {
    if pid_visible {
        is_compositor_exe(exe, expected)
    } else {
        folder_private
    }
}

pub fn folder_is_private(dir: &Path) -> bool {
    std::fs::metadata(dir).is_ok_and(|m| {
        use std::os::unix::fs::MetadataExt;
        m.mode() & 0o777 == 0o700 && m.uid() == rustix::process::geteuid().as_raw()
    })
}

pub fn peer_trusted(cred: &tokio::net::unix::UCred, expected: &Path, dir: &Path) -> bool {
    let pid = cred.pid().filter(|p| *p > 0);
    let exe = pid.and_then(|p| std::fs::read_link(format!("/proc/{p}/exe")).ok());
    let private = folder_is_private(dir);
    if pid.is_none() && private {
        crate::log::note("socket: peer pid not visible, trusted because the folder is private to this user");
    }
    trusted_peer(pid.is_some(), exe.as_deref(), expected, private)
}

fn made_up(w: Wire) -> Wire {
    match w {
        Wire::Key(mut e) => {
            e.real = false;
            Wire::Key(e)
        }
        Wire::Tap { key, .. } => Wire::Tap { key, real: false },
        w => w,
    }
}

#[derive(Default)]
struct Slot {
    current: Option<Holder>,
    last_id: u64,
}

struct Holder {
    id: u64,
    trusted: bool,
    kick: Rc<Notify>,
}

impl Slot {
    fn claim(&mut self, trusted: bool) -> Option<(u64, Rc<Notify>)> {
        if let Some(h) = &self.current {
            if h.trusted || !trusted {
                return None;
            }
            h.kick.notify_one();
        }
        self.last_id += 1;
        let kick = Rc::new(Notify::new());
        self.current = Some(Holder { id: self.last_id, trusted, kick: kick.clone() });
        Some((self.last_id, kick))
    }

    fn release(&mut self, id: u64) -> bool {
        let mine = self.current.as_ref().is_some_and(|h| h.id == id);
        if mine {
            self.current = None;
        }
        mine
    }
}

async fn read_keys(rd: tokio::net::unix::OwnedReadHalf, tx: mpsc::Sender<Wire>, trusted: bool) {
    let mut rd = tokio::io::BufReader::new(rd);
    loop {
        let mut buf = Vec::new();
        let read = (&mut rd).take(MAX_LINE as u64 + 1).read_until(b'\n', &mut buf).await;
        if !matches!(read, Ok(n) if n > 0) {
            return;
        }
        if buf.last() != Some(&b'\n') {
            if buf.len() > MAX_LINE {
                crate::log::note("key socket: line too long, closing the connection");
            }
            return;
        }
        if let Some(w) = parse_line(&String::from_utf8_lossy(&buf)).map(|w| if trusted { w } else { made_up(w) }) {
            let _ = tx.try_send(w);
        }
    }
}

pub fn listen(dir: &Path, tx: mpsc::Sender<Wire>, back: Back) -> Result<Option<KeySock>, String> {
    listen_for(dir, tx, back, PathBuf::from(COMPOSITOR_EXE))
}

fn listen_for(dir: &Path, tx: mpsc::Sender<Wire>, back: Back, compositor: PathBuf) -> Result<Option<KeySock>, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    let lock = File::options().create(true).truncate(false).write(true).read(true).open(dir.join("reader.lock")).map_err(|e| e.to_string())?;
    match lock.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => return Ok(None),
        Err(TryLockError::Error(e)) => return Err(e.to_string()),
    }
    let path = dir.join("keys.sock");
    let _ = std::fs::remove_file(&path);
    let listener = tokio::net::UnixListener::bind(&path).map_err(|e| e.to_string())?;
    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    let slot = Rc::new(RefCell::new(Slot::default()));
    let dir = dir.to_path_buf();
    tokio::task::spawn_local(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let me = rustix::process::geteuid().as_raw();
            let Ok(cred) = stream.peer_cred() else { continue };
            if cred.uid() != me {
                crate::log::note("key socket: refused a connection from another user");
                continue;
            }
            let trusted = peer_trusted(&cred, &compositor, &dir);
            if !trusted {
                crate::log::note("key socket: peer is not the compositor, its keys count as made up");
            }
            let claim = slot.borrow_mut().claim(trusted);
            let Some((id, kick)) = claim else {
                crate::log::note("key socket: refused a second connection");
                continue;
            };
            if trusted {
                let _ = tx.try_send(Wire::Linked);
            }
            let (rd, mut wr) = stream.into_split();
            let (otx, mut orx) = mpsc::channel::<String>(16);
            *back.borrow_mut() = Some(otx);
            tokio::task::spawn_local(async move {
                while let Some(l) = orx.recv().await {
                    if wr.write_all(l.as_bytes()).await.is_err() {
                        break;
                    }
                }
            });
            let (tx, back, slot) = (tx.clone(), back.clone(), slot.clone());
            tokio::task::spawn_local(async move {
                tokio::select! {
                    _ = read_keys(rd, tx, trusted) => {}
                    _ = kick.notified() => crate::log::note("key socket: the compositor took over from another peer"),
                }
                if slot.borrow_mut().release(id) {
                    *back.borrow_mut() = None;
                }
            });
        }
    });
    Ok(Some(KeySock { _lock: lock, path }))
}

pub fn send_back(back: &Back, line: String) {
    if let Some(tx) = back.borrow().as_ref() {
        let _ = tx.try_send(line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;
    use tokio::net::UnixStream;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("trindle-sr-ks-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn local<F: std::future::Future>(f: F) -> F::Output {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        tokio::task::LocalSet::new().block_on(&rt, f)
    }

    fn me() -> PathBuf {
        std::env::current_exe().unwrap()
    }

    #[test]
    fn only_the_compositor_exe_is_trusted() {
        let want = Path::new(COMPOSITOR_EXE);
        assert!(is_compositor_exe(Some(want), want));
        assert!(!is_compositor_exe(Some(Path::new("/usr/bin/evil")), want));
        assert!(!is_compositor_exe(Some(Path::new("/usr/lib/trindle/bin/trindle-compose2")), want));
        assert!(is_compositor_exe(Some(Path::new("/usr/lib/trindle/bin/trindle-compose (deleted)")), want));
        assert!(!is_compositor_exe(Some(Path::new("/tmp/x/trindle-compose (deleted)")), want));
        assert!(!is_compositor_exe(None, want));
    }

    #[test]
    fn a_peer_with_no_visible_pid_is_trusted_only_through_a_private_folder() {
        let want = Path::new(COMPOSITOR_EXE);
        assert!(trusted_peer(false, None, want, true));
        assert!(!trusted_peer(false, None, want, false));
        assert!(trusted_peer(true, Some(want), want, false));
        assert!(!trusted_peer(true, Some(Path::new("/usr/bin/evil")), want, true), "a visible pid is never excused by the folder");
        assert!(!trusted_peer(true, None, want, true));
    }

    #[test]
    fn only_a_folder_closed_to_others_and_owned_by_us_is_private() {
        let d = temp("private");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(folder_is_private(&d));
        std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(!folder_is_private(&d));
        assert!(!folder_is_private(&d.join("missing")));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn a_peer_that_is_not_the_compositor_cannot_send_real_keys() {
        local(async {
            let dir = temp("untrusted");
            let (tx, mut rx) = mpsc::channel(8);
            let ks = listen(&dir, tx, Default::default()).unwrap().unwrap();
            let mut c = UnixStream::connect(&ks.path).await.unwrap();
            c.write_all(b"tap\tCaps_Lock\treal\nkey\tr\tdown\treader\treal\nstate\tcaps\ton\n").await.unwrap();
            assert!(matches!(rx.recv().await, Some(Wire::Tap { real: false, .. })));
            assert!(matches!(rx.recv().await, Some(Wire::Key(e)) if !e.real && e.reader));
            assert!(matches!(rx.recv().await, Some(Wire::CapsState(true))));
            let _ = std::fs::remove_dir_all(dir);
        });
    }

    #[test]
    fn a_trusted_peer_replaces_an_untrusted_one_but_never_the_reverse() {
        let mut slot = Slot::default();
        let (first, kick) = slot.claim(false).expect("an empty slot is free");
        assert!(slot.claim(false).is_none(), "an untrusted peer cannot take a taken slot");
        let (second, _) = slot.claim(true).expect("a trusted peer replaces an untrusted one");
        assert!(!slot.release(first), "the replaced peer no longer owns the slot");
        assert!(slot.claim(false).is_none());
        assert!(slot.claim(true).is_none(), "a trusted peer is not replaced");
        assert!(slot.release(second));
        assert!(slot.claim(true).is_some());
        drop(kick);
    }

    #[test]
    fn the_peer_that_is_replaced_is_told_to_stop() {
        local(async {
            let mut slot = Slot::default();
            let (_, kick) = slot.claim(false).unwrap();
            slot.claim(true).unwrap();
            tokio::time::timeout(std::time::Duration::from_millis(500), kick.notified()).await.expect("kicked");
        });
    }

    #[test]
    fn lines_arrive_and_replies_go_back() {
        local(async {
            let dir = temp("basic");
            let (tx, mut rx) = mpsc::channel(8);
            let back: Back = Default::default();
            let ks = listen_for(&dir, tx, back.clone(), me()).unwrap().unwrap();
            let mut c = UnixStream::connect(&ks.path).await.unwrap();
            c.write_all(b"tap\tCaps_Lock\treal\nstate\tcaps\ton\n").await.unwrap();
            assert!(matches!(rx.recv().await, Some(Wire::Linked)), "a trusted connection is announced");
            assert!(matches!(rx.recv().await, Some(Wire::Tap { real: true, .. })));
            assert!(matches!(rx.recv().await, Some(Wire::CapsState(true))));
            send_back(&back, "state\tlearn\ton\n".into());
            let mut got = vec![0u8; 15];
            c.read_exact(&mut got).await.unwrap();
            assert_eq!(got, b"state\tlearn\ton\n");
            let _ = std::fs::remove_dir_all(dir);
        });
    }

    #[test]
    fn a_second_reader_is_kept_out_and_a_second_connection_refused() {
        local(async {
            let dir = temp("single");
            let (tx, _rx) = mpsc::channel(8);
            let back: Back = Default::default();
            let ks = listen(&dir, tx.clone(), back.clone()).unwrap().unwrap();
            assert!(listen(&dir, tx, Default::default()).unwrap().is_none());
            let _first = UnixStream::connect(&ks.path).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let mut second = UnixStream::connect(&ks.path).await.unwrap();
            let mut b = [0u8; 1];
            assert_eq!(second.read(&mut b).await.unwrap(), 0, "refused connections are closed");
            let _ = std::fs::remove_dir_all(dir);
        });
    }

    #[test]
    fn a_long_line_ends_the_connection_and_a_flood_is_bounded() {
        local(async {
            let dir = temp("long");
            let (tx, mut rx) = mpsc::channel(4);
            let ks = listen_for(&dir, tx, Default::default(), me()).unwrap().unwrap();
            let mut c = UnixStream::connect(&ks.path).await.unwrap();
            c.write_all(&vec![b'x'; 600]).await.unwrap();
            let mut b = [0u8; 1];
            assert_eq!(c.read(&mut b).await.unwrap(), 0);
            let mut c = UnixStream::connect(&ks.path).await.unwrap();
            for _ in 0..50 {
                c.write_all(b"tap\tCaps_Lock\treal\n").await.unwrap();
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let mut n = 0;
            while rx.try_recv().is_ok() {
                n += 1;
            }
            assert_eq!(n, 4, "only the queue's worth is kept");
            let _ = std::fs::remove_dir_all(dir);
        });
    }
}
