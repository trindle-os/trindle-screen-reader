//! The reader proper: bus events in, speech out. One struct holds the state; each kind of input has its own method.

use reader::bus::Bus;
use reader::coordinator::{estimate_ms, Coordinator, Out};
use reader::cues::{Cue, CueGate};
use reader::describe::Describer;
use reader::keys::{Action, BrowseCmd, KeyState, Keymap, Layout, ReviewCmd, SettingCmd, TapCounter, Wire};
use reader::model::{Event, ObjectInfo, Priority, Source, Utterance};
use reader::novoice::{Alarm, Fallback};
use atspi::ObjectRefOwned;
use reader::browse::Browser;
use reader::bus::Doc;
use reader::review::{Review, ReviewItem};
use reader::settings::Settings;
use reader::voice::{self, Note, Voice};
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

mod commands;

enum Fetched {
    Items { generation: u64, items: Option<(Vec<ReviewItem>, Option<u64>)> },
    Text { generation: u64, text: Option<(String, bool)> },
    Doc { generation: u64, doc: Option<Doc> },
}

const FETCH_TEXT: u8 = 0;
const FETCH_ITEMS: u8 = 1;
const FETCH_DOC: u8 = 2;

const OWN_FOCUS_MS: u64 = 1500;

const REAP_EVERY: Duration = Duration::from_millis(500);

fn own(text: impl Into<String>, priority: Priority) -> Utterance {
    Utterance::new(text, priority, Source::Own)
}

async fn connect_voice() -> Option<(Voice, mpsc::UnboundedReceiver<Note>)> {
    Voice::connect(&voice::socket_path()?).await.ok()
}

async fn next_note(notes: &mut Option<mpsc::UnboundedReceiver<Note>>) -> Option<Note> {
    match notes.as_mut() {
        Some(n) => n.recv().await,
        None => std::future::pending().await,
    }
}

fn review_outs(coord: &mut Coordinator, now: u64, lines: Vec<String>) -> Vec<Out> {
    let mut outs = Vec::new();
    for (i, l) in lines.into_iter().enumerate() {
        let pr = if i == 0 { Priority::Urgent } else { Priority::Normal };
        outs.extend(coord.submit(now, own(l, pr)));
    }
    outs
}

fn run_browse(b: &mut Browser, cmd: BrowseCmd) -> Vec<String> {
    match cmd {
        BrowseCmd::Read => b.read(),
        BrowseCmd::Next => b.step(true),
        BrowseCmd::Prev => b.step(false),
        BrowseCmd::Jump(kind, forward) => b.jump(kind, forward),
        BrowseCmd::List(kind) => b.elements(kind),
        BrowseCmd::ToggleMode => b.toggle_mode(),
    }
}

struct Reader {
    bus: Rc<Bus>,
    settings: Settings,
    describer: Describer,
    coord: Coordinator,
    start: Instant,
    force_text: bool,
    voice: Option<Voice>,
    notes: Option<mpsc::UnboundedReceiver<Note>>,
    retry_at: Option<u64>,
    failures: u32,
    was_lost: bool,
    alarm: Alarm,
    player: Fallback,
    gate: CueGate,
    cue_dir: Option<PathBuf>,
    keymap: Keymap,
    key_state: KeyState,
    taps: TapCounter,
    to_compositor: reader::keysock::Back,
    last_focus: Option<ObjectInfo>,
    review: Option<Review>,
    generation: u64,
    say_all_wanted: bool,
    review_wanted: Option<(u64, ReviewCmd)>,
    browse: Option<Browser>,
    browse_refs: Vec<ObjectRefOwned>,
    own_focus: Vec<(reader::model::Role, String, u64)>,
    browse_wanted: Option<(u64, BrowseCmd)>,
    fetching: HashSet<(u64, u8)>,
    fetch_tx: mpsc::UnboundedSender<Fetched>,
    fake_tx: mpsc::UnboundedSender<u64>,
}

