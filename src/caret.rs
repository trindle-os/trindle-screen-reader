//! Reading text by character, word and line, and deciding what a caret move should say.

use unicode_segmentation::UnicodeSegmentation;

pub const BLANK: &str = "blank";

fn chars(text: &str) -> Vec<char> {
    text.chars().collect()
}

fn slice(c: &[char], a: usize, b: usize) -> String {
    c[a.min(c.len())..b.min(c.len())].iter().collect()
}

pub fn char_name(g: &str) -> String {
    match g {
        " " => "space".into(),
        "\t" => "tab".into(),
        "\n" | "\r\n" => "new line".into(),
        "" => String::new(),
        x => x.to_string(),
    }
}

pub fn char_at(text: &str, at: usize) -> String {
    let c = chars(text);
    if at >= c.len() {
        return String::new();
    }
    let rest = slice(&c, at, c.len());
    rest.graphemes(true).next().unwrap_or("").to_string()
}

pub fn word_at(text: &str, at: usize) -> String {
    let mut pos = 0usize;
    let mut best = String::new();
    for (_, w) in text.split_word_bound_indices() {
        let len = w.chars().count();
        let is_word = w.chars().any(|c| c.is_alphanumeric());
        if is_word && at < pos + len {
            return w.to_string();
        }
        if is_word && best.is_empty() && pos >= at {
            best = w.to_string();
        }
        pos += len;
    }
    best
}

pub fn line_at(text: &str, at: usize) -> String {
    let c = chars(text);
    let at = at.min(c.len());
    let start = c[..at].iter().rposition(|&x| x == '\n').map_or(0, |p| p + 1);
    let end = c[at..].iter().position(|&x| x == '\n').map_or(c.len(), |p| at + p);
    slice(&c, start, end).trim_end_matches('\r').to_string()
}

fn line_index(c: &[char], at: usize) -> usize {
    c[..at.min(c.len())].iter().filter(|&&x| x == '\n').count()
}

pub fn caret_moved(text: &str, old: usize, new: usize) -> String {
    let c = chars(text);
    if old == new {
        return String::new();
    }
    if line_index(&c, old) != line_index(&c, new) {
        let l = line_at(text, new);
        return if l.trim().is_empty() { BLANK.into() } else { l };
    }
    let (a, b) = (old.min(new), old.max(new));
    let between = slice(&c, a, b);
    if between.graphemes(true).count() <= 1 {
        return char_name(&char_at(text, new));
    }
    word_at(text, new)
}

pub fn last_word(before: &str) -> String {
    let w: Vec<char> = before.chars().rev().take_while(|c| c.is_alphanumeric() || *c == '\'').collect();
    w.into_iter().rev().collect()
}

pub fn typed(before: &str, inserted: &str, echo: crate::settings::TypingEcho) -> String {
    use crate::settings::TypingEcho::*;
    if echo == None {
        return String::new();
    }
    if inserted.graphemes(true).count() != 1 {
        return inserted.to_string();
    }
    let c = inserted.chars().next().unwrap_or(' ');
    if !(c.is_alphanumeric() || c == '\'') {
        let w = last_word(before);
        if !w.is_empty() && matches!(echo, Word | Both) {
            return w;
        }
    } else if echo == Word {
        return String::new();
    }
    char_name(inserted)
}

pub fn deleted(removed: &str) -> String {
    let w = removed.graphemes(true).count();
    let said = if w == 1 { char_name(removed) } else { removed.to_string() };
    crate::words::DELETED.replace("{what}", &said)
}

pub fn spell(text: &str) -> String {
    text.graphemes(true).map(char_name).collect::<Vec<_>>().join(", ")
}
