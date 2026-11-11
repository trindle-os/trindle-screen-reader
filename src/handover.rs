//! Login to session hand-over lock, so two voices never overlap and a stuck lock never leaves silence.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::PathBuf;
use std::time::Duration;

pub const GREETER_ENV: &str = "TRINDLE_SR_CONTEXT";
pub const LOCK_ENV: &str = "TRINDLE_SR_HANDOVER_LOCK";
pub const MAX_WAIT: Duration = Duration::from_secs(5);
const STEP: Duration = Duration::from_millis(50);

pub fn lock_path() -> PathBuf {
    std::env::var_os(LOCK_ENV).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/run/trindle/greeter/screen-reader-handover.lock"))
}

pub fn is_greeter() -> bool {
    std::env::var(GREETER_ENV).is_ok_and(|v| v == "greeter")
}

fn open(path: &std::path::Path, create: bool) -> Option<File> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
    let made = if create {
        match OpenOptions::new().write(true).create_new(true).mode(0o644).custom_flags(libc_nofollow()).open(path) {
            Ok(f) => Some(f),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => None,
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    crate::log::note(&format!("hand-over lock {} cannot be made: {e}", path.display()));
                }
                return None;
            }
        }
    } else {
        None
    };
    let f = match made {
        Some(f) => {
            if let Err(e) = f.set_permissions(std::fs::Permissions::from_mode(0o644)) {
                crate::log::note(&format!("hand-over lock {} cannot be opened to readers: {e}", path.display()));
                return None;
            }
            f
        }
        None => match OpenOptions::new().read(true).custom_flags(libc_nofollow()).open(path) {
            Ok(f) => f,
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    crate::log::note(&format!("hand-over lock {} cannot be opened: {e}", path.display()));
                }
                return None;
            }
        },
    };
    let meta = f.metadata().ok()?;
    let folder = path.parent().and_then(|p| std::fs::metadata(p).ok()).map(|m| m.uid());
    let me = rustix::process::geteuid().as_raw();
    if !meta.is_file() || ![Some(0), folder, Some(me)].contains(&Some(meta.uid())) {
        crate::log::note(&format!("hand-over lock {} is not a plain file of the system", path.display()));
        return None;
    }
    Some(f)
}

fn libc_nofollow() -> i32 {
    rustix::fs::OFlags::NOFOLLOW.bits() as i32
}

pub struct Held {
    _file: File,
}

pub fn take(path: &std::path::Path) -> Option<Held> {
    let f = open(path, true)?;
    match f.try_lock() {
        Ok(()) => Some(Held { _file: f }),
        Err(TryLockError::WouldBlock) | Err(TryLockError::Error(_)) => None,
    }
}

pub async fn wait_until_free(path: &std::path::Path, max: Duration) -> bool {
    let Some(f) = open(path, false) else { return true };
    let mut waited = Duration::ZERO;
    loop {
        match f.try_lock() {
            Ok(()) => return true,
            Err(_) if waited >= max => return false,
            Err(_) => {
                tokio::time::sleep(STEP).await;
                waited += STEP;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("trindle-sr-{name}-{}", std::process::id()));
        let _ = std::fs::remove_file(&d);
        d
    }

    #[tokio::test]
    async fn waits_for_the_greeter_then_goes_ahead() {
        let p = temp("wait");
        let held = take(&p).expect("greeter takes the lock");
        assert!(take(&p).is_none(), "a second greeter cannot");
        let p2 = p.clone();
        let waiter = tokio::spawn(async move { wait_until_free(&p2, Duration::from_secs(3)).await });
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(!waiter.is_finished());
        drop(held);
        assert!(waiter.await.unwrap());
        let _ = std::fs::remove_file(p);
    }

    #[tokio::test]
    async fn gives_up_after_the_limit_rather_than_stay_silent() {
        let p = temp("giveup");
        let _held = take(&p).unwrap();
        assert!(!wait_until_free(&p, Duration::from_millis(200)).await);
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn the_lock_file_is_readable_by_the_session_user() {
        use std::os::unix::fs::PermissionsExt;
        let p = temp("mode");
        let _held = take(&p).unwrap();
        assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o644);
        let _ = std::fs::remove_file(p);
    }

    #[tokio::test]
    async fn a_read_only_lock_file_still_waits() {
        use std::os::unix::fs::PermissionsExt;
        let p = temp("ro");
        let held = take(&p).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o444)).unwrap();
        assert!(!wait_until_free(&p, Duration::from_millis(150)).await);
        drop(held);
        assert!(wait_until_free(&p, Duration::from_millis(150)).await);
        let _ = std::fs::remove_file(p);
    }

    #[tokio::test]
    async fn a_link_in_place_of_the_lock_is_not_followed_or_changed() {
        use std::os::unix::fs::PermissionsExt;
        let target = temp("target");
        std::fs::write(&target, "x").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
        let link = temp("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(take(&link).is_none(), "the greeter's reader refuses a link");
        assert!(wait_until_free(&link, Duration::from_millis(100)).await, "the session's reader goes ahead without a lock");
        assert_eq!(std::fs::metadata(&target).unwrap().permissions().mode() & 0o777, 0o600);
        let _ = std::fs::remove_file(link);
        let _ = std::fs::remove_file(target);
    }

    #[tokio::test]
    async fn a_session_reader_never_makes_the_file() {
        let p = temp("absent");
        assert!(wait_until_free(&p, Duration::from_millis(100)).await);
        assert!(!p.exists());
    }

    #[tokio::test]
    async fn no_lock_file_means_no_wait() {
        assert!(wait_until_free(std::path::Path::new("/nonexistent-dir/x.lock"), Duration::from_secs(1)).await);
    }
}
