use super::*;

impl Kernel {
    /// Merge separate ranked streams by match evidence, never by cross-language scores.
    pub(super) fn mix_english(&mut self, chinese_word: bool) {
        let words = self.english.mixed(&self.buffer, self.learner.as_ref(), 2);
        let chinese_count = self.candidates.len();
        for (offset, word) in words.into_iter().enumerate() {
            if self.candidates.iter().any(|c| c.text == word.text) {
                continue;
            }
            let exact = word.text.eq_ignore_ascii_case(&self.buffer);
            // A learned Latin spelling can be a code or pinyin, rather than an
            // English word. Retain it without promoting it over Chinese matches.
            let before = if chinese_count > 0 && !self.english.contains_word(&word.text) {
                chinese_count
            } else if exact && !chinese_word {
                0
            } else if exact {
                1
            } else {
                2
            };
            let position = before.min(chinese_count) + offset;
            self.candidates
                .insert(position.min(self.candidates.len()), word);
        }
        self.candidates.truncate(self.cfg.candidate_cap);
    }
    pub(super) fn refresh_english(&mut self, actions: &mut Vec<KernelAction>) {
        self.bump_gen();
        self.english_selected = false;
        self.status = self.status.difference(StatusFlags::ENGLISH_SELECTED);
        self.selected = 0;
        self.page_start = 0;
        self.page_starts.clear();
        self.candidates = self.english.complete_input(
            &self.buffer,
            self.learner.as_ref(),
            24,
            self.cfg.english_pinyin,
            self.cfg.english_segmentation,
            self.cfg.english_phrases,
        );
        actions.push(KernelAction::Render(self.render_state()));
        if self.cfg.english_spelling && self.buffer.len() >= 3 {
            actions.push(KernelAction::Side(SideEffect::EnglishSuggest {
                gen: self.gen,
                input: self.buffer.clone(),
            }));
        }
    }
    pub(super) fn commit_english(
        &mut self,
        index: Option<usize>,
        suffix: &str,
        actions: &mut Vec<KernelAction>,
    ) {
        if self.buffer.is_empty() {
            return;
        }
        let text = index
            .and_then(|i| self.candidates.get(i))
            .map_or_else(|| self.buffer.clone(), |c| c.text.clone());
        // English Assist uses the read-only base vocabulary. Successful typing
        // must not silently create a personal word history; optional learning
        // can be added only alongside an explicit user preference.
        self.reset_composition();
        self.bump_gen();
        actions.push(KernelAction::Commit(CommitRequest::ReplaceComposition {
            text: format!("{text}{suffix}"),
        }));
        actions.push(KernelAction::Render(self.render_state()));
    }
    pub(super) fn on_english_key(
        &mut self,
        key: Key,
        mods: Modifiers,
        actions: &mut Vec<KernelAction>,
    ) {
        if !mods.is_plain() {
            self.commit_english(None, "", actions);
            actions.push(KernelAction::PassThrough);
            return;
        }
        match key {
            Key::Char(c) if c.is_ascii_alphabetic() || (c == '\'' && !self.buffer.is_empty()) => {
                if self.buffer.len() >= self.cfg.max_buffer_chars {
                    self.commit_english(None, "", actions);
                }
                self.buffer.push(c);
                self.refresh_english(actions);
            }
            Key::Backspace if !self.buffer.is_empty() => {
                self.buffer.pop();
                self.refresh_english(actions);
            }
            Key::Escape if self.has_composition() => self.commit_english(None, "", actions),
            Key::Char(d @ '1'..='5')
                if self.has_composition()
                    && (d as usize - '1' as usize) < self.current_page_size()
                    && self.page_start + (d as usize - '1' as usize) < self.candidates.len() =>
            {
                let index = self.page_start + (d as usize - '1' as usize);
                self.commit_english(Some(index), "", actions);
            }
            Key::Tab if !mods.contains(Modifiers::SHIFT) && !self.candidates.is_empty() => {
                self.commit_english(Some(self.selected), "", actions)
            }
            Key::Space if self.has_composition() => {
                let index = self.english_selected.then_some(self.selected);
                if self.cfg.english_auto_space {
                    // A plain Space may add boundaries, never untyped letters.
                    // Explicitly navigating to a candidate still accepts it.
                    // This lookup is independent of the visible candidate cap.
                    // A recognized pinyin interpretation requires explicit
                    // acceptance. Do not respell its letters as accidental
                    // English words (e.g. "they I yang") on a plain Space.
                    let pinyin_interpretation = self.cfg.english_pinyin
                        && self.candidates.iter().any(|candidate| {
                            candidate
                                .annotation
                                .as_ref()
                                .is_some_and(|note| note.pinyin_match)
                        });
                    if index.is_none() && self.cfg.english_segmentation && !pinyin_interpretation {
                        if let Some(text) = self.english.segment_exact(&self.buffer) {
                            self.buffer = text;
                        }
                    }
                    self.commit_english(index, "", actions);
                } else {
                    // With automatic boundaries disabled, a physical Space
                    // keeps its traditional manual-space meaning.
                    self.commit_english(index, " ", actions);
                }
            }
            // Enter is the literal escape even after candidate navigation.
            // It ends only this composition; the next Enter reaches the host.
            Key::Enter if self.has_composition() && !mods.contains(Modifiers::SHIFT) => {
                self.commit_english(None, "", actions);
            }
            Key::Up | Key::Down if !self.candidates.is_empty() => {
                self.selected = if !self.english_selected {
                    0
                } else if key == Key::Down {
                    (self.selected + 1).min(self.candidates.len() - 1)
                } else {
                    self.selected.saturating_sub(1)
                };
                self.english_selected = true;
                self.status = self.status.union(StatusFlags::ENGLISH_SELECTED);
                self.page_start = self
                    .page_starts
                    .iter()
                    .copied()
                    .take_while(|start| *start <= self.selected)
                    .last()
                    .unwrap_or_else(|| self.selected / self.page_capacity() * self.page_capacity());
                actions.push(KernelAction::Render(self.render_state()));
            }
            Key::PageUp | Key::PageDown | Key::Char('-' | '_' | '=' | '+')
                if self.has_composition() && !self.candidates.is_empty() =>
            {
                self.english_selected = true;
                self.status = self.status.union(StatusFlags::ENGLISH_SELECTED);
                self.page(
                    if matches!(key, Key::PageUp | Key::Char('-' | '_')) {
                        -1
                    } else {
                        1
                    },
                    actions,
                );
            }
            _ => {
                // Without a matching visible candidate, digits and punctuation
                // finish the literal spelling and retain their native meaning.
                self.commit_english(None, "", actions);
                actions.push(KernelAction::PassThrough);
            }
        }
    }
}
