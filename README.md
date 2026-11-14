# encoding-converter

进制 / Base-N / 字符编码转换器：Windows 与 Linux 桌面工具，Rust + egui 实现，编译产物是单文件可执行程序（无运行时依赖）。

## 功能

**进制与数制**
- 任意 2–36 进制互转：支持任意长度整数、小数、负号、`0x/0b/0o/0d` 前缀、`_`/空格/逗号分隔
- 二进制补码解释（8/16/32/64 位）
- IEEE-754 单精度 / 双精度分解视图（符号位、指数、尾数、十六进制）
- 特殊数制：罗马数字、双射 26 进制（Excel 列名）、六十进制、平衡三进制、负二进制、阶乘进制、Zeckendorf（斐波那契）、玛雅数字

**Base-N 编码（18 种）**
Base16、Base32、Base32hex、z-base-32、Crockford Base32、Base58（Bitcoin/Flickr/Ripple）、Base62、Base64、Base64URL、Base64 MIME、Ascii85、Z85、RFC 1924、Base91、UUencode、XXencode

**字符编码**
- encoding_rs 全量 WHATWG 编码表：UTF-8、UTF-16LE/BE、GBK、GB18030、Big5、Shift_JIS、EUC-JP、ISO-2022-JP、EUC-KR、Windows-1250–1258、Windows-874、ISO-8859-2…16、KOI8-R/U、MacRoman、MacCyrillic、IBM866
- 自研编码器：UTF-32LE/BE（含自动 BOM）、UCS-2LE/BE、CESU-8、Modified UTF-8、WTF-8、ISO-8859-1（真 Latin-1）、US-ASCII
- 文本 ⇄ 字节双向转换，字节十六进制视图，BOM 输出选项，错误字节偏移提示

**转义 / 传输编码**
URL 百分号、HTML 实体、HTML 数字引用（十进制/十六进制）、JSON 字符串、Unicode 转义、C 字符串、Quoted-Printable、摩尔斯电码

**编码探测**
BOM → ASCII → 无 BOM UTF-16 特征 → UTF-8 校验 → chardetng 统计模型，输出候选列表与置信度

**批量文件转码**
拖拽文件/文件夹进窗口加入队列，自动探测或指定源编码，按目标编码批量转码；支持输出目录、后缀、覆盖开关、试运行、BOM 保留，逐文件状态与汇总

**命令行**
`detect` / `convert` / `batch` 三个子命令，便于脚本调用

## 尚未实现（规划中）

- 表驱动的传统单字节编码：DOS OEM（CP437/850/852…）、EBCDIC、MacJapanese/MacKorean、VISCII、TCVN 等
- UTF-7、SCSU、BOCU-1、ISO-2022-CN、HZ-GB-2312、EUC-TW、Johab
- 批量转码目前在 UI 线程同步执行，大文件时会阻塞刷新（后续改为后台线程 + 进度回调）
- 界面设置持久化（保存上次使用的编码）

## 打开方式

### 从源码运行（开发）

```bash
cargo run                 # 启动图形界面
cargo run -- --help       # 命令行帮助
cargo test                # 运行单元测试
```

需要 Rust 1.80+；Windows 依赖 MSVC 工具链，Linux 需要 X11/Wayland 开发库（如 Ubuntu：`sudo apt install libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev`）。

### 打包

```bash
cargo build --release
```

产物：
- Windows：`target/release/encoding-converter.exe`
- Linux：`target/release/encoding-converter`

单文件、无运行时依赖；Linux 版本需在 Linux 机器（或交叉编译环境）上构建。

### 命令行示例

```bash
encoding-converter detect note.txt
encoding-converter convert --from gbk --to utf-8 note.txt -o note.utf8.txt
encoding-converter batch --to utf-8 --out ./utf8 ./downloads
encoding-converter batch --to utf-8 --out ./utf8 --overwrite ./downloads   # 覆盖已存在的输出
```

`batch` 默认**不覆盖**已存在的输出文件（会显示为“跳过”），需要覆盖时显式加 `--overwrite`。

## 本地存储

- 当前版本**不写配置文件、不联网、不上报任何数据**。
- 批量转码只写入用户指定的输出目录（或原目录的副本）；GUI 覆盖已有文件需显式勾选，CLI 需显式加 `--overwrite`。
- 计划中的设置持久化会写到系统配置目录（`~/.config/encoding-converter/settings.json`），当前尚未实现。

## 开发约定

- AI 编码遵循 [AGENTS.md](./AGENTS.md)（约定式提交、改动配测试、写后审计）。
- 模块划分与实现状态见 [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md)。

## 自动推送

- 本机已安装 `.git/hooks/post-commit`：每次提交后自动通过 SSH `git push` 到 `origin`。
- 推送日志见 `.git/autopush.log`；不需要自动推送时，删除该钩子文件即可。

## 远程仓库

- GitHub：https://github.com/AlanLumier/encoding-converter
- origin：`git@github.com:AlanLumier/encoding-converter.git`（SSH）
