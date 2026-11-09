# English Assist 产品规格与交付门槛

版本：0.6.0。更新日期：2026-11-09。

## 0.6.0 现代词义与扩展实体

当前检查点：0.6.0 R1正式构建、升级安装、双架构激活探针及有限真实记事本验收已完成。正式通过1,135次（Rust427＝常规425＋准备资源2、Python74、PowerShell634），0失败，含x86重复，不是唯一测试数；4项安装探针另计。0.5及更早结果仅属历史。

| 模块 | 当前行为与数据边界 |
| --- | --- |
| 现代词义 | EAMODLX1独立词义流保留POS、中文、用途、说明性全称、来源URL及稳定sense ID；4,009义/2,134键，和普通ECDICT义、实体分别去重 |
| 中文来源 | 归并前3,982条来源中文（中文维基词典3,824、英语版本确证中译158），另有61条Codex辅助项目中文补充；合计4,043，归并34明确重复后4,009，不把项目补充冒充上游译文 |
| 缩写与普通义 | IDK/IMO/TBH已审核常用网络义优先；IMO其他海事/数学义保独立身份。已有普通词性/高频普通义受到保护，全大写不是提升APPLE/SICK冷义的理由 |
| 大小写与提交 | 普通现代词尊重输入case，如Rizz/RIZZ；有全称证据的缩写与专名保canonical。GOAT只有全大写、精确、审核优先级和网络缩写全称证据齐全时可越过普通义；lowergoat保持动物普通义。全称仅说明、永不隐式展开提交 |
| 来源徽标 | 网络/缩写/俚语等用途与现代·公司及拼音/扩展汉字证据分别展示；缺全称不伪造，LOL另一缩写义只可帮助确认规范拼写，不冒充最高网络感叹词义的全称 |
| 实体目录 | EAMODEN2为1,173实体/2,239条别名关系（2,161种不同拼写），保原51审核种子，允许同别名多个QID；短中文身份说明不是完整词义，缺真实官网时不伪造 |

现代词义TSV620,800字节，SHA-256 `ccbb70532631d3cdc000ff63fd702b6d496e09d1b4f024e11a9670d4bfaa9749`。目录规模是有限冻结覆盖，不是全量Wiktionary/Wikidata，也不是当前热度或句意模型。数据清单见 `data/dict/modern-lexicon.json`、`data/dict/modern-entities.json`。

资源worker验证并构造不可变prefix优先索引；每现代流最多24候选、256个叶扫描，精确键先于前缀，不逐键全扫数千义项。User显式词条优先，多个现代sense及同别名实体以来源身份独立保留；低优先技术缩写不锁掉ia/iam/iamt的旧连续路径。普通查询/冻结数据常规校验不读Key、不联网、不自动学习输入历史。

原始通道沿用：Enter始终raw；plain Space不展开全称，审核网络缩写视为完整原串；自动空格ON无尾空格、OFF保物理空格，单Shift只切空格偏好。拼音尾词仍需显式选择并完整提交一次。当前有限验收如下，不从下方旧版通过项推定更多范围。

### 0.6.0 R1正式结果与边界

升级退出0，ActiveDir为 `C:\Program Files\EnglishAssist\versions\0.6.0-20261109143156-791774`，29项产物摘要及build.json全部匹配；映射偏好文件身份及23个值保留。原始Rust汇总8次忽略中的2组准备资源后续已计入427次通过，安装后x64/x86探针各2项通过、激活S_OK并另计，仍有2项未执行；详见 `work/build-logs/v0.6.0-r1-test-counts.json`。安装收据 `work/research/v0.6.0-r1-installed-hashes.json`。

真实Notepad PID29288已确认加载本次DLL，SHA-256 `4af101ed03fec47d16e5e833534f2e8793ecbc0d44af4a70ac765cd7abbce111`。逐键操作并保存自有文件读回核对：

| 本次实际场景 | 结果 |
| --- | --- |
| IDK按1、IMO按Tab | 只提交缩写，中文/POS/全称仅展示；IMO海事/数学义独立可见 |
| rizz按1、Shift字母输入RIZZ后Tab | 大小写保持，Shift字母组合不切自动空格 |
| GOAT按1、小写goat按Tab | 分别采用网络缩写义与普通山羊义 |
| libreof按1 | 规范专名LibreOffice，中文身份说明和现代软件标记可见 |
| tbh导航后Enter、idk翻页后Enter | 原串上屏、不附换行；idk的Shift+等号2/2、Shift+减号1/2往返正确且空格仍ON |
| iamrobot按Space、混合yiyang按1 | 分别得到I am robot、完整I will do the same一次，无尾空格 |
| kotlin未收录 | 无候选，Enter保原串成功；属于已知覆盖缺口，不能称Kotlin补全通过 |

