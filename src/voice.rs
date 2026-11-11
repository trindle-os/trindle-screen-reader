//! The reader's link to the shared voice service (see VOICE-INTERFACE.md).

use crate::coordinator::Out;
use crate::model::Priority;
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::mpsc;

#[derive(Debug, PartialEq, Eq)]
pub enum Note {
    Started(u64),
    Done(u64),
    Lost,
}

pub fn socket_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("TRINDLE_VOICE_SOCKET") {
        return Some(PathBuf::from(p));
    }
    Some(trindle_common::files::private_runtime_dir().ok()?.join("trindle-voice/say.sock"))
}

fn is_line_end(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

pub fn encode(o: &Out) -> String {
    encode_with(o, None)
}

pub fn encode_with(o: &Out, levels: Option<(u8, u8)>) -> String {
    match o {
        Out::Stop => "STOP\n\n".to_string(),
        Out::Say { id, text, priority } => {
            let p = match priority {
                Priority::Urgent => "urgent",
                Priority::Focus => "focus",
                Priority::Normal => "normal",
                Priority::Background => "background",
            };
            let text: String = text.chars().map(|c| if is_line_end(c) || c.is_control() { ' ' } else { c }).collect();
            let lv = levels.map(|(r, v)| format!("rate={r}\nvolume={v}\n")).unwrap_or_default();
            format!("SAY\nid={id}\npriority={p}\n{lv}text={text}\n\n")
        }
    }
}

pub fn encode_cue(path: &std::path::Path, volume: u8) -> Option<String> {
    let p = path.to_str()?;
    (!p.chars().any(is_line_end)).then(|| format!("CUE\npath={p}\nvolume={}\n\n", volume.min(100)))
}

pub fn decode(line: &str) -> Option<Note> {
    let mut it = line.split_whitespace();
    let word = it.next()?;
    let id = it.find_map(|kv| kv.strip_prefix("id=")?.parse::<u64>().ok());
    match (word, id) {
        ("STARTED", Some(i)) => Some(Note::Started(i)),
        ("DONE", Some(i)) | ("CANCELLED", Some(i)) => Some(Note::Done(i)),
        ("ERROR", Some(i)) => {
            crate::log::note(&format!("voice service could not say message {i}: {}", line.trim()));
            Some(Note::Done(i))
        }
        _ => None,
    }
}

pub fn retry_delay_ms(failures: u32) -> u64 {
    (1000u64 << failures.min(4)).min(10_000)
}

pub struct Voice {
    tx: mpsc::UnboundedSender<String>,
    levels: Option<(u8, u8)>,
}

impl Voice {
    pub async fn connect(path: &std::path::Path) -> std::io::Result<(Voice, mpsc::UnboundedReceiver<Note>)> {
        let stream = UnixStream::connect(path).await?;
        let (rd, mut wr) = stream.into_split();
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let (ntx, nrx) = mpsc::unbounded_channel();
        let ntx2 = ntx.clone();
        tokio::spawn(async move {
            while let Some(m) = rx.recv().await {
                if wr.write_all(m.as_bytes()).await.is_err() {
                    let _ = ntx2.send(Note::Lost);
                    break;
                }
            }
        });
        tokio::spawn(async move {
            let mut lines = BufReader::new(rd).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                if let Some(n) = decode(&l) {
                    let _ = ntx.send(n);
                }
            }
            let _ = ntx.send(Note::Lost);
        });
        Ok((Voice { tx, levels: None }, nrx))
    }

    pub fn set_levels(&mut self, rate: u8, volume: u8) {
        self.levels = Some((rate, volume));
    }

    pub fn cue(&self, path: &std::path::Path, volume: u8) {
        if let Some(m) = encode_cue(path, volume) {
            let _ = self.tx.send(m);
        }
    }

    pub fn send(&self, o: &Out) {
        let _ = self.tx.send(encode_with(o, self.levels));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_line_ends_cannot_carry_a_second_command() {
        let o = Out::Say { id: 1, text: "a\u{85}b\u{2028}c\u{2029}d\ne".into(), priority: Priority::Normal };
        let m = encode(&o);
        assert!(m.contains("text=a b c d e\n"));
        assert_eq!(m.matches('\n').count(), 5);
        assert!(encode_cue(std::path::Path::new("/a/b\u{2028}c"), 50).is_none());
    }

    #[test]
    fn no_runtime_folder_means_no_socket_in_tmp() {
        let _g = crate::testenv::LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (a, b) = (std::env::var_os("XDG_RUNTIME_DIR"), std::env::var_os("TRINDLE_VOICE_SOCKET"));
        std::env::remove_var("XDG_RUNTIME_DIR");
        std::env::remove_var("TRINDLE_VOICE_SOCKET");
        assert_eq!(socket_path(), None);
        let d = std::env::temp_dir().join(format!("trindle-sr-voice-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::env::set_var("XDG_RUNTIME_DIR", &d);
        std::fs::set_permissions(&d, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        assert_eq!(socket_path(), None);
        std::fs::set_permissions(&d, std::os::unix::fs::PermissionsExt::from_mode(0o700)).unwrap();
        assert_eq!(socket_path(), Some(d.join("trindle-voice/say.sock")));
        let _ = std::fs::remove_dir_all(&d);
        for (k, v) in [("XDG_RUNTIME_DIR", a), ("TRINDLE_VOICE_SOCKET", b)] {
            match v { Some(v) => std::env::set_var(k, v), None => std::env::remove_var(k) }
        }
    }

    #[test]
    fn an_error_note_still_frees_the_queue() {
        assert_eq!(decode("ERROR id=4 reason=no-engine"), Some(Note::Done(4)));
    }

    #[test]
    fn retry_waits_longer_then_levels_off() {
        let d: Vec<u64> = (0..7).map(retry_delay_ms).collect();
        assert_eq!(d, [1000, 2000, 4000, 8000, 10_000, 10_000, 10_000]);
    }
}
