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

fn open(path: &std::path::Path) -> Option<File> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let made = OpenOptions::new().create(true).truncate(false).write(true).read(true).mode(0o666).open(path);
    let f = match made {
        Ok(f) => {
            let _ = f.set_permissions(std::fs::Permissions::from_mode(0o666));
            f
        }
        Err(_) => match File::open(path) {
            Ok(f) => f,
            Err(e) => {
                if path.parent().is_some_and(|p| !p.exists()) {
                    return None;
                }
                crate::log::note(&format!("hand-over lock {} cannot be opened: {e}", path.display()));
                return None;
            }
        },
    };
    Some(f)
}

pub struct Held {
    _file: File,
}

pub fn take(path: &std::path::Path) -> Option<Held> {
    let f = open(path)?;
    match f.try_lock() {
        Ok(()) => Some(Held { _file: f }),
        Err(TryLockError::WouldBlock) | Err(TryLockError::Error(_)) => None,
    }
}

pub async fn wait_until_free(path: &std::path::Path, max: Duration) -> bool {
    let Some(f) = open(path) else { return true };
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
    fn the_lock_file_is_open_to_the_session_user() {
        use std::os::unix::fs::PermissionsExt;
        let p = temp("mode");
        let _held = take(&p).unwrap();
        assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o666);
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
    async fn no_lock_file_means_no_wait() {
        assert!(wait_until_free(std::path::Path::new("/nonexistent-dir/x.lock"), Duration::from_secs(1)).await);
    }
}
