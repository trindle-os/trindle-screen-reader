//! The default key bindings, by layer.

use super::*;

#[derive(Clone, Debug)]
pub struct Binding {
    pub id: &'static str,
    pub name: &'static str,
    pub layer: Option<Layer>,
    pub combo: Combo,
    pub action: Action,
    pub changes_settings: bool,
}

fn b(id: &'static str, name: &'static str, layer: Option<Layer>, combo: Combo, action: Action) -> Binding {
    let changes_settings = matches!(
        action,
        Action::ToggleSpeech | Action::RateStep(_) | Action::VolumeStep(_) | Action::Setting(_) | Action::Learn | Action::OpenLayer(_) | Action::PassNext
    );
    Binding { id, name, layer, combo, action, changes_settings }
}

use Action as A;
use ReviewCmd as R;
use SettingCmd as S;

fn review_layer_bindings() -> Vec<Binding> {
    let l = Some(Layer::Review);
    vec![
        b("rv.prev-line", "previous line", l, Combo::plain("Up"), A::Review(R::PrevLine)),
        b("rv.next-line", "next line", l, Combo::plain("Down"), A::Review(R::NextLine)),
        b("rv.prev-word", "previous word", l, Combo::plain("Left"), A::Review(R::PrevWord)),
        b("rv.next-word", "next word", l, Combo::plain("Right"), A::Review(R::NextWord)),
        b("rv.prev-char", "previous character", l, Combo::plain("comma"), A::Review(R::PrevChar)),
        b("rv.next-char", "next character", l, Combo::plain("period"), A::Review(R::NextChar)),
        b("rv.read-line", "read line", l, Combo::plain("l"), A::Review(R::ReadLine)),
        b("rv.read-word", "read word", l, Combo::plain("w"), A::Review(R::ReadWord)),
        b("rv.read-char", "read character", l, Combo::plain("c"), A::Review(R::ReadChar)),
        b("rv.first-line", "first line", l, Combo::plain("Home"), A::Review(R::FirstLine)),
        b("rv.last-line", "last line", l, Combo::plain("End"), A::Review(R::LastLine)),
        b("rv.top-left", "top left", l, Combo::plain("t"), A::Review(R::TopLeft)),
        b("rv.bottom-right", "bottom right", l, Combo::plain("b"), A::Review(R::BottomRight)),
        b("rv.start-of-line", "start of line", l, Combo::plain("s"), A::Review(R::StartOfLine)),
        b("rv.end-of-line", "end of line", l, Combo::plain("e"), A::Review(R::EndOfLine)),
        b("rv.read-all", "read all from here", l, Combo::plain("a"), A::Review(R::ReadAll)),
    ]
}

fn browse_layer_bindings() -> Vec<Binding> {
    let l = Some(Layer::Browse);
    let mut v = vec![
        b("br.next", "next", l, Combo::plain("Down"), A::Browse(BrowseCmd::Next)),
        b("br.prev", "previous", l, Combo::plain("Up"), A::Browse(BrowseCmd::Prev)),
        b("br.read", "read here", l, Combo::plain("r"), A::Browse(BrowseCmd::Read)),
        b("br.mode", "switch between browse and focus mode", l, Combo::plain("m"), A::Browse(BrowseCmd::ToggleMode)),
        b("br.list-headings", "list the headings", l, Combo::plain("1"), A::Browse(BrowseCmd::List(Kind::Heading))),
        b("br.list-links", "list the links", l, Combo::plain("2"), A::Browse(BrowseCmd::List(Kind::Link))),
        b("br.list-fields", "list the form fields", l, Combo::plain("3"), A::Browse(BrowseCmd::List(Kind::Field))),
    ];
    for (id, back, name, key, kind) in [
        ("br.heading", "br.heading-back", "heading", "h", Kind::Heading),
        ("br.link", "br.link-back", "link", "k", Kind::Link),
        ("br.list", "br.list-back", "list", "l", Kind::List),
        ("br.table", "br.table-back", "table", "t", Kind::Table),
        ("br.field", "br.field-back", "form field", "f", Kind::Field),
        ("br.button", "br.button-back", "button", "b", Kind::Button),
        ("br.graphic", "br.graphic-back", "graphic", "g", Kind::Graphic),
        ("br.landmark", "br.landmark-back", "landmark", "a", Kind::Landmark),
    ] {
        v.push(b(id, name, l, Combo::plain(key), A::Browse(BrowseCmd::Jump(kind, true))));
        v.push(b(back, name, l, Combo::shifted(key), A::Browse(BrowseCmd::Jump(kind, false))));
    }
    v
}

