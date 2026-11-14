//! Bounded exact-word segmentation with a final unfinished word prefix.
//! The raw composition stays authoritative; every result commits one complete
//! alternative only after explicit selection. No IO, learning or cloud calls.
use super::{casing, lexicon, ranking, Vocabulary};
use retype_types::{Candidate, CandidateLanguage, CandidateSource};
use std::{cmp::Reverse, collections::BinaryHeap, sync::OnceLock};

pub(super) const MAX_INPUT: usize = 128;
const MAX_WORD: usize = 24;
const MAX_WORDS: usize = 32;
const BEAM: usize = 8;
const MAX_RESULTS: usize = 24;
const SPLIT_COST: f64 = 0.4;
const COMPLETION_CHAR_COST: f64 = 0.04;
const EXTRA_WORD_COST: f64 = 0.15;
const CONTEXT_WEIGHT: f64 = 0.95;
const PHRASE_CONFIDENCE: f64 = 0.7;
const MIN_ANCHOR_PROBABILITY: f64 = 0.005;
const BACKOFF_PREFIX_CANDIDATES: usize = BEAM * 2;
const EXACT_MAX_WORDS: usize = MAX_WORDS;
const MAX_BACKOFF_RUN: usize = 2;
const MIN_BACKOFF_WORD_PROBABILITY: f64 = 0.000_001;
const EXACT_BEAM: usize = 32;
const EXACT_MIN_WORD_PROBABILITY: f64 = 0.000_001;
const EXACT_MIN_LOG_GAP: f64 = std::f64::consts::LN_2 * 3.;
const EXACT_MIN_ALIAS_PRIOR_RATIO: u64 = 8;
const EXACT_MIN_CONTEXT_EDGES: usize = 8;
const EXACT_MIN_CONTEXTUAL_ODDS: f64 = 4.;
const DATA: &[u8] = include_bytes!("../assets/bigrams.bin");

pub(super) fn valid_composition(input: &str) -> bool {
    !input.is_empty()
        && input.len() <= MAX_INPUT
        && input.as_bytes()[0].is_ascii_alphabetic()
        && input.as_bytes()[input.len() - 1].is_ascii_alphabetic()
        && input.bytes().all(|b| b.is_ascii_alphabetic() || b == b'\'')
}

fn contraction_alias(key: &str) -> Option<ranking::Row<'static>> {
    ranking::current()?.contraction(key)
}

fn is_contraction(word: &str) -> bool {
    word.contains('\'')
        && ranking::current()
            .and_then(|overlay| overlay.get(word))
            .is_some_and(|row| row.reason == ranking::Reason::Contraction)
}

/// Recover evidence from the existing model for unambiguous expansions of
/// reviewed contractions. The expansion is used solely for context lookup.
fn contraction_context(word: &str) -> Option<(&str, &str)> {
    if !is_contraction(word) {
        return None;
    }
    if let Some(base) = word.strip_suffix("n't") {
        let head = match base {
            "ca" => "can",
            "wo" => "will",
            _ => base,
        };
        return Some((head, "not"));
    }
    let (head, ending) = word.split_once('\'')?;
    let tail = match ending {
        "m" => "am",
        "re" => "are",
        "ve" => "have",
        "ll" => "will",
        "s" if head == "let" => "us",
        _ => return None,
    };
    Some((head, tail))
}

fn completed_words<'a>(vocab: &Vocabulary, key: &'a str) -> [Option<&'a str>; 2] {
    if !super::valid_word(key) || key.len() == 1 && !matches!(key, "a" | "i") {
        return [None, None];
    }
    [
        (vocab.find(key).is_some() || is_contraction(key)).then_some(key),
        contraction_alias(key).map(|row| row.word),
    ]
}

