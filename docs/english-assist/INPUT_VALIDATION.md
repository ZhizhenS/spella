# English Assist：交互与验证记录

更新：2026-11-09。目标版本：0.2.0。上游基线：Retype `902e35e06ee4f744320cf995c39b1f64693034cf`。

本文区分本子任务执行的定向检查和根任务负责的最终集成、安装及真实宿主验收。产品范围见 [产品规格](../english-assist-product.md)，操作说明见 [英文输入](../english-input.md)。最终安装包与宿主兼容结论以根任务的版本化构建记录为准。

## 2026-11-09 集成交付检查点

此节记录 final-r2 原生构建与 r3 脚本刷新后的安装→卸载→重装证据。r3 只重打包脚本，所有原生文件仍为 final-r2 冻结的相同字节；应用验证按实际执行版本分别记录。

| 项目 | 当前结果及边界 |
| --- | --- |
| 正式 schema-2 构建 final-r2 | `work/build-logs/v0.2.0-final-r2-build.log`，构建退出 0，安装包生成完成；24 条 Rust summary 合计 336 通过、0 失败、6 忽略，包含 x86 重复执行，不是 336 个唯一用例；Python 14+5=19 通过；PowerShell 各次检查合计 446 项 |
| 正式产物安装 r2 | `0.2.0-20261109023323-821500` 安装退出 0，24 项文件 SHA-256、大小及 build.json 与冻结清单一致；自有 Run 不存在、Uninstalling=0。日志 `work/build-logs/v0.2.0-final-r2-install.log` |
| r3 脚本重打包 | 仅 RefreshScripts，不重编译或替换原生文件；日志 `work/build-logs/v0.2.0-final-r3-repack.log`。x64/x86 各30项 cleanup、100项 pending 策略测试通过，单列证据，不合并到旧 r2 的446项 PowerShell 结果 |
| r3 安装→卸载→重装 | 首次安装 `0.2.0-20261109024712-326271` 退出 0，卸载退出 0，再次重装退出 0。最终 ActiveDir=`C:\Program Files\EnglishAssist\versions\0.2.0-20261109024807-741245`；24 项文件 SHA-256、大小及 build.json 全部匹配，LearningAutorun=false、Uninstalling=0。日志分别为 `work/build-logs/v0.2.0-final-r3-install.log`、`v0.2.0-final-r3-uninstall.log`、`v0.2.0-final-r3-reinstall.log` |
| r3 卸载状态快照 | 卸载后 HKLM/HKCU 产品键、HKCU 自有 CTF TIP 缓存、产品 AppData 全部不存在；机器 COM/Profile 及 HKCU 自有 Profile 的32/64视图均无，搜狗和 US 语言列表保留。此快照在最终重新安装之前读取，不与重装后的有效登记混淆 |
| 独立 NativeRichEdit desktop_host | 真实 UI 三场景：`hello → 4 → hello`；`pingguo → 1 → apple`；`hel → - / + / = 翻页 → 第二页 3 → helped`。证明该独立测试宿主的本地候选、数字选择、拼音及翻页，不扩大为全部应用兼容或新冻结包全部通过 |
| Windows 11 新记事本进程 | PID 40620 实际加载 r2 DLL；h/e/hel 的词性与中文正常；`hel + 4 → hello`、`pingguo + 1 → apple`；hel 下 Shift+equals 到 2/6 页、Shift+minus 回 1/6、equals 到 2/6，按 3 得 helped；自有测试文档实际保存 `hello apple helped`，未混入释义/编号。收据 `work/research/20261109-notepad-final-r2.json`。只代表该真实 Notepad 场景，不扩为全部应用兼容 |
| r2 卸载与状态核对 | DLL 仍被宿主加载时卸载器退出 0。品牌 HKLM/HKCU 与产品 AppData 均移除；机器 COM/Profile 在32/64视图均移除；自有 SortOrder 无项，搜狗和 US 语言列表保持完整。日志 `work/build-logs/v0.2.0-final-r2-uninstall.log`；当时的缓存/重装问题及后续处理见历史段 |
| 最终安装探针 | 最终安装的 x64/x86 各 2 项发现、加载和进程内激活检查通过，激活 HRESULT 均为 S_OK；日志 `work/build-logs/v0.2.0-installed-x64-final.log`、`v0.2.0-installed-x86-final.log`。这不替代 x86 编辑器的真实键入测试 |
| 设置截图/操作 | 设置套件 20 通过/1 忽略；最终 installed exe 的常规、输入、外观、词库、AI 五页已重新截图并逐页检查布局，见 `work/research/settings-cli-0.2-20261109-024903/manifest.json`。截图不代表所有控件逐个完成真实点击验收，跨宿主手工词库与控件效果仍按下方矩阵区分 |
| DeepSeek 外部服务 | 未配置新 Key、未调用外部 API；已有选区、预览、确认和过期结果回归不代表真实服务、输出质量或延迟通过 |

