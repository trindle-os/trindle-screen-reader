//! Say all: read a document from a position to its end, one sentence at a time.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sentence {
    pub start: usize,
    pub text: String,
    pub paragraph_start: bool,
}

pub fn split(text: &str, from: usize) -> Vec<Sentence> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = from.min(chars.len());
    let mut para = true;
    while i < chars.len() {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        let start = i;
        let mut blank_after = false;
        while i < chars.len() {
            let c = chars[i];
            i += 1;
            let ends = matches!(c, '.' | '!' | '?' | '\u{2026}') && (i >= chars.len() || chars[i].is_whitespace());
            let double_newline = c == '\n' && i < chars.len() && chars[i] == '\n';
            if ends || double_newline {
                while i < chars.len() && matches!(chars[i], '"' | '\'' | ')' | '\u{201d}' | '\u{2019}') {
                    i += 1;
                }
                let mut j = i;
                let mut newlines = 0;
                while j < chars.len() && chars[j].is_whitespace() {
                    if chars[j] == '\n' {
                        newlines += 1;
                    }
                    j += 1;
                }
                blank_after = newlines >= 2 || j >= chars.len() || double_newline;
                break;
            }
        }
        let piece: String = chars[start..i].iter().collect();
        let piece = piece.trim().to_string();
        if !piece.is_empty() {
            out.push(Sentence { start, text: piece, paragraph_start: para });
        }
        para = blank_after;
    }
    out
}

#[derive(Debug)]
pub struct SayAll {
    sentences: Vec<Sentence>,
    cur: usize,
    ended: bool,
}

impl SayAll {
    pub fn start(text: &str, from: usize) -> Option<SayAll> {
        let sentences = split(text, from);
        (!sentences.is_empty()).then_some(SayAll { sentences, cur: 0, ended: false })
    }

    pub fn current(&self) -> Option<&Sentence> {
        (!self.ended).then(|| &self.sentences[self.cur])
    }

    pub fn position(&self) -> usize {
        self.sentences[self.cur].start
    }

    pub fn advance(&mut self) -> Option<&Sentence> {
        if self.cur + 1 < self.sentences.len() {
            self.cur += 1;
            Some(&self.sentences[self.cur])
        } else {
            self.ended = true;
            None
        }
    }

    pub fn skip_sentence(&mut self, by: i32) -> Option<&Sentence> {
        let last = self.sentences.len() as i64 - 1;
        self.cur = (self.cur as i64 + by as i64).clamp(0, last) as usize;
        self.ended = false;
        self.current()
    }

    pub fn skip_paragraph(&mut self, forward: bool) -> Option<&Sentence> {
        if forward {
            match (self.cur + 1..self.sentences.len()).find(|&i| self.sentences[i].paragraph_start) {
                Some(i) => self.cur = i,
                None => {
                    self.ended = true;
                    return None;
                }
            }
        } else {
            let mut i = self.cur;
            if self.sentences[i].paragraph_start && i > 0 {
                i -= 1;
            }
            while i > 0 && !self.sentences[i].paragraph_start {
                i -= 1;
            }
            self.cur = i;
        }
        self.ended = false;
        self.current()
    }
}

pub fn with_marker(n: usize, text: &str) -> String {
    format!("\u{1}{n}\u{1}{text}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "First one. Second one! Third?\n\nNew paragraph here. Last bit";

    fn texts(v: &[Sentence]) -> Vec<&str> {
        v.iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn splits_into_sentences_with_offsets() {
        let s = split(DOC, 0);
        assert_eq!(texts(&s), ["First one.", "Second one!", "Third?", "New paragraph here.", "Last bit"]);
        assert_eq!((s[0].start, s[1].start), (0, 11));
        assert!(s[0].paragraph_start && s[3].paragraph_start && !s[1].paragraph_start);
    }

    #[test]
    fn starts_from_the_caret() {
        assert_eq!(split(DOC, 11)[0].text, "Second one!");
    }

    #[test]
    fn decimals_and_empty_text_do_not_split() {
        assert_eq!(texts(&split("It costs 3.5 pounds. Fine.", 0)), ["It costs 3.5 pounds.", "Fine."]);
        assert!(SayAll::start("   \n ", 0).is_none());
        assert!(SayAll::start("abc", 99).is_none());
    }

    #[test]
    fn reads_to_the_end_then_says_finished() {
        let mut r = SayAll::start("A. B.", 0).unwrap();
        assert_eq!(r.current().unwrap().text, "A.");
        assert_eq!(r.advance().unwrap().text, "B.");
        assert!(r.advance().is_none());
        assert!(r.ended);
    }

    #[test]
    fn skips_by_sentence_and_paragraph() {
        let mut r = SayAll::start(DOC, 0).unwrap();
        assert_eq!(r.skip_sentence(1).unwrap().text, "Second one!");
        assert_eq!(r.skip_sentence(-5).unwrap().text, "First one.");
        assert_eq!(r.skip_paragraph(true).unwrap().text, "New paragraph here.");
        assert_eq!(r.skip_paragraph(false).unwrap().text, "First one.");
        r.skip_sentence(4);
        assert_eq!(r.skip_paragraph(false).unwrap().text, "New paragraph here.");
        assert!(r.skip_paragraph(true).is_none());
    }

    #[test]
    fn marker_is_wrapped_in_control_characters() {
        assert_eq!(with_marker(3, "Hi."), "\u{1}3\u{1}Hi.");
    }
}
