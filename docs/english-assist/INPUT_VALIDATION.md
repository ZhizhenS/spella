# English Assist：交互与验证记录

更新：2026-11-09。0.5.0 R1已完成正式构建、升级、产物核对和有限真实记事本检查。上游基线：Retype `902e35e06ee4f744320cf995c39b1f64693034cf`。

本文区分本子任务执行的定向检查和根任务负责的最终集成、安装及真实宿主验收。产品范围见 [产品规格](../english-assist-product.md)，操作说明见 [英文输入](../english-input.md)。最终安装包与宿主兼容结论以根任务的版本化构建记录为准。

## 0.5.0 R1 关系词库、混合拼音尾词与现代实体

数据契约和维护方式见 [词典 v2](LEXICON_V2.md)。本节记录自动检查、正式安装及真实应用的不同范围，旧版本结果保留在后面。

| 0.5.0 项目 | 实际结果 / 验收边界 |
| --- | --- |
| 改动前备份 | `work/backups/20261109-before-0.4.1-mixed-pinyin` 保存0.4.0源码提交、安装产物/安装包、产品配置、共享偏好值和Notepad LocalState；目录名来自最初小版本计划。冷启动前另备份LocalState。不读取恢复标签或凭据内容 |
| 实际旧问题 | 旧版 `xiang` 前五项to/help/support/high/school来自向、襄、庠等精确同音字，不是长拼音扩展污染。旧关系只留英文ID且按英文频率重排，丢失中文来源；fragrant还被每键64项编译截断删除。相应旧截图不能解释为仅第一项进行了拼音匹配 |
| 来源 | ECDICT固定`bc015ed…`/MIT，CC-CEDICT是Hallelujah固定镜像的2017 MDBG快照/CC BY-SA3.0；万象v18.0.14字/词与读音频率/CC BY4.0。现代实体另用固定Wikidata修订和官网身份材料/CC0。没有声称采用最新CC-CEDICT全量、Google输入法模型或全量Wikidata |
| v2数据 | RTENLX02：83,367个英文词、37,668个拼音键、121,350条关系，15,098,862字节；SHA256=`cca750ec8fbd1b3366038f3a096fc48d4e2843a4a1bf5265507e6b868e7e933c`。每关系保留汉字、带调读音、原义项、中文用频、来源/用法及义项序；普通构建必须与审核清单严格一致 |
| 关系和排序 | CC-CEDICT整词/受限系词归一保原gloss，ECDICT只用完整中文释义项反查；有歧义的多音反查与有明确lemma的屈折扩散受限。中文词和具体读音频率、主次/用法、常用词项多样性在英文频率之前；合并来源时保留最佳义项自己的用法和序号，不把另一古义标签混到现代义上 |
| 有界前缀 | worker加载时建立只读中文频率范围树，非精确键有界选择64个高优先级键，单键最多64关系、总扫描512关系、最多64个英文去重结果。精确键保留编译预排、总扫描512，不能再按全局英文频率重排。主补词通道取24项，异步拼写可追加至内核32项；因此数据中存在的低频关系未必出现在可见页 |
| 罕见义项 | 离线全键审计仍找到school←庠，去重后第66位，低于think/like/fragrant/elephant；它在该次前64个返回项之外，不宣称GUI后页一定可见。学校拼音的正常school关系独立存在。`work/research/v0.5.0-lexicon-quality.json` |
| 英文＋拼音 | `iwilldotheyiyang`、`iwilldothetongyang`同时受拼音与连写开关控制，候选显示same，正文完整替换为I will do the same一次；中文来源、词性和释义只展示。可靠英文前缀、普通整词/个人词保护、回退预算和同分稳定排序回归均覆盖 |
| 原文确认 | 数字/Tab/点击或明确导航后的Space接受译词；有拼音解释但未明确选词时，ON的Space保留原串，避免偶然拆成they I yang。Enter即使导航/翻页后仍raw，不换行；无组字的下一次Enter交还应用。普通iamrobot自动内部空格与无尾空格语义保留 |
| 现代实体 | 51条经审核的公司/产品/开发工具目录，固定QID、修订和官网链接。Anthropic公司/Claude开发商与普通anthropic形容词独立显示；Apple水果、Apple公司及显式个人词优先级/容量均有回归。目录校验及6个隔离变异测试是离线检查，不在输入时联网或自动扩充 |
| 定向检查 | English crate39通过/1忽略；独立真实准备词库的engine集成1通过，覆盖两例same、完整数字提交、raw Enter/Space、明确导航Space、开关、xiang证据和Anthropic双义。该1项也在正式流水显式执行，不与正式统计重复相加 |
| 正式构建 | `work/build-logs/v0.5.0-r1-full-build.log`退出0，ISCC成功。Rust26条汇总413通过（常规412＋显式准备词库1）、Python46、PowerShell619，正式合计1,078次通过；包含架构重复，不是唯一用例数。计数收据 `v0.5.0-r1-test-counts.json` |
| 忽略项与安装探针 | 常规日志原始7次忽略中，准备词库1项随后在正式流水执行；安装后x64/x86各2项另行通过、HRESULT均S_OK，FORPROCESS作用域恢复、不改默认键盘，4项不加进1,078。还有2项未执行，见计数收据IgnoredAccounting；不把忽略项统一称为已测 |
| 升级与冻结 | 升级退出0；ActiveDir=`C:\Program Files\EnglishAssist\versions\0.5.0-20261109121445-346651`。25项文件及build.json全部匹配生产冻结清单，已映射共享偏好File ID和全部槽值保持；现代目录审核JSON/TSV摘要及51条数量写入build.json |
| 宿主身份 | 冷启动Notepad PID58000确认加载本次DLL，SHA256=`4f5a738d8e9647e884f333746380c7bdbe24006604d897f3677e75a1d7b8bd3c`；模块收据 `work/research/v0.5.0-r1-notepad-module.json`。用自有公开测试文件验证，不读取其他恢复标签 |
| 两例真实提交 | 逐键输入yiyang示例，首项same/拼音·一样，按1；tongyang示例首项same/拼音·同样，按Tab。每阶段Ctrl+S后读回，两行均精确I will do the same，无重复前缀/中文注释/尾空格；分别留有裁剪候选PNG |
| xiang真实候选 | 第一页think想、mutually相、to向、like像、item项；Shift+equal翻至第二页，见fragrant香、ring响、elephant象、enjoy享、township乡。翻页组合键不切换自动空格；翻页后Enter保存raw xiang、无换行。候选位置仅为本次快照 |
| 实体真实显示 | anthropic首项Anthropic/专名/现代·公司，释义美国人工智能公司、Claude开发商；第二项anthropic/adj./人类的。按1仅提交规范大小写Anthropic，未提交释义或徽标。`v0.5.0-r1-anthropic-live.png` |
| 其他真实回归 | iamrobot按Space精确I am robot、无尾空格；iamnihao显示hello/拼音·你好后，未选词Space精确保留iamnihao，不译成hello、不插内部空格。共享偏好自动空格仍ON |
| 保存结果和范围 | 自有文件 `work/ui-tests/notepad-v050-e22cdb54.txt` 精确六行 `I will do the same\nI will do the same\nxiang\nAnthropic\nI am robot\niamnihao`，无最终换行/尾空格。收据 `work/research/v0.5.0-r1-notepad-live.json`；语言列表、InputMethodTips、默认override前后相同，见 `v0.5.0-keyboard-preserved.json` |
| 未宣称通过 | 全部应用、x86编辑器真实键入、AI联机/AI GUI、卸载/重启清理、全部长尾翻译质量以及从源码ZIP重做整套原生编译。本轮不读密钥、不请求外部AI；老应用应完全退出后重开以加载新DLL。源码归档完整性与离线重建另由交付收据记录 |

