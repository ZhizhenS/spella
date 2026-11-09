# English Assist 英文助手（开发分支）

本项目基于 [Retype 固定版本 902e35e](https://github.com/L-Chris/retype/tree/902e35e06ee4f744320cf995c39b1f64693034cf)
开发 Windows 英文输入法，目标是在英语键盘列表中提供本地单词补全、拼写建议，
以及主动触发的 AI 语法修正和句子补全。

[English](README.md) · [英文输入操作](docs/english-input.md) ·
[P0 验证记录](docs/english-assist/INPUT_VALIDATION.md)

**0.3.1 最终检查点：正式构建退出0，24套Rust汇总368通过、0失败、6忽略（含x86重复），Python33通过、PowerShell508项通过；安装后x64/x86探针各2项通过，激活S_OK。** 升级退出0，目录 `0.3.1-20261109070929-873493` 的24项产物摘要及build.json均匹配。可靠多词前缀后允许有限词频回退，避免相邻词表缺项时漏掉 `gay`、`good` 等尾词。候选顶部显示当前选择推断的词界，例如 `iamg` → `I am · g`；行内仍只显示尾词，确认仍完整提交当前组合一次。

用户反馈的 `iamg` 截图来自记事本PID39900，该进程在系统已安装0.3后仍加载0.2 R5 DLL，并非两个开关失效。新窗口可能复用旧进程；升级后应先保存工作、完全退出应用，再重新打开。本次退出全部旧记事本后，冷启动PID46068已确认加载0.3.1 DLL：iamg顶部显示 `I am · g`、首项going；iamga按3、iamgood按Tab、iamt按4后，保存文件精确为三行 `I am gay`、`I am good`、`I am trying to`。其他恢复标签未读取。

候选排名是本次快照，不是保证；本地解码尚不具备完整语法或神经语言模型句意理解，字符不同的 `imag` 仍走普通单词路径。本轮未重新调用AI HTTP或验收AI GUI，0.3 CLI和0.2 R5 GUI结果保留各自版本边界，具体收据见验证记录。

**历史 0.3 最终 R2 检查点：正式构建 Rust 364通过、0失败、6忽略（含x86重复），Python33通过、PowerShell508项通过；安装后x64/x86探针各2项通过，激活S_OK。** 当时目录 `0.3.0-20261109063014-912375` 的24项产物摘要及build.json匹配冻结清单。加载最终DLL的私有RichEdit宿主通过选词、退格、Esc和翻页，文档文本精确为 `I am trying to iambic apple iamt helped`。

两个开关在前一个0.3设置程序中完成off/on并读取HKCU确认保存；最终设置exe摘要不同，另行确认输入页两项均on，未重复保存测试，也不代表全部控件通过。0.3 production CLI真实DeepSeek请求在升级后成功，凭据保留。本轮未重新做全部应用、x86编辑器键入、Notepad AI GUI、卸载或重启删除验收；下方0.2 R5结果仍为历史。

## 0.3 连写英文

- 本地同时保留整体单词前缀和多条词界解释，允许末尾单词尚未输入完整。
- `ia` 可同时提供 `ia…` 开头单词和 `I am`；`iamt` 的行内候选可只显示 `trying`、`trying to`，确认时一次替换为完整 `I am trying`、`I am trying to`。
- 推断前缀留在当前组合内，不提前上屏；Esc 保留原始串，未显式选词的空格保留原始串并加空格。
- 连续输入超过48字符时停止连写搜索，回退普通单词补全路径，不截断原文。
- Windows 的“输入与查词”默认开启“连写智能分词”和“短语候选”；共享 Kernel 两项默认关闭，其他前端须先支持显示/提交分离再显式启用。

多条解释可能同时合理，以上示例不承诺唯一答案或固定排名。短语没有独立词典释义时显示“短语补全”，不挪用组成单词的词性/释义；词库查询耗时与GUI端到端耗时分别记录。

**历史 0.2 R5 检查点（2026-11-09）：R5 正式构建完成，Rust 合计 350 通过、0 失败、6 忽略，Python 19 通过，PowerShell 检查505项。**
Rust 数量包含 x86 重复执行，不是唯一用例总数。升级退出 0，当时验证的目录为
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

个人词库保存前保留上个版本备份；0.3 短语作为当前输入的候选，显式选择后才插入。AI 结果仍需人工检查，其他应用需分别验证兼容性。

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

已构建安装包为 dist/english-assist-0.3.1-windows-x64-setup.exe，安装及有限实测边界见验证记录。
交付产物以版本化构建记录为准。不要使用继承的 Retype 安装器、register.ps1
或上游发布页安装包来替代本项目构建。

安装后的验收目标是在“设置 → 时间和语言 → 语言和区域 → 英语（美国）
→ 语言选项 → 键盘”中看到本项目，用 Win+Space 选择，且保留原默认键盘。
验证记录包含0.3.1升级及冷启动记事本输入结果；旧AI与r3生命周期证据分别保留，其他应用与重启删除仍有未覆盖项。

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

0.3 独立内嵌的双词偏好来自 SymSpell MIT 仓库固定提交
`c239062ae02961df18ab7da1671d01b4388204e0` 的已发布文件，精确来源/许可哈希见
data/dict/raw/english-bigrams/SOURCES.json，完整声明见 NOTICE-English-Assist.txt。
它与 CC BY-SA 英汉侧车分别声明，不声称复现了该双词表的原始语料生成链，也不包含 Gboard 模型。

配套源码 ZIP 必须包含当前修改源码、Cargo.lock、data/dict/raw/english/，以及
data/dict/raw/english-assist/ 全目录中的 ecdict.csv.gz、cedict.txt.gz、SOURCES.json、
完整许可和署名；同时保留 data/dict/english-lexicon.json、resources.json 及
tools/english-assist/ 下的数据编译、准备、验证脚本和测试。新文件未提交时，
仅归档 Git 已跟踪文件会漏掉这些工作树内容。
0.3 还须保留 data/dict/raw/english-bigrams/ 与 core/english/assets/，包括原始gzip、
许可、来源/输出清单及内嵌二进制。

源码目录保留上游中文、其他平台与同步模块，用于后续整理和必要兼容；
这不代表英文专用产品会安装或提供这些功能。
