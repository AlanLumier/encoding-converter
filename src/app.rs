//! egui 界面：六个功能页（进制 / Base-N / 字符编码 / 转义 / 批量文件 / 编码探测）。

use std::path::PathBuf;

use eframe::egui::{self, ComboBox, RichText, TextEdit};

use crate::batch::{self, BatchItem, BatchOptions, ItemStatus, SourceEncoding};
use crate::core::base_n::BaseN;
use crate::core::charset::{self, EncodingInfo, Group};
use crate::core::detect::{self, Detection};
use crate::core::escape::EscapeKind;
use crate::core::{morse, numeral, radix};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Radix,
    BaseN,
    Charset,
    Escape,
    Batch,
    Detect,
}

impl Tab {
    const ALL: [Tab; 6] = [
        Tab::Radix,
        Tab::BaseN,
        Tab::Charset,
        Tab::Escape,
        Tab::Batch,
        Tab::Detect,
    ];

    fn label(self) -> &'static str {
        match self {
            Tab::Radix => "进制",
            Tab::BaseN => "Base-N",
            Tab::Charset => "字符编码",
            Tab::Escape => "转义 / 符号",
            Tab::Batch => "批量文件",
            Tab::Detect => "编码探测",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    Encode,
    Decode,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NumeralSystem {
    Roman,
    Bijective26,
    Sexagesimal,
    BalancedTernary,
    Negabinary,
    Factorial,
    Zeckendorf,
    Mayan,
}

impl NumeralSystem {
    const ALL: [NumeralSystem; 8] = [
        NumeralSystem::Roman,
        NumeralSystem::Bijective26,
        NumeralSystem::Sexagesimal,
        NumeralSystem::BalancedTernary,
        NumeralSystem::Negabinary,
        NumeralSystem::Factorial,
        NumeralSystem::Zeckendorf,
        NumeralSystem::Mayan,
    ];

    fn label(self) -> &'static str {
        match self {
            NumeralSystem::Roman => "罗马数字",
            NumeralSystem::Bijective26 => "双射 26 进制（Excel 列）",
            NumeralSystem::Sexagesimal => "六十进制",
            NumeralSystem::BalancedTernary => "平衡三进制",
            NumeralSystem::Negabinary => "负二进制",
            NumeralSystem::Factorial => "阶乘进制",
            NumeralSystem::Zeckendorf => "Zeckendorf（斐波那契）",
            NumeralSystem::Mayan => "玛雅数字",
        }
    }
}

pub struct App {
    tab: Tab,

    radix_input: String,
    radix_from: u32,
    radix_to: u32,
    radix_output: String,
    radix_error: Option<String>,
    numeral_input: String,
    numeral_system: NumeralSystem,
    numeral_output: String,
    numeral_error: Option<String>,
    tc_input: String,
    tc_bits: u32,
    tc_output: String,
    float_input: String,
    float_single: bool,
    float_view: Option<radix::FloatView>,

    base_kind: BaseN,
    base_direction: Direction,
    base_input: String,
    base_output: String,
    base_error: Option<String>,

    cs_input: String,
    cs_source: String,
    cs_target: String,
    cs_direction: Direction,
    cs_output: String,
    cs_error: Option<String>,
    cs_bytes: Vec<u8>,
    cs_bom: bool,

    esc_kind: EscapeKind,
    esc_direction: Direction,
    esc_input: String,
    esc_output: String,
    esc_error: Option<String>,

    batch_items: Vec<BatchItem>,
    batch_target: String,
    batch_source_auto: bool,
    batch_source: String,
    batch_output_dir: String,
    batch_suffix: String,
    batch_overwrite: bool,
    batch_dry_run: bool,
    batch_keep_bom: bool,
    batch_progress: (usize, usize),

    detect_file: Option<String>,
    detect_result: Option<Detection>,

