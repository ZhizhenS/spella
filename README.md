# Spella

**An English input method for Windows, for people who know the word but not the spelling.**

[简体中文](README.zh-CN.md)

Spella helps in the two moments when writing English stops:

- **You only know the word in Chinese.** Type its pinyin and get the English
  word: `canting` gives **restaurant**, `huanjing` gives **environment**.
- **You know the word but not its letters.** Type what you hear and get the
  word you meant: `restaraunt` gives **restaurant**, `fone` gives **phone**.

Each candidate shows its Chinese meaning, so you can tell it is the right word
before it goes into your text.

Everything runs on your computer. Nothing is sent anywhere while you type, and
your typing is never recorded.

> Spella is installed and shown in Windows as **English Assist 英文助手**. That
> was the project's working name; the display name will follow in a later release.

## Pinyin to English

Think of the word in Chinese, type its pinyin without tones or spaces, and pick
the English word. No switching to a dictionary or a translator.

| You type | Chinese | Spella offers |
|---|---|---|
| `nihao` | 你好 | **hello**, hi |
| `pingguo` | 苹果 | **apple** |
| `mingtian` | 明天 | **tomorrow** |
| `pengyou` | 朋友 | **friend**, pal, companion |
| `zhongyao` | 重要 | **important**, major, significant |
| `mafan` | 麻烦 | **annoying**, inconvenient, troublesome |
| `tushuguan` | 图书馆 | **library** |
| `duibuqi` | 对不起 | **sorry**, excuse me |
| `shouji` | 手机 | **cell phone**, mobile phone |
| `xinyongka` | 信用卡 | **credit card** |

- Several English words are offered when the Chinese word has several, each with
  its part of speech and meaning, so you can choose the one that fits.
- Unfinished pinyin works too: `pingg` already offers **apple**.
- With joined typing on, pinyin can end an English sentence typed without
  spaces: `iwilldotheyiyang` offers **I will do the same**.
- Real English words keep priority. `he`, `you` and `long` stay English, and
  `fo` is the start of *for* before it is 佛.
- When several Chinese words share the same pinyin, the common one comes first:
  `wanshang` is 晚上 (*evening*) before 玩赏.
- The lookup is offline. It covers Chinese words whose English is one word or a
  short phrase found in the dictionary; a single syllable such as `gui` (贵) is
  treated as English letters first.

## Everything else she does

| You type | Spella offers | Why |
|---|---|---|
| `definately` | **definitely** | a wrong vowel in a long word |
| `tommorow` | **tomorrow** | a doubled letter in the wrong place |
| `fone`, `nite`, `skool` | **phone**, **night**, **school** | spelled the way it sounds |
| `teh`, `freind` | **the**, **friend** | two letters swapped |
| `probly`, `wensday` | **probably**, **Wednesday** | letters left out |
| `dont`, `im` | **don't**, **I'm** | apostrophe left out |
| `iamrobot` + Space | **I am robot** | words typed without spaces |
| `hel` | **help**, held, hello… | ordinary completion, common words first |
| `idk`, `rizz`, `anthropic` | **IDK**, **rizz**, **Anthropic** | modern words, abbreviations and names |

Every candidate shows its part of speech and Chinese meaning. On a set of 222
common misspellings, the intended word is the first candidate for 92% and among
the first three for 98%.

## Keys

- **1–5** or **Tab**: take a candidate (Tab takes the first, or the one you moved to).
- **Space**: keep exactly what you typed. Words typed without spaces are split;
  nothing else is changed behind your back.
- **Enter**: commit the letters as typed, always.
- **↑ ↓**, **PageUp / PageDown**: move through candidates.
- **Shift** (tap): turn automatic spacing on or off.

Spella never replaces a word on her own. A correction happens only when you pick it.

## Install

1. Download `english-assist-<version>-windows-x64-setup.exe` from
   [Releases](../../releases) and run it. Windows 10 or 11, 64-bit.
2. Press **Win + Space** and choose **English Assist 英文助手**.
3. Reopen any application that was already running before you type in it.

Your existing keyboards and default input method are left as they are. To remove
Spella, uninstall *English Assist* from Windows Settings → Apps.

## Build from source

You need Rust (stable, MSVC, with the `i686-pc-windows-msvc` target), Visual
Studio Build Tools with the C++ workload, Python 3.11 or newer and Inno Setup 6.

```powershell
.\platforms\windows\installer\build-english-assist.ps1 -Installer
```

The script checks that every dictionary reproduces from its frozen source, builds
the 64-bit and 32-bit input method, runs the tests and writes the installer to
`dist\`. To run only the engine tests:

```powershell
cargo test -p retype-english -p retype-engine
```

## How it works

- **Vocabulary**: about 82,000 English words with frequencies, annotated from
  ECDICT. A reviewed overlay corrects what a book corpus gets wrong for everyday
  typing: contractions, modern words, archaic words, names.
- **Spelling recovery** ([`core/english/src/spelling.rs`](core/english/src/spelling.rs)):
  a word is suggested when it is within two edits of what you typed or has the
  same *sound key*. Suggestions are ranked by how common the word is minus a
  cost for the mistake, and the mistakes people really make are cheap.
  See [docs/english-assist/SPELLING.md](docs/english-assist/SPELLING.md).
- **First page**: common words come before rare names, slang and archaic words.
  See [docs/english-assist/MODERN_LEXICONS.md](docs/english-assist/MODERN_LEXICONS.md).
- **Input method**: a Windows Text Services Framework text service written in
  Rust, with its own candidate window.

Development history and detailed validation records are kept in
[docs/english-assist/HISTORY.md](docs/english-assist/HISTORY.md).

## Found a word she gets wrong?

Add a line to [`core/english/src/misspellings.txt`](core/english/src/misspellings.txt)
(`what-you-typed intended-word`) and open an issue or pull request. That file is
the test: the build fails if recovery gets worse.

## Credits and licences

Spella is built on [Retype](https://github.com/L-Chris/retype) by L-Chris and
contributors (MIT), which provides the Rust input-method core and the Windows
text service. The code in this repository is released under the
[MIT License](LICENSE).

Dictionary data keeps its own licences; see [NOTICE.txt](NOTICE.txt) and
[NOTICE-English-Assist.txt](NOTICE-English-Assist.txt):

- [ECDICT](https://github.com/skywind3000/ECDICT) (MIT) — English–Chinese definitions
- [CC-CEDICT](https://cc-cedict.org/) (CC BY-SA 4.0) — Chinese–English relations for pinyin lookup
- Wiktionary through [Kaikki](https://kaikki.org/) (CC BY-SA 4.0) — modern words and abbreviations
- [Wikidata](https://www.wikidata.org/) (CC0) — names of software, companies and products
- [SymSpell](https://github.com/wolfgarbe/SymSpell) frequency lists (MIT; derived from SCOWL and Google Books Ngram)
- [Wanxiang](https://github.com/amzxyz/rime-wanxiang) (CC BY 4.0) — Chinese readings and frequencies

This is a personal project, written with a great deal of help from AI coding
assistants. Issues and suggestions are welcome.
