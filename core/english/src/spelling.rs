//! Spelling suggestions for people who know a word by ear but not by letter.
//!
//! A suggestion is a vocabulary word within two edits of the typed spelling or
//! with the same sound key (`fone` -> phone, `nite` -> night). Each one is
//! ranked by `ln(count) - cost`, where the cost is low for the mistakes people
//! actually make: a missed or extra doubled letter, two swapped letters, a
//! wrong vowel in a long word, a spelling that only sounds right.
//!
//! Everything runs on the background worker over the mapped vocabulary; no
//! index is built, so an application that loads the input method pays nothing
//! until a misspelling is typed.
use crate::Vocabulary;

/// Rarer words are not offered as corrections; a learner means a common word.
const MIN_COUNT: u64 = 150_000;
/// A doubled or swapped letter or the same sound is enough for a rarer word.
const MIN_COUNT_LIKELY: u64 = 20_000;
/// Sound-alike words may differ in length by this much (`becos`/`because`).
const MAX_SOUND_GAP: usize = 3;

const SWAPPED: f32 = 1.5;
const DOUBLED: f32 = 1.5;
const ONE_EDIT_SOUND: f32 = 2.0;
const TWO_EDITS_SOUND: f32 = 2.5;
const SOUND_ONLY: f32 = 3.5;
const ONE_EDIT: f32 = 4.5;
/// The right letters in the wrong order (`tounge`).
const REORDERED: f32 = 3.0;
/// Two letters left out (`probly`); adding two is rarer (`publically`).
const LETTERS_MISSING: f32 = 5.5;
const LETTERS_ADDED: f32 = 7.0;
const TWO_EDITS: f32 = 8.0;
const NAME: f32 = 1.5;
/// The typed ending was thrown away (`finaly` -> final, `happend` -> happen).
const DROPPED_TAIL: f32 = 1.5;
const OTHER_FIRST_LETTER: f32 = 2.0;
const OTHER_FIRST_LETTER_SOUND: f32 = 0.5;

fn is_vowel(c: u8) -> bool {
    matches!(c, b'a' | b'e' | b'i' | b'o' | b'u')
}

/// Sound class of the first vowel group, which is usually spelled right even
/// when the rest is not: `fone` is phone, never fine.
fn vowel_class(group: &[u8]) -> u8 {
    match group {
        [b'e', b'i', ..] | [b'y', ..] => b'i',
        [b'o', b'u', ..] | [b'o', b'o', ..] | [b'e', b'w', ..] => b'u',
        [first, ..] => *first,
        [] => 0,
    }
}

