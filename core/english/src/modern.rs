//! Immutable, sense-preserving modern vocabulary. Catalog construction belongs
//! to the resource worker; production queries never parse a catalog or do IO.
use retype_types::{Candidate, CandidateLanguage, CandidateSource, EnglishAnnotation};
use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashSet},
    sync::OnceLock,
};

const DATA: &str = include_str!("../../../data/dict/modern-lexicon.tsv");
const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_ROWS: usize = 100_000;
pub(super) const MAX_RESULTS: usize = 24;
const SCAN_BUDGET: usize = 256;

#[derive(Clone, Copy)]
struct Key<'a> {
    text: &'a str,
    row: usize,
    priority: u32,
}

/// A worker-built range-max index shared by the lexical and entity catalogs.
/// Duplicate aliases retain their distinct row identities, not one overwritten
/// meaning. Only bounded best-first tree traversal happens on the input thread.
pub(super) struct RankedKeys<'a> {
    keys: Vec<Key<'a>>,
    size: usize,
    best: Vec<usize>,
}
impl<'a> RankedKeys<'a> {
    pub(super) fn new(
        items: impl IntoIterator<Item = (&'a str, usize, u32)>,
    ) -> Result<Self, String> {
        let mut keys = Vec::new();
        for (text, row, priority) in items {
            if keys.len() >= 200_000 {
                return Err("Modern catalog has too many lookup keys".into());
            }
            keys.push(Key {
                text,
                row,
                priority,
            });
        }
        keys.sort_by_key(|key| (key.text, Reverse(key.priority), key.row));
        keys.dedup_by_key(|key| (key.text, key.row));
        let size = keys.len().max(1).next_power_of_two();
        let mut index = Self {
            keys,
            size,
            best: vec![usize::MAX; size * 2],
        };
        for id in 0..index.keys.len() {
            index.best[size + id] = id;
        }
        for node in (1..size).rev() {
            let (a, b) = (index.best[node * 2], index.best[node * 2 + 1]);
            index.best[node] = if index.rank(a) >= index.rank(b) { a } else { b };
        }
        Ok(index)
    }
    fn rank(&self, id: usize) -> (u32, Reverse<usize>, Reverse<usize>) {
        self.keys
            .get(id)
            .map_or((0, Reverse(usize::MAX), Reverse(usize::MAX)), |key| {
                (key.priority, Reverse(key.row), Reverse(id))
            })
    }
    fn range(
        &self,
        lo: usize,
        hi: usize,
        limit: usize,
        scanned: &mut usize,
        seen: &mut HashSet<usize>,
        hits: &mut Vec<(usize, bool)>,
        exact: bool,
    ) {
        let mut heap = BinaryHeap::new();
        let push = |heap: &mut BinaryHeap<_>, node: usize| {
            let id = self.best[node];
            if id != usize::MAX {
                heap.push((self.rank(id), node));
            }
        };
        let (mut left, mut right) = (lo + self.size, hi + self.size);
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
        while hits.len() < limit && *scanned < SCAN_BUDGET {
            let Some((_, node)) = heap.pop() else {
                break;
            };
            if node < self.size {
                push(&mut heap, node * 2);
                push(&mut heap, node * 2 + 1);
            } else {
                *scanned += 1;
                let row = self.keys[node - self.size].row;
                if seen.insert(row) {
                    hits.push((row, exact));
                }
            }
        }
    }
    pub(super) fn lookup(&self, input: &str, limit: usize) -> Vec<(usize, bool)> {
        self.lookup_counted(input, limit).0
    }
    fn lookup_counted(&self, input: &str, limit: usize) -> (Vec<(usize, bool)>, usize) {
        if limit == 0
            || input.len() < 2
            || input.len() > 64
            || !input.bytes().all(|byte| byte.is_ascii_lowercase())
        {
            return (Vec::new(), 0);
        }
        let lo = self.keys.partition_point(|key| key.text < input);
        let exact_hi = self.keys.partition_point(|key| key.text <= input);
        let end = format!("{input}{{");
        let hi = self.keys.partition_point(|key| key.text < end.as_str());
        let mut hits = Vec::new();
        let mut scanned = 0;
        let mut seen = HashSet::new();
        let limit = limit.min(MAX_RESULTS);
        self.range(
            lo,
            exact_hi,
            limit,
            &mut scanned,
            &mut seen,
            &mut hits,
            true,
        );
        self.range(
            exact_hi,
            hi,
            limit,
            &mut scanned,
            &mut seen,
            &mut hits,
            false,
        );
        (hits, scanned)
    }
}

pub(super) struct Match {
    pub candidate: Candidate,
    pub exact: bool,
    pub priority: u32,
}
#[derive(Clone, Copy)]
struct Entry<'a> {
    key: &'a str,
    text: &'a str,
    pos: &'a str,
    definition: &'a str,
    usage: &'a str,
    full_form: &'a str,
    source: &'a str,
    sense: &'a str,
    priority: u32,
    canonical: bool,
}
struct Catalog<'a> {
    entries: Vec<Entry<'a>>,
    index: RankedKeys<'a>,
}
fn mixed_case(text: &str) -> bool {
    let letters: Vec<u8> = text.bytes().filter(u8::is_ascii_alphabetic).collect();
    letters.iter().skip(1).any(u8::is_ascii_uppercase) && letters.iter().any(u8::is_ascii_lowercase)
}
/// Registers shown only when their exact spelling is typed, never as completions.
pub(super) fn exact_only(usage: &str) -> bool {
    matches!(usage, "粗俗" | "旧义")
}
fn clean(value: &str, max: usize, empty: bool) -> bool {
    (empty || !value.is_empty())
        && value.chars().count() <= max
        && !value.chars().any(char::is_control)
        && value.trim() == value
}
impl<'a> Catalog<'a> {
    fn parse(data: &'a str) -> Result<Self, String> {
        let bad = || "Invalid modern lexical catalog".to_owned();
        if data.len() > MAX_BYTES {
            return Err(bad());
        }
        let mut lines = data.lines();
        if !lines
            .next()
            .is_some_and(|header| header.starts_with("# EAMODLX1\t"))
        {
            return Err(bad());
        }
        let mut entries = Vec::new();
        let mut previous = None;
        let mut senses = HashSet::new();
        for line in lines {
            let fields: Vec<_> = line.split('\t').collect();
            let [key, text, pos, definition, usage, full_form, source, sense, priority] =
                fields.as_slice()
            else {
                return Err(bad());
            };
            let priority = priority.parse::<u32>().map_err(|_| bad())?;
            let order = (*key, Reverse(priority), *sense);
            if entries.len() >= MAX_ROWS
                || !(2..=64).contains(&key.len())
                || !key.bytes().all(|byte| byte.is_ascii_lowercase())
                || !clean(text, 96, false)
                || !text.is_ascii()
                || !clean(pos, 16, false)
                || !clean(definition, 160, false)
                || !definition.chars().any(|c| {
                    ('\u{3400}'..='\u{9fff}').contains(&c)
                        || ('\u{20000}'..='\u{3134f}').contains(&c)
                })
                || !matches!(
                    *usage,
                    "词汇" | "网络" | "缩写" | "俚语" | "非正式" | "专业" | "旧义" | "粗俗"
                )
                || !clean(full_form, 256, true)
                || !clean(source, 512, false)
                || source.len() > 512
                || !source.starts_with("https://")
                || !clean(sense, 128, false)
                || !sense.is_ascii()
                || previous.is_some_and(|old| order <= old)
                || !senses.insert((*key, *sense))
            {
                return Err(bad());
            }
            previous = Some(order);
            entries.push(Entry {
                key,
                text,
                pos,
                definition,
                usage,
                full_form,
                source,
                sense,
                priority,
                // Mixed-case brand spellings (YouTube, iPhone) keep their casing.
                canonical: !full_form.is_empty() || *pos == "专名" || mixed_case(text),
            });
        }
        if entries.is_empty() {
            return Err(bad());
        }
        // A separate conversational sense may still use an initialism's
        // spelling. Require a real abbreviation record with the same key and
        // canonical text, not just a capitalized slang heading.
        let mut start = 0;
        while start < entries.len() {
            let mut end = start + 1;
            while end < entries.len() && entries[end].key == entries[start].key {
                end += 1;
            }
            let abbreviations: HashSet<_> = entries[start..end]
                .iter()
                .filter(|entry| entry.usage == "缩写" && !entry.full_form.is_empty())
                .map(|entry| entry.text)
                .collect();
            for entry in &mut entries[start..end] {
                if entry.usage == "网络" && abbreviations.contains(entry.text) {
                    let mut letters = entry.text.bytes().filter(u8::is_ascii_alphabetic);
                    entry.canonical |= letters
                        .next()
                        .is_some_and(|letter| letter.is_ascii_uppercase())
                        && letters.all(|letter| letter.is_ascii_uppercase());
                }
            }
            start = end;
        }
        let index = RankedKeys::new(
            entries
                .iter()
                .enumerate()
                .map(|(id, entry)| (entry.key, id, entry.priority)),
        )?;
        Ok(Self { entries, index })
    }
    fn complete(&self, input: &str, limit: usize) -> Vec<Match> {
        let key = input.to_ascii_lowercase();
        self.index
            .lookup(&key, limit)
            .into_iter()
            .map(|(id, exact)| {
                let entry = self.entries[id];
                let text = if entry.canonical {
                    entry.text.to_owned()
                } else {
                    crate::casing(&entry.text.to_ascii_lowercase(), input)
                };
                let mut candidate = Candidate::new(text, CandidateSource::Local);
                candidate.language = CandidateLanguage::English;
                candidate.consumed = input.len();
                candidate.annotation = Some(EnglishAnnotation {
                    part_of_speech: entry.pos.into(),
                    definition: entry.definition.into(),
                    lexical_usage: entry.usage.into(),
                    expanded_form: entry.full_form.into(),
                    lexical_source: entry.source.into(),
                    lexical_sense_id: entry.sense.into(),
                    ..Default::default()
                });
                candidate.comment = candidate.annotation_text(true, true, false);
                candidate.score = entry.priority as f32;
                Match {
                    candidate,
                    exact,
                    priority: entry.priority,
                }
            })
            .collect()
    }
    fn protects_word(&self, input: &str) -> bool {
        let key = input.to_ascii_lowercase();
        self.index
            .lookup(&key, MAX_RESULTS)
            .into_iter()
            .any(|(id, exact)| {
                let entry = self.entries[id];
                exact && entry.priority >= 1200 && matches!(entry.usage, "网络" | "缩写")
            })
    }
}