## 历史0.4.0 词前空格、原文回车与状态条

用户要求：确认新词时只补词前空格，不自动补尾空格；连写串如 `iamrobot` 按 Space 可补齐内部词界，单按 Shift 切换；回车始终原文上屏。关闭自动空格后，物理 Space 保留手动空格的传统语义。

新版改动前的源码、已装产物、产品配置和Notepad LocalState已备份至 `work/backups/20261109-090521-before-0.4.0`；每次需要替换宿主进程前另备份Notepad LocalState。不复制或读取凭据管理器里的密钥，不打开恢复的其他标签页。

| 0.4.0 项目 | 实际结果 / 验收边界 |
| --- | --- |
| 确认空格 | ON时仅在需要的相邻英文之间补词前空格，不加尾空格。ON的未导航Space先尝试完整原串分词；Tab/数字/点击/明确导航后Space仍可接受完整候选。OFF的Space保留传统物理手动空格，Tab/数字/点击不补前后空格 |
| 精确原串分词 | 独立于候选预测的完整词路径，仅增加内部空格（独立i可规范为I），最多64 ASCII字符、16词、beam32，完整词/个人词保护、实际观察词对锚点与歧义拒绝。`iamt`不会因预测候选而自动补成`I am trying to`；不宣称任意连写串都能唯一或正确拆分 |
| Shift与队列 | 单按Shift切换自动空格，组合键保持原义。异步偏好编辑排在先前文字之后，后续输入保持同队列；偏好与文字pending分计、一次释放，含持有完成COM对象及嵌套只读编辑锁回归 |
| 回车 | 有组字时普通Enter精确原文上屏、吃键，不插词前/内部/尾空格，不接受候选、换行或发送。带修饰键的Enter保留宿主快捷键，空组字的下一次Enter交给宿主 |
| 周边文字 | 只在显式确认时读取当前选区左侧一个UTF-16单元，先核验输入范围与邻区，原生兼容分支核对可编辑、非Hidden/Protected及最终焦点/选区。不保留跨输入框文字、不自动学习；AI发送非空选区门禁未放宽 |
| 状态栏 | 大小写、空格、补词、拼音、连写和设置/隐藏入口。补词/拼音/连写切换在下次组字应用，当前输入保留；Caps按钮实际切换CapsLock。桌面状态条不会注册键盘钩子或后台预测服务 |
| R4正式构建 | `work/build-logs/v0.4.0-r4-full-build.log`退出0；Rust396通过/0失败/6忽略（24汇总，含架构重复）、Python33、PowerShell602，合计1,031次通过，不是1,031个唯一用例。安装后4个探针另计 |
| R4正式安装 | 升级退出0；ActiveDir=`0.4.0-20261109103215-913067`，24项文件及build.json与冻结清单一致。真实Notepad PID20416加载该DLL，SHA256=`1901a863e219b9597ea945d03327c9e572981c19f9fc89a6923213bbc4f72c20`。文件与模块收据分别为 `v0.4.0-r4-installed-hashes.json`、`v0.4.0-r4-notepad-module.json` |
| R4真实输入 | 逐键`iamrobot Space`保存为`I am robot`；`i Space am Space jiqiren 1`保存为`i am robot`；`iamrobot Down Enter`保存为raw且无换行；Shift OFF的两次robot Tab连为robotrobot，手动Space保留；Shift ON的`iamrobotandilikeapples Space`精确为`I am robot and I like apples`。自动确认各行没有尾空格 |
| R4状态栏实测 | 空格按钮切换后无需重新点击编辑框即可继续输入；Caps按钮上/下切换得到G、g，Shift+A得到A且自动空格保持ON；输入rob时关闭补词，当前robot Tab仍完成，下次hel无本输入法候选，重新打开后下一次hel恢复。Shift+equal/minus后Enter仍raw hel。设置入口启动R4设置；显示开关可隐藏并在重新启用、下一次字母输入时重现 |
| R4保存核对 | 自有文件 `work/ui-tests/notepad-v04-final-20261109-103544.txt`；每个主要阶段Ctrl+S后读回核对，收据 `work/research/v0.4.0-r4-notepad-live.json`。鼠标拖动未移动且HasPosition缺失，不把这项标为R4通过，随后R5窄修鼠标捕获并重新正式构建 |
| 最终R5正式构建 | 只修正状态栏鼠标捕获的重复/重入及取消处理后重新完整编译、冻结和打包；`work/build-logs/v0.4.0-r5-full-build.log`退出0。24条Rust汇总396通过/0失败/6忽略，Python33、PowerShell602，正式合计1,031次通过；`v0.4.0-r5-test-counts.json`。安装后x64/x86各2通过、ActivateProfile均S_OK，4项另计，不等于x86编辑器键入 |
| 最终R5安装 | 升级退出0；ActiveDir=`C:\Program Files\EnglishAssist\versions\0.4.0-20261109110234-275066`，24项及build.json匹配；已映射偏好文件File ID和全部槽值保持。冷启动Notepad PID57228实际加载R5 DLL，SHA256=`dddf592c331706fbacea9bb802ca14aa10673ada24ca450732143abdf646778c`。收据 `v0.4.0-r5-installed-hashes.json`、`v0.4.0-r5-notepad-module.json` |
| 最终R5拖动/恢复 | 底部拖动使屏幕x从7092变为6867、y1965，独立映射读取保存坐标一致且HasPosition=1；Hide按钮确实隐藏、Disabled=1。R5设置“显示悬浮输入状态条”重新开启，恢复默认位置将HasPosition清0，回编辑框后下一次字母输入重新显示。不是只凭源代码或单元测试声称拖动有效 |
| 最终R5输入回归 | 再次逐键检查连写Space、英文加拼音1选词、Down后原文Enter，Ctrl+S保存文件精确为 `I am robot\ni am robot\niamrobot`，自动确认两行无尾空格、raw行无附加换行。Shift OFF后独立读取1、ON后读取0；最后自动空格/补词/拼音/连写/状态栏ON、CapsLock OFF、默认位置。`work/research/v0.4.0-r5-notepad-live.json` |
| 共享偏好失败与修正 | 初始R1/R2中Notepad的MSIX注册表COW使开关与独立设置不同步。R2每句柄DONT_VIRTUALIZE未解决MSIX，不声称有效；最终由安装器一次初始化用户Profile下固定288字节共享映射文件，23个已知DWORD槽，运行时不创建/替换文件、不读写混合JSON、不回退注册表。其他用户或数据目录不读取 |
| R3双向同步证据 | R3 Notepad Shift OFF后，独立只读映射器读到1；新启动R3设置显示OFF，点ON后独立读取0；回原Notepad的`iamrobot Space`保存精确I am robot。`work/research/v0.4.0-r3-bidirectional-live.json`是R3证据；R4升级在旧进程仍映射时保留相同File ID及全部23槽值，不能冒充R4重新执行完整双向流程 |
| 键盘配置 | `work/research/v0.4.0-keyboard-preserved.json`对比本轮前后LanguageTag、InputMethodTips与默认输入法override均相同。验收中的进程/会话选择不等于更改系统默认键盘 |
| 本轮未声明通过 | 全部应用、x86编辑器真实键入、全部控件、端到端性能统计、AI联机/AI GUI重新验收、卸载循环及重启删除。0.3/0.2的AI与生命周期证据继续保留其版本边界；老应用必须完全退出后重开才能加载新DLL |

