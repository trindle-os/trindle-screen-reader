//! The one owner of output: everything that wants to be heard goes through `submit`, and time is passed in.

use crate::model::{Priority, Utterance};
use crate::sayall::{self, SayAll};
use std::collections::VecDeque;

pub const DUPLICATE_WINDOW_MS: u64 = 1000;
pub const TYPING_PAUSE_MS: u64 = 700;
pub const BACKGROUND_STALE_MS: u64 = 5000;
pub const QUEUE_LIMIT_MS: u64 = 10_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Out {
    Say { id: u64, text: String, priority: Priority },
    Stop,
}

#[derive(Debug)]
struct Item {
    u: Utterance,
    at: u64,
}

pub fn estimate_ms(text: &str) -> u64 {
    (text.chars().count() as u64 * 1000 / 15).max(300)
}

pub struct Coordinator {
    queue: VecDeque<Item>,
    speaking: Option<(u64, Priority)>,
    deadline: u64,
    clock: u64,
    recent: Vec<(String, crate::model::Source, u64)>,
    last_key: Option<u64>,
    next_id: u64,
    pub muted: bool,
    history: VecDeque<String>,
    say_all: Option<(SayAll, u64)>,
    say_all_cut: bool,
}

impl Default for Coordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl Coordinator {
    pub fn new() -> Self {
        Coordinator {
            queue: VecDeque::new(),
            speaking: None,
            deadline: 0,
            clock: 0,
            recent: Vec::new(),
            last_key: None,
            next_id: 1,
            muted: false,
            history: VecDeque::new(),
            say_all: None,
            say_all_cut: false,
        }
    }

    pub fn repeat_last(&mut self, spell: bool) -> Vec<Out> {
        let Some(last) = self.history.back().cloned().filter(|_| !self.muted) else { return vec![] };
        let text = if spell { crate::caret::spell(&last) } else { last };
        let mut out = Vec::new();
        if self.speaking.take().is_some() {
            out.push(Out::Stop);
        }
        self.queue.clear();
        let id = self.next_id;
        self.next_id += 1;
        self.begin(id, Priority::Focus, &text);
        out.push(Out::Say { id, text, priority: Priority::Focus });
        out
    }

    pub fn say_all_start(&mut self, text: &str, from: usize) -> Vec<Out> {
        let mut out = self.stop();
        if self.muted {
            return out;
        }
        self.say_all = SayAll::start(text, from).map(|r| (r, 0));
        self.say_all_cut = false;
        self.say_all_speak(Priority::Focus, &mut out);
        out
    }

    pub fn say_all_is_cut(&mut self) {
        self.say_all_cut = true;
    }

    pub fn say_all_skip(&mut self, sentences: Option<i32>, paragraph_forward: Option<bool>) -> Vec<Out> {
        let Some((r, _)) = self.say_all.as_mut() else { return vec![] };
        let moved = match (sentences, paragraph_forward) {
            (Some(n), _) => r.skip_sentence(n).is_some(),
            (None, Some(f)) => r.skip_paragraph(f).is_some(),
            _ => true,
        };
        let mut out = Vec::new();
        if self.speaking.take().is_some() {
            out.push(Out::Stop);
        }
        if moved {
            self.say_all_speak(Priority::Focus, &mut out);
        } else {
            self.say_all_end(&mut out);
        }
        out
    }

    fn say_all_speak(&mut self, priority: Priority, out: &mut Vec<Out>) {
        let Some((r, sid)) = self.say_all.as_mut() else { return };
        let Some(sent) = r.current() else { return };
        let (id, text) = (self.next_id, sayall::with_marker(sent.start, &sent.text));
        self.next_id += 1;
        *sid = id;
        self.begin(id, priority, &text);
        out.push(Out::Say { id, text, priority });
    }

    fn say_all_end(&mut self, out: &mut Vec<Out>) {
        self.say_all = None;
        let id = self.next_id;
        self.next_id += 1;
        let text = if std::mem::take(&mut self.say_all_cut) { crate::words::DOCUMENT_CUT } else { crate::words::END_OF_DOCUMENT }.to_string();
        self.begin(id, Priority::Normal, &text);
        out.push(Out::Say { id, text, priority: Priority::Normal });
    }

