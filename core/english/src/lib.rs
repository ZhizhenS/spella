//! Read-only English vocabulary loaded by platform resource workers.
#![forbid(unsafe_code)]
use retype_types::{Candidate, CandidateLanguage, CandidateSource, LearningStore};
use std::collections::{BinaryHeap, HashSet};
mod continuous;
mod entities;
mod image;
pub mod lexicon;
mod modern;
pub use image::{compile, current, Vocabulary, MAGIC, MAX_BYTES};
pub use lexicon::set_manual_words;
/// Resource-worker entry point. A missing sidecar supports older installations.
/// Lookup methods below never open files or execute SQL.
pub fn install(path: &std::path::Path) -> Result<(), String> {
    let sidecar = path.with_file_name(lexicon::FILE_NAME);
    let annotations = if sidecar.try_exists().map_err(|error| error.to_string())? {
        lexicon::Lexicon::open(&sidecar)?
    } else {
        std::sync::Arc::new(lexicon::Lexicon::empty())
    };
    // Embedded catalogs are validated/indexed here on the resource worker,
    // never on the first production keystroke.
    modern::initialize()?;
    entities::initialize()?;
    image::install(path)?;
    lexicon::publish(annotations);
    Ok(())
}
pub fn valid_word(input: &str) -> bool {
    !input.is_empty()
        && input.len() <= 64
        && input.as_bytes()[0].is_ascii_alphabetic()
        && input.as_bytes()[input.len() - 1].is_ascii_alphabetic()
        && input.bytes().all(|b| b.is_ascii_alphabetic() || b == b'\'')
}
fn casing(word: &str, input: &str) -> String {
    if word != "i" && word.eq_ignore_ascii_case(input) {
        return input.to_owned();
    }
    if input.bytes().skip(1).any(|c| c.is_ascii_uppercase())
        && !input.bytes().all(|c| c.is_ascii_uppercase())
        && word.starts_with(&input.to_ascii_lowercase())
    {
        return format!("{input}{}", &word[input.len()..]);
    }
    let letters: Vec<_> = input.bytes().filter(u8::is_ascii_alphabetic).collect();
    if !letters.is_empty() && letters.iter().all(u8::is_ascii_uppercase) {
        return word.to_ascii_uppercase();
    }
    if letters.first().is_some_and(u8::is_ascii_uppercase) {
        let mut output = word.to_owned();
        if let Some(first) = output.get_mut(..1) {
            first.make_ascii_uppercase();
        }
        return output;
    }
    if word == "i" {
        "I".into()
    } else {
        word.into()
    }
}

fn is_entity(candidate: &Candidate) -> bool {
    candidate
        .annotation
        .as_ref()
        .is_some_and(|annotation| !annotation.entity_kind.is_empty())
}
fn is_lexical(candidate: &Candidate) -> bool {
    candidate
        .annotation
        .as_ref()
        .is_some_and(|note| !note.lexical_sense_id.is_empty())
}
fn candidate_identity(candidate: &Candidate) -> (String, u8, String, String) {
    let text = candidate.text.to_ascii_lowercase();
    match candidate.annotation.as_ref() {
        Some(note) if !note.lexical_sense_id.is_empty() => (
            text,
            2,
            note.lexical_source.clone(),
            note.lexical_sense_id.clone(),
        ),
        Some(note) if !note.entity_kind.is_empty() => {
            (text, 1, note.entity_source.clone(), String::new())
        }
        _ => (text, 0, String::new(), String::new()),
    }
}
fn conversational_modern(candidate: &Candidate) -> bool {
    candidate.score >= 1200.
        && candidate.annotation.as_ref().is_some_and(|note| {
            !note.lexical_sense_id.is_empty()
                && matches!(note.lexical_usage.as_str(), "网络" | "缩写")
        })
}