## 历史 0.3.1 最终构建、升级与真实记事本检查点

本次正式构建、升级、产物核对、双架构安装探针，以及冷启动真实记事本有限输入与保存文件核对已完成。下方0.3私有RichEdit、CLI AI与0.2 Notepad AI通过项仍属于其历史版本，不移作0.3.1重新联机、AI GUI或全部应用验收。

| 0.3.1 项目 | 实际结果 / 验收边界 |
| --- | --- |
| 用户截图来源 | iamg只出现iamb的截图来自记事本PID39900；只读模块核对证实它仍加载 `C:\Program Files\EnglishAssist\versions\0.2.0-20261109035537-776110` 下DLL，尽管系统全局已安装0.3。不是两个开关失效；新开窗口会复用旧进程，不能用该截图推断0.3解码效果 |
| 备份与应用边界 | `work/backups/20261109-before-0.3.1-065410` 已保存Notepad LocalState的18个文件及产品配置，停止前另刷新before-stop备份。按用户已有直接退出/不保存授权退出全部旧记事本后冷启动，新PID46068已核对本次DLL；旧0.2进程消失，未读取其他恢复标签，验收后新记事本留开 |
| 可靠前缀的尾词回退 | 已完成至少2个词、内部相邻词边有实际记录且几何平均归一化概率>=0.005的多词前缀，允许有限unigram尾词回退，覆盖am gay/good缺pair的情形；单词前缀及噪声仍严格。候选上限和整段组合只提交一次的约定不变 |
| 词界提示 | 英文顶部按当前选项从完整text减去display_text推断前缀；前缀去空格须与raw起始字母ASCII大小写不敏感严格匹配，且有实际词间空格，才显示 `I am · g`、`I · a`、`I am · t`。不匹配、普通整体词或中文模式保留原串；不更改raw、候选主行或完整提交。选择、页码与推断前缀变化均参与绘制缓存失效；表驱动helper与渲染回归随本轮正式流水通过，iamg提示另经真实记事本核对 |
| 先行核心检查 | 根任务已执行engine lib67、英文集成18、english crate20通过/1忽略，合计105通过。正式完整流水另计368通过，不把此先行结果相加或把旧0.3的364移作本版结果 |
| 真实词库CLI | iamg首项going，后续有glad/getting；iamga第3项gay，iamgay和iamgood各首项gay、good。iamg在0.3本来也应能提供going，不能把旧进程现象当作该功能刚加入；字符不同的imag仍走image，zzzga无候选。排名仅为本次观察值 |
| 有限查询耗时 | 4,000次词库查询p50=0.0784ms、p95=0.2789ms、最大0.4046ms；仅词库查询，不含TSF编辑、绘制、IPC或GUI端到端耗时 |
| 正式构建 | `work/build-logs/v0.3.1-full-build.log` 退出0；24套Rust汇总368通过、0失败、6忽略（含x86重复），Python33通过、PowerShell508项通过。计数收据 `work/build-logs/v0.3.1-test-counts.json`；不是368个唯一用例 |
| 升级与产物核对 | `work/build-logs/v0.3.1-upgrade.log` 升级退出0；ActiveDir=`C:\Program Files\EnglishAssist\versions\0.3.1-20261109070929-873493`，24项文件摘要及build.json匹配。收据 `work/research/v0.3.1-installed-hashes.json` |
| 安装后双架构探针 | x64/x86各2通过，激活均S_OK；日志 `work/build-logs/v0.3.1-installed-probes-x64.log`、`v0.3.1-installed-probes-x86.log`。单列，不加进368；不替代x86编辑器真实键入 |
| 冷启动真实Notepad | PID46068实际加载本次 `retype_ime.dll`，模块收据 `work/research/v0.3.1-notepad-module.json`。iamg顶部为 `I am · g`、首项going；iamga第3项gay按3提交 `I am gay`；下一行iamgood按Tab提交 `I am good`；下一行iamt按4提交 `I am trying to`，均无重复前缀 |
| 精确保存核对 | Ctrl+S后自有测试文件精确为三行 `I am gay\nI am good\nI am trying to`。收据 `work/research/v0.3.1-notepad-live.json`；裁剪截图 `work/research/v0.3.1-notepad-iamga-live.png`、`v0.3.1-notepad-iamt-live.png`。仅代表本次实际Notepad受控场景，不扩大为全部应用 |
| 源码字节策略 | 源码构建策略测试14通过；不再强制JSON使用LF，而验证Git归一化后的哈希必须与已审核收据一致。此项为归档/构建完整性检查，不是运行时行为变化，不额外叠加进上述正式计数 |
| 本轮未声明通过 | 全部应用、x86编辑器真实键入、全部设置控件、AI联机请求或AI GUI重新验收、卸载循环及重启删除；0.3 CLI AI与0.2 R5记事本AI、生命周期证据保留历史版本边界。本地解码不具备完整语法或神经语言模型句意理解，候选排名是快照而非保证，raw imag与iamg不同且仍走单词路径 |