    fn typing(&self, now: u64) -> bool {
        self.last_key.is_some_and(|k| now.saturating_sub(k) < TYPING_PAUSE_MS)
    }

    fn begin(&mut self, id: u64, priority: Priority, text: &str) {
        self.speaking = Some((id, priority));
        self.deadline = self.clock + estimate_ms(text) * 3 + 1000;
    }

    pub fn voice_lost(&mut self) -> Vec<Out> {
        match self.speaking {
            Some((id, _)) => self.speech_done(self.clock, id),
            None => vec![],
        }
    }

    fn start(&mut self, u: Utterance, out: &mut Vec<Out>) {
        let id = self.next_id;
        self.next_id += 1;
        self.begin(id, u.priority, &u.text);
        self.history.push_back(u.text.clone());
        while self.history.len() > 50 {
            self.history.pop_front();
        }
        out.push(Out::Say { id, text: u.text, priority: u.priority });
    }

    pub fn submit(&mut self, now: u64, u: Utterance) -> Vec<Out> {
        self.clock = now;
        let mut out = Vec::new();
        if u.text.trim().is_empty() || (self.muted && u.text != crate::words::SPEECH_ON) {
            return out;
        }
        self.recent.retain(|(_, _, t)| now.saturating_sub(*t) < DUPLICATE_WINDOW_MS);
        let repeats_ok = matches!(u.source, crate::model::Source::Typing | crate::model::Source::Caret | crate::model::Source::Focus);
        if !repeats_ok && self.recent.iter().any(|(t, s, _)| *t == u.text && *s == u.source) {
            return out;
        }
        self.recent.push((u.text.clone(), u.source, now));

        match u.priority {
            Priority::Urgent | Priority::Focus => {
                self.say_all = None;
                self.queue.retain(|i| i.u.priority >= u.priority);
                if self.speaking.is_some() {
                    out.push(Out::Stop);
                    self.speaking = None;
                }
                self.start(u, &mut out);
            }
            Priority::Normal | Priority::Background => {
                self.queue.push_back(Item { u, at: now });
                self.trim();
                self.pump(now, &mut out);
            }
        }
        out
    }

    pub fn speech_done(&mut self, now: u64, id: u64) -> Vec<Out> {
        self.clock = now;
        let mut out = Vec::new();
        if self.speaking.map(|(i, _)| i) == Some(id) {
            self.speaking = None;
            if self.say_all.as_ref().is_some_and(|(_, sid)| *sid == id) {
                let more = self.say_all.as_mut().is_some_and(|(r, _)| r.advance().is_some());
                if more {
                    self.say_all_speak(Priority::Normal, &mut out);
                } else {
                    self.say_all_end(&mut out);
                }
                return out;
            }
        }
        self.pump(now, &mut out);
        out
    }

    pub fn tick(&mut self, now: u64) -> Vec<Out> {
        self.clock = now;
        if let Some((id, _)) = self.speaking.filter(|_| now >= self.deadline) {
            return self.speech_done(now, id);
        }
        let mut out = Vec::new();
        self.pump(now, &mut out);
        out
    }

    pub fn next_wake(&self, now: u64) -> Option<u64> {
        if self.speaking.is_some() {
            return Some(self.deadline);
        }
        if self.queue.is_empty() {
            return None;
        }
        self.last_key.filter(|_| self.typing(now)).map(|k| k + TYPING_PAUSE_MS)
    }

    pub fn key(&mut self, now: u64, modifier_only: bool, stops_speech: bool) -> Vec<Out> {
        self.clock = now;
        let mut out = Vec::new();
        if modifier_only {
            return out;
        }
        self.last_key = Some(now);
        if stops_speech && self.say_all.take().is_some() && self.speaking.is_none() {
            return out;
        }
        if stops_speech && self.speaking.is_some() {
            self.speaking = None;
            self.queue.clear();
            out.push(Out::Stop);
        }
        out
    }

    pub fn stop(&mut self) -> Vec<Out> {
        self.say_all = None;
        self.queue.clear();
        if self.speaking.take().is_some() {
            vec![Out::Stop]
        } else {
            vec![]
        }
    }