最终自有文件精确12行，无尾空格或最后换行；完整文字见 `docs/english-input.md` 或 `work/research/v0.6.0-r1-notepad-live.json`。模块收据 `v0.6.0-r1-notepad-module.json`；有效语言标签、Input Tips及默认覆盖值前后保持一致，收据 `work/research/v0.6.0-keyboard-preserved.json`。

本版未验收全部应用、x86编辑器真实键入、AI服务/GUI、卸载或重启清理。最终源码ZIP尚待提交后复验，从ZIP完整原生重建未验证；候选排名只是本次观察快照，覆盖仍是有限目录。

## 沿用0.5的关系词库与历史检查点

历史状态：0.5.0 R1正式构建、升级安装、双架构激活探针及有限真实记事本验收已完成。下述0.5、0.4及更早结果均不挪作0.6验收。

| 模块 | 本版行为与边界 |
| --- | --- |
| 关系词库v2 | `english-lexicon-v2.bin` / RTENLX02保留中文词条、带数字声调读音、原始义项、关系来源/使用证据和英文词引用；拼音不再只压成英文词集合 |
| 拼音排序 | 精确键保留按中文用频、现代/古旧与变体证据、关系质量等生成的预排；前缀查询用不可变优先索引选取有限键，再按中文证据排序去重，不用全局英文词频覆盖中文含义 |
| 匹配提示 | 精确键显示 `拼音 · 想`，前缀扩展显示 `扩展 · 想法`；实际匹配汉字与英文词性、中文释义分开，原读音/义项保留为来源证据；不保证同音输入唯一理解 |
| 英文夹拼音 | 同时开启连写与拼音时，可靠英文前缀后可查询最后的拼音尾串。iwilldotheyiyang / iwilldothetongyang提供same，确认完整I will do the same一次，候选主行只显示same |
| 原串通道 | 未明确选词时，Space遇已识别拼音解释保留原串，不误拆拼音字母；其他exact分词只插原串内部英文词界。数字/Tab/点击或显式导航后Space可确认混合候选；Enter保持原始字节，导航后也不选词 |
| 现代实体 | 51条有限审核种子保留规范名字、实体类型、简短中文说明、Wikidata修订和官网身份链接；大写前缀优先名字，罕见同拼写英文可优先实体，常见小写普通义保留首位且实体独立一行 |

词典正文来源仍是固定ECDICT与2017-11-17 CC-CEDICT，而非更新到当前MDBG词典；新增固定Wanxiang v18.0.14字/词频仅为中文关系排序提供证据。现代实体是有限离线种子，不是完整Wikidata、通用知识库、实时检索或新的句意模型。全部本地查询不逐键联网、不自动学习输入历史。

运行查询预算：拼音最多64个候选、64个前缀键，总扫描512条关系（重复也计入）；格式可保留更多关系，避免编译时按英文频率永久截掉不同中文词条。混合查询复用现有48字符/6词/beam8英文路径，每个候选边界查尾词一次，不额外预测尾词后的短语。英文整体词与显式个人词条继续受到保护；两个开关分别关闭都不生成混合尾词。

原串保护示例：`iwilldotheyiyang` 直接Space保持原串，不误拆成 `I will do they I yang`；先选中same候选再按Space则完整提交 `I will do the same` 一次。自动空格关闭时物理Space另保留手动空格行为。定向英文39项通过；真实准备资源集成1项随后纳入正式构建通过总数。

### 0.5.0 R1正式结果与边界

正式构建通过1,078次检查：Rust413、Python46、PowerShell619，Rust0失败，含x86重复运行，不是唯一测试数。Rust原始汇总7次忽略，其中真实准备资源集成后续显式通过、已计入413；安装后另行执行x64/x86探针各2项通过、激活S_OK，不重复加入1,078总数。记账收据 `work/build-logs/v0.5.0-r1-test-counts.json` 保留原始与后续执行关系。