## 历史 0.3 最终 R2 构建与有限实测检查点

0.3最终R2已完成正式构建、冻结安装核对、双架构安装探针、私有RichEdit输入和CLI真实AI检查。下方0.2 R5的Notepad AI GUI、旧离线与生命周期结果继续保留版本边界，不自动扩大为本版全部应用或重新卸载通过。

| 0.3 项目 | 实际结果 / 验收边界 |
| --- | --- |
| 本地连写 | 同时考虑完整词/整体前缀与多词边界，末尾允许未完成prefix；词频及独立bigram参与有限搜索。核心查询及一个真实私有RichEdit宿主已测，候选整体质量与GUI端到端耗时不据此泛化 |
| 显示与提交 | Candidate.display_text仅作行内标签，Candidate.text保留当前组合的完整替换正文；Surface、PaintKey、UI-less GetString/更新通知和辅助名称使用显示字段，Finalize仍走完整text。显示刷新、完整提交与新增短语注释回归包含在最终完整流水中；缺少独立短语词典项时comment为“短语补全”，不借用trying等单词的词性/释义 |
| 示例约定 | ia保留ia…整体前缀及I am等解释；iamt可显示trying/trying to，确认一次得到I am trying/I am trying to，已识别I am不重复成行内候选或正文。多解允许，不承诺固定排名 |
| 原始输入与上限 | 推断前缀保留在同一组合范围，Esc及未显式选词的空格保留原始串；超过48输入字符退出连写搜索，沿用普通单词补全，不截断原文。退格与Esc已有下述GUI场景；失焦、边界、异步和最长输入的真实应用覆盖仍按有限范围解释 |
| 两个设置 | Windows默认启用连写智能分词/短语候选，Disabled缺失为0；共享Kernel默认false。前一个0.3设置GUI完成两项off/on操作并读HKCU核对保存。最终设置exe摘要不同，PID25020仅确认两项on、读取/滚动及截图 `work/research/v0.3-final-settings-input-live.png`，未重复off/on测试，不宣称同字节或全部控件通过 |
| 数据 | 独立双词资源、源码gzip及完整MIT声明已冻结，并经正式构建接入；精确来源见下文。数据来源、生成链边界与运行验证仍分别记录 |
| 早期 core R2快照 | engine lib67通过、英文集成17通过、english crate18通过/1忽略、types14通过，合计116次通过；该快照早于新增注释回归，不改写它的计数。最终完整流水另计364通过并覆盖新增测试；本子任务没有运行Cargo |
| 最终真实词库 CLI | annotation修复后的最终源码已重新生成CLI及renderer。ia/iam的I am位于首项；iamt的trying/trying to为第3/4项，iamtr为第1/2项；iamb回普通iambic，iambic/international/apple保持整体词路径，zzz/qzx没有该次词库补全结果。排名是本次观察值，不是固定产品保证；日志 `work/build-logs/v0.3.0-final-candidates.log` |
| 最终有限查询耗时 | 11,400次词库查询：p50=0.0539ms，p95=0.3137ms，最大0.6508ms。仅最终源码的词库查询，不包含TSF编辑锁、候选绘制、IPC或GUI端到端耗时 |
| 正式构建与安装 | 最终R2构建/安装成功；正式流水Rust364通过、0失败、6忽略（含x86重复），Python33通过，PowerShell508项通过。`work/build-logs/v0.3.0-final-r2-build.log`、`v0.3.0-final-r2-test-counts.json`。ActiveDir=`C:\Program Files\EnglishAssist\versions\0.3.0-20261109063014-912375`，24项产物摘要及build.json匹配冻结清单；升级日志 `work/build-logs/v0.3.0-final-r2-upgrade.log` |
| 安装后探针 | x64/x86各2通过，ActivateProfile均S_OK；单独记录，不加进364。日志 `work/build-logs/v0.3.0-final-r2-installed-probes-x64.log`、`work/build-logs/v0.3.0-final-r2-installed-probes-x86.log`；不替代x86编辑器真实键入 |
| 真实私有RichEdit | 自有宿主PID38236加载最终DLL；ia的am/ian两条解释同时可见；iamt第4项trying to按4完整提交I am trying to一次；继续iamt、Backspace、bic、Tab得到iambic；pingguo按1得到apple；iamt按Esc保留raw iamt；hel经ShiftEqual到2/6、ShiftMinus回1/6、Equal再翻页后按3得到helped |
| 精确文本核对 | 最终可观察document_text精确为 `I am trying to iambic apple iamt helped`，无重复前缀或释义。收据 `work/research/v0.3-final-rich-edit-tests.json`、截图 `work/research/v0.3-final-iamt-live.png`。这是自有RichEdit的文档/可访问状态核对，不称为本轮Notepad live或保存文件验收；测试后自有宿主已关闭 |
| 0.3真实production CLI AI | 升级后现有凭据实际请求成功，退出0：GrammarCorrect返回 `She goes to school every day.`，702ms；CompleteSentence返回 `I would like to know more about it.`，865ms。日志 `work/build-logs/v0.3.0-deepseek-live.log`，未输出密钥内容；这是CLI服务复验，不是本版AI GUI端到端复验 |
| 未覆盖边界 | 不声明所有应用、x86编辑器真实键入、全部控件、长文本质量或GUI端到端延迟通过；本轮没有重新做Notepad AI GUI、卸载循环或重启删除。0.2相关成功保留为历史，用户已有Notepad及其他工作窗口未关闭或读取 |

