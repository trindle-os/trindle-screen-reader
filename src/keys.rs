//! The command table and key logic: a key event goes in, actions come out.

use crate::browse::Kind;
use crate::review::Review;
use ReviewCmd as R;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    Desktop,
    Laptop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Layer {
    Review,
    Settings,
    Browse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewCmd {
    ReadLine,
    PrevLine,
    NextLine,
    ReadWord,
    PrevWord,
    NextWord,
    ReadChar,
    PrevChar,
    NextChar,
    FirstLine,
    LastLine,
    TopLeft,
    BottomRight,
    StartOfLine,
    EndOfLine,
    ReadAll,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingCmd {
    Verbosity,
    TypingEcho,
    Sounds,
    CueStep(i8),
    Layout,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowseCmd {
    Read,
    Next,
    Prev,
    Jump(Kind, bool),
    List(Kind),
    ToggleMode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    StopSpeech,
    ToggleSpeech,
    RepeatLast,
    SpellLast,
    GiveBackCapsLock,
    WhereAmI,
    SayAll,
    SkipSentence(i32),
    SkipParagraph(bool),
    RateStep(i8),
    VolumeStep(i8),
    Setting(SettingCmd),
    Review(ReviewCmd),
    Browse(BrowseCmd),
    Say(String),
    Pass(String),
    PassNext,
    Learn,
    OpenLayer(Layer),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Combo {
    pub key: String,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

impl Combo {
    pub fn plain(key: &str) -> Combo {
        Combo { key: key.into(), shift: false, ctrl: false, alt: false }
    }
    pub fn shifted(key: &str) -> Combo {
        Combo { shift: true, ..Combo::plain(key) }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: String,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub reader: bool,
    pub real: bool,
    pub pressed: bool,
}

impl KeyEvent {
    pub fn press(key: &str) -> KeyEvent {
        KeyEvent { key: key.into(), shift: false, ctrl: false, alt: false, reader: false, real: true, pressed: true }
    }
    pub fn with_reader(key: &str) -> KeyEvent {
        KeyEvent { reader: true, ..KeyEvent::press(key) }
    }
    fn combo(&self) -> Combo {
        Combo { key: self.key.clone(), shift: self.shift, ctrl: self.ctrl, alt: self.alt }
    }
}

mod bindings;
pub use bindings::{default_bindings, Binding};

pub struct Keymap {
    bindings: Vec<Binding>,
}

impl Keymap {
    pub fn new(layout: Layout) -> Keymap {
        Keymap { bindings: default_bindings(layout) }
    }

    fn find(&self, layer: Option<Layer>, c: &Combo) -> Option<&Binding> {
        self.bindings.iter().find(|x| x.layer == layer && &x.combo == c)
    }
}

#[derive(Debug, Default)]
pub struct KeyState {
    layer: Option<Layer>,
    learn: bool,
    escapes: u8,
    pass_next: bool,
    ctrl_alone: bool,
    plain_ok: bool,
    sticky: bool,
}

impl KeyState {
    pub fn in_learn_mode(&self) -> bool {
        self.learn
    }

    pub fn in_browse_keys(&self) -> bool {
        self.sticky && self.layer == Some(Layer::Browse)
    }

    pub fn set_plain_keys(&mut self, ok: bool) {
        self.plain_ok = ok;
        self.sticky = false;
        if self.layer == Some(Layer::Browse) {
            self.layer = None;
        }
    }

    pub fn new_link(&mut self) {
        self.layer = None;
        self.learn = false;
        self.escapes = 0;
        self.pass_next = false;
        self.ctrl_alone = false;
        self.sticky = false;
        self.plain_ok = false;
    }

    pub fn leave_browse(&mut self) {
        if self.in_browse_keys() {
            self.layer = None;
            self.sticky = false;
        }
    }

    fn browse_key(&mut self, map: &Keymap, ev: &KeyEvent) -> Vec<Action> {
        if ev.key == "Escape" {
            self.layer = None;
            self.sticky = false;
            return vec![Action::Say(crate::words::BROWSE_OFF.into())];
        }
        if ev.reader {
            self.layer = None;
            self.sticky = false;
            return self.reader_command(map, ev);
        }
        if ev.ctrl || ev.alt {
            return vec![];
        }
        match map.find(Some(Layer::Browse), &ev.combo()) {
            Some(bd) => Self::run(bd, ev),
            None => vec![],
        }
    }

    pub fn handle(&mut self, map: &Keymap, ev: &KeyEvent) -> Vec<Action> {
        if ev.key == "Control" {
            return self.control_key(ev);
        }
        if !ev.pressed {
            return vec![];
        }
        self.ctrl_alone = false;

        if !ev.real && (self.learn || self.layer.is_some() || self.pass_next) {
            return vec![];
        }
        if self.layer.is_some() && matches!(ev.key.as_str(), "Shift" | "Alt" | "Super") {
            return vec![];
        }

        if self.pass_next {
            self.pass_next = false;
            return vec![Action::Pass(pass_line(ev))];
        }
        if self.learn {
            return self.learn_key(map, ev);
        }
        if self.in_browse_keys() {
            return self.browse_key(map, ev);
        }
        if let Some(layer) = self.layer.take() {
            return Self::layer_key(map, layer, ev);
        }
        if !ev.reader {
            return vec![];
        }
        self.reader_command(map, ev)
    }

    fn control_key(&mut self, ev: &KeyEvent) -> Vec<Action> {
        if ev.pressed {
            self.ctrl_alone = ev.real;
        } else if std::mem::take(&mut self.ctrl_alone) && ev.real && !self.learn {
            return vec![Action::StopSpeech];
        }
        vec![]
    }

    fn layer_key(map: &Keymap, layer: Layer, ev: &KeyEvent) -> Vec<Action> {
        if ev.key == "Escape" {
            return vec![];
        }
        match map.find(Some(layer), &ev.combo()) {
            Some(bd) => Self::run(bd, ev),
            None => vec![],
        }
    }

    fn reader_command(&mut self, map: &Keymap, ev: &KeyEvent) -> Vec<Action> {
        if matches!(ev.key.as_str(), "Insert" | "Caps_Lock") {
            return vec![];
        }
        let Some(bd) = map.find(None, &ev.combo()) else { return vec![] };
        if bd.changes_settings && !ev.real {
            return vec![Action::Say(crate::words::REFUSED.into())];
        }
        match bd.action {
            Action::Learn => {
                self.learn = true;
                self.escapes = 0;
                vec![Action::Say(crate::words::LEARN_ON.into())]
            }
            Action::OpenLayer(layer) => {
                self.layer = Some(layer);
                self.sticky = layer == Layer::Browse && self.plain_ok;
                let said = match layer {
                    Layer::Review => crate::words::LAYER_REVIEW,
                    Layer::Settings => crate::words::LAYER_SETTINGS,
                    Layer::Browse => crate::words::LAYER_BROWSE,
                };
                vec![Action::Say(said.into())]
            }
            Action::PassNext => {
                self.pass_next = true;
                vec![]
            }
            _ => Self::run(bd, ev),
        }
    }

    fn run(bd: &Binding, ev: &KeyEvent) -> Vec<Action> {
        if bd.changes_settings && !ev.real {
            return vec![Action::Say(crate::words::REFUSED.into())];
        }
        vec![bd.action.clone()]
    }

    fn learn_key(&mut self, map: &Keymap, ev: &KeyEvent) -> Vec<Action> {
        if ev.key == "Escape" {
            self.escapes += 1;
            if self.escapes >= 2 {
                self.learn = false;
                return vec![Action::Say(crate::words::LEARN_OFF.into())];
            }
            return vec![Action::Say("escape".into())];
        }
        self.escapes = 0;
        let said = map
            .find(None, &ev.combo())
            .filter(|_| ev.reader)
            .map(|bd| bd.name.to_string())
            .unwrap_or_else(|| crate::words::NOT_A_COMMAND.into());
        vec![Action::Say(said)]
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Wire {
    Key(KeyEvent),
    Tap { key: String, real: bool },
    CapsState(bool),
    PlainKeys,
    Linked,
}

pub const CAPS_TOGGLE_LINE: &str = "caps\ttoggle\n";

fn normal_name(n: &str) -> String {
    match n {
        "Control_L" | "Control_R" => "Control",
        "Shift_L" | "Shift_R" => "Shift",
        "Alt_L" | "Alt_R" => "Alt",
        "Super_L" | "Super_R" => "Super",
        other => other,
    }
    .into()
}

pub fn parse_line(line: &str) -> Option<Wire> {
    let f: Vec<&str> = line.trim_end_matches(['\n', '\r']).split('\t').collect();
    let real = |s: &str| s == "real";
    match f.as_slice() {
        ["key", name, dir, mods, who] if matches!(*dir, "down" | "up") && matches!(*who, "real" | "made") && !name.is_empty() => {
            let has = |m: &str| mods.split(',').any(|x| x == m);
            Some(Wire::Key(KeyEvent {
                key: normal_name(name),
                shift: has("shift"),
                ctrl: has("ctrl"),
                alt: has("alt"),
                reader: has("reader"),
                real: real(who),
                pressed: *dir == "down",
            }))
        }
        ["tap", name, who] if matches!(*who, "real" | "made") && !name.is_empty() => Some(Wire::Tap { key: (*name).into(), real: real(who) }),
        ["state", "plainkeys", "available"] => Some(Wire::PlainKeys),
        ["state", "caps", v] if matches!(*v, "on" | "off") => Some(Wire::CapsState(*v == "on")),
        _ => None,
    }
}

pub fn pass_line(ev: &KeyEvent) -> String {
    let mods: Vec<&str> = [(ev.shift, "shift"), (ev.ctrl, "ctrl"), (ev.alt, "alt")].iter().filter(|m| m.0).map(|m| m.1).collect();
    format!("pass\t{}\t{}\n", ev.key, if mods.is_empty() { "-".to_string() } else { mods.join(",") })
}

pub fn browse_line(on: bool) -> String {
    format!("state\tbrowse\t{}\n", if on { "on" } else { "off" })
}

pub fn learn_line(on: bool) -> String {
    format!("state\tlearn\t{}\n", if on { "on" } else { "off" })
}

pub const DOUBLE_TAP_MS: u64 = 500;

#[derive(Default)]
pub struct TapCounter {
    last: Option<u64>,
}

impl TapCounter {
    pub fn tap(&mut self, now: u64, key: &str, real: bool) -> bool {
        if key != "Caps_Lock" || !real {
            self.last = None;
            return false;
        }
        match self.last.take() {
            Some(t) if now.saturating_sub(t) <= DOUBLE_TAP_MS => true,
            _ => {
                self.last = Some(now);
                false
            }
        }
    }
}

pub fn apply_basic(c: &mut crate::coordinator::Coordinator, now: u64, a: &Action) -> Vec<crate::coordinator::Out> {
    use crate::model::{Priority, Source, Utterance};
    let own = |t: &str| Utterance::new(t, Priority::Urgent, Source::Own);
    match a {
        Action::StopSpeech => c.stop(),
        Action::RepeatLast => c.repeat_last(false),
        Action::SpellLast => c.repeat_last(true),
        Action::SkipSentence(n) => c.say_all_skip(Some(*n), None),
        Action::SkipParagraph(f) => c.say_all_skip(None, Some(*f)),
        Action::Say(t) if !t.is_empty() => c.submit(now, own(t)),
        Action::ToggleSpeech if c.muted => {
            c.muted = false;
            c.submit(now, own(crate::words::SPEECH_ON))
        }
        Action::ToggleSpeech => {
            c.muted = true;
            c.stop()
        }
        _ => vec![],
    }
}

pub fn run_review(r: &mut Review, c: ReviewCmd) -> Vec<String> {
    match c {
        R::ReadLine => vec![r.read_line()],
        R::PrevLine => vec![r.prev_line()],
        R::NextLine => vec![r.next_line()],
        R::ReadWord => vec![r.read_word()],
        R::PrevWord => vec![r.prev_word()],
        R::NextWord => vec![r.next_word()],
        R::ReadChar => vec![r.read_char()],
        R::PrevChar => vec![r.prev_char()],
        R::NextChar => vec![r.next_char()],
        R::FirstLine => vec![r.first_line()],
        R::LastLine => vec![r.last_line()],
        R::TopLeft => vec![r.top_left()],
        R::BottomRight => vec![r.bottom_right()],
        R::StartOfLine => vec![r.start_of_line()],
        R::EndOfLine => vec![r.end_of_line()],
        R::ReadAll => r.read_from_cursor(),
    }
}

#[cfg(test)]
mod tests;
