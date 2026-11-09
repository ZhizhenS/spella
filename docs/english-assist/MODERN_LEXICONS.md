# 现代词和现代专名词库维护

0.6 的两条数据流彼此独立：现代普通词按具体义项提供词性、中文和用法；Wikidata 专名保留实体身份、类别和简短描述。它们补充原有 ECDICT、CC-CEDICT 和万象词库，并不代表全量现代英语或实时热度。

## 现代词义项

[审核 manifest](../../data/dict/modern-lexicon.json) 由 [Kaikki 原始来源](../../data/dict/raw/kaikki-modern/SOURCES.json) 和中文来源记录生成。源是 Wiktionary 贡献者的词典文本，选择 CC BY-SA 4.0 授权；完整版授权和变更说明分别随安装包提供。解析器和本项目编译代码的 MIT 许可不改变数据许可。不引入 Urban Dictionary 数据，不打包例句、引文、音频或图片。

只把有来源中文或经项目审核的中文覆盖义项编入运行数据。项目补充与上游中文分开标记，不能把英文短释义直接称作现成中文翻译。原始选中 sense 数、真正有中文的安装 sense 数和去重键数分别记账。排序依据审核 priority、上游语域/旧义标签和稳定源序，不是实时社交热度。

当前冻结来源的版本身份是完整下载的 SHA-256、ETag 和 HTTP Last-Modified，不把一次网页访问日期当作词典发行版本。两份源的 Last-Modified 都是 2026-10-30；确切信息分别见 [英文来源记录](../../data/dict/raw/kaikki-modern/SOURCES.json) 与 [中文来源记录](../../data/dict/raw/kaikki-modern/ZH-SOURCES.json)。

| 来源 | 完整 gzip / 扫描行数 | 其中英文条目 | 投影条目 / 义项 | 入库投影 gzip |
| --- | --- | --- | --- | --- |
| Kaikki 英文版全语言 raw | 3,082,476,978 字节 / 11,121,531 行 | 1,499,161 | 73,576 / 98,586 | 12,197,678 字节 |
| Kaikki 中文版 extract | 234,450,611 字节 / 2,965,486 行 | 97,019 | 3,126 / 4,822 | 496,926 字节 |

这些投影只是待筛选源，不能称作安装词数。经过中文准入、关系释义排除、词性检查、去重及同缩写全称的跨来源合并后，当前运行数据为 **4,009 义项、2,134 个键**，620,800 字节；SHA-256 为 `ccbb70532631d3cdc000ff63fd702b6d496e09d1b4f024e11a9670d4bfaa9749`。逐义审核记录和过滤计数在审核 manifest 中；大体积完整下载保留在独立采集缓存，源码只带可复现编译所需的投影和来源摘要。

EAMODLX1 是 9 列 TSV：`key`、`text`、`pos`、`definition_zh`、`usage`、`full_form`、`source_url`、`sense_id`、`priority`。key 是移除非字母后的 2–64 个小写 ASCII 字母；候选 text 保留真实拼写。普通词按输入的大小写惯例处理，明确缩写和专名保留规范大小写。缩写的全称是注释信息，选中 `IDK` 仍提交 `IDK`，不会自动提交全称；Space/Enter 也不因看到全称而自动展开。

词典文本选择 CC BY-SA 4.0，原 Wiktionary 也提供 GFDL 替代条款；此分发按 CC BY-SA 4.0 保存完整授权、贡献者归属、词条链接和改动说明。英文版/中文版来源和项目中文覆盖分别标记。Wiktextract 解析器及本项目代码的 MIT 许可只适用于代码，不替代词典数据许可。

## Wikidata 专名

[审核 catalog](../../data/dict/modern-entities.json) 和 [来源快照](../../data/dict/raw/modern-entities/) 使用 CC0 1.0；完整版授权随包安装。按公开有界类型查询发现有中英文标签的编程语言、软件、公司等实体，再以每批最多 50 个 ID 取真实结构化字段。保存 QID、修订号、修改时间、英文/中文标签与描述、别名、分类、官网声明和 sitelink 数。不是从名字编造 QID，也不是整库 dump。