    status: String,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_cjk_font(&cc.egui_ctx);
        cc.egui_ctx.set_visuals(egui::Visuals::light());
        Self {
            tab: Tab::Radix,
            radix_input: "255".into(),
            radix_from: 10,
            radix_to: 16,
            radix_output: String::new(),
            radix_error: None,
            numeral_input: "2024".into(),
            numeral_system: NumeralSystem::Roman,
            numeral_output: String::new(),
            numeral_error: None,
            tc_input: "FF".into(),
            tc_bits: 8,
            tc_output: String::new(),
            float_input: "1.5".into(),
            float_single: true,
            float_view: None,
            base_kind: BaseN::Base64,
            base_direction: Direction::Encode,
            base_input: "你好，编码转换器".into(),
            base_output: String::new(),
            base_error: None,
            cs_input: "你好，编码转换器".into(),
            cs_source: "utf-8".into(),
            cs_target: "gbk".into(),
            cs_direction: Direction::Encode,
            cs_output: String::new(),
            cs_error: None,
            cs_bytes: Vec::new(),
            cs_bom: false,
            esc_kind: EscapeKind::UrlPercent,
            esc_direction: Direction::Encode,
            esc_input: "你好 world".into(),
            esc_output: String::new(),
            esc_error: None,
            batch_items: Vec::new(),
            batch_target: "utf-8".into(),
            batch_source_auto: true,
            batch_source: "gbk".into(),
            batch_output_dir: String::new(),
            batch_suffix: String::new(),
            batch_overwrite: false,
            batch_dry_run: false,
            batch_keep_bom: false,
            batch_progress: (0, 0),
            detect_file: None,
            detect_result: None,
            status: "就绪".into(),
        }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        if dropped.is_empty() {
            return;
        }
        match self.tab {
            Tab::Batch => {
                let mut items = batch::scan(&dropped, false);
                self.batch_items.append(&mut items);
                self.status = format!("已加入 {} 个文件", self.batch_items.len());
            }
            Tab::Detect => {
                if let Some(path) = dropped.first() {
                    self.run_detect(path);
                }
            }
            _ => {
                if let Some(path) = dropped.first() {
                    self.status = format!("文件：{}", path.display());
                }
            }
        }
    }

    fn run_detect(&mut self, path: &std::path::Path) {
        match std::fs::read(path) {
            Ok(bytes) => {
                let detection = detect::detect(&bytes);
                self.detect_file = Some(path.display().to_string());
                self.detect_result = Some(detection);
                self.status = format!("已分析 {} 字节", bytes.len());
            }
            Err(err) => {
                self.detect_file = Some(path.display().to_string());
                self.detect_result = None;
                self.status = format!("读取失败：{err}");
            }
        }
    }

    fn radix_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("进制转换");
        ui.label("支持 2–36 进制、任意长度整数与小数、补码与 IEEE-754 视图。");

        ui.horizontal(|ui| {
            ui.label("输入：");
            ui.add(TextEdit::singleline(&mut self.radix_input).desired_width(240.0));
            ui.label("从");
            ui.add(egui::DragValue::new(&mut self.radix_from).range(2..=36));
            ui.label("进制 →");
            ui.add(egui::DragValue::new(&mut self.radix_to).range(2..=36));
            ui.label("进制");
            if ui.button("转换").clicked() {
                match radix::convert(&self.radix_input, self.radix_from, self.radix_to, 32) {
                    Ok(out) => {
                        self.radix_output = out;
                        self.radix_error = None;
                    }
                    Err(err) => {
                        self.radix_error = Some(err);
                        self.radix_output.clear();
                    }
                }
            }
        });
        if let Some(err) = &self.radix_error {
            ui.colored_label(egui::Color32::RED, err.as_str());
        }
        ui.horizontal(|ui| {
            ui.label("结果：");
            ui.add(
                TextEdit::singleline(&mut self.radix_output)
                    .desired_width(360.0)
                    .interactive(false),
            );
            if ui.button("复制").clicked() {
                ui.ctx().copy_text(self.radix_output.clone());
                self.status = "已复制结果".into();
            }
        });

