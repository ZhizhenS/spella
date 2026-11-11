//! Real prepared-resource integration, isolated from the other test binaries.
use retype_engine::{offline_cloud, Kernel, KernelConfig};
use retype_types::*;
use std::{sync::Arc, time::Duration};

fn kernel() -> Kernel {
    Kernel::new(
        KernelConfig {
            chinese_on_start: false,
            english_auto_space: true,
            english_segmentation: true,
            english_phrases: true,
            rerank_enabled: false,
            ..Default::default()
        },
        Arc::new(retype_dict::single_char_fallback()),
        Arc::new(retype_dict::Learner::new(Arc::new(
            retype_dict::UserDict::new(),
        ))),
        offline_cloud(Duration::from_millis(10)),
    )
}
fn key(kernel: &mut Kernel, key: Key) -> Vec<KernelAction> {
    kernel.handle(InputEvent::Key {
        key,
        mods: Modifiers::NONE,
        source: InputSource::Keyboard,
    })
}
fn type_word(kernel: &mut Kernel, text: &str) {
    for character in text.chars() {
        key(kernel, Key::Char(character));
    }
}
fn commits(actions: &[KernelAction]) -> Vec<&str> {
    actions
        .iter()
        .filter_map(|action| match action {
            KernelAction::Commit(
                CommitRequest::ReplaceComposition { text } | CommitRequest::Text(text),
            ) => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn check_continuous_contractions() {
    let input = "idontwanttosaygoodbye";
    let restored = "I don't want to say goodbye";
    let mut typing = kernel();
    for (offset, letter) in input.char_indices() {
        let actions = key(&mut typing, Key::Char(letter));
        let end = offset + 1;
        assert!(commits(&actions).is_empty());
        let state = typing.render_state();
        assert_eq!(state.composition, input[..end]);
        if end >= "idont".len() {
            assert!(
                state
                    .candidates
                    .iter()
                    .any(|candidate| candidate.consumed == end),
                "candidate refresh stopped at {}",
                &input[..end]
            );
        }
    }
    for (raw, expected) in [
        ("idont", "I don't"),
        ("idontw", "I don't want"),
        ("idontwanttosaygoodb", restored),
        (input, restored),
    ] {
        let mut k = kernel();
        type_word(&mut k, raw);
        let state = k.render_state();
        assert_eq!(state.composition, raw);
        let index =
            state.candidates.iter().take(5).position(|candidate| {
                candidate.text == expected && candidate.consumed == raw.len()
            });
        assert!(index.is_some(), "{raw}: {:?}", state.candidates);
        assert_eq!(
            commits(&key(
                &mut k,
                Key::Char(char::from(b'1' + index.unwrap() as u8))
            )),
            [expected]
        );
    }

    for (raw, expected) in [
        (input, restored),
        ("idon'twanttosaygoodbye", restored),
        ("icantwait", "I can't wait"),
        ("imgoinghome", "I'm going home"),
        ("idontknowwhattodo", "I don't know what to do"),
        (
            "idontwanttosaygoodbyebutihavetogohomeandiwillcallyouwhenigettheretonight",
            "I don't want to say goodbye but I have to go home and I will call you when I get there tonight",
        ),
    ] {
        let mut k = kernel();
        type_word(&mut k, raw);
        assert_eq!(k.render_state().composition, raw);
        if raw.len() > 64 {
            assert!(k.render_state().candidates.iter().any(|c| c.text == expected),
                "a long composition must still offer its completed final word");
        }
        assert_eq!(commits(&key(&mut k, Key::Space)), [expected], "{raw}");
    }

    let mut k = kernel();
    for mark in ['.', ',', '!', '?', ';', ':'] {
        type_word(&mut k, input);
        let actions = key(&mut k, Key::Char(mark));
        assert_eq!(commits(&actions), [format!("{restored}{mark}")]);
        assert!(!actions.contains(&KernelAction::PassThrough));
    }
    type_word(&mut k, input);
    key(&mut k, Key::Down);
    assert_eq!(commits(&key(&mut k, Key::Enter)), [input]);
    type_word(&mut k, "idontw");
    key(&mut k, Key::Backspace);
    assert_eq!(k.render_state().composition, "idont");
    assert!(k
        .render_state()
        .candidates
        .iter()
        .take(5)
        .any(|c| c.text == "I don't"));
    assert_eq!(commits(&key(&mut k, Key::Enter)), ["idont"]);

    for raw in ["iamt", "international", "idontwanttosayqxvv"] {
        type_word(&mut k, raw);
        assert_eq!(commits(&key(&mut k, Key::Space)), [raw]);
    }
    type_word(&mut k, "example");
    let actions = key(&mut k, Key::Char('.'));
    assert_eq!(commits(&actions), ["example"]);
    assert!(actions.contains(&KernelAction::PassThrough));
    k.handle(InputEvent::SetEnglishAutoSpace(false));
    type_word(&mut k, input);
    assert_eq!(commits(&key(&mut k, Key::Space)), [format!("{input} ")]);
    type_word(&mut k, input);
    let actions = key(&mut k, Key::Char('?'));
    assert_eq!(commits(&actions), [input]);
    assert!(actions.contains(&KernelAction::PassThrough));

    let full = "q".repeat(retype_english::MAX_COMPOSITION_BYTES);
    type_word(&mut k, &full);
    assert_eq!(k.render_state().composition, full);
    assert_eq!(commits(&key(&mut k, Key::Char('z'))), [full]);
    assert_eq!(commits(&key(&mut k, Key::Enter)), ["z"]);
}

#[test]
#[ignore = "requires the prepared immutable English dictionary; run by the Windows installer build"]
fn prepared_pinyin_tail_dictionary_and_modern_entities_keep_commit_semantics() {
    let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../dist/english-dictionaries");
    let image = std::fs::read_dir(&directory)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("english-e") && name.ends_with(".bin"))
        })
        .expect("prepared English frequency image");
    retype_english::install(&image).unwrap();
    check_continuous_contractions();
    for (input, chinese) in [("iwilldotheyiyang", "一样"), ("iwilldothetongyang", "同样")] {
        let mut k = kernel();
        type_word(&mut k, input);
        let state = k.render_state();
        let index = state
            .candidates
            .iter()
            .position(|candidate| candidate.text == "I will do the same")
            .expect("real mixed pinyin candidate");
        assert!(
            index < 5,
            "same should be visible on the first page: {index}"
        );
        let candidate = &state.candidates[index];
        assert_eq!(candidate.display_text(), "same");
        assert_eq!(candidate.consumed, input.len());
        let note = candidate.annotation.as_ref().unwrap();
        assert!(note.pinyin_match && note.pinyin_exact);
        assert_eq!(note.matched_chinese, chinese);
        assert!(!note.part_of_speech.is_empty());
        let actions = key(&mut k, Key::Char(char::from(b'1' + index as u8)));
        assert_eq!(commits(&actions), ["I will do the same"]);
        assert!(!actions.contains(&KernelAction::PassThrough));
        assert!(!k.has_composition());
        type_word(&mut k, input);
        key(&mut k, Key::Down);
        let actions = key(&mut k, Key::Enter);
        assert_eq!(commits(&actions), [input]);
        assert!(!actions.contains(&KernelAction::PassThrough));
        type_word(&mut k, input);
        assert_eq!(commits(&key(&mut k, Key::Space)), [input]);
        type_word(&mut k, input);
        for _ in 0..=index {
            key(&mut k, Key::Down);
        }
        assert_eq!(commits(&key(&mut k, Key::Space)), ["I will do the same"]);
        k.handle(InputEvent::SetEnglishPinyin(false));
        type_word(&mut k, input);
        assert!(k
            .render_state()
            .candidates
            .iter()
            .all(|c| c.text != "I will do the same"));
    }
    let mut k = kernel();
    type_word(&mut k, "xiang");
    let state = k.render_state();
    let pinyin: Vec<_> = state
        .candidates
        .iter()
        .filter(|c| c.annotation.as_ref().is_some_and(|a| a.pinyin_match))
        .collect();
    assert!(pinyin
        .iter()
        .take(5)
        .any(|c| c.text == "think" && c.annotation.as_ref().unwrap().matched_chinese == "想"));
    assert!(pinyin
        .iter()
        .take(5)
        .all(|c| !matches!(c.text.as_str(), "school" | "help" | "high")));
    for (word, chinese) in [("like", "像"), ("fragrant", "香"), ("elephant", "象")] {
        assert!(
            pinyin.iter().any(
                |c| c.text == word && c.annotation.as_ref().unwrap().matched_chinese == chinese
            ),
            "{word}"
        );
    }
    let mut k = kernel();
    type_word(&mut k, "anthropic");
    let state = k.render_state();
    assert!(state.candidates.iter().any(|c| {
        c.text == "Anthropic"
            && c.annotation
                .as_ref()
                .is_some_and(|a| a.entity_kind == "公司")
    }));
    assert!(state.candidates.iter().any(|c| {
        c.text == "anthropic"
            && c.annotation
                .as_ref()
                .is_some_and(|a| a.entity_kind.is_empty())
    }));
    assert_eq!(commits(&key(&mut k, Key::Enter)), ["anthropic"]);
}
