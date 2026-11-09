//! Modern meanings use independent identities; explanatory text never commits.
use retype_engine::{offline_cloud, Kernel, KernelConfig};
use retype_types::*;
use std::{
    collections::HashSet,
    sync::{Arc, OnceLock},
    time::Duration,
};

fn kernel() -> Kernel {
    static PREPARED: OnceLock<()> = OnceLock::new();
    PREPARED.get_or_init(|| {
        let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../dist/english-dictionaries");
        let image = std::fs::read_dir(&directory)
            .expect("prepare English resources first")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("english-e") && name.ends_with(".bin"))
            })
            .expect("prepared English frequency image");
        retype_english::install(&image)
            .expect("load the prepared English image and modern catalogs");
    });
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
fn type_word(kernel: &mut Kernel, word: &str) {
    for c in word.chars() {
        key(kernel, Key::Char(c));
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
fn lexical(candidate: &Candidate) -> bool {
    candidate
        .annotation
        .as_ref()
        .is_some_and(|note| !note.lexical_sense_id.is_empty())
}

fn common_modern_abbreviations_offer_real_meanings_without_implicit_expansion() {
    for input in ["idk", "imo", "tbh", "lol"] {
        for accept in 0..3 {
            let mut k = kernel();
            type_word(&mut k, input);
            let state = k.render_state();
            let first = state
                .candidates
                .first()
                .expect("modern abbreviation candidate");
            assert!(lexical(first), "modern sense must lead {input}");
            let note = first.annotation.as_ref().unwrap();
            assert!(matches!(note.lexical_usage.as_str(), "网络" | "缩写"));
            assert!(!note.part_of_speech.is_empty() && !note.definition.is_empty());
            assert!(note.lexical_source.starts_with("https://"));
            if input == "idk" {
                assert_eq!(note.expanded_form, "I don't know");
            }
            let expansion = note.expanded_form.clone();
            let expected = first.text.clone();
            assert!(expected.eq_ignore_ascii_case(input));
            let actions = match accept {
                0 => key(&mut k, Key::Char('1')),
                1 => key(&mut k, Key::Tab),
                _ => k.handle(InputEvent::CandidateChosen { index: 0 }),
            };
            assert_eq!(commits(&actions), [expected.as_str()]);
            if !expansion.is_empty() {
                assert_ne!(expected, expansion);
            }
            assert!(!actions
                .iter()
                .any(|action| matches!(action, KernelAction::Side(SideEffect::Learn(_)))));
            assert!(!k.has_composition());
        }
    }
}

fn multiple_modern_senses_of_one_abbreviation_remain_independently_selectable() {
    let mut k = kernel();
    type_word(&mut k, "imo");
    let senses: Vec<_> = k
        .render_state()
        .candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            let note = candidate.annotation.as_ref()?;
            (!note.lexical_sense_id.is_empty())
                .then(|| (index, candidate.text.clone(), note.lexical_sense_id.clone()))
        })
        .collect();
    assert!(
        senses.len() >= 2,
        "common and other sourced senses must coexist"
    );
    assert_eq!(
        senses
            .iter()
            .map(|(_, _, id)| id)
            .collect::<HashSet<_>>()
            .len(),
        senses.len()
    );
    for (index, expected, _) in senses {
        let mut k = kernel();
        type_word(&mut k, "imo");
        let actions = k.handle(InputEvent::CandidateChosen { index });
        assert_eq!(commits(&actions), [expected.as_str()]);
        assert!(expected.eq_ignore_ascii_case("imo"));
    }
}

fn modern_candidates_preserve_raw_enter_space_and_spacing_toggle_channels() {
    for input in ["idk", "Idk", "IMO", "tbh"] {
        let mut k = kernel();
        type_word(&mut k, input);
        key(&mut k, Key::Down);
        let actions = key(&mut k, Key::Enter);
        assert_eq!(commits(&actions), [input]);
        assert!(!actions.contains(&KernelAction::PassThrough));
        type_word(&mut k, input);
        assert_eq!(commits(&key(&mut k, Key::Space)), [input]);
        k.handle(InputEvent::SetEnglishAutoSpace(false));
        type_word(&mut k, input);
        assert_eq!(
            commits(&key(&mut k, Key::Space)),
            [format!("{input} ").as_str()]
        );
        k.handle(InputEvent::SetEnglishAutoSpace(true));
        type_word(&mut k, "iamrobot");
        assert_eq!(commits(&key(&mut k, Key::Space)), ["I am robot"]);
    }
}

