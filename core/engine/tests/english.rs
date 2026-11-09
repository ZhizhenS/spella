use retype_engine::{offline_cloud, InlineBackend, Kernel, KernelBackend, KernelConfig};
use retype_types::*;
use std::{sync::Arc, time::Duration};
fn kernel() -> Kernel {
    let user = Arc::new(retype_dict::UserDict::new());
    Kernel::new(
        KernelConfig {
            chinese_on_start: false,
            rerank_enabled: false,
            english_segmentation: true,
            english_phrases: true,
            ..Default::default()
        },
        Arc::new(retype_dict::single_char_fallback()),
        Arc::new(retype_dict::Learner::new(user)),
        offline_cloud(Duration::from_millis(10)),
    )
}
fn key(k: &mut Kernel, key: Key, mods: Modifiers) -> Vec<KernelAction> {
    k.handle(InputEvent::Key {
        key,
        mods,
        source: InputSource::Keyboard,
    })
}
fn type_word(k: &mut Kernel, word: &str) {
    for c in word.chars() {
        key(k, Key::Char(c), Modifiers::NONE);
    }
}
fn commits(actions: &[KernelAction]) -> Vec<String> {
    actions
        .iter()
        .filter_map(|a| match a {
            KernelAction::Commit(
                CommitRequest::Text(t) | CommitRequest::ReplaceComposition { text: t },
            ) => Some(t.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn continuous_candidates_display_only_the_tail_and_commit_the_full_composition_once() {
    let mut k = kernel();
    type_word(&mut k, "iamt");
    let state = k.render_state();
    assert_eq!(state.composition, "iamt");
    let index = state
        .candidates
        .iter()
        .position(|candidate| {
            candidate.text == "I am trying to" && candidate.display_text() == "trying to"
        })
        .expect("local context should offer a trying-to completion");
    let actions = k.handle(InputEvent::CandidateChosen { index });
    assert_eq!(commits(&actions), ["I am trying to"]);
    assert!(!k.has_composition());
    assert!(!actions
        .iter()
        .any(|action| matches!(action, KernelAction::Side(SideEffect::Learn(_)))));

    type_word(&mut k, "iamt");
    let first = k.render_state().candidates[0].text.clone();
    assert_eq!(commits(&key(&mut k, Key::Tab, Modifiers::NONE)), [first]);
}

#[test]
fn continuous_digits_use_visible_labels_but_commit_full_text_without_annotations() {
    let mut k = kernel();
    type_word(&mut k, "iamt");
    let state = k.render_state();
    let index = state
        .candidates
        .iter()
        .position(|candidate| candidate.text == "I am trying to")
        .expect("phrase candidate");
    for _ in 0..index / 5 {
        key(&mut k, Key::Char('='), Modifiers::NONE);
    }
    let actions = key(
        &mut k,
        Key::Char(char::from(b'1' + (index % 5) as u8)),
        Modifiers::NONE,
    );
    assert_eq!(commits(&actions), ["I am trying to"]);
    assert!(!actions.contains(&KernelAction::PassThrough));
}

#[test]
fn continuous_raw_escape_backspace_and_settings_keep_original_input_authoritative() {
    for ending in [Key::Escape, Key::Space, Key::Enter] {
        let mut k = kernel();
        type_word(&mut k, "iamt");
        let actions = key(&mut k, ending, Modifiers::NONE);
        assert_eq!(
            commits(&actions),
            [if ending == Key::Space {
                "iamt "
            } else {
                "iamt"
            }]
        );
    }
    let mut k = kernel();
    type_word(&mut k, "iamt");
    let old_gen = k.generation();
    key(&mut k, Key::Backspace, Modifiers::NONE);
    type_word(&mut k, "bic");
    assert_eq!(k.render_state().composition, "iambic");
    assert_eq!(
        k.render_state().candidates[0].text.to_ascii_lowercase(),
        "iambic"
    );
    let before = k.render_state().candidates;
    k.handle(InputEvent::EnglishCompleted {
        gen: old_gen,
        candidates: vec![Candidate::new("stale phrase", CandidateSource::Local)],
    });
    assert_eq!(k.render_state().candidates, before);

    let mut k = kernel();
    type_word(&mut k, "iamt");
    k.handle(InputEvent::SetEnglishContinuous {
        segmentation: true,
        phrases: false,
    });
    assert!(k
        .render_state()
        .candidates
        .iter()
        .any(|candidate| candidate.text == "I am trying"));
    assert!(k.render_state().candidates.iter().all(|candidate| candidate
        .display_text()
        .split_whitespace()
        .count()
        < 2));
    k.handle(InputEvent::SetEnglishContinuous {
        segmentation: false,
        phrases: false,
    });
    assert_eq!(k.render_state().composition, "iamt");
    assert!(k
        .render_state()
        .candidates
        .iter()
        .all(|candidate| !candidate.text.contains(' ')));
}
#[test]
fn space_preserves_raw_and_tab_accepts_suggestion() {
    let mut k = kernel();
    type_word(&mut k, "hel");
    assert!(k
        .render_state()
        .candidates
        .iter()
        .any(|c| c.text == "hello"));
    assert!(!k.english_candidate_selected());
    assert_eq!(commits(&key(&mut k, Key::Space, Modifiers::NONE)), ["hel "]);
    type_word(&mut k, "hel");
    let first = k.render_state().candidates[0].text.clone();
    assert_eq!(commits(&key(&mut k, Key::Tab, Modifiers::NONE)), [first]);
    assert!(!k.has_composition());
}

#[test]
fn switching_to_chinese_commits_without_learning_the_english_word() {
    let mut k = kernel();
    type_word(&mut k, "retype");
    let actions = k.handle(InputEvent::ToggleChinese);
    assert_eq!(commits(&actions), ["retype"]);
    assert!(!actions
        .iter()
        .any(|action| matches!(action, KernelAction::Side(SideEffect::Learn(_)))));
    assert!(k.is_chinese());
    assert!(!k.has_composition());
}
#[test]
fn explicit_selection_space_and_enter_confirmation_do_not_submit_the_host() {
    let mut k = kernel();
    type_word(&mut k, "hel");
    key(&mut k, Key::Down, Modifiers::NONE);
    let first = k.render_state().candidates[0].text.clone();
    assert_eq!(
        commits(&key(&mut k, Key::Space, Modifiers::NONE)),
        [format!("{first} ")]
    );
    type_word(&mut k, "hel");
    let actions = key(&mut k, Key::Enter, Modifiers::NONE);
    assert_eq!(commits(&actions), ["hel"]);
    assert!(actions.contains(&KernelAction::PassThrough));
    assert!(!actions
        .iter()
        .any(|action| matches!(action, KernelAction::Side(SideEffect::Learn(_)))));
    type_word(&mut k, "hel");
    key(&mut k, Key::Down, Modifiers::NONE);
    let actions = key(&mut k, Key::Enter, Modifiers::NONE);
    assert_eq!(commits(&actions), [first]);
    assert!(!actions.contains(&KernelAction::PassThrough));
    let actions = key(&mut k, Key::Enter, Modifiers::NONE);
    assert!(commits(&actions).is_empty());
    assert!(actions.contains(&KernelAction::PassThrough));
}

#[test]
fn choosing_a_word_matches_tab_without_inserting_a_space() {
    let mut k = kernel();
    type_word(&mut k, "hel");
    let word = k.render_state().candidates[0].text.clone();
    let actions = k.handle(InputEvent::CandidateChosen { index: 0 });
    assert_eq!(commits(&actions), [word.clone()]);
    assert!(!actions
        .iter()
        .any(|action| matches!(action, KernelAction::Side(SideEffect::Learn(_)))));
    type_word(&mut k, "world");
    assert_eq!(k.render_state().composition, "world");
}

#[test]
fn repeated_english_commits_do_not_emit_learning_or_add_personal_words() {
    for ending in 0..9 {
        let user = Arc::new(retype_dict::UserDict::new());
        let learner = Arc::new(retype_dict::Learner::new(Arc::clone(&user)));
        let cloud = offline_cloud(Duration::from_millis(10));
        let kernel = Kernel::new(
            KernelConfig {
                chinese_on_start: false,
                rerank_enabled: false,
                ..Default::default()
            },
            Arc::new(retype_dict::single_char_fallback()),
            Arc::clone(&learner) as Arc<dyn LearningStore>,
            Arc::clone(&cloud),
        );
        // InlineBackend applies learning side effects, so an empty store proves
        // that commit routes cannot accidentally train the real learning path.
        let backend = InlineBackend::new(kernel, cloud);
        let event = |key| InputEvent::Key {
            key,
            mods: Modifiers::NONE,
            source: InputSource::Keyboard,
        };
        for _ in 0..3 {
            for c in "hel".chars() {
                backend.submit(event(Key::Char(c)));
            }
            let actions = match ending {
                0 => backend.submit(event(Key::Space)),
                1 => backend.submit(event(Key::Tab)),
                2 => backend.submit(InputEvent::CandidateChosen { index: 0 }),
                3 => {
                    backend.submit(event(Key::Down));
                    backend.submit(event(Key::Enter))
                }
                4 => backend.submit(event(Key::Enter)),
                5 => backend.submit(event(Key::Escape)),
                6 => backend.submit(event(Key::Char('3'))),
                7 => backend.submit(InputEvent::Key {
                    key: Key::Char('a'),
                    mods: Modifiers::CTRL,
                    source: InputSource::Keyboard,
                }),
                _ => {
                    let actions = backend.submit(InputEvent::ToggleChinese);
                    backend.submit(InputEvent::ToggleChinese);
                    actions
                }
            };
            assert!(!commits(&actions).is_empty());
            assert!(!actions
                .iter()
                .any(|action| matches!(action, KernelAction::Side(SideEffect::Learn(_)))));
        }
        assert!(learner.english_words("", 100).is_empty());
        assert!(user.usage_snapshot().records.is_empty());
    }
}

#[test]
fn case_apostrophes_punctuation_and_shortcuts_preserve_text() {
    let mut k = kernel();
    key(&mut k, Key::Char('H'), Modifiers::SHIFT);
    type_word(&mut k, "el");
    assert!(k
        .render_state()
        .candidates
        .iter()
        .any(|c| c.text == "Hello"));
    assert_eq!(
        commits(&key(&mut k, Key::Char(','), Modifiers::NONE)),
        ["Hel"]
    );
    type_word(&mut k, "don't");
    assert_eq!(
        commits(&key(&mut k, Key::Space, Modifiers::NONE)),
        ["don't "]
    );
    for ending in [Key::Char('0'), Key::Char('9'), Key::Char('.'), Key::Left] {
        type_word(&mut k, "test");
        let actions = key(&mut k, ending, Modifiers::NONE);
        assert_eq!(commits(&actions), ["test"]);
        assert!(actions.contains(&KernelAction::PassThrough));
    }
    type_word(&mut k, "word");
    let actions = key(&mut k, Key::Char('a'), Modifiers::CTRL);
    assert_eq!(commits(&actions), ["word"]);
    assert!(actions.contains(&KernelAction::PassThrough));
}

#[test]
fn tab_without_candidates_and_shift_tab_preserve_host_navigation() {
    let mut k = kernel();
    let actions = key(&mut k, Key::Tab, Modifiers::NONE);
    assert!(commits(&actions).is_empty());
    assert!(actions.contains(&KernelAction::PassThrough));
    type_word(&mut k, "zzzzzzzzzz");
    assert!(k.render_state().candidates.is_empty());
    let actions = key(&mut k, Key::Tab, Modifiers::NONE);
    assert_eq!(commits(&actions), ["zzzzzzzzzz"]);
    assert!(actions.contains(&KernelAction::PassThrough));
    type_word(&mut k, "hel");
    let actions = key(&mut k, Key::Tab, Modifiers::SHIFT);
    assert_eq!(commits(&actions), ["hel"]);
    assert!(actions.contains(&KernelAction::PassThrough));
}

fn candidate_fixture(count: usize) -> Kernel {
    let mut k = kernel();
    type_word(&mut k, "qzxvfixture");
    assert!(k.render_state().candidates.is_empty());
    let candidates = (0..count)
        .map(|i| {
            let mut candidate = Candidate::new(format!("choice{i}"), CandidateSource::Local);
            candidate.language = CandidateLanguage::English;
            candidate
        })
        .collect();
    k.handle(InputEvent::EnglishCompleted {
        gen: k.generation(),
        candidates,
    });
    k
}

#[test]
fn digits_one_to_five_select_the_current_page_without_a_trailing_space() {
    for digit in '1'..='5' {
        let mut k = candidate_fixture(12);
        assert_eq!(k.render_state().visible().len(), 5);
        let expected = k.render_state().visible()[digit as usize - '1' as usize]
            .text
            .clone();
        let actions = key(&mut k, Key::Char(digit), Modifiers::NONE);
        assert_eq!(commits(&actions), [expected]);
        assert!(!actions.contains(&KernelAction::PassThrough));
        assert!(!k.has_composition());
    }
    for digit in ['0', '6', '7', '8', '9'] {
        let mut k = candidate_fixture(12);
        let actions = key(&mut k, Key::Char(digit), Modifiers::NONE);
        assert_eq!(commits(&actions), ["qzxvfixture"]);
        assert!(actions.contains(&KernelAction::PassThrough));
    }
}

#[test]
fn numeric_selection_uses_visible_rows_and_never_a_hidden_candidate() {
    let mut k = candidate_fixture(12);
    // Even narrow viewports keep five vertical rows; their widths do not
    // accumulate horizontally into one-item pages.
    k.layout_candidates(&vec![420; 12], 420, 2);
    assert_eq!(k.render_state().page_starts, [0, 5, 10]);
    assert_eq!(k.render_state().visible().len(), 5);
    key(&mut k, Key::PageDown, Modifiers::NONE);
    assert_eq!(k.render_state().page_start, 5);
    assert!(k.english_candidate_selected());
    assert_eq!(
        commits(&key(&mut k, Key::Char('2'), Modifiers::NONE)),
        ["choice6"]
    );

    let mut k = candidate_fixture(12);
    key(&mut k, Key::Char('='), Modifiers::NONE);
    key(&mut k, Key::Char('+'), Modifiers::SHIFT);
    assert_eq!(k.render_state().page_start, 10);
    assert_eq!(k.render_state().visible().len(), 2);
    let actions = key(&mut k, Key::Char('3'), Modifiers::NONE);
    assert_eq!(commits(&actions), ["qzxvfixture"]);
    assert!(actions.contains(&KernelAction::PassThrough));
}

#[test]
fn minus_plus_and_page_keys_navigate_without_committing_or_changing_input() {
    let mut k = candidate_fixture(12);
    for (page_key, mods, expected) in [
        (Key::Char('-'), Modifiers::NONE, 0),
        (Key::Char('='), Modifiers::NONE, 5),
        (Key::Char('+'), Modifiers::SHIFT, 10),
        (Key::PageDown, Modifiers::NONE, 10),
        (Key::PageUp, Modifiers::NONE, 5),
        (Key::Char('_'), Modifiers::SHIFT, 0),
        (Key::Char('-'), Modifiers::NONE, 0),
    ] {
        let actions = key(&mut k, page_key, mods);
        assert!(commits(&actions).is_empty());
        assert!(!actions.contains(&KernelAction::PassThrough));
        assert_eq!(k.render_state().page_start, expected);
        assert_eq!(k.render_state().composition, "qzxvfixture");
        assert!(k.english_candidate_selected());
    }
}

#[test]
fn page_symbols_and_page_keys_remain_native_without_candidates() {
    for (page_key, mods) in [
        (Key::Char('-'), Modifiers::NONE),
        (Key::Char('_'), Modifiers::SHIFT),
        (Key::Char('='), Modifiers::NONE),
        (Key::Char('+'), Modifiers::SHIFT),
        (Key::PageUp, Modifiers::NONE),
        (Key::PageDown, Modifiers::NONE),
    ] {
        let mut k = kernel();
        let actions = key(&mut k, page_key, mods);
        assert!(commits(&actions).is_empty());
        assert!(actions.contains(&KernelAction::PassThrough));
        type_word(&mut k, "qzxvfixture");
        assert!(k.render_state().candidates.is_empty());
        let actions = key(&mut k, page_key, mods);
        assert_eq!(commits(&actions), ["qzxvfixture"]);
        assert!(actions.contains(&KernelAction::PassThrough));
    }
}

#[test]
fn digits_remain_literal_without_candidates_or_with_a_shortcut_modifier() {
    let mut k = kernel();
    for text in ["", "zzzzzzzzzz"] {
        type_word(&mut k, text);
        assert!(k.render_state().candidates.is_empty());
        let actions = key(&mut k, Key::Char('2'), Modifiers::NONE);
        assert!(actions.contains(&KernelAction::PassThrough));
        assert_eq!(
            commits(&actions),
            if text.is_empty() {
                vec![]
            } else {
                vec![text.to_string()]
            }
        );
    }
    type_word(&mut k, "art");
    let actions = key(&mut k, Key::Char('2'), Modifiers::CTRL);
    assert_eq!(commits(&actions), ["art"]);
    assert!(actions.contains(&KernelAction::PassThrough));
    type_word(&mut k, "python");
    assert_eq!(
        commits(&key(&mut k, Key::Escape, Modifiers::NONE)),
        ["python"]
    );
    assert!(key(&mut k, Key::Char('3'), Modifiers::NONE).contains(&KernelAction::PassThrough));
}
#[test]
fn escape_keeps_text_reset_clears_and_late_spelling_is_ignored() {
    let mut k = kernel();
    type_word(&mut k, "hellp");
    let gen = k.generation();
    let candidates = retype_english::correct("hellp", 8);
    k.handle(InputEvent::EnglishCompleted {
        gen,
        candidates: candidates.clone(),
    });
    assert!(k
        .render_state()
        .candidates
        .iter()
        .any(|c| c.text == "hello"));
    assert_eq!(
        commits(&key(&mut k, Key::Escape, Modifiers::NONE)),
        ["hellp"]
    );
    assert!(k
        .handle(InputEvent::EnglishCompleted { gen, candidates })
        .is_empty());
    type_word(&mut k, "hello");
    k.handle(InputEvent::ResetComposition);
    assert!(!k.has_composition());
    type_word(&mut k, "hel");
    key(&mut k, Key::Backspace, Modifiers::NONE);
    assert_eq!(k.composition_text(), "he");
}

#[test]
fn pinyin_setting_refreshes_live_composition_and_discards_old_async_results() {
    let mut k = kernel();
    assert!(k.config().english_pinyin);
    type_word(&mut k, "hello");
    let old_generation = k.generation();
    let actions = k.handle(InputEvent::SetEnglishPinyin(false));
    assert!(!k.config().english_pinyin);
    assert_eq!(k.composition_text(), "hello");
    assert!(commits(&actions).is_empty());
    assert!(actions
        .iter()
        .any(|action| matches!(action, KernelAction::Render(_))));
    assert_ne!(k.generation(), old_generation);
    assert!(k
        .handle(InputEvent::EnglishCompleted {
            gen: old_generation,
            candidates: vec![retype_types::Candidate::new(
                "stale",
                retype_types::CandidateSource::Local
            )],
        })
        .is_empty());
    let current = k.generation();
    assert!(k.handle(InputEvent::SetEnglishPinyin(false)).is_empty());
    assert_eq!(k.generation(), current);
    k.handle(InputEvent::SetEnglishPinyin(true));
    assert!(k.config().english_pinyin);
    assert_eq!(k.composition_text(), "hello");
}
