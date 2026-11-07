//! The reader's settings file; bad values fall back to safe ones and are reported.

use crate::model::Verbosity;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const MIN_RATE: u8 = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TypingEcho {
    None,
    Character,
    Word,
    #[default]
    Both,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub verbosity: Verbosity,
    pub typing_echo: TypingEcho,
    pub rate: u8,
    pub volume: u8,
    pub sounds: bool,
    pub cue_volume: u8,
    pub laptop_layout: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { verbosity: Verbosity::Brief, typing_echo: TypingEcho::Both, rate: 50, volume: 100, sounds: true, cue_volume: 60, laptop_layout: false }
    }
}

impl TypingEcho {
    pub fn next(self) -> TypingEcho {
        match self {
            TypingEcho::None => TypingEcho::Character,
            TypingEcho::Character => TypingEcho::Word,
            TypingEcho::Word => TypingEcho::Both,
            TypingEcho::Both => TypingEcho::None,
        }
    }
}

impl Settings {
    pub fn parse(text: &str) -> (Settings, Vec<String>) {
        let mut problems = Vec::new();
        let mut s = Settings::default();
        let table = match text.parse::<toml::Table>() {
            Ok(t) => t,
            Err(e) => {
                problems.push(format!("settings file not understood, using the defaults: {}", e.message()));
                return (s, problems);
            }
        };
        macro_rules! field {
            ($name:ident) => {
                if let Some(v) = table.get(stringify!($name)) {
                    match v.clone().try_into() {
                        Ok(x) => s.$name = x,
                        Err(_) => problems.push(format!("setting {} not understood, using the default", stringify!($name))),
                    }
                }
            };
        }
        field!(verbosity);
        field!(typing_echo);
        field!(rate);
        field!(volume);
        field!(sounds);
        field!(cue_volume);
        field!(laptop_layout);
        if s.rate < MIN_RATE {
            problems.push(format!("speech rate {} is too slow, using {}", s.rate, MIN_RATE));
            s.rate = MIN_RATE;
        }
        s.rate = s.rate.min(100);
        s.volume = s.volume.min(100);
        s.cue_volume = s.cue_volume.min(100);
        (s, problems)
    }

    pub fn cues_dir() -> Option<PathBuf> {
        Some(crate::runtime_dir()?.join("cues"))
    }

    pub fn path() -> Option<PathBuf> {
        if let Some(p) = std::env::var_os("TRINDLE_SR_CONFIG") {
            return Some(PathBuf::from(p));
        }
        Some(trindle_common::files::config_home()?.join(crate::APP).join("settings.toml"))
    }

    pub fn load() -> (Settings, Vec<String>) {
        let Some(p) = Self::path() else {
            return (Settings::default(), vec!["no home folder, so settings cannot be kept".into()]);
        };
        match std::fs::read_to_string(&p) {
            Ok(t) => Self::parse(&t),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Settings::default(), Vec::new()),
            Err(e) => (Settings::default(), vec![format!("settings file could not be read, using the defaults: {e}")]),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let p = Self::path().ok_or("no home folder, so settings cannot be kept")?;
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        let text = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        trindle_common::files::write_private(&p, text.as_bytes()).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_echo_cycles_through_all_four() {
        let mut e = TypingEcho::None;
        let seen: Vec<_> = (0..4).map(|_| { e = e.next(); e }).collect();
        assert_eq!(seen, [TypingEcho::Character, TypingEcho::Word, TypingEcho::Both, TypingEcho::None]);
    }

    #[test]
    fn empty_file_is_all_defaults() {
        let (s, p) = Settings::parse("");
        assert_eq!(s, Settings::default());
        assert!(p.is_empty());
    }

    #[test]
    fn values_are_read() {
        let (s, p) = Settings::parse("verbosity = \"verbose\"\ntyping_echo = \"word\"\nrate = 70\n");
        assert_eq!((s.verbosity, s.typing_echo, s.rate), (Verbosity::Verbose, TypingEcho::Word, 70));
        assert!(p.is_empty());
    }

    #[test]
    fn rate_zero_cannot_lock_the_person_out() {
        let (s, p) = Settings::parse("rate = 0\n");
        assert_eq!(s.rate, MIN_RATE);
        assert_eq!(p.len(), 1);
    }

    #[test]
    fn damaged_file_falls_back_and_says_so() {
        let (s, p) = Settings::parse("verbosity = [[[");
        assert_eq!(s, Settings::default());
        assert_eq!(p.len(), 1);
    }

    #[test]
    fn one_bad_value_keeps_the_good_ones() {
        let (s, p) = Settings::parse("rate = 300\nvolume = 40\nverbosity = \"loud\"\nsounds = false\n");
        assert_eq!((s.volume, s.sounds, s.verbosity), (40, false, Settings::default().verbosity));
        assert_eq!(s.rate, Settings::default().rate);
        assert_eq!(p.len(), 2);
    }

    #[test]
    fn no_home_means_no_path_not_the_current_folder() {
        let _g = crate::testenv::LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let keep: Vec<_> = ["TRINDLE_SR_CONFIG", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_RUNTIME_DIR", "HOME"].iter().map(|k| (*k, std::env::var_os(k))).collect();
        for (k, _) in &keep {
            std::env::remove_var(k);
        }
        assert_eq!(Settings::path(), None);
        assert_eq!(Settings::cues_dir(), None);
        assert!(Settings::default().save().is_err());
        assert_eq!(Settings::load().1.len(), 1);
        for (k, v) in keep {
            if let Some(v) = v {
                std::env::set_var(k, v);
            }
        }
    }

    #[test]
    fn an_unreadable_file_is_reported() {
        let _g = crate::testenv::LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let old = std::env::var_os("TRINDLE_SR_CONFIG");
        let dir = std::env::temp_dir().join(format!("trindle-sr-set-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("TRINDLE_SR_CONFIG", &dir);
        let (s, p) = Settings::load();
        assert_eq!(s, Settings::default());
        assert_eq!(p.len(), 1);
        match old { Some(v) => std::env::set_var("TRINDLE_SR_CONFIG", v), None => std::env::remove_var("TRINDLE_SR_CONFIG") }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn round_trips() {
        let s = Settings { rate: 80, typing_echo: TypingEcho::None, ..Settings::default() };
        let t = toml::to_string_pretty(&s).unwrap();
        assert_eq!(Settings::parse(&t).0, s);
    }
}