        ui.separator();
        ui.horizontal(|ui| {
            ui.label("补码解释（十六进制输入）：");
            ui.add(TextEdit::singleline(&mut self.tc_input).desired_width(120.0));
            ComboBox::from_id_salt("tc-bits")
                .selected_text(format!("{} 位", self.tc_bits))
                .show_ui(ui, |ui| {
                    for bits in [8u32, 16, 32, 64] {
                        ui.selectable_value(&mut self.tc_bits, bits, format!("{bits} 位"));
                    }
                });
            if ui.button("解释").clicked() {
                self.tc_output =
                    match radix::twos_complement_decimal(&self.tc_input, 16, self.tc_bits) {
                        Ok(v) => format!("有符号十进制：{v}"),
                        Err(err) => format!("错误：{err}"),
                    };
            }
            ui.label(RichText::new(&self.tc_output).monospace());
        });

        ui.horizontal(|ui| {
            ui.label("浮点视图：");
            ui.add(TextEdit::singleline(&mut self.float_input).desired_width(120.0));
            ui.checkbox(&mut self.float_single, "单精度");
            if ui.button("查看 IEEE-754").clicked() {
                self.float_view = self
                    .float_input
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .map(|v| radix::ieee754(v, self.float_single));
                if self.float_view.is_none() {
                    self.status = "浮点解析失败".into();
                }
            }
        });
        if let Some(view) = &self.float_view {
            egui::Grid::new("float-grid").num_columns(2).show(ui, |ui| {
                ui.label("二进制");
                ui.monospace(view.binary.as_str());
                ui.end_row();
                ui.label("十六进制");
                ui.monospace(view.hex.as_str());
                ui.end_row();
                ui.label("符号位");
                ui.monospace(view.sign.to_string());
                ui.end_row();
                ui.label("指数");
                ui.monospace(view.exponent.as_str());
                ui.end_row();
                ui.label("尾数");
                ui.monospace(view.mantissa.as_str());
                ui.end_row();
            });
        }

