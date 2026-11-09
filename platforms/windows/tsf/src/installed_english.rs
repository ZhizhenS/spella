//! Worker-only lookup of the immutable English resources beside this TIP DLL.
use std::path::{Path, PathBuf};

/// An absolute explicit override is authoritative, including when unavailable.
/// Never consult the host executable, current directory or other version trees.
pub(super) fn candidate(
    override_root: Option<&Path>,
    module_dir: Option<&Path>,
) -> Option<PathBuf> {
    if override_root.is_some_and(Path::is_absolute) {
        return None;
    }
    let directory = module_dir.filter(|path| path.is_absolute())?;
    let directory = if directory
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("x86"))
    {
        directory.parent()?
    } else {
        directory
    };
    Some(directory.join("dictionaries"))
}

/// Validate both package components before selecting this root. Unlike the
/// legacy/user-root path, a package cannot silently omit its metadata sidecar.
/// Errors are fixed classes suitable for diagnostics, never file/input content.
pub(super) fn vocabulary(root: &Path) -> Result<PathBuf, &'static str> {
    let catalog = retype_resources::Catalog::builtin().map_err(|_| "catalog_invalid")?;
    let english = catalog
        .components
        .iter()
        .find(|component| component.id == "english")
        .ok_or("catalog_invalid")?;
    validate(root, english)
}