升级退出0，安装目录 `C:\Program Files\EnglishAssist\versions\0.5.0-20261109121445-346651` 的25项产物摘要及build.json全部匹配冻结构建。真实Notepad PID58000加载该目录DLL，SHA-256为 `4f5a738d8e9647e884f333746380c7bdbe24006604d897f3677e75a1d7b8bd3c`。安装及模块收据为 `work/research/v0.5.0-r1-installed-hashes.json`、`v0.5.0-r1-notepad-module.json`。

| 真实记事本场景 | 本次观察结果 |
| --- | --- |
| 两个英文夹拼音样例 | same均为首项，分别按1和Tab完整提交I will do the same一次，无尾空格 |
| xiang匹配与翻页 | 首屏think/想、mutually/相、to/向、like/像、item/项；Shift+等号翻页可见fragrant/香、elephant/象；Enter原样提交xiang、不换行 |
| 现代实体与普通义 | Anthropic公司/Claude开发商说明与普通anthropic形容词义并存，按1仅提交规范名字Anthropic |
| Space与拼音保护 | iamrobot+Space得到I am robot；iamnihao已有hello/你好候选，未选词Space仍保留iamnihao |

自有保存文件精确为六行：`I will do the same`、`I will do the same`、`xiang`、`Anthropic`、`I am robot`、`iamnihao`。自动空格保持ON，原键盘与默认设置保留；收据 `work/research/v0.5.0-r1-notepad-live.json`。这些候选排名只是本次快照，不保证固定顺序。本版未验收全部应用、x86编辑器真实键入、AI联机/GUI、卸载或重启删除；源码ZIP尚不能作为完整原生重建通过证据。

## 历史0.4.0 状态条与自动空格检查点

正式状态：最终R5构建完成，24套Rust汇总396通过、0失败、6忽略（含x86重复），Python33通过、PowerShell602项通过，合计1,031次通过结果；不是1,031个唯一测试。安装后x64/x86探针各2项通过，进程内激活均S_OK，4项单列不加进正式合计。升级退出0，安装目录 `C:\Program Files\EnglishAssist\versions\0.4.0-20261109110234-275066` 的24项产物摘要及build.json匹配冻结构建。计数收据 `work/build-logs/v0.4.0-r5-test-counts.json`，安装核对 `work/research/v0.4.0-r5-installed-hashes.json`。源码归档不替代原生运行验收，本节不宣称本轮AI、卸载或全部应用通过。

最终有限实测：冷启动Notepad PID57228已确认本次DLL，SHA-256为 `dddf592c331706fbacea9bb802ca14aa10673ada24ca450732143abdf646778c`，模块收据 `work/research/v0.4.0-r5-notepad-module.json`。iamrobot按Space得到I am robot无尾空格；i按Space、am按Space、jiqiren按1得到i am robot；iamrobot先Down再Enter得到原始iamrobot且无换行。自有文件精确保存这三行。Shift OFF/ON对应共享值1/0。R5修复鼠标捕获后，状态条实际从x=7092拖到6867，保存StatusBarX=6867/HasPosition=1；×隐藏对应Disabled=1，最终设置显示ON后恢复状态条，重置位置对应HasPosition=0。截图 `work/research/v0.4.0-r5-statusbar-live.png`、`v0.4.0-r5-settings-input-live.png`；不把这些有限动作扩大为全部控件通过。

历史0.4证据：R3在真实打包记事本与独立设置程序中完成双向空格偏好同步及exact Space核对。Notepad PID56416加载 `0.4.0-20261109102312-570716` DLL：Shift将共享值置1，设置GUI显示OFF；设置点ON将同一共享值置0，记事本随后把iamrobot+Space保存为I am robot且无尾空格。收据 `work/research/v0.4.0-r3-bidirectional-live.json`。R4回调在当前组字结束后同步其他功能开关、不要求再切焦点；R4的长串、Caps Lock和功能开关操作仅保留为R4证据，未全部重新测试于R5。R5与R4的原生源码差异仅为状态条鼠标捕获修复，不因其余源码相同就声称其余原生GUI行为全部复测。下方0.3.1和更早记录同样保留历史边界。