### 独立 bigram 来源记录

- 仓库：SymSpell，MIT声明，固定提交 `c239062ae02961df18ab7da1671d01b4388204e0`。
- 原路径：`SymSpell/frequency_bigramdictionary_en_243_342.txt`；实际242,342条，原文5,137,213字节；原文SHA-256 `fd892a160184101dd7ae807ac5a302d01fcea1c47304181a8ed7ed9c94545bcd`。
- 无损gzip：`data/dict/raw/english-bigrams/frequency-bigrams.txt.gz`，1,953,535字节；SHA-256 `9af65d1e5ce272076ebd60e719f11b6341ca93294411af160d33c48e2b4f1fd1`。
- 完整LICENSE-SymSpell SHA-256：`858e5c7e7a38824f33cf466719509884f2ee5e35992dbfce377fec0656545814`。来源/许可清单为同目录SOURCES.json；内嵌产物与编译器摘要见core/english/assets/bigrams.json。
- 与CC BY-SA英汉侧车分别声明，保留NOTICE-English-Assist.txt新说明；不声称复现上游bigram原始语料生成链、Google Books V3重生成或包含Gboard模型。原 unigram 的SCOWL/Google Ngram许可继续分别适用。

## 历史 0.2 R5 集成交付检查点

以下为旧0.2 R5证据。它当时重新正式构建，区分生产HTTP、真实记事本GUI、保存文件核对及更早版本记录，不扩为0.3的新构建/安装通过。