/// Consonant skeleton with every vowel group collapsed to `A`. Returns the
/// class of the first vowel group. `gh_as_f` selects the reading of `gh`
/// after a vowel (lau**gh**, enou**gh**) instead of the silent one (ni**gh**t).
fn sound_key(word: &[u8], gh_as_f: bool, out: &mut Vec<u8>) -> u8 {
    out.clear();
    let n = word.len();
    let at = |i: usize| word.get(i).copied().unwrap_or(0);
    let mut first_vowel = 0;
    let mut i = match word {
        [b'k' | b'g' | b'p', b'n', ..] | [b'w', b'r', ..] | [b'p', b's', ..] => 1,
        [b'r', b'h', ..] => {
            out.push(b'r');
            2
        }
        _ => 0,
    };
    while i < n {
        let c = word[i];
        let prev = if i > 0 { word[i - 1] } else { 0 };
        let next = at(i + 1);
        if c == b'\'' {
            i += 1;
            continue;
        }
        let vowel = is_vowel(c) || (c == b'y' && i > 0);
        if vowel {
            let start = i;
            while i < n
                && (is_vowel(word[i])
                    || word[i] == b'y'
                    // `ow`, `aw`, `ew` and `oh` spell one vowel sound.
                    || (matches!(word[i], b'w' | b'h') && !is_vowel(at(i + 1))))
            {
                i += 1;
            }
            let group = &word[start..i];
            let seen = out.contains(&b'A');
            // Silent endings: mak**e**, ton**gue**, uniq**ue**.
            let silent = i == n
                && seen
                && (group == b"e" || (group == b"ue" && matches!(out.last(), Some(b'g' | b'k'))));
            if !silent {
                if first_vowel == 0 {
                    first_vowel = vowel_class(group);
                }
                if out.last() != Some(&b'A') {
                    out.push(b'A');
                }
            }
            continue;
        }
        // A doubled consonant sounds like one; the rules read the second.
        if c == next {
            i += 1;
            continue;
        }
        let mut step = 1;
        match c {
            b'b' if prev == b'm' && i + 1 == n => {}
            b'c' if next == b'h' => {
                out.push(if prev == b's' { b'k' } else { b'x' });
                step = 2;
            }
            b'c' if next == b'k' => {
                out.push(b'k');
                step = 2;
            }
            b'c' if matches!(next, b'e' | b'i' | b'y') => out.push(b's'),
            b'c' | b'k' => out.push(b'k'),
            // The d of e**dg**e and knowle**dg**e is not heard.
            b'd' if next == b'g' && matches!(at(i + 2), b'e' | b'i' | b'y') => {}
            b'g' if next == b'h' => {
                if is_vowel(prev) || matches!(prev, b'y' | b'w') {
                    if gh_as_f {
                        out.push(b'f');
                    }
                } else {
                    out.push(b'g');
                }
                step = 2;
            }
            b'g' if next == b'n' && i + 2 >= n => {}
            b'h' if is_vowel(prev) && !is_vowel(next) => {}
            b'l' if i >= 2 && &word[i - 2..i] == b"ou" && next == b'd' => {}
            b'l' if prev == b'a' && matches!(next, b'k' | b'f' | b'm') => {}
            b'n' if prev == b'm' && i + 1 == n => {}
            b'p' if next == b'h' => {
                out.push(b'f');
                step = 2;
            }
            b'q' => {
                out.extend_from_slice(b"kw");
                if next == b'u' {
                    step = 2;
                }
            }
            b's' if next == b'h' => {
                out.push(b'X');
                step = 2;
            }
            b's' if next == b'c' && matches!(at(i + 2), b'e' | b'i' | b'y') => {
                out.push(b's');
                step = 2;
            }
            b's' if next == b'i' && at(i + 2) == b'o' => out.push(b'X'),
            b't' if next == b'h' => {
                out.push(b'0');
                step = 2;
            }
            b't' if next == b'i' && matches!(at(i + 2), b'o' | b'a') => out.push(b'X'),
            b't' if next == b'c' && at(i + 2) == b'h' => {}
            b'w' if next == b'h' => {
                out.push(b'w');
                step = 2;
            }
            b'x' => out.extend_from_slice(b"ks"),
            b'z' => out.push(b's'),
            other => out.push(other),
        }
        i += step;
    }
    first_vowel
}

/// Optimal string alignment distance (adjacent swaps count once), or
/// `max + 1` as soon as the distance must exceed `max`.
fn distance(a: &[u8], b: &[u8], max: usize) -> usize {
    let (n, m) = (a.len(), b.len());
    if n.abs_diff(m) > max || n > 64 || m > 64 {
        return max + 1;
    }
    let mut rows = [[0u8; 65]; 3];
    let (mut older, mut previous, mut current) = (0, 1, 2);
    for (j, cell) in rows[previous].iter_mut().enumerate().take(m + 1) {
        *cell = j as u8;
    }
    for i in 1..=n {
        rows[current][0] = i as u8;
        let mut best = i as u8;
        for j in 1..=m {
            let mut value = (rows[previous][j - 1] + u8::from(a[i - 1] != b[j - 1]))
                .min(rows[previous][j] + 1)
                .min(rows[current][j - 1] + 1);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                value = value.min(rows[older][j - 2] + 1);
            }
            rows[current][j] = value;
            best = best.min(value);
        }
        if usize::from(best) > max {
            return max + 1;
        }
        (older, previous, current) = (previous, current, older);
    }
    usize::from(rows[previous][m]).min(max + 1)
}

/// Exactly one pair of adjacent letters is swapped (`teh`/`the`).
fn swapped(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).position(|(x, y)| x != y).is_some_and(|i| {
            i + 1 < a.len() && a[i] == b[i + 1] && a[i + 1] == b[i] && a[i + 2..] == b[i + 2..]
        })
}

