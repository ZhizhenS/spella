> Archived: this was the repository README up to English Assist 0.6.0, kept as the
> development and validation record. Links in it are relative to the repository root.

# English Assist 0.6.0 — Windows development branch

English Assist is a Windows English input-method project derived from
[Retype at 902e35e](https://github.com/L-Chris/retype/tree/902e35e06ee4f744320cf995c39b1f64693034cf).
It aims to provide local word completion and spelling suggestions, plus explicit
AI grammar correction and sentence completion, through a real English TSF entry.

[简体中文](README.zh-CN.md) · [English input](docs/english-input.md) ·
[P0 validation](docs/english-assist/INPUT_VALIDATION.md)

## 0.6.0 modern meanings and expanded names

**Final 0.6.0 R1 checkpoint:** the formal build passed 427 Rust checks (425
standard plus 2 explicitly run prepared-resource groups), 74 Python checks and
634 PowerShell checks: 1,135 counted passes, including repeated x86 runs, with
0 failures. Of 8 raw ignored entries, the 2 prepared groups later passed inside
that total; 4 installed x64/x86 probes passed separately with activation S_OK,
leaving 2 unexecuted entries. Upgrade exited 0 to
`C:\Program Files\EnglishAssist\versions\0.6.0-20261109143156-791774`;
all 29 artifact hashes and build.json matched. The mapped preference file identity
and all 23 values were preserved. Earlier results remain version-specific history.

- The offline modern-word subset contains **4,009 senses across 2,134 keys** from
  frozen Kaikki extracts of English and Chinese Wiktionary. Before 34 duplicate
  senses were merged, it admitted 3,982 source-Chinese senses (3,824 from the
  Chinese edition and 158 with confirmed Chinese translations in the English
  edition), plus 61 separately identified project Chinese additions prepared
  with Codex assistance. Those additions are not presented as upstream Chinese.
- Real POS and Chinese meanings coexist with ordinary ECDICT meanings and named
  entities. Badges distinguish network usage, abbreviations and slang from
  `现代 · 公司` and the existing pinyin evidence. Different senses of IMO retain
  their identities instead of being merged merely because they commit the same
  abbreviation. Reviewed conversational IDK/IMO/TBH meanings precede older cold
  abbreviation senses; common ordinary words retain their normal candidate lane.
- Selecting an abbreviation commits its short spelling, never its displayed full
  form. A reviewed exact network abbreviation with a sourced expansion can lead
  for an explicitly uppercase input such as `GOAT`; lowercase `goat` keeps its
  ordinary animal meaning, and uppercase alone does not promote SICK or APPLE
  slang. Ordinary modern words respect input case, such as `Rizz` and `RIZZ`;
  proven abbreviations and proper names retain canonical spelling.
- The Wikidata subset expands to **1,173 entities and 2,239 alias relations**
  (2,161 distinct spellings), preserving
  the original 51 reviewed seeds. Shared aliases may identify different entities.
  Chinese labels and short descriptions aid identification; they are not complete
  translations or a claim to cover all names or current Internet terminology.

Lookup and normal frozen-data checks are offline, do not read an API key, and do
not learn input history. Raw Enter, plain Space, bare Shift spacing toggle and
full-composition mixed-pinyin acceptance retain their existing rules. Reviewed
network abbreviations stay whole on plain Space, without expanding their full
forms. The lexical TSV is 620,800 bytes, SHA-256
`ccbb70532631d3cdc000ff63fd702b6d496e09d1b4f024e11a9670d4bfaa9749`;
coverage and provenance are in `data/dict/modern-lexicon.json` and
`data/dict/modern-entities.json`.

Notepad PID29288 was confirmed to load the R1 DLL. Physical-key checks and saved
file readback passed IDK/IMO short-form acceptance, rizz/RIZZ case, uppercase GOAT
versus ordinary goat, LibreOffice prefix completion, raw Enter after selection and
paging, Shift paging without toggling spacing, `iamrobot` + Space, and complete
`I will do the same` acceptance from the mixed-pinyin example. The owned file
matched all 12 expected lines, without trailing spaces or a final newline.
`kotlin` has no entry in this approved catalog and showed no candidate; Enter
preserved its raw spelling. This is a known coverage gap, not Kotlin completion.
Valid language tags, Input Tips and the default override matched their pre-upgrade
snapshots. Receipts: `work/research/v0.6.0-r1-notepad-live.json`,
`v0.6.0-r1-notepad-module.json`, `v0.6.0-keyboard-preserved.json` and
`work/build-logs/v0.6.0-r1-test-counts.json`.

These are bounded Notepad checks. This version did not verify every application,
real x86 editor typing, AI service/GUI, uninstall or reboot cleanup. Final source
ZIP check results are recorded in the accompanying delivery receipts; a complete
native rebuild from the ZIP was not performed. Candidate ranks are observed snapshots.

## Inherited 0.5 dictionary and mixed input; historical checks

**Historical 0.5.0 R1 checkpoint:** the formal build passed 413 Rust checks, 46 Python
checks and 619 PowerShell checks: 1,078 counted passes, including repeated x86
runs. Rust reported 0 failures and 7 raw ignored entries; the prepared-resource
integration was then explicitly run and is included in the 413 passes. Installed
x64/x86 activation probes each passed 2 checks with S_OK, counted separately.
Upgrade exited 0 to `C:\Program Files\EnglishAssist\versions\0.5.0-20261109121445-346651`;
all 25 installed artifact hashes and build.json matched the frozen build.
The 0.4.0 and earlier results below remain version-specific history.

- The relational dictionary keeps the matched Chinese headword, numbered-tone
  reading, original sense and source evidence for each pinyin-to-English link.
  Ordering uses Chinese word/reading frequency and relation quality instead of
  letting a frequent English gloss hide unrelated or dated Chinese senses.
- Labels such as `拼音 · 想` identify an exact pinyin key; `扩展 · 想法` marks a
  longer dictionary key found through a prefix. English POS/definitions remain
  separate from the Chinese word that caused the match. These are dictionary
  alternatives, not a promise of understanding a sentence's intended meaning.
- With both pinyin lookup and continuous segmentation enabled,
  `iwilldotheyiyang` and `iwilldothetongyang` can offer the suffix `same`, while
  confirmation replaces the composition once with `I will do the same`.
  The entire English prefix is preserved; it is not repeated in the candidate
  row. Incomplete pinyin uses the same bounded prefix lookup.
- Plain Space preserves the raw spelling when a pinyin interpretation is
  recognized, rather than splitting its letters into accidental English words
  or translating the tail. Tab, a number, a click, or Up/Down followed by Space
  explicitly accepts the mixed candidate. Enter still commits the raw
  composition, even after navigation. Neither cloud requests nor automatic
  learning are involved.
- A limited reviewed seed of 51 modern entities adds canonical company/product names
  such as Anthropic and its Claude-related identity. Its frozen Wikidata
  revisions and official identity links are retained separately from ordinary
  dictionary meanings. Capitalized prefixes promote names; common lowercase
  words retain their ordinary meaning alongside the entity. This is a small
  offline catalog, not the complete Wikidata knowledge base or an automatic
  current-events feed.

The bilingual sources are still a pinned ECDICT revision and the **2017-11-17
CC-CEDICT snapshot**. This rebuild changes the relationships and ranking; it does
not silently replace them with today's dictionary. Pinned Wanxiang v18.0.14
character/word frequencies help order Chinese matches. See the source and license
section for the exact independent snapshots.

Notepad PID58000 was confirmed to load the final R1 DLL. The two mixed examples
offered `same` first; number 1 and Tab each committed `I will do the same` once.
For `xiang`, the first page showed think/想, mutually/相, to/向, like/像 and item/项;
paging exposed fragrant/香 and elephant/象, while Enter kept literal `xiang`.
Anthropic's company/Claude identity and the ordinary adjective coexisted, with
number 1 committing canonical `Anthropic`. Plain Space produced `I am robot`
from `iamrobot`, and kept `iamnihao` raw despite a visible hello/你好 candidate.
The owned saved file matched all six expected lines. Automatic spacing stayed on,
and existing keyboard/default preferences were preserved. Receipts:
`work/research/v0.5.0-r1-notepad-live.json` and
`work/build-logs/v0.5.0-r1-test-counts.json`.
These are bounded Notepad checks; this version did not repeat AI, uninstall,
reboot deletion or real x86 editor typing, and a complete native rebuild from the
source ZIP has not been verified. Candidate ranks are snapshots, not guarantees.

## Inherited 0.4 behavior and historical checks

**Historical 0.4.0 R5 build and installation checkpoint:** the formal build completed with
396 Rust passes, 0 failures and 6 ignored results across 24 suites (including
repeated x86 runs), 33 Python passes and 602 PowerShell checks: 1,031 counted pass
results. Installed x64/x86 probes each passed 2 checks with in-process activation
S_OK; these 4 probe results are separate from the formal total. Upgrade exited 0
to `C:\Program Files\EnglishAssist\versions\0.4.0-20261109110234-275066`;
all 24 installed artifact hashes and build.json matched the frozen build.
Counts: `work/build-logs/v0.4.0-r5-test-counts.json`.

Fresh Notepad PID57228 was confirmed to load that final DLL. It independently
passed `iamrobot` + Space → `I am robot` without a trailing space; separate
`i` + Space, `am` + Space, `jiqiren` + 1 → `i am robot`; and `iamrobot` + Down +
Enter → literal `iamrobot` without adding a newline. The saved file matched those
three lines exactly. Shift changed the shared spacing flag off and back on.
Dragging the status bar persisted its position; × hid it, and final settings
restored its visibility and reset its saved position. These are bounded final
Notepad/status-bar checks, not an every-application, AI, or uninstall pass.

The 0.4 implementation adds a small, non-activating floating status bar for the
active English input field, including when no candidate list is open. It shows
live Caps Lock, automatic word spacing, word completion, pinyin lookup and
continuous segmentation, with settings and hide buttons. Drag its lower area to
move it; Input and lookup can restore its position or show it again. Feature
switches preserve the current composition; they synchronize when it ends and
apply to the next word without requiring another focus change. The caps
button changes Caps Lock only while the original editable field still has focus
and no modifier is held; Shift capitalization does not turn on its Caps indicator.

Automatic spacing defaults on in Windows. Confirming separate words such as
`i`, `am`, and `robot` inserts only needed spaces before each new word, leaving
the caret directly after `robot`. It never adds an automatic trailing space;
this also applies to pinyin selections. Plain Space with continuous segmentation
on can restore the internal spaces in the letters already typed, for example
`iamrobot` → `I am robot`, without completing an untyped word. A bare Shift tap
toggles automatic spacing; Shift with letters, `_`, or `+` keeps its normal use.
With spacing off, Space manually separates words. Enter submits the raw current
composition without choosing a candidate or inserting spaces; another Enter with
no composition retains the application's normal action. Escape preserves the raw
composition. Local spacing does not invoke AI or add input-history learning.
Exact spacing is conservative and limited to 64 typed characters; unfinished or
uncertain strings stay literal. Use Tab, a number or an explicit candidate
selection to accept a completion such as `trying` from `iamt`.

Input DWORD preferences now share one persistent mapping backed by
`%USERPROFILE%\.EnglishAssist\input-preferences-v1.bin`, so a packaged editor and
the separate settings application use the same switches. It contains preference
values only: no API key, input text or history. Installation migrates previous
HKCU input DWORDs; strings such as the manual-dictionary path retain their registry
representation. Invalid or unavailable shared data is rejected and settings
reports the error, rather than saving into an isolated package view.

**Historical 0.4 R3 synchronization checkpoint:** Notepad PID56416 loaded the R3
DLL; tapping Shift changed the shared spacing flag to off and the settings GUI
showed off. Switching it on in settings changed the same flag, and Notepad then
saved `I am robot` from `iamrobot` + Space with no trailing space. Receipt:
`work/research/v0.4.0-r3-bidirectional-live.json`. This is a bounded check of that
packaged Notepad and settings pair, not a guarantee for every AppContainer.
R4's longer-input, Caps Lock and feature-switch observations remain R4 evidence;
they were not all repeated in R5. Final R5 checks are listed above. The source
archive is not a native-runtime test, and earlier AI/lifecycle results remain
version-specific.
The older checkpoints below remain version-specific history.

**Historical 0.3.1 final checkpoint:** the formal build exited 0 with 368 Rust passes,
0 failures and 6 ignored results across 24 suites (including repeated x86 runs),
33 Python passes and 508 PowerShell checks. Installed x64/x86 probes each passed
2 checks with activation S_OK. Upgrade exited 0; payload
`0.3.1-20261109070929-873493` matched all 24 artifact hashes and build.json.

Continuous completion now allows a bounded word-frequency
fallback after a reliable multiword prefix, so missing adjacent-word pairs do not
hide tails such as `gay` or `good`. The candidate header explains the selected
boundary, for example `iamg` → `I am · g`; candidate rows still show only the tail,
and confirmation still commits the complete composition once.

The reported `iamg` screenshot came from Notepad PID39900, which still had the
0.2 R5 DLL loaded despite the system-wide 0.3 installation. This was not caused by
the two settings switches. Opening a new window can reuse the same process:
save your work, fully exit the application, then reopen it after upgrading.
After all old Notepad processes exited, fresh Notepad PID46068 was confirmed to
load that 0.3.1 DLL. Its live header showed `I am · g` with `going` first for
`iamg`; `iamga` + 3, `iamgood` + Tab, and `iamt` + 4 saved the exact three lines
`I am gay`, `I am good`, and `I am trying to`. Other restored tabs were not read.
Ranks are snapshots, not guarantees; this local decoder is not a full grammar or
neural sentence-understanding model, and `imag` follows a different ordinary-word
path from `iamg`. AI HTTP/GUI requests were not repeated for 0.3.1; the earlier
0.3 CLI and 0.2 R5 GUI results remain version-specific. See the
[validation record](docs/english-assist/INPUT_VALIDATION.md) for receipts and limits.

**Historical 0.3 final R2 checkpoint:** the formal build passed 364 Rust tests with 0 failures
and 6 ignored results (including repeated x86 runs), 33 Python tests and 508
PowerShell checks. Installed x64/x86 probes each passed 2 checks with activation
S_OK. Payload `0.3.0-20261109063014-912375` matched all 24 artifact hashes and
build.json. A private RichEdit host loading that DLL produced the exact document
text `I am trying to iambic apple iamt helped` through the tested selection,
backspace, Escape and paging actions.

Both new switches were saved off/on in an earlier 0.3 settings build. The final
settings executable has a different hash; its input page was separately checked
with both switches on, without repeating the save test or claiming all controls
tested. Production CLI DeepSeek requests also passed after upgrade with the
existing credential. This release was not tested in every application, in an x86
editor, or in a fresh Notepad AI GUI cycle; uninstall/reboot deletion were not
repeated. Detailed version-specific receipts are in the validation record.

## 0.3 continuous English

- Local decoding keeps ordinary whole-word prefixes alongside multiple word-boundary alternatives, with the final word allowed to remain unfinished.
- `ia` can offer an `ia…` word and `I am`; for `iamt`, a row can show only `trying` or `trying to`, while confirmation replaces the owned composition once with `I am trying` or `I am trying to`.
- An inferred prefix stays inside the current composition until explicit confirmation. Escape preserves the raw input; 0.4 Space follows the automatic-spacing rules above.
- Beyond 48 input characters, continuous search falls back to the ordinary word-completion path without truncating the original text.
- Windows opts in by default through “连写智能分词” and “短语候选” in Input and lookup. The shared Kernel defaults both options off so another frontend must support independent display/commit text before opting in.

Candidate ranking can have multiple valid interpretations; these examples do not
promise a unique result or fixed position. A phrase without its own dictionary
entry uses a phrase-completion hint instead of borrowing a component word's
definition or part of speech. Query timings are recorded separately from GUI latency.

**Historical 0.2 R5 checkpoint (2026-11-09):** the formal R5 build completed with 350 Rust
test passes, 0 failures and 6 ignored results across x64/x86 runs (including
repeated tests), plus 19 Python passes and 505 PowerShell checks. Upgrade returned 0; that tested payload was
`0.2.0-20261109035537-776110`, with all 24 artifacts and build.json matching the
frozen build. Successful requests confirmed that the privately configured
credential survived the upgrade.
Installed R5 x64/x86 probes each passed 2 checks with activation S_OK; these do not replace real typing tests in x86 editors.

Real DeepSeek HTTP requests corrected a controlled grammar sample and completed
a sentence. In a Notepad process loading the R5 DLL, grammar preview and confirmed
replacement matched the saved file; sentence completion also passed both confirmed
replacement and Escape cancellation with the original text preserved. These are
bounded Notepad checks, not a compatibility or quality claim for every application.
R5 was upgraded and tested without another uninstall cycle. Earlier r2/r3 offline,
lifecycle, probe and settings screenshot evidence remains version-specific history;
reboot deletion of queued old files is still unverified. See the
[validation record](docs/english-assist/INPUT_VALIDATION.md) for samples and limits.

## Implemented features

- Independent English Assist 英文助手 entry under English (United States).
- Local prefix completion and spelling suggestions with original case preserved.
- Five candidates per page, 1–5 selection and minus/plus paging; digits remain literal without candidates.
- Chinese definitions, part of speech, optional phonetics and pinyin lookup.
- Separate modern-word senses and expanded named-entity aliases, with sourced
  usage labels and display-only abbreviation expansions.
- Tab or mouse acceptance without an automatic trailing space.
- Automatic spacing adds only needed word boundaries and never a trailing space.
- Enter submits the raw current composition even after candidate navigation;
  with no composition, Enter retains native behavior.
- Read-only offline English vocabulary packaging, separate product identity and
  user-data paths.
- Automatic English word learning disabled; typing does not start a learning broker or add its login startup entry.
- Grouped Chinese settings for input, candidate appearance, shortcuts, manual words and explicit AI editing.

Manual words support explicit editing, import/export and previous-version backups.
0.3 phrase alternatives require explicit selection; they do not silently insert
text. AI results still require review, and other applications need their own compatibility checks.

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

The 0.6.0 R1 installer is `dist/english-assist-0.6.0-windows-x64-setup.exe`.
Its formal build, installed hashes, activation probes and bounded Notepad checks
are recorded above. The 0.5.0 installation and earlier lifecycle checks remain
historical; this release did not repeat uninstall or reboot cleanup.
Do not use the inherited Retype installer, register.ps1 or upstream release
download links for this fork.

A validated installation should appear under Windows Settings → Time & language
→ Language & region → English (United States) → Language options → Keyboards,
and be selectable with Win+Space. It must preserve the existing default keyboard.
See the validation record for the current build and historical synchronization checkpoints;
earlier Notepad, AI and lifecycle results remain separate from the current version
and outstanding application/reboot-deletion coverage.

## Everyday keyboard behavior

Use 1–5 to select a visible candidate, minus/plus to page, or Tab to accept without a trailing space.
To enter an original spelling with a digit, such as python3, press Esc before the digit when candidates are visible. Use
Up/Down then Space to accept a selected candidate. With automatic spacing on,
plain Space confirms without a trailing space and may restore already-typed
internal word boundaries when segmentation is on; a recognized pinyin
interpretation keeps its raw spelling unless explicitly selected. With automatic
spacing off, Space inserts a manual space. Shift toggles that preference. Enter submits the current raw input and
consumes that first Enter; a subsequent Enter has the application's normal action.
Esc keeps the original text.

See [English input](docs/english-input.md) for the full keyboard contract and
[P0 validation](docs/english-assist/INPUT_VALIDATION.md) for actual test status.

## License and attribution

Inherited source code is MIT licensed; retain LICENSE, NOTICE.txt and NOTICE-English-Assist.txt.
The English vocabulary has separate SymSpell, SCOWL and Google Ngram notices
under data/dict/raw/english/. Definitions and pinyin data retain ECDICT MIT and
CC-CEDICT CC BY-SA 3.0 attribution; the combined metadata file is distributed
under CC BY-SA 3.0. Data licenses do not become MIT because the code is MIT.

The frozen CC-CEDICT source is the 2017 snapshot mirrored at HallelujahIM
`8be294f70a4dc7b24a93f07c41df6c34add07a07`, with its original CC BY-SA 3.0
header. The [current MDBG download page](https://www.mdbg.net/chinese/dictionary?page=cc-cedict)
lists CC BY-SA 4.0 for its current release; that does not change this package's
source snapshot. ECDICT is pinned at `bc015ed2e24a7abef49fc6dbbb7fe32c1dadaf8b`
under MIT. Wanxiang v18.0.14 `zi.dict.yaml` and `jichu.dict.yaml` retain CC BY 4.0
attribution and hashes in `data/dict/raw/english-assist/SOURCES.json` and
`data/dict/raw/wanxiang-base/SOURCE.md`. Modern entities retain their separate
CC0 Wikidata snapshot/revision records and reviewed official identity URLs in
`data/dict/modern-entities.json`; they are independent of the bilingual sidecar.

The 0.6 modern lexical subset and its separately marked project Chinese additions
are distributed under CC BY-SA 4.0, retaining Wiktionary-contributor attribution,
per-sense URLs, frozen hashes and the projection/change notice. Wiktextract's MIT
software license does not relicense its data. Retain `data/dict/modern-lexicon*`,
the frozen projections and both edition-source manifests in
`data/dict/raw/kaikki-modern/`, full licenses/notices and the independent compiler
and checks. Normal builds do not refresh sources. Complete large dumps remain in
a separate acquisition cache; the bundled projections are not the full dictionary.

The separate 0.3 embedded bigram preferences use the frozen published SymSpell
file from MIT repository commit `c239062ae02961df18ab7da1671d01b4388204e0`.
Exact source/license hashes and provenance are in `data/dict/raw/english-bigrams/SOURCES.json`;
the full notice is in NOTICE-English-Assist.txt. This is independent of the CC BY-SA
lexicon sidecar, with no claim of reproducing the bigram file's original corpus
generation chain or shipping a Gboard model.

The matching source ZIP must include the current modified sources, Cargo.lock,
`data/dict/raw/english/`, and the complete `data/dict/raw/english-assist/` directory,
including `ecdict.csv.gz`, `cedict.txt.gz`, `SOURCES.json`, full licenses and notices.
Include `data/dict/english-lexicon.json`, `data/dict/resources.json` and the compiler,
preparation and verification scripts/tests in `tools/english-assist/`. A tracked-files-only
archive omits new working-tree files until they are committed.
For 0.3 also retain `data/dict/raw/english-bigrams/` and `core/english/assets/`,
including the frozen gzip, license, source/output manifests and embedded image.
For 0.5 also retain the pinned Wanxiang frequency inputs, relational v2 compiler
and tests, and the modern-entity JSON/TSV, raw Wikidata projection, source/license
records and offline verification tools. Source archives must exclude credentials
and personal input files.
Inherited modules for Chinese input, other platforms and synchronization remain
in the checkout; their existence does not imply those features ship in this
English-only product.