impl Vocabulary {
    fn lower_bound(&self, input: &str) -> usize {
        let (mut lo, mut hi) = (0, self.count());
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.word(mid) < input {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo
    }
    fn find(&self, input: &str) -> Option<usize> {
        let i = self.lower_bound(input);
        (i < self.count() && self.word(i) == input).then_some(i)
    }
    /// Dictionary evidence for mixed-language priority, separate from personal spellings.
    pub fn contains_word(&self, input: &str) -> bool {
        self.find(&input.to_ascii_lowercase()).is_some()
    }
    /// Conservatively insert internal spaces between complete typed words.
    /// None preserves the original spelling, including complete personal words.
    /// This does not complete/correct letters, learn text, or perform filesystem IO.
    pub fn segment_exact(&self, input: &str) -> Option<String> {
        continuous::segment_exact(self, input, &lexicon::current(), &lexicon::manual())
    }
    fn candidate(&self, id: usize, input: &str, lexicon: &lexicon::Lexicon) -> Candidate {
        let mut c = Candidate::new(casing(self.word(id), input), CandidateSource::Local);
        c.language = CandidateLanguage::English;
        c.consumed = input.len();
        c.score = (self.frequency(id) as f64).ln() as f32;
        if let Some(entry) = lexicon.lookup(self.word(id)) {
            c.comment = entry.comment();
            c.annotation = Some(entry.annotation(false));
        }
        c
    }
    /// Top-frequency prefix results via a precompiled range-max tree.
    pub fn complete(
        &self,
        input: &str,
        learner: &dyn LearningStore,
        limit: usize,
    ) -> Vec<Candidate> {
        self.complete_with_pinyin(input, learner, limit, true)
    }
    pub fn complete_with_pinyin(
        &self,
        input: &str,
        learner: &dyn LearningStore,
        limit: usize,
        enabled: bool,
    ) -> Vec<Candidate> {
        if limit == 0 || !valid_word(input) {
            return Vec::new();
        }
        self.complete_from(
            input,
            learner,
            limit,
            enabled,
            &lexicon::current(),
            &lexicon::manual(),
        )
    }
    /// Incremental English decoding. Each candidate still replaces the whole
    /// raw composition; only its display label may omit an inferred prefix.
    pub fn complete_input(
        &self,
        input: &str,
        learner: &dyn LearningStore,
        limit: usize,
        pinyin: bool,
        segmentation: bool,
        phrases: bool,
    ) -> Vec<Candidate> {
        if limit == 0 || !valid_word(input) {
            return Vec::new();
        }
        self.complete_input_from(
            input,
            learner,
            limit,
            (pinyin, segmentation, phrases),
            &lexicon::current(),
            &lexicon::manual(),
        )
    }
    fn complete_input_from(
        &self,
        input: &str,
        learner: &dyn LearningStore,
        limit: usize,
        options: (bool, bool, bool),
        metadata: &lexicon::Lexicon,
        manual: &lexicon::ManualWords,
    ) -> Vec<Candidate> {
        let (pinyin, segmentation, phrases) = options;
        let plain = self.complete_from(input, learner, limit, pinyin, metadata, manual);
        if !segmentation
            || limit == 0
            || plain.iter().any(|candidate| {
                candidate.source == CandidateSource::User
                    && candidate.text.eq_ignore_ascii_case(input)
            })
        {
            return plain;
        }
        let segmented =
            continuous::complete_with_pinyin(self, input, limit, phrases, pinyin, metadata);
        if segmented.is_empty() {
            return plain;
        }
        // Preserve explicit personal words, literal dictionary words, and the
        // established exact-pinyin stream before competing model hypotheses.
        let primary_pinyin = plain.first().is_some_and(|candidate| {
            candidate
                .annotation
                .as_ref()
                .is_some_and(|note| note.pinyin_match)
        }) && !self.contains_word(input);
        let mut pinned = Vec::new();
        let mut ranked = Vec::new();
        let mut independent = Vec::new();
        let explicit_case = input
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_uppercase());
        let explicit_abbreviation =
            input.len() >= 2 && input.bytes().all(|byte| byte.is_ascii_uppercase());
        let ordinary_common = self.ordinary_common(input, &plain);
        for mut candidate in plain {
            if is_entity(&candidate) || is_lexical(&candidate) {
                if (is_entity(&candidate) && explicit_case
                    || is_lexical(&candidate) && explicit_abbreviation)
                    || (conversational_modern(&candidate)
                        && !ordinary_common
                        && candidate.text.eq_ignore_ascii_case(input))
                {
                    pinned.push(candidate);
                } else {
                    // A low-priority technical abbreviation must not lock a
                    // reliable lowercase English path such as `I am` out.
                    independent.push(candidate);
                }
                continue;
            }
            if candidate.source == CandidateSource::User
                || candidate.text.eq_ignore_ascii_case(input)
                || (primary_pinyin
                    && candidate
                        .annotation
                        .as_ref()
                        .is_some_and(|note| note.pinyin_match))
            {
                pinned.push(candidate);
            } else {
                candidate.score = continuous::score(self, &candidate.text, input.len());
                ranked.push(candidate);
            }
        }
        ranked.extend(segmented);
        ranked.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then_with(|| a.text.cmp(&b.text))
        });
        if independent.is_empty() {
            pinned.extend(ranked);
        } else {
            // Keep language-model scores separate from dictionary priorities,
            // while making a few modern meanings visible on the first page.
            let mut ranked = ranked.into_iter();
            let leading_model_slots = if independent.iter().any(conversational_modern) {
                3
            } else {
                5
            };
            pinned.extend(ranked.by_ref().take(leading_model_slots));
            pinned.extend(independent);
            pinned.extend(ranked);
        }
        let mut seen = HashSet::new();
        pinned.retain(|candidate| seen.insert(candidate_identity(candidate)));
        pinned.truncate(limit);
        pinned
    }
    fn complete_from(
        &self,
        input: &str,
        learner: &dyn LearningStore,
        limit: usize,
        enabled: bool,
        lexicon: &lexicon::Lexicon,
        manual: &lexicon::ManualWords,
    ) -> Vec<Candidate> {
        if limit == 0 || !valid_word(input) {
            return Vec::new();
        }
        let key = input.to_ascii_lowercase();
        let lo = self.lower_bound(&key);
        let hi = self.lower_bound(&format!("{key}{{"));
        let (mut l, mut r) = (lo + self.size(), hi + self.size());
        let mut heap = BinaryHeap::new();
        let push = |heap: &mut BinaryHeap<_>, node: usize| {
            let id = self.u32_at(16 + self.count() * 16 + node * 4);
            if id < self.count() {
                heap.push((self.frequency(id), std::cmp::Reverse(id), node));
            }
        };
        while l < r {
            if l % 2 == 1 {
                push(&mut heap, l);
                l += 1;
            }
            if r % 2 == 1 {
                r -= 1;
                push(&mut heap, r);
            }
            l /= 2;
            r /= 2;
        }
        let mut hits = Vec::new();
        while hits.len() < limit {
            let Some((_, _, node)) = heap.pop() else {
                break;
            };
            if node >= self.size() {
                hits.push(self.candidate(node - self.size(), input, lexicon));
            } else {
                push(&mut heap, node * 2);
                push(&mut heap, node * 2 + 1);
            }
        }
        // Empirical personal counts form a smoothed distribution, rather than a
        // fixed per-selection bonus. Keep the vocabulary prior for known words.
        let total: f64 = hits
            .iter()
            .map(|c| f64::from(c.score).exp())
            .sum::<f64>()
            .max(1.);
        for c in &mut hits {
            c.score -= total.ln() as f32;
        }
        for (text, uses) in learner.english_words(&key, limit) {
            let normalized = text.to_ascii_lowercase();
            let prior = self
                .find(&normalized)
                .map_or(0., |id| self.frequency(id) as f64);
            let score = (prior / total + uses as f64).ln() as f32;
            let text = casing(&text, input);
            if let Some(existing) = hits.iter_mut().find(|c| c.text == text) {
                existing.score = score;
                existing.source = CandidateSource::User;
            } else {
                let mut c = Candidate::new(text, CandidateSource::User);
                c.language = CandidateLanguage::English;
                c.consumed = input.len();
                c.score = score;
                hits.push(c);
            }
        }
        for entry in lexicon.supplementary(input, limit, |word| self.find(word).is_none()) {
            if hits
                .iter()
                .any(|candidate| candidate.text.eq_ignore_ascii_case(entry.word))
            {
                continue;
            }
            let mut candidate = Candidate::new(casing(entry.word, input), CandidateSource::Local);
            candidate.language = CandidateLanguage::English;
            candidate.consumed = input.len();
            candidate.comment = entry.comment();
            candidate.annotation = Some(entry.annotation(false));
            candidate.score = -20. - (entry.rank as f64).ln() as f32;
            hits.push(candidate);
        }
        hits.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then_with(|| a.text.cmp(&b.text))
        });
        if self.find(&key).is_none() && lexicon.lookup(&key).is_some() {
            hits.sort_by_key(|candidate| !candidate.text.eq_ignore_ascii_case(input));
        }
        let mut personal = manual.complete(input, limit);
        let personal_keys: HashSet<_> = personal
            .iter()
            .map(|candidate| candidate.text.to_ascii_lowercase())
            .collect();
        hits.retain(|candidate| !personal_keys.contains(&candidate.text.to_ascii_lowercase()));
        personal.extend(hits);
        let ordinary = self.assist(input, personal, limit, enabled, lexicon);
        self.with_entities(input, ordinary, limit)
    }

    fn with_entities(&self, input: &str, ordinary: Vec<Candidate>, limit: usize) -> Vec<Candidate> {
        let names = entities::complete(input, limit);
        let lexical = modern::complete(input, limit);
        self.with_modern_streams(input, ordinary, lexical, names, limit)
    }

    fn with_modern_streams(
        &self,
        input: &str,
        ordinary: Vec<Candidate>,
        mut lexical: Vec<modern::Match>,
        names: Vec<Candidate>,
        limit: usize,
    ) -> Vec<Candidate> {
        if names.is_empty() && lexical.is_empty() {
            return ordinary;
        }
        let exact_name = names
            .iter()
            .any(|candidate| candidate.text.eq_ignore_ascii_case(input));
        let ordinary_common = self.ordinary_common(input, &ordinary);
        let explicit_abbreviation =
            input.len() >= 2 && input.bytes().all(|byte| byte.is_ascii_uppercase());
        let promote_lexical = lexical.iter().any(|hit| {
            if !hit.exact {
                return false;
            }
            let reviewed = conversational_modern(&hit.candidate) && hit.priority >= 1200;
            let explicit = explicit_abbreviation
                && hit.candidate.annotation.as_ref().is_some_and(|note| {
                    !note.expanded_form.is_empty()
                        && matches!(note.lexical_usage.as_str(), "网络" | "缩写")
                });
            // Uppercase alone is not evidence that APPLE's slang meanings are
            // abbreviations. Overriding a common word also requires review.
            !ordinary_common && (reviewed || explicit) || explicit && reviewed
        });
        let promote = !names.is_empty()
            && (input
                .bytes()
                .next()
                .is_some_and(|letter| letter.is_ascii_uppercase())
                || ordinary.is_empty()
                || (exact_name && !ordinary_common));
        let (mut personal, ordinary): (Vec<_>, Vec<_>) = ordinary
            .into_iter()
            .partition(|candidate| candidate.source == CandidateSource::User);
        // An explicit personal definition overrides dictionary meanings of its
        // spelling; separately identified companies/products may still coexist.
        let personal_keys: HashSet<_> = personal
            .iter()
            .map(|candidate| candidate.text.to_ascii_lowercase())
            .collect();
        lexical.retain(|hit| !personal_keys.contains(&hit.candidate.text.to_ascii_lowercase()));
        let lexical: Vec<_> = lexical.into_iter().map(|hit| hit.candidate).collect();
        let ordinary_leads = !promote_lexical && !promote;
        let mut lanes = if promote_lexical {
            [lexical, ordinary, names]
        } else if promote {
            [names, ordinary, lexical]
        } else {
            [ordinary, names, lexical]
        };
        let available = limit.saturating_sub(personal.len());
        let reserve = lanes.iter().skip(1).filter(|lane| !lane.is_empty()).count();
        let minimum = usize::from(!lanes[0].is_empty() && (personal.is_empty() || !ordinary_leads));
        let head = available.saturating_sub(reserve).max(minimum).min(
            if ordinary_leads && !ordinary_common {
                3
            } else {
                limit
            },
        );
        let mut primary = lanes[0].drain(..).collect::<Vec<_>>().into_iter();
        personal.extend(primary.by_ref().take(head));
        for lane in lanes.iter_mut().skip(1) {
            if !lane.is_empty() {
                personal.push(lane.remove(0));
            }
        }
        personal.extend(primary);
        for lane in lanes.into_iter().skip(1) {
            personal.extend(lane);
        }
        let mut seen = HashSet::new();
        personal.retain(|candidate| seen.insert(candidate_identity(candidate)));
        personal.truncate(limit);
        personal
    }

    fn ordinary_common(&self, input: &str, ordinary: &[Candidate]) -> bool {
        self.find(&input.to_ascii_lowercase()).is_some_and(|id| {
            self.frequency(id) as f64 / self.total_frequency().max(1) as f64 >= 0.000_001
        }) && !ordinary
            .iter()
            .filter(|candidate| {
                candidate.text.eq_ignore_ascii_case(input)
                    && !is_entity(candidate)
                    && !is_lexical(candidate)
            })
            .any(|candidate| {
                candidate.annotation.as_ref().is_some_and(|note| {
                    let pos = note.part_of_speech.to_ascii_lowercase();
                    (pos.contains("abbr") || pos.contains("缩写"))
                        && !["n.", "v.", "adj.", "adv."]
                            .iter()
                            .any(|ordinary| pos.contains(ordinary))
                })
            })
    }

    fn assist(
        &self,
        input: &str,
        mut hits: Vec<Candidate>,
        limit: usize,
        enabled: bool,
        lexicon: &lexicon::Lexicon,
    ) -> Vec<Candidate> {
        for candidate in &mut hits {
            if candidate.source == CandidateSource::User && !candidate.comment.is_empty()
                || is_entity(candidate)
                || is_lexical(candidate)
            {
                continue;
            }
            if let Some(entry) = lexicon.lookup(&candidate.text) {
                candidate.comment = entry.comment();
                candidate.annotation = Some(entry.annotation(false));
            }
        }
        if !enabled {
            hits.truncate(limit);
            return hits;
        }
        let (exact, pinyin) = lexicon.pinyin(input, limit);
        if pinyin.is_empty() {
            hits.truncate(limit);
            return hits;
        }
        let pinyin: Vec<_> = pinyin
            .into_iter()
            .map(|entry| {
                let mut candidate =
                    Candidate::new(casing(entry.word, input), CandidateSource::Local);
                candidate.language = CandidateLanguage::English;
                candidate.consumed = input.len();
                candidate.comment = entry.comment();
                candidate.annotation = Some(entry.annotation(true));
                candidate.score = self
                    .find(entry.word)
                    .map_or(-20., |id| (self.frequency(id) as f64).ln() as f32);
                candidate
            })
            .collect();
        let mut combined = Vec::new();
        // Preserve actual English words (he/an/you) when they are also pinyin.
        // Complete pinyin such as nihao/pingguo must reach its English meanings.
        let known_english = self.contains_word(input)
            || lexicon.lookup(input).is_some()
            || hits.iter().any(|candidate| {
                candidate.source == CandidateSource::User
                    && candidate.text.eq_ignore_ascii_case(input)
            });
        if exact && !known_english {
            combined.extend(pinyin);
            combined.extend(hits);
        } else {
            // Keep English first, reserving a small tail for prefix lookup so a
            // large English prefix list cannot permanently hide pinyin results.
            let reserved = pinyin.len().min(4).min(limit);
            let english_slots = if hits.is_empty() {
                0
            } else {
                limit.saturating_sub(reserved).max(1)
            };
            let tail = hits.split_off(hits.len().min(english_slots));
            combined.extend(hits);
            combined.extend(pinyin);
            combined.extend(tail);
        }
        let mut seen = HashSet::new();
        combined.retain(|candidate| seen.insert(candidate.text.to_ascii_lowercase()));
        combined.truncate(limit);
        combined
    }

    /// A small mixed-language stream, exact spelling before frequency-ranked completions.
    /// Exact matches are looked up separately so a high-frequency completion cannot hide one.
    pub fn mixed(&self, input: &str, learner: &dyn LearningStore, limit: usize) -> Vec<Candidate> {
        if input.len() < 2 || limit == 0 || !valid_word(input) {
            return Vec::new();
        }
        let mut hits = self.complete(input, learner, limit);
        if let Some(id) = self.find(&input.to_ascii_lowercase()) {
            if !hits.iter().any(|c| c.text.eq_ignore_ascii_case(input)) {
                hits.push(self.candidate(id, input, &lexicon::current()));
            }
        }
        hits.sort_by_key(|c| !c.text.eq_ignore_ascii_case(input));
        hits.truncate(limit);
        hits
    }
    /// One-edit spelling suggestions (insertion/deletion/substitution/transposition).
    /// Called on the background worker, never while holding the kernel lock.
    pub fn correct(&self, input: &str, limit: usize) -> Vec<Candidate> {
        if limit == 0 || !valid_word(input) || input.len() < 3 {
            return Vec::new();
        }
        let lexicon = lexicon::current();
        let key = input.to_ascii_lowercase();
        let mut ids = HashSet::new();
        let mut add = |s: &str| {
            if s != key {
                if let Some(id) = self.find(s) {
                    ids.insert(id);
                }
            }
        };
        for i in 0..key.len() {
            add(&format!("{}{}", &key[..i], &key[i + 1..]));
            if i + 1 < key.len() {
                let mut swapped = key.as_bytes().to_vec();
                swapped.swap(i, i + 1);
                if let Ok(s) = std::str::from_utf8(&swapped) {
                    add(s);
                }
            }
            for c in b'a'..=b'z' {
                add(&format!("{}{}{}", &key[..i], c as char, &key[i + 1..]));
            }
        }
        for i in 0..=key.len() {
            for c in b'a'..=b'z' {
                add(&format!("{}{}{}", &key[..i], c as char, &key[i..]));
            }
        }
        let mut ids: Vec<_> = ids.into_iter().collect();
        ids.sort_by(|a, b| {
            self.frequency(*b)
                .cmp(&self.frequency(*a))
                .then_with(|| a.cmp(b))
        });
        ids.into_iter()
            .take(limit)
            .map(|id| {
                let mut c = self.candidate(id, input, &lexicon);
                if c.comment.is_empty() {
                    c.comment = "拼写纠正".into();
                }
                c
            })
            .collect()
    }
}

