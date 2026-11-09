# Frozen offline English metadata sources

`SOURCES.json` pins the original repositories, commits, Git blob IDs, byte counts,
compressed/content SHA256 digests and full license-file digests. The gzip files
are lossless originals (empty gzip filename, mtime=0, compression level 9). A normal
build reads these committed archives without downloading or recompressing them.

- `ecdict.csv.gz`: ECDICT commit `bc015ed2e24a7abef49fc6dbbb7fe32c1dadaf8b`, MIT.
- `cedict.txt.gz`: the original CC-CEDICT text mirrored at HallelujahIM commit
  `8be294f70a4dc7b24a93f07c41df6c34add07a07`, CC BY-SA **3.0**, with its original
  2017-11-17 header intact. It is not the mirror's derived JSON/SQLite database.
- `LICENSE-ECDICT`, `LICENSE-CC-CEDICT`: complete unmodified license texts.
- `NOTICE-English-Lexicon.txt`: attribution, original header and modification notice.
- `../wanxiang-base/zi.dict.yaml` and `jichu.dict.yaml`: Chinese word/reading
  frequency evidence from Wanxiang Base v18.0.14, CC BY 4.0; the complete upstream
  `LICENSE` is installed as `LICENSE-Wanxiang`. `SOURCE.md` and this directory's
  manifest pin the release asset and selected source-file hashes.

RTENLX02 preserves each Chinese headword, numbered-tone reading, source gloss,
Chinese frequency and source/usage flags. It combines eligible CC-CEDICT English
gloss heads with exact complete Chinese-gloss reverse links from ECDICT. It does
not infer reverse links from arbitrary Chinese substrings. Exact pinyin and
longer-word expansion retain distinct evidence, and Chinese-word quality precedes
global English frequency when ranking these relations.

The independent compiler is `tools/english-assist/build-lexicon.py`; the guarded
build entry is `tools/english-assist/build-english-lexicon.ps1`. Python 3.11+ is
required, with no third-party Python packages. Set `ENGLISH_ASSIST_PYTHON` if needed.

```powershell
.\tools\english-assist\build-english-lexicon.ps1 -Test
.\tools\english-assist\build-english-lexicon.ps1 -VerifyOnly
```

The `english-lexicon-v2.bin` metadata binary and `english-lexicon-v2.json` manifest
are placed beside the compiled English
frequency image in `dist/english-dictionaries/` and then in each installation's
immutable `dictionaries/` directory. The source distribution contains these frozen
archives and the compiler; the installer contains the compiled files, attribution,
source manifest and licenses. This is an offline component and is not part of the
upstream two-component download catalog or resource-bundle protocol.

The reviewed output is pinned separately in `data/dict/english-lexicon.json`.
Normal builds fail on a changed source, license, binary or output manifest.
After intentionally reviewing a compiler/data change, maintainers can regenerate
that expected manifest explicitly (never as an automatic build fallback):

```powershell
& $env:ENGLISH_ASSIST_PYTHON tools/english-assist/prepare-lexicon.py --output dist/english-dictionaries --refresh-manifest
```

Preserve the CC-CEDICT-derived metadata's CC BY-SA 3.0 terms, the independent
ECDICT MIT notice, Wanxiang CC BY 4.0 attribution and the frequency-source notices.
The 2017 CC-CEDICT snapshot is not relabeled as CC BY-SA 4.0, and the original
Wanxiang inputs keep their CC BY 4.0 terms. Code remains independently MIT.

The modern-entity catalog is separate from this lexicon: a limited reviewed seed
of 51 names, with Wikidata CC0 structured IDs/revisions, official-site
references and reviewed Chinese summaries. Approved data is in
`data/dict/modern-entities.json`, generated embedded data in
`data/dict/modern-entities.tsv`, and frozen provenance in
`data/dict/raw/modern-entities/`. `tools/english-assist/modern-entities.py --check`
validates it offline before production compilation; updates require an explicit
proposal/review and runtime regeneration. It is not a complete modern-entity
corpus. Actual count and both file hashes are recorded in installed `build.json`;
the independent validation/runtime code remains MIT.
