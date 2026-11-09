# English Assist — Windows development branch

English Assist is a Windows English input-method project derived from
[Retype at 902e35e](https://github.com/L-Chris/retype/tree/902e35e06ee4f744320cf995c39b1f64693034cf).
It aims to provide local word completion and spelling suggestions, plus explicit
AI grammar correction and sentence completion, through a real English TSF entry.

[简体中文](README.zh-CN.md) · [English input](docs/english-input.md) ·
[P0 validation](docs/english-assist/INPUT_VALIDATION.md)

**Current checkpoint (2026-11-09):** the formal `final-r2` build completed with
336 Rust test passes, 0 failures and 6 ignored results across x64/x86 runs
(including repeated tests), plus 19 Python passes and 446 PowerShell checks.
The r3 package refreshes installer scripts only; native files retain the same
frozen final-r2 bytes. Its install → uninstall → reinstall cycle returned 0 at
each step. The final payload is `0.2.0-20261109024807-741245`; all 24 file hashes,
sizes and build.json matched, with `Uninstalling=0` and no learning-service
login startup entry. Separate x64/x86 runs each passed 30 cleanup and 100 pending
operation policy checks; these are not added to the earlier 446 checks.

A fresh Windows 11 Notepad process loaded the r2 native DLL and displayed Chinese
definitions and parts of speech; numeric selection and paging produced the saved
text `hello apple helped`. This supplements the independent NativeRichEdit host
check and remains relevant to r3's unchanged native bytes; it does not establish
a fresh final-install test in every application. The r3 uninstall removed the
checked product registry, per-user CTF cache and AppData state while preserving
the existing Sogou and US language entries. Eight old file/directory deletion
operations remain queued without alteration; their actual deletion after reboot
has not been verified. Final-installed probes and settings screenshots are tracked
separately in the [validation record](docs/english-assist/INPUT_VALIDATION.md).
No new DeepSeek key has been configured and no external API request has been made.

## Implemented features

- Independent English Assist 英文助手 entry under English (United States).
- Local prefix completion and spelling suggestions with original case preserved.
- Five candidates per page, 1–5 selection and minus/plus paging; digits remain literal without candidates.
- Chinese definitions, part of speech, optional phonetics and pinyin lookup.
- Tab or mouse acceptance without an automatic trailing space.
- Space preserves the original spelling unless a candidate was explicitly selected.
- Enter after explicit candidate navigation confirms the candidate without also
  submitting a chat, search or newline; a second Enter retains native behavior.
- Read-only offline English vocabulary packaging, separate product identity and
  user-data paths.
- Automatic English word learning disabled; typing does not start a learning broker or add its login startup entry.
- Grouped Chinese settings for input, candidate appearance, shortcuts, manual words and explicit AI editing.

Manual words support explicit editing, import/export and previous-version backups.
Automatic next-word prediction is outside this release; AI service and host validation remain separate acceptance items.

In a genuine AppContainer, AI cannot cold-start if its helper is not already
running, and opening settings from the candidate menu is restricted; open
“英文助手设置” from the Start menu. This limitation does not apply to every packaged app.

## Build and installation status

Windows builds require Rust stable with MSVC targets, Visual C++ build tools,
the Windows SDK, and the i686-pc-windows-msvc target. Installer compilation also
requires Inno Setup 6; metadata compilation requires Python 3.11+ without
third-party Python packages. Pass `-PythonPath` or set `ENGLISH_ASSIST_PYTHON`.

~~~powershell
.\platforms\windows\installer\build-english-assist.ps1 -Installer -PythonPath C:\Python313\python.exe
~~~

The English-specific script builds both TSF architectures, the settings/helper
programs and the pinned English vocabulary, then runs its configured checks.
It does not install the product or modify the system keyboard list.

Installer output:
dist/english-assist-0.2.0-windows-x64-setup.exe. See the versioned build record for the delivered artifact.
Do not use the inherited Retype installer, register.ps1 or upstream release
download links for this fork.

A validated installation should appear under Windows Settings → Time & language
→ Language & region → English (United States) → Language options → Keyboards,
and be selectable with Win+Space. It must preserve the existing default keyboard.
See the validation record for the completed r3 install/uninstall/reinstall cycle,
the bounded r2 Notepad evidence, and the remaining reboot and application checks.

## Everyday keyboard behavior

Use 1–5 to select a visible candidate, minus/plus to page, or Tab to accept without a space.
To enter an original spelling with a digit, such as python3, press Esc before the digit when candidates are visible. Use
Up/Down then Space to accept it with a space. Space alone keeps the spelling you
typed. Explicitly selected candidates consume the first Enter; otherwise Enter
retains the application's normal action. Esc keeps the original text.

See [English input](docs/english-input.md) for the full keyboard contract and
[P0 validation](docs/english-assist/INPUT_VALIDATION.md) for actual test status.

## License and attribution

Inherited source code is MIT licensed; retain LICENSE, NOTICE.txt and NOTICE-English-Assist.txt.
The English vocabulary has separate SymSpell, SCOWL and Google Ngram notices
under data/dict/raw/english/. Definitions and pinyin data retain ECDICT MIT and
CC-CEDICT CC BY-SA 3.0 attribution; the combined metadata file is distributed
under CC BY-SA 3.0. Data licenses do not become MIT because the code is MIT.

The matching source ZIP must include the current modified sources, Cargo.lock,
`data/dict/raw/english/`, and the complete `data/dict/raw/english-assist/` directory,
including `ecdict.csv.gz`, `cedict.txt.gz`, `SOURCES.json`, full licenses and notices.
Include `data/dict/english-lexicon.json`, `data/dict/resources.json` and the compiler,
preparation and verification scripts/tests in `tools/english-assist/`. A tracked-files-only
archive omits new working-tree files until they are committed.
Inherited modules for Chinese input, other platforms and synchronization remain
in the checkout; their existence does not imply those features ship in this
English-only product.
