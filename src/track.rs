//! Turns bus events into plain `Event` values, one ordered worker per program; asks facts through a trait so it runs on a fake.

use crate::model::{Event, ObjectInfo, State as St};
use atspi::events::object::{AnnouncementEvent, Property, PropertyChangeEvent, StateChangedEvent, TextCaretMovedEvent, TextChangedEvent};
use atspi::events::Event as Ev;
use atspi::events::{ObjectEvents, WindowEvents};
use atspi::{ObjectRefOwned, Operation, State as AS};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

const MAX_WORD: usize = 64;
pub const CARET_CONTEXT: usize = 1000;
const LIVE_PER_SECOND: usize = 20;
const MAX_CARETS: usize = 256;
const MAX_BACKLOG: usize = 256;
pub const WORKER_IDLE: Duration = Duration::from_secs(30);

pub fn key_of(r: &ObjectRefOwned) -> String {
    format!("{}{}", r.name_as_str().unwrap_or(""), r.path_as_str())
}

pub fn state_of(s: AS) -> Option<St> {
    Some(match s {
        AS::Selected => St::Selected,
        AS::Checked => St::Checked,
        AS::Pressed => St::Pressed,
        AS::Expanded => St::Expanded,
        AS::Required => St::Required,
        AS::InvalidEntry => St::Invalid,
        AS::Busy => St::Busy,
        AS::Modal => St::Modal,
        _ => return None,
    })
}

#[allow(async_fn_in_trait)]
pub trait Facts {
    async fn describe(&self, r: &ObjectRefOwned) -> Option<ObjectInfo>;
    async fn light(&self, r: &ObjectRefOwned) -> Option<ObjectInfo>;
    async fn text_range(&self, r: &ObjectRefOwned, from: i32, to: i32) -> Option<String>;
    async fn text_around(&self, r: &ObjectRefOwned, lo: usize, hi: usize) -> Option<(String, usize)>;
    async fn appeared(&self, r: &ObjectRefOwned) -> Option<Event>;
}

#[derive(Default)]
pub struct Tracker {
    carets: HashMap<String, usize>,
    focused: String,
    pub focused_ref: Option<ObjectRefOwned>,
    live: HashMap<String, VecDeque<Instant>>,
}

impl Tracker {
    pub fn observe(&mut self, ev: &Ev) {
        if let Ev::Object(ObjectEvents::StateChanged(s)) = ev {
            if s.state == AS::Focused && s.enabled {
                self.focused = key_of(&s.item);
                self.focused_ref = Some(s.item.clone());
            }
        }
    }

    fn is_focused(&self, r: &ObjectRefOwned) -> bool {
        key_of(r) == self.focused
    }

    fn in_front(&self, r: &ObjectRefOwned) -> bool {
        self.focused_ref.as_ref().is_some_and(|f| f.name_as_str() == r.name_as_str())
    }

    fn live_allowed(&mut self, app: &str, now: Instant) -> bool {
        rate_allowed(&mut self.live, app, now)
    }

    fn remember_caret(&mut self, key: String, pos: usize) -> Option<usize> {
        remember(&mut self.carets, key, pos)
    }
}

pub fn rate_allowed(live: &mut HashMap<String, VecDeque<Instant>>, app: &str, now: Instant) -> bool {
    let q = live.entry(app.to_string()).or_default();
    while q.front().is_some_and(|t| now.duration_since(*t) > Duration::from_secs(1)) {
        q.pop_front();
    }
    if q.len() >= LIVE_PER_SECOND {
        return false;
    }
    q.push_back(now);
    true
}

fn remember(c: &mut HashMap<String, usize>, key: String, pos: usize) -> Option<usize> {
    if c.len() >= MAX_CARETS && !c.contains_key(&key) {
        c.clear();
    }
    c.insert(key, pos)
}

fn caret_span(old: usize, new: usize) -> (usize, usize) {
    if old.abs_diff(new) > 2 * CARET_CONTEXT {
        (new, new)
    } else {
        (old.min(new), old.max(new))
    }
}

fn within(pos: usize, base: usize, len: usize) -> usize {
    pos.clamp(base, base + len) - base
}

