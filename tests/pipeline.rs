use reader::coordinator::*;
use reader::describe::Describer;
use reader::model::*;
use reader::scenario;

fn obj(role: Role, name: &str) -> ObjectInfo {
    ObjectInfo { role: Some(role), name: name.into(), ..Default::default() }
}

#[test]
fn focus_scenario_says_exactly_this() {
    let s = scenario::parse(include_str!("scenarios/focus.toml")).unwrap();
    let log = scenario::run(s);
    assert_eq!(
        log,
        vec![
            "0 say Focus: Hello, window",
            "2000 say Focus: Save, button",
            "4000 say Focus: Network settings, group, Remember me, check box, checked",
            "4100 stop",
            "4100 say Focus: Name, text entry, required, empty",
        ]
    );
}

#[test]
fn unchecked_box_says_not_checked() {
    let mut d = Describer::default();
    assert_eq!(d.focus(&obj(Role::CheckBox, "Remember me")), "Remember me, check box, not checked");
}

#[test]
fn unnamed_button_is_honest() {
    let mut d = Describer::default();
    assert_eq!(d.focus(&obj(Role::Button, "")), "unlabelled, button");
}

#[test]
fn password_is_never_spoken() {
    let mut d = Describer::default();
    let mut o = obj(Role::PasswordEntry, "Password");
    o.value = Some("hunter2".into());
    o.description = "hunter2 hint".into();
    d.verbosity = Verbosity::Verbose;
    let said = d.focus(&o);
    assert!(!said.contains("hunter2"), "{said}");
    assert!(d.event(&Event::ValueChanged { object: o.clone() }).is_none());
    assert!(d.event(&Event::Alert { object: o.clone(), text: "hunter2".into() }).is_none());
    let mut o2 = obj(Role::TextEntry, "PIN");
    o2.states = vec![State::Secret];
    o2.value = Some("1234".into());
    assert!(!d.focus(&o2).contains("1234"));
}

#[test]
fn hostile_name_is_capped() {
    let mut d = Describer::default();
    let o = obj(Role::Button, &"x".repeat(1_000_000));
    assert!(d.focus(&o).len() <= reader::describe::MAX_PIECE);
}

fn say(t: &str, p: Priority, s: Source) -> Utterance {
    Utterance::new(t, p, s)
}

#[test]
fn focus_cancels_older_speech_and_queue() {
    let mut c = Coordinator::new();
    c.submit(0, say("first", Priority::Normal, Source::Live));
    c.submit(10, say("waits", Priority::Normal, Source::Live));
    let out = c.submit(20, say("now", Priority::Focus, Source::Focus));
    assert!(matches!(out[0], Out::Stop));
    assert!(matches!(&out[1], Out::Say { text, .. } if text == "now"));
    assert!(c.speech_done(30, 2).is_empty());
}

#[test]
fn duplicates_within_a_second_are_spoken_once() {
    let mut c = Coordinator::new();
    assert_eq!(c.submit(0, say("Save", Priority::Focus, Source::Window)).len(), 1);
    assert!(c.submit(500, say("Save", Priority::Focus, Source::Window)).is_empty());
    assert!(!c.submit(1500, say("Save", Priority::Focus, Source::Window)).is_empty());
}

#[test]
fn tabbing_between_identical_rows_speaks_each() {
    let mut c = Coordinator::new();
    assert!(!c.submit(0, say("Delete, button", Priority::Focus, Source::Focus)).is_empty());
    assert!(!c.submit(200, say("Delete, button", Priority::Focus, Source::Focus)).is_empty());
}

#[test]
fn normal_queues_behind_and_urgent_cuts() {
    let mut c = Coordinator::new();
    c.submit(0, say("a", Priority::Normal, Source::Live));
    assert!(c.submit(1, say("b", Priority::Normal, Source::Live)).is_empty());
    let out = c.submit(2, say("alert", Priority::Urgent, Source::Live));
    assert!(matches!(out[0], Out::Stop));
    assert!(c.speech_done(3, 2).is_empty());
}

