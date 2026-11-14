# 架构说明

本文件描述 encoding-converter 的模块划分与实现状态，供后续开发（含 AI）对照。

## 技术栈

- Rust 2021，GUI 用 `egui/eframe`（单二进制、无运行时依赖，Windows/Linux 同一份代码）
- 字符编码：`encoding_rs`（WHATWG 全量编码表）+ 自研 codec（UTF-32/UTF-7/CESU-8/EBCDIC 等）
- 编码探测：`chardetng`（Firefox 同款）+ BOM 嗅探 + 统计打分

## 目录结构

```
src/
├── main.rs            # 入口：无参数启动 GUI；带参数走 CLI
├── app.rs             # egui 主界面与状态
├── ui/                # 各功能页
│   ├── radix.rs       # 进制转换页
│   ├── base.rs        # Base-N 编码页
│   ├── charset.rs     # 字符编码页
│   ├── batch.rs       # 批量文件转码页（拖拽）
│   └── detect.rs      # 编码探测页
├── core/
│   ├── radix.rs       # 任意进制 2-36、补码、浮点位视图
│   ├── numeral.rs     # 特殊数制：罗马、六十进制、双射 26、平衡三进制等
│   ├── base_n.rs      # Base16/32/58/62/64/85/91、Bech32
│   ├── escape.rs      # URL/HTML/Unicode/JSON 转义
│   ├── morse.rs       # 摩尔斯电码
│   ├── charset/       # 编码注册表与 codec 实现
│   │   ├── mod.rs     # EncodingInfo 列表、find/decode/encode
│   │   └── custom.rs  # 自研 codec
│   └── detect.rs      # 编码探测
└── batch.rs           # 批量任务执行（读文件 → 探测 → 转码 → 写文件）
```

## 核心接口约定

- `core::charset::list() -> &'static [EncodingInfo]`：注册表，UI 与 CLI 共用
- `core::charset::decode(&EncodingInfo, &[u8]) -> DecodeResult`：带错误位置
- `core::charset::encode(&EncodingInfo, &str) -> EncodeResult`：带无法映射的字符
- `core::detect::detect(&[u8]) -> Detection`：返回候选编码与置信度
- `batch::run(&BatchJob) -> BatchReport`：批量转码，逐文件状态

## 实现状态

| 模块 | 状态 |
|---|---|
| 项目骨架 / GUI 窗口 | ✅ 已完成（六个页签可切换） |
| 进制转换（2-36） | ✅ 已完成（含小数、大整数、分隔符） |
| 特殊数制 | ✅ 已完成（罗马/双射26/六十/平衡三/负二/阶乘/Zeckendorf/玛雅） |
| Base-N 编码 | ✅ 已完成（18 种，含往返测试） |
| 转义编码 | ✅ 已完成（URL/HTML/JSON/Unicode/C 字符串/Quoted-Printable） |
| 摩尔斯电码 | ✅ 已完成 |
| 字符编码注册表（encoding_rs 部分） | ✅ 已完成（约 45 种 WHATWG 编码） |
| 自研 codec | ✅ 已完成（UTF-32/UCS-2/CESU-8/MUTF-8/WTF-8/Latin-1/ASCII） |
| 自动编码探测 | ✅ 已完成（BOM + UTF-16 特征 + chardetng 打分） |
| 批量文件转码 | ✅ 已完成（拖拽、探测、输出目录、试运行） |
| 表驱动传统编码（OEM/EBCDIC 等） | ⬜ 规划中 |
| UTF-7 / SCSU / BOCU-1 / HZ-GB-2312 等 | ⬜ 规划中 |
| 批量转码后台线程化 | ⬜ 规划中 |

