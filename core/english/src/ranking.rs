//! Compiled-in candidate ranking overlay (EARANK01).
//!
//! The pinned vocabulary image keeps its Google Books counts. This reviewed
//! table replaces individual counts at lookup time: spoken contractions,
//! everyday/modern words, archaic words and personal/place names. It is built
//! by `tools/english-assist/build-english-ranking.py`; lookup never does IO.
use std::{collections::HashMap, sync::OnceLock};

const DATA: &str = include_str!("../../../data/dict/english-ranking.tsv");
const MAX_ROWS: usize = 20_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Reason {
    Contraction,
    Everyday,
    Modern,
    Typo,
    Archaic,
    /// Personal or place name: demoted and capitalized.
    Name,
    /// Written capitalized (Monday, English): casing only, count unchanged.
    Proper,
}
impl Reason {
    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "contraction" => Self::Contraction,
            "everyday" => Self::Everyday,
            "modern" => Self::Modern,
            "typo" => Self::Typo,
            "archaic" => Self::Archaic,
            "name" => Self::Name,
            "proper" => Self::Proper,
            _ => return None,
        })
    }
    /// Rows that may add a word the frequency tree would not surface.
    pub(super) fn boosts(self) -> bool {
        matches!(self, Self::Contraction | Self::Everyday | Self::Modern)
    }
    /// Words that should never be pulled forward just because they were typed.
    pub(super) fn demoted(self) -> bool {
        matches!(self, Self::Typo | Self::Archaic)
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Row<'a> {
    pub word: &'a str,
    pub frequency: u64,
    pub display: &'a str,
    pub reason: Reason,
}

pub(super) struct Overlay<'a> {
    rows: Vec<Row<'a>>,
    /// Apostrophe-free typing key ("dont") to its contraction row ("don't").
    contractions: HashMap<String, usize>,
}

fn valid_word(word: &str) -> bool {
    (1..=64).contains(&word.len())
        && word.as_bytes()[0].is_ascii_lowercase()
        && word.bytes().all(|b| b.is_ascii_lowercase() || b == b'\'')
}

fn parse(data: &str) -> Option<Overlay<'_>> {
    let mut lines = data.lines();
    if !lines.next()?.starts_with("# EARANK01\t") {
        return None;
    }
    let mut rows: Vec<Row<'_>> = Vec::new();
    let mut contractions = HashMap::new();
    for line in lines {
        let [word, frequency, display, reason] = line.split('\t').collect::<Vec<_>>()[..] else {
            return None;
        };
        let frequency = frequency.parse::<u64>().ok().filter(|&count| count > 0)?;
        let reason = Reason::parse(reason)?;
        if !valid_word(word)
            || rows.len() >= MAX_ROWS
            || rows.last().is_some_and(|previous| previous.word >= word)
            || !(display.is_empty() || display.is_ascii() && display.eq_ignore_ascii_case(word))
        {
            return None;
        }
        if reason == Reason::Contraction {
            contractions.insert(word.replace('\'', ""), rows.len());
        }
        rows.push(Row {
            word,
            frequency,
            display,
            reason,
        });
    }
    Some(Overlay { rows, contractions })
}

impl<'a> Overlay<'a> {
    pub(super) fn get(&self, word: &str) -> Option<Row<'a>> {
        self.rows
            .binary_search_by(|row| row.word.cmp(word))
            .ok()
            .map(|index| self.rows[index])
    }
    /// Rows whose spelling starts with `key` (already lowercase).
    pub(super) fn prefix(&self, key: &str) -> &[Row<'a>] {
        let lo = self.rows.partition_point(|row| row.word < key);
        let hi = lo + self.rows[lo..].partition_point(|row| row.word.starts_with(key));
        &self.rows[lo..hi]
    }
    /// The contraction typed without its apostrophe: "dont" -> "don't".
    pub(super) fn contraction(&self, key: &str) -> Option<Row<'a>> {
        self.contractions.get(key).map(|&index| self.rows[index])
    }
}

static OVERLAY: OnceLock<Option<Overlay<'static>>> = OnceLock::new();

/// Resource-worker validation, never on the first production keystroke.
pub(super) fn initialize() -> Result<(), String> {
    OVERLAY
        .get_or_init(|| parse(DATA))
        .as_ref()
        .map(|_| ())
        .ok_or_else(|| "Invalid English ranking overlay".into())
}

pub(super) fn current() -> Option<&'static Overlay<'static>> {
    #[cfg(any(test, feature = "test-data"))]
    let _ = initialize();
    OVERLAY.get().and_then(Option::as_ref)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedded_overlay_parses_and_indexes_contractions() {
        let overlay = parse(DATA).expect("valid overlay");
        assert_eq!(overlay.contraction("dont").unwrap().word, "don't");
        assert_eq!(overlay.contraction("im").unwrap().display, "I'm");
        assert_eq!(overlay.get("thy").unwrap().reason, Reason::Archaic);
        assert_eq!(overlay.get("andrew").unwrap().display, "Andrew");
        assert_eq!(overlay.get("monday").unwrap().reason, Reason::Proper);
        assert_eq!(overlay.get("oklahoma").unwrap().display, "Oklahoma");
        assert!(overlay.prefix("don").iter().any(|row| row.word == "don't"));
        assert!(overlay.prefix("zzzz").is_empty());
    }
    #[test]
    fn malformed_rows_fail_closed() {
        for bad in [
            "# other\n",
            "# EARANK01\t\nb\t1\t\tname\na\t1\t\tname\n",
            "# EARANK01\t\nword\t0\t\tname\n",
            "# EARANK01\t\nword\t1\tOther\tname\n",
            "# EARANK01\t\nword\t1\t\tunknown\n",
            "# EARANK01\t\nWord\t1\t\tname\n",
        ] {
            assert!(parse(bad).is_none(), "{bad:?}");
        }
    }
}
