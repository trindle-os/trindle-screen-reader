//! The reader's commands that need the bus, a fetch or a setting: review, browse, settings and what the fetches bring back.

use super::*;

impl Reader {
    pub(super) fn fetch_text(&mut self) {
        if self.fetching.insert((self.generation, FETCH_TEXT)) {
            let (bus, tx, generation) = (self.bus.clone(), self.fetch_tx.clone(), self.generation);
            tokio::task::spawn_local(async move {
                let _ = tx.send(Fetched::Text { generation, text: bus.focused_text().await });
            });
        }
    }

    pub(super) fn review_command(&mut self, cmd: ReviewCmd) -> Vec<Out> {
        if let Some(r) = self.review.as_mut() {
            let lines = reader::keys::run_review(r, cmd);
            let now = self.now();
            return review_outs(&mut self.coord, now, lines);
        }
        self.review_wanted = Some((self.generation, cmd));
        if self.fetching.insert((self.generation, FETCH_ITEMS)) {
            let (bus, tx, generation) = (self.bus.clone(), self.fetch_tx.clone(), self.generation);
            tokio::task::spawn_local(async move {
                let _ = tx.send(Fetched::Items { generation, items: bus.window_items().await });
            });
        }
        vec![]
    }

    fn follow_cursor(&mut self) {
        let Some(b) = self.browse.as_ref() else { return };
        let Some(i) = b.focusable_cursor() else { return };
        let Some(target) = self.browse_refs.get(i).cloned() else { return };
        let node = b.node_at(i);
        let now = self.now();
        self.own_focus.retain(|(_, _, at)| now.saturating_sub(*at) < OWN_FOCUS_MS);
        self.own_focus.push((node.role, node.name.clone(), now));
        let bus = self.bus.clone();
        tokio::task::spawn_local(async move {
            bus.grab_focus(&target).await;
        });
    }

    pub(super) fn took_own_focus(&mut self, object: &reader::model::ObjectInfo) -> bool {
        let now = self.now();
        self.own_focus.retain(|(_, _, at)| now.saturating_sub(*at) < OWN_FOCUS_MS);
        let hit = self.own_focus.iter().position(|(role, name, _)| Some(*role) == object.role && *name == object.name);
        hit.map(|i| self.own_focus.remove(i)).is_some()
    }

    pub(super) fn tell_browse_keys(&mut self, was: bool) {
        let now = self.key_state.in_browse_keys();
        if now != was {
            reader::keysock::send_back(&self.to_compositor, reader::keys::browse_line(now));
        }
    }

    fn leave_browse_for_focus(&mut self) {
        let browsing = self.key_state.in_browse_keys();
        if browsing && self.browse.as_ref().is_some_and(|b| b.mode() == reader::browse::Mode::Focus) {
            self.key_state.leave_browse();
            self.tell_browse_keys(true);
        }
    }

    pub(super) fn browse_command(&mut self, cmd: BrowseCmd) -> Vec<Out> {
        if let Some(b) = self.browse.as_mut() {
            let lines = run_browse(b, cmd);
            self.follow_cursor();
            self.leave_browse_for_focus();
            let now = self.now();
            return review_outs(&mut self.coord, now, lines);
        }
        self.browse_wanted = Some((self.generation, cmd));
        if self.fetching.insert((self.generation, FETCH_DOC)) {
            let (bus, tx, generation) = (self.bus.clone(), self.fetch_tx.clone(), self.generation);
            tokio::task::spawn_local(async move {
                let _ = tx.send(Fetched::Doc { generation, doc: bus.document_nodes().await });
            });
        }
        vec![]
    }

    pub(super) fn step_level(&mut self, rate: bool, n: i8) -> Vec<Out> {
        let field = if rate { &mut self.settings.rate } else { &mut self.settings.volume };
        let low = if rate { reader::settings::MIN_RATE } else { 0 };
        *field = (*field as i16 + n as i16).clamp(low as i16, 100) as u8;
        let said = if rate { reader::words::rate_said(*field) } else { reader::words::volume_said(*field) };
        if let Some(v) = self.voice.as_mut() {
            v.set_levels(self.settings.rate, self.settings.volume);
        }
        if let Err(e) = self.settings.save() {
            reader::log::note(&format!("could not save settings: {e}"));
        }
        self.coord.submit(self.now(), own(said, Priority::Focus))
    }

