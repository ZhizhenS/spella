//! Immutable English annotations and pinyin lookup, loaded only by resource workers.
use retype_file_map::ReadOnlyFile;
use std::{
    collections::HashSet,
    path::Path,
    sync::{Arc, OnceLock, RwLock},
};

pub const FILE_NAME: &str = "english-lexicon-v1.bin";
pub const MAGIC: &[u8; 8] = b"RTENLX01";
pub const MAX_BYTES: u64 = 64 * 1024 * 1024;
const HEADER: usize = 24;
const WORD: usize = 36;
const PINYIN: usize = 16;

enum Image {
    Mapped(ReadOnlyFile),
    Owned(Box<[u8]>),
}
impl AsRef<[u8]> for Image {
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::Mapped(x) => x.as_ref(),
            Self::Owned(x) => x,
        }
    }
}
pub struct Lexicon {
    image: Image,
    words: usize,
    pinyin: usize,
    pinyin_at: usize,
    hits_at: usize,
    arena: usize,
}
pub struct Entry<'a> {
    pub word: &'a str,
    pub part_of_speech: &'a str,
    pub definition: &'a str,
    pub ipa: &'a str,
    pub(super) rank: usize,
}
impl Entry<'_> {
    pub fn comment(&self) -> String {
        if self.part_of_speech.is_empty() {
            self.definition.to_owned()
        } else {
            format!("{} {}", self.part_of_speech, self.definition)
        }
    }
    pub fn annotation(&self, pinyin_match: bool) -> retype_types::EnglishAnnotation {
        retype_types::EnglishAnnotation {
            part_of_speech: self.part_of_speech.to_owned(),
            definition: self.definition.to_owned(),
            phonetic: self.ipa.to_owned(),
            pinyin_match,
        }
    }
}
impl Lexicon {
    pub fn open(path: &Path) -> Result<Arc<Self>, String> {
        let image = ReadOnlyFile::open(path, MAX_BYTES).map_err(|e| e.to_string())?;
        Self::validate(Image::Mapped(image)).map(Arc::new)
    }
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, String> {
        Self::validate(Image::Owned(bytes.into_boxed_slice()))
    }
    pub fn empty() -> Self {
        let mut bytes = MAGIC.to_vec();
        bytes.resize(HEADER, 0);
        Self {
            image: Image::Owned(bytes.into_boxed_slice()),
            words: 0,
            pinyin: 0,
            pinyin_at: HEADER,
            hits_at: HEADER,
            arena: HEADER,
        }
    }
    fn number(&self, at: usize) -> usize {
        u32::from_le_bytes(
            self.image.as_ref()[at..at + 4]
                .try_into()
                .unwrap_or_default(),
        ) as usize
    }
    fn string(&self, at: usize) -> &str {
        let start = self.arena + self.number(at);
        let end = start + self.number(at + 4);
        std::str::from_utf8(&self.image.as_ref()[start..end]).unwrap_or("")
    }
    fn valid_string(&self, at: usize, max: usize) -> bool {
        let start = self.number(at);
        let len = self.number(at + 4);
        len <= max
            && start
                .checked_add(len)
                .is_some_and(|end| end <= self.image.as_ref().len() - self.arena)
            && std::str::from_utf8(
                &self.image.as_ref()[self.arena + start..self.arena + start + len],
            )
            .is_ok_and(|s| !s.chars().any(char::is_control))
    }
    fn entry(&self, id: usize) -> Entry<'_> {
        let at = HEADER + id * WORD;
        Entry {
            word: self.string(at),
            part_of_speech: self.string(at + 8),
            definition: self.string(at + 16),
            ipa: self.string(at + 24),
            rank: self.number(at + 32),
        }
    }
    fn key(&self, id: usize) -> &str {
        self.string(self.pinyin_at + id * PINYIN)
    }
    fn validate(image: Image) -> Result<Self, String> {
        let bytes = image.as_ref();
        let bad = || "Invalid English annotation/pinyin image".to_owned();
        if bytes.len() < HEADER || bytes.len() as u64 > MAX_BYTES || !bytes.starts_with(MAGIC) {
            return Err(bad());
        }
        let read =
            |at| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap_or_default()) as usize;
        let words = read(8);
        let pinyin = read(12);
        let mappings = read(16);
        let strings = read(20);
        if words > 500_000 || pinyin > 500_000 || mappings > 4_000_000 {
            return Err(bad());
        }
        let pinyin_at = HEADER + words * WORD;
        let hits_at = pinyin_at + pinyin * PINYIN;
        let arena = hits_at + mappings * 4;
        if arena.checked_add(strings) != Some(bytes.len()) {
            return Err(bad());
        }
        let result = Self {
            image,
            words,
            pinyin,
            pinyin_at,
            hits_at,
            arena,
        };
        let mut previous = "";
        for id in 0..words {
            let at = HEADER + id * WORD;
            if !result.valid_string(at, 64)
                || !result.valid_string(at + 8, 32)
                || !result.valid_string(at + 16, 480)
                || !result.valid_string(at + 24, 192)
            {
                return Err(bad());
            }
            let entry = result.entry(id);
            if entry.word <= previous
                || !crate::valid_word(entry.word)
                || entry.word.bytes().any(|c| c.is_ascii_uppercase())
                || entry.rank == 0
            {
                return Err(bad());
            }
            previous = entry.word;
        }
        previous = "";
        let mut next_hit = 0;
        for id in 0..pinyin {
            let at = pinyin_at + id * PINYIN;
            if !result.valid_string(at, 64) {
                return Err(bad());
            }
            let key = result.key(id);
            let start = result.number(at + 8);
            let count = result.number(at + 12);
            if key <= previous
                || key.is_empty()
                || !key.bytes().all(|c| c.is_ascii_lowercase())
                || start != next_hit
                || count == 0
                || count > 64
                || start.checked_add(count).is_none_or(|end| end > mappings)
            {
                return Err(bad());
            }
            let mut seen = HashSet::new();
            for hit in start..start + count {
                let word = result.number(hits_at + hit * 4);
                if word >= words || !seen.insert(word) {
                    return Err(bad());
                }
            }
            previous = key;
            next_hit = start + count;
        }
        if next_hit != mappings {
            return Err(bad());
        }
        Ok(result)
    }
    pub fn count(&self) -> usize {
        self.words
    }
    pub fn pinyin_count(&self) -> usize {
        self.pinyin
    }
    pub fn lookup(&self, word: &str) -> Option<Entry<'_>> {
        let key = word.to_ascii_lowercase();
        let (mut lo, mut hi) = (0, self.words);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.entry(mid).word < key.as_str() {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        (lo < self.words && self.entry(lo).word == key).then(|| self.entry(lo))
    }
    /// Include dictionary words absent from the frequency image without scanning
    /// the entire lexicon on the input thread. Exact keys always start this range.
    pub(super) fn supplementary(
        &self,
        input: &str,
        limit: usize,
        eligible: impl Fn(&str) -> bool,
    ) -> Vec<Entry<'_>> {
        if limit == 0 {
            return Vec::new();
        }
        let key = input.to_ascii_lowercase();
        let (mut lo, mut hi) = (0, self.words);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.entry(mid).word < key.as_str() {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        let mut hits = Vec::new();
        for id in lo..(lo + 512).min(self.words) {
            let entry = self.entry(id);
            if !entry.word.starts_with(&key) {
                break;
            }
            if eligible(entry.word) {
                hits.push(entry);
            }
        }
        hits.sort_by(|a, b| a.rank.cmp(&b.rank).then_with(|| a.word.cmp(b.word)));
        hits.truncate(limit);
        hits
    }
    /// Exact full pinyin first; prefix expansion is bounded to 64 keys and 64 results.
    pub fn pinyin(&self, input: &str, limit: usize) -> (bool, Vec<Entry<'_>>) {
        let key = input.to_ascii_lowercase();
        if key.len() < 2
            || key.len() > 64
            || limit == 0
            || !key.bytes().all(|c| c.is_ascii_lowercase())
        {
            return (false, Vec::new());
        }
        let (mut lo, mut hi) = (0, self.pinyin);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.key(mid) < key.as_str() {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        let exact = lo < self.pinyin && self.key(lo) == key;
        if !exact && key.len() < 3 {
            return (false, Vec::new());
        }
        let end = if exact {
            (lo + 1).min(self.pinyin)
        } else {
            (lo + 64).min(self.pinyin)
        };
        let mut ids = Vec::new();
        let mut seen = HashSet::new();
        for id in lo..end {
            if !self.key(id).starts_with(&key) {
                break;
            }
            let at = self.pinyin_at + id * PINYIN;
            let start = self.number(at + 8);
            let count = self.number(at + 12);
            for hit in start..start + count {
                let word = self.number(self.hits_at + hit * 4);
                if seen.insert(word) {
                    ids.push(word);
                }
            }
        }
        ids.sort_by_key(|id| (self.entry(*id).rank, *id));
        ids.truncate(limit.min(64));
        (exact, ids.into_iter().map(|id| self.entry(id)).collect())
    }
}
static ACTIVE: OnceLock<RwLock<Arc<Lexicon>>> = OnceLock::new();
pub fn current() -> Arc<Lexicon> {
    Arc::clone(
        &ACTIVE
            .get_or_init(|| RwLock::new(Arc::new(Lexicon::empty())))
            .read()
            .unwrap_or_else(|p| p.into_inner()),
    )
}
pub(super) fn publish(image: Arc<Lexicon>) {
    *ACTIVE
        .get_or_init(|| RwLock::new(Arc::new(Lexicon::empty())))
        .write()
        .unwrap_or_else(|p| p.into_inner()) = image;
}