        ui.separator();
        ui.heading("特殊数制");
        ui.horizontal(|ui| {
            ui.add(TextEdit::singleline(&mut self.numeral_input).desired_width(160.0));
            ComboBox::from_id_salt("numeral-system")
                .selected_text(self.numeral_system.label())
                .show_ui(ui, |ui| {
                    for system in NumeralSystem::ALL {
                        ui.selectable_value(&mut self.numeral_system, system, system.label());
                    }
                });
            if ui.button("十进制 → 特殊表示").clicked() {
                match self.convert_numeral() {
                    Ok(out) => {
                        self.numeral_output = out;
                        self.numeral_error = None;
                    }
                    Err(err) => {
                        self.numeral_error = Some(err);
                        self.numeral_output.clear();
                    }
                }
            }
        });
        if let Some(err) = &self.numeral_error {
            ui.colored_label(egui::Color32::RED, err.as_str());
        }
        if !self.numeral_output.is_empty() {
            ui.horizontal(|ui| {
                ui.label("结果：");
                ui.monospace(self.numeral_output.as_str());
            });
        }
    }

    fn convert_numeral(&self) -> Result<String, String> {
        let text = self.numeral_input.trim();
        match self.numeral_system {
            NumeralSystem::Roman => {
                let n: u32 = text.parse().map_err(|_| "请输入十进制整数（1–3999）")?;
                numeral::roman_encode(n).ok_or_else(|| "罗马数字只支持 1–3999".to_string())
            }
            NumeralSystem::Bijective26 => {
                let n: u64 = text.parse().map_err(|_| "请输入正整数")?;
                numeral::bijective26_encode(n).ok_or_else(|| "数值必须大于 0".to_string())
            }
            NumeralSystem::Sexagesimal => {
                let n: u64 = text.parse().map_err(|_| "请输入非负整数")?;
                Ok(numeral::sexagesimal_encode(n))
            }
            NumeralSystem::BalancedTernary => {
                let n: i64 = text.parse().map_err(|_| "请输入整数")?;
                Ok(numeral::balanced_ternary_encode(n))
            }
            NumeralSystem::Negabinary => {
                let n: i64 = text.parse().map_err(|_| "请输入整数")?;
                Ok(numeral::negabinary_encode(n))
            }
            NumeralSystem::Factorial => {
                let n: u64 = text.parse().map_err(|_| "请输入非负整数")?;
                Ok(numeral::factorial_encode(n))
            }
            NumeralSystem::Zeckendorf => {
                let n: u64 = text.parse().map_err(|_| "请输入非负整数")?;
                Ok(numeral::zeckendorf_encode(n))
            }
            NumeralSystem::Mayan => {
                let n: u64 = text.parse().map_err(|_| "请输入非负整数")?;
                Ok(numeral::mayan_encode(n))
            }
        }
    }

    fn base_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Base-N 编码");
        ui.horizontal(|ui| {
            ComboBox::from_id_salt("base-kind")
                .selected_text(self.base_kind.name())
                .show_ui(ui, |ui| {
                    for kind in BaseN::ALL {
                        ui.selectable_value(&mut self.base_kind, kind, kind.name());
                    }
                });
            ui.separator();
            if ui
                .selectable_label(self.base_direction == Direction::Encode, "编码（文本→Base）")
                .clicked()
            {
                self.base_direction = Direction::Encode;
            }
            if ui
                .selectable_label(self.base_direction == Direction::Decode, "解码（Base→文本）")
                .clicked()
            {
                self.base_direction = Direction::Decode;
            }
            if ui.button("执行").clicked() {
                self.run_base();
            }
        });
        ui.label(RichText::new(self.base_kind.note()).small().weak());
        ui.add_space(4.0);
        ui.label("输入：");
        ui.add(
            TextEdit::multiline(&mut self.base_input)
                .desired_rows(6)
                .desired_width(f32::INFINITY)
                .font(egui::TextStyle::Monospace),
        );
        if let Some(err) = &self.base_error {
            ui.colored_label(egui::Color32::RED, err.as_str());
        }
        ui.label("输出：");
        ui.add(
            TextEdit::multiline(&mut self.base_output)
                .desired_rows(6)
                .desired_width(f32::INFINITY)
                .interactive(false)
                .font(egui::TextStyle::Monospace),
        );
        if ui.button("复制输出").clicked() {
            ui.ctx().copy_text(self.base_output.clone());
            self.status = "已复制".into();
        }
    }

    fn run_base(&mut self) {
        self.base_error = None;
        match self.base_direction {
            Direction::Encode => {
                self.base_output = self.base_kind.encode(self.base_input.as_bytes());
            }
            Direction::Decode => match self.base_kind.decode(&self.base_input) {
                Ok(bytes) => {
                    self.base_output = String::from_utf8_lossy(&bytes).to_string();
                    if String::from_utf8(bytes).is_err() {
                        self.base_error = Some("解码结果不是合法 UTF-8，已按替换字符显示".into());
                    }
                }
                Err(err) => {
                    self.base_error = Some(err);
                    self.base_output.clear();
                }
            },
        }
    }

    fn charset_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("字符编码转换");
        ui.horizontal(|ui| {
            if ui
                .selectable_label(self.cs_direction == Direction::Encode, "文本 → 字节")
                .clicked()
            {
                self.cs_direction = Direction::Encode;
            }
            if ui
                .selectable_label(self.cs_direction == Direction::Decode, "字节 → 文本")
                .clicked()
            {
                self.cs_direction = Direction::Decode;
            }
        });
        ui.horizontal(|ui| {
            ui.label("源编码：");
            encoding_combo(ui, "cs-source", &mut self.cs_source);
            ui.label("目标编码：");
            encoding_combo(ui, "cs-target", &mut self.cs_target);
        });
        if self.cs_target.starts_with("utf-") {
            ui.checkbox(&mut self.cs_bom, "输出带 BOM");
        }

        ui.add_space(4.0);
        ui.label(match self.cs_direction {
            Direction::Encode => "输入文本：",
            Direction::Decode => "输入十六进制字节（空格/换行分隔，如 D6 D0）：",
        });
        ui.add(
            TextEdit::multiline(&mut self.cs_input)
                .desired_rows(6)
                .desired_width(f32::INFINITY)
                .font(egui::TextStyle::Monospace),
        );
        if ui.button("执行转换").clicked() {
            self.run_charset();
        }
        if let Some(err) = &self.cs_error {
            ui.colored_label(egui::Color32::RED, err.as_str());
        }
        if !self.cs_output.is_empty() || !self.cs_bytes.is_empty() {
            ui.separator();
            ui.label("结果：");
            ui.add(
                TextEdit::multiline(&mut self.cs_output)
                    .desired_rows(4)
                    .desired_width(f32::INFINITY)
                    .interactive(false)
                    .font(egui::TextStyle::Monospace),
            );
            if !self.cs_bytes.is_empty() {
                ui.label("字节（十六进制）：");
                ui.monospace(hexdump(&self.cs_bytes));
            }
        }
    }

    fn run_charset(&mut self) {
        self.cs_error = None;
        self.cs_output.clear();
        self.cs_bytes.clear();
        let Some(source) = charset::find(&self.cs_source) else {
            self.cs_error = Some("源编码不存在".into());
            return;
        };
        let Some(target) = charset::find(&self.cs_target) else {
            self.cs_error = Some("目标编码不存在".into());
            return;
        };
        match self.cs_direction {
            Direction::Encode => {
                let encoded = charset::encode(target, &self.cs_input);
                let mut payload = encoded.bytes;
                if self.cs_bom {
                    prepend_bom(&mut payload, target.id);
                }
                self.cs_bytes = payload;
                self.cs_output = String::from_utf8_lossy(&self.cs_bytes).to_string();
                if !encoded.unmappable.is_empty() {
                    self.cs_error = Some(format!(
                        "{} 个字符无法用 {} 表示，已跳过：{}",
                        encoded.unmappable.len(),
                        target.name,
                        encoded
                            .unmappable
                            .iter()
                            .take(10)
                            .map(|c| c.to_string())
                            .collect::<Vec<_>>()
                            .join(" ")
                    ));
                }
            }
            Direction::Decode => {
                let bytes = match parse_hex_bytes(&self.cs_input) {
                    Ok(b) => b,
                    Err(err) => {
                        self.cs_error = Some(err);
                        return;
                    }
                };
                let decoded = charset::decode(source, &bytes);
                self.cs_bytes = bytes;
                self.cs_output = decoded.text;
                if decoded.had_errors {
                    self.cs_error = Some(format!(
                        "{} 解码有 {} 处非法字节（首个偏移 {}）",
                        source.name,
                        decoded.error_offsets.len(),
                        decoded.error_offsets.first().copied().unwrap_or(0)
                    ));
                }
            }
        }
    }

    fn escape_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("转义 / 符号编码");
        ui.horizontal(|ui| {
            ComboBox::from_id_salt("escape-kind")
                .selected_text(self.esc_kind.name())
                .show_ui(ui, |ui| {
                    for kind in EscapeKind::ALL {
                        ui.selectable_value(&mut self.esc_kind, kind, kind.name());
                    }
                });
            ui.separator();
            if ui
                .selectable_label(self.esc_direction == Direction::Encode, "编码")
                .clicked()
            {
                self.esc_direction = Direction::Encode;
            }
            if ui
                .selectable_label(self.esc_direction == Direction::Decode, "解码")
                .clicked()
            {
                self.esc_direction = Direction::Decode;
            }
            if ui.button("执行").clicked() {
                self.run_escape();
            }
        });
        ui.label(RichText::new(self.esc_kind.note()).small().weak());
        ui.add(
            TextEdit::multiline(&mut self.esc_input)
                .desired_rows(6)
                .desired_width(f32::INFINITY)
                .font(egui::TextStyle::Monospace),
        );
        if let Some(err) = &self.esc_error {
            ui.colored_label(egui::Color32::RED, err.as_str());
        }
        ui.add(
            TextEdit::multiline(&mut self.esc_output)
                .desired_rows(6)
                .desired_width(f32::INFINITY)
                .interactive(false)
                .font(egui::TextStyle::Monospace),
        );

        ui.separator();
        ui.heading("摩尔斯电码");
        ui.horizontal(|ui| {
            if ui.button("文本 → 摩尔斯").clicked() {
                self.esc_output = morse::encode(&self.esc_input);
                self.esc_error = None;
            }
            if ui.button("摩尔斯 → 文本").clicked() {
                match morse::decode(&self.esc_input) {
                    Ok(text) => {
                        self.esc_output = text;
                        self.esc_error = None;
                    }
                    Err(err) => self.esc_error = Some(err),
                }
            }
        });
    }

    fn run_escape(&mut self) {
        self.esc_error = None;
        match self.esc_direction {
            Direction::Encode => self.esc_output = self.esc_kind.encode(&self.esc_input),
            Direction::Decode => match self.esc_kind.decode(&self.esc_input) {
                Ok(text) => self.esc_output = text,
                Err(err) => {
                    self.esc_error = Some(err);
                    self.esc_output.clear();
                }
            },
        }
    }

    fn batch_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("批量文件转码");
        ui.label("把文件或文件夹拖进窗口即可加入队列，也可以点“添加文件”。");

        ui.horizontal(|ui| {
            if ui.button("添加文件").clicked() {
                if let Some(files) = rfd::FileDialog::new().pick_files() {
                    self.batch_items.extend(batch::scan(&files, false));
                }
            }
            if ui.button("添加文件夹").clicked() {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    self.batch_items.extend(batch::scan(&[dir], true));
                }
            }
            if ui.button("清空队列").clicked() {
                self.batch_items.clear();
                self.batch_progress = (0, 0);
            }
        });

        ui.separator();
        ui.horizontal(|ui| {
            ui.label("目标编码：");
            encoding_combo(ui, "batch-target", &mut self.batch_target);
            ui.separator();
            ui.checkbox(&mut self.batch_source_auto, "自动探测源编码");
        });
        if !self.batch_source_auto {
            ui.horizontal(|ui| {
                ui.label("源编码：");
                encoding_combo(ui, "batch-source", &mut self.batch_source);
            });
        }
        ui.horizontal(|ui| {
            ui.label("输出目录：");
            ui.add(TextEdit::singleline(&mut self.batch_output_dir).desired_width(320.0));
            if ui.button("选择…").clicked() {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    self.batch_output_dir = dir.display().to_string();
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("无输出目录时追加后缀：");
            ui.add(TextEdit::singleline(&mut self.batch_suffix).desired_width(80.0));
            ui.checkbox(&mut self.batch_overwrite, "覆盖已存在");
            ui.checkbox(&mut self.batch_dry_run, "仅试运行");
            ui.checkbox(&mut self.batch_keep_bom, "保留 BOM");
        });

        let (done, total) = self.batch_progress;
        let ratio = if total == 0 {
            0.0
        } else {
            done as f32 / total as f32
        };
        ui.add(egui::ProgressBar::new(ratio).text(format!("{done}/{total}")));
        if ui.button("开始转码").clicked() {
            self.run_batch();
        }

        ui.separator();
        let summary = batch::summarize(&self.batch_items);
        ui.label(format!(
            "共 {} 项：完成 {}，跳过 {}，失败 {}",
            summary.total, summary.done, summary.skipped, summary.failed
        ));

        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
            egui::Grid::new("batch-grid")
                .num_columns(4)
                .striped(true)
                .show(ui, |ui| {
                    ui.label(RichText::new("文件").strong());
                    ui.label(RichText::new("状态").strong());
                    ui.label(RichText::new("大小").strong());
                    ui.label(RichText::new("说明").strong());
                    ui.end_row();
                    for item in &self.batch_items {
                        ui.label(
                            item.path
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| item.path.display().to_string()),
                        );
                        let text = item.status.label();
                        match item.status {
                            ItemStatus::Done => {
                                ui.label(RichText::new(text).color(egui::Color32::DARK_GREEN))
                            }
                            ItemStatus::Failed => {
                                ui.label(RichText::new(text).color(egui::Color32::RED))
                            }
                            _ => ui.label(RichText::new(text)),
                        };
                        ui.label(human_size(item.size));
                        ui.label(RichText::new(&item.message).small());
                        ui.end_row();
                    }
                });
        });
    }

    fn run_batch(&mut self) {
        let Some(target) = charset::find(&self.batch_target) else {
            self.status = "目标编码不存在".into();
            return;
        };
        let source = if self.batch_source_auto {
            SourceEncoding::Auto
        } else {
            match charset::find(&self.batch_source) {
                Some(info) => SourceEncoding::Fixed(info),
                None => {
                    self.status = "源编码不存在".into();
                    return;
                }
            }
        };
        let options = BatchOptions {
            target,
            source,
            output_dir: if self.batch_output_dir.trim().is_empty() {
                None
            } else {
                Some(PathBuf::from(self.batch_output_dir.trim()))
            },
            suffix: self.batch_suffix.clone(),
            overwrite: self.batch_overwrite,
            keep_bom: self.batch_keep_bom,
            dry_run: self.batch_dry_run,
            backup: false,
        };
        self.batch_progress = (0, self.batch_items.len());
        let mut progress = 0usize;
        batch::run(&mut self.batch_items, &options, |done, _total| {
            progress = done;
        });
        self.batch_progress = (progress, self.batch_items.len());
        let summary = batch::summarize(&self.batch_items);
        self.status = format!(
            "批量完成：成功 {}，跳过 {}，失败 {}",
            summary.done, summary.skipped, summary.failed
        );
    }

    fn detect_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("编码探测");
        ui.label("拖入文件，自动分析 BOM、字节分布与 chardetng 统计模型。");
        ui.horizontal(|ui| {
            if ui.button("选择文件…").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_file() {
                    self.run_detect(&path);
                }
            }
            if let Some(file) = &self.detect_file {
                ui.label(RichText::new(file).small().weak());
            }
        });
        ui.separator();
        if let Some(result) = &self.detect_result {
            if let Some(best) = result.best {
                ui.label(RichText::new(format!("最佳匹配：{}（{}）", best.name, best.note)).strong());
                ui.label(format!("置信度：{:.0}%", result.confidence * 100.0));
            } else {
                ui.label("未能判断编码");
            }
            if let Some(bom) = result.bom {
                ui.label(format!("BOM：{bom}"));
            }
            ui.add_space(6.0);
            egui::Grid::new("detect-grid")
                .num_columns(4)
                .striped(true)
                .show(ui, |ui| {
                    ui.label(RichText::new("候选").strong());
                    ui.label(RichText::new("得分").strong());
                    ui.label(RichText::new("说明").strong());
                    ui.label("");
                    ui.end_row();
                    for candidate in &result.candidates {
                        ui.label(candidate.name);
                        ui.label(format!("{:.0}%", candidate.score * 100.0));
                        ui.label(RichText::new(&candidate.note).small());
                        if ui.button("用它解码").clicked() {
                            self.cs_source = candidate.id.to_string();
                            self.tab = Tab::Charset;
                        }
                        ui.end_row();
                    }
                });
        } else {
            ui.label(RichText::new("还没有分析任何文件").weak());
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_dropped_files(&ctx);

        egui::Panel::top("tabs").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading("编码转换器");
                ui.separator();
                for tab in Tab::ALL {
                    if ui.selectable_label(self.tab == tab, tab.label()).clicked() {
                        self.tab = tab;
                    }
                }
            });
        });

        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&self.status).small());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new("v0.1.0").small());
                });
            });
        });

        egui::CentralPanel::default().show(ui, |ui| match self.tab {
            Tab::Radix => self.radix_tab(ui),
            Tab::BaseN => self.base_tab(ui),
            Tab::Charset => self.charset_tab(ui),
            Tab::Escape => self.escape_tab(ui),
            Tab::Batch => self.batch_tab(ui),
            Tab::Detect => self.detect_tab(ui),
        });
    }
}

