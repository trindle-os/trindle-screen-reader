//! Browse mode: a virtual cursor over a window's objects in reading order, jumps by structure, modes.

use crate::model::Role;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub role: Role,
    pub name: String,
    pub text: String,
    pub level: u8,
    pub depth: u8,
    pub count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Heading,
    Link,
    List,
    Table,
    Field,
    Button,
    Graphic,
    Landmark,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Browse,
    Focus,
}

fn is_field(r: Role) -> bool {
    matches!(r, Role::TextEntry | Role::PasswordEntry | Role::CheckBox | Role::RadioButton | Role::Switch | Role::ComboBox | Role::Slider | Role::SpinButton)
}

fn needs_keys(r: Role) -> bool {
    matches!(r, Role::TextEntry | Role::PasswordEntry | Role::ComboBox | Role::Slider | Role::SpinButton)
}

fn matches_kind(r: Role, k: Kind) -> bool {
    match k {
        Kind::Heading => r == Role::Heading,
        Kind::Link => r == Role::Link,
        Kind::List => r == Role::List,
        Kind::Table => r == Role::Table,
        Kind::Field => is_field(r),
        Kind::Button => r == Role::Button,
        Kind::Graphic => r == Role::Image,
        Kind::Landmark => r == Role::Landmark,
    }
}

fn plural(k: Kind) -> &'static str {
    match k {
        Kind::Heading => "headings",
        Kind::Link => "links",
        Kind::List => "lists",
        Kind::Table => "tables",
        Kind::Field => "form fields",
        Kind::Button => "buttons",
        Kind::Graphic => "graphics",
        Kind::Landmark => "landmarks",
    }
}

fn singular(k: Kind) -> &'static str {
    match k {
        Kind::Heading => "heading",
        Kind::Link => "link",
        Kind::List => "list",
        Kind::Table => "table",
        Kind::Field => "form field",
        Kind::Button => "button",
        Kind::Graphic => "graphic",
        Kind::Landmark => "landmark",
    }
}

const MODE_BROWSE: &str = "browse mode";
const MODE_FOCUS: &str = "focus mode";

fn is_stop(n: &Node) -> bool {
    let words = !n.name.is_empty() || !n.text.is_empty();
    match n.role {
        Role::Label | Role::Heading | Role::Link | Role::ListItem | Role::Cell | Role::MenuItem | Role::Tab | Role::Image => words,
        Role::Button | Role::ProgressBar => true,
        r => is_field(r),
    }
}

#[derive(Debug)]
pub struct Browser {
    nodes: Vec<Node>,
    stops: Vec<usize>,
    at: usize,
    mode: Mode,
    forced: bool,
}

impl Browser {
    pub fn new(nodes: Vec<Node>, focus: Option<usize>) -> Browser {
        let stops: Vec<usize> = (0..nodes.len()).filter(|&i| is_stop(&nodes[i])).collect();
        let at = focus.and_then(|f| stops.iter().position(|&s| s >= f)).unwrap_or(0);
        let mode = stops.get(at).map_or(Mode::Browse, |&s| if needs_keys(nodes[s].role) { Mode::Focus } else { Mode::Browse });
        Browser { nodes, stops, at, mode, forced: false }
    }

    pub fn focusable_cursor(&self) -> Option<usize> {
        let i = *self.stops.get(self.at)?;
        let r = self.nodes[i].role;
        (matches!(r, Role::Button | Role::Link | Role::Tab | Role::MenuItem) || is_field(r)).then_some(i)
    }

    pub fn node_at(&self, i: usize) -> &Node {
        &self.nodes[i]
    }