#[test]
fn background_waits_for_quiet_and_goes_stale() {
    let mut c = Coordinator::new();
    c.submit(0, say("talk", Priority::Focus, Source::Focus));
    assert!(c.submit(1, say("note", Priority::Background, Source::Notification)).is_empty());
    assert!(c.tick(6000).is_empty());
    let out = c.speech_done(6001, 1);
    assert!(out.is_empty(), "stale background note must be dropped");
}

#[test]
fn nothing_non_urgent_while_typing() {
    let mut c = Coordinator::new();
    c.key(100, false, true);
    assert!(c.submit(200, say("news", Priority::Normal, Source::Live)).is_empty());
    assert!(c.tick(500).is_empty());
    let out = c.tick(801);
    assert!(matches!(&out[0], Out::Say { text, .. } if text == "news"));
}

#[test]
fn key_stops_speech_unless_modifier_only() {
    let mut c = Coordinator::new();
    c.submit(0, say("long words", Priority::Focus, Source::Focus));
    assert!(c.key(10, true, true).is_empty());
    assert_eq!(c.key(20, false, true), vec![Out::Stop]);
}

#[test]
fn queue_never_holds_more_than_ten_seconds() {
    let mut c = Coordinator::new();
    c.submit(0, say("start", Priority::Focus, Source::Focus));
    for i in 0..50 {
        c.submit(1 + i, say(&format!("{i} {}", "word ".repeat(30)), Priority::Normal, Source::Live));
    }
    let mut said = 0u64;
    let mut id = 1;
    let mut t = 100;
    loop {
        let out = c.speech_done(t, id);
        t += 1;
        match out.first() {
            Some(Out::Say { id: n, text, .. }) => {
                said += estimate_ms(text);
                id = *n;
            }
            _ => break,
        }
    }
    assert!(said <= QUEUE_LIMIT_MS + 3000, "{said}");
}

#[test]
fn muted_is_silent_except_speech_on() {
    let mut c = Coordinator::new();
    c.submit(0, say("kept", Priority::Focus, Source::Focus));
    c.muted = true;
    assert!(c.submit(10, say("x", Priority::Focus, Source::Focus)).is_empty());
    assert!(c.submit(11, say("alert", Priority::Urgent, Source::Live)).is_empty());
    assert!(c.repeat_last(false).is_empty());
    assert!(!c.say_all_start("One. Two.", 0).iter().any(|o| matches!(o, Out::Say { .. })));
    assert!(!c.submit(12, say(reader::words::SPEECH_ON, Priority::Urgent, Source::Own)).is_empty());
}

#[test]
fn speech_that_never_reports_done_does_not_stall_the_queue() {
    let mut c = Coordinator::new();
    c.submit(0, say("one", Priority::Focus, Source::Focus));
    assert!(c.submit(10, say("two", Priority::Normal, Source::Live)).is_empty());
    assert!(c.tick(500).is_empty());
    let out = c.tick(5000);
    assert!(matches!(&out[0], Out::Say { text, .. } if text == "two"));
}

#[test]
fn losing_the_voice_releases_what_waited() {
    let mut c = Coordinator::new();
    c.submit(0, say("one", Priority::Focus, Source::Focus));
    c.submit(10, say("two", Priority::Normal, Source::Live));
    let out = c.voice_lost();
    assert!(matches!(&out[0], Out::Say { text, .. } if text == "two"));
}

#[test]
fn window_focus_and_activation_are_one_announcement() {
    let mut d = Describer::default();
    let mut c = Coordinator::new();
    let w = obj(Role::Window, "Notes");
    let a = d.event(&Event::WindowActivated { object: w.clone() }).unwrap();
    let b = d.event(&Event::Focus { object: w }).unwrap();
    assert_eq!(a, b);
    assert_eq!(c.submit(0, a).len(), 1);
    assert!(c.submit(50, b).is_empty());
}