fn completed_text(word: &str, raw: &str) -> String {
    if !is_contraction(word) {
        return casing(word, raw);
    }
    // Copy every typed letter in its original case and insert only the quote
    // present in the exact reviewed alias. Contractions of I capitalize I.
    let mut letters = raw.bytes().filter(u8::is_ascii_alphabetic);
    word.bytes()
        .enumerate()
        .map(|(index, byte)| {
            if byte == b'\'' {
                '\''
            } else {
                let letter = letters.next().unwrap_or(byte);
                char::from(if index == 0 && word.starts_with("i'") {
                    letter.to_ascii_uppercase()
                } else {
                    letter
                })
            }
        })
        .collect()
}

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
        self.smoothed(previous, self.context_probability(previous, next), unigram)
    }
    fn smoothed(&self, previous: &str, observed: Option<f64>, unigram: f64) -> f64 {
        if observed.is_none()
            && !self
                .find(previous)
                .and_then(|id| self.row(id))
                .is_some_and(|(_, _, total)| total != 0)
        {
            return unigram;
        }
        (CONTEXT_WEIGHT * observed.unwrap_or(0.) + (1. - CONTEXT_WEIGHT) * unigram)
            .max(f64::MIN_POSITIVE)
    }
    fn context_probability(&self, previous: &str, next: &str) -> Option<f64> {
        if let Some(probability) = self.observed_probability(previous, next) {
            return Some(probability);
        }
        let left = contraction_context(previous).map_or(previous, |(_, tail)| tail);
        let right = contraction_context(next).map_or(next, |(head, _)| head);
        let expanded = (left != previous || right != next)
            .then(|| self.observed_probability(left, right))
            .flatten();
        let plain_left = is_contraction(previous).then(|| previous.replace('\'', ""));
        let plain_right = is_contraction(next).then(|| next.replace('\'', ""));
        if plain_left.is_none() && plain_right.is_none() {
            return None;
        }
        let plain = self.observed_probability(
            plain_left.as_deref().unwrap_or(previous),
            plain_right.as_deref().unwrap_or(next),
        );
        expanded
            .into_iter()
            .chain(plain)
            .reduce(f64::max)
            .map(|p| p * 0.5)
    }
    #[cfg(test)]
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
    // Overlay-adjusted counts: spoken contractions, demoted archaic words.
    let count = vocab.prior(word).max(1);
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
pub(super) fn unigram_top(vocab: &Vocabulary, prefix: &str, limit: usize) -> Vec<usize> {
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
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Part<'a> {
    start: usize,
    end: usize,
    word: &'a str,
}

#[derive(Clone)]
struct Path<'a> {
    parts: Vec<Part<'a>>,
    score: f64,
    // Sum only genuinely observed completed-word transitions, never unigram
    // smoothing. This is an admission check, independent of candidate ranking.
    context_log_sum: f64,
    observed_edges: usize,
    backoff_run: usize,
}
impl Path<'_> {
    fn allows_tail_backoff(&self) -> bool {
        self.observed_edges > 0
            && self.context_log_sum / self.observed_edges as f64 >= MIN_ANCHOR_PROBABILITY.ln()
    }
}
fn path_text(input: &str, parts: &[Part<'_>]) -> String {
    parts
        .iter()
        .map(|part| completed_text(part.word, &input[part.start..part.end]))
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Clone)]
struct ExactPath<'a> {
    parts: Vec<Part<'a>>,
    score: f64,
    observed_edges: usize,
}
impl ExactPath<'_> {
    fn has_dense_context(&self, model: &Model<'_>) -> bool {
        if self.observed_edges < EXACT_MIN_CONTEXT_EDGES
            || self.observed_edges * 5 < self.parts.len().saturating_sub(1) * 4
        {
            return false;
        }
        let mut context_log_sum = 0.;
        let mut missing = 0;
        for pair in self.parts.windows(2) {
            if let Some(probability) = model.context_probability(pair[0].word, pair[1].word) {
                context_log_sum += probability.ln();
                missing = 0;
            } else {
                missing += 1;
                if missing > MAX_BACKOFF_RUN {
                    return false;
                }
            }
        }
        context_log_sum / self.observed_edges as f64 >= MIN_ANCHOR_PROBABILITY.ln()
    }

    fn has_confident_margin(&self, runners: &[Self], model: &Model<'_>) -> bool {
        if runners
            .first()
            .is_none_or(|runner| self.score - runner.score >= EXACT_MIN_LOG_GAP)
        {
            return true;
        }
        // A densely observed sentence can tolerate a closer alternative.
        // Compare with the combined weight of every retained full path so
        // several competing interpretations cannot each pass on their own.
        self.has_dense_context(model)
            && runners
                .iter()
                .map(|runner| (runner.score - self.score).exp())
                .sum::<f64>()
                <= 1. / EXACT_MIN_CONTEXTUAL_ODDS
    }
}
fn exact_contraction_bonus(
    vocab: &Vocabulary,
    model: &Model<'_>,
    key: &str,
    parts: &[Part<'_>],
    next: &str,
) -> f64 {
    let [.., left, contraction] = parts else {
        return 0.;
    };
    let typed = &key[contraction.start..contraction.end];
    if typed == contraction.word {
        return 0.;
    }
    let Some((head, tail)) = contraction_context(contraction.word) else {
        return 0.;
    };
    let literal_frequency = vocab.prior(typed);
    if literal_frequency == 0 {
        return 0.;
    }
    let ratio = vocab.prior(contraction.word) as f64 / literal_frequency as f64;
    // Both neighbors must support the contraction. An expanded edge provides
    // additional evidence beyond the corpus's apostrophe-free spelling alone.
    if ratio < EXACT_MIN_ALIAS_PRIOR_RATIO as f64
        || model
            .context_probability(left.word, contraction.word)
            .is_none()
        || model.observed_probability(left.word, head).is_none()
            && model.observed_probability(tail, next).is_none()
    {
        return 0.;
    }
    ratio.min(16.).ln()
}
fn exact_order(left: &ExactPath<'_>, right: &ExactPath<'_>) -> std::cmp::Ordering {
    right
        .score
        .total_cmp(&left.score)
        .then_with(|| left.parts.len().cmp(&right.parts.len()))
        .then_with(|| left.parts.cmp(&right.parts))
}
fn admit_exact<'a>(paths: &mut Vec<ExactPath<'a>>, path: ExactPath<'a>) {
    let index = paths.partition_point(|old| exact_order(old, &path).is_le());
    if index < EXACT_BEAM {
        paths.insert(index, path);
        paths.truncate(EXACT_BEAM);
    }
}

