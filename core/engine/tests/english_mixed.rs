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