新增条目必须有真正中文标签或描述，优先采用中文描述；无描述时用“真实中文标签（类别）”，明确记录 origin。原 51 条经项目审核的中文说明、拼写和来源修订保留优先。描述是实体元数据，不承诺完整定义。官网缺失则留空，来源始终可以追溯到精确 Wikidata 修订。

EAMODEN2 是 8 列有界 TSV：text、aliases、kind、definition_zh、QID、revision、可空官网、priority。不同实体允许同别名，QID 不重复。每行别名最多 32 个、每个规范键 2–64 个 ASCII 字母；最多 20,000 行、200,000 别名、16 MiB。按首别名、priority 降序、QID 字符串稳定排序；运行时另建不可变别名索引，不依赖别名唯一。

priority = min(sitelinks, 65535) × 100 + 类别偏好（编程语言30/软件25/公司20/组织15/产品10/网站5）+ 原审核种子50。这是实体流内部的覆盖度/知名度代理，不是实际打字词频，不是把专名优先级套在普通词义上。

## 更新与验证

1. 明确作者操作采集公共数据到研究目录，保存实际查询、响应摘要与筛选边界。服务超时不等于空结果，不把失败查询写成已覆盖类别。
2. `modern-entities.py --import-research <目录> --output-proposal <新目录>` 只生成独立提案；逐批核查来源与过滤统计、同名多义、中文来源及原 51 条保护后才替换审核输入。
3. 现代词作者刷新使用 `build-modern-lexicon.py --write-runtime --refresh-manifest`；只对已审核来源/中文覆盖运行。普通构建必须是 `--check`，不能在失败时隐式刷新摘要。
4. 分别运行两个 producer 的隔离 fixture 与离线 `--check`；构建在 Cargo 前执行这些门禁。`build.json` 分别记录 approved JSON、内嵌 TSV 和源摘要，正式 DLL 在后续测试前冻结。
5. 来源 ZIP 应包含所有投影 raw、完整授权、NOTICE、审核覆盖和编译脚本。用隔离解压的源码重跑离线检查与正式构建；不能以安装包哈希相同推导源码重编译已经验证。

以下是实际作者命令，需从仓库根目录执行；仅文档示例，正常构建不会调用下载或全量扫描。为英文和中文使用独立缓存目录，投影先写入新的提案目录，不直接覆盖审核 raw：

```powershell
python tools/english-assist/build-modern-lexicon.py --download-source <cache-en> --edition en
python tools/english-assist/build-modern-lexicon.py --download-source <cache-zh> --edition zh
python tools/english-assist/build-modern-lexicon.py --extract-full <cache-en>/raw-wiktextract-data.jsonl.gz --download-metadata <cache-en>/download.json --snapshot-output <proposal-raw> --edition en
python tools/english-assist/build-modern-lexicon.py --extract-full <cache-zh>/zh-extract.jsonl.gz --download-metadata <cache-zh>/download.json --snapshot-output <proposal-raw> --edition zh
```

下载保存 `download.json`、源页面和分段版本记录；完整提取会核对 gzip 大小与 SHA-256，并记录解压后完整内容摘要。提案审核后再显式迁入版本控制的 raw、维护中文覆盖并运行下列命令；只有最后两项用于日常只读验证：

```powershell
python tools/english-assist/build-modern-lexicon.py --write-runtime --refresh-manifest
python tools/english-assist/test-build-modern-lexicon.py
python tools/english-assist/build-modern-lexicon.py --check
```

上游下载地址提供的是当时可用文件，未来重复下载不保证得到本次相同版本；必须比较来源摘要，不能以新文件静默覆盖旧审核摘要。改变提取/准入/中文覆盖规则也属于需要审核的变更。

输入期间不联网查词、不逐键学习，也不现场翻译。Space 的原文保护、Enter 原文提交和显式选择候选的行为不因数据扩充改变；少见词、最新词以及所有义项均不保证进入有限候选页。
