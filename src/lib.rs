//! Trindle's own screen reader.
pub const APP: &str = "trindle-screen-reader";

pub fn runtime_dir() -> Option<std::path::PathBuf> {
    Some(trindle_common::files::private_runtime_dir().ok()?.join(APP))
}

pub mod browse;
pub mod bus;
pub mod caret;
pub mod coordinator;
pub mod cues;
pub mod describe;
pub mod handover;
pub mod keys;
pub mod keysock;
pub mod locksock;
pub mod log;
pub mod novoice;
pub mod model;
pub mod review;
pub mod sayall;
pub mod scenario;
pub mod voice;
pub mod words;
pub mod supervisor;
pub mod track;
pub mod settings;
#[cfg(test)]
mod testenv {
    pub static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
}