/// Exact completed-word paths are independent of visible completion candidates.
/// Add spaces and exact reviewed contraction quotes without changing letters.
/// All snapshots are already in memory; this function never opens a resource.
pub(super) fn segment_exact(
    vocab: &Vocabulary,
    input: &str,
    metadata: &lexicon::Lexicon,
    manual: &lexicon::ManualWords,
) -> Option<String> {
    // A reviewed conversational abbreviation is already a complete spelling.
    // Its explanatory full form belongs in annotation, never Space expansion.
    if crate::modern::protects_word(input) {
        return None;
    }
    if input.len() < 2 || !valid_composition(input) {
        return None;
    }
    let key = input.to_ascii_lowercase();
    // Protect a complete spelling before considering a decomposition. Manual
    // entries can be codes/names and are intentionally not split or learned.
    if vocab.find(&key).is_some()
        || metadata.lookup(&key).is_some()
        || manual
            .complete(input, 1)
            .first()
            .is_some_and(|candidate| candidate.text.eq_ignore_ascii_case(input))
    {
        return None;
    }
    if let Some(row) = contraction_alias(&key) {
        return Some(completed_text(row.word, input));
    }
    let model = model()?;
    let mut paths: Vec<Vec<ExactPath<'_>>> = (0..=input.len()).map(|_| Vec::new()).collect();
    paths[0].push(ExactPath {
        parts: Vec::new(),
        score: 0.,
        observed_edges: 0,
    });
    for start in 0..input.len() {
        if paths[start].is_empty() {
            continue;
        }
        let previous_paths = paths[start].clone();
        for end in start + 1..=(start + MAX_WORD).min(input.len()) {
            let typed = &key[start..end];
            if typed.len() > 1 && typed.bytes().all(|value| value == typed.as_bytes()[0]) {
                continue;
            }
            for word in completed_words(vocab, typed).into_iter().flatten() {
                // Space is an automatic commit. An inferred quote should not
                // compete with an established ordinary spelling on weak priors.
                if word != typed
                    && vocab.prior(word)
                        < vocab
                            .prior(typed)
                            .saturating_mul(EXACT_MIN_ALIAS_PRIOR_RATIO)
                {
                    continue;
                }
                let prior = unigram(vocab, word);
                for path in &previous_paths {
                    if path.parts.len() >= EXACT_MAX_WORDS {
                        continue;
                    }
                    let observed = path
                        .parts
                        .last()
                        .and_then(|part| model.context_probability(part.word, word));
                    let probability = path
                        .parts
                        .last()
                        .map_or(prior, |part| model.smoothed(part.word, observed, prior));
                    let bonus = if observed.is_some() {
                        exact_contraction_bonus(vocab, model, &key, &path.parts, word)
                    } else {
                        0.
                    };
                    let mut next = path.clone();
                    next.observed_edges += usize::from(observed.is_some());
                    next.parts.push(Part { start, end, word });
                    next.score +=
                        probability.ln() + bonus - if start == 0 { 0. } else { SPLIT_COST };
                    admit_exact(&mut paths[end], next);
                }
            }
        }
    }
    let (best, runners) = paths[input.len()].split_first()?;
    if best.parts.len() < 2
        // Dictionary abbreviations alone are not enough evidence for an
        // automatic boundary (e.g. an unfinished "iamt" is not "I amt").
        // One genuine context edge anchors a path; gaps after that may back off.
        || best.observed_edges == 0
        || best.score / (best.parts.len() as f64) < EXACT_MIN_WORD_PROBABILITY.ln()
        || !best.has_confident_margin(runners, model)
    {
        return None;
    }
    let output = path_text(input, &best.parts);
    let letters = |value: &str| {
        value
            .bytes()
            .filter(u8::is_ascii_alphabetic)
            .map(|byte| byte.to_ascii_lowercase())
            .collect::<Vec<_>>()
    };
    (letters(&output) == letters(input)).then_some(output)
}
fn admit_path<'a>(paths: &mut Vec<Path<'a>>, path: Path<'a>) {
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
#[cfg(test)]
pub(super) fn complete(
    vocab: &Vocabulary,
    input: &str,
    limit: usize,
    phrases: bool,
) -> Vec<Candidate> {
    complete_with_pinyin(vocab, input, limit, phrases, false, &lexicon::current())
}
/// Final pinyin is an alternative tail lookup, never part of exact Space segmentation.
pub(super) fn complete_with_pinyin(
    vocab: &Vocabulary,
    input: &str,
    limit: usize,
    phrases: bool,
    pinyin: bool,
    metadata: &lexicon::Lexicon,
) -> Vec<Candidate> {
    let cap = limit.min(MAX_RESULTS);
    if cap == 0 || input.len() < 2 || !valid_composition(input) {
        return Vec::new();
    }
    let key = input.to_ascii_lowercase();
    // Ordinary complete words have a fully literal interpretation. Do not split
    // them merely because short high-frequency fragments can also be found.
    if vocab.find(&key).is_some()
        || metadata.lookup(&key).is_some()
        || contraction_alias(&key).is_some()
    {
        return Vec::new();
    }
    let Some(model) = model() else {
        return Vec::new();
    };
    // Whole exact pinyin retains its established stream, just like an English
    // whole word. Do not reinterpret its initial syllables as English prefixes.
    let pinyin = pinyin && !metadata.pinyin(input, 1).0;
    let mut paths: Vec<Vec<Path<'_>>> = vec![Vec::new(); input.len() + 1];
    paths[0].push(Path {
        parts: Vec::new(),
        score: 0.,
        context_log_sum: 0.,
        observed_edges: 0,
        backoff_run: 0,
    });
    let mut strong_whole_prefix = None;
    for start in 0..input.len() - 1 {
        if paths[start].is_empty() {
            continue;
        }
        let previous = paths[start].clone();
        for end in start + 1..=(start + MAX_WORD).min(input.len() - 1) {
            let typed = &key[start..end];
            for word in completed_words(vocab, typed).into_iter().flatten() {
                // An inferred opening quote should not create a new split in
                // a strong whole-word prefix such as imag (image).
                if start == 0
                    && word != typed
                    && *strong_whole_prefix.get_or_insert_with(|| {
                        unigram_top(vocab, &key, 1).first().is_some_and(|&id| {
                            unigram(vocab, vocab.word(id)) >= super::STRONG_ORDINARY
                        })
                    })
                {
                    continue;
                }
                let prior = unigram(vocab, word);
                for path in &previous {
                    if path.parts.len() + 1 >= MAX_WORDS {
                        continue;
                    }
                    let left = path.parts.last();
                    let observed = left.and_then(|part| model.context_probability(part.word, word));
                    if left.is_some()
                        && observed.is_none()
                        && (!path.allows_tail_backoff()
                            || path.backoff_run >= MAX_BACKOFF_RUN
                            || prior < MIN_BACKOFF_WORD_PROBABILITY)
                    {
                        continue;
                    }
                    let probability =
                        left.map_or(prior, |part| model.smoothed(part.word, observed, prior));
                    let mut next = path.clone();
                    next.parts.push(Part { start, end, word });
                    if let Some(observed) = observed {
                        next.context_log_sum += observed.ln();
                        next.observed_edges += 1;
                        next.backoff_run = 0;
                    } else if left.is_some() {
                        next.backoff_run += 1;
                    }
                    // Each advance scores its one new edge. Rebuilding and
                    // rescoring every prefix would scale with its word count.
                    next.score += probability.ln() - if left.is_some() { SPLIT_COST } else { 0. };
                    admit_path(&mut paths[end], next);
                }
            }
        }
    }
    let mut pool = Vec::new();
    let mut raw_tail: Option<(usize, &Path<'_>)> = None;
    for start in 1..input.len() {
        let prefix = &key[start..];
        if let Some(path) = paths[start].iter().find(|path| path.allows_tail_backoff()) {
            // Iteration is in ascending byte order, so a later supported
            // boundary preserves the longest recognized prefix.
            raw_tail = Some((start, path));
        }
        if prefix.len() > MAX_WORD || !super::valid_word(prefix) {
            continue;
        }
        // Look up each tail once, not once per beam path. Use a fixed bounded
        // pool before contextual ranking so a one-row request can still choose
        // a less frequent translation that fits the English prefix better.
        let pinyin_tails = if pinyin
            && paths[start].iter().any(Path::allows_tail_backoff)
            && vocab.find(prefix).is_none()
            && metadata.lookup(prefix).is_none()
        {
            metadata.pinyin(prefix, MAX_RESULTS).1
        } else {
            Vec::new()
        };
        for path in &paths[start] {
            let Some(previous) = path.parts.last().map(|part| part.word) else {
                continue;
            };
            let beginning = path_text(input, &path.parts);
            if path.allows_tail_backoff() {
                for (position, hit) in pinyin_tails.iter().enumerate() {
                    if let Some(candidate) =
                        pinyin_tail(vocab, input, start, &beginning, hit, position)
                    {
                        admit_suggestion(
                            &mut pool,
                            Suggestion {
                                candidate,
                                phrase: false,
                            },
                            cap,
                        );
                    }
                }
            }
            let mut observed = model.top(previous, prefix, BEAM);
            if observed.is_empty() {
                if let Some((_, expanded_tail)) = contraction_context(previous) {
                    observed = model.top(expanded_tail, prefix, BEAM);
                }
                if observed.is_empty() && is_contraction(previous) {
                    observed = model.top(&previous.replace('\'', ""), prefix, BEAM);
                }
            }
            let backoff = path.allows_tail_backoff();
            let mut tails: Vec<String> = observed
                .into_iter()
                .map(|(id, _, _)| model.word(id).to_owned())
                .collect();
            let alias = contraction_alias(prefix);
            // An exact typed word can be rarer than every completion in a
            // top-K bucket. Keep it available, including reviewed contractions.
            for word in completed_words(vocab, prefix).into_iter().flatten() {
                if (backoff || model.context_probability(previous, word).is_some())
                    && !tails.iter().any(|tail| tail == word)
                {
                    tails.push(word.to_owned());
                }
            }
            if tails.is_empty() && !backoff {
                continue;
            }
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
                if (backoff || model.context_probability(previous, word).is_some())
                    && !tails.iter().any(|tail| tail == word)
                {
                    tails.push(word.to_owned());
                }
            }
            for word in tails {
                let alias_match = alias.is_some_and(|row| row.word == word);
                if word.len() > MAX_WORD
                    || (!word.starts_with(prefix) && !alias_match)
                    || !super::valid_word(&word)
                {
                    continue;
                }
                let suffix = if alias_match || word.eq_ignore_ascii_case(&input[start..]) {
                    completed_text(&word, &input[start..])
                } else {
                    casing(&word, &input[start..])
                };
                let text = format!("{beginning} {suffix}");
                let mut candidate = if let Some(id) = vocab.find(&word) {
                    vocab.candidate(id, &input[start..], metadata)
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
                annotate_phrase(&mut candidate, &display_suffix, metadata);
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
    if pool.is_empty() {
        if let Some((start, path)) = raw_tail {
            let suffix = &input[start..];
            let beginning = path_text(input, &path.parts);
            let mut candidate =
                Candidate::new(format!("{beginning} {suffix}"), CandidateSource::Local);
            candidate.language = CandidateLanguage::English;
            candidate.consumed = input.len();
            candidate.display_text = Some(suffix.to_owned());
            candidate.comment = "保留输入".into();
            candidate.score = score(vocab, &candidate.text, input.len());
            return vec![candidate];
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

fn pinyin_tail(
    vocab: &Vocabulary,
    input: &str,
    start: usize,
    beginning: &str,
    hit: &lexicon::PinyinHit<'_>,
    position: usize,
) -> Option<Candidate> {
    if hit.word.len() > MAX_WORD || !super::valid_word(hit.word) {
        return None;
    }
    let suffix = casing(hit.word, &input[start..]);
    let mut candidate = Candidate::new(format!("{beginning} {suffix}"), CandidateSource::Local);
    candidate.language = CandidateLanguage::English;
    candidate.consumed = input.len();
    candidate.display_text = Some(suffix);
    candidate.comment = hit.comment();
    candidate.annotation = Some(hit.annotation(true));
    // Ordinary assist scores are ln(frequency), not continuous log-probability.
    // Rank translations on the same complete-text scale as English candidates.
    let evidence_cost = position as f32 * 0.02
        + if hit.exact { 0. } else { 0.5 }
        + if hit.flags & 4 != 0 { 2. } else { 0. }
        + if hit.flags & 8 != 0 { 1. } else { 0. };
    candidate.score = score(vocab, &candidate.text, input.len()) - evidence_cost;
    Some(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frozen_vocabulary() -> Vocabulary {
        let source = std::fs::File::open(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../data/dict/raw/english/frequency.txt"
        ))
        .unwrap();
        let mut bytes = Vec::new();
        super::super::compile(std::io::BufReader::new(source), &mut bytes).unwrap();
        Vocabulary::from_bytes(bytes).unwrap()
    }
    fn vocabulary() -> Vocabulary {
        let source=b"a 9081174698\nam 576436203\nare 1000000000\napple 50551171\nhow 1000000000\ni 3086225277\niambic 110064\ni'm 1000000\nthe 20000000000\nthinking 20000000\ntalking 10000000\nthank 10000000\nto 12136980858\ntrying 57462639\nwe 1000000000\nyou 1000000000\nyour 1000000000\n";
        let mut bytes = Vec::new();
        super::super::compile(&source[..], &mut bytes).unwrap();
        Vocabulary::from_bytes(bytes).unwrap()
    }
    #[test]
    fn joined_english_pinyin_tail_keeps_complete_commit_and_chinese_evidence() {
        let vocab = frozen_vocabulary();
        let metadata = lexicon::fixture();
        for (input, hanzi, reading) in [
            ("iwilldotheyiyang", "一样", "yi1 yang4"),
            ("iwilldothetongyang", "同样", "tong2 yang4"),
        ] {
            for limit in [1, 5, 24] {
                for phrases in [false, true] {
                    let hits = complete_with_pinyin(&vocab, input, limit, phrases, true, &metadata);
                    let candidate = hits
                        .iter()
                        .take(5)
                        .find(|candidate| candidate.text == "I will do the same")
                        .unwrap_or_else(|| {
                            panic!(
                                "missing first-page mixed tail for {input}: {:?}",
                                hits.iter().map(|c| &c.text).collect::<Vec<_>>()
                            )
                        });
                    assert_eq!(candidate.display_text.as_deref(), Some("same"));
                    assert_eq!(candidate.consumed, input.len());
                    assert_eq!(candidate.comment, "adj. 相同的；同样的");
                    let annotation = candidate.annotation.as_ref().unwrap();
                    assert!(annotation.pinyin_match && annotation.pinyin_exact);
                    assert_eq!(annotation.matched_chinese, hanzi);
                    assert_eq!(annotation.matched_pinyin, reading);
                    assert!(!candidate.text.ends_with(' '));
                    assert!(hits.len() <= limit.min(MAX_RESULTS));
                }
            }
        }
        let prefix = complete_with_pinyin(&vocab, "iwilldotheyiy", 5, false, true, &metadata);
        let same = prefix
            .iter()
            .find(|candidate| candidate.text == "I will do the same")
            .unwrap();
        assert!(!same.annotation.as_ref().unwrap().pinyin_exact);
        assert_eq!(same.annotation.as_ref().unwrap().matched_chinese, "一样");
        let upper = complete_with_pinyin(&vocab, "IWILLDOTHETONGYANG", 5, false, true, &metadata);
        assert!(upper
            .iter()
            .any(|candidate| candidate.text == "I WILL DO THE SAME"
                && candidate.display_text.as_deref() == Some("SAME")));
    }
    #[test]
    fn mixed_tail_retains_multiple_english_boundaries_without_greedy_prefix_commit() {
        let vocab = frozen_vocabulary();
        let hits = complete_with_pinyin(&vocab, "iamanquan", 24, false, true, &lexicon::fixture());
        assert!(hits.iter().any(|candidate| candidate.text == "I am safe"
            && candidate.display_text.as_deref() == Some("safe")));
        assert!(hits.iter().any(|candidate| candidate.text == "I am an all"
            && candidate.display_text.as_deref() == Some("all")));
        assert!(hits
            .iter()
            .all(|candidate| candidate.consumed == "iamanquan".len()));
    }
    #[test]
    fn mixed_tail_requires_pinyin_and_reliable_prefix_and_never_changes_exact_space() {
        let vocab = frozen_vocabulary();
        let metadata = lexicon::fixture();
        for input in ["iwilldotheyiyang", "iwilldothetongyang"] {
            assert!(
                !complete_with_pinyin(&vocab, input, 24, true, false, &metadata)
                    .iter()
                    .any(|candidate| candidate
                        .annotation
                        .as_ref()
                        .is_some_and(|note| note.pinyin_match))
            );
            assert!(complete_with_pinyin(&vocab, input, 0, true, true, &metadata).is_empty());
            assert!(!complete_with_pinyin(
                &vocab,
                input,
                24,
                true,
                true,
                &lexicon::Lexicon::empty()
            )
            .iter()
            .any(|candidate| candidate.text == "I will do the same"));
            if let Some(spaced) =
                segment_exact(&vocab, input, &metadata, &lexicon::ManualWords::default())
            {
                assert!(spaced.replace(' ', "").eq_ignore_ascii_case(input));
            }
        }
        for input in [
            "zzzyiyang",
            "qzxtongyang",
            "nihao",
            "apple",
            "iambic",
            "international",
            "iam机器人",
        ] {
            assert!(
                complete_with_pinyin(&vocab, input, 24, false, true, &metadata)
                    .iter()
                    .all(|candidate| !candidate
                        .annotation
                        .as_ref()
                        .is_some_and(|note| note.pinyin_match)),
                "{input}"
            );
        }
        for input in ["iamt", "iloveyou", "thisisan"] {
            let old = complete(&vocab, input, 24, false);
            let new = complete_with_pinyin(&vocab, input, 24, false, true, &metadata);
            assert_eq!(
                old.iter().map(|c| &c.text).collect::<Vec<_>>(),
                new.iter().map(|c| &c.text).collect::<Vec<_>>(),
                "{input}"
            );
        }
        assert!(complete_with_pinyin(
            &vocab,
            &"i".repeat(MAX_INPUT + 1),
            24,
            false,
            true,
            &metadata
        )
        .is_empty());
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
        assert!(!model().unwrap().observed("i'm", "trying"));
        assert!(model().unwrap().observed("am", "trying"));
        assert!(complete(&vocab, &input, 24, false)
            .iter()
            .any(|candidate| candidate.text == "I'm trying"));
        assert_eq!(input, "i'mtr");
        assert!(score(&vocab, "I am trying to", 4) < score(&vocab, "I am trying", 4));
        assert!(score(&vocab, "invalid  text", 4).is_infinite());
    }
    #[test]
    fn reviewed_contractions_consume_typed_letters_and_keep_an_unknown_tail() {
        let vocab = frozen_vocabulary();
        let model = model().unwrap();
        assert!(!model.observed("i", "don't"));
        assert!(!model.observed("don't", "want"));
        assert!(model.observed("i", "do") && model.observed("not", "want"));
        for (raw, expected) in [
            ("idont", "I don't"),
            ("idontw", "I don't want"),
            ("idontwanttosaygoodbye", "I don't want to say goodbye"),
        ] {
            let hits = complete(&vocab, raw, 24, false);
            assert_eq!(
                hits.first().map(|c| c.text.as_str()),
                Some(expected),
                "{raw}"
            );
            assert_eq!(hits[0].consumed, raw.len());
        }
        assert_eq!(
            vocab.segment_exact("idontwanttosaygoodbye").as_deref(),
            Some("I don't want to say goodbye")
        );
        assert!(vocab.segment_exact("idontw").is_none());
        let raw = "idontwantQzxQzx";
        let hits = complete(&vocab, raw, 24, false);
        let tail = hits.first().expect("supported prefix keeps the typed tail");
        assert_eq!(tail.text, "I don't want QzxQzx");
        assert_eq!(tail.display_text(), "QzxQzx");
        assert_eq!(tail.consumed, raw.len());
        assert!(vocab.segment_exact(raw).is_none());
    }
    #[test]
    fn exact_contraction_conflicts_use_supported_context_and_preserve_ordinary_words() {
        let vocab = frozen_vocabulary();
        for (raw, expected) in [
            ("icantwait", "I can't wait"),
            ("weweregoing", "we were going"),
        ] {
            assert_eq!(vocab.segment_exact(raw).as_deref(), Some(expected));
        }
        for literal in ["cant", "well", "were", "its"] {
            assert!(vocab.segment_exact(literal).is_none(), "{literal}");
        }
    }
    #[test]
    fn exact_long_sentence_keeps_all_typed_letters() {
        let vocab = frozen_vocabulary();
        let raw = "idontwanttosaygoodbyebutihavetogohomeandiwillcallyouwhenigettheretonight";
        let expected = "I don't want to say goodbye but I have to go home and I will call you when I get there tonight";
        assert_eq!(vocab.segment_exact(raw).as_deref(), Some(expected));
        let with_unknown = raw.replacen("goodbye", "goodbyeqzxvv", 1);
        assert!(vocab.segment_exact(&with_unknown).is_none());
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
    #[test]
    fn exact_segmentation_keeps_every_typed_letter_with_sparse_internal_context() {
        let vocab = frozen_vocabulary();
        assert!(!model().unwrap().observed("am", "robot"));
        for (input, expected) in [
            ("iamrobot", "I am robot"),
            ("iamrobotandilikeapples", "I am robot and I like apples"),
            ("iamgood", "I am good"),
            ("iamgay", "I am gay"),
            ("helloworld", "hello world"),
            ("iloveyou", "I love you"),
        ] {
            let result = vocab
                .segment_exact(input)
                .unwrap_or_else(|| panic!("no exact split for {input}"));
            assert_eq!(result, expected);
            assert!(result.replace(' ', "").eq_ignore_ascii_case(input));
            assert!(!result.starts_with(' ') && !result.ends_with(' '));
        }
    }
    #[test]
    fn exact_segmentation_never_completes_corrects_transliterates_or_splits_complete_words() {
        let vocab = frozen_vocabulary();
        for input in [
            "iamt",
            "iamro",
            "iamtr",
            "iamq",
            "abcxyz",
            "hellp",
            "jiqiren",
            "pingguo",
            "qzxvfixture",
            "zzzga",
            "zzzzzzzzzz",
            "iamzzzzzz",
            "iambic",
            "international",
            "HEL",
            "apple",
            "don't",
            "somebody",
            "nowhere",
        ] {
            assert!(
                vocab.segment_exact(input).is_none(),
                "unexpected exact split for {input}"
            );
        }
    }
    #[test]
    fn exact_segmentation_preserves_typed_case_except_standalone_i() {
        let vocab = frozen_vocabulary();
        for (input, expected) in [
            ("IAMROBOT", "I AM ROBOT"),
            ("IamRobot", "I am Robot"),
            ("iAmRobot", "I Am Robot"),
        ] {
            assert_eq!(vocab.segment_exact(input).as_deref(), Some(expected));
        }
    }
    #[test]
    fn exact_segmentation_declines_ambiguous_unknown_and_over_limit_input() {
        let vocab = frozen_vocabulary();
        for input in [
            "",
            "i am",
            "iamrobot!",
            "iamr0bot",
            "iam机器人",
            "iam'",
            "godisnowhere",
        ] {
            assert!(
                vocab.segment_exact(input).is_none(),
                "unsafe exact split for {input}"
            );
        }
        assert!(vocab.segment_exact(&"i".repeat(MAX_INPUT + 1)).is_none());
        assert!(vocab
            .segment_exact(&"iloveyou".repeat(EXACT_MAX_WORDS / 3 + 1))
            .is_none());
    }
    #[test]
    fn exact_segmentation_protects_complete_manual_and_metadata_words_without_io() {
        let vocab = frozen_vocabulary();
        let empty = lexicon::Lexicon::empty();
        let manual =
            lexicon::ManualWords::validate(vec![("IamRobot".into(), "explicit code".into())])
                .unwrap();
        assert!(segment_exact(&vocab, "iamrobot", &empty, &manual).is_none());
        let no_manual = lexicon::ManualWords::default();
        assert!(segment_exact(&vocab, "iamrobot", &empty, &no_manual).is_some());
        let mut bytes = lexicon::MAGIC.to_vec();
        for number in [1u32, 0, 0, 8, 0, 8, 8, 0, 8, 0, 8, 0, 1] {
            bytes.extend_from_slice(&number.to_le_bytes());
        }
        bytes.extend_from_slice(b"iamrobot");
        let metadata = lexicon::Lexicon::from_bytes(bytes).unwrap();
        assert!(segment_exact(&vocab, "iamrobot", &metadata, &no_manual).is_none());
    }
    #[test]
    fn exact_segmentation_is_separate_from_visible_completion_limits() {
        let vocab = frozen_vocabulary();
        let input = "iamrobotandilikeapples";
        assert!(complete(&vocab, input, 0, true).is_empty());
        assert_eq!(
            vocab.segment_exact(input).as_deref(),
            Some("I am robot and I like apples")
        );
    }
}
