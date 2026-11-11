use super::*;
use SettingCmd as S;

fn run(layout: Layout, evs: &[KeyEvent]) -> Vec<Vec<Action>> {
    let map = Keymap::new(layout);
    let mut st = KeyState::default();
    evs.iter().map(|e| st.handle(&map, e)).collect()
}

#[test]
fn no_two_commands_share_a_key_in_the_same_layer() {
    for layout in [Layout::Desktop, Layout::Laptop] {
        let v = default_bindings(layout);
        for (i, x) in v.iter().enumerate() {
            for y in &v[i + 1..] {
                assert!(!(x.layer == y.layer && x.combo == y.combo), "{} and {} clash", x.name, y.name);
            }
            assert!(!x.name.is_empty());
        }
    }
}

#[test]
fn the_browse_layer_jumps_by_structure_and_shift_goes_back() {
    let mut shifted = KeyEvent::press("h");
    shifted.shift = true;
    let r = run(Layout::Laptop, &[KeyEvent::with_reader("b"), KeyEvent::press("h"), KeyEvent::with_reader("b"), shifted, KeyEvent::with_reader("b"), KeyEvent::press("m")]);
    assert_eq!(r[0], vec![Action::Say(crate::words::LAYER_BROWSE.into())]);
    assert_eq!(r[1], vec![Action::Browse(BrowseCmd::Jump(Kind::Heading, true))]);
    assert_eq!(r[3], vec![Action::Browse(BrowseCmd::Jump(Kind::Heading, false))]);
    assert_eq!(r[5], vec![Action::Browse(BrowseCmd::ToggleMode)]);
}

fn sticky() -> (Keymap, KeyState) {
    let mut st = KeyState::default();
    st.set_plain_keys(true);
    (Keymap::new(Layout::Laptop), st)
}

#[test]
fn settings_layer_keys_change_each_setting_and_are_refused_when_made_up() {
    let (map, mut st) = sticky();
    for (key, want) in [
        ("v", S::Verbosity),
        ("t", S::TypingEcho),
        ("s", S::Sounds),
        ("Page_Up", S::CueStep(5)),
        ("Page_Down", S::CueStep(-5)),
        ("l", S::Layout),
    ] {
        st.handle(&map, &KeyEvent::with_reader("g"));
        assert_eq!(st.handle(&map, &KeyEvent::press(key)), vec![Action::Setting(want)], "{key}");
    }
    st.handle(&map, &KeyEvent::with_reader("g"));
    assert!(st.handle(&map, &made(KeyEvent::press("v"))).is_empty());
}

#[test]
fn browse_stays_open_for_plain_keys_until_escape() {
    let (map, mut st) = sticky();
    st.handle(&map, &KeyEvent::with_reader("b"));
    assert!(st.in_browse_keys());
    assert_eq!(st.handle(&map, &KeyEvent::press("Down")), vec![Action::Browse(BrowseCmd::Next)]);
    assert_eq!(st.handle(&map, &KeyEvent::press("h")), vec![Action::Browse(BrowseCmd::Jump(Kind::Heading, true))]);
    assert!(st.handle(&map, &KeyEvent::press("z")).is_empty());
    assert!(st.in_browse_keys());
    assert_eq!(st.handle(&map, &KeyEvent::press("Escape")), vec![Action::Say(crate::words::BROWSE_OFF.into())]);
    assert!(!st.in_browse_keys());
}

#[test]
fn a_reader_command_leaves_browse_and_runs() {
    let (map, mut st) = sticky();
    st.handle(&map, &KeyEvent::with_reader("b"));
    assert_eq!(st.handle(&map, &KeyEvent::with_reader("r")), vec![Action::RepeatLast]);
    assert!(!st.in_browse_keys());
}

#[test]
fn made_up_and_control_keys_do_nothing_in_open_browse() {
    let (map, mut st) = sticky();
    st.handle(&map, &KeyEvent::with_reader("b"));
    let mut fake = KeyEvent::press("Down");
    fake.real = false;
    assert!(st.handle(&map, &fake).is_empty());
    let mut ctrl = KeyEvent::press("Down");
    ctrl.ctrl = true;
    assert!(st.handle(&map, &ctrl).is_empty());
    assert!(st.in_browse_keys());
}

#[test]
fn without_plain_keys_from_the_compositor_browse_takes_one_key() {
    let map = Keymap::new(Layout::Laptop);
    let mut st = KeyState::default();
    st.handle(&map, &KeyEvent::with_reader("b"));
    assert!(!st.in_browse_keys());
    assert_eq!(st.handle(&map, &KeyEvent::press("Down")), vec![Action::Browse(BrowseCmd::Next)]);
    assert!(st.handle(&map, &KeyEvent::press("Down")).is_empty());
}