| R5 项目 | 当前结果及边界 |
| --- | --- |
| 正式构建 | `work/build-logs/v0.2.0-ai-r5-build.log` 显示正式构建完成；24 条 Rust summary 合计350通过、0失败、6忽略（含 x86 重复执行，不是350个唯一用例），Python19通过；PowerShell19条 cases-passed 输出合计505项 |
| 正式升级与文件核对 | 升级退出0，ActiveDir=`C:\Program Files\EnglishAssist\versions\0.2.0-20261109035537-776110`；24项产物及 build.json 全部匹配冻结清单。日志 `work/build-logs/v0.2.0-ai-r5-upgrade.log`；核对收据 `work/research/20261109-installed-ai-r5-hashes.json`。本轮没有再次卸载 |
| R5 安装后 x64/x86 探针 | 两种架构各2通过、0失败、退出0，ActivateProfile 均为 S_OK；直接运行已有 R5 测试程序，进程内选择已恢复，没有注册或默认输入法变更。日志 `work/build-logs/v0.2.0-ai-r5-installed-probes-x64.log`、`work/build-logs/v0.2.0-ai-r5-installed-probes-x86.log`。单独记录，不加进正式构建的350项；不替代 x86 编辑器真实键入测试 |
| 凭据与真实 production HTTP | 用户已私下配置新 Key，升级后实际成功请求确认凭据保留；不在文档、日志或源码包保存密钥。受控样例 `She go to school every day.` → `She goes to school every day.`，观察耗时889ms；`I would like to` → `I would like to go home.`，799ms。收据 `work/research/deepseek-live-api.json`。这两次 HTTP 观察值不等于 GUI 端到端延迟、质量统计或服务保证 |
| 真实 Notepad 语法修正 | PID39900 已加载该 R5 DLL。选中公开测试句 `She go to school every day.`，Ctrl+Alt+G 得真实预览 `She goes to school every day.`；Ctrl+Enter 应用、Ctrl+S 保存，再读取自有测试文件与预期完全一致 |
| 真实 Notepad 补句取消 | 选中 `I would like to`，点击补完句子，真实预览 `I would like to go home now.`；Esc 取消后 Ctrl+S，自有文件保持原输入完全一致 |
| 真实 Notepad 补句应用 | 再次补完，真实预览 `I would like to learn more about this.`；Ctrl+Enter、Ctrl+S 后，自有文件与该结果完全一致。三场景收据 `work/research/20261109-notepad-ai-r5.json` |
| 输入范围核验 | R5 对 scope 查询 E_FAIL 的兼容处理仅适用于能证明为本进程同线程、可见可编辑的 RichEdit，且选择与 TSF 选区一致、非 password/hidden/protected；捕获、派发及应用三个阶段均复验，显式敏感 scope 继续拒绝。固定元数据导出 `work/research/notepad-ai-r5-scope-metadata.jsonl` 不记录输入正文 |
| 验证范围 | 真实 GUI 结果限上述 Notepad 进程及公开受控样例；不声明全部应用、x86 编辑器键入、全部敏感控件或长文本质量已逐项实测。R5 未再次卸载；旧文件实际重启删除未核验；配置凭据后不再截图 AI 设置页 |

