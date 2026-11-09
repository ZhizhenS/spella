//! Bounded exact-word segmentation with a final unfinished word prefix.
//! The raw composition stays authoritative; every result commits one complete
//! alternative only after explicit selection. No IO, learning or cloud calls.
use super::{casing, lexicon, Vocabulary};
use retype_types::{Candidate, CandidateLanguage, CandidateSource};
use std::{cmp::Reverse, collections::BinaryHeap, sync::OnceLock};

const MAX_INPUT: usize = 48;
const MAX_WORD: usize = 24;
const MAX_WORDS: usize = 6;
const BEAM: usize = 8;
const MAX_RESULTS: usize = 24;
const SPLIT_COST: f64 = 0.4;
const COMPLETION_CHAR_COST: f64 = 0.04;
const EXTRA_WORD_COST: f64 = 0.15;
const CONTEXT_WEIGHT: f64 = 0.95;
const PHRASE_CONFIDENCE: f64 = 0.7;
const MIN_ANCHOR_PROBABILITY: f64 = 0.005;
const BACKOFF_PREFIX_CANDIDATES: usize = BEAM * 2;
const DATA: &[u8] = include_bytes!("../assets/bigrams.bin");

#[derive(Clone, Copy)]
struct Model<'a> {
    bytes: &'a [u8],
    words: usize,
    pairs: usize,
    size: usize,
    edges: usize,
    tree: usize,
    arena: usize,
}
fn u32_at(bytes: &[u8], at: usize) -> usize {
    bytes
        .get(at..at.saturating_add(4))
        .and_then(|part| part.try_into().ok())
        .map(u32::from_le_bytes)
        .unwrap_or_default() as usize
}
fn u64_at(bytes: &[u8], at: usize) -> u64 {
    bytes
        .get(at..at.saturating_add(8))
        .and_then(|part| part.try_into().ok())
        .map(u64::from_le_bytes)
        .unwrap_or_default()
}
impl<'a> Model<'a> {
    fn open(bytes: &'a [u8]) -> Option<Self> {
        // The build pipeline verifies every record, count, tree node and source
        // digest. Runtime shape checks remain O(1); record reads stay bounds checked.
        if bytes.len() < 32 || &bytes[..8] != b"RTENBG01" || bytes[24..32] != [0; 8] {
            return None;
        }
        let words = u32_at(bytes, 8);
        let pairs = u32_at(bytes, 12);
        let strings = u32_at(bytes, 16);
        let size = u32_at(bytes, 20);
        if words > 100_000
            || pairs > 1_000_000
            || !size.is_power_of_two()
            || size != pairs.max(1).next_power_of_two()
        {
            return None;
        }
        let edges = 32usize.checked_add(words.checked_mul(24)?)?;
        let tree = edges.checked_add(pairs.checked_mul(12)?)?;
        let arena = tree.checked_add(size.checked_mul(8)?)?;
        if arena.checked_add(strings)? != bytes.len() {
            return None;
        }
        Some(Self {
            bytes,
            words,
            pairs,
            size,
            edges,
            tree,
            arena,
        })
    }
    fn word(&self, id: usize) -> &'a str {
        if id >= self.words {
            return "";
        }
        let at = 32 + id * 24;
        let start = self.arena.saturating_add(u32_at(self.bytes, at));
        let end = start.saturating_add(u32_at(self.bytes, at + 4));
        self.bytes
            .get(start..end)
            .and_then(|part| std::str::from_utf8(part).ok())
            .unwrap_or("")
    }
    fn lower(&self, key: &str) -> usize {
        let (mut lo, mut hi) = (0, self.words);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.word(mid) < key {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo
    }
    fn find(&self, key: &str) -> Option<usize> {
        let id = self.lower(key);
        (id < self.words && self.word(id) == key).then_some(id)
    }
    fn row(&self, id: usize) -> Option<(usize, usize, u64)> {
        if id >= self.words {
            return None;
        }
        let at = 32 + id * 24;
        let first = u32_at(self.bytes, at + 8);
        let count = u32_at(self.bytes, at + 12);
        let end = first.checked_add(count)?;
        (end <= self.pairs).then_some((first, end, u64_at(self.bytes, at + 16)))
    }
    fn target(&self, edge: usize) -> usize {
        u32_at(self.bytes, self.edges + edge * 12)
    }
    fn count(&self, edge: usize) -> u64 {
        u64_at(self.bytes, self.edges + edge * 12 + 4)
    }
    fn edge_lower(&self, mut lo: usize, mut hi: usize, target: usize) -> usize {
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.target(mid) < target {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo
    }
    fn conditional(&self, previous: &str, next: &str, unigram: f64) -> f64 {
        let Some((first, end, total)) = self.find(previous).and_then(|id| self.row(id)) else {
            return unigram;
        };
        if total == 0 {
            return unigram;
        }
        let observed = self.find(next).map_or(0., |id| {
            let edge = self.edge_lower(first, end, id);
            if edge < end && self.target(edge) == id {
                (self.count(edge) as f64 / total as f64).min(1.)
            } else {
                0.
            }
        });
        (CONTEXT_WEIGHT * observed + (1. - CONTEXT_WEIGHT) * unigram).max(f64::MIN_POSITIVE)
    }
    fn observed(&self, previous: &str, next: &str) -> bool {
        self.observed_probability(previous, next).is_some()
    }
    fn observed_probability(&self, previous: &str, next: &str) -> Option<f64> {
        let (first, end, total) = self.find(previous).and_then(|id| self.row(id))?;
        if total == 0 {
            return None;
        }
        let id = self.find(next)?;
        let edge = self.edge_lower(first, end, id);
        (edge < end && self.target(edge) == id && self.count(edge) > 0)
            .then(|| (self.count(edge) as f64 / total as f64).min(1.))
    }
    fn top(&self, previous: &str, prefix: &str, limit: usize) -> Vec<(usize, u64, u64)> {
        let Some((first, end, total)) = self.find(previous).and_then(|id| self.row(id)) else {
            return Vec::new();
        };
        if total == 0 || limit == 0 {
            return Vec::new();
        }
        let lo = self.lower(prefix);
        let hi = if prefix.is_empty() {
            self.words
        } else {
            self.lower(&format!("{prefix}{{"))
        };
        let (mut left, mut right) = (
            self.edge_lower(first, end, lo) + self.size,
            self.edge_lower(first, end, hi) + self.size,
        );
        let mut heap = BinaryHeap::new();
        let push = |heap: &mut BinaryHeap<_>, node| {
            let edge = u32_at(self.bytes, self.tree + node * 4);
            if edge < self.pairs {
                heap.push((self.count(edge), Reverse(edge), node));
            }
        };
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
        let mut result = Vec::new();
        while result.len() < limit {
            let Some((count, Reverse(edge), node)) = heap.pop() else {
                break;
            };
            if node >= self.size {
                result.push((self.target(edge), count, total));
            } else {
                push(&mut heap, node * 2);
                push(&mut heap, node * 2 + 1);
            }
        }
        result
    }
}
fn model() -> Option<&'static Model<'static>> {
    static MODEL: OnceLock<Option<Model<'static>>> = OnceLock::new();
    MODEL.get_or_init(|| Model::open(DATA)).as_ref()
}
fn unigram(vocab: &Vocabulary, word: &str) -> f64 {
    let count = vocab.find(word).map_or(1, |id| vocab.frequency(id));
    (count as f64 / vocab.total_frequency().max(1) as f64).clamp(f64::MIN_POSITIVE, 1.)
}
/// A common log-probability scale for both unsplit prefix words and segmented
/// candidates. Dictionary counts and bigram counts are never divided across
/// corpora: each bigram row has its own stored normalization.
pub(super) fn score(vocab: &Vocabulary, text: &str, input_len: usize) -> f32 {
    let words: Vec<_> = text.split(' ').collect();
    if words.is_empty()
        || words.len() > MAX_WORDS
        || words.iter().any(|word| !super::valid_word(word))
    {
        return f32::NEG_INFINITY;
    }
    let lower: Vec<_> = words.iter().map(|word| word.to_ascii_lowercase()).collect();
    let mut result = 0.;
    let mut letters = 0usize;
    for (index, word) in lower.iter().enumerate() {
        let prior = unigram(vocab, word);
        let probability = if index == 0 {
            prior
        } else {
            model().map_or(prior, |model| {
                model.conditional(&lower[index - 1], word, prior)
            })
        };
        result += probability.ln();
        if index > 0 {
            result -= SPLIT_COST;
            if letters >= input_len {
                result -= EXTRA_WORD_COST;
            }
        }
        letters += word.len();
    }
    result -= letters.saturating_sub(input_len) as f64 * COMPLETION_CHAR_COST;
    result as f32
}
fn unigram_top(vocab: &Vocabulary, prefix: &str, limit: usize) -> Vec<usize> {
    let lo = vocab.lower_bound(prefix);
    let hi = vocab.lower_bound(&format!("{prefix}{{"));
    let (mut left, mut right) = (lo + vocab.size(), hi + vocab.size());
    let mut heap = BinaryHeap::new();
    let push = |heap: &mut BinaryHeap<_>, node| {
        let id = vocab.u32_at(16 + vocab.count() * 16 + node * 4);
        if id < vocab.count() {
            heap.push((vocab.frequency(id), Reverse(id), node));
        }
    };
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
    let mut result = Vec::new();
    while result.len() < limit {
        let Some((_, Reverse(id), node)) = heap.pop() else {
            break;
        };
        if node >= vocab.size() {
            result.push(id);
        } else {
            push(&mut heap, node * 2);
            push(&mut heap, node * 2 + 1);
        }
    }
    result
}
#[derive(Clone)]
struct Path {
    parts: Vec<(usize, usize)>,
    score: f32,
    // Sum only genuinely observed completed-word transitions, never unigram
    // smoothing. This is an admission check, independent of candidate ranking.
    context_log_sum: f64,
}
impl Path {
    fn allows_tail_backoff(&self) -> bool {
        self.parts.len() >= 2
            && self.context_log_sum / (self.parts.len() - 1) as f64 >= MIN_ANCHOR_PROBABILITY.ln()
    }
}
fn path_text(input: &str, parts: &[(usize, usize)]) -> String {
    parts
        .iter()
        .map(|(start, end)| {
            casing(
                &input[*start..*end].to_ascii_lowercase(),
                &input[*start..*end],
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}
fn admit_path(paths: &mut Vec<Path>, path: Path) {
    paths.push(path);
    paths.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.parts.len().cmp(&b.parts.len()))
            .then_with(|| a.parts.cmp(&b.parts))
    });
    paths.truncate(BEAM);
}
struct Suggestion {
    candidate: Candidate,
    phrase: bool,
}
fn annotate_phrase(candidate: &mut Candidate, display_suffix: &str, metadata: &lexicon::Lexicon) {
    // A stem's definition/POS does not describe the completed phrase. Only an
    // exact dictionary entry for the complete displayed suffix can annotate it.
    if let Some(entry) = metadata.lookup(display_suffix) {
        candidate.comment = entry.comment();
        candidate.annotation = Some(entry.annotation(false));
    } else {
        candidate.comment = "短语补全".into();
        candidate.annotation = None;
    }
}
fn admit_suggestion(pool: &mut Vec<Suggestion>, suggestion: Suggestion, cap: usize) {
    if let Some(existing) = pool
        .iter_mut()
        .find(|old| old.candidate.text == suggestion.candidate.text)
    {
        if suggestion.candidate.score > existing.candidate.score {
            *existing = suggestion;
        }
        return;
    }
    pool.push(suggestion);
    if pool.len() > cap * 3 {
        pool.sort_by(|a, b| {
            b.candidate
                .score
                .total_cmp(&a.candidate.score)
                .then_with(|| a.candidate.text.cmp(&b.candidate.text))
        });
        pool.truncate(cap * 2);
    }
}
pub(super) fn complete(
    vocab: &Vocabulary,
    input: &str,
    limit: usize,
    phrases: bool,
) -> Vec<Candidate> {
    let cap = limit.min(MAX_RESULTS);
    if cap == 0 || input.len() < 2 || input.len() > MAX_INPUT || !super::valid_word(input) {
        return Vec::new();
    }
    let key = input.to_ascii_lowercase();
    let metadata = lexicon::current();
    // Ordinary complete words have a fully literal interpretation. Do not split
    // them merely because short high-frequency fragments can also be found.
    if vocab.find(&key).is_some() || metadata.lookup(&key).is_some() {
        return Vec::new();
    }
    let Some(model) = model() else {
        return Vec::new();
    };
    let mut paths: Vec<Vec<Path>> = vec![Vec::new(); input.len() + 1];
    paths[0].push(Path {
        parts: Vec::new(),
        score: 0.,
        context_log_sum: 0.,
    });
    for start in 0..input.len() - 1 {
        if paths[start].is_empty() {
            continue;
        }
        let previous = paths[start].clone();
        for end in start + 1..=(start + MAX_WORD).min(input.len() - 1) {
            let token = &key[start..end];
            // Single-letter labels are plentiful in a spelling dictionary, but
            // a/i are the productive standalone English words for this decoder.
            if (token.len() == 1 && !matches!(token, "a" | "i"))
                || !super::valid_word(token)
                || vocab.find(token).is_none()
            {
                continue;
            }
            for path in &previous {
                if path.parts.len() + 1 >= MAX_WORDS {
                    continue;
                }
                // A dictionary token alone is insufficient evidence for a new
                // boundary (e.g. repeated "zzz"). Every completed transition
                // must be observed in the frozen context model.
                let context_log = if let Some((left, right)) = path.parts.last() {
                    let Some(probability) = model.observed_probability(&key[*left..*right], token)
                    else {
                        continue;
                    };
                    probability.ln()
                } else {
                    0.
                };
                let mut next = path.clone();
                next.parts.push((start, end));
                next.context_log_sum += context_log;
                next.score = score(vocab, &path_text(input, &next.parts), end);
                admit_path(&mut paths[end], next);
            }
        }
    }
    let mut pool = Vec::new();
    for start in 1..input.len() {
        let prefix = &key[start..];
        if prefix.len() > MAX_WORD || !super::valid_word(prefix) {
            continue;
        }
        for path in &paths[start] {
            let Some((previous_start, previous_end)) = path.parts.last().copied() else {
                continue;
            };
            let previous = &key[previous_start..previous_end];
            let observed = model.top(previous, prefix, BEAM);
            let backoff = path.allows_tail_backoff();
            if observed.is_empty() && !backoff {
                continue;
            }
            let mut tails: Vec<String> = observed
                .into_iter()
                .map(|(id, _, _)| model.word(id).to_owned())
                .collect();
            // A sparse bigram table is not an exhaustive vocabulary. A reliable
            // multiword anchor can complete its final unfinished word from the
            // bounded unigram stream, still ranked by the same smoothed model.
            // A lone dictionary token (including repeated noise) grants no such access.
            for id in unigram_top(
                vocab,
                prefix,
                if backoff {
                    BACKOFF_PREFIX_CANDIDATES
                } else {
                    BEAM
                },
            ) {
                let word = vocab.word(id);
                if (backoff || model.observed(previous, word))
                    && !tails.iter().any(|tail| tail == word)
                {
                    tails.push(word.to_owned());
                }
            }
            let beginning = path_text(input, &path.parts);
            for word in tails {
                if word.len() > MAX_WORD || !word.starts_with(prefix) || !super::valid_word(&word) {
                    continue;
                }
                let suffix = casing(&word, &input[start..]);
                let text = format!("{beginning} {suffix}");
                let mut candidate = if let Some(id) = vocab.find(&word) {
                    vocab.candidate(id, &input[start..], &metadata)
                } else {
                    Candidate::new(suffix.clone(), CandidateSource::Local)
                };
                candidate.language = CandidateLanguage::English;
                candidate.consumed = input.len();
                candidate.display_text = Some(suffix.clone());
                candidate.text = text.clone();
                candidate.score = score(vocab, &text, input.len());
                if candidate.annotation.is_none() {
                    if let Some(entry) = metadata.lookup(&word) {
                        candidate.comment = entry.comment();
                        candidate.annotation = Some(entry.annotation(false));
                    }
                }
                admit_suggestion(
                    &mut pool,
                    Suggestion {
                        candidate: candidate.clone(),
                        phrase: false,
                    },
                    cap,
                );
                if !phrases || cap < 3 || path.parts.len() + 2 > MAX_WORDS {
                    continue;
                }
                let Some((next_id, count, total)) = model.top(&word, "", 1).into_iter().next()
                else {
                    continue;
                };
                let next = model.word(next_id);
                if (count as f64 / total as f64) < PHRASE_CONFIDENCE
                    || next.len() < 2
                    || next.len() > MAX_WORD
                    || !super::valid_word(next)
                {
                    continue;
                }
                let next = if input
                    .bytes()
                    .all(|b| !b.is_ascii_alphabetic() || b.is_ascii_uppercase())
                {
                    next.to_ascii_uppercase()
                } else {
                    casing(next, next)
                };
                candidate.text = format!("{text} {next}");
                let display_suffix = format!("{suffix} {next}");
                candidate.display_text = Some(display_suffix.clone());
                annotate_phrase(&mut candidate, &display_suffix, &metadata);
                candidate.score = score(vocab, &candidate.text, input.len());
                admit_suggestion(
                    &mut pool,
                    Suggestion {
                        candidate,
                        phrase: true,
                    },
                    cap,
                );
            }
        }
    }
    pool.sort_by(|a, b| {
        b.candidate
            .score
            .total_cmp(&a.candidate.score)
            .then_with(|| a.candidate.text.cmp(&b.candidate.text))
    });
    let mut phrase_count = 0;
    pool.into_iter()
        .filter_map(|suggestion| {
            if suggestion.phrase {
                if phrase_count >= cap / 3 {
                    return None;
                }
                phrase_count += 1;
            }
            Some(suggestion.candidate)
        })
        .take(cap)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn vocabulary() -> Vocabulary {
        let source=b"a 9081174698\nam 576436203\nare 1000000000\napple 50551171\nhow 1000000000\ni 3086225277\niambic 110064\ni'm 1000000\nthe 20000000000\nthinking 20000000\ntalking 10000000\nthank 10000000\nto 12136980858\ntrying 57462639\nwe 1000000000\nyou 1000000000\nyour 1000000000\n";
        let mut bytes = Vec::new();
        super::super::compile(&source[..], &mut bytes).unwrap();
        Vocabulary::from_bytes(bytes).unwrap()
    }
    #[test]
    fn pinned_context_prefix_topk_has_real_support_and_consistent_rows() {
        let model = model().unwrap();
        let hits = model.top("i", "a", 3);
        assert_eq!(model.word(hits[0].0), "am");
        assert!(model
            .top("am", "t", 5)
            .iter()
            .any(|(id, _, _)| model.word(*id) == "trying"));
        let (id, count, total) = model.top("trying", "", 1)[0];
        assert_eq!(model.word(id), "to");
        assert!(count as f64 / total as f64 > PHRASE_CONFIDENCE);
        assert!(model.top("absent-previous", "t", 5).is_empty());
        assert!(model.top("i", "a", 0).is_empty());
    }
    #[test]
    fn complete_words_remain_whole_and_unknown_and_limits_remain_bounded() {
        let vocab = vocabulary();
        for input in ["apple", "iambic", "APPLE", "qzxqzx", "i am", "iam'", ""] {
            assert!(complete(&vocab, input, 24, true).is_empty(), "{input}");
        }
        assert!(complete(&vocab, "iamt", 0, true).is_empty());
        assert!(complete(&vocab, &"i".repeat(MAX_INPUT + 1), 24, true).is_empty());
        assert!(complete(&vocab, "iamt", usize::MAX, true).len() <= MAX_RESULTS);
    }
    #[test]
    fn incomplete_tail_is_completed_with_display_suffix_and_full_commit() {
        let vocab = vocabulary();
        for (input, expected, suffix) in [
            ("ia", "I am", "am"),
            ("iamt", "I am trying", "trying"),
            ("iamtr", "I am trying", "trying"),
            ("howarey", "how are you", "you"),
            ("thanky", "thank you", "you"),
            ("wearet", "we are trying", "trying"),
        ] {
            let hits = complete(&vocab, input, 24, false);
            let candidate = hits
                .iter()
                .find(|candidate| candidate.text == expected)
                .unwrap_or_else(|| {
                    panic!(
                        "missing {input} -> {expected}: {:?}",
                        hits.iter()
                            .map(|candidate| &candidate.text)
                            .collect::<Vec<_>>()
                    )
                });
            assert_eq!(candidate.display_text.as_deref(), Some(suffix));
            assert_eq!(candidate.consumed, input.len());
            assert_eq!(candidate.score, score(&vocab, &candidate.text, input.len()));
        }
    }
    #[test]
    fn phrase_expansion_is_optional_costed_and_does_not_hide_stems() {
        let vocab = vocabulary();
        let hits = complete(&vocab, "iamt", 5, true);
        let trying = hits
            .iter()
            .position(|candidate| candidate.text == "I am trying")
            .unwrap();
        let phrase = hits
            .iter()
            .position(|candidate| candidate.text == "I am trying to")
            .unwrap();
        assert!(trying < phrase);
        assert!(hits[trying].score > hits[phrase].score);
        assert_eq!(hits[phrase].display_text.as_deref(), Some("trying to"));
        assert!(hits[phrase].annotation.is_none());
        assert_eq!(hits[phrase].comment, "短语补全");
        assert_eq!(
            hits.iter()
                .filter(|candidate| candidate.text == "I am trying to")
                .count(),
            1
        );
        assert!(!complete(&vocab, "iamt", 24, false)
            .iter()
            .any(|candidate| candidate.text == "I am trying to"));
    }
    #[test]
    fn missing_phrase_definition_clears_stem_metadata_without_changing_commit_or_display() {
        let metadata = lexicon::fixture();
        let entry = metadata.lookup("apple").unwrap();
        let mut candidate = Candidate::new("I like apple to", CandidateSource::Local);
        candidate.display_text = Some("apple to".into());
        candidate.comment = entry.comment();
        candidate.annotation = Some(entry.annotation(false));
        assert!(!candidate.comment.is_empty());
        assert!(candidate.annotation.is_some());
        annotate_phrase(&mut candidate, "apple to", &metadata);
        assert_eq!(candidate.comment, "短语补全");
        assert!(candidate.annotation.is_none());
        assert_eq!(candidate.text, "I like apple to");
        assert_eq!(candidate.display_text.as_deref(), Some("apple to"));
    }
    #[test]
    fn casing_and_apostrophes_survive_without_mutating_the_raw_input() {
        let vocab = vocabulary();
        assert!(complete(&vocab, "IAMTR", 24, false)
            .iter()
            .any(|candidate| candidate.text == "I AM TRYING"));
        let input = "i'mtr".to_owned();
        // This frozen corpus has no "i'm" context row. Preserve the literal
        // spelling rather than invent a contraction-normalization exception.
        assert!(complete(&vocab, &input, 24, false).is_empty());
        assert_eq!(input, "i'mtr");
        assert!(score(&vocab, "I am trying to", 4) < score(&vocab, "I am trying", 4));
        assert!(score(&vocab, "invalid  text", 4).is_infinite());
    }
    #[test]
    fn malformed_embedded_shape_declines_without_io_or_panics() {
        for end in [0, 7, 31, DATA.len() - 1] {
            assert!(Model::open(&DATA[..end]).is_none());
        }
    }
    #[test]
    fn repeated_dictionary_noise_requires_observed_context_before_suggesting() {
        let mut bytes = Vec::new();
        super::super::compile(&b"zzz 693209\nzone 1000000\nzero 1000000\n"[..], &mut bytes)
            .unwrap();
        let vocab = Vocabulary::from_bytes(bytes).unwrap();
        let hits = complete(&vocab, "zzzzzzzzzz", 24, true);
        assert!(
            hits.is_empty(),
            "noise candidates: {:?}",
            hits.iter()
                .map(|candidate| (&candidate.text, &candidate.display_text))
                .collect::<Vec<_>>()
        );
    }
    #[test]
    fn reliable_multiword_anchor_keeps_sparse_corpus_gaps_in_the_tail_vocabulary() {
        // Use the actual frozen unigram source, rather than adding target words
        // only to a tiny fixture and accidentally hiding top-K ranking mistakes.
        let source = std::fs::File::open(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../data/dict/raw/english/frequency.txt"
        ))
        .unwrap();
        let mut bytes = Vec::new();
        super::super::compile(std::io::BufReader::new(source), &mut bytes).unwrap();
        let vocab = Vocabulary::from_bytes(bytes).unwrap();
        let model = model().unwrap();
        assert!(!model.observed("am", "gay"));
        assert!(!model.observed("am", "good"));
        for (input, expected) in [
            ("iamg", "I am good"),
            ("iamgo", "I am good"),
            ("iamgay", "I am gay"),
            ("iamgood", "I am good"),
            ("iamt", "I am trying"),
        ] {
            assert!(
                complete(&vocab, input, 24, true)
                    .iter()
                    .any(|candidate| candidate.text == expected),
                "missing {input} -> {expected}"
            );
        }
        let ga = complete(&vocab, "iamga", 5, true);
        assert!(
            ga.iter().any(|candidate| candidate.text == "I am gay"),
            "ga first page: {:?}",
            ga.iter()
                .map(|candidate| &candidate.text)
                .collect::<Vec<_>>()
        );
        for (input, expected) in [("iamgay", "I am gay"), ("iamgood", "I am good")] {
            assert_eq!(complete(&vocab, input, 5, true)[0].text, expected);
        }
        for input in [
            "zzzzzzzzzz",
            "zzzga",
            "zzzgood",
            "qzxqzx",
            "apple",
            "iambic",
            "international",
            "imag",
        ] {
            assert!(
                complete(&vocab, input, 24, true).is_empty(),
                "unexpected split for {input}"
            );
        }
    }
}