use reader::caret;

#[test]
fn caret_reading() {
    let t = "Hello world\n\n  indent line\nlast";
    assert_eq!(caret::char_at(t, 0), "H");
    assert_eq!(caret::word_at(t, 7), "world");
    assert_eq!(caret::line_at(t, 15), "  indent line");
    assert_eq!(caret::caret_moved(t, 0, 1), "e");
    assert_eq!(caret::caret_moved(t, 4, 5), "space");
    assert_eq!(caret::caret_moved(t, 0, 6), "world");
    assert_eq!(caret::caret_moved(t, 3, 12), "blank");
    assert_eq!(caret::caret_moved(t, 12, 14), "  indent line");
    assert_eq!(caret::caret_moved(t, 4, 4), "");
}

#[test]
fn unicode_characters_are_whole() {
    assert_eq!(caret::char_at("e\u{301}x", 0), "e\u{301}");
    assert_eq!(caret::deleted("a"), "deleted a");
    assert_eq!(caret::deleted("ab"), "deleted ab");
}

fn heard(d: &mut Describer, e: Event) -> Option<String> {
    d.event(&e).map(|u| u.text)
}

fn entry() -> ObjectInfo {
    obj(Role::TextEntry, "Name")
}

#[test]
fn typing_echoes_letters_and_finished_words() {
    let mut d = Describer::default();
    let t = |before: &str, ins: &str| Event::TextInserted { object: entry(), before: before.into(), inserted: ins.into() };
    assert_eq!(heard(&mut d, t("hell", "o")).as_deref(), Some("o"));
    assert_eq!(heard(&mut d, t("hello", " ")).as_deref(), Some("hello"));
    assert_eq!(heard(&mut d, t("", " ")).as_deref(), Some("space"));
    assert_eq!(heard(&mut d, t("it", "'")).as_deref(), Some("'"));
    assert_eq!(heard(&mut d, t("", "some pasted words")).as_deref(), Some("some pasted words"));
}

#[test]
fn deleting_and_caret_moves_are_read() {
    let mut d = Describer::default();
    let del = Event::TextRemoved { object: entry(), removed: "x".into() };
    assert_eq!(heard(&mut d, del).as_deref(), Some("deleted x"));
    let mv = Event::CaretMoved { object: entry(), text: "cat dog".into(), old: 0, new: 1 };
    assert_eq!(heard(&mut d, mv).as_deref(), Some("a"));
    let same = Event::CaretMoved { object: entry(), text: "cat dog".into(), old: 3, new: 3 };
    assert_eq!(heard(&mut d, same), None);
}

#[test]
fn password_typing_and_caret_are_silent() {
    let mut d = Describer::default();
    let pw = obj(Role::PasswordEntry, "Password");
    assert_eq!(heard(&mut d, Event::TextInserted { object: pw.clone(), before: String::new(), inserted: "a".into() }), None);
    assert_eq!(heard(&mut d, Event::TextRemoved { object: pw.clone(), removed: "a".into() }), None);
    assert_eq!(heard(&mut d, Event::CaretMoved { object: pw, text: "secret".into(), old: 0, new: 3 }), None);
}

#[test]
fn same_letter_twice_is_spoken_twice() {
    let mut c = Coordinator::new();
    let u = || Utterance::new("a", Priority::Focus, Source::Typing);
    assert!(!c.submit(0, u()).is_empty());
    assert!(!c.submit(100, u()).is_empty());
}

#[test]
fn repeat_and_spell_last() {
    let mut c = Coordinator::new();
    c.submit(0, Utterance::new("Save, button", Priority::Focus, Source::Focus));
    let r = c.repeat_last(false);
    assert!(matches!(&r[..], [Out::Stop, Out::Say { text, .. }] if text == "Save, button"));
    let r = c.repeat_last(true);
    assert!(matches!(r.last(), Some(Out::Say { text, .. }) if text == "S, a, v, e, ,, space, b, u, t, t, o, n"));
}

