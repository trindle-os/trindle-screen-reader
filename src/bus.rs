//! The only part that talks to the accessibility bus; turns bus events into plain `Event` values.

use crate::browse::Node;
use crate::model::{Container, Event, ObjectInfo, Role, State as St};
use crate::review::ReviewItem;
use crate::track::{key_of, state_of, Facts, Pipeline, Tracker, CARET_CONTEXT, WORKER_IDLE};
use atspi::events::object::{AnnouncementEvent, PropertyChangeEvent, StateChangedEvent, TextCaretMovedEvent, TextChangedEvent};
use atspi::events::window::{ActivateEvent, CloseEvent};
use atspi::proxy::accessible::{AccessibleProxy, ObjectRefExt};
use atspi::proxy::component::ComponentProxy;
use atspi::proxy::text::TextProxy;
use atspi::proxy::value::ValueProxy;
use atspi::{CoordType, ObjectRefOwned, Role as AR, State as AS};
use atspi_connection::AccessibilityConnection;
use futures_lite::StreamExt;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::time::{Duration, Instant};
use tokio::time::timeout;

pub const ASK_LIMIT: Duration = Duration::from_millis(250);
const SLOW_FOR: Duration = Duration::from_secs(5);
const MAX_TEXT: i32 = 1000;
const MAX_ALL: i32 = 20_000;
const MAX_ITEMS: usize = 500;
const MAX_NODES: usize = 1500;
const MAX_DEPTH: u8 = 12;
const GATHER_LIMIT: Duration = Duration::from_secs(1);
const PLACE_LIMIT: Duration = Duration::from_millis(300);

fn role_of(r: AR) -> Role {
    match r {
        AR::Button => Role::Button,
        AR::CheckBox | AR::CheckMenuItem => Role::CheckBox,
        AR::RadioButton | AR::RadioMenuItem => Role::RadioButton,
        AR::ToggleButton => Role::Switch,
        AR::Entry | AR::Text => Role::TextEntry,
        AR::PasswordText => Role::PasswordEntry,
        AR::Label | AR::Static => Role::Label,
        AR::Heading => Role::Heading,
        AR::Link => Role::Link,
        AR::List | AR::ListBox => Role::List,
        AR::ListItem => Role::ListItem,
        AR::Table => Role::Table,
        AR::TableRow => Role::Row,
        AR::TableCell => Role::Cell,
        AR::Menu | AR::MenuBar => Role::Menu,
        AR::MenuItem => Role::MenuItem,
        AR::PageTab => Role::Tab,
        AR::PageTabList => Role::TabList,
        AR::Slider => Role::Slider,
        AR::SpinButton => Role::SpinButton,
        AR::ProgressBar => Role::ProgressBar,
        AR::Image | AR::Icon => Role::Image,
        AR::Separator => Role::Separator,
        AR::ToolBar => Role::ToolBar,
        AR::StatusBar => Role::StatusBar,
        AR::Panel | AR::Filler | AR::Grouping => Role::Group,
        AR::ComboBox => Role::ComboBox,
        AR::DocumentFrame | AR::DocumentWeb => Role::Document,
        AR::Landmark => Role::Landmark,
        AR::Frame | AR::Window => Role::Window,
        AR::Dialog | AR::FileChooser => Role::Dialog,
        AR::Alert => Role::Alert,
        _ => Role::Unknown,
    }
}

fn announced(r: Role) -> bool {
    matches!(r, Role::Group | Role::List | Role::ToolBar | Role::TabList | Role::Table | Role::Menu)
}

fn has_position(r: Role) -> bool {
    matches!(r, Role::ListItem | Role::Tab | Role::MenuItem | Role::RadioButton | Role::Row)
}

fn is_top(r: &ObjectRefOwned) -> bool {
    r.is_null() || r.path_as_str().ends_with("/root")
}

async fn ask<T>(f: impl std::future::Future<Output = zbus::Result<T>>) -> Option<T> {
    timeout(ASK_LIMIT, f).await.ok()?.ok()
}

macro_rules! proxy {
    ($ty:ty, $conn:expr, $r:expr) => {
        async {
            let b = <$ty>::builder($conn).destination($r.name_as_str()?).ok()?.path($r.path_as_str()).ok()?;
            timeout(ASK_LIMIT, b.build()).await.ok()?.ok()
        }
    };
}

fn slow_now(slow: &mut HashMap<String, Instant>, app: &str, now: Instant) -> bool {
    match slow.get(app) {
        Some(until) if *until > now => true,
        Some(_) => {
            slow.remove(app);
            false
        }
        None => false,
    }
}