fn validate(root: &Path, english: &retype_resources::Component) -> Result<PathBuf, &'static str> {
    if !english.present(root) {
        return Err("vocabulary_missing_or_wrong_size");
    }
    let path = english.path(root);
    if retype_resources::checksum(&path).map_err(|_| "vocabulary_unreadable")? != english.sha256 {
        return Err("vocabulary_hash_mismatch");
    }
    retype_english::Vocabulary::open(&path).map_err(|_| "vocabulary_invalid")?;
    let sidecar =
        retype_english::lexicon::Lexicon::open(&root.join(retype_english::lexicon::FILE_NAME))
            .map_err(|_| "metadata_missing_or_invalid")?;
    if sidecar.count() == 0 || sidecar.pinyin_count() == 0 {
        return Err("metadata_empty");
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "english-assist-package-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn lexicon() -> Vec<u8> {
        let mut arena = Vec::new();
        let mut words = Vec::new();
        for text in ["hello", "int.", "你好", ""] {
            words.extend_from_slice(&(arena.len() as u32).to_le_bytes());
            words.extend_from_slice(&(text.len() as u32).to_le_bytes());
            arena.extend_from_slice(text.as_bytes());
        }
        words.extend_from_slice(&1u32.to_le_bytes());
        let mut pinyin = Vec::new();
        pinyin.extend_from_slice(&(arena.len() as u32).to_le_bytes());
        pinyin.extend_from_slice(&5u32.to_le_bytes());
        pinyin.extend_from_slice(&0u32.to_le_bytes());
        pinyin.extend_from_slice(&1u32.to_le_bytes());
        arena.extend_from_slice(b"nihao");
        let mut result = retype_english::lexicon::MAGIC.to_vec();
        for value in [1u32, 1, 1, arena.len() as u32] {
            result.extend_from_slice(&value.to_le_bytes());
        }
        result.extend(words);
        result.extend(pinyin);
        result.extend_from_slice(&0u32.to_le_bytes());
        result.extend(arena);
        result
    }
    fn populated(root: &Path) -> retype_resources::Component {
        let staging = root.join("frequency.tmp");
        retype_english::compile(
            &b"hello 100\nhelp 90\n"[..],
            &mut std::fs::File::create(&staging).unwrap(),
        )
        .unwrap();
        let english = retype_resources::Component {
            id: "english".into(),
            format: "RTENGL01".into(),
            bytes: std::fs::metadata(&staging).unwrap().len(),
            sha256: retype_resources::checksum(&staging).unwrap(),
            compressed_bytes: 1,
            compressed_sha256: "0".repeat(64),
            release: None,
        };
        std::fs::rename(staging, english.path(root)).unwrap();
        std::fs::write(root.join(retype_english::lexicon::FILE_NAME), lexicon()).unwrap();
        english
    }

    #[test]
    fn package_paths_use_own_x64_or_x86_module_only() {
        let fixture = Fixture::new();
        let root = fixture.0.join("versions/0.2.0-current");
        let expected = root.join("dictionaries");
        assert_eq!(candidate(None, Some(&root)), Some(expected.clone()));
        assert_eq!(
            candidate(None, Some(&root.join("x86"))),
            Some(expected.clone())
        );
        assert_eq!(candidate(None, Some(&root.join("X86"))), Some(expected));
        assert_eq!(candidate(None, Some(Path::new("relative"))), None);
        assert_eq!(candidate(None, None), None);
    }
    #[test]
    fn explicit_override_suppresses_even_a_valid_package() {
        let fixture = Fixture::new();
        let package = fixture.0.join("dictionaries");
        std::fs::create_dir(&package).unwrap();
        let english = populated(&package);
        assert!(validate(&package, &english).is_ok());
        let unavailable_override = fixture.0.join("not-created");
        assert_eq!(
            candidate(Some(&unavailable_override), Some(&fixture.0)),
            None
        );
        assert_eq!(
            candidate(Some(Path::new("relative")), Some(&fixture.0)),
            Some(package)
        );
    }
    #[test]
    fn complete_package_has_real_annotation_without_global_publication() {
        let fixture = Fixture::new();
        let english = populated(&fixture.0);
        assert_eq!(
            validate(&fixture.0, &english).unwrap(),
            english.path(&fixture.0)
        );
        let sidecar = retype_english::lexicon::Lexicon::open(
            &fixture.0.join(retype_english::lexicon::FILE_NAME),
        )
        .unwrap();
        assert_eq!(sidecar.lookup("hello").unwrap().definition, "你好");
        assert_eq!(sidecar.pinyin_count(), 1);
    }
    #[test]
    fn incomplete_or_corrupt_package_remains_eligible_for_user_root_fallback() {
        let fixture = Fixture::new();
        let english = populated(&fixture.0);
        let metadata = fixture.0.join(retype_english::lexicon::FILE_NAME);
        std::fs::remove_file(&metadata).unwrap();
        assert_eq!(
            validate(&fixture.0, &english),
            Err("metadata_missing_or_invalid")
        );
        std::fs::write(&metadata, b"invalid secret-like fixture text").unwrap();
        assert_eq!(
            validate(&fixture.0, &english),
            Err("metadata_missing_or_invalid")
        );
        let mut empty = retype_english::lexicon::MAGIC.to_vec();
        empty.resize(24, 0);
        std::fs::write(&metadata, empty).unwrap();
        assert_eq!(validate(&fixture.0, &english), Err("metadata_empty"));
    }
    #[test]
    fn catalog_size_and_hash_are_checked_before_accepting_frequency() {
        let fixture = Fixture::new();
        let english = populated(&fixture.0);
        let frequency = english.path(&fixture.0);
        let original = std::fs::read(&frequency).unwrap();
        std::fs::write(&frequency, b"short").unwrap();
        assert_eq!(
            validate(&fixture.0, &english),
            Err("vocabulary_missing_or_wrong_size")
        );
        let mut changed = original;
        changed[0] ^= 1;
        std::fs::write(&frequency, changed).unwrap();
        assert_eq!(
            validate(&fixture.0, &english),
            Err("vocabulary_hash_mismatch")
        );
    }
    #[test]
    fn invalid_package_does_not_search_neighbor_versions() {
        let fixture = Fixture::new();
        let old = fixture.0.join("versions/old/dictionaries");
        let current = fixture.0.join("versions/current/dictionaries");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(&current).unwrap();
        let english = populated(&old);
        assert_eq!(
            validate(&current, &english),
            Err("vocabulary_missing_or_wrong_size")
        );
    }
}
