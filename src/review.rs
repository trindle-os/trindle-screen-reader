//! Flat review: the window as lines and a cursor that moves without touching focus; secrets are only named.

use crate::caret::char_name;
use crate::model::Role;
use crate::words;

#[derive(Clone, Debug)]
pub struct ReviewItem {
    pub id: u64,
    pub role: Role,
    pub name: String,
    pub text: String,
    pub secret: bool,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Clone, Debug)]
struct Unit {
    item: usize,
    spoken: String,
}

#[derive(Clone, Debug, Default)]
struct Line {
    units: Vec<Unit>,
}

impl Line {
    fn text(&self) -> String {
        let mut s = String::new();
        for (i, u) in self.units.iter().enumerate() {
            if i > 0 {
                s.push_str(if self.units[i - 1].item == u.item { " " } else { ", " });
            }
            s.push_str(&u.spoken);
        }
        s
    }
}

#[derive(Debug)]
pub struct Review {
    items: Vec<ReviewItem>,
    lines: Vec<Line>,
    line: usize,
    unit: usize,
    ch: usize,
}

fn is_text_role(r: Role) -> bool {
    matches!(r, Role::Label | Role::Heading | Role::Link | Role::Cell | Role::ListItem | Role::Document)
}

fn with_role(body: &str, r: Role) -> String {
    match (body.is_empty(), words::role(r)) {
        (true, "") => String::new(),
        (true, role) => format!("{}, {role}", words::UNLABELLED),
        (false, "") => body.to_string(),
        (false, role) => format!("{body}, {role}"),
    }
}

fn units_for(idx: usize, it: &ReviewItem) -> Vec<Unit> {
    if it.secret {
        let name = if it.name.is_empty() { String::new() } else { format!("{}, ", it.name) };
        return vec![Unit { item: idx, spoken: format!("{name}{}", words::role(it.role)) }];
    }
    let body = if it.text.is_empty() { &it.name } else { &it.text };
    if is_text_role(it.role) && !body.is_empty() {
        return body.split_whitespace().map(|w| Unit { item: idx, spoken: w.to_string() }).collect();
    }
    vec![Unit { item: idx, spoken: with_role(body, it.role) }]
}