pub type Doc = (Vec<Node>, Vec<ObjectRefOwned>, Option<usize>);

pub struct Bus {
    conn: AccessibilityConnection,
    tracker: Rc<RefCell<Tracker>>,
    slow: RefCell<HashMap<String, Instant>>,
}

impl Bus {
    pub async fn connect() -> Result<Self, String> {
        let conn = match std::env::var("AT_SPI_BUS_ADDRESS").ok().filter(|a| !a.is_empty()) {
            Some(a) => AccessibilityConnection::from_address(a.parse().map_err(|e: zbus::Error| e.to_string())?).await,
            None => AccessibilityConnection::new().await,
        }
        .map_err(|e| e.to_string())?;
        if let Err(e) = atspi_connection::set_session_accessibility(true).await {
            crate::log::note(&format!("could not set the accessibility switch: {e}"));
        }
        conn.register_event::<StateChangedEvent>().await.map_err(|e| e.to_string())?;
        conn.register_event::<ActivateEvent>().await.map_err(|e| e.to_string())?;
        conn.register_event::<CloseEvent>().await.map_err(|e| e.to_string())?;
        conn.register_event::<TextCaretMovedEvent>().await.map_err(|e| e.to_string())?;
        conn.register_event::<TextChangedEvent>().await.map_err(|e| e.to_string())?;
        conn.register_event::<PropertyChangeEvent>().await.map_err(|e| e.to_string())?;
        conn.register_event::<AnnouncementEvent>().await.map_err(|e| e.to_string())?;
        Ok(Bus { conn, tracker: Rc::new(RefCell::new(Tracker::default())), slow: RefCell::new(HashMap::new()) })
    }

    pub async fn finish(&self) {
        let _ = atspi_connection::set_session_accessibility(false).await;
    }

    async fn ask_in<T>(&self, app: &str, f: impl std::future::Future<Output = zbus::Result<T>>) -> Option<T> {
        match timeout(ASK_LIMIT, f).await {
            Ok(r) => r.ok(),
            Err(_) => {
                self.slow.borrow_mut().insert(app.to_string(), Instant::now() + SLOW_FOR);
                None
            }
        }
    }

    fn is_slow(&self, app: &str) -> bool {
        slow_now(&mut self.slow.borrow_mut(), app, Instant::now())
    }

    async fn accessible<'a>(&'a self, r: &'a ObjectRefOwned) -> Option<AccessibleProxy<'a>> {
        timeout(ASK_LIMIT, r.as_accessible_proxy(self.conn.connection())).await.ok()?.ok()
    }