impl Reader {
    fn new(
        bus: Rc<Bus>,
        settings: Settings,
        force_text: bool,
        to_compositor: reader::keysock::Back,
        fetch_tx: mpsc::UnboundedSender<Fetched>,
        fake_tx: mpsc::UnboundedSender<u64>,
    ) -> Reader {
        let mut describer = Describer::new(settings.verbosity);
        describer.typing_echo = settings.typing_echo;
        let layout = if settings.laptop_layout { Layout::Laptop } else { Layout::Desktop };
        Reader {
            bus,
            settings,
            describer,
            coord: Coordinator::new(),
            start: Instant::now(),
            force_text,
            voice: None,
            notes: None,
            retry_at: None,
            failures: 0,
            was_lost: false,
            alarm: Alarm::default(),
            player: Fallback::new(),
            gate: CueGate::default(),
            cue_dir: None,
            keymap: Keymap::new(layout),
            key_state: KeyState::default(),
            taps: TapCounter::default(),
            to_compositor,
            last_focus: None,
            review: None,
            generation: 0,
            say_all_wanted: false,
            review_wanted: None,
            browse: None,
            browse_refs: Vec::new(),
            own_focus: Vec::new(),
            browse_wanted: None,
            fetching: HashSet::new(),
            fetch_tx,
            fake_tx,
        }
    }

    fn now(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }

