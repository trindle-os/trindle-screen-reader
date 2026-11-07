//! The reader's short log in its runtime folder; with no voice it is the only place a fault shows.

use std::io::Write;
use std::path::PathBuf;

const MAX_BYTES: u64 = 64 * 1024;

pub fn path() -> Option<PathBuf> {
    Some(crate::runtime_dir()?.join("log"))
}

pub fn note(msg: &str) {
    eprintln!("{msg}");
    let Some(p) = path() else { return };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::metadata(&p).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = std::fs::remove_file(&p);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&p) {
        let _ = writeln!(f, "{msg}");
    }
}