真正 AppContainer 中，AI helper 未预先运行时仍不支持冷启动 AI，从候选菜单启动设置也受限制；开始菜单“英文助手设置”可用。不能仅凭 packaged 身份认定 AppContainer，也不能把 Notepad 成功扩大为全部沙箱应用通过。

### 此前 r2/r3 证据（历史）

以下记录属于旧构建/安装版本，保留作溯源。r3 当时只刷新脚本、原生与 final-r2 相同；该字节一致性不适用于本次重新构建的 R5。

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
| r3 安装探针 | 当时安装的 x64/x86 各 2 项发现、加载和进程内激活检查通过，激活 HRESULT 均为 S_OK；日志 `work/build-logs/v0.2.0-installed-x64-final.log`、`v0.2.0-installed-x86-final.log`。这不替代 R5 探针或 x86 编辑器真实键入 |
| r3 设置截图/操作 | 设置套件20通过/1忽略；当时 installed exe 的常规、输入、外观、词库、AI 五页已截图并逐页检查布局，见 `work/research/settings-cli-0.2-20261109-024903/manifest.json`。这是配置新凭据前的旧版截图，不是 R5 新截图，也不代表全部控件真实点击验收 |

**历史 r3 生命周期：安装→卸载→重装完成。** 当时 pending 队列未修改，8项均为已知旧 DLL 删除或经属性核验的已知目录空删：3个旧 DLL、3个旧 payload 目录、versions 和产品根目录。新目录独立分配，排队成功不等于实际删除；本次 R5 没有执行重启删除核验。