    fn emit(&self, outs: Vec<Out>) {
        for o in outs {
            match (&o, &self.voice) {
                (_, Some(v)) => v.send(&o),
                (Out::Say { id, text, .. }, None) => {
                    println!("{text}");
                    let (id, ms, tx) = (*id, estimate_ms(text), self.fake_tx.clone());
                    tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_millis(ms)).await;
                        let _ = tx.send(id);
                    });
                }
                (Out::Stop, None) => {}
            }
        }
    }

    fn say(&mut self, u: Utterance) {
        let outs = self.coord.submit(self.now(), u);
        self.emit(outs);
    }

    fn cue(&mut self, cue: Cue) {
        let at = self.now();
        if let (true, Some(dir), Some(v)) = (self.settings.sounds, self.cue_dir.as_deref(), self.voice.as_ref()) {
            if self.gate.allow(cue, at) {
                v.cue(&reader::cues::path(dir, cue), self.settings.cue_volume);
            }
        }
    }

    fn use_voice(&mut self, mut v: Voice, notes: mpsc::UnboundedReceiver<Note>) {
        v.set_levels(self.settings.rate, self.settings.volume);
        self.voice = Some(v);
        self.notes = Some(notes);
    }

    fn on_bus_event(&mut self, ev: Event) {
        if let Some(c) = reader::cues::cue_for(&ev) {
            self.cue(c);
        }
        match &ev {
            Event::Focus { object } if self.took_own_focus(object) => {
                self.last_focus = Some(object.clone());
                let _ = self.describer.event(&ev);
                return;
            }
            Event::Focus { object } => {
                self.last_focus = Some(object.clone());
                self.review = None;
                self.review_wanted = None;
                self.browse = None;
                self.browse_wanted = None;
                self.generation += 1;
            }
            Event::TextInserted { .. } => {
                let outs = self.coord.key(self.now(), false, false);
                self.emit(outs);
            }
            _ => {}
        }
        if let Some(u) = self.describer.event(&ev) {
            self.say(u);
        }
    }

    fn on_lock(&mut self, e: reader::locksock::LockEvent) {
        use reader::locksock::LockEvent as L;
        let (text, priority) = match e {
            L::Locked => (reader::words::LOCKED.to_string(), Priority::Urgent),
            L::Unlocked => (reader::words::UNLOCKED.to_string(), Priority::Urgent),
            L::Wrong => (reader::words::WRONG_PASSWORD.to_string(), Priority::Urgent),
            L::Time(h, m) => (reader::words::lock_time_said(h, m), Priority::Normal),
            L::Entry(0) => (reader::words::ENTRY_EMPTY.to_string(), Priority::Normal),
            L::Entry(n) => (reader::words::entry_said(n), Priority::Normal),
        };
        self.say(own(text, priority));
    }

    fn on_wire(&mut self, w: Wire) {
        let actions = match w {
            Wire::Key(k) => {
                if k.pressed {
                    let modifier = matches!(k.key.as_str(), "Shift" | "Control" | "Alt" | "Super" | "Caps_Lock" | "Insert");
                    let outs = self.coord.key(self.now(), modifier, k.real && !k.reader);
                    if !modifier && k.real && !k.reader {
                        self.say_all_wanted = false;
                    }
                    self.emit(outs);
                }
                let was = self.key_state.in_learn_mode();
                let browsing = self.key_state.in_browse_keys();
                let a = self.key_state.handle(&self.keymap, &k);
                if self.key_state.in_learn_mode() != was {
                    reader::keysock::send_back(&self.to_compositor, reader::keys::learn_line(!was));
                }
                self.tell_browse_keys(browsing);
                a
            }
            Wire::Tap { key, real } => {
                if self.taps.tap(self.now(), &key, real) {
                    vec![Action::GiveBackCapsLock]
                } else {
                    vec![]
                }
            }
            Wire::Linked => {
                self.key_state.new_link();
                vec![]
            }
            Wire::PlainKeys => {
                self.key_state.set_plain_keys(true);
                vec![]
            }
            Wire::CapsState(on) => vec![Action::Say((if on { reader::words::CAPS_ON } else { reader::words::CAPS_OFF }).into())],
        };
        for a in actions {
            self.do_action(a);
        }
    }

    fn do_action(&mut self, a: Action) {
        let outs = match &a {
            Action::Pass(line) => {
                reader::keysock::send_back(&self.to_compositor, line.clone());
                return;
            }
            Action::GiveBackCapsLock => {
                reader::keysock::send_back(&self.to_compositor, reader::keys::CAPS_TOGGLE_LINE.into());
                return;
            }
            Action::WhereAmI => match &self.last_focus {
                Some(o) => self.coord.submit(self.now(), own(self.describer.where_am_i(o), Priority::Focus)),
                None => vec![],
            },
            Action::StopSpeech => {
                self.say_all_wanted = false;
                let now = self.now();
                reader::keys::apply_basic(&mut self.coord, now, &a)
            }
            Action::SayAll => {
                self.say_all_wanted = true;
                self.fetch_text();
                vec![]
            }
            Action::Review(cmd) => self.review_command(*cmd),
            Action::Browse(cmd) => self.browse_command(*cmd),
            Action::RateStep(n) => self.step_level(true, *n),
            Action::VolumeStep(n) => self.step_level(false, *n),
            Action::Setting(cmd) => self.change_setting(*cmd),
            _ => {
                let now = self.now();
                reader::keys::apply_basic(&mut self.coord, now, &a)
            }
        };
        match &a {
            Action::ToggleSpeech => self.cue(if self.coord.muted { Cue::SpeechOff } else { Cue::SpeechOn }),
            Action::OpenLayer(_) => self.cue(Cue::Layer),
            _ => {}
        }
        self.emit(outs);
    }

    fn on_note(&mut self, n: Option<Note>) {
        match n {
            Some(Note::Done(id)) => {
                let outs = self.coord.speech_done(self.now(), id);
                self.emit(outs);
            }
            Some(Note::Started(_)) => {}
            Some(Note::Lost) | None => {
                reader::log::note("voice service lost; printing the words instead");
                self.voice = None;
                self.notes = None;
                self.was_lost = true;
                self.retry_at = Some(self.now() + voice::retry_delay_ms(0));
                let outs = self.coord.voice_lost();
                self.emit(outs);
            }
        }
    }

    fn on_fake_done(&mut self, id: u64) {
        let outs = self.coord.speech_done(self.now(), id);
        self.emit(outs);
    }

    async fn on_wake(&mut self) {
        if !self.force_text && self.voice.is_none() && self.retry_at.is_some_and(|t| self.now() >= t) {
            self.retry_voice().await;
        }
        if !self.force_text && self.alarm.due(Instant::now(), self.voice.is_some()) {
            self.player.sound();
        }
        self.player.busy();
        let outs = self.coord.tick(self.now());
        self.emit(outs);
    }

    async fn retry_voice(&mut self) {
        match connect_voice().await {
            Some((v, n)) => {
                self.use_voice(v, n);
                self.retry_at = None;
                self.failures = 0;
                if std::mem::take(&mut self.was_lost) {
                    self.say(own(reader::words::SPEECH_RESTARTED, Priority::Urgent));
                }
            }
            None => {
                self.failures += 1;
                self.retry_at = Some(self.now() + voice::retry_delay_ms(self.failures));
            }
        }
    }

    fn wake_in(&self) -> Option<Duration> {
        let now = self.now();
        let mut waits = Vec::new();
        waits.extend(self.coord.next_wake(now).map(|t| Duration::from_millis(t.saturating_sub(now))));
        if !self.force_text && self.voice.is_none() {
            waits.extend(self.retry_at.map(|t| Duration::from_millis(t.saturating_sub(now))));
        }
        if !self.force_text {
            waits.extend(self.alarm.wait(Instant::now(), self.voice.is_some()));
        }
        if self.player.running() {
            waits.push(REAP_EVERY);
        }
        waits.into_iter().min()
    }

    fn greet(&mut self, greeter: bool) {
        if std::env::var_os(reader::supervisor::RESTARTED_ENV).is_some() {
            self.say(own(reader::words::RESTARTED, Priority::Urgent));
            self.cue(Cue::Restarted);
        } else {
            self.say(own(if greeter { reader::words::SCREEN_READER_ON } else { reader::words::WELCOME }, Priority::Urgent));
        }
    }
}