fn app_of(ev: &Ev) -> Option<&str> {
    let item = match ev {
        Ev::Object(ObjectEvents::StateChanged(s)) => &s.item,
        Ev::Object(ObjectEvents::PropertyChange(p)) => &p.item,
        Ev::Object(ObjectEvents::Announcement(a)) => &a.item,
        Ev::Object(ObjectEvents::TextCaretMoved(c)) => &c.item,
        Ev::Object(ObjectEvents::TextChanged(t)) => &t.item,
        Ev::Window(WindowEvents::Activate(a)) => &a.item,
        Ev::Window(WindowEvents::Close(c)) => &c.item,
        _ => return None,
    };
    item.name_as_str()
}

async fn caret_moved<F: Facts>(f: &F, tr: &RefCell<Tracker>, c: TextCaretMovedEvent) -> Option<Event> {
    if !tr.borrow().is_focused(&c.item) {
        return None;
    }
    let object = f.light(&c.item).await?;
    if object.is_secret() {
        return None;
    }
    let new = c.position.max(0) as usize;
    let old = tr.borrow_mut().remember_caret(key_of(&c.item), new).unwrap_or(new);
    let (lo, hi) = caret_span(old, new);
    let (text, base) = f.text_around(&c.item, lo, hi).await?;
    let len = text.chars().count();
    Some(Event::CaretMoved { object, text, old: within(old, base, len), new: within(new, base, len) })
}

async fn state_changed<F: Facts>(f: &F, tr: &RefCell<Tracker>, s: StateChangedEvent) -> Option<Event> {
    let state = s.state;
    if state == AS::Focused {
        if !s.enabled {
            return None;
        }
        let object = f.describe(&s.item).await?;
        if !tr.borrow().is_focused(&s.item) {
            return None;
        }
        return Some(Event::Focus { object });
    }
    if state == AS::Showing {
        return if s.enabled { f.appeared(&s.item).await } else { None };
    }
    let st = state_of(state)?;
    if !tr.borrow().is_focused(&s.item) {
        return None;
    }
    let object = f.light(&s.item).await?;
    Some(Event::StateChanged { object, state: st, on: s.enabled })
}

async fn property_changed<F: Facts>(f: &F, tr: &RefCell<Tracker>, p: PropertyChangeEvent) -> Option<Event> {
    if !tr.borrow().is_focused(&p.item) {
        return None;
    }
    let is_value = matches!(&p.value, Property::Name(_)) || matches!(&p.value, Property::Other((n, _)) if n.contains("value"));
    if !is_value {
        return None;
    }
    let object = f.describe(&p.item).await?;
    Some(Event::ValueChanged { object })
}

fn announced(tr: &RefCell<Tracker>, a: AnnouncementEvent) -> Option<Event> {
    let app = a.item.name_as_str().unwrap_or("").to_string();
    let mut t = tr.borrow_mut();
    if !t.live_allowed(&app, Instant::now()) {
        return None;
    }
    let urgent = a.live == atspi::Politeness::Assertive && t.in_front(&a.item);
    Some(Event::Live { text: a.text, urgent })
}

async fn text_changed<F: Facts>(f: &F, tr: &RefCell<Tracker>, t: TextChangedEvent) -> Option<Event> {
    if !tr.borrow().is_focused(&t.item) {
        return None;
    }
    let object = f.light(&t.item).await?;
    let start = t.start_pos.max(0) as usize;
    let len = t.length.max(0) as usize;
    let key = key_of(&t.item);
    if object.is_secret() {
        return Some(match t.operation {
            Operation::Insert => Event::TextInserted { object, before: String::new(), inserted: String::new() },
            Operation::Delete => Event::TextRemoved { object, removed: String::new() },
        });
    }
    match t.operation {
        Operation::Insert => {
            tr.borrow_mut().remember_caret(key, start + len);
            let from = start.saturating_sub(MAX_WORD) as i32;
            let before = f.text_range(&t.item, from, start as i32).await.unwrap_or_default();
            Some(Event::TextInserted { object, before, inserted: t.text })
        }
        Operation::Delete => {
            tr.borrow_mut().remember_caret(key, start);
            Some(Event::TextRemoved { object, removed: t.text })
        }
    }
}