fn modern_catalog_keeps_continuous_and_common_ordinary_word_paths() {
    for (input, expected) in [("ia", "I am"), ("iam", "I am"), ("iamt", "I am trying to")] {
        let mut k = kernel();
        type_word(&mut k, input);
        assert!(
            k.render_state()
                .candidates
                .iter()
                .take(5)
                .any(|candidate| candidate.text == expected),
            "{input} retains {expected}"
        );
    }
    for input in ["apple", "image", "cap", "sick", "based", "cooked"] {
        let mut k = kernel();
        type_word(&mut k, input);
        let state = k.render_state();
        let first = state.candidates.first().unwrap();
        if matches!(input, "apple" | "image") {
            assert_eq!(first.text, input);
        }
        assert!(!lexical(first));
        assert!(first
            .annotation
            .as_ref()
            .is_none_or(|note| note.entity_kind.is_empty()));
        let ordinary = state
            .candidates
            .iter()
            .position(|candidate| {
                candidate.text == input
                    && !lexical(candidate)
                    && candidate
                        .annotation
                        .as_ref()
                        .is_none_or(|note| note.entity_kind.is_empty())
            })
            .expect("ordinary spelling remains available");
        assert!(ordinary < 5, "ordinary {input} remains on the first page");
        if let Some(slang) = state.candidates.iter().position(lexical) {
            assert!(
                ordinary < slang,
                "ordinary {input} precedes its modern sense"
            );
        }
    }
}

fn modern_word_case_and_explicit_network_initialisms_keep_distinct_meanings() {
    for (input, expected) in [
        ("rizz", "rizz"),
        ("Rizz", "Rizz"),
        ("RIZZ", "RIZZ"),
        ("DOOMSCROLLING", "DOOMSCROLLING"),
    ] {
        let mut k = kernel();
        type_word(&mut k, input);
        let state = k.render_state();
        let (index, candidate) = state
            .candidates
            .iter()
            .enumerate()
            .find(|(_, candidate)| lexical(candidate))
            .expect("modern dictionary word");
        assert_eq!(candidate.text, expected);
        let actions = k.handle(InputEvent::CandidateChosen { index });
        assert_eq!(commits(&actions), [expected]);
    }
    for input in ["goat", "GOAT", "SICK"] {
        let mut k = kernel();
        type_word(&mut k, input);
        let state = k.render_state();
        let first = state.candidates.first().unwrap();
        if input == "GOAT" {
            assert!(lexical(first));
            assert_eq!(
                first.annotation.as_ref().unwrap().expanded_form,
                "greatest of all time"
            );
            assert_eq!(first.text, "GOAT");
        } else {
            assert!(!lexical(first), "ordinary {input} keeps priority");
            let ordinary = state
                .candidates
                .iter()
                .position(|candidate| {
                    !lexical(candidate)
                        && candidate.text.eq_ignore_ascii_case(input)
                        && candidate
                            .annotation
                            .as_ref()
                            .is_none_or(|note| note.entity_kind.is_empty())
                })
                .expect("ordinary meaning");
            assert!(ordinary < 5);
            if let Some(slang) = state.candidates.iter().position(lexical) {
                assert!(ordinary < slang);
            }
        }
        key(&mut k, Key::Down);
        assert_eq!(commits(&key(&mut k, Key::Enter)), [input]);
        type_word(&mut k, input);
        assert_eq!(commits(&key(&mut k, Key::Space)), [input]);
    }
}

#[test]
#[ignore = "requires prepared immutable English resources; run by the Windows installer build"]
fn prepared_modern_dictionary_and_entities_keep_commit_semantics() {
    common_modern_abbreviations_offer_real_meanings_without_implicit_expansion();
    multiple_modern_senses_of_one_abbreviation_remain_independently_selectable();
    modern_candidates_preserve_raw_enter_space_and_spacing_toggle_channels();
    modern_catalog_keeps_continuous_and_common_ordinary_word_paths();
    modern_word_case_and_explicit_network_initialisms_keep_distinct_meanings();
}