r3 重装前检查支持实测的精确 `*1\??\` NT 路径形式，仍校验产品范围、旧版本关系、目录属性与重解析限制。`*1` 在此仅是实测队列格式，不能解释为公开 API flags。目录重启删除只在目录为空时发生的依据是 [Microsoft MoveFileExW 文档](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw)；该文档不用于解释此实测前缀。

r2 已加入本包资源优先加载、生产输入 Noop 学习、无学习服务登录自启及 broker 登记/卸载状态门。缺失或错误的登记、Uninstalling 非显式 DWORD0 时 broker 拒绝生产启动；测试命令的隔离数据库例外不代表实际用户路径。r3 新脚本由其独立策略测试及实际生命周期结果覆盖。旧 r2 记事本离线三场景仍保留为旧版本证据，不当作 R5 全部应用重新实测。

在上述旧检查点尚未配置新凭据，不能由那些旧记录证明真实 DeepSeek 服务；最新 HTTP 与 GUI 结果已更新到本节顶部 R5 表。

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
| 0.3 连写智能分词 | EnglishSegmentationDisabled（1=关闭） | Windows开启；共享Kernel默认关闭 |
| 0.3 短语候选 | EnglishPhrasesDisabled（1=关闭） | Windows开启；共享Kernel默认关闭 |
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
- 0.3 的 `data/dict/raw/english-bigrams/` 全目录及 `core/english/assets/`：原始gzip、完整MIT声明、来源/输出清单、内嵌image与独立编译/校验脚本及测试；不能只归档旧词义数据。
- `data/dict/english-lexicon.json`、`data/dict/resources.json`、顶层 LICENSE、NOTICE.txt、NOTICE-English-Assist.txt。
- `tools/english-assist/` 下的数据编译、准备、验证脚本及测试，以及该版本构建脚本、产品操作和验证说明。

完成 ZIP 后应记录实际摘要并检查归档条目；本次文档更新不声明源码 ZIP 已完成。不得将凭据或用户输入文件纳入源码包。

## 历史边界

- 2026-11-07：初始源码阶段缺少 link.exe，首次 cargo test 在链接阶段失败，没有运行测试；当时采用数字全部原样输入、无编号候选方案。
- 随后安装 MSVC/SDK，修复测试 fixture 对旧 Shift 默认绑定的依赖；生产 Shift 默认保持禁用。普通富文本候选路径和私密范围分别建立回归。
- 2026-11-08：用户明确要求 1–5 选词、减加号翻页，当前交互表取代初始数字方案；新增候选释义、拼音、外观和手工词库设置。根任务负责 0.2.0 最终构建、安装和实际应用验证。
- 2026-11-09 早期检查点：旧 final-build 合计318 Rust通过/6忽略、Python19、脚本374；当时安装 DLL 为87DAFFFA前缀。旧9431进程中的记事本数字选词可提交hello，但释义失败，新窗口仍复用旧进程；早期卸载1→直接cleanup0→retry1。以上均为旧构建历史，最新证据在顶部 R5 表，不将旧进程结果混入新包验收。
- r2 历史待办曾有 HKCU CTF TIP Enable=0 缓存及8项旧文件/目录排队删除，重装尚未完成。r3 已清除自有缓存、核验旧路径并完成再次重装，pending 队列本身未修改；仍未执行实际重启删除核验。
- R4 曾在正常 Notepad 的 scope 查询 E_FAIL 时误拒绝 AI 选区，不能视为 GUI 成功；R5 窄范围兼容修复后已完成顶部三场景真实 GUI 验证。r3 卸载循环与旧版截图不替代 R5 的重新卸载或配置凭据后截图。
