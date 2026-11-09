# English Assist 英文助手（开发分支）

本项目基于 [Retype 固定版本 902e35e](https://github.com/L-Chris/retype/tree/902e35e06ee4f744320cf995c39b1f64693034cf)
开发 Windows 英文输入法，目标是在英语键盘列表中提供本地单词补全、拼写建议，
以及主动触发的 AI 语法修正和句子补全。

[English](README.md) · [英文输入操作](docs/english-input.md) ·
[P0 验证记录](docs/english-assist/INPUT_VALIDATION.md)

**当前检查点（2026-11-09）：R5 正式构建完成，Rust 合计 350 通过、0 失败、6 忽略，Python 19 通过，PowerShell 检查505项。**
Rust 数量包含 x86 重复执行，不是唯一用例总数。升级退出 0，当前目录为
`0.2.0-20261109035537-776110`，24 项产物及 build.json 与冻结构建一致；用户私下
配置的凭据在升级后由实际成功请求确认保留。
R5 安装后 x64/x86 探针各2项通过，激活均为 S_OK；不替代 x86 编辑器真实键入测试。

真实 DeepSeek HTTP 已完成受控样例的语法修正与补句。加载 R5 DLL 的记事本中，
语法预览后确认替换、补句后确认替换，以及 Esc 取消保留原文，均与保存文件核对一致。
这是有限的记事本验证，不等于所有应用兼容或输出质量普遍保证。本轮没有重新卸载；
r2/r3 离线、生命周期、探针及设置截图属于此前版本证据，旧文件重启删除仍未核验，
具体样例及边界见验证记录。
上游安装包、历史测试报告不等于英文助手的交付或验收结果。

## 已实现功能

- 独立的 English Assist 英文助手系统入口，绑定英语（美国）。
- 本地前缀补词、拼写建议和大小写保留。
- 每页五条候选，1–5 选当前页，减加号翻页；无候选时数字原样输入。
- 中文释义、词性、可选音标，以及拼音找英文。
- Tab 和鼠标选词默认不加空格。
- 空格保留原词；只有显式选中候选后，空格才接受该候选并加空格。
- 显式选择后的第一次 Enter 只选词，第二次 Enter 才执行应用的回车行为。
- 英文离线词库随专用安装包分发；产品标识、用户数据路径与上游隔离。
- 本版关闭自动英文词汇学习，输入不启动学习后台服务，也不添加该服务的登录启动项。
- 中文分类设置、候选外观、快捷键、手动词库增删改及导入导出。

个人词库保存前保留上个版本备份；本版不提供下一词自动预测。AI 结果仍需人工检查，其他应用需分别验证兼容性。

真正运行在 AppContainer 中的应用，在 AI 辅助进程尚未运行时不支持 AI 冷启动，
从候选菜单打开设置也受限制；请从开始菜单打开“英文助手设置”。此限制不代表所有打包应用。

## 构建与安装状态

需要 Rust MSVC 工具链、Visual C++ 构建工具、Windows SDK 和
i686-pc-windows-msvc 目标。生成安装包还需要 Inno Setup 6；词义数据编译需要
Python 3.11+，无需第三方 Python 包。使用 `-PythonPath` 或设置 `ENGLISH_ASSIST_PYTHON`。

~~~powershell
.\platforms\windows\installer\build-english-assist.ps1 -Installer -PythonPath C:\Python313\python.exe
~~~

英文专用脚本构建 x64/x86 输入组件、设置程序、后台组件和固定版本英文词库，
并执行其配置的检查；该脚本本身不安装产品，也不修改系统键盘列表。

安装包为 dist/english-assist-0.2.0-windows-x64-setup.exe。
交付产物以版本化构建记录为准。不要使用继承的 Retype 安装器、register.ps1
或上游发布页安装包来替代本项目构建。

安装后的验收目标是在“设置 → 时间和语言 → 语言和区域 → 英语（美国）
→ 语言选项 → 键盘”中看到本项目，用 Win+Space 选择，且保留原默认键盘。
验证记录区分 R5 构建、升级和记事本 AI 结果，保留此前 r3 生命周期证据与重启、其他应用待核验项。

## 键盘操作

1–5 接受当前页候选，减加号翻页，Tab 接受候选且不补空格。候选可见时要输入
python3 等原始串，可先 Esc 结束组字再输入数字。上下键显式选中后，
空格接受候选并加空格。未显式选择时，空格保留原词。

显式选词后的 Enter 只确认候选，避免同时发送聊天或提交搜索；再次 Enter
才交给应用。未显式选词的 Enter 保留应用原有行为，Esc 保留原输入。

完整语义和待验收项目见 [英文输入](docs/english-input.md)
和 [P0 验证记录](docs/english-assist/INPUT_VALIDATION.md)。

## 许可与来源

继承的代码使用 MIT，保留 LICENSE、NOTICE.txt 和 NOTICE-English-Assist.txt。
英文数据另有 SymSpell、SCOWL 和 Google Ngram 的许可及署名，
位于 data/dict/raw/english/；词义及拼音数据保留 ECDICT MIT 和 CC-CEDICT CC BY-SA 3.0
署名，组合元数据文件按 CC BY-SA 3.0 分发，代码仍为 MIT。

配套源码 ZIP 必须包含当前修改源码、Cargo.lock、data/dict/raw/english/，以及
data/dict/raw/english-assist/ 全目录中的 ecdict.csv.gz、cedict.txt.gz、SOURCES.json、
完整许可和署名；同时保留 data/dict/english-lexicon.json、resources.json 及
tools/english-assist/ 下的数据编译、准备、验证脚本和测试。新文件未提交时，
仅归档 Git 已跟踪文件会漏掉这些工作树内容。

源码目录保留上游中文、其他平台与同步模块，用于后续整理和必要兼容；
这不代表英文专用产品会安装或提供这些功能。