pub fn complete(input: &str, learner: &dyn LearningStore, limit: usize) -> Vec<Candidate> {
    current().complete(input, learner, limit)
}
pub fn mixed(input: &str, learner: &dyn LearningStore, limit: usize) -> Vec<Candidate> {
    current().mixed(input, learner, limit)
}
pub fn correct(input: &str, limit: usize) -> Vec<Candidate> {
    current().correct(input, limit)
}
#[cfg(test)]
fn count() -> usize {
    current().count()
}
#[cfg(test)]
fn word(id: usize) -> String {
    current().word(id).to_owned()
}
#[cfg(test)]
fn frequency(id: usize) -> u64 {
    current().frequency(id)
}
#[cfg(test)]
fn find(input: &str) -> Option<usize> {
    current().find(input)
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Empty;
    impl LearningStore for Empty {
        fn record(&self, _: retype_types::LearningEvent) {}
    }
    fn small_vocabulary(source: &[u8]) -> Vocabulary {
        let mut bytes = Vec::new();
        compile(source, &mut bytes).unwrap();
        Vocabulary::from_bytes(bytes).unwrap()
    }
    fn lexical_hit(text: &str, sense: &str, usage: &str, priority: u32) -> modern::Match {
        let mut candidate = Candidate::new(text, CandidateSource::Local);
        candidate.language = CandidateLanguage::English;
        candidate.score = priority as f32;
        candidate.annotation = Some(retype_types::EnglishAnnotation {
            part_of_speech: "短语".into(),
            definition: format!("中文义项{sense}"),
            lexical_usage: usage.into(),
            lexical_source: "https://example.org/lexicon".into(),
            lexical_sense_id: sense.into(),
            expanded_form: "in my opinion".into(),
            ..Default::default()
        });
        modern::Match {
            candidate,
            exact: true,
            priority,
        }
    }
    #[test]
    fn modern_meaning_identity_and_priority_preserve_polysemy_and_user_overrides() {
        let vocab = small_vocabulary(b"imo 10\nthe 100000000\n");
        let mut old = Candidate::new("imo", CandidateSource::Local);
        old.annotation = Some(retype_types::EnglishAnnotation {
            part_of_speech: "abbr.".into(),
            definition: "旧缩写含义".into(),
            ..Default::default()
        });
        let meanings = || {
            vec![
                lexical_hit("IMO", "chat", "缩写", 2200),
                lexical_hit("IMO", "maritime", "专业", 500),
                lexical_hit("IMO", "math", "专业", 400),
            ]
        };
        let hits = vocab.with_modern_streams("imo", vec![old.clone()], meanings(), Vec::new(), 24);
        assert_eq!(hits.len(), 4);
        assert_eq!(
            hits[0].annotation.as_ref().unwrap().lexical_sense_id,
            "chat"
        );
        assert_eq!(
            hits.iter()
                .map(candidate_identity)
                .collect::<HashSet<_>>()
                .len(),
            4
        );
        for sense in ["chat", "maritime", "math"] {
            assert!(hits.iter().any(|candidate| candidate
                .annotation
                .as_ref()
                .is_some_and(|note| note.lexical_sense_id == sense)));
        }
        assert!(hits
            .iter()
            .all(|candidate| candidate.text.eq_ignore_ascii_case("imo")));
        let mut personal = Candidate::new("IMO", CandidateSource::User);
        personal.comment = "显式个人定义".into();
        let hits = vocab.with_modern_streams("imo", vec![personal], meanings(), Vec::new(), 1);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].source, CandidateSource::User);
        assert_eq!(hits[0].comment, "显式个人定义");
        let mut one = Candidate::new("IMO", CandidateSource::Local);
        one.annotation = Some(retype_types::EnglishAnnotation {
            entity_kind: "组织".into(),
            entity_source: "Wikidata Q1@1".into(),
            ..Default::default()
        });
        let mut two = one.clone();
        two.annotation.as_mut().unwrap().entity_source = "Wikidata Q2@1".into();
        assert_ne!(candidate_identity(&one), candidate_identity(&two));
        let hits = vocab.with_modern_streams("imo", vec![old], meanings(), vec![one, two], 24);
        assert_eq!(hits.len(), 6);
    }

    #[test]
    fn modern_informal_meanings_do_not_displace_common_ordinary_words() {
        let vocab = small_vocabulary(b"apple 1000000\nbased 1000000\ncap 1000000\ncooked 1000000\nsick 1000000\nthe 1000000\n");
        for word in ["apple", "based", "cap", "cooked", "sick"] {
            let mut ordinary = Candidate::new(word, CandidateSource::Local);
            ordinary.annotation = Some(retype_types::EnglishAnnotation {
                part_of_speech: "n. / v. / adj.".into(),
                definition: "常见普通含义".into(),
                ..Default::default()
            });
            for limit in [1, 2, 5, 24] {
                let hits = vocab.with_modern_streams(
                    word,
                    vec![ordinary.clone()],
                    vec![lexical_hit(word, "informal", "网络", 2400)],
                    Vec::new(),
                    limit,
                );
                assert!(!is_lexical(&hits[0]), "ordinary {word} must lead");
                assert!(hits.len() <= limit);
                if limit > 1 {
                    assert!(hits.iter().any(is_lexical));
                }
            }
        }
        let technical = lexical_hit("IAM", "identity-access", "缩写", 500);
        assert!(!conversational_modern(&technical.candidate));
    }

    #[test]
    fn uppercase_network_initialism_requires_explicit_expansion_to_override_a_common_word() {
        let vocab = small_vocabulary(b"goat 1000000\nsick 1000000\nthe 1000000\n");
        let ordinary = |text: &str| {
            let mut candidate = Candidate::new(text, CandidateSource::Local);
            candidate.annotation = Some(retype_types::EnglishAnnotation {
                part_of_speech: "n. / adj.".into(),
                definition: "普通动物或形容词含义".into(),
                ..Default::default()
            });
            candidate
        };
        for input in ["goat", "GOAT"] {
            let mut modern = lexical_hit("GOAT", "greatest", "缩写", 2000);
            modern.candidate.annotation.as_mut().unwrap().expanded_form =
                "greatest of all time".into();
            let hits = vocab.with_modern_streams(
                input,
                vec![ordinary(input)],
                vec![modern],
                Vec::new(),
                5,
            );
            assert_eq!(is_lexical(&hits[0]), input == "GOAT");
            assert_eq!(hits.len(), 2);
        }
        for (usage, priority, expansion) in [
            ("俚语", 2400, ""),
            ("缩写", 2400, ""),
            ("缩写", 500, "example expansion"),
        ] {
            let mut modern = lexical_hit("SICK", "cool", usage, priority);
            modern.candidate.annotation.as_mut().unwrap().expanded_form = expansion.into();
            let hits = vocab.with_modern_streams(
                "SICK",
                vec![ordinary("SICK")],
                vec![modern],
                Vec::new(),
                5,
            );
            assert!(
                !is_lexical(&hits[0]),
                "{usage}/{priority}/{expansion} cannot displace a common word"
            );
        }
    }

    #[test]
    fn uppercase_without_abbreviation_evidence_keeps_supplementary_ordinary_meaning_before_slang() {
        let vocab = small_vocabulary(b"hello 100\nhelp 50\n");
        let mut ordinary = Candidate::new("APPLE", CandidateSource::Local);
        ordinary.annotation = Some(retype_types::EnglishAnnotation {
            part_of_speech: "n.".into(),
            definition: "苹果".into(),
            ..Default::default()
        });
        let mut company = Candidate::new("Apple", CandidateSource::Local);
        company.annotation = Some(retype_types::EnglishAnnotation {
            entity_kind: "公司".into(),
            entity_source: "Wikidata Q312@1".into(),
            ..Default::default()
        });
        for usage in ["俚语", "非正式", "网络", "缩写"] {
            for names in [Vec::new(), vec![company.clone()]] {
                let mut cold = lexical_hit("APPLE", "radio", usage, 500);
                cold.candidate
                    .annotation
                    .as_mut()
                    .unwrap()
                    .expanded_form
                    .clear();
                let hits = vocab.with_modern_streams(
                    "APPLE",
                    vec![ordinary.clone()],
                    vec![cold],
                    names,
                    5,
                );
                let normal = hits
                    .iter()
                    .position(|candidate| !is_entity(candidate) && !is_lexical(candidate))
                    .unwrap();
                let modern = hits.iter().position(is_lexical).unwrap();
                assert!(normal < modern && normal < 5, "{usage}: {hits:?}");
                assert_eq!(hits[normal].annotation.as_ref().unwrap().definition, "苹果");
            }
        }
    }

    #[test]
    fn modern_catalog_does_not_lock_existing_continuous_and_whole_word_paths() {
        let vocab = current();
        for (input, expected) in [("ia", "I am"), ("iam", "I am"), ("iamt", "I am trying to")] {
            let hits = vocab.complete_input(input, &Empty, 24, true, true, true);
            assert!(
                hits.iter()
                    .take(5)
                    .any(|candidate| candidate.text == expected),
                "{input} must retain {expected}"
            );
        }
        for input in ["iamrobot", "image"] {
            let hits = vocab.complete_input(input, &Empty, 24, true, true, true);
            assert!(!hits.is_empty());
            if input == "image" {
                assert_eq!(hits[0].text, "image");
            }
        }
    }
    #[test]
    fn joined_pinyin_options_and_exact_personal_spelling_are_independent() {
        let vocab = current();
        let metadata = lexicon::fixture();
        let empty = lexicon::ManualWords::default();
        let input = "iwilldotheyiyang";
        for options in [
            (false, true, true),
            (true, false, true),
            (false, false, false),
        ] {
            let hits = vocab.complete_input_from(input, &Empty, 5, options, &metadata, &empty);
            assert!(!hits
                .iter()
                .any(|candidate| candidate.text == "I will do the same"));
        }
        let hits =
            vocab.complete_input_from(input, &Empty, 5, (true, true, false), &metadata, &empty);
        assert!(hits
            .iter()
            .take(5)
            .any(|candidate| candidate.text == "I will do the same"));
        let manual =
            lexicon::ManualWords::validate(vec![(input.into(), "显式保留拼写".into())]).unwrap();
        let hits =
            vocab.complete_input_from(input, &Empty, 5, (true, true, true), &metadata, &manual);
        assert_eq!(hits[0].text, input);
        assert_eq!(hits[0].comment, "显式保留拼写");
        assert!(hits
            .iter()
            .all(|candidate| candidate.display_text.is_none()));
        struct Personal;
        impl LearningStore for Personal {
            fn record(&self, _: retype_types::LearningEvent) {
                panic!("completion must not learn");
            }
            fn english_words(&self, _: &str, _: usize) -> Vec<(String, u64)> {
                vec![("iwilldotheyiyang".into(), 2)]
            }
        }
        let hits =
            vocab.complete_input_from(input, &Personal, 5, (true, true, true), &metadata, &empty);
        assert_eq!(hits[0].text, input);
        assert_eq!(hits[0].source, CandidateSource::User);
        assert!(hits
            .iter()
            .all(|candidate| candidate.display_text.is_none()));
    }
    #[test]
    fn modern_entities_keep_canonical_names_and_distinct_common_word_meanings() {
        let vocab = current();
        for input in ["anthropic", "Anthr", "ANTHROPIC"] {
            let hits = vocab.complete_input(input, &Empty, 5, false, true, true);
            assert_eq!(hits[0].text, "Anthropic");
            assert!(is_entity(&hits[0]));
            assert!(hits[0]
                .annotation
                .as_ref()
                .unwrap()
                .entity_source
                .contains("Q116758847"));
        }
        let hits = vocab.complete_input("anthropic", &Empty, 5, false, true, true);
        assert!(
            hits.iter().any(|candidate| candidate.text == "anthropic"
                && !is_entity(candidate)
                && !is_lexical(candidate)),
            "ordinary adjective must remain independently available"
        );
        let hits = vocab.complete_input("apple", &Empty, 5, false, true, true);
        assert_eq!(hits[0].text, "apple");
        assert!(!is_entity(&hits[0]) && !is_lexical(&hits[0]));
        assert!(hits
            .iter()
            .any(|candidate| candidate.text == "Apple" && is_entity(candidate)));
        assert!(is_entity(&vocab.complete("Apple", &Empty, 1)[0]));
        let ordinary = vocab.complete("apple", &Empty, 1);
        assert!(!is_entity(&ordinary[0]) && !is_lexical(&ordinary[0]));
        let manual =
            lexicon::ManualWords::validate(vec![("Apple".into(), "我的显式词条".into())]).unwrap();
        let hits = vocab.complete_input_from(
            "Apple",
            &Empty,
            5,
            (true, true, true),
            &lexicon::fixture(),
            &manual,
        );
        assert_eq!(hits[0].comment, "我的显式词条");
        assert_eq!(hits[0].source, CandidateSource::User);
        assert!(!is_entity(&hits[0]) && !is_lexical(&hits[0]));
        for input in ["app", "App", "APP"] {
            for limit in [1, 2, 5] {
                let hits = vocab.complete_input_from(
                    input,
                    &Empty,
                    limit,
                    (true, true, true),
                    &lexicon::fixture(),
                    &manual,
                );
                assert_eq!(
                    hits[0].source,
                    CandidateSource::User,
                    "personal prefix priority for {input}"
                );
                assert_eq!(hits[0].comment, "我的显式词条");
                assert!(!is_entity(&hits[0]) && !is_lexical(&hits[0]));
                assert!(hits.len() <= limit);
                if limit > 1 {
                    assert!(
                        hits.iter()
                            .skip(1)
                            .any(|candidate| candidate.text == "Apple" && is_entity(candidate)),
                        "entity meaning remains independently available for {input}"
                    );
                }
            }
        }
    }
    #[test]
    fn known_english_keeps_a_slot_even_with_one_or_two_candidate_limits() {
        let vocabulary = small_vocabulary(b"nihao 100\nnihaoz 10\n");
        let lexicon = lexicon::fixture();
        for limit in [1, 2, 5] {
            let hits = vocabulary.complete_from(
                "nihao",
                &Empty,
                limit,
                true,
                &lexicon,
                &lexicon::ManualWords::default(),
            );
            assert_eq!(hits[0].text, "nihao");
            assert!(hits.len() <= limit);
        }
    }
    #[test]
    fn supplemental_words_manual_overrides_missing_metadata_and_zero_limits() {
        let vocabulary = small_vocabulary(b"hello 100\nhelp 50\n");
        let lexicon = lexicon::fixture();
        for (input, word) in [("app", "apple"), ("APPLE", "APPLE")] {
            let hits = vocabulary.complete_from(
                input,
                &Empty,
                5,
                false,
                &lexicon,
                &lexicon::ManualWords::default(),
            );
            let (ordinary_index, ordinary) = hits
                .iter()
                .enumerate()
                .find(|(_, candidate)| {
                    candidate.text == word && !is_entity(candidate) && !is_lexical(candidate)
                })
                .unwrap();
            assert!(
                ordinary.comment.contains("苹果"),
                "input={input}, hits={hits:?}"
            );
            assert!(ordinary_index < 5);
            if let Some(slang) = hits.iter().position(is_lexical) {
                assert!(
                    ordinary_index < slang,
                    "ordinary definition precedes supplemental slang for {input}: {hits:?}"
                );
            }
        }
        let manual =
            lexicon::ManualWords::validate(vec![("Apple".into(), "显式个人备注".into())]).unwrap();
        let hits = vocabulary.complete_from("app", &Empty, 5, true, &lexicon, &manual);
        assert_eq!(
            hits.iter()
                .filter(|candidate| candidate.text.eq_ignore_ascii_case("apple")
                    && !is_entity(candidate)
                    && !is_lexical(candidate))
                .count(),
            1
        );
        assert_eq!(hits[0].comment, "显式个人备注");
        assert_eq!(hits[0].source, CandidateSource::User);
        assert!(!is_entity(&hits[0]) && !is_lexical(&hits[0]));
        assert_eq!(
            hits[0].annotation.as_ref().unwrap().definition,
            "显式个人备注"
        );
        let entity = hits
            .iter()
            .find(|candidate| candidate.text == "Apple" && is_entity(candidate))
            .expect("company meaning is separate from the personal word");
        assert!(entity
            .annotation
            .as_ref()
            .unwrap()
            .entity_source
            .contains("Q312"));
        assert_ne!(entity.comment, "显式个人备注");
        let hits = vocabulary.complete_from(
            "hel",
            &Empty,
            5,
            true,
            &lexicon::Lexicon::empty(),
            &lexicon::ManualWords::default(),
        );
        assert_eq!(hits[0].text, "hello");
        assert!(hits[0].annotation.is_none());
        struct NoRead;
        impl LearningStore for NoRead {
            fn record(&self, _: retype_types::LearningEvent) {}
            fn english_words(&self, _: &str, _: usize) -> Vec<(String, u64)> {
                panic!("zero-limit requests must not read personal data");
            }
        }
        assert!(vocabulary
            .complete_from("hel", &NoRead, 0, true, &lexicon, &manual)
            .is_empty());
        assert!(vocabulary.correct("hellp", 0).is_empty());
    }
    #[test]
    fn pinyin_meanings_merge_with_english_without_entering_committed_text() {
        let lexicon = lexicon::fixture();
        let vocabulary = current();
        for (input, expected) in [("nihao", "hello"), ("pingguo", "apple"), ("pingg", "apple")] {
            let english = vocabulary.complete_with_pinyin(input, &Empty, 24, false);
            let hits = vocabulary.assist(input, english.clone(), 24, true, &lexicon);
            assert!(hits.iter().any(|candidate| candidate.text == expected));
            assert_eq!(hits[0].text, expected);
            assert!(hits
                .iter()
                .all(|candidate| candidate.consumed == input.len()));
            assert!(hits
                .iter()
                .all(|candidate| !candidate.text.chars().any(|c| c > '\u{7f}')));
            assert_eq!(
                vocabulary
                    .assist(input, english.clone(), 24, false, &lexicon)
                    .len(),
                english.len()
            );
        }
        let mut candidate = Candidate::new("apple", CandidateSource::Local);
        candidate.language = CandidateLanguage::English;
        let hits = vocabulary.assist("pingguo", vec![candidate], 24, true, &lexicon);
        assert_eq!(
            hits.iter()
                .filter(|candidate| candidate.text == "apple")
                .count(),
            1
        );
        assert_eq!(hits[0].comment, "n. 苹果；苹果树");
        assert_eq!(hits[0].text, "apple");
    }
    #[test]
    fn mixed_stream_keeps_exact_spelling_case_and_personal_words() {
        for input in ["an", "hello", "Hello", "HELLO"] {
            let words = mixed(input, &Empty, 2);
            assert!(words[0].text.eq_ignore_ascii_case(input));
            assert_eq!(words[0].language, CandidateLanguage::English);
            assert!(words.iter().all(|c| c.consumed == input.len()));
        }
        assert_eq!(mixed("Hello", &Empty, 2)[0].text, "Hello");
        struct Personal;
        impl LearningStore for Personal {
            fn record(&self, _: retype_types::LearningEvent) {}
            fn english_words(&self, prefix: &str, _: usize) -> Vec<(String, u64)> {
                if "retypetestword".starts_with(prefix) {
                    vec![("retypetestword".into(), 10)]
                } else {
                    Vec::new()
                }
            }
        }
        assert_eq!(
            mixed("retypetestword", &Personal, 2)[0].source,
            CandidateSource::User
        );
        assert!(mixed("ni'hao", &Empty, 2).is_empty());
    }
    #[test]
    fn vocabulary_and_range_top_k() {
        assert!(count() > 80_000);
        for prefix in ["a", "hel", "transl", "don't", "z"] {
            let actual = complete(prefix, &Empty, 12);
            let mut expected: Vec<_> = (0..count())
                .filter(|i| word(*i).starts_with(prefix))
                .collect();
            expected.sort_by(|a, b| frequency(*b).cmp(&frequency(*a)).then_with(|| a.cmp(b)));
            let ordinary: Vec<_> = actual
                .iter()
                .filter(|candidate| !is_entity(candidate) && !is_lexical(candidate))
                .collect();
            assert!(actual.len() <= 12);
            assert_eq!(
                ordinary
                    .iter()
                    .map(|c| c.text.to_lowercase())
                    .collect::<Vec<_>>(),
                expected
                    .into_iter()
                    .take(ordinary.len())
                    .map(|id| word(id).to_owned())
                    .collect::<Vec<_>>()
            );
        }
    }
    #[test]
    fn spelling_and_case() {
        assert!(correct("hellp", 12).iter().any(|c| c.text == "hello"));
        let suggestions = correct("teh", 12);
        assert!(
            suggestions.iter().any(|c| c.text == "the"),
            "suggestions={suggestions:?}, the={:?}",
            find("the")
        );
        assert!(correct("hello", 12).iter().all(|c| c.text != "hello"));
        assert!(complete("HEL", &Empty, 12)
            .iter()
            .any(|c| c.text == "HELLO"));
        assert!(complete("Hel", &Empty, 12)
            .iter()
            .any(|c| c.text == "Hello"));
        assert!(!valid_word("a@b"));
    }
}
