//! Hostile and garbage input: nothing may panic, exceed its cap, or let a secret reach the voice.

use reader::caret;
use reader::coordinator::{Coordinator, Out};
use reader::describe::{Describer, MAX_PIECE};
use reader::model::*;
use reader::review::{Review, ReviewItem};
use reader::sayall;
use reader::settings::{Settings, TypingEcho};
use reader::voice;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    fn text(&mut self, max: u64) -> String {
        const BITS: &[&str] = &[
            "a", "Z", " ", "\n", "\r", "\t", ".", "!", "?", "\u{1}", "\u{0}", "é", "\u{1F600}", "e\u{301}", "\u{200d}",
            "\u{202e}", "{name}", "{role}", "=", "%", "\"", "\\", "日本語", "STOP", "\n\n",
        ];
        (0..self.below(max)).map(|_| BITS[self.below(BITS.len() as u64) as usize]).collect()
    }
}

fn big(r: &mut Rng) -> String {
    if r.below(8) == 0 { "x".repeat(20_000 + r.below(5000) as usize) } else { r.text(40) }
}

fn object(r: &mut Rng) -> ObjectInfo {
    let roles = [Role::Button, Role::TextEntry, Role::PasswordEntry, Role::Label, Role::List, Role::Unknown];
    ObjectInfo {
        role: if r.below(5) == 0 { None } else { Some(roles[r.below(roles.len() as u64) as usize]) },
        name: big(r),
        description: big(r),
        value: if r.below(2) == 0 { Some(big(r)) } else { None },
        states: if r.below(3) == 0 { vec![State::Secret, State::Focused] } else { vec![] },
        position: if r.below(3) == 0 { Some((r.below(9) as u32, r.below(9) as u32)) } else { None },
        container: None,
        window: big(r),
    }
}

#[test]
fn text_functions_survive_garbage_and_wild_offsets() {
    let mut r = Rng(0x9E3779B97F4A7C15);
    for _ in 0..3000 {
        let t = r.text(60);
        let (a, b) = (r.below(200) as usize, r.below(200) as usize);
        let _ = caret::char_at(&t, a);
        let _ = caret::word_at(&t, a);
        let _ = caret::line_at(&t, a);
        let _ = caret::caret_moved(&t, a, b);
        let _ = caret::last_word(&t);
        let _ = caret::spell(&t);
        let ins = r.text(4);
        for echo in [TypingEcho::None, TypingEcho::Character, TypingEcho::Word, TypingEcho::Both] {
            let _ = caret::typed(&t, &ins, echo);
        }
        let sentences = sayall::split(&t, a);
        for s in &sentences {
            assert!(!s.text.is_empty());
        }
        if let Some(mut s) = sayall::SayAll::start(&t, a) {
            for _ in 0..5 {
                let _ = s.skip_sentence(r.below(9) as i32 - 4);
                let _ = s.skip_paragraph(r.below(2) == 0);
                let _ = s.advance();
            }
        }
    }
}

#[test]
fn describer_never_exceeds_the_cap_and_never_reads_secrets() {
    let mut r = Rng(42);
    let mut d = Describer::default();
    for _ in 0..2000 {
        let o = object(&mut r);
        let events = vec![
            Event::Focus { object: o.clone() },
            Event::ValueChanged { object: o.clone() },
            Event::Alert { object: o.clone(), text: big(&mut r) },
            Event::Live { text: big(&mut r), urgent: r.below(2) == 0 },
            Event::Notification { app: big(&mut r), text: big(&mut r), urgent: false },
            Event::TextInserted { object: o.clone(), before: big(&mut r), inserted: big(&mut r) },
            Event::TextRemoved { object: o.clone(), removed: big(&mut r) },
            Event::CaretMoved { object: o.clone(), text: big(&mut r), old: r.below(300) as usize, new: r.below(300) as usize },
        ];
        assert!(d.focus(&o).len() <= MAX_PIECE + 4);
        assert!(d.where_am_i(&o).len() <= MAX_PIECE + 4);
        for e in events {
            let secret_typing = matches!(&e, Event::TextInserted { object, .. } | Event::TextRemoved { object, .. } | Event::CaretMoved { object, .. } | Event::ValueChanged { object } if object.is_secret());
            if let Some(u) = d.event(&e) {
                assert!(u.text.len() <= MAX_PIECE + 4, "piece of {} bytes", u.text.len());
                if secret_typing {
                    let (typed, secret_text) = match &e {
                        Event::TextInserted { inserted, .. } => (inserted.clone(), true),
                        Event::TextRemoved { removed, .. } => (removed.clone(), true),
                        Event::ValueChanged { object } => (object.value.clone().unwrap_or_default(), true),
                        _ => (String::new(), false),
                    };
                    if secret_text && typed.chars().count() > 3 {
                        assert!(!u.text.contains(&typed), "secret spoken: {:?}", u.text);
                    }
                }
            }
        }
    }
}