    pub fn events(self: Rc<Self>) -> tokio::sync::mpsc::UnboundedReceiver<Event> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let pipeline = Pipeline::new(self.clone(), self.tracker.clone(), tx.clone(), WORKER_IDLE);
        tokio::task::spawn_local(async move {
            let stream = self.conn.event_stream();
            let mut stream = std::pin::pin!(stream);
            while let Some(item) = stream.next().await {
                match item {
                    Ok(ev) => {
                        pipeline.feed(ev);
                        if tx.is_closed() {
                            break;
                        }
                    }
                    Err(e) => crate::log::note(&format!("bus event error: {e}")),
                }
            }
        });
        rx
    }

    async fn value_of(&self, r: &ObjectRefOwned, role: Role) -> Option<String> {
        match role {
            Role::TextEntry | Role::ComboBox => {
                let t = proxy!(TextProxy, self.conn.connection(), r).await?;
                let n = ask(t.character_count()).await.unwrap_or(0).min(MAX_TEXT);
                Some(ask(t.get_text(0, n)).await.unwrap_or_default())
            }
            Role::Slider | Role::SpinButton | Role::ProgressBar => {
                let v = proxy!(ValueProxy, self.conn.connection(), r).await?;
                let x = ask(v.current_value()).await?;
                Some(format!("{}", (x * 100.0).round() / 100.0))
            }
            _ => None,
        }
    }

    async fn position(&self, acc: &AccessibleProxy<'_>) -> Option<(u32, u32)> {
        let index = ask(acc.get_index_in_parent()).await?;
        let parent = ask(acc.parent()).await?;
        if is_top(&parent) {
            return None;
        }
        let p = self.accessible(&parent).await?;
        let total = ask(p.child_count()).await?;
        (index >= 0 && total > 0 && index < total).then_some((index as u32 + 1, total as u32))
    }

    async fn place(&self, acc: &AccessibleProxy<'_>, app: &str) -> (Option<Container>, String) {
        let deadline = Instant::now() + PLACE_LIMIT;
        let mut found = None;
        let Some(mut cur) = self.ask_in(app, acc.parent()).await else { return (None, String::new()) };
        for _ in 0..12 {
            if is_top(&cur) {
                break;
            }
            if Instant::now() >= deadline {
                self.slow.borrow_mut().insert(app.to_string(), Instant::now() + SLOW_FOR);
                break;
            }
            let Some(p) = self.accessible(&cur).await else { break };
            let Some(raw) = self.ask_in(app, p.get_role()).await else { break };
            let role = role_of(raw);
            if matches!(role, Role::Window | Role::Dialog) {
                return (found, self.ask_in(app, p.name()).await.unwrap_or_default());
            }
            if found.is_none() && announced(role) {
                let name = self.ask_in(app, p.name()).await.unwrap_or_default();
                if !name.trim().is_empty() || role != Role::Group {
                    found = Some(Container { id: key_of(&cur), role, name });
                }
            }
            let Some(up) = self.ask_in(app, p.parent()).await else { break };
            cur = up;
        }
        (found, String::new())
    }

    pub async fn window_items(&self) -> Option<(Vec<ReviewItem>, Option<u64>)> {
        let deadline = Instant::now() + GATHER_LIMIT;
        let focus = self.tracker.borrow().focused_ref.clone()?;
        let window = self.window_of(&focus).await?;
        let focus_key = key_of(&focus);
        let mut items = Vec::new();
        let mut focus_id = None;
        let mut seen = HashSet::new();
        let mut queue = VecDeque::from([(window, 0u8)]);
        while let Some((r, depth)) = queue.pop_front() {
            if items.len() >= MAX_ITEMS || seen.len() >= MAX_NODES || Instant::now() >= deadline {
                break;
            }
            if !seen.insert(key_of(&r)) {
                continue;
            }
            let Some(acc) = self.accessible(&r).await else { continue };
            let Some(set) = ask(acc.get_state()).await else { continue };
            if !set.contains(AS::Showing) {
                continue;
            }
            if let Some(item) = self.review_item(&r, &acc, items.len() as u64).await {
                if key_of(&r) == focus_key {
                    focus_id = Some(item.id);
                }
                items.push(item);
            }
            if depth < MAX_DEPTH {
                self.queue_children(&acc, depth + 1, deadline, &mut queue).await;
            }
        }
        Some((items, focus_id))
    }

    pub async fn document_nodes(&self) -> Option<Doc> {
        let deadline = Instant::now() + GATHER_LIMIT;
        let focus = self.tracker.borrow().focused_ref.clone()?;
        let window = self.window_of(&focus).await?;
        let focus_key = key_of(&focus);
        let mut nodes = Vec::new();
        let mut refs = Vec::new();
        let mut focus_at = None;
        let mut seen = HashSet::new();
        let mut stack = vec![(window, 0u8)];
        while let Some((r, depth)) = stack.pop() {
            if nodes.len() >= MAX_NODES || seen.len() >= MAX_NODES || Instant::now() >= deadline {
                break;
            }
            if !seen.insert(key_of(&r)) {
                continue;
            }
            let Some(acc) = self.accessible(&r).await else { continue };
            let Some(set) = ask(acc.get_state()).await else { continue };
            if !set.contains(AS::Showing) {
                continue;
            }
            if key_of(&r) == focus_key {
                focus_at = Some(nodes.len());
            }
            nodes.push(self.document_node(&r, &acc, depth).await);
            refs.push(r.clone());
            if depth < MAX_DEPTH {
                let mut kids = VecDeque::new();
                self.queue_children(&acc, depth + 1, deadline, &mut kids).await;
                stack.extend(kids.into_iter().rev());
            }
        }
        Some((nodes, refs, focus_at))
    }

    pub async fn grab_focus(&self, r: &ObjectRefOwned) -> bool {
        let Some(c) = proxy!(ComponentProxy, self.conn.connection(), r).await else { return false };
        ask(c.grab_focus()).await.unwrap_or(false)
    }

    async fn document_node(&self, r: &ObjectRefOwned, acc: &AccessibleProxy<'_>, depth: u8) -> Node {
        let role = role_of(ask(acc.get_role()).await.unwrap_or(AR::Unknown));
        let secret = role == Role::PasswordEntry;
        let name = ask(acc.name()).await.unwrap_or_default();
        let text = if secret { String::new() } else { self.review_text(r, role).await };
        let level = if role == Role::Heading {
            ask(acc.get_attributes()).await.and_then(|a| a.get("level").and_then(|l| l.parse().ok())).unwrap_or(0)
        } else {
            0
        };
        let count = if matches!(role, Role::List | Role::Table) { ask(acc.child_count()).await.unwrap_or(0).max(0) as u32 } else { 0 };
        Node { role, name, text, level, depth, count }
    }

    async fn window_of(&self, focus: &ObjectRefOwned) -> Option<ObjectRefOwned> {
        let mut cur = focus.clone();
        for _ in 0..24 {
            let p = self.accessible(&cur).await?;
            if matches!(role_of(ask(p.get_role()).await?), Role::Window | Role::Dialog) {
                return Some(cur);
            }
            let up = ask(p.parent()).await?;
            if is_top(&up) {
                return None;
            }
            cur = up;
        }
        None
    }

    async fn review_item(&self, r: &ObjectRefOwned, acc: &AccessibleProxy<'_>, id: u64) -> Option<ReviewItem> {
        let role = role_of(ask(acc.get_role()).await.unwrap_or(AR::Unknown));
        let leaf = !matches!(role, Role::Window | Role::Dialog | Role::Group | Role::List | Role::Table | Role::Row | Role::Menu | Role::ToolBar | Role::TabList | Role::Landmark | Role::Unknown);
        if !leaf {
            return None;
        }
        let c = proxy!(ComponentProxy, self.conn.connection(), r).await?;
        let (x, y, w, h) = ask(c.get_extents(CoordType::Screen)).await?;
        if w <= 0 || h <= 0 {
            return None;
        }
        let secret = role == Role::PasswordEntry;
        let name = ask(acc.name()).await.unwrap_or_default();
        let text = if secret { String::new() } else { self.review_text(r, role).await };
        Some(ReviewItem { id, role, name, text, secret, x, y, w, h })
    }

    async fn review_text(&self, r: &ObjectRefOwned, role: Role) -> String {
        if !matches!(role, Role::Label | Role::Heading | Role::Link | Role::Cell | Role::ListItem | Role::TextEntry) {
            return String::new();
        }
        let Some(t) = proxy!(TextProxy, self.conn.connection(), r).await else { return String::new() };
        let n = ask(t.character_count()).await.unwrap_or(0).min(MAX_TEXT);
        ask(t.get_text(0, n)).await.unwrap_or_default()
    }

    async fn queue_children(&self, acc: &AccessibleProxy<'_>, depth: u8, deadline: Instant, queue: &mut VecDeque<(ObjectRefOwned, u8)>) {
        let n = ask(acc.child_count()).await.unwrap_or(0).clamp(0, 200);
        for i in 0..n {
            if Instant::now() >= deadline {
                break;
            }
            if let Some(c) = ask(acc.get_child_at_index(i)).await {
                if !c.is_null() {
                    queue.push_back((c, depth));
                }
            }
        }
    }

    pub async fn focused_text(&self) -> Option<(String, bool)> {
        let r = self.tracker.borrow().focused_ref.clone()?;
        if self.light(&r).await?.is_secret() {
            return None;
        }
        let t = proxy!(TextProxy, self.conn.connection(), r).await?;
        let count = ask(t.character_count()).await?;
        let caret = ask(t.caret_offset()).await.unwrap_or(0);
        let (from, to) = say_all_window(count, caret);
        let text = ask(t.get_text(from, to)).await?;
        Some((text, to < count))
    }
}