fn encoding_combo(ui: &mut egui::Ui, id: &str, current: &mut String) {
    let selected = charset::find(current)
        .map(|e| e.name.to_string())
        .unwrap_or_else(|| current.clone());
    ComboBox::from_id_salt(id).selected_text(selected).show_ui(ui, |ui| {
        for group in Group::ALL {
            let encodings: Vec<&EncodingInfo> =
                charset::LIST.iter().filter(|e| e.group == group).collect();
            if encodings.is_empty() {
                continue;
            }
            ui.label(RichText::new(group.label()).small().strong());
            for info in encodings {
                ui.selectable_value(current, info.id.to_string(), info.name);
            }
            ui.separator();
        }
    });
}

fn install_cjk_font(ctx: &egui::Context) {
    let candidates = [
        "C:/Windows/Fonts/msyh.ttc",
        "C:/Windows/Fonts/msyh.ttf",
        "C:/Windows/Fonts/simhei.ttf",
        "C:/Windows/Fonts/simsun.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
    ];
    for path in candidates {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        ctx.add_font(egui::FontInsert {
            name: "cjk".to_owned(),
            data: egui::FontData::from_owned(bytes),
            families: vec![
                egui::epaint::text::InsertFontFamily {
                    family: egui::FontFamily::Proportional,
                    priority: egui::epaint::text::FontPriority::Highest,
                },
                egui::epaint::text::InsertFontFamily {
                    family: egui::FontFamily::Monospace,
                    priority: egui::epaint::text::FontPriority::Lowest,
                },
            ],
        });
        return;
    }
    eprintln!("警告：没有找到可用的中文字体，界面可能显示方块");
}