- 独立原生悬浮状态条使用NOACTIVATE/TOOLWINDOW，不依赖候选弹窗是否有条目；仅在本输入法激活且安全可编辑输入框聚焦时显示，失焦、关闭或私密范围隐藏。持弱状态引用，无全局键盘hook、无输入正文或凭据读取。
- 实时Caps Lock读toggle位，不以Shift修饰状态代替；点击大小写前再次核对同线程原编辑窗口和前台根窗口，修饰键按住时不注入切换。
- 自动词前空格、补词、拼音、连写按钮使用真实偏好；自动空格即时同步，其他功能不强制确认当前组字，结束组字后同步并从下次词生效，无需再次切焦点。设置/隐藏可点击，隐藏后可在设置或系统语言栏菜单恢复。失败反馈明确，不伪装写入成功。
- 原生窗口显示与拖拽均不激活宿主外窗口。位置记忆保留负坐标并限制在可用屏幕范围；“输入与查词”提供显示开关及恢复默认位置，字段为StatusBarDisabled/StatusBarX/StatusBarY/StatusBarHasPosition。
- EnglishAutoSpaceDisabled缺省0即Windows开启；共享Kernel默认关闭。确认新词只按需添加词前空格，绝无自动尾空格；拼音选择相同。单按Shift切换，不改变补词、拼音、连写或AI偏好，Shift字母/符号和其他组合键保持原用途。
- 开启自动空格及连写分词时，plain Space对当前原始串执行独立exact分词，只补已输入词的内部词界，如iamrobot→I am robot；不选择未输入词的补全。显式导航后Space可确认候选，数字/Tab/点击使用完整候选。关闭自动空格时Space用于手动隔词，数字/Tab/点击不补空格。
- 有组字时Enter始终原样确认并消费按键，即使候选曾被导航也不选词、不插空格；没有组字时Space/Enter保持宿主行为。Esc原串保留。此逻辑不调用AI或自动学习。

历史0.3.1状态：正式构建及升级退出0，24套Rust汇总368通过/0失败/6忽略（含x86重复）、Python33、PowerShell508；x64/x86安装探针各2通过、激活S_OK。目录 `0.3.1-20261109070929-873493` 的24项产物摘要及build.json匹配，冷启动真实记事本已通过有限输入、词界提示和保存文件核对。0.3设置/CLI AI与0.2 R5 Notepad AI、旧离线和生命周期证据保留各自版本边界，不作为0.4重新验收全部控件、AI或全部应用的结论。

## 0.3.1 修复检查点

用户的iamg截图来自记事本PID39900：系统已安装0.3，但该进程仍加载 `0.2.0-20261109035537-776110` 的DLL。不是两个连写开关失效；升级后仅新开窗口可能继续复用旧进程。已将该应用LocalState的18个文件及产品配置备份到 `work/backups/20261109-before-0.3.1-065410`，停止前另刷新before-stop备份。退出全部旧记事本后冷启动PID46068，已核对当前DLL路径；其他恢复标签未读，验收后该记事本保持打开。

- 已完成至少两个词、内部相邻词边有实际记录且几何平均归一化概率不低于0.005的可靠前缀，允许有限单词频率尾词回退；单词前缀与噪声仍使用严格规则，不增加候选上限或改变完整组合提交。
- 当前选项的完整提交文字减去显示尾词后，只有其前缀去空格与原始输入起始字母按ASCII大小写不敏感严格匹配，顶部才显示 `I am · g` 等词界；组合原串、候选主行与提交正文不改。
- 真实词库CLI观察：iamg首项going（0.3本来也应能提供），iamga第3项gay，iamgay/iamgood各首项gay/good；字符不同的imag仍走image，zzzga无候选。不保证所有输入的唯一解释或固定排名。
- 先行核心检查engine67、英文集成18、英语库20通过/1忽略，共105通过；4,000次有限词库查询p50=0.0784ms、p95=0.2789ms、最大0.4046ms，仅查询耗时，不含GUI。正式完整流水另计368通过，不与该先行计数相加。
- 本次真实记事本中iamg顶部显示 `I am · g`、首项going；iamga按3、iamgood按Tab、iamt按4后，保存文件精确为三行 `I am gay`、`I am good`、`I am trying to`。收据、模块路径及裁剪截图见验证记录。本地解码不具备完整语法或神经语言模型句意理解，观察排名不构成保证；本轮AI服务和AI GUI未重新执行。

## 产品定位

Windows 原生 TSF 英文输入法，登记在英语（美国）语言下。保留 Hallelujah 的英语学习辅助体验：英文补全、拼写建议、中文释义、词性、音标和拼音找英文。候选与中文设置采用统一的现代界面。设置的信息结构参考 Gboard，所有可操作控件必须有持久化和实际运行时行为。

## 首版完整功能

