//! Most of the words the reader says on its own, as constants. A few more sit in browse.rs, caret.rs and the key names, and nothing is looked up at run time, so another language means a rebuild.

use crate::model::{Role, State};

pub fn role(r: Role) -> &'static str {
    match r {
        Role::Window => "window",
        Role::Dialog => "dialog",
        Role::Alert => "alert",
        Role::Button => "button",
        Role::CheckBox => "check box",
        Role::RadioButton => "radio button",
        Role::Switch => "switch",
        Role::TextEntry => "text entry",
        Role::PasswordEntry => "password entry",
        Role::Label => "",
        Role::Heading => "heading",
        Role::Link => "link",
        Role::List => "list",
        Role::ListItem => "",
        Role::Table => "table",
        Role::Row => "row",
        Role::Cell => "cell",
        Role::Menu => "menu",
        Role::MenuItem => "menu item",
        Role::Tab => "tab",
        Role::TabList => "tab list",
        Role::Slider => "slider",
        Role::SpinButton => "spin button",
        Role::ProgressBar => "progress bar",
        Role::Image => "image",
        Role::Separator => "separator",
        Role::ToolBar => "tool bar",
        Role::StatusBar => "status bar",
        Role::Group => "group",
        Role::ComboBox => "combo box",
        Role::Document => "document",
        Role::Landmark => "landmark",
        Role::Unknown => "",
    }
}

pub fn state_on(s: State) -> Option<&'static str> {
    Some(match s {
        State::Selected => "selected",
        State::Checked => "checked",
        State::Pressed => "pressed",
        State::Expanded => "expanded",
        State::Collapsed => "collapsed",
        State::Disabled => "unavailable",
        State::ReadOnly => "read only",
        State::Required => "required",
        State::Invalid => "invalid",
        State::Busy => "busy",
        State::Focused | State::Secret | State::Modal => return None,
    })
}

pub fn state_off(s: State) -> Option<&'static str> {
    Some(match s {
        State::Selected => "not selected",
        State::Checked => "not checked",
        State::Pressed => "not pressed",
        State::Expanded => "collapsed",
        State::Collapsed => "expanded",
        State::Disabled => "available",
        State::Busy => "ready",
        _ => return None,
    })
}

pub const UNLABELLED: &str = "unlabelled";
pub const EMPTY: &str = "empty";
pub const WHERE_WINDOW: &str = "{window}, {rest}";
pub const RESTARTED: &str = "Screen reader restarted";
pub const SCREEN_READER_ON: &str = "Screen reader on";
pub const WELCOME: &str = "Welcome";
pub const END_OF_DOCUMENT: &str = "End of document";
pub const DOCUMENT_CUT: &str = "Stopped, the rest is too long to read at once";
pub const CAPS_ON: &str = "Caps lock on";
pub const CAPS_OFF: &str = "Caps lock off";
pub const SPEECH_ON: &str = "Speech on";
pub const LAYER_REVIEW: &str = "review";
pub const LAYER_SETTINGS: &str = "settings";
pub const LAYER_BROWSE: &str = "browse";
pub const BROWSE_OFF: &str = "browse off";
pub const BROWSE_EMPTY: &str = "nothing to browse";
pub const START_OF_DOCUMENT: &str = "Start of document";
pub const LEARN_ON: &str = "learn mode, press a key to hear what it does, escape twice to leave";
pub const LEARN_OFF: &str = "learn mode off";
pub const NOT_A_COMMAND: &str = "not a command";
pub const REFUSED: &str = "that key did not come from the keyboard, so this command is not run";
pub const DELETED: &str = "deleted {what}";
pub const SETTINGS_PROBLEM: &str = "Some settings could not be used, so safe ones are in place";
pub const SPEECH_RESTARTED: &str = "Speech restarted";
pub const NOTIFICATION: &str = "{app}: {text}";
pub const POSITION: &str = "{n} of {total}";
pub const IN_CONTAINER: &str = "{name}, {role}";

pub const LOCKED: &str = "Screen locked";
pub const UNLOCKED: &str = "Unlocked";
pub const WRONG_PASSWORD: &str = "Wrong password, try again";
pub const ENTRY_EMPTY: &str = "nothing typed";

pub fn lock_time_said(h: u8, m: u8) -> String {
    format!("{h:02}:{m:02}")
}

pub fn entry_said(n: usize) -> String {
    format!("{n} typed")
}

pub const REVIEW_TOP: &str = "top";

pub const REVIEW_BOTTOM: &str = "bottom";

pub const REVIEW_EMPTY: &str = "nothing on screen";
pub const WINDOW_CUT: &str = "This window is too large to read whole. The rest is left out.";

pub const REVIEW_PROTECTED: &str = "protected";

pub const VERBOSITY_BRIEF: &str = "brief";
pub const VERBOSITY_VERBOSE: &str = "verbose";
pub const SOUNDS_ON: &str = "sounds on";
pub const SOUNDS_OFF: &str = "sounds off";
pub const LAYOUT_LAPTOP: &str = "laptop keys for review";
pub const LAYOUT_NUMBER_PAD: &str = "number pad keys for review";

pub fn typing_echo_said(e: crate::settings::TypingEcho) -> &'static str {
    use crate::settings::TypingEcho as T;
    match e {
        T::None => "typing echo off",
        T::Character => "typing echo characters",
        T::Word => "typing echo words",
        T::Both => "typing echo characters and words",
    }
}

pub fn cue_volume_said(n: u8) -> String {
    format!("Sound volume {n}")
}

pub fn rate_said(n: u8) -> String {
    format!("Rate {n}")
}

pub fn volume_said(n: u8) -> String {
    format!("Volume {n}")
}