fn say_all_window(count: i32, caret: i32) -> (i32, i32) {
    let from = caret.clamp(0, count.max(0));
    (from, count.max(0).min(from.saturating_add(MAX_ALL)))
}

impl Facts for Bus {
    async fn appeared(&self, r: &ObjectRefOwned) -> Option<Event> {
        let app = r.name_as_str()?;
        if self.is_slow(app) {
            return None;
        }
        let acc = self.accessible(r).await?;
        match self.ask_in(app, acc.get_role()).await? {
            AR::Alert => {
                let object = self.describe(r).await?;
                Some(Event::Alert { text: object.name.clone(), object })
            }
            AR::Notification => {
                let name = ask(acc.name()).await.unwrap_or_default();
                let description = ask(acc.description()).await.unwrap_or_default();
                let mut app_name = String::new();
                if let Some(a) = ask(acc.get_application()).await.filter(|a| !a.is_null()) {
                    if let Some(p) = self.accessible(&a).await {
                        app_name = ask(p.name()).await.unwrap_or_default();
                    }
                }
                let text = if description.is_empty() { name } else { format!("{name}. {description}") };
                Some(Event::Notification { app: app_name, text, urgent: false })
            }
            _ => None,
        }
    }

    async fn text_range(&self, r: &ObjectRefOwned, from: i32, to: i32) -> Option<String> {
        let t = proxy!(TextProxy, self.conn.connection(), r).await?;
        ask(t.get_text(from, to)).await
    }