impl Review {
    pub fn build(mut items: Vec<ReviewItem>, hide_unlabelled: bool) -> Review {
        if hide_unlabelled {
            items.retain(|i| !(i.name.is_empty() && i.text.is_empty()));
        }
        let mut order: Vec<usize> = (0..items.len()).collect();
        let centre_of = |it: &ReviewItem| it.y as i64 + it.h as i64 / 2;
        order.sort_by_key(|&i| (centre_of(&items[i]), items[i].x));
        let mut rows: Vec<(i64, i64, Vec<usize>)> = Vec::new();
        for i in order {
            let it = &items[i];
            let (top, bot) = (it.y as i64, it.y as i64 + (it.h as i64).max(1));
            let centre = centre_of(it);
            match rows.last_mut() {
                Some(r) if centre >= r.0 && centre < r.1 => {
                    r.0 = r.0.min(top);
                    r.1 = r.1.max(bot);
                    r.2.push(i);
                }
                _ => rows.push((top, bot, vec![i])),
            }
        }
        let lines = rows
            .into_iter()
            .map(|(_, _, mut m)| {
                m.sort_by_key(|&i| items[i].x);
                Line { units: m.into_iter().flat_map(|i| units_for(i, &items[i])).collect() }
            })
            .collect();
        Review { items, lines, line: 0, unit: 0, ch: 0 }
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    fn cur_unit(&self) -> Option<&Unit> {
        self.lines.get(self.line)?.units.get(self.unit)
    }

    fn here(&self) -> String {
        self.cur_unit().map(|u| u.spoken.clone()).unwrap_or_else(|| words::REVIEW_EMPTY.into())
    }

    fn set(&mut self, line: usize, unit: usize) {
        self.line = line;
        self.unit = unit;
        self.ch = 0;
    }

    pub fn enter(&mut self, focus_id: u64) -> Option<String> {
        for (l, line) in self.lines.iter().enumerate() {
            if let Some(u) = line.units.iter().position(|u| self.items[u.item].id == focus_id) {
                self.set(l, u);
                return Some(self.here());
            }
        }
        None
    }

    pub fn read_line(&self) -> String {
        self.lines.get(self.line).map(Line::text).unwrap_or_else(|| words::REVIEW_EMPTY.into())
    }

    fn move_line(&mut self, to: Option<usize>, edge: &'static str) -> String {
        match to {
            Some(l) if l < self.lines.len() => {
                let u = self.unit.min(self.lines[l].units.len().saturating_sub(1));
                self.set(l, u);
                self.read_line()
            }
            _ if self.lines.is_empty() => words::REVIEW_EMPTY.into(),
            _ => edge.into(),
        }
    }

    pub fn next_line(&mut self) -> String {
        self.move_line(self.line.checked_add(1), words::REVIEW_BOTTOM)
    }

    pub fn prev_line(&mut self) -> String {
        self.move_line(self.line.checked_sub(1), words::REVIEW_TOP)
    }

    pub fn first_line(&mut self) -> String {
        self.unit = 0;
        self.move_line(Some(0), words::REVIEW_TOP)
    }

    pub fn last_line(&mut self) -> String {
        let l = self.lines.len().saturating_sub(1);
        self.move_line(Some(l), words::REVIEW_BOTTOM)
    }

    pub fn top_left(&mut self) -> String {
        self.set(0, 0);
        self.here()
    }

    pub fn bottom_right(&mut self) -> String {
        let l = self.lines.len().saturating_sub(1);
        let u = self.lines.get(l).map_or(0, |x| x.units.len().saturating_sub(1));
        self.set(l, u);
        self.here()
    }

    pub fn start_of_line(&mut self) -> String {
        self.set(self.line, 0);
        self.here()
    }

    pub fn end_of_line(&mut self) -> String {
        let u = self.lines.get(self.line).map_or(0, |x| x.units.len().saturating_sub(1));
        self.set(self.line, u);
        self.here()
    }

    pub fn read_word(&self) -> String {
        self.here()
    }

    pub fn next_word(&mut self) -> String {
        let Some(line) = self.lines.get(self.line) else { return words::REVIEW_EMPTY.into() };
        if self.unit + 1 < line.units.len() {
            self.set(self.line, self.unit + 1);
        } else if self.line + 1 < self.lines.len() {
            self.set(self.line + 1, 0);
        } else {
            return words::REVIEW_BOTTOM.into();
        }
        self.here()
    }

    pub fn prev_word(&mut self) -> String {
        if self.unit > 0 {
            self.set(self.line, self.unit - 1);
        } else if self.line > 0 {
            let u = self.lines[self.line - 1].units.len().saturating_sub(1);
            self.set(self.line - 1, u);
        } else {
            return words::REVIEW_TOP.into();
        }
        self.here()
    }

    pub fn read_char(&self) -> String {
        match self.cur_unit() {
            Some(u) if self.items[u.item].secret => words::REVIEW_PROTECTED.into(),
            Some(u) => u.spoken.chars().nth(self.ch).map(|c| char_name(&c.to_string())).unwrap_or_default(),
            None => words::REVIEW_EMPTY.into(),
        }
    }

    pub fn next_char(&mut self) -> String {
        let Some(u) = self.cur_unit() else { return words::REVIEW_EMPTY.into() };
        let secret = self.items[u.item].secret;
        if !secret && self.ch + 1 < u.spoken.chars().count() {
            self.ch += 1;
            return self.read_char();
        }
        let moved = self.next_word();
        if moved == words::REVIEW_BOTTOM { moved } else { self.read_char() }
    }

    pub fn prev_char(&mut self) -> String {
        if self.ch > 0 {
            self.ch -= 1;
            return self.read_char();
        }
        let moved = self.prev_word();
        if moved == words::REVIEW_TOP {
            return moved;
        }
        self.ch = self.cur_unit().map_or(0, |u| u.spoken.chars().count().saturating_sub(1));
        self.read_char()
    }

    pub fn read_from_cursor(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (l, ln) in self.lines.iter().enumerate().skip(self.line) {
            if l == self.line && self.unit > 0 {
                let rest = Line { units: ln.units[self.unit..].to_vec() };
                out.push(rest.text());
            } else {
                out.push(ln.text());
            }
        }
        out
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    fn it(id: u64, role: Role, name: &str, text: &str, x: i32, y: i32) -> ReviewItem {
        ReviewItem { id, role, name: name.into(), text: text.into(), secret: false, x, y, w: 80, h: 20 }
    }

    fn dialog() -> Review {
        Review::build(
            vec![
                it(3, Role::Button, "Cancel", "", 200, 100),
                it(1, Role::Label, "", "Save changes now", 0, 0),
                it(2, Role::Button, "Save", "", 100, 100),
                it(4, Role::TextEntry, "Name", "", 0, 40),
            ],
            false,
        )
    }

    #[test]
    fn items_on_one_row_become_one_line_left_to_right() {
        let mut r = dialog();
        assert_eq!(r.read_line(), "Save changes now");
        assert_eq!(r.next_line(), "Name, text entry");
        assert_eq!(r.next_line(), "Save, button, Cancel, button");
        assert_eq!(r.next_line(), "bottom");
        assert_eq!(r.prev_line(), "Name, text entry");
        assert_eq!(r.first_line(), "Save changes now");
        assert_eq!(r.prev_line(), "top");
    }

    #[test]
    fn words_flow_across_lines_and_stop_at_the_ends() {
        let mut r = dialog();
        r.end_of_line();
        assert_eq!(r.read_word(), "now");
        assert_eq!(r.next_word(), "Name, text entry");
        assert_eq!(r.bottom_right(), "Cancel, button");
        assert_eq!(r.next_word(), "bottom");
        assert_eq!(r.top_left(), "Save");
        assert_eq!(r.prev_word(), "top");
    }

    #[test]
    fn characters_and_spelling() {
        let mut r = dialog();
        assert_eq!(r.read_char(), "S");
        assert_eq!(r.next_char(), "a");
        r.end_of_line();
        r.prev_char();
        assert_eq!(r.read_char(), "s");
    }

    #[test]
    fn enter_starts_at_the_focus() {
        let mut r = dialog();
        assert_eq!(r.enter(2).unwrap(), "Save, button");
        assert!(r.enter(99).is_none());
    }

    #[test]
    fn read_from_cursor_goes_to_the_end() {
        let mut r = dialog();
        r.next_word();
        assert_eq!(r.read_from_cursor(), ["changes now", "Name, text entry", "Save, button, Cancel, button"]);
    }

    #[test]
    fn secrets_are_named_never_read() {
        let mut p = it(9, Role::PasswordEntry, "Password", "hunter2", 0, 0);
        p.secret = true;
        let mut r = Review::build(vec![p], false);
        assert_eq!(r.read_line(), "Password, password entry");
        assert_eq!(r.read_char(), "protected");
        assert!(!r.read_from_cursor().join(" ").contains("hunter2"));
        assert_eq!(r.next_char(), "bottom");
    }

    #[test]
    fn unlabelled_items_can_be_hidden() {
        let items = vec![it(1, Role::Button, "", "", 0, 0), it(2, Role::Button, "OK", "", 100, 0)];
        assert_eq!(Review::build(items.clone(), false).read_line(), "unlabelled, button, OK, button");
        assert_eq!(Review::build(items, true).read_line(), "OK, button");
    }

    #[test]
    fn empty_window_says_so() {
        let mut r = Review::build(vec![], false);
        assert_eq!(r.next_line(), "nothing on screen");
        assert_eq!(r.read_char(), "nothing on screen");
        assert!(r.read_from_cursor().is_empty());
    }
}