**当前生命周期门槛：安装→卸载→重装已完成。** 原有 Windows pending 队列没有修改，8项均为已知旧 DLL 删除或经文件属性核验的已知目录空删：3个旧 DLL、3个旧 payload 目录、versions 和产品根目录。新目录独立分配，排队旧文件的实际删除尚未重启核验；不把排队成功写为删除已经完成。

r3 重装前检查支持实测的精确 `*1\??\` NT 路径形式，仍校验产品范围、旧版本关系、目录属性与重解析限制。`*1` 在此仅是实测队列格式，不能解释为公开 API flags。目录重启删除只在目录为空时发生的依据是 [Microsoft MoveFileExW 文档](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw)；该文档不用于解释此实测前缀。

r2 已包含本包资源优先加载、生产输入 Noop 学习、无学习服务登录自启及 broker 登记/卸载状态门。缺失或错误的登记、Uninstalling 非显式 DWORD0 时 broker 拒绝生产启动；测试命令的隔离数据库例外不代表实际用户路径。原生文件证据沿用 final-r2；r3 新脚本由其独立策略测试及实际安装/卸载/重装结果覆盖，不借用 r2 测试数量。r2 记事本三场景仍有效地限定到这些相同原生字节，不称为最终重新安装后已逐应用全测。

真正 AppContainer 中，AI helper 未预先运行时不支持冷启动 AI，从候选菜单启动设置也受限制；开始菜单“英文助手设置”可用。此边界不适用于所有打包应用，不能只凭 packaged 身份认定 AppContainer。DeepSeek 未配置新 Key、未外网验收的状态不变。

## 当前键盘约定

用户在 0.2.0 开发中明确改为 Hallelujah 风格数字选词，取代早期“所有数字原样输入”的方案。

| 操作 | 实际约定 |
| --- | --- |
| 输入英文 | 本地前缀补全；后台拼写建议不自动替换正文 |
| 输入拼音 | 默认查对应英文，例如 nihao、pingguo；设置可关闭 |
| 有候选时 1–5 | 选择当前页对应行，仅提交英文，无编号、释义或尾空格 |
| 0、6–9，或当前页没有对应行 | 保留原输入并按原样输入数字，不选择隐藏候选 |
| 无候选时数字 | 原样输入 |
| `-` / `_` | 有候选时上一页；无候选时保留符号用途 |
| `=` / `+` | 有候选时下一页；无候选时保留符号用途 |
| PageUp / PageDown | 有候选时翻页；每页固定五条，末页可少于五条 |
| Tab / 点击 | 接受当前候选，不附加空格 |
| ↑ / ↓ | 显式选择候选 |
| 空格 | 未显式选择时原词加空格；显式选择后所选词加空格 |
| Enter | 显式选择后仅确认候选；再次 Enter 交给宿主；未选择时保留原词并交给宿主 |
| Esc | 保留原词并关闭候选；要输入 python3 等原始串，可先 Esc 再输入数字 |
| 无候选 Tab / Shift+Tab | 保留宿主焦点导航 |

UI-less `ShowCandidateNumbers` 返回 TRUE；展示字符串可以含注释，Finalize 必须提交原 Candidate.text。普通富文本控件不因缺少 `TS_SS_NOHIDDENTEXT` 被阻断；实际密码、PIN 和私密输入范围仍通过编辑会话安全门排除。

## 设置与存储

七个页面：常规、输入与查词、候选外观、快捷键、个人词库与数据、AI 服务与编辑、关于。没有自动学习开关；生产输入使用 Noop 学习存储，不自动记录个人词条、不因输入启动学习 broker，安装不添加其登录自启。显式手工词库单独保存与加载。

以下 DWORD 位于 `HKCU\Software\EnglishAssist`，返回输入框时读取输入偏好；候选外观通过运行时短缓存刷新。

| 设置 | 注册值 | 缺省 |
| --- | --- | --- |
| 英文补全 | EnglishDisabled（1=关闭） | 开启 |
| 拼写建议 | EnglishSpellingDisabled（1=关闭） | 开启 |
| 拼音查英文 | EnglishPinyinDisabled（1=关闭） | 开启 |
| 符号配对 | SymbolCompletionEnabled（1=开启） | 关闭 |
| 候选主题 | CandidateTheme（0系统/1浅色/2深色） | 0 |
| 字号 / 宽度 | CandidateFontSize / CandidateWidth | 17 / 420，范围14–22 / 320–460 |
| 紧凑模式 | CandidateCompact（1=开启） | 关闭 |
| 中文释义 / 词性 | CandidateHideMeanings / CandidateHidePartOfSpeech（1=隐藏） | 显示 |
| 音标 | CandidateShowPhonetics（1=显示） | 隐藏 |
| 操作提示 | CandidateHideKeyHints（1=隐藏） | 显示 |

手工词库：`%LOCALAPPDATA%\EnglishAssist\words\manual.json`。支持显式新增、编辑、删除、清空，以及 JSON 或 UTF-8 TXT/TSV 导入、JSON 导出；同名单词不区分大小写。文件最多 1 MiB、10,000 条，单词最多 64 个英文字母及内部撇号，注释最多 256 字且不允许控制字符。导入冲突以导入条目更新，合并后仍校验上限。

保存先备份 `manual.json.bak`，同目录临时文件写入及同步后替换。文件或注册代次发布失败会恢复旧文件，界面保留旧列表并显示错误。`ManualWordsPath` 是绝对路径，`ManualWordsGeneration` 提示其他宿主在聚焦时后台读取；按键查词不读文件。旧 worker 不能覆盖新快照，读取或校验失败保留旧快照并允许下次聚焦重试。

2026-11-08 只读 ACL 检查发现当时产品根目录没有通用 AppContainer 读取继承，words 尚不存在。安装初始化负责只给 words 子目录及平面词库/备份文件授予应用包只读/目录遍历权限，拒绝重解析链接；不对整个 AppData 放权。因此显式词条和备份对本机沙箱应用可读，界面说明此边界。该安装脚本由安装子任务维护，本子任务未执行真实 ACL 初始化。

## 已执行的定向验证

本子任务使用 MSVC/SDK 环境，执行命令前载入 `tools/english-assist/enter-env.ps1`。测试夹具只使用独立临时文件或测试注册表项，不修改生产输入偏好。

| 检查 | 本子任务结果 |
| --- | --- |
| `cargo check --locked -p retype-settings-egui -p retype-tsf` | 通过 |
| `cargo test --release --locked -p retype-engine --test english` | 14 通过，含数字选词、固定分页、透传、无尾空格、Enter、禁自动学习、拼音偏好刷新及过期结果丢弃 |
| `cargo test --release --locked -p retype-engine --lib` | 键盘改动阶段 66 通过；后续跨模块整体验证由根任务执行 |
| `cargo test --release --locked -p retype-tsf --lib real_tsf_composition_commit_cancel_and_passthrough -- --test-threads=1` | 键盘改动阶段 1 通过，含真实 TSF 数字/翻页与带释义 UI-less Finalize 只提交英文 |
| `cargo test --release --locked -p retype-tsf --lib manual_ -- --test-threads=1` | 2 通过：文件格式及 publication 串行化/过期结果拒绝 |
| `cargo test --release --locked -p retype-settings-egui --bin retype-settings-egui` | 20 通过、1 下载测试忽略；含备份/清空/失败回滚、导入导出、坏文件拒绝和外观部分写失败恢复 |
| 负责文件 rustfmt | 已执行；最终全仓库 fmt 由根任务复核 |
| 设置 GUI 截图与点击、真实手工词库跨宿主加载 | 本子任务未执行，根任务验收 |
| 0.2.0 安装升级、x86 全套、AI 实际请求、真实宿主兼容矩阵 | 此表为先前子任务范围；11/09 根任务已完成的构建/安装与仍待处理项见本文顶部检查点 |

设置截图参数：`--page=general|input|appearance|shortcuts|dictionary|providers|about --screenshot=<绝对PNG路径> --exit-after-ms=2500`。调试二进制名为 `retype-settings-egui`；安装后文件名由专用打包脚本确定。

## 真实产品 P0 待办

下表是最终验收清单，不将单测通过视为宿主兼容通过。

| 验收 | 必须核对 |
| --- | --- |
| 系统入口 | 英语（美国）下独立入口；Win+Space 真正激活；保留其他键盘与默认选择 |
| 富文本 / 记事本 / Chromium / VS Code / x86 | 分别记录切换、组字、1–5、Tab、翻页、Enter、失焦；Chromium textarea/contenteditable 分开 |
| 英汉词典 | hel 显示释义/POS；字段不进正文；nihao/hello、pingguo/apple、xuexi/study；关闭拼音后不提供拼音结果 |
| 外观 | 深浅/系统、字号14/22、宽度320/460、紧凑、字段显隐；当前候选刷新、点击区域与编号一致 |
| 手工词库 | 新增→聚焦后候选出现；编辑注释更新；删除后消失；导入合并/导出回读；坏文件与发布失败旧数据不丢；AppContainer 可读仅 words |
| 原样与私密 | 无候选数字、符号、Tab、快捷键、撤销；密码/PIN/private 不出候选；不丢字、不重复 |
| 冷启动与资源失败 | 离线启动；未加载时可原样输入，加载后下一词可补全；坏文件不崩溃 |
| AI 编辑 | 主动选区触发、先预览后确认；焦点/选区变化和 A→B→A 旧结果零写入；真实服务单独记录 |
| 安装升级卸载 | 新版本 DLL/资源 hash、无重复 Profile、用户词库保留策略、旧 DLL 占用路径与重启清理明确 |

最终记录应包含源码状态、构建命令及退出码、包版本/hash、应用版本、动作、实际提交文本和剩余问题。未测应用不得写为通过。

## 源码包与数据许可

构建需要 Python 3.11+，无第三方 Python 包；可用 `-PythonPath` 或 `ENGLISH_ASSIST_PYTHON` 指定。组合元数据文件按 CC BY-SA 3.0 分发，保留独立 ECDICT MIT 及词频来源署名；代码仍使用 MIT。

源码 ZIP 应包含当前修改和新增的源码及 Cargo.toml/Cargo.lock，不能只归档旧提交的已跟踪文件而漏掉工作树新增内容。至少核对：

- `data/dict/raw/english-assist/` 全目录：`ecdict.csv.gz`、`cedict.txt.gz`、`SOURCES.json`、`LICENSE-ECDICT`、`LICENSE-CC-CEDICT`、`NOTICE-English-Lexicon.txt`、README。
- `data/dict/raw/english/` 的 frequency.txt、SOURCE 和 SymSpell/SCOWL/Google Ngram 完整许可。
- `data/dict/english-lexicon.json`、`data/dict/resources.json`、顶层 LICENSE、NOTICE.txt、NOTICE-English-Assist.txt。
- `tools/english-assist/` 下的数据编译、准备、验证脚本及测试，以及该版本构建脚本、产品操作和验证说明。

完成 ZIP 后应记录实际摘要并检查归档条目；本次文档更新不声明源码 ZIP 已完成。不得将凭据或用户输入文件纳入源码包。

## 历史边界

- 2026-11-07：初始源码阶段缺少 link.exe，首次 cargo test 在链接阶段失败，没有运行测试；当时采用数字全部原样输入、无编号候选方案。
- 随后安装 MSVC/SDK，修复测试 fixture 对旧 Shift 默认绑定的依赖；生产 Shift 默认保持禁用。普通富文本候选路径和私密范围分别建立回归。
- 2026-11-08：用户明确要求 1–5 选词、减加号翻页，当前交互表取代初始数字方案；新增候选释义、拼音、外观和手工词库设置。根任务负责 0.2.0 最终构建、安装和实际应用验证。
- 2026-11-09 早期检查点：旧 final-build 合计 318 Rust 通过/6忽略、Python19、脚本374；当时安装 DLL 为87DAFFFA前缀。旧9431进程中的记事本数字选词可提交hello，但释义失败，新窗口仍复用旧进程；早期卸载1→直接cleanup0→retry1。以上属于历史问题和旧构建，当前证据已更新为顶部 final-r2 原生加 r3 脚本，不将旧进程结果混入新包验收。
- r2 历史待办曾有 HKCU CTF TIP Enable=0 缓存及8项旧文件/目录排队删除，重装尚未完成。r3 已清除自有缓存、核验旧路径并完成再次重装，pending 队列本身未修改；仍未执行实际重启删除核验。
