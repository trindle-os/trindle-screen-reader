//! Plain data the rest of the reader works on. Nothing here touches a bus or a voice.

use serde::Deserialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Window,
    Dialog,
    Alert,
    Button,
    CheckBox,
    RadioButton,
    Switch,
    TextEntry,
    PasswordEntry,
    Label,
    Heading,
    Link,
    List,
    ListItem,
    Table,
    Row,
    Cell,
    Menu,
    MenuItem,
    Tab,
    TabList,
    Slider,
    SpinButton,
    ProgressBar,
    Image,
    Separator,
    ToolBar,
    StatusBar,
    Group,
    ComboBox,
    Document,
    Landmark,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Focused,
    Selected,
    Checked,
    Pressed,
    Expanded,
    Collapsed,
    Disabled,
    ReadOnly,
    Required,
    Invalid,
    Busy,
    Secret,
    Modal,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ObjectInfo {
    #[serde(default)]
    pub role: Option<Role>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub states: Vec<State>,
    #[serde(default)]
    pub position: Option<(u32, u32)>,
    #[serde(default)]
    pub container: Option<Container>,
    #[serde(default)]
    pub window: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Container {
    pub id: String,
    pub role: Role,
    #[serde(default)]
    pub name: String,
}

impl ObjectInfo {
    pub fn role(&self) -> Role {
        self.role.unwrap_or(Role::Unknown)
    }
    pub fn has(&self, s: State) -> bool {
        self.states.contains(&s)
    }
    pub fn is_secret(&self) -> bool {
        self.role() == Role::PasswordEntry || self.has(State::Secret)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Source {
    Focus,
    Window,
    Value,
    State,
    Caret,
    Typing,
    Live,
    Notification,
    Own,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Priority {
    Background,
    Normal,
    Focus,
    Urgent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Utterance {
    pub text: String,
    pub priority: Priority,
    pub source: Source,
}

impl Utterance {
    pub fn new(text: impl Into<String>, priority: Priority, source: Source) -> Self {
        Utterance { text: text.into(), priority, source }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, serde::Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Verbosity {
    #[default]
    Brief,
    Verbose,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    Focus { object: ObjectInfo },
    WindowActivated { object: ObjectInfo },
    WindowClosed,
    StateChanged { object: ObjectInfo, state: State, on: bool },
    ValueChanged { object: ObjectInfo },
    Alert { object: ObjectInfo, text: String },
    Notification { app: String, text: String, urgent: bool },
    Live { text: String, urgent: bool },
    CaretMoved { object: ObjectInfo, text: String, old: usize, new: usize },
    TextInserted { object: ObjectInfo, before: String, inserted: String },
    TextRemoved { object: ObjectInfo, removed: String },
}