| 模块 | 功能与默认行为 |
| --- | --- |
| 英文输入 | 离线前缀补全、大小写保留、独立拼写建议开关 |
| 连写与短语 | 本地多路径分词，允许末尾prefix；候选显示当前后缀，显式确认后完整替换组合一次；Windows默认启用两个独立开关 |
| 0.5混合与实体 | 英文前缀加拼音尾词、匹配中文证据提示，以及独立现代实体含义；R1有限记事本结果见上方 |
| 0.6现代词义与扩展实体 | 4,009独立义项、2,134检索键，1,173实体/2,239别名关系（2,161种拼写）；缩写只提交短形式，来源/全称仅展示；R1有限记事本验收见上方 |
| 英汉词典 | 候选英文与词性、中文释义分开显示；音标可选；展示字段不写入正文 |
| 拼音查英 | 默认开启，拼音精确匹配优先，也提供有限前缀结果；去重后提交英文 |
| 候选操作 | 每页五条，1–5 选当前页；Tab 接受首选或已选择项；鼠标点击选词 |
| 翻页 | `-` / `_` 上一页；`=` / `+` 下一页；支持 PageUp / PageDown |
| 原样输入 | 无候选时数字与翻页符号原样输入；Enter和Esc保留当前原始串；空Space/Enter保留宿主行为 |
| 确认与空格 | 数字/Tab/点击确认候选；默认只按需补词前空格，无尾空格；Space按自动空格开关处理；Enter不选候选 |
| 输入状态条 | Caps Lock实时状态、词前空格/补词/拼音/连写开关、设置与隐藏；可拖动，不抢输入焦点 |
| 外观 | 跟随系统/浅色/深色、字号、宽度、紧凑模式、释义/词性/音标/操作提示显隐 |
| 个人词库 | 显式增删改、导入、导出；保存后下次聚焦加载；不自动记录输入历史 |
| AI 辅助 | 用户明确选择文本后触发语法修正或补句；先预览，确认后替换；支持取消 |
| 系统集成 | x64/x86、独立产品标识、英语键盘入口、独立设置入口、升级与卸载 |

数字选词会占用有候选时的数字键。输入带数字的原始词时可先按 Esc 结束当前组字，再输入数字。

## 设置结构

1. 常规：版本、系统输入法使用指引与安装状态。
2. 输入与查词：英文补全、拼写、拼音查英、连写与短语、自动词前空格、状态条显示与位置恢复、显式符号补全及键盘操作说明。
3. 候选外观：主题、字号、密度、宽度与释义字段。
4. 个人词库与数据：词条管理、导入导出、备份和本地数据位置。
5. 快捷键：明确显示实际生效的键盘操作与 AI 触发键。
6. AI 服务与编辑：服务商、模型、凭据、连接测试、编辑任务和替换行为。
7. 关于：版本、许可、第三方数据来源与诊断位置。

## 架构与数据边界

- 0.4输入DWORD偏好使用原生UserProfile下 `.EnglishAssist/input-preferences-v1.bin` 的常驻共享映射，不放在会产生包内副本的AppData或HKCU运行时DWORD路径。KnownFolder Profile定位，固定EAINPF01版本1/288字节，23个已知字段及保留槽；不含API Key、输入正文、候选或历史。
- 安装时初始化并一次迁移旧原生HKCU输入DWORD；有效同版本文件升级时保留，不替换仍被宿主映射的文件对象。旧HKCU只承担这些输入DWORD的迁移来源，路径等字符串及产品登记继续沿用注册表；凭据仍在Windows凭据管理器。
- 偏好按已知字段原子读写，未知字段、坏头、长度/版本不符或初始化失败均拒绝，不退回包内注册表写入；设置显示实际错误。成功映射常驻，失败仅在激活/聚焦边界允许重试，不逐键重新打开文件。
- 当前双向实测限上述打包Notepad与设置程序；真正AppContainer是否有共享文件访问权限须单独验证，不把包身份等同于AppContainer或承诺所有容器可读写。AI helper冷启动和候选菜单打开设置的既有限制另行保留。
- TSF 负责生命周期、焦点、按键路由、编辑锁及候选枚举。
- 内核维护当前组字、候选排序、五条分页、确认和取消。
- 英文资源由后台工作线程加载；按键路径只查询已加载的只读映射。
- 安装版优先校验当前 DLL 同包词典；显式开发环境覆盖仍具有最高优先级。
- 频率词库与词义/拼音侧车分离。0.5使用RTENLX02关系侧车，英文词记录保留独立词性、中文释义、音标，拼音关系另保留中文匹配词、原始读音和义项；不回退旧v1混排格式。
- 候选的提交文字与展示注释采用独立字段，数字、释义和词性不混入正文。
- 0.3 Candidate.text 保持完整组合的提交文字，display_text 仅用于当前尾词/短语显示；TSF、UI-less、位图缓存及辅助名称使用显示字段，确认操作仍使用完整 text。
- 个人词库通过显式编辑原子保存，其他宿主在聚焦后按代次刷新。
- 英文版使用空学习存储，不启动自动学习连接或登录自启；旧助手启动受本产品登记及卸载状态保护。
- DeepSeek 请求在独立辅助进程中执行。只有明确触发的选区送出；密码、私密和 PIN 范围不处理。
- 云端结果应用前重新核对焦点、原选区及文字；过期结果不得写入。

