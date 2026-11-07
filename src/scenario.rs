//! A scripted session with no screen and no bus: steps go in, the text that would be spoken comes out.

use crate::coordinator::{estimate_ms, Coordinator, Out};
use crate::describe::Describer;
use crate::model::*;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct Scenario {
    #[serde(default)]
    pub verbosity: Verbosity,
    #[serde(rename = "step")]
    pub steps: Vec<Step>,
}

#[derive(Deserialize, Debug)]
pub struct Step {
    pub at: u64,
    #[serde(flatten)]
    pub what: What,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum What {
    Focus { object: ObjectInfo },
    WindowActivated { object: ObjectInfo },
    WindowClosed,
    StateChanged { object: ObjectInfo, state: State, on: bool },
    ValueChanged { object: ObjectInfo },
    Alert { object: ObjectInfo, #[serde(default)] text: String },
    Notification { app: String, text: String, #[serde(default)] urgent: bool },
    Live { text: String, #[serde(default)] urgent: bool },
    CaretMoved { object: ObjectInfo, text: String, old: usize, new: usize },
    TextInserted { object: ObjectInfo, #[serde(default)] before: String, inserted: String },
    TextRemoved { object: ObjectInfo, removed: String },
    Key { #[serde(default)] modifier_only: bool },
}

enum Input {
    Event(Box<Event>),
    Key { modifier_only: bool },
}

impl What {
    fn input(self) -> Input {
        Input::Event(Box::new(match self {
            What::Focus { object } => Event::Focus { object },
            What::WindowActivated { object } => Event::WindowActivated { object },
            What::WindowClosed => Event::WindowClosed,
            What::StateChanged { object, state, on } => Event::StateChanged { object, state, on },
            What::ValueChanged { object } => Event::ValueChanged { object },
            What::Alert { object, text } => Event::Alert { object, text },
            What::Notification { app, text, urgent } => Event::Notification { app, text, urgent },
            What::Live { text, urgent } => Event::Live { text, urgent },
            What::CaretMoved { object, text, old, new } => Event::CaretMoved { object, text, old, new },
            What::TextInserted { object, before, inserted } => Event::TextInserted { object, before, inserted },
            What::TextRemoved { object, removed } => Event::TextRemoved { object, removed },
            What::Key { modifier_only } => return Input::Key { modifier_only },
        }))
    }
}

fn line(t: u64, o: &Out) -> String {
    match o {
        Out::Say { text, priority, .. } => format!("{t} say {priority:?}: {text}"),
        Out::Stop => format!("{t} stop"),
    }
}

pub fn run(s: Scenario) -> Vec<String> {
    let mut d = Describer::new(s.verbosity);
    let mut c = Coordinator::new();
    let mut log = Vec::new();
    let mut ends: Option<(u64, u64)> = None;

    let apply = |t: u64, outs: Vec<Out>, ends: &mut Option<(u64, u64)>, log: &mut Vec<String>| {
        for o in outs {
            log.push(line(t, &o));
            match o {
                Out::Say { id, text, .. } => *ends = Some((id, t + estimate_ms(&text))),
                Out::Stop => *ends = None,
            }
        }
    };

    for step in s.steps {
        while let Some((id, end)) = ends {
            if end > step.at {
                break;
            }
            ends = None;
            let outs = c.speech_done(end, id);
            apply(end, outs, &mut ends, &mut log);
        }
        let outs = match step.what.input() {
            Input::Event(e) => match d.event(&e) {
                Some(u) => c.submit(step.at, u),
                None => vec![],
            },
            Input::Key { modifier_only } => c.key(step.at, modifier_only, true),
        };
        apply(step.at, outs, &mut ends, &mut log);
        let outs = c.tick(step.at);
        apply(step.at, outs, &mut ends, &mut log);
    }
    while let Some((id, end)) = ends {
        ends = None;
        let outs = c.speech_done(end, id);
        apply(end, outs, &mut ends, &mut log);
    }
    log
}

pub fn parse(text: &str) -> Result<Scenario, String> {
    toml::from_str(text).map_err(|e| e.to_string())
}