/// The only difference is one letter doubled or undoubled (`realy`/`really`).
fn doubled(a: &[u8], b: &[u8]) -> bool {
    let (long, short) = if a.len() > b.len() { (a, b) } else { (b, a) };
    if long.len() != short.len() + 1 {
        return false;
    }
    let i = long
        .iter()
        .zip(short)
        .position(|(x, y)| x != y)
        .unwrap_or(short.len());
    i > 0 && long[i] == long[i - 1] && long[i + 1..] == short[i..]
}

fn reordered(a: &[u8], b: &[u8]) -> bool {
    let mut counts = [0i8; 256];
    for c in a {
        counts[usize::from(*c)] += 1;
    }
    for c in b {
        counts[usize::from(*c)] -= 1;
    }
    a.len() == b.len() && counts.iter().all(|count| *count == 0)
}

/// Every letter of `short` appears in `long` in the same order.
fn within(short: &[u8], long: &[u8]) -> bool {
    let mut rest = long.iter();
    short.iter().all(|c| rest.any(|other| other == c))
}

/// First letters a sound-speller confuses, as a bit set over `a..=z`.
fn first_letters(input: &[u8]) -> u32 {
    let bit = |c: u8| 1u32 << (c - b'a');
    let first = input[0];
    let mut set = bit(first);
    for other in match first {
        b'c' => &b"ksq"[..],
        b'k' => b"cq",
        b'q' => b"ck",
        b'f' => b"p",
        b'n' => b"kgp",
        b'r' => b"w",
        b's' => b"cpz",
        b'z' => b"sx",
        b'j' => b"g",
        b'g' => b"j",
        b'a' | b'e' | b'i' | b'o' | b'u' => b"h",
        _ => b"",
    } {
        set |= bit(*other);
    }
    // The first two letters may be swapped (`hte`).
    if let Some(second) = input.get(1).filter(|c| c.is_ascii_lowercase()) {
        set |= bit(*second);
    }
    set
}