    pub(super) fn change_setting(&mut self, cmd: SettingCmd) -> Vec<Out> {
        use reader::{model::Verbosity, words};
        let s = &mut self.settings;
        let said: String = match cmd {
            SettingCmd::Verbosity => {
                s.verbosity = if s.verbosity == Verbosity::Brief { Verbosity::Verbose } else { Verbosity::Brief };
                self.describer.verbosity = s.verbosity;
                (if s.verbosity == Verbosity::Brief { words::VERBOSITY_BRIEF } else { words::VERBOSITY_VERBOSE }).into()
            }
            SettingCmd::TypingEcho => {
                s.typing_echo = s.typing_echo.next();
                self.describer.typing_echo = s.typing_echo;
                words::typing_echo_said(s.typing_echo).into()
            }
            SettingCmd::Sounds => {
                s.sounds = !s.sounds;
                (if s.sounds { words::SOUNDS_ON } else { words::SOUNDS_OFF }).into()
            }
            SettingCmd::CueStep(n) => {
                s.cue_volume = (s.cue_volume as i16 + n as i16).clamp(0, 100) as u8;
                words::cue_volume_said(s.cue_volume)
            }
            SettingCmd::Layout => {
                s.laptop_layout = !s.laptop_layout;
                self.keymap = Keymap::new(if s.laptop_layout { Layout::Laptop } else { Layout::Desktop });
                (if s.laptop_layout { words::LAYOUT_LAPTOP } else { words::LAYOUT_NUMBER_PAD }).into()
            }
        };
        if let Err(e) = self.settings.save() {
            reader::log::note(&format!("could not save settings: {e}"));
        }
        self.coord.submit(self.now(), own(said, Priority::Focus))
    }

    pub(super) fn on_fetched(&mut self, f: Fetched) {
        match f {
            Fetched::Text { generation, text } => {
                self.fetching.remove(&(generation, FETCH_TEXT));
                let wanted = std::mem::take(&mut self.say_all_wanted);
                if let (true, true, Some((text, cut))) = (wanted, generation == self.generation, text) {
                    let mut outs = self.coord.say_all_start(&text, 0);
                    if cut {
                        self.coord.say_all_is_cut();
                    }
                    self.emit(std::mem::take(&mut outs));
                }
            }
            Fetched::Doc { generation, doc } => {
                self.fetching.remove(&(generation, FETCH_DOC));
                if generation == self.generation {
                    self.browse_arrived(doc);
                }
            }
            Fetched::Items { generation, items } => {
                self.fetching.remove(&(generation, FETCH_ITEMS));
                if generation == self.generation {
                    self.review_arrived(items);
                }
            }
        }
    }

    fn browse_arrived(&mut self, doc: Option<Doc>) {
        if let Some((nodes, refs, focus)) = doc {
            self.browse = Some(Browser::new(nodes, focus));
            self.browse_refs = refs;
        }
        let lines = match (self.browse.as_mut(), self.browse_wanted.take()) {
            (Some(b), Some((_, cmd))) => run_browse(b, cmd),
            (None, Some(_)) => vec![reader::words::BROWSE_EMPTY.to_string()],
            _ => vec![],
        };
        self.follow_cursor();
        self.leave_browse_for_focus();
        let now = self.now();
        let outs = review_outs(&mut self.coord, now, lines);
        self.emit(outs);
    }

    fn review_arrived(&mut self, items: Option<(Vec<ReviewItem>, Option<u64>)>) {
        if let Some((items, focus)) = items {
            let mut r = Review::build(items, false);
            if let Some(f) = focus {
                r.enter(f);
            }
            self.review = Some(r);
        }
        let lines = match (self.review.as_mut(), self.review_wanted.take()) {
            (Some(r), Some((_, cmd))) => reader::keys::run_review(r, cmd),
            (None, Some(_)) => vec![reader::words::REVIEW_EMPTY.to_string()],
            _ => vec![],
        };
        let now = self.now();
        let outs = review_outs(&mut self.coord, now, lines);
        self.emit(outs);
    }
}
