# English Assist — Windows development branch

English Assist is a Windows English input-method project derived from
[Retype at 902e35e](https://github.com/L-Chris/retype/tree/902e35e06ee4f744320cf995c39b1f64693034cf).
It aims to provide local word completion and spelling suggestions, plus explicit
AI grammar correction and sentence completion, through a real English TSF entry.

[简体中文](README.zh-CN.md) · [English input](docs/english-input.md) ·
[P0 validation](docs/english-assist/INPUT_VALIDATION.md)

**Current status:** source implementation in progress. This branch has not yet
been compiled, installed or validated in real applications. Do not treat upstream
release binaries or historical upstream test results as this product's deliverable.

## Implemented in source, awaiting validation

- Independent English Assist 英文助手 entry under English (United States).
- Local prefix completion and spelling suggestions with original case preserved.
- Literal digits 0–9; English candidate numbers are removed.
- Tab or mouse acceptance without an automatic trailing space.
- Space preserves the original spelling unless a candidate was explicitly selected.
- Enter after explicit candidate navigation confirms the candidate without also
  submitting a chat, search or newline; a second Enter retains native behavior.
- Read-only offline English vocabulary packaging, separate product identity and
  user-data paths.
- Automatic English word learning disabled.
- Chinese settings work and explicit AI-editing protocol/service work in progress.

Theme polish, manual personal-word management, optional next-word prediction,
and full AI editing validation remain on the project plan.

## Build and installation status

Windows builds require Rust stable with MSVC targets, Visual C++ build tools,
the Windows SDK, and the i686-pc-windows-msvc target. Installer compilation also
requires Inno Setup 6.

~~~powershell
.\platforms\windows\installer\build-english-assist.ps1 -Installer
~~~

The English-specific script builds both TSF architectures, the settings/helper
programs and the pinned English vocabulary, then runs its configured checks.
It does not install the product or modify the system keyboard list.

Expected installer output after a successful build:
dist/english-assist-0.1.0-windows-x64-setup.exe. No successful build is claimed yet.
Do not use the inherited Retype installer, register.ps1 or upstream release
download links for this fork.

A validated installation should appear under Windows Settings → Time & language
→ Language & region → English (United States) → Language options → Keyboards,
and be selectable with Win+Space. It must preserve the existing default keyboard.
These behaviors remain P0 acceptance items.

## Everyday keyboard behavior

Type python3 directly. Use Tab to accept a word without adding a space, or
Up/Down then Space to accept it with a space. Space alone keeps the spelling you
typed. Explicitly selected candidates consume the first Enter; otherwise Enter
retains the application's normal action. Esc keeps the original text.

See [English input](docs/english-input.md) for the full keyboard contract and
[P0 validation](docs/english-assist/INPUT_VALIDATION.md) for actual test status.

## License and attribution

Inherited source code is MIT licensed; retain LICENSE and NOTICE.txt.
The English vocabulary has separate SymSpell, SCOWL and Google Ngram notices
under data/dict/raw/english/. Data licenses do not become MIT because the code is MIT.
Inherited modules for Chinese input, other platforms and synchronization remain
in the checkout; their existence does not imply those features ship in this
English-only product.