/// Vocabulary ids with `ln(count) - cost`, best first. `input` is lowercase.
pub(super) fn suggest(vocab: &Vocabulary, input: &str, limit: usize) -> Vec<(usize, f32)> {
    let typed = input.as_bytes();
    let n = typed.len();
    if limit == 0 || !(3..=64).contains(&n) || !typed[0].is_ascii_lowercase() {
        return Vec::new();
    }
    let overlay = crate::ranking::current();
    let mut keys = [Vec::new(), Vec::new()];
    let typed_vowel = sound_key(typed, false, &mut keys[0]);
    let typed_gh = typed.windows(2).any(|pair| pair == b"gh");
    if typed_gh {
        sound_key(typed, true, &mut keys[1]);
    }
    let mut key = Vec::new();
    let mut hits = Vec::new();
    let letters = first_letters(typed);
    for letter in (b'a'..=b'z').filter(|c| letters & (1 << (c - b'a')) != 0) {
        let lo = vocab.lower_bound(std::str::from_utf8(&[letter]).unwrap_or("a"));
        let hi = vocab.lower_bound(std::str::from_utf8(&[letter + 1]).unwrap_or("{"));
        for id in lo..hi {
            let word = vocab.word(id).as_bytes();
            let m = word.len();
            let count = vocab.frequency(id);
            if m < 3
                || m + MAX_SOUND_GAP < n
                || m > n + MAX_SOUND_GAP
                || count < MIN_COUNT_LIKELY
                || word == typed
                || word.contains(&b'\'')
            {
                continue;
            }
            let edits = distance(typed, word, 2);
            let vowel = sound_key(word, false, &mut key);
            let mut sound = key == keys[0] || (typed_gh && key == keys[1]);
            if !sound && word.windows(2).any(|pair| pair == b"gh") {
                sound_key(word, true, &mut key);
                sound = key == keys[0] || (typed_gh && key == keys[1]);
            }
            let sound = sound && vowel == typed_vowel;
            let same_start = word[0] == typed[0];
            let swap = edits == 1 && swapped(typed, word);
            let double = edits == 1 && doubled(typed, word);
            if !(same_start || sound || swap)
                || (count < MIN_COUNT && !(sound || swap || double))
                // Three letters carry little evidence on their own.
                || (n == 3 && m != 3 && !sound)
            {
                continue;
            }
            let mut cost = match (edits, sound) {
                (1, _) if swap => SWAPPED,
                (1, _) if double => DOUBLED,
                (1, true) => ONE_EDIT_SOUND,
                (1, false) => ONE_EDIT,
                (2, true) => TWO_EDITS_SOUND,
                (2, false) if n < 5 || m < 4 => continue,
                (2, false) if reordered(typed, word) => REORDERED,
                (2, false) if within(typed, word) => LETTERS_MISSING,
                (2, false) if within(word, typed) => LETTERS_ADDED,
                (2, false) => TWO_EDITS,
                (_, true) if edits > 2 => SOUND_ONLY,
                _ => continue,
            };
            if m < n && typed.starts_with(word) && !double {
                cost += DROPPED_TAIL;
            }
            if !same_start {
                cost += if sound {
                    OTHER_FIRST_LETTER_SOUND
                } else {
                    OTHER_FIRST_LETTER
                };
            }
            let text = vocab.word(id);
            let row = overlay.and_then(|overlay| overlay.get(text));
            if row.is_some_and(|row| row.reason.demoted()) {
                continue;
            }
            // A misspelling is rarely aimed at a personal or place name.
            if row.is_some_and(|row| row.reason == crate::ranking::Reason::Name) {
                cost += NAME;
            }
            hits.push((id, (vocab.prior(text).max(1) as f64).ln() as f32 - cost));
        }
    }
    hits.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    hits.truncate(limit);
    hits
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(word: &str) -> String {
        let mut out = Vec::new();
        sound_key(word.as_bytes(), false, &mut out);
        String::from_utf8(out).unwrap()
    }
    fn key_f(word: &str) -> String {
        let mut out = Vec::new();
        sound_key(word.as_bytes(), true, &mut out);
        String::from_utf8(out).unwrap()
    }
    #[test]
    fn sound_keys_match_spellings_that_only_sound_right() {
        for (typed, word) in [
            ("fone", "phone"),
            ("nite", "night"),
            ("skool", "school"),
            ("shud", "should"),
            ("wich", "which"),
            ("becos", "because"),
            ("definately", "definitely"),
            ("seperate", "separate"),
            ("calender", "calendar"),
            ("restaraunt", "restaurant"),
            ("neccessary", "necessary"),
            ("tommorow", "tomorrow"),
            ("lisence", "license"),
            ("concious", "conscious"),
            ("rythm", "rhythm"),
            ("writting", "writing"),
            ("peple", "people"),
            ("garantee", "guarantee"),
            ("thru", "through"),
        ] {
            assert_eq!(key(typed), key(word), "{typed} / {word}");
        }
        assert_eq!(key_f("laff"), key_f("laugh"));
        assert_eq!(key_f("enuff"), key_f("enough"));
        // Different sounds stay apart, by skeleton or by first vowel.
        let sound = |word: &str| {
            let mut out = Vec::new();
            let vowel = sound_key(word.as_bytes(), false, &mut out);
            (out, vowel)
        };
        for (a, b) in [
            ("form", "from"),
            ("night", "note"),
            ("then", "than"),
            ("fone", "fine"),
        ] {
            assert_ne!(sound(a), sound(b), "{a} / {b}");
        }
    }
    #[test]
    fn distance_counts_swaps_once_and_stops_early() {
        assert_eq!(distance(b"teh", b"the", 2), 1);
        assert_eq!(distance(b"realy", b"really", 2), 1);
        assert_eq!(distance(b"tounge", b"tongue", 2), 2);
        assert_eq!(distance(b"wensday", b"wednesday", 2), 2);
        assert_eq!(distance(b"cat", b"dogs", 2), 3);
        assert_eq!(distance(b"a", b"abcdef", 2), 3);
        assert!(swapped(b"freind", b"friend") && !swapped(b"form", b"farm"));
        assert!(doubled(b"realy", b"really") && doubled(b"untill", b"until"));
        assert!(!doubled(b"real", b"realy") && !doubled(b"happen", b"happend"));
        assert!(reordered(b"tounge", b"tongue") && !reordered(b"tone", b"tune"));
        assert!(within(b"probly", b"probably") && !within(b"probly", b"problem"));
    }
}