    pub fn is_empty(&self) -> bool {
        self.stops.is_empty()
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    fn node(&self, at: usize) -> &Node {
        &self.nodes[self.stops[at]]
    }

    fn containers(&self, index: usize) -> Vec<usize> {
        let mut chain = Vec::new();
        let mut depth = self.nodes[index].depth;
        for i in (0..index).rev() {
            if depth == 0 {
                break;
            }
            if self.nodes[i].depth < depth {
                depth = self.nodes[i].depth;
                if matches!(self.nodes[i].role, Role::List | Role::Table | Role::Landmark) {
                    chain.push(i);
                }
            }
        }
        chain.reverse();
        chain
    }

    fn say(&self, n: &Node) -> String {
        let body = if n.role == Role::PasswordEntry {
            String::new()
        } else if n.text.is_empty() {
            n.name.clone()
        } else {
            n.text.clone()
        };
        match n.role {
            Role::Heading if n.level > 0 => format!("heading level {}, {body}", n.level),
            Role::Heading => format!("heading, {body}"),
            Role::Link => format!("link, {body}"),
            Role::Label | Role::ListItem | Role::Cell => body,
            r => {
                let role = crate::words::role(r);
                let said = if r == Role::PasswordEntry { n.name.as_str() } else { body.as_str() };
                if said.is_empty() { role.to_string() } else { format!("{said}, {role}") }
            }
        }
    }

    fn arrive(&mut self, from: Option<usize>) -> Vec<String> {
        let here = self.stops[self.at];
        let before = from.map(|f| self.containers(self.stops[f])).unwrap_or_default();
        let mut lines = Vec::new();
        for c in self.containers(here).into_iter().filter(|c| !before.contains(c)) {
            let n = &self.nodes[c];
            lines.push(match n.role {
                Role::Table => format!("table with {} rows", n.count),
                Role::Landmark if n.name.is_empty() => "landmark".to_string(),
                Role::Landmark => format!("{}, landmark", n.name),
                _ => format!("list with {} items", n.count),
            });
        }
        lines.push(self.say(&self.nodes[here]));
        if !self.forced {
            let want = if needs_keys(self.nodes[here].role) { Mode::Focus } else { Mode::Browse };
            if want != self.mode {
                self.mode = want;
                lines.push(if want == Mode::Focus { MODE_FOCUS } else { MODE_BROWSE }.into());
            }
        }
        lines
    }

    pub fn read(&self) -> Vec<String> {
        if self.is_empty() {
            return vec![crate::words::BROWSE_EMPTY.into()];
        }
        vec![self.say(self.node(self.at))]
    }

    pub fn step(&mut self, forward: bool) -> Vec<String> {
        if self.is_empty() {
            return vec![crate::words::BROWSE_EMPTY.into()];
        }
        let next = if forward { self.at + 1 } else { self.at.wrapping_sub(1) };
        if next >= self.stops.len() {
            return vec![if forward { crate::words::END_OF_DOCUMENT } else { crate::words::START_OF_DOCUMENT }.into()];
        }
        let from = self.at;
        self.at = next;
        self.arrive(Some(from))
    }

    pub fn jump(&mut self, kind: Kind, forward: bool) -> Vec<String> {
        if self.is_empty() {
            return vec![crate::words::BROWSE_EMPTY.into()];
        }
        let here = self.stops[self.at];
        let first_stop = |i: usize| self.stops.iter().position(|&s| s >= i);
        let hits: Vec<usize> = if forward {
            (here + 1..self.nodes.len()).filter(|&i| matches_kind(self.nodes[i].role, kind)).collect()
        } else {
            (0..here).rev().filter(|&i| matches_kind(self.nodes[i].role, kind)).collect()
        };
        let target = hits.into_iter().filter_map(first_stop).find(|&p| p != self.at);
        match target {
            Some(p) => {
                let from = self.at;
                self.at = p;
                let entering_container = matches!(kind, Kind::List | Kind::Table | Kind::Landmark);
                self.arrive(if entering_container { None } else { Some(from) })
            }
            None => vec![format!("no {} {}", if forward { "next" } else { "previous" }, singular(kind))],
        }
    }

    pub fn toggle_mode(&mut self) -> Vec<String> {
        if self.forced {
            self.forced = false;
            self.mode = match self.stops.get(self.at) {
                Some(&i) if needs_keys(self.nodes[i].role) => Mode::Focus,
                _ => Mode::Browse,
            };
        } else {
            self.forced = true;
            self.mode = if self.mode == Mode::Browse { Mode::Focus } else { Mode::Browse };
        }
        vec![if self.mode == Mode::Focus { MODE_FOCUS } else { MODE_BROWSE }.into()]
    }

    pub fn elements(&self, kind: Kind) -> Vec<String> {
        const SHOWN: usize = 12;
        let all: Vec<&Node> = self.nodes.iter().filter(|n| matches_kind(n.role, kind)).collect();
        if all.is_empty() {
            return vec![format!("no {}", plural(kind))];
        }
        let names: Vec<String> = all
            .iter()
            .take(SHOWN)
            .map(|n| if n.role == Role::PasswordEntry || (n.name.is_empty() && n.text.is_empty()) { crate::words::UNLABELLED.to_string() } else if n.text.is_empty() { n.name.clone() } else { n.text.clone() })
            .collect();
        let more = if all.len() > SHOWN { format!(", and {} more", all.len() - SHOWN) } else { String::new() };
        let word = if all.len() == 1 { singular(kind) } else { plural(kind) };
        vec![format!("{} {word}: {}{more}", all.len(), names.join(", "))]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggling_mode_with_nothing_to_browse_does_not_panic() {
        let mut b = Browser::new(vec![], None);
        b.toggle_mode();
        b.toggle_mode();
        b.toggle_mode();
        assert_eq!(b.mode(), Mode::Focus);
    }

    fn n(role: Role, name: &str, depth: u8) -> Node {
        Node { role, name: name.into(), text: String::new(), level: 0, depth, count: 0 }
    }

    fn page() -> Vec<Node> {
        let mut h = n(Role::Heading, "Welcome", 1);
        h.level = 1;
        let mut list = n(Role::List, "", 1);
        list.count = 2;
        let mut h2 = n(Role::Heading, "More", 1);
        h2.level = 2;
        vec![
            n(Role::Window, "Page", 0),
            h,
            n(Role::Label, "Some words", 1),
            n(Role::Link, "Home", 1),
            list,
            n(Role::ListItem, "one", 2),
            n(Role::ListItem, "two", 2),
            h2,
            n(Role::TextEntry, "Name", 1),
            n(Role::Button, "Send", 1),
        ]
    }

    #[test]
    fn stepping_reads_in_order_and_stops_at_the_ends() {
        let mut b = Browser::new(page(), None);
        assert_eq!(b.read(), vec!["heading level 1, Welcome"]);
        assert_eq!(b.step(false), vec!["Start of document"]);
        assert_eq!(b.step(true), vec!["Some words"]);
        assert_eq!(b.step(true), vec!["link, Home"]);
    }

    #[test]
    fn entering_a_list_says_its_size_once() {
        let mut b = Browser::new(page(), None);
        b.step(true);
        b.step(true);
        assert_eq!(b.step(true), vec!["list with 2 items", "one"]);
        assert_eq!(b.step(true), vec!["two"]);
    }

    #[test]
    fn jumps_find_the_next_and_previous_of_a_kind_and_say_when_there_is_none() {
        let mut b = Browser::new(page(), None);
        assert_eq!(b.jump(Kind::Heading, true), vec!["heading level 2, More"]);
        assert_eq!(b.jump(Kind::Heading, true), vec!["no next heading"]);
        assert_eq!(b.jump(Kind::Heading, false), vec!["heading level 1, Welcome"]);
        assert_eq!(b.jump(Kind::Link, true), vec!["link, Home"]);
        assert_eq!(b.jump(Kind::Table, true), vec!["no next table"]);
    }

    #[test]
    fn jumping_to_a_list_reads_its_size_and_first_item() {
        let mut b = Browser::new(page(), None);
        assert_eq!(b.jump(Kind::List, true), vec!["list with 2 items", "one"]);
    }

    #[test]
    fn landing_in_a_field_switches_to_focus_mode_and_back() {
        let mut b = Browser::new(page(), None);
        assert_eq!(b.jump(Kind::Field, true), vec!["Name, text entry", "focus mode"]);
        assert_eq!(b.mode(), Mode::Focus);
        assert_eq!(b.step(true), vec!["Send, button", "browse mode"]);
    }

    #[test]
    fn a_chosen_mode_sticks_until_chosen_again() {
        let mut b = Browser::new(page(), None);
        assert_eq!(b.toggle_mode(), vec!["focus mode"]);
        assert_eq!(b.step(true), vec!["Some words"]);
        assert_eq!(b.mode(), Mode::Focus);
        assert_eq!(b.toggle_mode(), vec!["browse mode"]);
    }

    #[test]
    fn a_password_is_never_read() {
        let mut p = n(Role::PasswordEntry, "Password", 1);
        p.text = "hunter2".into();
        let b = Browser::new(vec![n(Role::Window, "Login", 0), p], None);
        let said = b.read().join(" ");
        assert!(!said.contains("hunter2"));
        assert_eq!(said, "Password, password entry");
        assert!(!b.elements(Kind::Field).join(" ").contains("hunter2"));
    }

    #[test]
    fn element_lists_count_and_name_things() {
        let b = Browser::new(page(), None);
        assert_eq!(b.elements(Kind::Heading), vec!["2 headings: Welcome, More"]);
        assert_eq!(b.elements(Kind::Table), vec!["no tables"]);
        assert_eq!(b.elements(Kind::Button), vec!["1 button: Send"]);
        let many: Vec<Node> = (0..15).map(|i| n(Role::Link, &format!("l{i}"), 0)).collect();
        assert!(Browser::new(many, None).elements(Kind::Link)[0].ends_with(", and 3 more"));
    }

    #[test]
    fn the_cursor_starts_at_the_focus_and_an_empty_page_says_so() {
        let b = Browser::new(page(), Some(9));
        assert_eq!(b.read(), vec!["Send, button"]);
        let mut e = Browser::new(vec![n(Role::Window, "Empty", 0)], None);
        assert_eq!(e.read(), vec!["nothing to browse"]);
        assert_eq!(e.step(true), vec!["nothing to browse"]);
        assert_eq!(e.jump(Kind::Link, true), vec!["nothing to browse"]);
    }

    #[test]
    fn a_landmark_is_announced_on_entering_and_jumped_to() {
        let mut region = n(Role::Landmark, "navigation", 1);
        region.count = 0;
        let nodes = vec![n(Role::Window, "W", 0), n(Role::Label, "Top", 1), region, n(Role::Link, "Home", 2), n(Role::Link, "About", 2), n(Role::Landmark, "", 1), n(Role::Label, "Footer", 2)];
        let mut b = Browser::new(nodes, None);
        assert_eq!(b.jump(Kind::Landmark, true), vec!["navigation, landmark", "link, Home"]);
        assert_eq!(b.step(true), vec!["link, About"]);
        assert_eq!(b.step(true), vec!["landmark", "Footer"]);
        assert_eq!(b.jump(Kind::Landmark, false), vec!["navigation, landmark", "link, Home"]);
        assert_eq!(b.elements(Kind::Landmark), vec!["2 landmarks: navigation, unlabelled"]);
    }

    #[test]
    fn only_controls_ask_the_program_to_move_its_focus() {
        let mut b = Browser::new(page(), None);
        assert_eq!(b.focusable_cursor(), None);
        b.jump(Kind::Link, true);
        assert_eq!(b.focusable_cursor(), Some(3));
        b.jump(Kind::Field, true);
        assert_eq!(b.focusable_cursor(), Some(8));
    }

    #[test]
    fn a_heading_with_text_in_its_text_not_its_name_is_read() {
        let mut h = n(Role::Heading, "", 1);
        h.text = "From the text".into();
        h.level = 3;
        let b = Browser::new(vec![n(Role::Window, "W", 0), h], None);
        assert_eq!(b.read(), vec!["heading level 3, From the text"]);
    }
}
