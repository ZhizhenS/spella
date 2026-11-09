//! Small reviewed modern-name stream, separate from ordinary English meanings.
//! The frozen catalog is intentionally limited; lookup never performs IO/network.
use crate::modern::RankedKeys;
use retype_types::{Candidate, CandidateLanguage, CandidateSource, EnglishAnnotation};
use std::{cmp::Reverse, collections::HashSet, sync::OnceLock};

const DATA: &str = include_str!("../../../data/dict/modern-entities.tsv");
const MAX_ENTITIES: usize = 20_000;
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
    priority: u32,
}
struct Catalog<'a> {
    entries: Vec<Entity<'a>>,
    index: RankedKeys<'a>,
}
fn valid_key(key: &str) -> bool {
    (2..=64).contains(&key.len()) && key.bytes().all(|byte| byte.is_ascii_lowercase())
}
fn parse(data: &str) -> Option<Catalog<'_>> {
    if data.len() > 16 * 1024 * 1024 {
        return None;
    }
    let mut lines = data.lines();
    if !lines.next()?.starts_with("# EAMODEN2\t") {
        return None;
    }
    let mut entries = Vec::new();
    let mut previous = None;
    let mut identities = HashSet::new();
    let mut aliases_count = 0;
    for line in lines {
        let fields: Vec<_> = line.split('\t').collect();
        if fields.len() != 8 {
            return None;
        }
        let [text, aliases, kind, definition, id, revision, official, priority] = fields.as_slice()
        else {
            return None;
        };
        let priority = priority.parse::<u32>().ok()?;
        let aliases_len = aliases.split(',').count();
        aliases_count += aliases_len;
        if text.is_empty()
            || text.len() > 96
            || !text.is_ascii()
            || text.chars().any(char::is_control)
            || text.trim() != *text
            || kind.is_empty()
            || kind.chars().count() > 12
            || kind.chars().any(char::is_control)
            || definition.is_empty()
            || definition.chars().count() > 192
            || definition.chars().any(char::is_control)
            || !id.starts_with('Q')
            || id.len() < 2
            || !id[1..].bytes().all(|byte| byte.is_ascii_digit())
            || revision
                .parse::<u64>()
                .ok()
                .is_none_or(|number| number == 0)
            || (!official.is_empty() && !official.starts_with("https://"))
            || official.len() > 1024
            || official.chars().any(char::is_control)
            || aliases.split(',').any(|alias| !valid_key(alias))
            || aliases_len > 32
            || aliases_count > 200_000
            || !identities.insert(*id)
        {
            return None;
        }
        let first = aliases.split(',').next()?;
        let order = (first, Reverse(priority), *id);
        if previous.is_some_and(|old| order <= old) || entries.len() >= MAX_ENTITIES {
            return None;
        }
        previous = Some(order);
        entries.push(Entity {
            text,
            aliases,
            kind,
            definition,
            id,
            revision,
            official,
            priority,
        });
    }
    let index = RankedKeys::new(entries.iter().enumerate().flat_map(|(id, entity)| {
        entity
            .aliases
            .split(',')
            .map(move |alias| (alias, id, entity.priority))
    }))
    .ok()?;
    (!entries.is_empty()).then_some(Catalog { entries, index })
}
static CATALOG: OnceLock<Option<Catalog<'static>>> = OnceLock::new();
pub(super) fn initialize() -> Result<(), String> {
    CATALOG
        .get_or_init(|| parse(DATA))
        .as_ref()
        .map(|_| ())
        .ok_or_else(|| "Invalid modern entity catalog".into())
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
    #[cfg(any(test, feature = "test-data"))]
    let _ = initialize();
    let Some(catalog) = CATALOG.get().and_then(Option::as_ref) else {
        return Vec::new();
    };
    catalog.complete(&key, input.len(), limit)
}
impl Catalog<'_> {
    fn complete(&self, key: &str, consumed: usize, limit: usize) -> Vec<Candidate> {
        self.index
            .lookup(key, limit.min(MAX_RESULTS))
            .into_iter()
            .map(|(ordinal, _)| {
                let entity = self.entries[ordinal];
                let mut candidate = Candidate::new(entity.text, CandidateSource::Local);
                candidate.language = CandidateLanguage::English;
                candidate.consumed = consumed;
                candidate.comment = format!("{} · {}", entity.kind, entity.definition);
                candidate.annotation = Some(EnglishAnnotation {
                    part_of_speech: "专名".to_owned(),
                    definition: entity.definition.to_owned(),
                    entity_kind: entity.kind.to_owned(),
                    entity_source: format!(
                        "Wikidata {}@{}; https://www.wikidata.org/w/index.php?title={}&oldid={}{}",
                        entity.id,
                        entity.revision,
                        entity.id,
                        entity.revision,
                        if entity.official.is_empty() {
                            String::new()
                        } else {
                            format!("; {}", entity.official)
                        }
                    ),
                    ..Default::default()
                });
                // Producer commonness is local to this stream, not a language-model probability.
                candidate.score = entity.priority as f32;
                candidate
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reviewed_catalog_has_identity_revision_and_canonical_case() {
        assert!(parse(DATA).is_some_and(|catalog| catalog.entries.len() >= 51));
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

    #[test]
    fn repeated_aliases_preserve_distinct_entity_identities_and_priority() {
        let source = "# EAMODEN2\ttest\nAcronym One\timo\t组织\t组织甲\tQ1\t1\thttps://example.org/one\t900\nAcronym Two\timo\t组织\t组织乙\tQ2\t1\thttps://example.org/two\t800\n";
        let catalog = parse(source).unwrap();
        let hits = catalog.complete("imo", 3, 24);
        assert_eq!(hits.len(), 2);
        assert!(hits[0]
            .annotation
            .as_ref()
            .unwrap()
            .entity_source
            .contains("Q1@1"));
        assert!(hits[1]
            .annotation
            .as_ref()
            .unwrap()
            .entity_source
            .contains("Q2@1"));
        assert_eq!(catalog.complete("imo", 3, 1).len(), 1);
        assert!(catalog.complete("imo", 3, 0).is_empty());
        assert!(parse(&source.replace("\tQ2\t", "\tQ1\t")).is_none());
        assert!(parse(&source.replace("\t800", "\t1000")).is_none());
    }
}