#[test]
fn focus_mode_and_a_new_link_close_browse() {
    let (map, mut st) = sticky();
    st.handle(&map, &KeyEvent::with_reader("b"));
    st.leave_browse();
    assert!(!st.in_browse_keys());
    st.handle(&map, &KeyEvent::with_reader("b"));
    st.set_plain_keys(true);
    assert!(!st.in_browse_keys());
}

#[test]
fn the_browse_lines_on_the_wire() {
    assert_eq!(browse_line(true), "state\tbrowse\ton\n");
    assert_eq!(browse_line(false), "state\tbrowse\toff\n");
    assert_eq!(parse_line("state\tplainkeys\tavailable\n"), Some(Wire::PlainKeys));
}

#[test]
fn reader_key_plus_letter_runs_a_command_and_plain_keys_pass_through() {
    let r = run(Layout::Laptop, &[KeyEvent::with_reader("r"), KeyEvent::press("r"), KeyEvent::press("Insert")]);
    assert_eq!(r[0], vec![Action::RepeatLast]);
    assert!(r[1].is_empty());
    assert!(r[2].is_empty());
}

#[test]
fn reader_key_alone_does_nothing() {
    let r = run(Layout::Desktop, &[KeyEvent::with_reader("Insert"), KeyEvent::with_reader("Caps_Lock")]);
    assert!(r.iter().all(Vec::is_empty));
}

#[test]
fn control_alone_stops_speech_but_control_with_a_key_does_not() {
    let mut ctrl_up = KeyEvent::press("Control");
    ctrl_up.pressed = false;
    let r = run(Layout::Desktop, &[KeyEvent::press("Control"), ctrl_up.clone()]);
    assert_eq!(r[1], vec![Action::StopSpeech]);
    let r = run(Layout::Desktop, &[KeyEvent::press("Control"), KeyEvent::press("c"), ctrl_up]);
    assert!(r[2].is_empty());
}

fn made(mut e: KeyEvent) -> KeyEvent {
    e.real = false;
    e
}

#[test]
fn made_up_keys_cannot_stop_speech_or_open_modes() {
    let mut up = KeyEvent::press("Control");
    up.pressed = false;
    let r = run(Layout::Desktop, &[made(KeyEvent::press("Control")), made(up)]);
    assert!(r[1].is_empty());
    for k in ["h", "v", "g", "p"] {
        let r = run(Layout::Laptop, &[made(KeyEvent::with_reader(k))]);
        assert_eq!(r[0], vec![Action::Say(crate::words::REFUSED.into())], "{k}");
    }
}

#[test]
fn made_up_keys_are_ignored_inside_learn_mode_and_layers() {
    let r = run(Layout::Laptop, &[KeyEvent::with_reader("h"), made(KeyEvent::press("Escape")), made(KeyEvent::press("Escape"))]);
    assert!(r[1].is_empty() && r[2].is_empty());
    let r = run(Layout::Laptop, &[KeyEvent::with_reader("v"), made(KeyEvent::press("Down")), KeyEvent::press("Down")]);
    assert!(r[1].is_empty());
    assert_eq!(r[2], vec![Action::Review(R::NextLine)]);
}

#[test]
fn shift_does_not_close_an_open_layer() {
    let r = run(Layout::Laptop, &[KeyEvent::with_reader("v"), KeyEvent::press("Shift"), KeyEvent::press("Down")]);
    assert!(r[1].is_empty());
    assert_eq!(r[2], vec![Action::Review(R::NextLine)]);
}

#[test]
fn numpad_review_exists_on_desktop_only() {
    assert_eq!(run(Layout::Desktop, &[KeyEvent::press("KP_7")])[0], vec![]);
    assert_eq!(run(Layout::Desktop, &[KeyEvent::with_reader("KP_7")])[0], vec![Action::Review(R::PrevLine)]);
    assert_eq!(run(Layout::Laptop, &[KeyEvent::with_reader("KP_7")])[0], vec![]);
    assert_eq!(run(Layout::Laptop, &[KeyEvent::with_reader("a")])[0], vec![Action::SayAll]);
}

#[test]
fn a_layer_takes_one_plain_key_then_closes_and_escape_closes_it() {
    let r = run(
        Layout::Laptop,
        &[KeyEvent::with_reader("v"), KeyEvent::press("Down"), KeyEvent::press("Down"), KeyEvent::with_reader("v"), KeyEvent::press("Escape"), KeyEvent::press("Down")],
    );
    assert_eq!(r[0], vec![Action::Say("review".into())]);
    assert_eq!(r[1], vec![Action::Review(R::NextLine)]);
    assert!(r[2].is_empty());
    assert!(r[4].is_empty() && r[5].is_empty());
}