fn prepend_bom(bytes: &mut Vec<u8>, target_id: &str) {
    let prefix: &[u8] = match target_id {
        "utf-8" => &[0xEF, 0xBB, 0xBF],
        "utf-16le" => &[0xFF, 0xFE],
        "utf-16be" => &[0xFE, 0xFF],
        "utf-32le" => &[0xFF, 0xFE, 0x00, 0x00],
        "utf-32be" => &[0x00, 0x00, 0xFE, 0xFF],
        _ => return,
    };
    bytes.splice(0..0, prefix.iter().copied());
}

fn hexdump(bytes: &[u8]) -> String {
    let limited = &bytes[..bytes.len().min(256)];
    let mut out = limited
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ");
    if bytes.len() > 256 {
        out.push_str(&format!(" …（共 {} 字节，仅显示前 256）", bytes.len()));
    }
    out
}

fn parse_hex_bytes(input: &str) -> Result<Vec<u8>, String> {
    let cleaned: String = input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ',' && *c != '-')
        .collect();
    if !cleaned.len().is_multiple_of(2) {
        return Err("十六进制位数必须是偶数".into());
    }
    let chars: Vec<char> = cleaned.chars().collect();
    let mut bytes = Vec::with_capacity(chars.len() / 2);
    for pair in chars.chunks(2) {
        let hex: String = pair.iter().collect();
        let value = u8::from_str_radix(&hex, 16).map_err(|_| format!("非法的十六进制：{hex}"))?;
        bytes.push(value);
    }
    Ok(bytes)
}

fn human_size(size: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = size as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{size} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// 启动 GUI。
pub fn run_gui() -> anyhow::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 760.0])
            .with_min_inner_size([760.0, 520.0])
            .with_title("编码转换器"),
        ..Default::default()
    };
    eframe::run_native(
        "编码转换器",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
    .map_err(|err| anyhow::anyhow!("GUI 启动失败：{err}"))
}
