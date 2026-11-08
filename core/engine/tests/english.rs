use retype_engine::{offline_cloud, InlineBackend, Kernel, KernelBackend, KernelConfig};
use retype_types::*;
use std::{sync::Arc, time::Duration};
fn kernel() -> Kernel {
    let user = Arc::new(retype_dict::UserDict::new());
    Kernel::new(
        KernelConfig {
            chinese_on_start: false,
            rerank_enabled: false,
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
        commits(&key(&mut k, Key::Char('='), Modifiers::NONE)),
        ["Hel"]
    );
    type_word(&mut k, "don't");
    assert_eq!(
        commits(&key(&mut k, Key::Space, Modifiers::NONE)),
        ["don't "]
    );
    for ending in [Key::Char('0'), Key::Char('9'), Key::Char('-'), Key::Left] {
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

#[test]
fn every_digit_preserves_the_typed_word_with_visible_candidates() {
    for digit in '0'..='9' {
        let mut k = kernel();
        type_word(&mut k, "python");
        assert!(!k.render_state().candidates.is_empty());
        let actions = key(&mut k, Key::Char(digit), Modifiers::NONE);
        assert_eq!(commits(&actions), ["python"]);
        assert!(actions.contains(&KernelAction::PassThrough));
        assert!(!k.has_composition());
    }
}

#[test]
fn digits_remain_literal_after_explicit_selection_and_paging() {
    let mut k = kernel();
    type_word(&mut k, "art");
    let count = k.render_state().candidates.len();
    assert!(count > 4);
    k.layout_candidates(&vec![100; count], 202, 2);
    key(&mut k, Key::Down, Modifiers::NONE);
    key(&mut k, Key::PageDown, Modifiers::NONE);
    let state = k.render_state();
    assert_eq!(state.page_start, 2);
    assert!(k.english_candidate_selected());
    let actions = key(&mut k, Key::Char('2'), Modifiers::NONE);
    assert_eq!(commits(&actions), ["art"]);
    assert!(actions.contains(&KernelAction::PassThrough));

    type_word(&mut k, "art");
    let count = k.render_state().candidates.len();
    k.layout_candidates(&vec![100; count], 202, 2);
    let actions = key(&mut k, Key::Char('3'), Modifiers::NONE);
    assert_eq!(commits(&actions), ["art"]);
    assert!(actions.contains(&KernelAction::PassThrough));
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