static CATALOG: OnceLock<Result<Catalog<'static>, String>> = OnceLock::new();
pub(super) fn initialize() -> Result<(), String> {
    CATALOG
        .get_or_init(|| Catalog::parse(DATA))
        .as_ref()
        .map(|_| ())
        .map_err(Clone::clone)
}
pub(super) fn complete(input: &str, limit: usize) -> Vec<Match> {
    // Test frontends may create in-memory vocabularies without a platform worker.
    #[cfg(any(test, feature = "test-data"))]
    let _ = initialize();
    CATALOG
        .get()
        .and_then(|catalog| catalog.as_ref().ok())
        .map_or_else(Vec::new, |catalog| catalog.complete(input, limit))
}
pub(super) fn protects_word(input: &str) -> bool {
    #[cfg(any(test, feature = "test-data"))]
    let _ = initialize();
    CATALOG
        .get()
        .and_then(|catalog| catalog.as_ref().ok())
        .is_some_and(|catalog| catalog.protects_word(input))
}

#[cfg(test)]
mod tests {
    use super::*;
    const FIXTURE: &str = "# EAMODLX1\ttest\nidk\tIDK\t短语\t我不知道\t缩写\tI don't know\thttps://example.org/idk\tidk-chat\t2200\nimo\tIMO\t短语\t依我看\t缩写\tin my opinion\thttps://example.org/imo\timo-chat\t2200\nimo\tIMO\t名词\t国际海事组织\t专业\tInternational Maritime Organization\thttps://example.org/imo\timo-ship\t500\n";
    #[test]
    fn modern_senses_and_expansions_never_change_abbreviation_commit_text() {
        let catalog = Catalog::parse(FIXTURE).unwrap();
        let hits = catalog.complete("IMO", 24);
        assert_eq!(hits.len(), 2);
        assert!(hits
            .iter()
            .all(|hit| hit.exact && hit.candidate.text == "IMO" && hit.candidate.consumed == 3));
        assert_eq!(
            hits[0].candidate.annotation.as_ref().unwrap().definition,
            "依我看"
        );
        assert_eq!(
            hits[1].candidate.annotation.as_ref().unwrap().definition,
            "国际海事组织"
        );
        assert_ne!(
            hits[0]
                .candidate
                .annotation
                .as_ref()
                .unwrap()
                .lexical_sense_id,
            hits[1]
                .candidate
                .annotation
                .as_ref()
                .unwrap()
                .lexical_sense_id
        );
        assert!(hits[0]
            .candidate
            .annotation_text(true, true, false)
            .contains("in my opinion"));
        assert_eq!(catalog.complete("id", 1)[0].candidate.text, "IDK");
        assert!(!catalog.complete("id", 1)[0].exact);
        assert!(catalog.complete("imo", 0).is_empty());
        assert!(catalog.protects_word("imo"));
        assert!(!catalog.protects_word("im"));
        assert!(Catalog::parse(&FIXTURE.replace("I don't know", "I don’t know")).is_ok());
    }
    #[test]
    fn ordinary_modern_words_follow_input_case_but_proven_initialisms_keep_their_spelling() {
        let source = "# EAMODLX1\ttest\ndoomscrolling\tdoomscrolling\t名词\t不断刷负面新闻\t非正式\t\thttps://example.org/doom\tdoom\t2200\nidk\tIDK\t短语\t我不知道\t缩写\tI don't know\thttps://example.org/idk\tidk\t2200\nlol\tLOL\t感叹词\t哈哈大笑\t网络\t\thttps://example.org/lol\tlol-chat\t2400\nlol\tLOL\t感叹词\t大笑\t缩写\tLaughing out loud\thttps://example.org/lol\tlol-expansion\t500\nopenai\tOpenAI\t专名\t人工智能公司\t词汇\t\thttps://example.org/openai\topenai\t500\nrizz\trizz\t名词\t吸引人的魅力\t俚语\t\thttps://example.org/rizz\trizz\t2400\nsick\tSICK\t形容词\t厉害的\t俚语\t\thttps://example.org/sick\tsick\t500\n";
        let catalog = Catalog::parse(source).unwrap();
        for (input, expected) in [
            ("rizz", "rizz"),
            ("Rizz", "Rizz"),
            ("RIZZ", "RIZZ"),
            ("RIZ", "RIZZ"),
            ("doomscrolling", "doomscrolling"),
            ("DOOMSCROLLING", "DOOMSCROLLING"),
            ("Doom", "Doomscrolling"),
            ("Si", "Sick"),
        ] {
            assert_eq!(
                catalog.complete(input, 1)[0].candidate.text,
                expected,
                "{input}"
            );
        }
        for (input, expected) in [
            ("idk", "IDK"),
            ("Idk", "IDK"),
            ("openai", "OpenAI"),
            ("lol", "LOL"),
        ] {
            assert_eq!(catalog.complete(input, 1)[0].candidate.text, expected);
        }
        let lol = &catalog.complete("lol", 1)[0].candidate;
        assert!(
            lol.annotation.as_ref().unwrap().expanded_form.is_empty(),
            "canonical spelling must not copy another sense's expansion"
        );
    }
    #[test]
    fn prefix_priority_reaches_late_keys_and_preserves_exact_and_alias_ambiguity() {
        let keys: Vec<_> = (0..400)
            .map(|i| {
                format!(
                    "ab{}{}",
                    char::from(b'a' + (i / 26) as u8),
                    char::from(b'a' + (i % 26) as u8)
                )
            })
            .collect();
        let mut items: Vec<_> = keys
            .iter()
            .enumerate()
            .map(|(id, key)| (key.as_str(), id, id as u32))
            .collect();
        items.extend([("ab", 400, 1), ("ab", 401, 2)]);
        let index = RankedKeys::new(items).unwrap();
        let hits = index.lookup("ab", 4);
        assert_eq!(hits, [(401, true), (400, true), (399, false), (398, false)]);
        assert_eq!(index.lookup("ab", 1), [(401, true)]);
        let (hits, scanned) = index.lookup_counted("ab", usize::MAX);
        assert_eq!(hits.len(), MAX_RESULTS);
        assert!(scanned <= SCAN_BUDGET);
        assert!(index.lookup("ab", 0).is_empty());
        for input in ["", "a", "ab ", "éé"] {
            assert!(index.lookup(input, 24).is_empty());
        }
    }
    #[test]
    fn repeated_aliases_cannot_escape_the_query_budget() {
        let keys: Vec<_> = (0..400)
            .map(|i| {
                format!(
                    "ab{}{}",
                    char::from(b'a' + (i / 26) as u8),
                    char::from(b'a' + (i % 26) as u8)
                )
            })
            .collect();
        let index = RankedKeys::new(keys.iter().map(|key| (key.as_str(), 0, 1))).unwrap();
        let (hits, scanned) = index.lookup_counted("ab", 24);
        assert_eq!(hits, [(0, false)]);
        assert_eq!(scanned, SCAN_BUDGET);
    }
    #[test]
    fn invalid_schema_order_fields_and_unsupported_usage_fail_closed() {
        for invalid in [
            FIXTURE.replace("EAMODLX1", "EAMODLX0"),
            FIXTURE.replace("我不知道", "unknown"),
            FIXTURE.replace("缩写", "unverified"),
            FIXTURE.replace("\t2200", "\t-1"),
            FIXTURE.replace("https://", "http://"),
            FIXTURE.replace("idk-chat", "imo-chat\tbad"),
        ] {
            assert!(Catalog::parse(&invalid).is_err());
        }
        let mut lines: Vec<_> = FIXTURE.lines().collect();
        lines.swap(1, 2);
        assert!(Catalog::parse(&lines.join("\n")).is_err());
    }
}