## 数据许可与可复现构建

英文频率数据沿用冻结资源。英汉词义采用 ECDICT 的 MIT 数据；拼音映射采用冻结 CC-CEDICT 原文，遵循该原文的 CC BY-SA 3.0。元数据侧车独立分发，保存来源版本、输入/输出哈希和完整许可。不得将来源不清的旧翻译库直接迁入。

具体冻结源：ECDICT提交 `bc015ed2e24a7abef49fc6dbbb7fe32c1dadaf8b`；CC-CEDICT为HallelujahIM提交 `8be294f70a4dc7b24a93f07c41df6c34add07a07` 镜像的2017-11-17原文，原文件头为CC BY-SA 3.0。[当前MDBG下载页](https://www.mdbg.net/chinese/dictionary?page=cc-cedict)所列最新数据采用CC BY-SA 4.0，不能与本包2017快照混为同一版本或许可。Wanxiang v18.0.14的 `zi.dict.yaml` 和 `jichu.dict.yaml` 另保留CC BY 4.0署名，精确原文/来源哈希见 `data/dict/raw/english-assist/SOURCES.json`、`data/dict/raw/wanxiang-base/SOURCE.md`。

现代实体另以CC0 Wikidata结构化快照、固定实体ID/修订及审核官网链接记录来源，目录与投影在 `data/dict/modern-entities.json`、`data/dict/modern-entities.tsv` 和 `data/dict/raw/modern-entities/`。更新工具只在明确调用时提出更新，运行时不访问网络；有限目录不承诺收录任意公司/产品。配套源码需同时保留这些材料、Wanxiang频率源、v2编译器及测试，不能只归档生成的词库文件。

0.6现代词义与项目中文补充按CC BY-SA 4.0独立发布，保留Wiktionary贡献者署名、逐sense来源URL、冻结哈希与改动说明；提取器Wiktextract软件MIT不改变数据许可。匹配源码包需包含 `data/dict/modern-lexicon*`、`data/dict/raw/kaikki-modern/` 两版冻结投影/来源清单/完整许可及独立编译校验脚本。大型完整dump在独立获取缓存，常规构建使用审核后的投影，不自动刷新数据；现代实体CC0和旧英汉关系侧车的许可各自保留。

0.3 的内嵌双词资源独立于英汉侧车：冻结 SymSpell MIT 仓库提交 `c239062ae02961df18ab7da1671d01b4388204e0` 中的 `SymSpell/frequency_bigramdictionary_en_243_342.txt`，原文5,137,213字节、242,342条，SHA-256 `fd892a160184101dd7ae807ac5a302d01fcea1c47304181a8ed7ed9c94545bcd`；无损 gzip SHA-256 `9af65d1e5ce272076ebd60e719f11b6341ca93294411af160d33c48e2b4f1fd1`。完整来源/许可哈希在 `data/dict/raw/english-bigrams/SOURCES.json`，内嵌产物清单在 `core/english/assets/bigrams.json`。保留 NOTICE-English-Assist.txt 的独立数据声明与 LICENSE-SymSpell，不声称已复现该 bigram 的原始生成链或包含 Gboard 模型。

构建脚本校验冻结源与生成产物，离线安装包携带词库，不要求首次启动联网。源码快照、产物哈希、构建记录和验证矩阵必须与交付版本一致。

## 验收门槛

- `hel` 显示英文候选及中文/词性；数字选词仅提交对应英文。
- `nihao` 能找到 hello，`pingguo` 能找到 apple，`xuexi` 能找到 study；关闭拼音后结果不再由拼音提供。
- 前后翻页正确处理第一页、最后一页、少于五条及异步拼写结果。
- 改主题、字号、字段显隐后，已有候选刷新；点击区域与当前页编号一致。
- 个人词条增删改和导入导出可核对，坏文件不破坏原数据。
- 0.5需分别核对xiang等同音词的匹配汉字/精确与扩展提示；混合尾词完整替换一次，plain Space与Enter保留原串、显式选中后Space可接受译词；普通词义与同拼写实体独立保留。未测应用不写为通过。
- AI 选区变化、文档切换和焦点离开后，旧结果零写入；失败不影响本地输入。
- 分别记录真实富文本宿主、记事本、Chromium 文本框、VS Code、x86 宿主的切换、输入、选词与焦点行为。未测项目明确标注。
- 卸载移除本项目用户入口、COM/Profile、产品注册；保留用户选择的数据策略，不影响其他输入法。
- 被运行中宿主占用的旧 DLL 可以等待重启删除，但必须列清路径，安全重装不得覆盖同目录。

## 0.3 连写英文：调研依据与实现约定

状态：2026-11-09完成0.3最终R2构建、安装及有限自有RichEdit验证，具体动作/计数/凭据升级与CLI服务结果见INPUT_VALIDATION。后文调研和数据基线保留为设计依据，不替代真实应用覆盖。

### 成熟方案与可复用边界

| 来源 | 可借鉴或复用的内容 | 取舍 |
| --- | --- | --- |
| [Gboard 官方解码说明](https://research.google/blog/the-machine-intelligence-behind-gboard/) | 词典 FST 允许省略词间空格，结合 n-gram 上下文与 beam search 保留并排序多条路径 | 采用其联合判断词界与单词的思路；不是简单最长匹配，也不保证短前缀唯一确定 |
| [Gboard 2024 原始论文](https://aclanthology.org/2024.emnlp-industry.93/) | 将神经语言模型接入增量解码搜索空间 | 作为后续扩展参照；本次未找到完整生产引擎、模型权重与词库的开源发布包，不把论文当成可直接部署的软件 |
| [AOSP LatinIME 解码源码](https://android.googlesource.com/platform/packages/inputmethods/LatinIME/+/refs/heads/main/native/jni/src/suggest/core/suggest.cpp) | 连续搜索、补全、漏空格成本、跨词边及单词/双词评分 | 优先参考算法与搜索状态，保留现有 Windows TSF 壳；Android/JNI 接口不能直接移植。其 [NOTICE](https://android.googlesource.com/platform/packages/inputmethods/LatinIME/+/refs/heads/main/NOTICE) 另列 Lexiteria 词典授权，英文数据不得仅按代码许可直接打包 |
| [SymSpell](https://github.com/wolfgarbe/SymSpell) | WordSegmentation、LookupCompound，以及公开的 unigram/bigram 数据 | 复用现有单词频率索引，研究双词表与缺空格成本；整串分词 API 不能直接代替允许尾词尚未输完的增量输入法 |
| [Mozc 产品与 OSS 差异](https://github.com/google/mozc/blob/master/docs/about_branding.md) | 日文输入法的候选状态与多路径设计 | 不是现成英文解码器；Google 内部系统词典、搭配和纠错数据不等同于 OSS 数据 |

默认 Unicode 词界规则处理字符、标点等边界，不会单独解决 `iamt` 的意图判断；这里需要词典与语言概率模型。参见 [UAX #29](https://www.unicode.org/reports/tr29/)。

### 数据基线

0.2安装词库有单词频率和英汉注释，没有英文bigram模型；0.3已将冻结双词资源独立编译并内嵌最终DLL。ECDICT的多词词条不直接充当上下文概率。

调研隔离目录已取得与现有频率词库同一 SymSpell 提交 `c239062ae02961df18ab7da1671d01b4388204e0` 的 [frequency_bigramdictionary_en_243_342.txt](https://github.com/wolfgarbe/SymSpell/blob/c239062ae02961df18ab7da1671d01b4388204e0/SymSpell/frequency_bigramdictionary_en_243_342.txt)：5,137,213 字节，SHA256 `fd892a160184101dd7ae807ac5a302d01fcea1c47304181a8ed7ed9c94545bcd`。实际记录数为 242,342，不按文件名推断数量。它覆盖 `i am`、`am trying`、`trying to`；`am` 后以 `t` 开头的高频结果中也有 `to`、`the` 等，因此不能声称仅凭 `iamt` 就唯一知道用户想输入 trying。

单词表与双词表的原始计数不在同一标度，不能直接用双词计数除以当前单词计数当作条件概率；实现时需按同源双词行和归一化、平滑并回退到单词模型。

固定提交的 [代码 LICENSE](https://github.com/wolfgarbe/SymSpell/blob/c239062ae02961df18ab7da1671d01b4388204e0/LICENSE) 为 MIT；上游发布文件按该仓库声明冻结，完整许可和准确来源已单独保留。其 [README 来源说明](https://github.com/wolfgarbe/SymSpell/blob/c239062ae02961df18ab7da1671d01b4388204e0/README.md#frequency-dictionary) 的 Google Books 与 SCOWL 交集针对 unigram，未建立 bigram 的精确语料版本及生成链。因此不将本资源描述成已证实的 Google V3 或 SCOWL 重生成物，也不把其他数据许可直接推给该资源；来源边界以新 NOTICE 和 SOURCES.json 为准。

### 输入与候选规则

1. 每次按键同时保留“完整单词/单词前缀”和“若干已识别词＋未完成尾词”等解释，用词频、相邻词偏好及切分/补全成本排序；禁止一遇到短词就固定切分。
2. `ia` 可保留整体前缀与 `I` 加 `a…` 两条解释；继续输入 `iamt` 时，`I am` 加 `t…` 应成为可用路径，并可提供 trying、trying to 等候选，不承诺它们在任意上下文都排名第一。
3. 同一组字内已识别的前缀暂不提前提交，整个原始按键串保留到确认；退格后重新解码，不能因过早写入 `I am` 而无法撤回词界判断。
4. 候选主词仅显示当前待补部分。例如完整提交文字为 `I am trying to`，候选显示文字为 `trying to`；没有独立词组词典项时显示“短语补全”，不挪用trying等组成词的词性/释义。已在宿主组合范围外确认的文字不能再次包含在提交文字中。
5. 数字、Tab、点击与 UI-less Finalize 统一使用完整提交文字替换当前组合一次；显示字段不参与正文提交。异步拼写建议也须补齐本组合的前缀，并按代次丢弃过期结果。
6. 保留原样输入通道；普通单词、手动词条、大小写、缩写、撇号和拼音查英需分别回归。Windows 设置中“连写智能分词”和“短语候选”默认开启，对应 EnglishSegmentationDisabled/EnglishPhrasesDisabled；共享 KernelConfig 两项默认false，其他前端须支持显示/提交分离后显式启用。
7. 本地有限搜索负责逐键候选，不逐键调用 DeepSeek，不自动学习输入历史。连写最大输入长度48字符，超过时回到普通单词补全，不截断原文；Esc保留原始串。0.4的Enter原样提交、Space按自动词前空格及exact分词规则处理，替代0.3的未选择Space约定。

### 0.3 已测范围与后续覆盖

最终自有RichEdit已核对ia双路径、iamt选词无重复、退格回iambic、拼音apple、Esc原串和翻页helped，document_text精确为 `I am trying to iambic apple iamt helped`。双架构正式构建和安装探针通过；早期0.3设置exe的两开关off/on保存已核对，最终不同摘要exe只重新读取两项on并截图。production CLI真实AI已通过；本轮未重新做Notepad AI GUI、全部应用、全部控件、卸载或重启删除。

- 用不止 `iamt` 的样例覆盖 `howarey`、`thanky`、`wearet`、完整词如 `international`、未知专名、撇号、混合大小写、个人词条及拼音冲突；区分允许的多解与禁止的强拆。
- 检查 `ia → iamt → iamtr` 的候选更新与连续退格；英文完整词的候选质量不得因增加分词明显退化。
- 候选显示 `trying to` 时，1–5、Tab、点击和 UI-less 确认都只得到一次完整文本；Esc、焦点变化、空格和标点不丢失原始输入。
- 独立测量真实词库上的候选覆盖、误切分、每键 p50/p95、最坏长度和内存；不把外部项目的性能数字当成本项目实测。
- 本轮已完成x64/x86正式冻结构建、保留凭据升级、24项摘要与build.json核对及自有RichEdit有限上屏；后续版本仍须分别绑定自己的产物和宿主证据，不能借用本轮或纯算法测试替代。

## 后续版本

当前句自动识别、词级修改对照、本地下一词预测、短语快捷输入、更多主题和发布签名按独立验收项推进。未实现功能不以可操作开关展示。开发版验证结果不得替代全部宿主兼容或用户验收。