pub async fn convert<F: Facts>(f: &F, tr: &RefCell<Tracker>, ev: Ev) -> Option<Event> {
    match ev {
        Ev::Object(ObjectEvents::StateChanged(s)) => state_changed(f, tr, s).await,
        Ev::Object(ObjectEvents::PropertyChange(p)) => property_changed(f, tr, p).await,
        Ev::Object(ObjectEvents::Announcement(a)) => announced(tr, a),
        Ev::Object(ObjectEvents::TextCaretMoved(c)) => caret_moved(f, tr, c).await,
        Ev::Object(ObjectEvents::TextChanged(t)) => text_changed(f, tr, t).await,
        Ev::Window(WindowEvents::Activate(a)) => {
            let object = f.describe(&a.item).await?;
            Some(Event::WindowActivated { object })
        }
        Ev::Window(WindowEvents::Close(_)) => Some(Event::WindowClosed),
        _ => None,
    }
}

type Workers = Rc<RefCell<HashMap<String, (UnboundedSender<Ev>, Rc<Cell<usize>>)>>>;

pub struct Pipeline<F> {
    facts: Rc<F>,
    tracker: Rc<RefCell<Tracker>>,
    out: UnboundedSender<Event>,
    idle: Duration,
    workers: Workers,
}

impl<F: Facts + 'static> Pipeline<F> {
    pub fn new(facts: Rc<F>, tracker: Rc<RefCell<Tracker>>, out: UnboundedSender<Event>, idle: Duration) -> Self {
        Pipeline { facts, tracker, out, idle, workers: Rc::new(RefCell::new(HashMap::new())) }
    }

    pub fn feed(&self, ev: Ev) {
        self.tracker.borrow_mut().observe(&ev);
        let Some(app) = app_of(&ev).map(str::to_string) else { return };
        let mut map = self.workers.borrow_mut();
        let (tx, pending) = map.entry(app.clone()).or_insert_with(|| {
            let (tx, rx) = unbounded_channel();
            let pending = Rc::new(Cell::new(0));
            tokio::task::spawn_local(worker(self.facts.clone(), self.tracker.clone(), Inbox { rx, pending: pending.clone() }, self.out.clone(), self.workers.clone(), app, self.idle));
            (tx, pending)
        });
        let important = matches!(&ev, Ev::Window(_)) || matches!(&ev, Ev::Object(ObjectEvents::StateChanged(s)) if s.state == AS::Focused);
        if pending.get() >= MAX_BACKLOG && !important {
            return;
        }
        pending.set(pending.get() + 1);
        let _ = tx.send(ev);
    }
}

struct Inbox {
    rx: tokio::sync::mpsc::UnboundedReceiver<Ev>,
    pending: Rc<Cell<usize>>,
}