#[derive(Default)]
pub struct ManualWords(Vec<(String, String, String)>);
impl ManualWords {
    pub(super) fn validate(words: Vec<(String, String)>) -> Result<Self, String> {
        if words.len() > 10_000 {
            return Err("At most 10000 explicit personal words are supported".into());
        }
        let mut result = Vec::with_capacity(words.len());
        for (word, comment) in words {
            if !crate::valid_word(&word)
                || comment.chars().count() > 256
                || comment.chars().any(char::is_control)
            {
                return Err("Invalid explicit personal word or annotation".into());
            }
            result.push((word.to_ascii_lowercase(), word, comment));
        }
        result.sort_by(|a, b| a.0.cmp(&b.0));
        if result.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err("Duplicate explicit personal word".into());
        }
        Ok(Self(result))
    }
    pub(super) fn complete(&self, input: &str, limit: usize) -> Vec<retype_types::Candidate> {
        let key = input.to_ascii_lowercase();
        let start = self.0.partition_point(|entry| entry.0 < key);
        self.0[start..]
            .iter()
            .take_while(|entry| entry.0.starts_with(&key))
            .take(limit)
            .map(|(_, word, comment)| {
                let mut candidate = retype_types::Candidate::new(
                    crate::casing(word, input),
                    retype_types::CandidateSource::User,
                );
                candidate.language = retype_types::CandidateLanguage::English;
                candidate.consumed = input.len();
                candidate.score = 0.;
                candidate.comment = comment.clone();
                if !comment.is_empty() {
                    candidate.annotation = Some(retype_types::EnglishAnnotation {
                        part_of_speech: String::new(),
                        definition: comment.clone(),
                        phonetic: String::new(),
                        pinyin_match: false,
                    });
                }
                candidate
            })
            .collect()
    }
}
static MANUAL: OnceLock<RwLock<Arc<ManualWords>>> = OnceLock::new();
pub(super) fn manual() -> Arc<ManualWords> {
    Arc::clone(
        &MANUAL
            .get_or_init(Default::default)
            .read()
            .unwrap_or_else(|p| p.into_inner()),
    )
}
/// Publish a complete explicit personal dictionary, without filesystem access.
/// Empty input clears it. Validate all entries before replacing the prior snapshot.
pub fn set_manual_words(words: Vec<(String, String)>) -> Result<(), String> {
    replace_manual(MANUAL.get_or_init(Default::default), words)
}
fn replace_manual(
    slot: &RwLock<Arc<ManualWords>>,
    words: Vec<(String, String)>,
) -> Result<(), String> {
    let words = Arc::new(ManualWords::validate(words)?);
    *slot.write().unwrap_or_else(|p| p.into_inner()) = words;
    Ok(())
}

