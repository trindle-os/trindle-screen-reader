//! Turns events and object facts into words. Pure: no bus, no clock, no voice.

use crate::model::*;
use crate::caret;
use crate::words;

pub const MAX_PIECE: usize = 4096;

#[derive(Default)]
pub struct Describer {
    pub verbosity: Verbosity,
    pub typing_echo: crate::settings::TypingEcho,
    last_container: Option<String>,
}

fn fill(template: &str, pairs: &[(&str, &str)]) -> String {
    let mut s = template.to_string();
    for (k, v) in pairs {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

fn clip(mut s: String) -> String {
    if s.len() > MAX_PIECE {
        let mut cut = MAX_PIECE;
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        s.truncate(cut);
    }
    s
}

fn clean(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl Describer {
    pub fn new(verbosity: Verbosity) -> Self {
        Describer { verbosity, typing_echo: Default::default(), last_container: None }
    }

    pub fn reset_context(&mut self) {
        self.last_container = None;
    }

    pub fn focus(&mut self, o: &ObjectInfo) -> String {
        let mut parts: Vec<String> = Vec::new();
        parts.extend(self.container_words(o));

        let role = o.role();
        let role_word = words::role(role);
        let name = clean(&o.name);
        if name.is_empty() && !role_word.is_empty() {
            parts.push(words::UNLABELLED.to_string());
        } else if !name.is_empty() {
            parts.push(name);
        }
        if !role_word.is_empty() {
            parts.push(role_word.to_string());
        }
        parts.extend(state_words(o, role));
        if !o.is_secret() {
            parts.extend(value_words(o, role));
        }
        parts.extend(self.extra_words(o));
        clip(parts.join(", "))
    }

    fn container_words(&mut self, o: &ObjectInfo) -> Option<String> {
        let cid = o.container.as_ref().map(|c| c.id.clone());
        if cid == self.last_container {
            return None;
        }
        self.last_container = cid;
        let c = o.container.as_ref()?;
        Some(name_in_role(clean(&c.name), words::role(c.role))).filter(|s| !s.is_empty())
    }

    fn extra_words(&self, o: &ObjectInfo) -> Vec<String> {
        let mut parts = Vec::new();
        if self.verbosity != Verbosity::Verbose {
            return parts;
        }
        if let Some((n, total)) = o.position {
            if total > 1 && (1..=total).contains(&n) {
                parts.push(fill(words::POSITION, &[("n", &n.to_string()), ("total", &total.to_string())]));
            }
        }
        let d = clean(&o.description);
        if !o.is_secret() && !d.is_empty() {
            parts.push(d);
        }
        parts
    }

    pub fn where_am_i(&mut self, o: &ObjectInfo) -> String {
        let saved = self.last_container.take();
        let t = self.focus(o);
        self.last_container = saved;
        let w = clean(&o.window);
        clip(if w.is_empty() { t } else { fill(words::WHERE_WINDOW, &[("window", &clip(w)), ("rest", &t)]) })
    }

    pub fn event(&mut self, e: &Event) -> Option<Utterance> {
        match e {
            Event::Focus { object } if matches!(object.role, Some(Role::Window | Role::Dialog)) => {
                self.event(&Event::WindowActivated { object: object.clone() })
            }
            Event::Focus { object } => {
                let t = self.focus(object);
                (!t.is_empty()).then(|| Utterance::new(t, Priority::Focus, Source::Focus))
            }
            Event::WindowActivated { object } => {
                self.reset_context();
                let t = name_in_role(clean(&object.name), words::role(object.role.unwrap_or(Role::Window)));
                spoken(t, Priority::Focus, Source::Window)
            }
            Event::WindowClosed => None,
            Event::StateChanged { object, state, on } => {
                if object.is_secret() {
                    return None;
                }
                let w = if *on { words::state_on(*state) } else { words::state_off(*state) }?;
                Some(Utterance::new(w, Priority::Focus, Source::State))
            }
            Event::Live { text, urgent } => spoken(clean(text), urgency(*urgent), Source::Live),
            Event::Notification { app, text, urgent } => {
                let t = fill(words::NOTIFICATION, &[("app", &clean(app)), ("text", &clean(text))]);
                Some(Utterance::new(clip(t), urgency(*urgent), Source::Notification))
            }
            _ => self.object_event(e),
        }
    }

    fn object_event(&self, e: &Event) -> Option<Utterance> {
        match e {
            Event::ValueChanged { object } if !object.is_secret() => {
                spoken(clean(object.value.as_deref().unwrap_or(&object.name)), Priority::Focus, Source::Value)
            }
            Event::Alert { object, text } if !object.is_secret() => {
                spoken(clean(if text.is_empty() { &object.name } else { text }), Priority::Urgent, Source::Live)
            }
            Event::CaretMoved { object, text, old, new } if !object.is_secret() => {
                spoken(caret::caret_moved(text, *old, *new), Priority::Focus, Source::Caret)
            }
            Event::TextInserted { object, before, inserted } if !object.is_secret() => {
                spoken(caret::typed(before, inserted, self.typing_echo), Priority::Focus, Source::Typing)
            }
            Event::TextRemoved { object, removed } if !object.is_secret() && self.typing_echo != crate::settings::TypingEcho::None => {
                Some(Utterance::new(clip(caret::deleted(removed)), Priority::Focus, Source::Typing))
            }
            _ => None,
        }
    }
}

fn name_in_role(name: String, role: &str) -> String {
    match (name.is_empty(), role.is_empty()) {
        (false, false) => fill(words::IN_CONTAINER, &[("name", &name), ("role", role)]),
        (false, true) => name,
        (true, false) => role.to_string(),
        (true, true) => String::new(),
    }
}

fn state_words(o: &ObjectInfo, role: Role) -> Vec<String> {
    let mut parts = Vec::new();
    if matches!(role, Role::CheckBox | Role::RadioButton | Role::Switch) && !o.has(State::Checked) {
        parts.extend(words::state_off(State::Checked).map(String::from));
    }
    parts.extend(o.states.iter().filter_map(|s| words::state_on(*s)).map(str::to_string));
    parts
}

fn value_words(o: &ObjectInfo, role: Role) -> Option<String> {
    let v = clean(o.value.as_ref()?);
    if !v.is_empty() {
        Some(v)
    } else if matches!(role, Role::TextEntry | Role::ComboBox) {
        Some(words::EMPTY.to_string())
    } else {
        None
    }
}

fn urgency(urgent: bool) -> Priority {
    if urgent {
        Priority::Urgent
    } else {
        Priority::Background
    }
}

fn spoken(text: String, priority: Priority, source: Source) -> Option<Utterance> {
    (!text.is_empty()).then(|| Utterance::new(clip(text), priority, source))
}
