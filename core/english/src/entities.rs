//! Small reviewed modern-name stream, separate from ordinary English meanings.
//! The frozen catalog is intentionally limited; lookup never performs IO/network.
use retype_types::{Candidate, CandidateLanguage, CandidateSource, EnglishAnnotation};
use std::sync::OnceLock;

const DATA: &str = include_str!("../../../data/dict/modern-entities.tsv");
const MAX_ENTITIES: usize = 80;
const MAX_RESULTS: usize = 24;

#[derive(Clone, Copy)]
struct Entity<'a> {
    text: &'a str,
    aliases: &'a str,
    kind: &'a str,
    definition: &'a str,
    id: &'a str,
    revision: &'a str,
    official: &'a str,
}
fn valid_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= 64 && key.bytes().all(|byte| byte.is_ascii_lowercase())
}
fn parse(data: &str) -> Option<Vec<Entity<'_>>> {
    let mut lines = data.lines();
    if !lines.next()?.starts_with("# EAMODEN1\t") {
        return None;
    }
    let mut entries = Vec::new();
    let mut previous = "";
    for line in lines {
        let fields: Vec<_> = line.split('\t').collect();
        if fields.len() != 7 {
            return None;
        }
        let [text, aliases, kind, definition, id, revision, official] = fields.as_slice() else {
            return None;
        };
        if text.is_empty()
            || text.len() > 96
            || !text.is_ascii()
            || text.chars().any(char::is_control)
            || kind.is_empty()
            || kind.chars().count() > 6
            || definition.is_empty()
            || definition.chars().count() > 96
            || !id.starts_with('Q')
            || id.len() < 2
            || !id[1..].bytes().all(|byte| byte.is_ascii_digit())
            || revision
                .parse::<u64>()
                .ok()
                .is_none_or(|number| number == 0)
            || !official.starts_with("https://")
            || official.chars().any(char::is_control)
            || aliases.split(',').any(|alias| !valid_key(alias))
        {
            return None;
        }
        let first = aliases.split(',').next()?;
        if first <= previous || entries.len() >= MAX_ENTITIES {
            return None;
        }
        previous = first;
        entries.push(Entity {
            text,
            aliases,
            kind,
            definition,
            id,
            revision,
            official,
        });
    }
    (!entries.is_empty()).then_some(entries)
}
fn catalog() -> &'static [Entity<'static>] {
    static CATALOG: OnceLock<Vec<Entity<'static>>> = OnceLock::new();
    CATALOG.get_or_init(|| parse(DATA).unwrap_or_default())
}

/// Return canonical brand spelling; callers merge this independently ranked
/// stream without discarding an ordinary dictionary meaning of the same text.
pub(super) fn complete(input: &str, limit: usize) -> Vec<Candidate> {
    if limit == 0
        || input.len() < 2
        || input.len() > 64
        || !input.bytes().all(|byte| byte.is_ascii_alphabetic())
    {
        return Vec::new();
    }
    let key = input.to_ascii_lowercase();
    let mut found: Vec<_> = catalog()
        .iter()
        .enumerate()
        .filter_map(|(ordinal, entity)| {
            let matches = entity
                .aliases
                .split(',')
                .any(|alias| alias.starts_with(&key));
            let exact = entity.aliases.split(',').any(|alias| alias == key);
            matches.then_some((!exact, ordinal, entity))
        })
        .collect();
    found.sort_by_key(|(not_exact, ordinal, _)| (*not_exact, *ordinal));
    found
        .into_iter()
        .take(limit.min(MAX_RESULTS))
        .map(|(_, ordinal, entity)| {
            let mut candidate = Candidate::new(entity.text, CandidateSource::Local);
            candidate.language = CandidateLanguage::English;
            candidate.consumed = input.len();
            candidate.comment = format!("{} · {}", entity.kind, entity.definition);
            candidate.annotation = Some(EnglishAnnotation {
                part_of_speech: "专名".to_owned(),
                definition: entity.definition.to_owned(),
                entity_kind: entity.kind.to_owned(),
                entity_source: format!(
                    "Wikidata {}@{}; https://www.wikidata.org/w/index.php?title={}&oldid={}; {}",
                    entity.id, entity.revision, entity.id, entity.revision, entity.official
                ),
                ..Default::default()
            });
            // A stable catalog ordinal, never an English-frequency probability.
            candidate.score = -(ordinal as f32);
            candidate
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reviewed_catalog_has_identity_revision_and_canonical_case() {
        assert_eq!(parse(DATA).map(|entries| entries.len()), Some(51));
        for (query, text, id) in [
            ("anthropic", "Anthropic", "Q116758847"),
            ("OPENAI", "OpenAI", "Q21708200"),
            ("deepseek", "DeepSeek", "Q131577453"),
            ("codex", "Codex", "Q138940795"),
            ("vscode", "Visual Studio Code", "Q19841877"),
        ] {
            let hits = complete(query, 1);
            assert_eq!(hits.len(), 1);
            assert_eq!(hits[0].text, text);
            assert_eq!(hits[0].consumed, query.len());
            let note = hits[0].annotation.as_ref().expect("entity annotation");
            assert!(!note.pinyin_match && !note.entity_kind.is_empty());
            assert!(note.entity_source.contains(id) && note.entity_source.contains("oldid="));
        }
    }
    #[test]
    fn bounded_entity_stream_keeps_word_meaning_merge_to_the_caller() {
        assert!(complete("anthropic", 0).is_empty());
        for query in ["a", "", "anthropic ", "qzxzzqzx", "node.js"] {
            assert!(complete(query, 5).is_empty());
        }
        assert!(complete(&"a".repeat(65), 5).is_empty());
        assert!(complete("co", usize::MAX).len() <= MAX_RESULTS);
        assert_eq!(complete("rust", 1)[0].text, "Rust");
        assert_eq!(complete("node", 1)[0].text, "Node.js");
        // This module generates entity rows only. The lib merge keeps a separate
        // normal adjective/noun candidate rather than replacing it here.
        assert!(complete("anthropic", 5).iter().all(|hit| hit
            .annotation
            .as_ref()
            .is_some_and(|note| !note.entity_kind.is_empty())));
    }
    #[test]
    fn malformed_frozen_rows_fail_closed() {
        assert!(parse("# other\n").is_none());
        assert!(parse(
            "# EAMODEN1\t\nAnthropic\tanthropic\t公司\t描述\tguess\t1\thttps://example.com\n"
        )
        .is_none());
        assert!(parse(
            "# EAMODEN1\t\nAnthropic\tanthropic\t公司\t描述\tQ1\t0\thttps://example.com\n"
        )
        .is_none());
    }
}
