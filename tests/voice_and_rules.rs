use reader::coordinator::Out;
use reader::model::Priority;
use reader::voice::{self, Note, Voice};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;

#[test]
fn wire_format_is_one_message_and_cannot_smuggle_a_second() {
    let m = voice::encode(&Out::Say { id: 7, text: "Save\n\nSTOP".into(), priority: Priority::Focus });
    assert_eq!(m, "SAY\nid=7\npriority=focus\ntext=Save  STOP\n\n");
    assert_eq!(voice::encode(&Out::Stop), "STOP\n\n");
    let m = voice::encode(&Out::Say { id: 8, text: "a\u{1}5\u{1}b".into(), priority: Priority::Focus });
    assert_eq!(m, "SAY\nid=8\npriority=focus\ntext=a 5 b\n\n", "a control character cannot become a marker");
    let m = voice::encode_with(&Out::Say { id: 1, text: "Hi".into(), priority: Priority::Normal }, Some((70, 90)));
    assert_eq!(m, "SAY\nid=1\npriority=normal\nrate=70\nvolume=90\ntext=Hi\n\n");
}

#[test]
fn notes_from_the_service_are_understood() {
    assert_eq!(voice::decode("STARTED id=4"), Some(Note::Started(4)));
    assert_eq!(voice::decode("DONE id=4"), Some(Note::Done(4)));
    assert_eq!(voice::decode("CANCELLED id=9"), Some(Note::Done(9)));
    assert_eq!(voice::decode("NONSENSE"), None);
}

#[tokio::test(flavor = "current_thread")]
async fn a_fake_voice_service_hears_the_words_and_answers() {
    let dir = std::env::temp_dir().join(format!("sr-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("say.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let server = tokio::spawn(async move {
        let (s, _) = listener.accept().await.unwrap();
        let (rd, mut wr) = s.into_split();
        let mut lines = BufReader::new(rd).lines();
        let mut got = Vec::new();
        while let Ok(Some(l)) = lines.next_line().await {
            if l.is_empty() {
                break;
            }
            got.push(l);
        }
        wr.write_all(b"STARTED id=1\nDONE id=1\n").await.unwrap();
        got
    });
    let (v, mut notes) = Voice::connect(&path).await.unwrap();
    v.send(&Out::Say { id: 1, text: "Hello".into(), priority: Priority::Normal });
    assert_eq!(notes.recv().await, Some(Note::Started(1)));
    assert_eq!(notes.recv().await, Some(Note::Done(1)));
    let got = server.await.unwrap();
    assert_eq!(got, vec!["SAY", "id=1", "priority=normal", "text=Hello"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn nothing_names_a_banned_speech_program() {
    let banned = [["esp", "eak"].concat(), ["speech", "-dispatcher"].concat(), ["libspeech", "d"].concat(), ["or", "ca"].concat() + " "];
    let mut files = vec![std::path::PathBuf::from("Cargo.toml"), std::path::PathBuf::from("Cargo.lock")];
    fn walk(d: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out)
            } else if p.extension().is_some_and(|x| x == "rs" || x == "toml") && !p.ends_with("voice_and_rules.rs") {
                out.push(p)
            }
        }
    }
    for d in ["src", "examples", "tests"] {
        walk(std::path::Path::new(d), &mut files);
    }
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap().to_lowercase();
        for b in &banned {
            assert!(!text.contains(b.as_str()), "{} names {b}", f.display());
        }
    }
}

#[test]
fn cue_message_is_one_command() {
    let m = voice::encode_cue(std::path::Path::new("/a/b.wav"), 250).unwrap();
    assert_eq!(m, "CUE\npath=/a/b.wav\nvolume=100\n\n");
    assert!(voice::encode_cue(std::path::Path::new("/a\nSTOP"), 50).is_none());
}
