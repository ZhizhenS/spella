//! Bounded manual-word file shared by settings and TSF resource workers.
//! This module performs no IO and never records automatically typed text.
use std::collections::HashSet;

pub const MAX_FILE_BYTES: usize = 1024 * 1024;
pub const MAX_WORDS: usize = 10_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManualWord {
    pub word: String,
    pub comment: String,
}

pub fn validate(words: &[ManualWord]) -> Result<(), String> {
    if words.len() > MAX_WORDS {
        return Err("个人词库最多保存 10,000 个词条".into());
    }
    let mut seen = HashSet::with_capacity(words.len());
    for entry in words {
        let word = entry.word.as_bytes();
        if word.is_empty()
            || word.len() > 64
            || !word[0].is_ascii_alphabetic()
            || !word[word.len() - 1].is_ascii_alphabetic()
            || !word.iter().all(|c| c.is_ascii_alphabetic() || *c == b'\'')
        {
            return Err("单词须为 1–64 个英文字母，可包含单词内部的撇号".into());
        }
        if entry.comment.chars().count() > 256 || entry.comment.chars().any(char::is_control) {
            return Err("释义/备注最多 256 个字符，不能包含换行或控制字符".into());
        }
        if !seen.insert(entry.word.to_ascii_lowercase()) {
            return Err("词库中存在重复单词（不区分大小写）".into());
        }
    }
    Ok(())
}

pub fn decode(bytes: &[u8]) -> Result<Vec<ManualWord>, String> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err("词库文件超过 1 MB".into());
    }
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| "词库 JSON 格式无效")?;
    if value["schema"].as_u64() != Some(1) {
        return Err("词库版本无效，需要 schema=1".into());
    }
    let rows = value["words"].as_array().ok_or("词库缺少 words 列表")?;
    if rows.len() > MAX_WORDS {
        return Err("个人词库最多保存 10,000 个词条".into());
    }
    let mut words = Vec::with_capacity(rows.len());
    for row in rows {
        words.push(ManualWord {
            word: row["word"].as_str().ok_or("词条缺少 word 字段")?.to_owned(),
            comment: row["comment"]
                .as_str()
                .ok_or("词条缺少 comment 字段")?
                .to_owned(),
        });
    }
    validate(&words)?;
    Ok(words)
}

#[allow(dead_code)] // Settings writes this format; TSF workers only decode it.
pub fn encode(words: &[ManualWord]) -> Result<Vec<u8>, String> {
    validate(words)?;
    let rows: Vec<_> = words
        .iter()
        .map(|entry| serde_json::json!({"word":entry.word,"comment":entry.comment}))
        .collect();
    let mut bytes = serde_json::to_vec_pretty(&serde_json::json!({"schema":1,"words":rows}))
        .map_err(|_| "无法编码个人词库")?;
    bytes.push(b'\n');
    if bytes.len() > MAX_FILE_BYTES {
        return Err("词库文件超过 1 MB".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    #[test]
    fn valid_words_round_trip_and_reject_corrupt_or_duplicate_data() {
        let words = vec![ManualWord {
            word: "Codex".into(),
            comment: "n. 项目名称".into(),
        }];
        assert_eq!(decode(&encode(&words).unwrap()).unwrap(), words);
        for word in ["", "python3", "two words", "'hello", "hello'", "路径"] {
            assert!(validate(&[ManualWord {
                word: word.into(),
                comment: String::new()
            }])
            .is_err());
        }
        assert!(validate(&[
            words[0].clone(),
            ManualWord {
                word: "codex".into(),
                comment: String::new()
            }
        ])
        .is_err());
        assert!(decode(br#"{"schema":2,"words":[]}"#).is_err());
        assert!(decode(br#"{"schema":1,"words":[{"word":"hello"}]}"#).is_err());
        assert!(decode(&vec![b' '; MAX_FILE_BYTES + 1]).is_err());
    }
}
