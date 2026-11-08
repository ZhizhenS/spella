# English Assist 英文助手（开发分支）

本项目基于 [Retype 固定版本 902e35e](https://github.com/L-Chris/retype/tree/902e35e06ee4f744320cf995c39b1f64693034cf)
开发 Windows 英文输入法，目标是在英语键盘列表中提供本地单词补全、拼写建议，
以及主动触发的 AI 语法修正和句子补全。

[English](README.md) · [英文输入操作](docs/english-input.md) ·
[P0 验证记录](docs/english-assist/INPUT_VALIDATION.md)

**当前状态：源码实现中，尚未完成本分支的编译、安装和真实应用验收。**
上游安装包、历史测试报告不等于英文助手的交付或验收结果。

## 已修改源码，等待验证

- 独立的 English Assist 英文助手系统入口，绑定英语（美国）。
- 本地前缀补词、拼写建议和大小写保留。
- 数字 0–9 正常输入，英文候选不显示数字选词编号。
- Tab 和鼠标选词默认不加空格。
- 空格保留原词；只有显式选中候选后，空格才接受该候选并加空格。
- 显式选择后的第一次 Enter 只选词，第二次 Enter 才执行应用的回车行为。
- 英文离线词库随专用安装包分发；产品标识、用户数据路径与上游隔离。
- 本版关闭自动英文词汇学习。
- 中文设置与显式 AI 编辑的协议、服务及交互正在开发。

主题完善、手动个人词库、可选下一词预测和完整 AI 编辑验收仍属后续工作。

## 构建与安装状态

需要 Rust MSVC 工具链、Visual C++ 构建工具、Windows SDK 和
i686-pc-windows-msvc 目标。生成安装包还需要 Inno Setup 6。

~~~powershell
.\platforms\windows\installer\build-english-assist.ps1 -Installer
~~~

英文专用脚本构建 x64/x86 输入组件、设置程序、后台组件和固定版本英文词库，
并执行其配置的检查；该脚本本身不安装产品，也不修改系统键盘列表。

成功构建后预期安装包为 dist/english-assist-0.1.0-windows-x64-setup.exe。
目前尚无成功构建结果。不要使用继承的 Retype 安装器、register.ps1
或上游发布页安装包来替代本项目构建。

安装后的验收目标是在“设置 → 时间和语言 → 语言和区域 → 英语（美国）
→ 语言选项 → 键盘”中看到本项目，用 Win+Space 选择，且保留原默认键盘。
这些仍需按 P0 真实验证。

## 键盘操作

直接输入 python3 即可。Tab 接受候选且不补空格；上下键显式选中后，
空格接受候选并加空格。未显式选择时，空格保留原词。

显式选词后的 Enter 只确认候选，避免同时发送聊天或提交搜索；再次 Enter
才交给应用。未显式选词的 Enter 保留应用原有行为，Esc 保留原输入。

完整语义和未通过的验收项目见 [英文输入](docs/english-input.md)
和 [P0 验证记录](docs/english-assist/INPUT_VALIDATION.md)。

## 许可与来源

继承的代码使用 MIT，保留 LICENSE 和 NOTICE.txt。
英文数据另有 SymSpell、SCOWL 和 Google Ngram 的许可及署名，
位于 data/dict/raw/english/，不能把词库统一当成 MIT。

源码目录保留上游中文、其他平台与同步模块，用于后续整理和必要兼容；
这不代表英文专用产品会安装或提供这些功能。