#[cfg(test)]
pub(super) fn fixture() -> Lexicon {
    let words = [
        ("apple", "n.", "苹果；苹果树", "ˈæpəl", 1u32),
        ("hello", "int.", "你好", "həˈləʊ", 2),
        ("help", "v.", "帮助", "help", 3),
    ];
    let keys = [
        ("nihao", vec![1u32]),
        ("pingguo", vec![0]),
        ("pingguoshu", vec![0]),
    ];
    let mut bytes = MAGIC.to_vec();
    for n in [words.len(), keys.len(), 3, 0] {
        bytes.extend_from_slice(&(n as u32).to_le_bytes());
    }
    let mut arena = Vec::new();
    let append = |bytes: &mut Vec<u8>, arena: &mut Vec<u8>, s: &str| {
        bytes.extend_from_slice(&(arena.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(s.len() as u32).to_le_bytes());
        arena.extend_from_slice(s.as_bytes());
    };
    for (word, pos, meaning, ipa, rank) in words {
        for s in [word, pos, meaning, ipa] {
            append(&mut bytes, &mut arena, s);
        }
        bytes.extend_from_slice(&rank.to_le_bytes());
    }
    let mut offset = 0u32;
    for (key, hits) in &keys {
        append(&mut bytes, &mut arena, key);
        bytes.extend_from_slice(&offset.to_le_bytes());
        bytes.extend_from_slice(&(hits.len() as u32).to_le_bytes());
        offset += hits.len() as u32;
    }
    for (_, hits) in keys {
        for id in hits {
            bytes.extend_from_slice(&id.to_le_bytes());
        }
    }
    bytes[20..24].copy_from_slice(&(arena.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&arena);
    Lexicon::from_bytes(bytes).unwrap()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn annotations_and_bounded_pinyin_lookup_preserve_english_text() {
        let lex = fixture();
        assert_eq!(lex.lookup("APPLE").unwrap().comment(), "n. 苹果；苹果树");
        assert_eq!(lex.lookup("apple").unwrap().ipa, "ˈæpəl");
        assert!(lex.lookup("unknown").is_none());
        let (exact, hits) = lex.pinyin("nihao", 5);
        assert!(exact);
        assert_eq!(hits[0].word, "hello");
        let (exact, hits) = lex.pinyin("pingg", 5);
        assert!(!exact);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].word, "apple");
        assert!(lex.pinyin("pi", 5).1.is_empty());
        assert!(lex.pinyin("pingguo", 0).1.is_empty());
        assert!(lex.pinyin("ping-guo", 5).1.is_empty());
    }
    #[test]
    fn explicit_personal_words_validate_before_publication_and_complete_stably() {
        let words = ManualWords::validate(vec![
            ("retypetest".into(), "测试词条".into()),
            ("retype".into(), "输入法".into()),
        ])
        .unwrap();
        let hits = words.complete("ret", 5);
        assert_eq!(hits[0].text, "retype");
        assert_eq!(hits[1].text, "retypetest");
        assert_eq!(hits[0].annotation.as_ref().unwrap().definition, "输入法");
        assert_eq!(hits[0].source, retype_types::CandidateSource::User);
        assert_eq!(words.complete("RET", 1)[0].text, "RETYPE");
        assert!(ManualWords::default().complete("ret", 5).is_empty());
        assert!(ManualWords::validate(vec![("a b".into(), String::new())]).is_err());
        assert!(ManualWords::validate(vec![("safe".into(), "x".repeat(257))]).is_err());
        assert!(ManualWords::validate(vec![("safe".into(), "line\nbreak".into())]).is_err());
        assert!(ManualWords::validate(vec![
            ("Word".into(), String::new()),
            ("word".into(), String::new())
        ])
        .is_err());
        assert!(ManualWords::validate(vec![("safe".into(), String::new()); 10_001]).is_err());
    }
    #[test]
    fn failed_manual_replacement_preserves_prior_snapshot_and_empty_clears() {
        let slot = RwLock::new(Arc::new(ManualWords::default()));
        replace_manual(&slot, vec![("safe".into(), "原词条".into())]).unwrap();
        let old = Arc::clone(&slot.read().unwrap());
        assert!(replace_manual(&slot, vec![("two words".into(), String::new())]).is_err());
        assert!(Arc::ptr_eq(&old, &slot.read().unwrap()));
        assert_eq!(slot.read().unwrap().complete("sa", 1)[0].comment, "原词条");
        replace_manual(&slot, Vec::new()).unwrap();
        assert!(slot.read().unwrap().complete("sa", 1).is_empty());
        assert_eq!(old.complete("sa", 1)[0].text, "safe");
    }
    #[test]
    fn truncated_and_out_of_range_sidecars_are_rejected() {
        let lex = fixture();
        let bytes = lex.image.as_ref();
        for end in 0..bytes.len() {
            assert!(
                Lexicon::from_bytes(bytes[..end].to_vec()).is_err(),
                "end={end}"
            );
        }
        for at in [
            8,
            12,
            16,
            20,
            24,
            28,
            32,
            36,
            40,
            44,
            48,
            lex.pinyin_at,
            lex.pinyin_at + 8,
            lex.hits_at,
        ] {
            let mut corrupt = bytes.to_vec();
            corrupt[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
            assert!(Lexicon::from_bytes(corrupt).is_err(), "at={at}");
        }
        assert_eq!(Lexicon::empty().count(), 0);
    }
    #[test]
    #[ignore = "requires the frozen lexicon prepared by tools/english-assist/build-english-lexicon.ps1"]
    fn prepared_lexicon_has_real_chinese_annotations_and_pinyin_meanings() {
        struct Empty;
        impl retype_types::LearningStore for Empty {
            fn record(&self, _: retype_types::LearningEvent) {}
        }
        let lex = Lexicon::open(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../dist/english-dictionaries/english-lexicon-v1.bin"),
        )
        .unwrap();
        assert!(lex.count() > 80_000);
        assert!(lex.pinyin_count() > 30_000);
        let apple = lex.lookup("apple").unwrap();
        assert_eq!(apple.part_of_speech, "n.");
        assert!(apple.definition.contains("苹果"));
        assert!(!apple.ipa.is_empty());
        let helps = lex.lookup("helps").unwrap();
        assert_ne!(helps.part_of_speech, "abbr.");
        assert!(helps.definition.contains("帮助"));
        for word in ["help", "study"] {
            let entry = lex.lookup(word).unwrap();
            assert!(!entry.part_of_speech.is_empty(), "{word}");
            assert!(
                entry
                    .definition
                    .chars()
                    .any(|c| ('\u{3400}'..='\u{9fff}').contains(&c)),
                "{word}"
            );
        }
        for (key, word) in [
            ("nihao", "hello"),
            ("pingguo", "apple"),
            ("suanfa", "algorithm"),
            ("xuexi", "study"),
        ] {
            let (exact, hits) = lex.pinyin(key, 24);
            assert!(exact, "missing exact pinyin key {key}");
            assert!(
                hits.iter().any(|entry| entry.word == word),
                "{key}: {:?}",
                hits.iter().map(|entry| entry.word).collect::<Vec<_>>()
            );
        }
        let vocabulary = crate::current();
        let manual = ManualWords::default();
        for input in ["he", "He", "HE", "an", "you"] {
            let hits = vocabulary.complete_from(input, &Empty, 1, true, &lex, &manual);
            assert_eq!(hits.len(), 1);
            assert!(
                !hits[0]
                    .annotation
                    .as_ref()
                    .is_some_and(|annotation| annotation.pinyin_match),
                "{input}: {}",
                hits[0].text
            );
        }
        for (input, word) in [
            ("airplane", "airplane"),
            ("Airp", "Airplane"),
            ("analyze", "analyze"),
            ("aluminum", "aluminum"),
        ] {
            let hits = vocabulary.complete_from(input, &Empty, 5, false, &lex, &manual);
            assert!(
                hits.iter().any(|candidate| candidate.text == word),
                "{input}"
            );
        }
    }
}