    fn trim(&mut self) {
        loop {
            let total: u64 = self.queue.iter().map(|i| estimate_ms(&i.u.text)).sum();
            if total <= QUEUE_LIMIT_MS {
                break;
            }
            let pos = self.queue.iter().position(|i| i.u.priority == Priority::Normal).unwrap_or(0);
            if self.queue.remove(pos).is_none() {
                break;
            }
        }
    }

    fn pump(&mut self, now: u64, out: &mut Vec<Out>) {
        self.queue.retain(|i| !(i.u.priority == Priority::Background && now.saturating_sub(i.at) > BACKGROUND_STALE_MS));
        if self.speaking.is_some() || self.typing(now) {
            return;
        }
        let best = self
            .queue
            .iter()
            .enumerate()
            .max_by(|(ia, a), (ib, b)| a.u.priority.cmp(&b.u.priority).then(ib.cmp(ia)))
            .map(|(i, _)| i);
        if let Some(item) = best.and_then(|i| self.queue.remove(i)) {
            self.start(item.u, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Source;

    fn say(text: &str, p: Priority) -> Utterance {
        Utterance::new(text, p, Source::Focus)
    }

    #[test]
    fn it_sleeps_when_idle_and_wakes_for_the_next_deadline_only() {
        let mut c = Coordinator::new();
        assert_eq!(c.next_wake(0), None);
        c.submit(0, say("Save, button", Priority::Focus));
        let deadline = c.next_wake(0).expect("speaking has a time-out");
        assert!(deadline > 0);
        c.speech_done(10, 1);
        assert_eq!(c.next_wake(10), None);
        c.key(1000, false, false);
        c.submit(1100, say("later", Priority::Normal));
        assert_eq!(c.next_wake(1100), Some(1000 + TYPING_PAUSE_MS));
        assert_eq!(c.next_wake(1000 + TYPING_PAUSE_MS), None);
    }

    #[test]
    fn history_keeps_fifty_newest_last() {
        let mut c = Coordinator::new();
        for i in 0..80 {
            c.submit(i * 2000, say(&format!("m{i}"), Priority::Focus));
        }
        assert_eq!(c.history.len(), 50);
        assert_eq!(c.history.back().map(String::as_str), Some("m79"));
        assert_eq!(c.history.front().map(String::as_str), Some("m30"));
    }

    #[test]
    fn repeat_is_not_remembered_as_a_new_item() {
        let mut c = Coordinator::new();
        c.submit(0, say("Save, button", Priority::Focus));
        c.repeat_last(false);
        c.repeat_last(true);
        assert_eq!(c.history, ["Save, button"]);
    }

    #[test]
    fn say_all_position_follows_the_sentence_and_clears_at_the_end() {
        let mut c = Coordinator::new();
        c.say_all_start("One. Two.", 0);
        assert_eq!(c.say_all.as_ref().map(|(r, _)| r.position()), Some(0));
        c.speech_done(100, 1);
        assert_eq!(c.say_all.as_ref().map(|(r, _)| r.position()), Some(5));
        c.speech_done(200, 2);
        assert!(c.say_all.is_none());
    }

    #[test]
    fn a_cut_document_does_not_say_end_of_document() {
        let said = |cut: bool| {
            let mut c = Coordinator::new();
            c.say_all_start("One.", 0);
            if cut {
                c.say_all_is_cut();
            }
            c.speech_done(100, 1)
                .iter()
                .find_map(|o| if let Out::Say { text, .. } = o { Some(text.clone()) } else { None })
                .unwrap()
        };
        assert_eq!(said(false), crate::words::END_OF_DOCUMENT);
        assert_eq!(said(true), crate::words::DOCUMENT_CUT);
    }

    #[test]
    fn a_key_press_or_an_urgent_message_ends_say_all() {
        let mut c = Coordinator::new();
        c.say_all_start("One. Two.", 0);
        c.key(50, false, true);
        assert!(c.say_all.is_none());
        c.say_all_start("One. Two.", 0);
        c.submit(10, Utterance::new("Alert", Priority::Urgent, Source::Live));
        assert!(c.say_all.is_none());
    }
}