#[test]
fn wire_messages_cannot_carry_a_second_command() {
    let mut r = Rng(7);
    for _ in 0..2000 {
        let text = r.text(30);
        let m = voice::encode(&Out::Say { id: r.next(), text, priority: Priority::Normal });
        assert!(m.ends_with("\n\n"));
        assert_eq!(m.matches("\n\n").count(), 1, "blank line inside {m:?}");
        assert_eq!(m.lines().filter(|l| *l == "STOP").count(), 0);
        let _ = voice::decode(&r.text(40));
    }
}

#[test]
fn settings_survive_garbage_and_never_lock_the_person_out() {
    let mut r = Rng(99);
    for _ in 0..2000 {
        let (s, _) = Settings::parse(&r.text(80));
        assert!(s.rate >= 10 && s.rate <= 100 && s.volume <= 100);
    }
    let (s, _) = Settings::parse("rate = -5\nvolume = 99999\nverbosity = 7\n");
    assert!(s.rate >= 10 && s.volume <= 100);
}

#[test]
fn coordinator_survives_any_order_of_calls() {
    let mut r = Rng(1234);
    let mut c = Coordinator::new();
    let mut now = 0u64;
    let mut ids: Vec<u64> = Vec::new();
    for _ in 0..5000 {
        now += r.below(400);
        let outs = match r.below(9) {
            0 => c.submit(now, Utterance::new(r.text(20), Priority::Focus, Source::Focus)),
            1 => c.submit(now, Utterance::new(r.text(20), Priority::Normal, Source::Value)),
            2 => c.submit(now, Utterance::new(r.text(20), Priority::Background, Source::Live)),
            3 => c.submit(now, Utterance::new(r.text(20), Priority::Urgent, Source::Notification)),
            4 => c.speech_done(now, ids.get(r.below(ids.len() as u64 + 1) as usize).copied().unwrap_or(0)),
            5 => c.key(now, r.below(2) == 0, r.below(2) == 0),
            6 => c.say_all_start(&r.text(60), r.below(30) as usize),
            7 => c.say_all_skip(Some(r.below(5) as i32 - 2), Some(r.below(2) == 0)),
            _ => c.tick(now),
        };
        for o in outs {
            if let Out::Say { id, text, .. } = o {
                assert!(!text.is_empty());
                ids.push(id);
            }
        }
        let _ = c.repeat_last(r.below(2) == 0);
    }
}

#[test]
fn review_survives_overlapping_and_absurd_rectangles() {
    let mut r = Rng(555);
    for _ in 0..500 {
        let items: Vec<ReviewItem> = (0..r.below(30))
            .map(|i| ReviewItem {
                id: i,
                role: [Role::Label, Role::Button, Role::PasswordEntry, Role::Unknown][r.below(4) as usize],
                name: r.text(10),
                text: r.text(20),
                secret: r.below(4) == 0,
                x: r.next() as i32,
                y: r.next() as i32,
                w: r.next() as i32,
                h: r.next() as i32,
            })
            .collect();
        let mut v = Review::build(items);
        for _ in 0..40 {
            let _ = match r.below(11) {
                0 => v.next_line(),
                1 => v.prev_line(),
                2 => v.next_word(),
                3 => v.prev_word(),
                4 => v.next_char(),
                5 => v.prev_char(),
                6 => v.first_line(),
                7 => v.last_line(),
                8 => v.top_left(),
                9 => v.bottom_right(),
                _ => v.enter(r.below(30)).unwrap_or_default(),
            };
            let _ = v.read_from_cursor();
        }
    }
}