    async fn text_around(&self, r: &ObjectRefOwned, lo: usize, hi: usize) -> Option<(String, usize)> {
        let t = proxy!(TextProxy, self.conn.connection(), r).await?;
        let count = ask(t.character_count()).await?.max(0) as usize;
        let start = lo.saturating_sub(CARET_CONTEXT).min(count);
        let end = (hi + CARET_CONTEXT).min(count);
        let text = ask(t.get_text(start as i32, end as i32)).await?;
        Some((text, start))
    }

    async fn light(&self, r: &ObjectRefOwned) -> Option<ObjectInfo> {
        let acc = self.accessible(r).await?;
        let role = role_of(ask(acc.get_role()).await?);
        let mut states = Vec::new();
        if role == Role::PasswordEntry {
            states.push(St::Secret);
        }
        Some(ObjectInfo { role: Some(role), states, ..Default::default() })
    }

    async fn describe(&self, r: &ObjectRefOwned) -> Option<ObjectInfo> {
        let app = r.name_as_str()?.to_string();
        let acc = self.accessible(r).await?;
        let (role, name, set) = tokio::join!(self.ask_in(&app, acc.get_role()), self.ask_in(&app, acc.name()), self.ask_in(&app, acc.get_state()));
        let role = role_of(role?);
        let mut states = Vec::new();
        let mut disabled = false;
        if let Some(set) = &set {
            for s in set.iter() {
                if let Some(x) = state_of(s) {
                    states.push(x);
                }
            }
            disabled = !set.contains(AS::Enabled) && !set.contains(AS::Sensitive);
            if !set.contains(AS::Editable) && matches!(role, Role::TextEntry) {
                states.push(St::ReadOnly);
            }
        }
        if disabled {
            states.push(St::Disabled);
        }
        if role == Role::PasswordEntry {
            states.push(St::Secret);
        }
        let secret = states.contains(&St::Secret);
        let mut info = ObjectInfo { role: Some(role), name: name.unwrap_or_default(), states, ..Default::default() };
        if self.is_slow(&app) {
            return Some(info);
        }
        let is_window = matches!(role, Role::Window | Role::Dialog);
        let (description, value, place, pos) = tokio::join!(
            async { if secret { None } else { ask(acc.description()).await } },
            async { if secret { None } else { self.value_of(r, role).await } },
            async { if is_window { (None, String::new()) } else { self.place(&acc, &app).await } },
            async { if has_position(role) { self.position(&acc).await } else { None } },
        );
        info.description = description.unwrap_or_default();
        info.value = value;
        (info.container, info.window) = place;
        info.position = pos;
        Some(info)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn say_all_reads_from_the_caret_even_past_the_old_limit() {
        assert_eq!(say_all_window(30_000, 25_000), (25_000, 30_000), "a start beyond 20,000 characters still reads");
        assert_eq!(say_all_window(100_000, 0), (0, MAX_ALL), "a long document is cut and the caller is told");
        assert_eq!(say_all_window(10, 99), (10, 10));
        assert_eq!(say_all_window(10, -5), (0, 10));
        assert_eq!(say_all_window(-1, 0), (0, 0));
    }

    use super::*;

    #[test]
    fn roles_map_and_unknown_ones_are_quiet() {
        assert_eq!(role_of(AR::Button), Role::Button);
        assert_eq!(role_of(AR::PasswordText), Role::PasswordEntry);
        assert_eq!(role_of(AR::Frame), Role::Window);
        assert_eq!(role_of(AR::FileChooser), Role::Dialog);
        assert_eq!(role_of(AR::Notification), Role::Unknown);
        assert!(has_position(Role::ListItem) && !has_position(Role::Button));
        assert!(announced(Role::List) && !announced(Role::Label));
    }

    #[test]
    fn a_slow_program_is_marked_and_forgiven() {
        let mut slow = HashMap::new();
        let t = Instant::now();
        assert!(!slow_now(&mut slow, "app", t));
        slow.insert("app".to_string(), t + SLOW_FOR);
        assert!(slow_now(&mut slow, "app", t + Duration::from_secs(1)));
        assert!(!slow_now(&mut slow, "app", t + SLOW_FOR + Duration::from_millis(1)));
        assert!(slow.is_empty());
    }
}