#[test]
fn where_am_i_says_window_and_container_again() {
    let mut d = Describer::default();
    let mut o = obj(Role::Button, "Save");
    o.window = "Notes".into();
    o.container = Some(Container { id: "c".into(), role: Role::ToolBar, name: "Tools".into() });
    let first = d.focus(&o);
    let again = d.where_am_i(&o);
    assert!(first.contains("Tools") && again.starts_with("Notes, ") && again.contains("Tools"), "{again}");
}

#[test]
fn live_and_name_changes() {
    let mut d = Describer::default();
    let u = d.event(&Event::Live { text: "Saved".into(), urgent: false }).unwrap();
    assert_eq!((u.text.as_str(), u.priority), ("Saved", Priority::Background));
    let u = d.event(&Event::Live { text: "Disk full".into(), urgent: true }).unwrap();
    assert_eq!(u.priority, Priority::Urgent);
    let u = d.event(&Event::ValueChanged { object: obj(Role::Button, "Stop") }).unwrap();
    assert_eq!(u.text, "Stop");
}

#[test]
fn typing_echo_setting_is_obeyed() {
    use reader::settings::TypingEcho;
    let ins = |before: &str, i: &str| Event::TextInserted { object: entry(), before: before.into(), inserted: i.into() };
    let mut d = Describer::default();
    d.typing_echo = TypingEcho::Word;
    assert_eq!(heard(&mut d, ins("hell", "o")), None);
    assert_eq!(heard(&mut d, ins("hello", " ")).as_deref(), Some("hello"));
    d.typing_echo = TypingEcho::Character;
    assert_eq!(heard(&mut d, ins("hello", " ")).as_deref(), Some("space"));
    d.typing_echo = TypingEcho::None;
    assert_eq!(heard(&mut d, ins("hell", "o")), None);
    assert_eq!(heard(&mut d, Event::TextRemoved { object: entry(), removed: "x".into() }), None);
}

#[test]
fn say_all_reads_sentence_by_sentence_then_says_the_end() {
    let mut c = Coordinator::new();
    let first = c.say_all_start("One. Two.", 0);
    assert_eq!(first, vec![Out::Say { id: 1, text: "One.".into(), priority: Priority::Focus }]);
    let second = c.speech_done(100, 1);
    assert_eq!(second, vec![Out::Say { id: 2, text: "Two.".into(), priority: Priority::Normal }]);
    let end = c.speech_done(200, 2);
    assert_eq!(end, vec![Out::Say { id: 3, text: "End of document".into(), priority: Priority::Normal }]);
}

#[test]
fn a_key_press_stops_say_all_for_good() {
    let mut c = Coordinator::new();
    c.say_all_start("One. Two.", 0);
    assert_eq!(c.key(50, false, true), vec![Out::Stop]);
    assert!(c.speech_done(60, 1).is_empty());
}

#[test]
fn say_all_skips_forward_and_back() {
    let mut c = Coordinator::new();
    c.say_all_start("One. Two. Three.", 0);
    let o = c.say_all_skip(Some(1), None);
    assert_eq!(o[0], Out::Stop);
    assert_eq!(o[1], Out::Say { id: 2, text: "Two.".into(), priority: Priority::Focus });
    let o = c.say_all_skip(Some(-1), None);
    assert!(matches!(&o[1], Out::Say { text, .. } if text == "One."));
}

#[test]
fn urgent_message_interrupts_say_all() {
    let mut c = Coordinator::new();
    c.say_all_start("One. Two.", 0);
    let o = c.submit(10, Utterance::new("Alert", Priority::Urgent, Source::Live));
    assert!(matches!(o.last(), Some(Out::Say { text, .. }) if text == "Alert"));
    assert!(c.speech_done(20, 1).is_empty());
}
