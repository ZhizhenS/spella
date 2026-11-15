# Spella

**一个 Windows 英文输入法，给"认识这个词，但拼不对"的人。**

[English](README.md)

你知道 *restaurant* 怎么读，却不确定是 *restaraunt*、*resturant* 还是
*restaurant*。用 Spella，按读音打出来就行，她会给出你想要的那个词，旁边带着中文
释义，上屏之前你就能确认是不是它。

所有功能都在本机运行。打字过程中不联网，也不记录你输入的内容。

> Spella 目前在 Windows 里显示为 **English Assist 英文助手**，这是项目早期的名字，
> 显示名会在之后的版本里改过来。

## 她能做什么

| 你输入 | Spella 给出 | 原因 |
|---|---|---|
| `definately` | **definitely** | 长词里元音写错 |
| `tommorow` | **tomorrow** | 双写字母放错位置 |
| `fone`、`nite`、`skool` | **phone**、**night**、**school** | 按读音拼写 |
| `teh`、`freind` | **the**、**friend** | 两个字母颠倒 |
| `probly`、`wensday` | **probably**、**Wednesday** | 漏了字母 |
| `dont`、`im` | **don't**、**I'm** | 省略了撇号 |
| `iamrobot` 加空格 | **I am robot** | 单词连着打，没加空格 |
| `nihao`、`pingguo` | **hello**、**apple** | 只记得中文，用拼音查英文 |
| `hel` | **help**、held、hello… | 普通补全，常用词优先 |
| `idk`、`rizz`、`anthropic` | **IDK**、**rizz**、**Anthropic** | 现代词、缩写和专名 |

每个候选都显示词性和中文释义。在 222 个常见拼写错误上，想要的词排在第 1 位的占
92%，排在前 3 位的占 98%。

## 按键

- **1–5** 或 **Tab**：选词（Tab 选第一个，或你移动到的那个）。
- **空格**：保留你输入的原样。连着打的单词会被拆开，除此之外不会偷偷改动。
- **Enter**：始终按输入的字母原样上屏。
- **↑ ↓**、**PageUp / PageDown**：在候选间移动、翻页。
- 单按 **Shift**：开关自动空格。

Spella 不会自作主张替换你的词，只有你选了候选，才会纠正。

## 安装

1. 在 [Releases](../../releases) 下载 `english-assist-<版本>-windows-x64-setup.exe`
   并运行。需要 64 位 Windows 10 或 11。
2. 按 **Win + 空格**，选择 **English Assist 英文助手**。
3. 安装前已经打开的应用，需要关掉重开后再输入。

安装不会改动你原有的键盘和默认输入法。卸载时，在 Windows 设置 → 应用里卸载
*English Assist*。

## 从源码构建

需要：Rust（stable、MSVC，并安装 `i686-pc-windows-msvc` 目标）、带 C++ 工作负载的
Visual Studio Build Tools、Python 3.11 或更新版本、Inno Setup 6。

```powershell
.\platforms\windows\installer\build-english-assist.ps1 -Installer
```

脚本会先核对每个词库都能从冻结的来源数据重新生成，然后编译 64 位和 32 位输入法、
运行测试，并把安装包写到 `dist\`。只跑引擎测试：

```powershell
cargo test -p retype-english -p retype-engine
```

## 工作原理

- **词表**：约 82,000 个带词频的英文单词，释义来自 ECDICT。另有一张人工审核的修正
  表，纠正书面语料对日常打字不合适的地方：缩写形式、现代词、古旧词、人名地名。
- **拼写找回**（[`core/english/src/spelling.rs`](core/english/src/spelling.rs)）：
  与输入相差不超过两处修改、或者*读音键*相同的词会被作为建议。排序按"词有多常用"
  减去"这种错误的代价"，人们真正常犯的错误代价低。详见
  [docs/english-assist/SPELLING.md](docs/english-assist/SPELLING.md)。
- **首页候选**：常用词排在生僻专名、俚语和古旧词之前。详见
  [docs/english-assist/MODERN_LEXICONS.md](docs/english-assist/MODERN_LEXICONS.md)。
- **输入法本体**：用 Rust 写的 Windows 文本服务（TSF），自带候选窗口。

开发历史和详细的验证记录在
[docs/english-assist/HISTORY.zh-CN.md](docs/english-assist/HISTORY.zh-CN.md)。

## 发现她弄错的词？

在 [`core/english/src/misspellings.txt`](core/english/src/misspellings.txt) 里加一行
（`你输入的拼写 想要的词`），然后提 issue 或 pull request。这个文件本身就是测试：
找回效果变差时构建会失败。

## 致谢与许可

Spella 基于 L-Chris 及贡献者的 [Retype](https://github.com/L-Chris/retype)（MIT），
它提供了 Rust 输入法内核和 Windows 文本服务。本仓库的代码以
[MIT 许可](LICENSE) 发布。

词库数据各自保留原有许可，见 [NOTICE.txt](NOTICE.txt) 和
[NOTICE-English-Assist.txt](NOTICE-English-Assist.txt)：

- [ECDICT](https://github.com/skywind3000/ECDICT)（MIT）— 英汉释义
- [CC-CEDICT](https://cc-cedict.org/)（CC BY-SA 4.0）— 用于拼音查词的汉英对应
- 经 [Kaikki](https://kaikki.org/) 整理的 Wiktionary（CC BY-SA 4.0）— 现代词和缩写
- [Wikidata](https://www.wikidata.org/)（CC0）— 软件、公司和产品名称
- [SymSpell](https://github.com/wolfgarbe/SymSpell) 词频表（MIT；源自 SCOWL 和 Google Books Ngram）
- [万象](https://github.com/amzxyz/rime-wanxiang)（CC BY 4.0）— 中文读音和词频

这是一个个人项目，开发过程中大量借助了 AI 编程助手。欢迎提 issue 和建议。
