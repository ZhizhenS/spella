# English Assist 0.4.0 — Windows development branch

English Assist is a Windows English input-method project derived from
[Retype at 902e35e](https://github.com/L-Chris/retype/tree/902e35e06ee4f744320cf995c39b1f64693034cf).
It aims to provide local word completion and spelling suggestions, plus explicit
AI grammar correction and sentence completion, through a real English TSF entry.

[简体中文](README.zh-CN.md) · [English input](docs/english-input.md) ·
[P0 validation](docs/english-assist/INPUT_VALIDATION.md)

## 0.4.0 input status and spacing

**Final R5 build and installation checkpoint:** the formal build completed with
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

The 0.4 installer path is `dist/english-assist-0.4.0-windows-x64-setup.exe`.
R5 was formally built, installed and checked in Notepad as recorded above; consult the versioned
validation record for the exact delivered artifact and completed UI checks.
Do not use the inherited Retype installer, register.ps1 or upstream release
download links for this fork.

A validated installation should appear under Windows Settings → Time & language
→ Language & region → English (United States) → Language options → Keyboards,
and be selectable with Win+Space. It must preserve the existing default keyboard.
See the validation record for the 0.4 build and synchronization checkpoints;
earlier Notepad, AI and lifecycle results remain separate from the current version
and outstanding application/reboot-deletion coverage.

## Everyday keyboard behavior

Use 1–5 to select a visible candidate, minus/plus to page, or Tab to accept without a trailing space.
To enter an original spelling with a digit, such as python3, press Esc before the digit when candidates are visible. Use
Up/Down then Space to accept a selected candidate. With automatic spacing on,
Space confirms without a trailing space and restores only already-typed internal
word boundaries when segmentation is on; with it off, Space inserts a manual
space. Shift toggles that preference. Enter submits the current raw input and
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
Inherited modules for Chinese input, other platforms and synchronization remain
in the checkout; their existence does not imply those features ship in this
English-only product.