fn open_keys(tx: mpsc::Sender<Wire>, back: &reader::keysock::Back) -> Result<Option<reader::keysock::KeySock>, String> {
    let Some(dir) = reader::runtime_dir() else {
        reader::log::note("no runtime folder, so no reader keys");
        return Ok(None);
    };
    match reader::keysock::listen(&dir, tx, back.clone())? {
        Some(k) => Ok(Some(k)),
        None => Err("another screen reader is already running".into()),
    }
}

fn make_cues(settings: &Settings) -> Option<PathBuf> {
    let dir = Settings::cues_dir();
    if let Some(d) = dir.as_deref().filter(|d| settings.sounds && reader::cues::incomplete(d)) {
        if let Err(e) = reader::cues::write_all(d) {
            reader::log::note(&format!("could not make the sound cues in {}: {e}", d.display()));
        }
    }
    dir
}

pub async fn run(force_text: bool) -> Result<(), String> {
    let (key_tx, mut key_rx) = mpsc::channel::<Wire>(64);
    let to_compositor = reader::keysock::Back::default();
    let _keys = open_keys(key_tx, &to_compositor)?;
    let (lock_tx, mut lock_rx) = mpsc::channel::<reader::locksock::LockEvent>(16);
    let _lock = match _keys.as_ref().and_then(|k| k.path.parent()) {
        Some(dir) => reader::locksock::listen(dir, lock_tx).map_err(|e| reader::log::note(&format!("no lock screen socket: {e}"))).ok(),
        None => None,
    };

    let bus = Rc::new(Bus::connect().await?);
    let mut events = bus.clone().events();
    let (settings, problems) = Settings::load();
    for p in &problems {
        reader::log::note(p);
    }
    let (fetch_tx, mut fetch_rx) = mpsc::unbounded_channel::<Fetched>();
    let (fake_tx, mut fake_rx) = mpsc::unbounded_channel::<u64>();
    let mut r = Reader::new(bus.clone(), settings, force_text, to_compositor, fetch_tx, fake_tx);

    if !force_text {
        match connect_voice().await {
            Some((v, n)) => r.use_voice(v, n),
            None => reader::log::note("no voice service; printing the words instead"),
        }
    }
    r.retry_at = r.voice.is_none().then(|| r.now() + voice::retry_delay_ms(0));
    eprintln!("screen reader on");
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).map_err(|e| e.to_string())?;

    let lock_path = reader::handover::lock_path();
    let (greeter, _held) = if reader::handover::is_greeter() {
        (true, reader::handover::take(&lock_path))
    } else {
        reader::handover::wait_until_free(&lock_path, reader::handover::MAX_WAIT).await;
        (false, None)
    };
    r.cue_dir = make_cues(&r.settings);
    r.greet(greeter);
    if !problems.is_empty() {
        r.say(own(reader::words::SETTINGS_PROBLEM, Priority::Normal));
    }

    let result = loop {
        let wake = r.wake_in();
        tokio::select! {
            ev = events.recv() => match ev {
                Some(ev) => r.on_bus_event(ev),
                None => break Err("the accessibility bus closed".to_string()),
            },
            Some(w) = key_rx.recv() => r.on_wire(w),
            Some(e) = lock_rx.recv() => r.on_lock(e),
            Some(f) = fetch_rx.recv() => r.on_fetched(f),
            Some(id) = fake_rx.recv() => r.on_fake_done(id),
            n = next_note(&mut r.notes) => r.on_note(n),
            _ = async { match wake { Some(d) => tokio::time::sleep(d).await, None => std::future::pending().await } } => r.on_wake().await,
            _ = tokio::signal::ctrl_c() => break Ok(()),
            _ = term.recv() => break Ok(()),
        }
    };
    bus.finish().await;
    result
}
