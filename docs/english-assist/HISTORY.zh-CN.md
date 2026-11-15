> 存档：这是 English Assist 0.6.0 及之前的仓库 README，作为开发与验证记录保留。
> 文中的链接以仓库根目录为基准。

# English Assist 0.6.0 英文助手（开发分支）

本项目基于 [Retype 固定版本 902e35e](https://github.com/L-Chris/retype/tree/902e35e06ee4f744320cf995c39b1f64693034cf)
开发 Windows 英文输入法，目标是在英语键盘列表中提供本地单词补全、拼写建议，
以及主动触发的 AI 语法修正和句子补全。

[English](README.md) · [英文输入操作](docs/english-input.md) ·
[P0 验证记录](docs/english-assist/INPUT_VALIDATION.md)

## 0.6.0 现代词义、缩写与扩展专名

**0.6.0 R1正式检查点：** Rust427次通过（常规425＋显式准备资源集成2）、Python74、PowerShell634，共1,135次通过结果、0失败，含x86重复，不是唯一测试总数。原始汇总8次忽略中的2组准备资源已计入通过；安装后x64/x86探针各2项通过、S_OK，4项另计，仍有2项未执行。升级退出0，目录 `C:\Program Files\EnglishAssist\versions\0.6.0-20261109143156-791774` 的29项产物摘要及build.json匹配，映射偏好文件身份及23个值保留。下方旧版结果仍属历史。

- 离线现代词库含 **4,009个义项、2,134个检索键**，来自冻结Kaikki英语及中文维基词典提取。归并前共有3,982条来源中文义项（中文版本3,824条、英语版本确证中译158条），另有61条Codex辅助整理的项目中文补充；34条明确重复归并后为4,009条。项目补充单独标来源，不冒充上游译文。
- 真实词性、中文义项与已有ECDICT普通义、现代实体分别保留。徽标区分网络、缩写、俚语等用途与“现代 · 公司”，拼音/扩展的匹配汉字标签继续保留。IMO的日常、海事及数学等不同义项可以分别选择，不因提交文字相同就合并消失。
- 已审核常用IDK/IMO/TBH网络缩写义优先于旧冷义；选择时只提交缩写，全称只用于说明。全大写GOAT须有精确匹配、已审核网络缩写及真实全称证据才可优先网络义，小写goat保留动物普通义；不能仅凭大写就把SICK或APPLE冷俚语升到普通义之前。普通现代词尊重输入大小写，如Rizz/RIZZ；有依据的缩写和专名保留规范拼写。
- Wikidata目录扩至 **1,173个实体、2,239条别名关系**（2,161种不同拼写），保留原51条审核种子。相同别名可对应不同实体；中文标签、短描述用于辨认身份，不等于完整中译，也不承诺所有专名或实时网络热词都已收录。

普通查词与冻结数据的常规构建/校验离线进行，不读取API Key、不逐键联网、不自动学习历史。Enter原串、Space原串/内部空格、单Shift空格开关和混合尾词完整确认规则不变；已审核网络缩写在直接Space时保持完整原串，不自动展开全称。词义TSV为620,800字节，SHA-256 `ccbb70532631d3cdc000ff63fd702b6d496e09d1b4f024e11a9670d4bfaa9749`，具体来源与覆盖见 `data/dict/modern-lexicon.json`、`data/dict/modern-entities.json`。

真实记事本PID29288已确认加载最终R1 DLL。逐键输入并保存读回通过：IDK/IMO只提交缩写、rizz/RIZZ大小写、GOAT网络义与goat山羊普通义、LibreOffice前缀补全、候选导航/翻页后Enter原串、Shift组合翻页不切空格偏好、iamrobot空格分词，以及混合拼音完整提交 `I will do the same` 一次。自有文件精确匹配12行，无尾空格、无最后换行。kotlin未收录于本次审核目录，界面无候选，Enter保留原串成功；这是已知覆盖缺口，不能说已补全Kotlin。有效语言标签、Input Tips及默认覆盖值升级前后相同。收据：`work/research/v0.6.0-r1-notepad-live.json`、`v0.6.0-r1-notepad-module.json`、`v0.6.0-keyboard-preserved.json`、`work/build-logs/v0.6.0-r1-test-counts.json`。

本次只验收有限记事本场景，未覆盖全部应用、x86编辑器真实键入、本版AI联机/GUI、卸载或重启清理。最终源码ZIP的检查结果见配套交付收据；未从ZIP完整原生重编。候选排名只是本次观察快照。

## 沿用0.5的关系词库与历史验收

**0.5.0 R1正式检查点：** 构建通过413次Rust、46次Python和619次PowerShell检查，共1,078次通过结果，含x86重复，不是唯一测试总数。Rust为0失败、原始汇总7次忽略；随后显式运行的真实准备资源集成已计入413次通过。安装后x64/x86激活探针各2项通过、S_OK，4项另计。升级退出0，目录 `C:\Program Files\EnglishAssist\versions\0.5.0-20261109121445-346651` 的25项产物摘要及build.json全部匹配冻结构建。下方0.4.0及更早结果仍为历史证据。

- 拼音词库保留“中文词条—带声调读音—原始义项—英文词”的关系证据，按中文词/读音用频、关系质量等排序，不再只按英文词频把常见英文释义推到前面。
- `拼音 · 想` 表示精确拼音键命中，`扩展 · 想法` 表示通过前缀找到更长的拼音键。匹配汉字与英文词性、中文释义分别展示；同音字仍可能有多解，不等于理解了整句语义。
- 同时开启“拼音查英文”和“连写智能分词”后，`iwilldotheyiyang`、`iwilldothetongyang` 可给出尾词 `same`，明确确认后一次提交完整 `I will do the same`。候选主行只显示尾词，英文前缀不重复；未完成拼音沿用有界前缀查找。
- 识别到拼音解释时，直接按Space保留原串，不把拼音尾词误拆成英文字，也不自动翻译；用Tab、数字、点击，或上下键明确选中后再按Space，才接受混合候选。Enter仍原样提交，包括已经导航过候选的情况；不调用云端，也不自动学习输入历史。
- 新增51条已审核现代实体的有限种子目录，提供Anthropic等公司/产品的规范拼写和独立实体说明，保留Wikidata固定修订及官网身份来源。大写开头优先名字，常见小写英文保留普通词义，并可另列实体含义；这不是完整知识库或自动更新的时事词典。

双语含义来源仍是固定ECDICT版本和 **2017-11-17 CC-CEDICT快照**，本次重建的是关系与排序，未悄悄换成最新词库。中文排序使用固定Wanxiang v18.0.14字/词频，具体独立来源和许可见下文。

真实记事本PID58000已确认加载最终R1 DLL：两个混合样例首项均为same，分别按1和Tab后完整提交 `I will do the same` 一次；xiang首屏显示think/想、mutually/相、to/向、like/像、item/项，翻页可见fragrant/香、elephant/象，Enter仍原样提交xiang。Anthropic公司及Claude开发商说明与普通形容词义并存，按1仅提交规范名字Anthropic。iamrobot直接Space得到 `I am robot`；iamnihao虽出现hello/你好候选，未选词的Space仍保留iamnihao。自有保存文件精确匹配六行预期，自动空格保持ON，原键盘及默认设置保留。收据为 `work/research/v0.5.0-r1-notepad-live.json`、`work/build-logs/v0.5.0-r1-test-counts.json`。

这些是有限的记事本检查，不代表全部应用；本版没有重新验收AI、x86编辑器真实键入、卸载或重启删除，也未验证从源码ZIP完整重建原生产物。候选顺序是本次快照，不保证固定排名。

## 沿用的0.4规则与历史验收

**最终R5正式构建与安装检查点：** 24套Rust汇总396通过、0失败、6忽略（含x86重复），Python33通过、PowerShell602项通过，正式流水合计1,031次通过结果。安装后x64/x86探针各2项通过，进程内激活均S_OK；这4项独立记录，不加进正式合计。升级退出0，安装目录为 `C:\Program Files\EnglishAssist\versions\0.4.0-20261109110234-275066`，24项安装文件摘要及build.json匹配冻结构建。计数收据：`work/build-logs/v0.4.0-r5-test-counts.json`。

冷启动记事本PID57228已确认加载最终DLL，并独立复验：iamrobot按Space得到 `I am robot` 且无尾空格；分别输入i+Space、am+Space、jiqiren按1得到 `i am robot`；iamrobot先Down再Enter仍原样得到 `iamrobot`，不插入换行。保存文件精确为上述三行；Shift将共享空格值置OFF再置ON。状态条拖动后坐标实际保存，×可隐藏，最终设置程序可重新显示并恢复默认位置。这是本次记事本与状态条受控验证，不代表全部应用、AI或卸载均已通过。

本轮新增不抢焦点的悬浮输入状态条：英文输入框聚焦时，即使没有候选列表也显示大小写、自动空格、补词、拼音和连写状态，可点击开关、打开设置或隐藏。拖动底部可移动；“输入与查词”可重新显示或恢复默认位置。功能开关保留当前组字，结束组字后自动同步，从下一词生效，无需重新切换焦点。大小写读取实时 Caps Lock 状态，Shift 临时大写不会误显示为 Caps Lock 开启；点击大小写只在原输入框仍有焦点、没有按住修饰键时切换。

Windows 默认开启自动词前空格。分别确认 `i`、`am`、`robot` 时，只在新词前按需补空格，光标紧贴 `robot`，不会自动添加尾空格；拼音选词同样处理。开启连写分词时，直接按 Space 可将 `iamrobot` 还原为 `I am robot`，只补已输入词的内部空格，不补未输入单词。单按 Shift 切换自动空格；Shift 配字母、`_`、`+` 保留原用途。关闭后用 Space 手动隔词。Enter 原样提交当前组字，不选候选、不补空格；没有组字后再次 Enter 交给应用。Esc 保留原始串。本地空格处理不调用AI，也不新增输入历史学习。

内部补空格采取保守判断，最多处理64个已输入字符；未完成或判断不明确时保留原串。要把iamt补为trying等未完整输入的词，使用Tab、数字或显式选中候选确认。

输入DWORD偏好改用 `%USERPROFILE%\.EnglishAssist\input-preferences-v1.bin` 的固定共享映射，让打包编辑器与独立设置程序读写同一组开关。文件仅保存偏好数值，不含API Key、输入正文或历史。安装初始化会迁移旧HKCU输入DWORD，之后以共享文件为准；个人词库路径等字符串仍保留注册表表示。共享数据不可用或格式不符时拒绝使用，设置明确显示错误，不会假称已保存到其他应用可见的配置。

**历史0.4 R3双向同步检查点：** 真实记事本PID56416已核对R3 DLL。单按Shift后，共享空格开关变OFF，设置GUI显示OFF；在设置点ON后，同一开关变ON，记事本随后将 `iamrobot` + Space 保存为 `I am robot`，无尾空格。收据为 `work/research/v0.4.0-r3-bidirectional-live.json`。这仅证明当时的打包记事本与设置程序，不承诺所有AppContainer均具备共享文件访问权限。R4的长串、Caps Lock和其他功能开关操作保留为R4证据，未全部搬作R5复测。最终R5实测见上方；源码包不代替原生运行验收，旧AI与生命周期结果保留版本边界。

**历史 0.3.1 最终检查点：正式构建退出0，24套Rust汇总368通过、0失败、6忽略（含x86重复），Python33通过、PowerShell508项通过；安装后x64/x86探针各2项通过，激活S_OK。** 升级退出0，目录 `0.3.1-20261109070929-873493` 的24项产物摘要及build.json均匹配。可靠多词前缀后允许有限词频回退，避免相邻词表缺项时漏掉 `gay`、`good` 等尾词。候选顶部显示当前选择推断的词界，例如 `iamg` → `I am · g`；行内仍只显示尾词，确认仍完整提交当前组合一次。

用户反馈的 `iamg` 截图来自记事本PID39900，该进程在系统已安装0.3后仍加载0.2 R5 DLL，并非两个开关失效。新窗口可能复用旧进程；升级后应先保存工作、完全退出应用，再重新打开。本次退出全部旧记事本后，冷启动PID46068已确认加载0.3.1 DLL：iamg顶部显示 `I am · g`、首项going；iamga按3、iamgood按Tab、iamt按4后，保存文件精确为三行 `I am gay`、`I am good`、`I am trying to`。其他恢复标签未读取。

候选排名是本次快照，不是保证；本地解码尚不具备完整语法或神经语言模型句意理解，字符不同的 `imag` 仍走普通单词路径。本轮未重新调用AI HTTP或验收AI GUI，0.3 CLI和0.2 R5 GUI结果保留各自版本边界，具体收据见验证记录。

**历史 0.3 最终 R2 检查点：正式构建 Rust 364通过、0失败、6忽略（含x86重复），Python33通过、PowerShell508项通过；安装后x64/x86探针各2项通过，激活S_OK。** 当时目录 `0.3.0-20261109063014-912375` 的24项产物摘要及build.json匹配冻结清单。加载最终DLL的私有RichEdit宿主通过选词、退格、Esc和翻页，文档文本精确为 `I am trying to iambic apple iamt helped`。

两个开关在前一个0.3设置程序中完成off/on并读取HKCU确认保存；最终设置exe摘要不同，另行确认输入页两项均on，未重复保存测试，也不代表全部控件通过。0.3 production CLI真实DeepSeek请求在升级后成功，凭据保留。本轮未重新做全部应用、x86编辑器键入、Notepad AI GUI、卸载或重启删除验收；下方0.2 R5结果仍为历史。

## 0.3 连写英文

- 本地同时保留整体单词前缀和多条词界解释，允许末尾单词尚未输入完整。
- `ia` 可同时提供 `ia…` 开头单词和 `I am`；`iamt` 的行内候选可只显示 `trying`、`trying to`，确认时一次替换为完整 `I am trying`、`I am trying to`。
- 推断前缀留在当前组合内，不提前上屏；Esc 保留原始串，0.4的Space按上方自动空格规则处理。
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
- 独立现代词义与扩展实体别名，保留用途/来源身份；缩写全称只说明、不自动展开。
- Tab 和鼠标选词不加尾空格，自动空格开启时按需补词前空格。
- 自动空格只补需要的词界，不添加尾空格。
- Enter 原样提交当前组字，即使曾导航候选也不选词；没有组字时交给应用。
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

0.6.0 R1安装包为 `dist/english-assist-0.6.0-windows-x64-setup.exe`，正式构建、安装摘要、激活探针及有限记事本检查见上方；旧版生命周期结果保留历史边界，本版未重新卸载或验收重启清理。不要使用继承的 Retype 安装器、register.ps1
或上游发布页安装包来替代本项目构建。

安装后的验收目标是在“设置 → 时间和语言 → 语言和区域 → 英语（美国）
→ 语言选项 → 键盘”中看到本项目，用 Win+Space 选择，且保留原默认键盘。
验证记录区分当前构建与历史双向同步检查点；旧版记事本、AI及生命周期证据分别保留，其他应用与重启删除仍有未覆盖项。

## 键盘操作

1–5 接受当前页候选，减加号翻页，Tab 接受候选且不补尾空格。候选可见时要输入
python3 等原始串，可先 Esc 结束组字再输入数字。上下键显式选中后，
空格接受该候选。自动空格开启时确认不补尾空格，连写开启时可还原已输入词的内部空格；识别到拼音解释但未明确选词时，Space保留原串，避免误拆拼音。关闭自动空格后用Space手动隔词。单按Shift切换该设置。

当前有组字时Enter原样提交并消费该次回车，不选候选、不补空格；没有组字后再次Enter才交给应用。Esc保留原输入。

完整语义和待验收项目见 [英文输入](docs/english-input.md)
和 [P0 验证记录](docs/english-assist/INPUT_VALIDATION.md)。

## 许可与来源

继承的代码使用 MIT，保留 LICENSE、NOTICE.txt 和 NOTICE-English-Assist.txt。
英文数据另有 SymSpell、SCOWL 和 Google Ngram 的许可及署名，
位于 data/dict/raw/english/；词义及拼音数据保留 ECDICT MIT 和 CC-CEDICT CC BY-SA 3.0
署名，组合元数据文件按 CC BY-SA 3.0 分发，代码仍为 MIT。

当前双语源具体为ECDICT提交 `bc015ed2e24a7abef49fc6dbbb7fe32c1dadaf8b`（MIT），以及HallelujahIM提交 `8be294f70a4dc7b24a93f07c41df6c34add07a07` 镜像的2017-11-17 CC-CEDICT（原文件头CC BY-SA 3.0）。[MDBG当前下载页](https://www.mdbg.net/chinese/dictionary?page=cc-cedict)已列最新发布采用CC BY-SA 4.0，与本包实际冻结源分别记录。Wanxiang v18.0.14的 `zi.dict.yaml`、`jichu.dict.yaml` 作为中文排序证据，保留其CC BY 4.0及精确哈希，见 `data/dict/raw/english-assist/SOURCES.json`、`data/dict/raw/wanxiang-base/SOURCE.md`。现代实体另保留CC0 Wikidata快照/修订和审核官网链接，见 `data/dict/modern-entities.json`，不混为双语词典许可。

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
0.5另须保留冻结Wanxiang字/词频、关系词库v2编译器及测试、现代实体JSON/TSV、原始Wikidata投影快照、来源许可和离线校验工具。源码包不得纳入凭据或个人输入文件。

0.6现代词义子集及项目中文补充按CC BY-SA 4.0独立分发，保留维基词典贡献者署名、逐义来源URL、冻结哈希和改动说明；Wiktextract的MIT软件许可不替代数据许可。配套源码须保留 `data/dict/modern-lexicon*`、`data/dict/raw/kaikki-modern/` 的冻结投影、两版来源清单、完整许可/NOTICE及独立编译/校验工具；现代实体继续单列CC0来源。正常构建不刷新源，大型完整dump位于单独获取缓存，不能把随附投影说成全量词典。

源码目录保留上游中文、其他平台与同步模块，用于后续整理和必要兼容；
这不代表英文专用产品会安装或提供这些功能。
