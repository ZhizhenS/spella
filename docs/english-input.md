# English Assist 英文输入

This Windows development branch provides an independent English Assist TSF entry.
The source changes cover local word completion, spelling suggestions and keyboard
selection. This branch has not yet been compiled, installed or tested in a real
application; source-level behavior below is the implementation target, not a
completed compatibility claim. See [P0 validation](english-assist/INPUT_VALIDATION.md).

## System entry

The installer registers only **English Assist 英文助手**, under English (United
States), language ID 0409. Choose it with Win+Space. Windows Settings exposes it
under Time & language → Language & region → English (United States) → Language
options → Add a keyboard. The installer preserves existing keyboard entries and
does not set a new default keyboard.

The entry stays English. This product does not install the upstream Chinese
profile. A delayed active-profile response during activation defaults to English.
Actual first-focus behavior and language-list enrollment still need P0 testing.

## Keyboard interaction

| Action | Behavior |
| --- | --- |
| Type letters | Show local prefix candidates; preserve capitals and internal apostrophes. |
| Type 0–9 | Keep the digit as literal input, including python3 and win11. Digits never select English candidates. |
| Tab with a candidate | Accept the highlighted candidate without inserting a trailing space. |
| Click a candidate | Accept it without inserting a trailing space, matching Tab. |
| Up / Down | Explicitly select a candidate. |
| Space | Commit the original spelling plus a space unless a candidate was explicitly selected; then commit that candidate plus a space. |
| Enter without explicit selection | Commit the original spelling and let the application handle Enter. |
| Enter after explicit selection | Accept only the candidate, without a trailing space or host Enter. A second Enter goes to the application. |
| Escape | Keep the typed spelling and dismiss completion. |
| Shift+Tab | Commit the original spelling and preserve the application's reverse focus navigation. |
| Tab without a candidate | Preserve the application's Tab action. |
| Left / Right / Home / End | Finish the original word and retain native caret navigation. |
| Punctuation and ordinary shortcuts | Preserve literal punctuation and native application actions. |

English candidates do not display numeric shortcut labels. A visual highlight
alone does not count as explicit selection: Space must not silently correct text.
For example, a spelling suggestion for teh can offer the, but Space retains teh
unless the user has deliberately selected the suggestion.

Spelling suggestions cover a bounded insertion, deletion, substitution or adjacent
transposition. They arrive after prefix candidates; they do not automatically
replace typed text.

## Offline vocabulary and learning

The English-specific build compiles the pinned frequency data and packages the
validated RTENGL01 image beside the installed components, in dictionaries/.
The installer publishes that immutable location to the current user's profile,
so first-use completion does not require an online dictionary download.

Source data and attribution:

- data/dict/raw/english/frequency.txt
- data/dict/raw/english/SOURCE.txt
- data/dict/raw/english/LICENSE-SymSpell
- data/dict/raw/english/LICENSE-SCOWL
- data/dict/raw/english/LICENSE-Google-Ngram

This uses the upstream word-frequency data; the current spelling algorithm is
Retype's bounded search, rather than the SymSpell algorithm.

**Automatic English word learning is disabled in this branch.** Committing a
word or accepting a candidate does not emit a learning event. The current product
uses the read-only base vocabulary; manual personal-word management and an
optional learning preference are future work. Existing upstream learning
infrastructure remains in the source for helper and compatibility functions,
which must not be confused with automatic English word collection.

Password, hidden and explicitly private input scopes are excluded by the existing
input-scope checks. Custom controls and unknown hosts still need real testing;
do not assume arbitrary controls expose reliable security or editing metadata.
Normal English completion does not call a cloud model. AI grammar/sentence editing
is a separate explicit operation under development, with its own range and
preview validation.

## Validation commands

Run these only after configuring Rust MSVC, Visual C++ tools and the Windows SDK:

~~~powershell
cargo fmt --all -- --check
cargo test --locked -p retype-engine --test english
cargo test --locked -p retype-candidate-ui --features egui --lib
cargo test --locked -p retype-tsf
cargo test --locked --target i686-pc-windows-msvc -p retype-tsf
.\platforms\windows\installer\build-english-assist.ps1 -Installer
~~~

These are proposed commands, not a record that tests passed. Native activation,
text submission, focus changes, application shortcuts and clean uninstall require
the real-application checks in [INPUT_VALIDATION.md](english-assist/INPUT_VALIDATION.md).
The original upstream validation record does not establish this branch's behavior.

## 中文操作说明

本分支的系统入口是 **English Assist 英文助手**，绑定英语（美国），通过
Win+Space 选择。源码已经改为英文专用入口，但尚未完成编译、安装和真实应用验收。

- 输入英文得到本地单词补全，数字 0–9 按原样输入，不用先按 Esc。
- Tab 或点击接受候选，默认不补空格；上下键进入显式选词。
- 空格默认保留原词并加空格；显式选词后才接受候选并加空格。
- 未显式选词时，Enter 保留原词并执行应用的回车行为。
- 显式选词后的第一次 Enter 只选词，第二次 Enter 才交给应用。
- Esc 保留原输入并关闭候选；无候选的 Tab、Shift+Tab 和光标移动保留应用用途。
- 英文候选不显示数字选词编号，也不会自动把拼写建议替换进正文。
- 英文专用安装包设计为内置离线词库；当前尚无已构建安装包可供验证。
- 本版不自动学习输入词汇，手动个人词库与可选学习开关尚未实现。
- AI 语法修正和补句为独立的主动操作；不能把协议或服务代码已加入视为端到端功能已验收。

本分支不安装中文 Profile。源码保留的上游中文及其他平台模块属于继承代码，
其旧使用方式不代表英文助手当前产品入口。