fn settings_layer_bindings() -> Vec<Binding> {
    let l = Some(Layer::Settings);
    vec![
        b("st.rate-up", "faster", l, Combo::plain("Up"), A::RateStep(5)),
        b("st.rate-down", "slower", l, Combo::plain("Down"), A::RateStep(-5)),
        b("st.volume-up", "louder", l, Combo::plain("Right"), A::VolumeStep(5)),
        b("st.volume-down", "quieter", l, Combo::plain("Left"), A::VolumeStep(-5)),
        b("st.verbosity", "brief or verbose", l, Combo::plain("v"), A::Setting(S::Verbosity)),
        b("st.typing-echo", "typing echo", l, Combo::plain("t"), A::Setting(S::TypingEcho)),
        b("st.sounds", "sounds on or off", l, Combo::plain("s"), A::Setting(S::Sounds)),
        b("st.cue-up", "louder sounds", l, Combo::plain("Page_Up"), A::Setting(S::CueStep(5))),
        b("st.cue-down", "quieter sounds", l, Combo::plain("Page_Down"), A::Setting(S::CueStep(-5))),
        b("st.layout", "laptop keys for review", l, Combo::plain("l"), A::Setting(S::Layout)),
    ]
}

fn numpad_review_bindings() -> Vec<Binding> {
    let n = |k: &str| Combo::plain(k);
    vec![
        b("np.prev-line", "previous line", None, n("KP_7"), A::Review(R::PrevLine)),
        b("np.read-line", "read line", None, n("KP_4"), A::Review(R::ReadLine)),
        b("np.next-line", "next line", None, n("KP_1"), A::Review(R::NextLine)),
        b("np.prev-word", "previous word", None, n("KP_8"), A::Review(R::PrevWord)),
        b("np.read-word", "read word", None, n("KP_5"), A::Review(R::ReadWord)),
        b("np.next-word", "next word", None, n("KP_2"), A::Review(R::NextWord)),
        b("np.prev-char", "previous character", None, n("KP_9"), A::Review(R::PrevChar)),
        b("np.read-char", "read character", None, n("KP_6"), A::Review(R::ReadChar)),
        b("np.next-char", "next character", None, n("KP_3"), A::Review(R::NextChar)),
        b("np.say-all", "say all", None, n("KP_Add"), A::SayAll),
    ]
}

pub fn default_bindings(layout: Layout) -> Vec<Binding> {
    let mut v = vec![
        b("toggle-speech", "toggle speech", None, Combo::plain("s"), A::ToggleSpeech),
        b("repeat", "repeat last", None, Combo::plain("r"), A::RepeatLast),
        b("spell-last", "spell last", None, Combo::shifted("r"), A::SpellLast),
        b("where-am-i", "where am I", None, Combo::plain("w"), A::WhereAmI),
        b("skip-sentence-next", "next sentence", None, Combo::plain("Right"), A::SkipSentence(1)),
        b("skip-sentence-prev", "previous sentence", None, Combo::plain("Left"), A::SkipSentence(-1)),
        b("skip-paragraph-next", "next paragraph", None, Combo::plain("Down"), A::SkipParagraph(true)),
        b("skip-paragraph-prev", "previous paragraph", None, Combo::plain("Up"), A::SkipParagraph(false)),
        b("pass-next", "send the next key to the program", None, Combo::plain("p"), A::PassNext),
        b("learn", "learn mode", None, Combo::plain("h"), A::Learn),
        b("layer-review", "review layer", None, Combo::plain("v"), A::OpenLayer(Layer::Review)),
        b("layer-settings", "settings layer", None, Combo::plain("g"), A::OpenLayer(Layer::Settings)),
        b("layer-browse", "browse layer", None, Combo::plain("b"), A::OpenLayer(Layer::Browse)),
    ];
    if layout == Layout::Desktop {
        v.extend(numpad_review_bindings());
    } else {
        v.push(b("say-all", "say all", None, Combo::plain("a"), A::SayAll));
    }
    v.extend(review_layer_bindings());
    v.extend(settings_layer_bindings());
    v.extend(browse_layer_bindings());
    v
}
