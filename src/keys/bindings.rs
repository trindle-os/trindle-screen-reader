//! The default key bindings, by layer.

use super::*;

#[derive(Clone, Debug)]
pub struct Binding {
    pub name: &'static str,
    pub layer: Option<Layer>,
    pub combo: Combo,
    pub action: Action,
    pub changes_settings: bool,
}

fn b(name: &'static str, layer: Option<Layer>, combo: Combo, action: Action) -> Binding {
    let changes_settings = matches!(
        action,
        Action::ToggleSpeech | Action::RateStep(_) | Action::VolumeStep(_) | Action::Setting(_) | Action::Learn | Action::OpenLayer(_) | Action::PassNext
    );
    Binding { name, layer, combo, action, changes_settings }
}

use Action as A;
use ReviewCmd as R;
use SettingCmd as S;

fn review_layer_bindings() -> Vec<Binding> {
    let l = Some(Layer::Review);
    vec![
        b("previous line", l, Combo::plain("Up"), A::Review(R::PrevLine)),
        b("next line", l, Combo::plain("Down"), A::Review(R::NextLine)),
        b("previous word", l, Combo::plain("Left"), A::Review(R::PrevWord)),
        b("next word", l, Combo::plain("Right"), A::Review(R::NextWord)),
        b("previous character", l, Combo::plain("comma"), A::Review(R::PrevChar)),
        b("next character", l, Combo::plain("period"), A::Review(R::NextChar)),
        b("read line", l, Combo::plain("l"), A::Review(R::ReadLine)),
        b("read word", l, Combo::plain("w"), A::Review(R::ReadWord)),
        b("read character", l, Combo::plain("c"), A::Review(R::ReadChar)),
        b("first line", l, Combo::plain("Home"), A::Review(R::FirstLine)),
        b("last line", l, Combo::plain("End"), A::Review(R::LastLine)),
        b("top left", l, Combo::plain("t"), A::Review(R::TopLeft)),
        b("bottom right", l, Combo::plain("b"), A::Review(R::BottomRight)),
        b("start of line", l, Combo::plain("s"), A::Review(R::StartOfLine)),
        b("end of line", l, Combo::plain("e"), A::Review(R::EndOfLine)),
        b("read all from here", l, Combo::plain("a"), A::Review(R::ReadAll)),
    ]
}

fn browse_layer_bindings() -> Vec<Binding> {
    let l = Some(Layer::Browse);
    let mut v = vec![
        b("next", l, Combo::plain("Down"), A::Browse(BrowseCmd::Next)),
        b("previous", l, Combo::plain("Up"), A::Browse(BrowseCmd::Prev)),
        b("read here", l, Combo::plain("r"), A::Browse(BrowseCmd::Read)),
        b("switch between browse and focus mode", l, Combo::plain("m"), A::Browse(BrowseCmd::ToggleMode)),
        b("list the headings", l, Combo::plain("1"), A::Browse(BrowseCmd::List(Kind::Heading))),
        b("list the links", l, Combo::plain("2"), A::Browse(BrowseCmd::List(Kind::Link))),
        b("list the form fields", l, Combo::plain("3"), A::Browse(BrowseCmd::List(Kind::Field))),
    ];
    for (name, key, kind) in [
        ("heading", "h", Kind::Heading),
        ("link", "k", Kind::Link),
        ("list", "l", Kind::List),
        ("table", "t", Kind::Table),
        ("form field", "f", Kind::Field),
        ("button", "b", Kind::Button),
        ("graphic", "g", Kind::Graphic),
        ("landmark", "a", Kind::Landmark),
    ] {
        v.push(b(name, l, Combo::plain(key), A::Browse(BrowseCmd::Jump(kind, true))));
        v.push(b(name, l, Combo::shifted(key), A::Browse(BrowseCmd::Jump(kind, false))));
    }
    v
}

fn settings_layer_bindings() -> Vec<Binding> {
    let l = Some(Layer::Settings);
    vec![
        b("faster", l, Combo::plain("Up"), A::RateStep(5)),
        b("slower", l, Combo::plain("Down"), A::RateStep(-5)),
        b("louder", l, Combo::plain("Right"), A::VolumeStep(5)),
        b("quieter", l, Combo::plain("Left"), A::VolumeStep(-5)),
        b("brief or verbose", l, Combo::plain("v"), A::Setting(S::Verbosity)),
        b("typing echo", l, Combo::plain("t"), A::Setting(S::TypingEcho)),
        b("sounds on or off", l, Combo::plain("s"), A::Setting(S::Sounds)),
        b("louder sounds", l, Combo::plain("Page_Up"), A::Setting(S::CueStep(5))),
        b("quieter sounds", l, Combo::plain("Page_Down"), A::Setting(S::CueStep(-5))),
        b("laptop keys for review", l, Combo::plain("l"), A::Setting(S::Layout)),
    ]
}

fn numpad_review_bindings() -> Vec<Binding> {
    let n = |k: &str| Combo::plain(k);
    vec![
        b("previous line", None, n("KP_7"), A::Review(R::PrevLine)),
        b("read line", None, n("KP_4"), A::Review(R::ReadLine)),
        b("next line", None, n("KP_1"), A::Review(R::NextLine)),
        b("previous word", None, n("KP_8"), A::Review(R::PrevWord)),
        b("read word", None, n("KP_5"), A::Review(R::ReadWord)),
        b("next word", None, n("KP_2"), A::Review(R::NextWord)),
        b("previous character", None, n("KP_9"), A::Review(R::PrevChar)),
        b("read character", None, n("KP_6"), A::Review(R::ReadChar)),
        b("next character", None, n("KP_3"), A::Review(R::NextChar)),
        b("say all", None, n("KP_Add"), A::SayAll),
    ]
}

pub fn default_bindings(layout: Layout) -> Vec<Binding> {
    let mut v = vec![
        b("toggle speech", None, Combo::plain("s"), A::ToggleSpeech),
        b("repeat last", None, Combo::plain("r"), A::RepeatLast),
        b("spell last", None, Combo::shifted("r"), A::SpellLast),
        b("where am I", None, Combo::plain("w"), A::WhereAmI),
        b("next sentence", None, Combo::plain("Right"), A::SkipSentence(1)),
        b("previous sentence", None, Combo::plain("Left"), A::SkipSentence(-1)),
        b("next paragraph", None, Combo::plain("Down"), A::SkipParagraph(true)),
        b("previous paragraph", None, Combo::plain("Up"), A::SkipParagraph(false)),
        b("send the next key to the program", None, Combo::plain("p"), A::PassNext),
        b("learn mode", None, Combo::plain("h"), A::Learn),
        b("review layer", None, Combo::plain("v"), A::OpenLayer(Layer::Review)),
        b("settings layer", None, Combo::plain("g"), A::OpenLayer(Layer::Settings)),
        b("browse layer", None, Combo::plain("b"), A::OpenLayer(Layer::Browse)),
    ];
    if layout == Layout::Desktop {
        v.extend(numpad_review_bindings());
    } else {
        v.push(b("say all", None, Combo::plain("a"), A::SayAll));
    }
    v.extend(review_layer_bindings());
    v.extend(settings_layer_bindings());
    v.extend(browse_layer_bindings());
    v
}