#[test]
fn learn_mode_names_keys_without_running_them_and_two_escapes_leave() {
    let r = run(
        Layout::Laptop,
        &[KeyEvent::with_reader("h"), KeyEvent::with_reader("r"), KeyEvent::press("x"), KeyEvent::press("Escape"), KeyEvent::press("Escape"), KeyEvent::with_reader("r")],
    );
    assert!(matches!(&r[0][0], Action::Say(s) if s.starts_with("learn mode")));
    assert_eq!(r[1], vec![Action::Say("repeat last".into())]);
    assert_eq!(r[2], vec![Action::Say("not a command".into())]);
    assert_eq!(r[4], vec![Action::Say("learn mode off".into())]);
    assert_eq!(r[5], vec![Action::RepeatLast]);
}

#[test]
fn pass_through_sends_the_next_key_untouched() {
    let r = run(Layout::Laptop, &[KeyEvent::with_reader("p"), KeyEvent::with_reader("r"), KeyEvent::with_reader("r")]);
    assert!(r[0].is_empty());
    assert_eq!(r[1], vec![Action::Pass("pass\tr\t-\n".into())]);
    assert_eq!(r[2], vec![Action::RepeatLast]);
}

#[test]
fn a_made_up_key_cannot_change_settings() {
    let mut fake = KeyEvent::with_reader("s");
    fake.real = false;
    let r = run(Layout::Laptop, &[fake, KeyEvent::with_reader("s")]);
    assert!(matches!(&r[0][0], Action::Say(_)));
    assert_eq!(r[1], vec![Action::ToggleSpeech]);
}

#[test]
fn toggle_speech_mutes_and_unmutes() {
    use crate::coordinator::{Coordinator, Out};
    use crate::model::{Priority, Source, Utterance};
    let mut c = Coordinator::new();
    c.submit(0, Utterance::new("Hello", Priority::Focus, Source::Focus));
    assert_eq!(apply_basic(&mut c, 10, &Action::ToggleSpeech), vec![Out::Stop]);
    assert!(c.submit(20, Utterance::new("Quiet", Priority::Focus, Source::Focus)).is_empty());
    let on = apply_basic(&mut c, 30, &Action::ToggleSpeech);
    assert!(matches!(&on[0], Out::Say { text, .. } if text == "Speech on"));
    assert!(apply_basic(&mut c, 40, &Action::Say(String::new())).is_empty());
}

#[test]
fn compositor_lines_are_read_and_junk_is_dropped() {
    let k = parse_line("key\tr\tdown\treader\treal\n").unwrap();
    assert!(matches!(&k, Wire::Key(e) if e.key == "r" && e.reader && e.pressed && e.real && !e.shift));
    let k = parse_line("key\tControl_L\tup\tshift,ctrl\tmade").unwrap();
    assert!(matches!(&k, Wire::Key(e) if e.key == "Control" && !e.pressed && !e.real && e.shift && e.ctrl && !e.reader));
    assert_eq!(parse_line("tap\tCaps_Lock\treal"), Some(Wire::Tap { key: "Caps_Lock".into(), real: true }));
    assert_eq!(parse_line("state\tcaps\ton"), Some(Wire::CapsState(true)));
    assert_eq!(parse_line("state\tcaps\toff\n"), Some(Wire::CapsState(false)));
    assert!(parse_line("state\tcaps\tmaybe").is_none());
    for bad in ["", "key", "key\tr\tsideways\t-\treal", "key\t\tdown\t-\treal", "tap\tx", "hello\tworld", "key\tr\tdown\t-\tmaybe"] {
        assert!(parse_line(bad).is_none(), "{bad:?}");
    }
}

#[test]
fn learn_state_line_is_exact() {
    assert_eq!(learn_line(true), "state\tlearn\ton\n");
    assert_eq!(learn_line(false), "state\tlearn\toff\n");
}

#[test]
fn two_real_caps_lock_taps_close_together_give_the_key_back() {
    let mut t = TapCounter::default();
    assert!(!t.tap(0, "Caps_Lock", true));
    assert!(t.tap(300, "Caps_Lock", true));
    assert!(!t.tap(400, "Caps_Lock", true));
    assert!(!t.tap(1500, "Caps_Lock", true));
    assert!(!t.tap(1600, "Insert", true));
    assert!(!t.tap(1700, "Caps_Lock", false));
}

#[test]
fn a_new_compositor_link_closes_whatever_the_old_one_left_open() {
    let mut st = KeyState::default();
    let map = Keymap::new(Layout::Laptop);
    assert!(st.handle(&map, &KeyEvent::with_reader("p")).is_empty());
    st.new_link();
    assert_eq!(st.handle(&map, &KeyEvent::with_reader("r")), vec![Action::RepeatLast], "pass-next did not survive the link");
}

#[test]
fn the_pass_line_carries_the_key_and_its_modifiers_but_never_the_reader_key() {
    let mut e = KeyEvent::with_reader("r");
    assert_eq!(pass_line(&e), "pass\tr\t-\n");
    e.ctrl = true;
    e.shift = true;
    assert_eq!(pass_line(&e), "pass\tr\tshift,ctrl\n");
}
