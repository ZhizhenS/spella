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

The independent compiler is `tools/english-assist/build-lexicon.py`; the guarded
build entry is `tools/english-assist/build-english-lexicon.ps1`. Python 3.11+ is
required, with no third-party Python packages. Set `ENGLISH_ASSIST_PYTHON` if needed.

```powershell
.\tools\english-assist\build-english-lexicon.ps1 -Test
.\tools\english-assist\build-english-lexicon.ps1 -VerifyOnly
```

The metadata binary and its JSON manifest are placed beside the compiled English
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

Preserve all attributions and the CC BY-SA 3.0 license for the combined metadata
data file. This does not change the separate MIT code license.