async fn worker<F: Facts>(
    facts: Rc<F>,
    tracker: Rc<RefCell<Tracker>>,
    inbox: Inbox,
    out: UnboundedSender<Event>,
    workers: Workers,
    app: String,
    idle: Duration,
) {
    let Inbox { mut rx, pending } = inbox;
    loop {
        match tokio::time::timeout(idle, rx.recv()).await {
            Ok(Some(ev)) => {
                pending.set(pending.get().saturating_sub(1));
                if let Some(e) = convert(&*facts, &tracker, ev).await {
                    if out.send(e).is_err() {
                        break;
                    }
                }
            }
            Ok(None) => break,
            Err(_) if rx.is_empty() => break,
            Err(_) => {}
        }
    }
    workers.borrow_mut().remove(&app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use atspi::events::object::{AnnouncementEvent, PropertyChangeEvent, StateChangedEvent, TextChangedEvent};
    use atspi::Politeness;
    use crate::model::Role;

    fn obj(app: &'static str, path: &'static str) -> ObjectRefOwned {
        ObjectRefOwned::from_static_str_unchecked(app, path)
    }

    fn focus(item: ObjectRefOwned) -> Ev {
        Ev::Object(ObjectEvents::StateChanged(StateChangedEvent { item, state: atspi::State::Focused, enabled: true }))
    }

    fn caret(item: ObjectRefOwned, position: i32) -> Ev {
        Ev::Object(ObjectEvents::TextCaretMoved(TextCaretMovedEvent { item, position }))
    }

    fn typed(item: ObjectRefOwned, op: Operation, start: i32, text: &str) -> Ev {
        Ev::Object(ObjectEvents::TextChanged(TextChangedEvent { item, operation: op, start_pos: start, length: text.chars().count() as i32, text: text.into() }))
    }

    fn announce(item: ObjectRefOwned, live: Politeness) -> Ev {
        Ev::Object(ObjectEvents::Announcement(AnnouncementEvent { item, text: "hi".into(), live }))
    }

    #[derive(Default)]
    struct Fake {
        secret: bool,
        text: String,
        slow_app: Option<(&'static str, Duration)>,
        slow_path: Option<(&'static str, Duration)>,
        asked: RefCell<Vec<(usize, usize)>>,
        order: RefCell<Vec<String>>,
    }

    impl Facts for Fake {
        async fn describe(&self, r: &ObjectRefOwned) -> Option<ObjectInfo> {
            if let Some((app, d)) = self.slow_app {
                if r.name_as_str() == Some(app) {
                    tokio::time::sleep(d).await;
                }
            }
            if let Some((path, d)) = self.slow_path {
                if r.path_as_str() == path {
                    tokio::time::sleep(d).await;
                }
            }
            self.order.borrow_mut().push(r.path_as_str().to_string());
            Some(ObjectInfo { role: Some(Role::Button), name: r.path_as_str().into(), ..Default::default() })
        }
        async fn light(&self, _: &ObjectRefOwned) -> Option<ObjectInfo> {
            let states = if self.secret { vec![St::Secret] } else { vec![] };
            Some(ObjectInfo { role: Some(Role::TextEntry), states, ..Default::default() })
        }
        async fn text_range(&self, _: &ObjectRefOwned, from: i32, to: i32) -> Option<String> {
            Some(self.text.chars().skip(from as usize).take((to - from) as usize).collect())
        }
        async fn text_around(&self, _: &ObjectRefOwned, lo: usize, hi: usize) -> Option<(String, usize)> {
            self.asked.borrow_mut().push((lo, hi));
            let n = self.text.chars().count();
            let start = lo.saturating_sub(CARET_CONTEXT).min(n);
            let end = (hi + CARET_CONTEXT).min(n);
            Some((self.text.chars().skip(start).take(end - start).collect(), start))
        }
        async fn appeared(&self, _: &ObjectRefOwned) -> Option<Event> {
            None
        }
    }

    fn tracker_focused(r: &ObjectRefOwned) -> RefCell<Tracker> {
        let mut t = Tracker::default();
        t.observe(&focus(r.clone()));
        RefCell::new(t)
    }

    #[test]
    fn roles_state_map_and_a_far_jump_reads_around_the_new_caret_only() {
        assert_eq!(state_of(AS::Checked), Some(St::Checked));
        assert_eq!(state_of(AS::Focused), None);
        assert_eq!(caret_span(10, 30), (10, 30));
        assert_eq!(caret_span(0, 5000), (5000, 5000));
        assert_eq!(caret_span(5000, 0), (0, 0));
        assert_eq!(within(0, 4000, 2000), 0, "an old place before the window is clamped to its start");
        assert_eq!(within(9000, 4000, 2000), 2000);
        assert_eq!(within(4500, 4000, 2000), 500);
    }

    #[test]
    fn one_program_cannot_announce_more_than_twenty_a_second() {
        let mut live = HashMap::new();
        let t = Instant::now();
        let ok = (0..30).filter(|_| rate_allowed(&mut live, "spam", t)).count();
        assert_eq!(ok, LIVE_PER_SECOND);
        assert!(rate_allowed(&mut live, "other", t), "another program is not affected");
        assert!(rate_allowed(&mut live, "spam", t + Duration::from_millis(1100)), "the cap lifts after a second");
    }

    #[test]
    fn the_caret_map_stays_small() {
        let mut c = HashMap::new();
        for i in 0..1000 {
            remember(&mut c, format!("obj{i}"), i);
            assert!(c.len() <= MAX_CARETS);
        }
        assert_eq!(remember(&mut c, "obj999".into(), 5), Some(999));
    }

    #[tokio::test]
    async fn only_the_focused_object_is_followed() {
        let (a, b) = (obj(":1.1", "/a"), obj(":1.1", "/b"));
        let f = Fake { text: "hello".into(), ..Default::default() };
        let tr = tracker_focused(&a);
        assert!(convert(&f, &tr, caret(b.clone(), 2)).await.is_none());
        assert!(convert(&f, &tr, typed(b, Operation::Insert, 0, "x")).await.is_none());
        assert!(matches!(convert(&f, &tr, caret(a, 2)).await, Some(Event::CaretMoved { .. })));
    }

    #[tokio::test]
    async fn caret_and_typing_do_not_say_the_same_thing_twice() {
        let a = obj(":1.1", "/a");
        let f = Fake { text: "hello".into(), ..Default::default() };
        let tr = tracker_focused(&a);
        let Some(Event::TextInserted { before, inserted, .. }) = convert(&f, &tr, typed(a.clone(), Operation::Insert, 4, "o")).await else { panic!("no insert") };
        assert_eq!((before.as_str(), inserted.as_str()), ("hell", "o"));
        let Some(Event::CaretMoved { old, new, .. }) = convert(&f, &tr, caret(a, 5)).await else { panic!("no caret") };
        assert_eq!(old, new, "the move after typing starts where typing ended");
    }

    #[tokio::test]
    async fn typing_in_a_secret_field_reads_nothing() {
        let a = obj(":1.1", "/pw");
        let f = Fake { secret: true, text: "hunter2".into(), ..Default::default() };
        let tr = tracker_focused(&a);
        let Some(Event::TextInserted { before, inserted, .. }) = convert(&f, &tr, typed(a.clone(), Operation::Insert, 6, "2")).await else { panic!("no click") };
        assert!(before.is_empty() && inserted.is_empty());
        assert!(matches!(convert(&f, &tr, typed(a.clone(), Operation::Delete, 6, "2")).await, Some(Event::TextRemoved { removed, .. }) if removed.is_empty()));
        assert!(convert(&f, &tr, caret(a, 3)).await.is_none(), "caret moves in a secret field are silent");
        assert!(f.asked.borrow().is_empty(), "no text was fetched");
    }

    #[tokio::test]
    async fn a_far_jump_asks_for_text_near_the_new_caret_only() {
        let a = obj(":1.1", "/doc");
        let f = Fake { text: "x".repeat(9000), ..Default::default() };
        let tr = tracker_focused(&a);
        convert(&f, &tr, caret(a.clone(), 0)).await;
        let Some(Event::CaretMoved { text, old, new, .. }) = convert(&f, &tr, caret(a, 8999)).await else { panic!("no caret") };
        assert_eq!(*f.asked.borrow().last().unwrap(), (8999, 8999));
        assert!(text.chars().count() <= CARET_CONTEXT + 1001);
        assert_eq!(old, 0, "the far old place is clamped to the start of the window");
        assert!(new > old);
    }

    #[tokio::test]
    async fn only_the_front_program_may_cut_in() {
        let (a, other) = (obj(":1.1", "/a"), obj(":1.2", "/z"));
        let f = Fake::default();
        let tr = tracker_focused(&a);
        assert!(matches!(convert(&f, &tr, announce(a.clone(), Politeness::Assertive)).await, Some(Event::Live { urgent: true, .. })));
        assert!(matches!(convert(&f, &tr, announce(other, Politeness::Assertive)).await, Some(Event::Live { urgent: false, .. })));
        assert!(matches!(convert(&f, &tr, announce(a, Politeness::Polite)).await, Some(Event::Live { urgent: false, .. })));
    }

    #[tokio::test]
    async fn value_changes_are_only_said_for_the_focused_object() {
        let (a, b) = (obj(":1.1", "/a"), obj(":1.1", "/b"));
        let f = Fake::default();
        let tr = tracker_focused(&a);
        let change = |item| Ev::Object(ObjectEvents::PropertyChange(PropertyChangeEvent { item, property: "accessible-name".into(), value: Property::Name("n".into()) }));
        assert!(matches!(convert(&f, &tr, change(a)).await, Some(Event::ValueChanged { .. })));
        assert!(convert(&f, &tr, change(b)).await.is_none());
    }

    #[tokio::test]
    async fn state_changes_are_only_said_for_the_focused_object() {
        let (a, b) = (obj(":1.1", "/a"), obj(":1.2", "/b"));
        let f = Fake::default();
        let tr = tracker_focused(&a);
        let busy = |item| Ev::Object(ObjectEvents::StateChanged(StateChangedEvent { item, state: atspi::State::Busy, enabled: true }));
        assert!(matches!(convert(&f, &tr, busy(a)).await, Some(Event::StateChanged { .. })));
        assert!(convert(&f, &tr, busy(b)).await.is_none(), "a background object stays quiet");
    }

    async fn run_pool(f: Rc<Fake>, evs: Vec<Ev>, idle: Duration, wait: Duration) -> Vec<Event> {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async move {
                let (tx, mut rx) = unbounded_channel();
                let pool = Pipeline::new(f, Rc::new(RefCell::new(Tracker::default())), tx, idle);
                for ev in evs {
                    pool.feed(ev);
                }
                tokio::time::sleep(wait).await;
                let mut got = Vec::new();
                while let Ok(e) = rx.try_recv() {
                    got.push(e);
                }
                got
            })
            .await
    }

    #[tokio::test]
    async fn a_slow_program_does_not_hold_up_another() {
        let f = Rc::new(Fake { slow_app: Some((":1.1", Duration::from_millis(400))), ..Default::default() });
        let evs = vec![focus(obj(":1.1", "/slow")), focus(obj(":1.2", "/fast"))];
        let got = run_pool(f, evs, WORKER_IDLE, Duration::from_millis(100)).await;
        assert_eq!(got.len(), 1, "only the fast program has answered yet");
        assert!(matches!(&got[0], Event::Focus { object } if object.name == "/fast"));
    }

    #[tokio::test]
    async fn events_from_one_program_keep_their_order() {
        let f = Rc::new(Fake { slow_path: Some(("/one", Duration::from_millis(150))), ..Default::default() });
        let evs = vec![focus(obj(":1.1", "/one")), focus(obj(":1.1", "/two")), focus(obj(":1.1", "/three"))];
        let got = run_pool(f.clone(), evs, WORKER_IDLE, Duration::from_millis(500)).await;
        assert_eq!(*f.order.borrow(), vec!["/one", "/two", "/three"], "the slow first event still comes first");
        assert_eq!(got.len(), 1, "only the last focus is still current");
        assert!(matches!(&got[0], Event::Focus { object } if object.name == "/three"));
    }

    #[tokio::test]
    async fn a_slow_programs_late_focus_answer_is_dropped_once_focus_has_moved() {
        let f = Rc::new(Fake { slow_app: Some((":1.1", Duration::from_millis(200))), ..Default::default() });
        let evs = vec![focus(obj(":1.1", "/slow")), focus(obj(":1.2", "/fast"))];
        let got = run_pool(f, evs, WORKER_IDLE, Duration::from_millis(500)).await;
        assert_eq!(got.len(), 1, "the slow program's answer came late and was not said");
        assert!(matches!(&got[0], Event::Focus { object } if object.name == "/fast"));
    }

    #[tokio::test]
    async fn a_flood_from_a_slow_program_is_bounded_but_focus_still_gets_through() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let f = Rc::new(Fake { slow_app: Some((":1.1", Duration::from_millis(300))), ..Default::default() });
                let (tx, _rx) = unbounded_channel();
                let pool = Pipeline::new(f, Rc::new(RefCell::new(Tracker::default())), tx, WORKER_IDLE);
                pool.feed(focus(obj(":1.1", "/a")));
                for i in 0..2000 {
                    pool.feed(caret(obj(":1.1", "/a"), i));
                }
                let pending = pool.workers.borrow()[":1.1"].1.get();
                assert!(pending <= MAX_BACKLOG + 1, "the backlog is capped, got {pending}");
                pool.feed(focus(obj(":1.1", "/b")));
                assert!(pool.workers.borrow()[":1.1"].1.get() > pending, "a focus change is kept even when full");
            })
            .await;
    }

    #[tokio::test]
    async fn an_idle_worker_leaves_and_a_new_one_takes_over() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (tx, mut rx) = unbounded_channel();
                let pool = Pipeline::new(Rc::new(Fake::default()), Rc::new(RefCell::new(Tracker::default())), tx, Duration::from_millis(30));
                pool.feed(focus(obj(":1.1", "/a")));
                tokio::time::sleep(Duration::from_millis(150)).await;
                assert!(rx.try_recv().is_ok());
                assert!(pool.workers.borrow().is_empty(), "the idle worker has gone");
                pool.feed(focus(obj(":1.1", "/b")));
                tokio::time::sleep(Duration::from_millis(60)).await;
                assert!(rx.try_recv().is_ok(), "a program that comes back after its worker left is still heard");
            })
            .await;
    }
}
