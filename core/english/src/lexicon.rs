//! Immutable English annotations and pinyin lookup, loaded only by resource workers.
use retype_file_map::ReadOnlyFile;
use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashSet},
    path::Path,
    sync::{Arc, OnceLock, RwLock},
};

pub const FILE_NAME: &str = "english-lexicon-v2.bin";
pub const MAGIC: &[u8; 8] = b"RTENLX02";
pub const MAX_BYTES: u64 = 64 * 1024 * 1024;
const HEADER: usize = 24;
const WORD: usize = 36;
const PINYIN: usize = 16;
const MAPPING: usize = 40;
const QUERY_RELATIONS: usize = 512;
const QUERY_KEYS: usize = 64;
const QUERY_HITS: usize = 64;
const MAX_KEY_RELATIONS: usize = 65_536;

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
    key_priorities: Vec<(u8, u32)>,
    key_tree: Vec<usize>,
}
#[derive(Debug)]
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
            ..Default::default()
        }
    }
}
/// One Chinese sense relation, kept distinct from an English word's definition.
#[derive(Debug)]
pub struct PinyinHit<'a> {
    pub entry: Entry<'a>,
    pub hanzi: &'a str,
    pub reading: &'a str,
    pub gloss: &'a str,
    pub key: &'a str,
    pub exact: bool,
    pub frequency: u32,
    pub flags: u32,
    pub ordinal: u32,
}
impl<'a> std::ops::Deref for PinyinHit<'a> {
    type Target = Entry<'a>;
    fn deref(&self) -> &Self::Target {
        &self.entry
    }
}
impl PinyinHit<'_> {
    pub fn annotation(&self, pinyin_match: bool) -> retype_types::EnglishAnnotation {
        let mut annotation = self.entry.annotation(pinyin_match);
        if pinyin_match {
            annotation.matched_chinese = self.hanzi.to_owned();
            annotation.matched_pinyin = self.reading.to_owned();
            annotation.pinyin_exact = self.exact;
        }
        annotation
    }
}
fn reading_key(reading: &str) -> String {
    reading
        .to_lowercase()
        .replace("u:", "v")
        .replace('ü', "v")
        .chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '1'..='5'))
        .collect()
}
fn relation_priority(flags: u32, frequency: u32) -> (u8, u32) {
    // Ordinary modern readings precede proper/variant and dated senses.
    (
        3 - u8::from(flags & 8 != 0) - 2 * u8::from(flags & 4 != 0),
        frequency,
    )
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
            key_priorities: Vec::new(),
            key_tree: vec![usize::MAX; 2],
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
    fn pinyin_hit(&self, mapping: usize, key_id: usize, exact: bool) -> PinyinHit<'_> {
        let at = self.hits_at + mapping * MAPPING;
        PinyinHit {
            entry: self.entry(self.number(at)),
            hanzi: self.string(at + 4),
            reading: self.string(at + 12),
            gloss: self.string(at + 20),
            // Key and relation strings all borrow the same immutable image.
            key: self.key(key_id),
            exact,
            frequency: self.number(at + 28) as u32,
            flags: self.number(at + 32) as u32,
            ordinal: self.number(at + 36) as u32,
        }
    }
    fn pinyin_lower(&self, key: &str) -> usize {
        let (mut lo, mut hi) = (0, self.pinyin);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.key(mid) < key {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo
    }
    fn key_rank(&self, id: usize) -> (u8, u32, Reverse<usize>) {
        let (tier, frequency) = self.key_priorities[id];
        (tier, frequency, Reverse(id))
    }
    fn best_key(&self, left: usize, right: usize) -> usize {
        if left == usize::MAX {
            return right;
        }
        if right == usize::MAX || self.key_rank(left) >= self.key_rank(right) {
            left
        } else {
            right
        }
    }
    fn index_keys(&mut self, priorities: Vec<(u8, u32)>) {
        self.key_priorities = priorities;
        let size = self.pinyin.max(1).next_power_of_two();
        self.key_tree = vec![usize::MAX; size * 2];
        for id in 0..self.pinyin {
            self.key_tree[size + id] = id;
        }
        for node in (1..size).rev() {
            self.key_tree[node] =
                self.best_key(self.key_tree[node * 2], self.key_tree[node * 2 + 1]);
        }
    }
    /// Range-max search visits only the best bounded key set, not every matching key.
    fn prefix_keys(&self, lo: usize, hi: usize) -> Vec<usize> {
        let size = self.key_tree.len() / 2;
        let mut heap = BinaryHeap::new();
        let push = |heap: &mut BinaryHeap<_>, node: usize| {
            let id = self.key_tree[node];
            if id != usize::MAX {
                heap.push((self.key_rank(id), node));
            }
        };
        let (mut left, mut right) = (lo + size, hi + size);
        while left < right {
            if left % 2 == 1 {
                push(&mut heap, left);
                left += 1;
            }
            if right % 2 == 1 {
                right -= 1;
                push(&mut heap, right);
            }
            left /= 2;
            right /= 2;
        }
        let mut keys = Vec::new();
        while keys.len() < QUERY_KEYS {
            let Some((_, node)) = heap.pop() else {
                break;
            };
            if node >= size {
                keys.push(self.key_tree[node]);
            } else {
                push(&mut heap, node * 2);
                push(&mut heap, node * 2 + 1);
            }
        }
        keys
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
        let arena = hits_at + mappings * MAPPING;
        if arena.checked_add(strings) != Some(bytes.len()) {
            return Err(bad());
        }
        let mut result = Self {
            image,
            words,
            pinyin,
            pinyin_at,
            hits_at,
            arena,
            key_priorities: Vec::new(),
            key_tree: Vec::new(),
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
            // A headword is one word or a short phrase (cell phone).
            if entry.word <= previous
                || !entry.word.split(' ').all(crate::valid_word)
                || entry.word.bytes().any(|c| c.is_ascii_uppercase())
                || entry.rank == 0
            {
                return Err(bad());
            }
            previous = entry.word;
        }
        previous = "";
        let mut next_hit = 0;
        let mut priorities = Vec::with_capacity(pinyin);
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
                || count > MAX_KEY_RELATIONS
                || start.checked_add(count).is_none_or(|end| end > mappings)
            {
                return Err(bad());
            }
            let mut priority = (0, 0);
            for hit in start..start + count {
                let relation = hits_at + hit * MAPPING;
                if result.number(relation) >= words
                    || !result.valid_string(relation + 4, 1024)
                    || !result.valid_string(relation + 12, 256)
                    || !result.valid_string(relation + 20, 4096)
                {
                    return Err(bad());
                }
                let flags = result.number(relation + 32);
                if result.string(relation + 4).trim().is_empty()
                    || result.string(relation + 20).trim().is_empty()
                    || reading_key(result.string(relation + 12)) != key
                    || flags & !15 != 0
                    || flags & 3 == 0
                    || result.number(relation + 36) > 1_000_000
                {
                    return Err(bad());
                }
                priority = priority.max(relation_priority(
                    flags as u32,
                    result.number(relation + 28) as u32,
                ));
            }
            priorities.push(priority);
            previous = key;
            next_hit = start + count;
        }
        if next_hit != mappings {
            return Err(bad());
        }
        result.index_keys(priorities);
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
            // Phrases are reached from pinyin, never offered as completions.
            if !entry.word.contains(' ') && eligible(entry.word) {
                hits.push(entry);
            }
        }
        hits.sort_by(|a, b| a.rank.cmp(&b.rank).then_with(|| a.word.cmp(b.word)));
        hits.truncate(limit);
        hits
    }
    /// Preserve the compiler's sense-aware order; bound all relation work, including duplicates.
    pub fn pinyin(&self, input: &str, limit: usize) -> (bool, Vec<PinyinHit<'_>>) {
        let key = input.to_ascii_lowercase();
        if key.len() < 2
            || key.len() > 64
            || limit == 0
            || !key.bytes().all(|c| c.is_ascii_lowercase())
        {
            return (false, Vec::new());
        }
        let lo = self.pinyin_lower(&key);
        let exact = lo < self.pinyin && self.key(lo) == key;
        if !exact && key.len() < 3 {
            return (false, Vec::new());
        }
        if !exact {
            return (false, self.pinyin_prefix(&key, lo, limit));
        }
        let end = (lo + 1).min(self.pinyin);
        let mut hits = Vec::new();
        let mut seen = HashSet::new();
        let mut scanned = 0;
        for id in lo..end {
            if !self.key(id).starts_with(&key) {
                break;
            }
            let at = self.pinyin_at + id * PINYIN;
            let start = self.number(at + 8);
            let count = self.number(at + 12);
            for hit in start..start + count {
                if scanned == QUERY_RELATIONS {
                    return (exact, hits);
                }
                scanned += 1;
                let word = self.number(self.hits_at + hit * MAPPING);
                if seen.insert(word) {
                    hits.push(self.pinyin_hit(hit, id, exact));
                    if hits.len() == limit.min(QUERY_HITS) {
                        return (exact, hits);
                    }
                }
            }
        }
        (exact, hits)
    }
    fn pinyin_prefix(&self, key: &str, lo: usize, limit: usize) -> Vec<PinyinHit<'_>> {
        let hi = self.pinyin_lower(&format!("{key}{{"));
        let mut pool = Vec::new();
        for id in self.prefix_keys(lo, hi) {
            let at = self.pinyin_at + id * PINYIN;
            let start = self.number(at + 8);
            for local in 0..self.number(at + 12).min(QUERY_HITS) {
                if pool.len() == QUERY_RELATIONS {
                    break;
                }
                pool.push((id, local, self.pinyin_hit(start + local, id, false)));
            }
            if pool.len() == QUERY_RELATIONS {
                break;
            }
        }
        pool.sort_by_key(|(id, local, hit)| {
            let (tier, frequency) = relation_priority(hit.flags, hit.frequency);
            (
                Reverse(tier),
                Reverse(frequency),
                hit.key.len() - key.len(),
                *local,
                *id,
            )
        });
        let mut seen = HashSet::new();
        pool.into_iter()
            .filter_map(|(_, _, hit)| seen.insert(hit.word).then_some(hit))
            .take(limit.min(QUERY_HITS))
            .collect()
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
                        ..Default::default()
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
type FixtureWord<'a> = (&'a str, &'a str, &'a str, &'a str, u32);
#[cfg(test)]
type FixtureRelation<'a> = (&'a str, usize, &'a str, &'a str, &'a str, u32, u32, u32);
#[cfg(test)]
fn fixture_bytes(words: &[FixtureWord<'_>], relations: &[FixtureRelation<'_>]) -> Vec<u8> {
    let mut keys = std::collections::BTreeMap::new();
    for relation in relations {
        keys.entry(relation.0)
            .or_insert_with(Vec::new)
            .push(relation);
    }
    let mut bytes = MAGIC.to_vec();
    for n in [words.len(), keys.len(), relations.len(), 0] {
        bytes.extend_from_slice(&(n as u32).to_le_bytes());
    }
    let mut arena = Vec::new();
    let append = |bytes: &mut Vec<u8>, arena: &mut Vec<u8>, s: &str| {
        bytes.extend_from_slice(&(arena.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(s.len() as u32).to_le_bytes());
        arena.extend_from_slice(s.as_bytes());
    };
    for &(word, pos, meaning, ipa, rank) in words {
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
        for &&(_, id, hanzi, reading, gloss, frequency, flags, ordinal) in hits.iter() {
            bytes.extend_from_slice(&(id as u32).to_le_bytes());
            for s in [hanzi, reading, gloss] {
                append(&mut bytes, &mut arena, s);
            }
            for number in [frequency, flags, ordinal] {
                bytes.extend_from_slice(&number.to_le_bytes());
            }
        }
    }
    bytes[20..24].copy_from_slice(&(arena.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&arena);
    bytes
}
#[cfg(test)]
pub(super) fn fixture() -> Lexicon {
    let words = [
        ("all", "pron.", "全部", "ɔːl", 9),
        ("apple", "n.", "苹果；苹果树", "ˈæpəl", 1),
        ("fragrant", "adj.", "芳香的", "ˈfreɪɡrənt", 8),
        ("hello", "int.", "你好", "həˈləʊ", 2),
        ("help", "v.", "帮助", "help", 3),
        ("high", "adj.", "高的", "haɪ", 4),
        ("like", "prep./v.", "像；喜欢", "laɪk", 5),
        ("safe", "adj.", "安全的", "seɪf", 6),
        ("same", "adj.", "相同的；同样的", "seɪm", 7),
        ("school", "n.", "学校", "skuːl", 10),
        ("think", "v.", "想；思考", "θɪŋk", 100),
    ];
    let relations = [
        ("anquan", 7, "安全", "an1 quan2", "safe", 8000, 1, 0),
        ("nihao", 3, "你好", "ni3 hao3", "hello", 5000, 1, 0),
        ("pingguo", 1, "苹果", "ping2 guo3", "apple", 6000, 1, 0),
        (
            "pingguoshu",
            1,
            "苹果树",
            "ping2 guo3 shu4",
            "apple tree",
            1000,
            2,
            1,
        ),
        ("quan", 0, "全", "quan2", "all", 9000, 1, 0),
        ("tongyang", 8, "同样", "tong2 yang4", "same", 7000, 1, 0),
        // Sense order deliberately differs from global English rank.
        ("xiang", 10, "想", "xiang3", "to think", 9000, 1, 0),
        ("xiang", 2, "香", "xiang1", "fragrant", 8000, 2, 1),
        ("xiang", 5, "襄", "xiang1", "high (archaic)", 1, 5, 1),
        ("xiang", 9, "庠", "xiang2", "school (archaic)", 0, 5, 0),
        ("yiyang", 6, "一样", "yi1 yang4", "like", 8000, 1, 1),
        ("yiyang", 8, "一样", "yi1 yang4", "same", 8000, 1, 0),
    ];
    Lexicon::from_bytes(fixture_bytes(&words, &relations)).unwrap()
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
    fn sense_order_and_exact_prefix_evidence_survive_english_deduplication() {
        let lex = fixture();
        let (exact, hits) = lex.pinyin("xiang", 64);
        assert!(exact);
        assert_eq!(
            hits.iter().map(|hit| hit.word).collect::<Vec<_>>(),
            ["think", "fragrant", "high", "school"]
        );
        assert!(
            hits[0].rank > hits[2].rank,
            "English frequency must not reorder Chinese senses"
        );
        let annotation = hits[0].annotation(true);
        assert_eq!(annotation.matched_chinese, "想");
        assert_eq!(annotation.matched_pinyin, "xiang3");
        assert!(annotation.pinyin_exact && annotation.pinyin_match);
        assert_eq!(hits[0].gloss, "to think");
        assert_eq!(hits[2].flags, 5);
        let (_, prefix) = lex.pinyin("pingg", 64);
        assert_eq!(
            prefix.len(),
            1,
            "same English word across keys has one visible meaning"
        );
        assert_eq!(prefix[0].hanzi, "苹果");
        assert_eq!(prefix[0].key, "pingguo");
        assert!(!prefix[0].annotation(true).pinyin_exact);
        let ordinary = lex.lookup("apple").unwrap().annotation(false);
        assert!(ordinary.matched_chinese.is_empty() && ordinary.matched_pinyin.is_empty());
        assert!(!ordinary.pinyin_exact);
    }
    #[test]
    fn pinyin_work_is_bounded_before_duplicate_relations_and_key_expansion() {
        let words = [("alpha", "n.", "甲", "", 2), ("beta", "n.", "乙", "", 1)];
        let mut relations = Vec::new();
        for ordinal in 0..QUERY_RELATIONS {
            relations.push(("xiang", 0, "想", "xiang3", "alpha", 1, 1, ordinal as u32));
        }
        relations.push(("xiang", 1, "象", "xiang4", "beta", 1, 1, 0));
        let lex = Lexicon::from_bytes(fixture_bytes(&words, &relations)).unwrap();
        let (_, hits) = lex.pinyin("xiang", usize::MAX);
        assert_eq!(
            hits.iter().map(|hit| hit.word).collect::<Vec<_>>(),
            ["alpha"]
        );
        let keys: Vec<_> = (0..=QUERY_KEYS)
            .map(|i| {
                format!(
                    "abc{}{}",
                    (b'a' + (i / 26) as u8) as char,
                    (b'a' + (i % 26) as u8) as char
                )
            })
            .collect();
        let relations: Vec<_> = keys
            .iter()
            .enumerate()
            .map(|(i, key)| {
                (
                    key.as_str(),
                    usize::from(i == QUERY_KEYS),
                    "词",
                    key.as_str(),
                    "fixture",
                    1,
                    1,
                    0,
                )
            })
            .collect();
        let lex = Lexicon::from_bytes(fixture_bytes(&words, &relations)).unwrap();
        let (exact, hits) = lex.pinyin("abc", usize::MAX);
        assert!(!exact);
        assert_eq!(
            hits.iter().map(|hit| hit.word).collect::<Vec<_>>(),
            ["alpha"]
        );
        assert_eq!(lex.pinyin("abc", 1).1.len(), 1);
    }
    #[test]
    fn prefix_priority_reaches_late_keys_and_keeps_best_chinese_relation_before_dedup() {
        let words = [("alpha", "n.", "甲", "", 1), ("beta", "n.", "乙", "", 100)];
        let keys: Vec<_> = (0..QUERY_KEYS + 2)
            .map(|i| {
                format!(
                    "abc{}{}",
                    (b'a' + (i / 26) as u8) as char,
                    (b'a' + (i % 26) as u8) as char
                )
            })
            .collect();
        let mut relations = Vec::new();
        for (i, key) in keys.iter().enumerate() {
            if i == keys.len() - 1 {
                // Compiled local order intentionally differs from Chinese frequency.
                relations.push((key.as_str(), 0, "优", key.as_str(), "alpha", 999, 1, 0));
                relations.push((key.as_str(), 1, "常", key.as_str(), "beta", 1000, 1, 1));
            } else {
                let (hanzi, frequency, flags) = match i {
                    62 => ("变体", u32::MAX, 9),
                    63 => ("过时", u32::MAX, 5),
                    _ => ("稀", 1, 1),
                };
                relations.push((
                    key.as_str(),
                    0,
                    hanzi,
                    key.as_str(),
                    "alpha",
                    frequency,
                    flags,
                    0,
                ));
            }
        }
        let lex = Lexicon::from_bytes(fixture_bytes(&words, &relations)).unwrap();
        let (exact, hits) = lex.pinyin("abc", 64);
        assert!(!exact);
        assert_eq!(
            hits.iter().map(|hit| hit.word).collect::<Vec<_>>(),
            ["beta", "alpha"]
        );
        assert_eq!((hits[0].hanzi, hits[1].hanzi), ("常", "优"));
        assert_eq!(hits[0].key, keys.last().unwrap().as_str());
        assert!(!hits[0].annotation(true).pinyin_exact);
        assert_eq!(lex.pinyin("abc", 1).1[0].word, "beta");
        assert!(lex.pinyin("abc", 0).1.is_empty());
        let (exact, hits) = lex.pinyin(keys.last().unwrap(), 64);
        assert!(exact);
        assert_eq!(
            hits.iter().map(|hit| hit.word).collect::<Vec<_>>(),
            ["alpha", "beta"],
            "exact lookup must retain compiled order"
        );
        assert_eq!(lex.pinyin(&keys[63], 1).1[0].flags, 5);
    }
    #[test]
    fn prefix_local_order_ties_and_relation_budgets_are_stable() {
        let words = [("alpha", "n.", "甲", "", 100), ("beta", "n.", "乙", "", 1)];
        let tied = [
            ("abcaa", 0, "先", "abcaa", "alpha", 7, 1, 20),
            ("abcaa", 1, "后", "abcaa", "beta", 7, 1, 0),
        ];
        let lex = Lexicon::from_bytes(fixture_bytes(&words, &tied)).unwrap();
        assert_eq!(
            lex.pinyin("abc", 1).1[0].word,
            "alpha",
            "compiled local order resolves equal Chinese priority"
        );
        let mut relations: Vec<_> = (0..QUERY_HITS)
            .map(|i| ("abcaa", 0, "先", "abcaa", "alpha", 1, 1, i as u32))
            .collect();
        relations.push(("abcaa", 1, "后", "abcaa", "beta", 1, 1, 0));
        let lex = Lexicon::from_bytes(fixture_bytes(&words, &relations)).unwrap();
        assert_eq!(
            lex.pinyin("abc", 64).1.len(),
            1,
            "prefix scans at most 64 relations per key"
        );
        assert_eq!(
            lex.pinyin("abcaa", 64).1.len(),
            2,
            "exact keeps its independent 512-relation budget"
        );
        let keys: Vec<_> = (0..9)
            .map(|i| format!("abc{}", (b'a' + i as u8) as char))
            .collect();
        let mut relations = Vec::new();
        for (i, key) in keys.iter().enumerate() {
            for local in 0..QUERY_HITS {
                relations.push((
                    key.as_str(),
                    usize::from(i == 8),
                    "词",
                    key.as_str(),
                    "fixture",
                    if i == 8 { 1 } else { 2 },
                    1,
                    local as u32,
                ));
            }
        }
        let lex = Lexicon::from_bytes(fixture_bytes(&words, &relations)).unwrap();
        assert_eq!(
            lex.pinyin("abc", 64)
                .1
                .iter()
                .map(|hit| hit.word)
                .collect::<Vec<_>>(),
            ["alpha"],
            "duplicate relations still consume the global 512 budget"
        );
    }
    #[test]
    fn relation_corruption_and_obsolete_schema_are_rejected() {
        let lex = fixture();
        for offset in [0, 4, 8, 12, 16, 20, 24, 32, 36] {
            let mut bytes = lex.image.as_ref().to_vec();
            bytes[lex.hits_at + offset..lex.hits_at + offset + 4]
                .copy_from_slice(&u32::MAX.to_le_bytes());
            assert!(Lexicon::from_bytes(bytes).is_err(), "offset={offset}");
        }
        let mut bytes = lex.image.as_ref().to_vec();
        bytes[7] = b'1';
        assert!(Lexicon::from_bytes(bytes).is_err());
        let mut bytes = lex.image.as_ref().to_vec();
        bytes[lex.hits_at + 32..lex.hits_at + 36].fill(0);
        assert!(Lexicon::from_bytes(bytes).is_err());
        assert_eq!(reading_key("LÜ4 se4"), "lvse");
        assert_eq!(reading_key("lu:4 se4"), "lvse");
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
                .join("../../dist/english-dictionaries")
                .join(FILE_NAME),
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
