//! egui 界面：薄荷糖渐变背景 + 左侧导航 + 六个功能页。
//!
//! 设计目标：新手打开就能用——默认停在“文本编码转换”，输入即出结果，
//! 除批量转码外都不需要点“执行”按钮。

use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use eframe::egui;
use eframe::egui::{
    pos2, vec2, Align, Align2, Color32, ComboBox, CornerRadius, FontId, Frame, Margin, Pos2, Rect,
    RichText, Sense, Shadow, Stroke, TextEdit, Ui, Vec2,
};

use crate::batch::{self, BatchEvent, BatchItem, BatchOptions, ItemStatus, SourceEncoding};
use crate::core::base_n::BaseN;
use crate::core::charset::{self, EncodingInfo, Group};
use crate::core::detect::{self, Detection};
use crate::core::escape::EscapeKind;
use crate::core::{morse, numeral, radix};
use crate::settings::Settings;

// 渐变两端：薄荷绿 → 樱花粉（#D0FDF8 → #FFC5CE）
const GRADIENT_TOP: Color32 = Color32::from_rgb(0xD0, 0xFD, 0xF8);
const GRADIENT_BOTTOM: Color32 = Color32::from_rgb(0xFF, 0xC5, 0xCE);
const INK: Color32 = Color32::from_rgb(0x24, 0x35, 0x3B);
const MUTED: Color32 = Color32::from_rgb(0x6C, 0x7E, 0x84);
const ACCENT: Color32 = Color32::from_rgb(0x17, 0x9E, 0x8C);
const ACCENT_SOFT: Color32 = Color32::from_rgb(0xE4, 0xF7, 0xF4);
const DANGER: Color32 = Color32::from_rgb(0xC4, 0x3D, 0x55);
const DANGER_SOFT: Color32 = Color32::from_rgb(0xFD, 0xEC, 0xEF);
const CARD: Color32 = Color32::from_rgba_unmultiplied_const(252, 254, 254, 228);
const REPO_URL: &str = "https://github.com/AlanLumier/encoding-converter";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Text,
    Radix,
    BaseN,
    Escape,
    Batch,
    Detect,
}

impl Tab {
    const ALL: [Tab; 6] = [
        Tab::Text,
        Tab::Radix,
        Tab::BaseN,
        Tab::Escape,
        Tab::Batch,
        Tab::Detect,
    ];

    fn id(self) -> &'static str {
        match self {
            Tab::Text => "text",
            Tab::Radix => "radix",
            Tab::BaseN => "base",
            Tab::Escape => "escape",
            Tab::Batch => "batch",
            Tab::Detect => "detect",
        }
    }

    fn from_id(id: &str) -> Tab {
        Tab::ALL.into_iter().find(|t| t.id() == id).unwrap_or(Tab::Text)
    }

    fn title(self) -> &'static str {
        match self {
            Tab::Text => "文本编码转换",
            Tab::Radix => "进制换算",
            Tab::BaseN => "Base 编码",
            Tab::Escape => "转义 / 符号",
            Tab::Batch => "文件批量转码",
            Tab::Detect => "编码探测",
        }
    }

    fn subtitle(self) -> &'static str {
        match self {
            Tab::Text => "粘贴文字，选目标编码，结果实时出现",
            Tab::Radix => "2–36 进制互转，顺带补码与浮点位视图",
            Tab::BaseN => "Base64 / Base32 / Base58 / Base85 …",
            Tab::Escape => "URL、HTML、JSON、Unicode 转义与摩尔斯电码",
            Tab::Batch => "拖入文件，一键批量换编码",
            Tab::Detect => "不认识的文件？拖进来看看它是什么编码",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Tab::Text => "文",
            Tab::Radix => "数",
            Tab::BaseN => "B",
            Tab::Escape => "转",
            Tab::Batch => "批",
            Tab::Detect => "探",
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

    text_input: String,
    text_source: String,
    text_target: String,
    text_direction: Direction,
    text_bom: bool,
    text_output: String,
    text_bytes: Vec<u8>,
    text_error: Option<String>,
    text_key: u64,

    radix_input: String,
    radix_from: u32,
    radix_custom_to: u32,
    numeral_input: String,
    numeral_system: NumeralSystem,
    float_input: String,
    float_single: bool,
    tc_input: String,
    tc_bits: u32,

    base_kind: BaseN,
    base_direction: Direction,
    base_input: String,
    base_output: String,
    base_error: Option<String>,
    base_key: u64,

    esc_kind: EscapeKind,
    esc_direction: Direction,
    esc_input: String,
    esc_output: String,
    esc_error: Option<String>,
    esc_key: u64,

    batch_items: Vec<BatchItem>,
    batch_target: String,
    batch_source_auto: bool,
    batch_source: String,
    batch_output_dir: String,
    batch_suffix: String,
    batch_overwrite: bool,
    batch_dry_run: bool,
    batch_keep_bom: bool,
    batch_rx: Option<Receiver<BatchEvent>>,
    batch_total: usize,
    batch_done: usize,
    batch_summary: Option<(usize, usize, usize)>,

    detect_path: Option<String>,
    detect_result: Option<Detection>,

    status: String,
    last_settings: Settings,
    last_save: Instant,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_cjk_font(&cc.egui_ctx);
        apply_theme(&cc.egui_ctx);

        let settings = Settings::load();
        let mut app = Self {
            tab: Tab::from_id(&settings.tab),
            text_input: "你好，编码转换器！Hello".into(),
            text_source: settings.cs_source.clone(),
            text_target: settings.cs_target.clone(),
            text_direction: Direction::Encode,
            text_bom: false,
            text_output: String::new(),
            text_bytes: Vec::new(),
            text_error: None,
            text_key: 0,
            radix_input: "255".into(),
            radix_from: settings.radix_from,
            radix_custom_to: settings.radix_to.clamp(2, 36),
            numeral_input: "2024".into(),
            numeral_system: NumeralSystem::Roman,
            float_input: "1.5".into(),
            float_single: true,
            tc_input: "FF".into(),
            tc_bits: 8,
            base_kind: BaseN::ALL
                .into_iter()
                .find(|b| b.id() == settings.base_kind)
                .unwrap_or(BaseN::Base64),
            base_direction: Direction::Encode,
            base_input: "你好，Base 编码".into(),
            base_output: String::new(),
            base_error: None,
            base_key: 0,
            esc_kind: EscapeKind::ALL
                .into_iter()
                .find(|e| e.id() == settings.escape_kind)
                .unwrap_or(EscapeKind::UrlPercent),
            esc_direction: Direction::Encode,
            esc_input: "你好 world & <tag>".into(),
            esc_output: String::new(),
            esc_error: None,
            esc_key: 0,
            batch_items: Vec::new(),
            batch_target: settings.batch_target.clone(),
            batch_source_auto: true,
            batch_source: "gbk".into(),
            batch_output_dir: settings.batch_output_dir.clone(),
            batch_suffix: settings.batch_suffix.clone(),
            batch_overwrite: settings.batch_overwrite,
            batch_dry_run: false,
            batch_keep_bom: settings.batch_keep_bom,
            batch_rx: None,
            batch_total: 0,
            batch_done: 0,
            batch_summary: None,
            detect_path: if settings.detect_path.is_empty() {
                None
            } else {
                Some(settings.detect_path.clone())
            },
            detect_result: None,
            status: "就绪 · 设置会自动保存".into(),
            last_settings: settings,
            last_save: Instant::now(),
        };
        app.recalc_text();
        app.recalc_base();
        app.recalc_escape();
        app
    }

    fn snapshot(&self) -> Settings {
        Settings {
            tab: self.tab.id().into(),
            cs_source: self.text_source.clone(),
            cs_target: self.text_target.clone(),
            radix_from: self.radix_from,
            radix_to: self.radix_custom_to,
            base_kind: self.base_kind.id().into(),
            escape_kind: self.esc_kind.id().into(),
            batch_target: self.batch_target.clone(),
            batch_output_dir: self.batch_output_dir.clone(),
            batch_suffix: self.batch_suffix.clone(),
            batch_overwrite: self.batch_overwrite,
            batch_keep_bom: self.batch_keep_bom,
            detect_path: self.detect_path.clone().unwrap_or_default(),
        }
    }

    fn maybe_save_settings(&mut self) {
        let snapshot = self.snapshot();
        if snapshot != self.last_settings && self.last_save.elapsed() > Duration::from_millis(700) {
            if let Err(err) = snapshot.save() {
                self.status = format!("设置保存失败：{err}");
            }
            self.last_settings = snapshot;
            self.last_save = Instant::now();
        }
    }

    fn text_key_now(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.text_input.hash(&mut hasher);
        self.text_source.hash(&mut hasher);
        self.text_target.hash(&mut hasher);
        (self.text_direction == Direction::Encode).hash(&mut hasher);
        self.text_bom.hash(&mut hasher);
        hasher.finish()
    }

    fn recalc_text(&mut self) {
        self.text_output.clear();
        self.text_bytes.clear();
        self.text_error = None;
        let Some(source) = charset::find(&self.text_source) else {
            self.text_error = Some("源编码不存在".into());
            return;
        };
        let Some(target) = charset::find(&self.text_target) else {
            self.text_error = Some("目标编码不存在".into());
            return;
        };
        match self.text_direction {
            Direction::Encode => {
                if self.text_input.len() > 500_000 {
                    self.text_error = Some("文本超过 500 KB，请到「文件批量转码」页处理".into());
                    return;
                }
                let encoded = charset::encode(target, &self.text_input);
                let mut payload = encoded.bytes;
                if self.text_bom {
                    prepend_bom(&mut payload, target.id);
                }
                if !encoded.unmappable.is_empty() {
                    self.text_error = Some(format!(
                        "{} 个字符无法用 {} 表示，已跳过：{}",
                        encoded.unmappable.len(),
                        target.name,
                        encoded
                            .unmappable
                            .iter()
                            .take(8)
                            .map(|c| c.to_string())
                            .collect::<Vec<_>>()
                            .join(" ")
                    ));
                }
                self.text_bytes = payload;
            }
            Direction::Decode => {
                let bytes = match parse_hex_bytes(&self.text_input) {
                    Ok(b) => b,
                    Err(err) => {
                        self.text_error = Some(err);
                        return;
                    }
                };
                let decoded = charset::decode(source, &bytes);
                if decoded.had_errors {
                    self.text_error = Some(format!(
                        "{} 解码有 {} 处非法字节（首个偏移 {}）",
                        source.name,
                        decoded.error_offsets.len(),
                        decoded.error_offsets.first().copied().unwrap_or(0)
                    ));
                }
                self.text_bytes = bytes;
                self.text_output = decoded.text;
            }
        }
    }

    fn recalc_base(&mut self) {
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

    fn recalc_escape(&mut self) {
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

    fn poll_batch(&mut self) {
        let Some(rx) = &self.batch_rx else {
            return;
        };
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
        let mut finished = None;
        for event in events {
            match event {
                BatchEvent::Item { index, item } => {
                    if let Some(slot) = self.batch_items.get_mut(index) {
                        if item.status != ItemStatus::Pending {
                            self.batch_done = self.batch_done.saturating_add(1);
                        }
                        *slot = item;
                    }
                }
                BatchEvent::Finished {
                    done,
                    skipped,
                    failed,
                } => finished = Some((done, skipped, failed)),
            }
        }
        if let Some(summary) = finished {
            self.batch_rx = None;
            self.batch_summary = Some(summary);
            self.status = format!(
                "批量完成：成功 {}，跳过 {}，失败 {}",
                summary.0, summary.1, summary.2
            );
        }
    }

    fn start_batch(&mut self, ctx: &egui::Context) {
        if self.batch_rx.is_some() {
            return;
        }
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
        let items = self.batch_items.clone();
        self.batch_total = items.len();
        self.batch_done = 0;
        self.batch_summary = None;
        for item in self.batch_items.iter_mut() {
            item.status = ItemStatus::Pending;
            item.message.clear();
        }
        let ctx = ctx.clone();
        self.batch_rx = Some(batch::spawn(items, options, move || ctx.request_repaint()));
        self.status = format!("开始处理 {} 个文件…", self.batch_total);
    }

    fn run_detect(&mut self, path: &std::path::Path) {
        match std::fs::read(path) {
            Ok(bytes) => {
                let detection = detect::detect(&bytes);
                self.detect_path = Some(path.display().to_string());
                self.detect_result = Some(detection);
                self.status = format!("已分析 {} 字节", bytes.len());
            }
            Err(err) => {
                self.detect_path = Some(path.display().to_string());
                self.detect_result = None;
                self.status = format!("读取失败：{err}");
            }
        }
    }

    fn handle_dropped(&mut self, ctx: &egui::Context) {
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
                let items = batch::scan(&dropped, false);
                self.batch_items.extend(items);
                self.batch_total = self.batch_items.len();
                self.status = format!("已加入队列，共 {} 个文件", self.batch_items.len());
            }
            Tab::Detect => {
                if let Some(path) = dropped.first() {
                    self.run_detect(path);
                }
            }
            _ => {
                if let Some(path) = dropped.first() {
                    // 其它页面拖入文件时直接跳到批量页，符合新手直觉
                    let items = batch::scan(std::slice::from_ref(path), false);
                    self.batch_items.extend(items);
                    self.batch_total = self.batch_items.len();
                    self.tab = Tab::Batch;
                    self.status = format!("已切换到批量页：{}", path.display());
                }
            }
        }
    }

    // ---------------- 页面 ----------------

    fn text_page(&mut self, ui: &mut Ui) {
        page_header(ui, Tab::Text);
        ui.horizontal_wrapped(|ui| {
            if chip(ui, self.text_direction == Direction::Encode, "文字 → 字节", "把文字转成指定编码的十六进制") {
                self.text_direction = Direction::Encode;
            }
            if chip(ui, self.text_direction == Direction::Decode, "字节 → 文字", "把十六进制字节还原成文字") {
                self.text_direction = Direction::Decode;
            }
            ui.add_space(8.0);
            match self.text_direction {
                Direction::Encode => {
                    ui.label(RichText::new("目标编码").color(MUTED).size(12.5));
                    if encoding_combo(ui, "text-target", &mut self.text_target) {
                        self.text_key = 0;
                    }
                    ui.checkbox(&mut self.text_bom, "带 BOM");
                }
                Direction::Decode => {
                    ui.label(RichText::new("源编码").color(MUTED).size(12.5));
                    if encoding_combo(ui, "text-source", &mut self.text_source) {
                        self.text_key = 0;
                    }
                }
            }
            if ui.button("⇄ 交换").clicked() {
                std::mem::swap(&mut self.text_source, &mut self.text_target);
                self.text_direction = match self.text_direction {
                    Direction::Encode => Direction::Decode,
                    Direction::Decode => Direction::Encode,
                };
                self.text_key = 0;
            }
        });
        ui.add_space(10.0);

        let key = self.text_key_now();
        if key != self.text_key {
            self.text_key = key;
            self.recalc_text();
        }

        ui.columns(2, |cols| {
            card().show(&mut cols[0], |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("① 输入").strong().color(INK).size(14.0));
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("{} 字符", self.text_input.chars().count()))
                                .small()
                                .color(MUTED),
                        );
                    });
                });
                ui.add_space(4.0);
                let hint = match self.text_direction {
                    Direction::Encode => "在这里粘贴或输入文字",
                    Direction::Decode => "粘贴十六进制字节，例如 D6 D0 CE C4",
                };
                ui.add(
                    TextEdit::multiline(&mut self.text_input)
                        .desired_rows(12)
                        .desired_width(f32::INFINITY)
                        .hint_text(hint)
                        .font(egui::TextStyle::Monospace),
                );
            });
            card().show(&mut cols[1], |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("② 结果").strong().color(INK).size(14.0));
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("复制").clicked() {
                            let text = match self.text_direction {
                                Direction::Encode => hex_bytes(&self.text_bytes),
                                Direction::Decode => self.text_output.clone(),
                            };
                            ui.ctx().copy_text(text);
                        }
                    });
                });
                ui.add_space(4.0);
                match self.text_direction {
                    Direction::Encode => {
                        if let Some(target) = charset::find(&self.text_target) {
                            ui.label(
                                RichText::new(format!(
                                    "{} 字节 · {}",
                                    self.text_bytes.len(),
                                    target.name
                                ))
                                .small()
                                .color(MUTED),
                            );
                        }
                        ui.add_space(2.0);
                        scroll_box(ui, &hex_bytes(&self.text_bytes));
                    }
                    Direction::Decode => {
                        let mut shown = self.text_output.clone();
                        ui.add(
                            TextEdit::multiline(&mut shown)
                                .desired_rows(12)
                                .desired_width(f32::INFINITY)
                                .interactive(false)
                                .font(egui::TextStyle::Monospace),
                        );
                    }
                }
            });
        });

        if let Some(err) = &self.text_error {
            ui.add_space(8.0);
            error_banner(ui, err);
        }
        if self.text_direction == Direction::Encode && !self.text_bytes.is_empty() {
            if let Some(target) = charset::find(&self.text_target) {
                let roundtrip = charset::decode(target, &self.text_bytes).text;
                ui.add_space(8.0);
                card().show(ui, |ui| {
                    ui.label(
                        RichText::new(format!(
                            "往返校验：这 {} 字节按 {} 解回来是",
                            self.text_bytes.len(),
                            target.name
                        ))
                        .small()
                        .strong()
                        .color(MUTED),
                    );
                    ui.label(RichText::new(roundtrip).monospace().color(INK));
                });
            }
        }
        ui.add_space(12.0);
        quick_actions(ui, self);
    }

    fn radix_page(&mut self, ui: &mut Ui) {
        page_header(ui, Tab::Radix);
        card().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("输入数字").color(MUTED));
                ui.add(
                    TextEdit::singleline(&mut self.radix_input)
                        .desired_width(260.0)
                        .hint_text("例如 255 或 0xFF"),
                );
                ui.label(RichText::new("源进制").color(MUTED));
                for base in [2u32, 8, 10, 16] {
                    if chip(ui, self.radix_from == base, &format!("{base}"), "") {
                        self.radix_from = base;
                    }
                }
                ui.add(egui::DragValue::new(&mut self.radix_from).range(2..=36).speed(0.2));
            });
        });

        ui.add_space(10.0);
        let mut error = None;
        let mut results: Vec<(u32, String)> = Vec::new();
        for base in [2u32, 8, 10, 16, self.radix_custom_to] {
            if results.iter().any(|(b, _)| *b == base) {
                continue;
            }
            match radix::convert(&self.radix_input, self.radix_from, base, 32) {
                Ok(value) => results.push((base, value)),
                Err(err) => error = Some(err),
            }
        }
        if let Some(err) = error {
            error_banner(ui, &err);
        } else {
            ui.columns(4, |cols| {
                for (index, (base, value)) in results.iter().take(4).enumerate() {
                    let column = &mut cols[index];
                    card().show(column, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{base} 进制")).small().color(MUTED));
                            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                if ui.small_button("复制").clicked() {
                                    ui.ctx().copy_text(value.clone());
                                }
                            });
                        });
                        ui.label(RichText::new(value).monospace().size(16.0).color(INK));
                    });
                }
            });
            if results.len() > 4 {
                let (base, value) = &results[4];
                let value = value.clone();
                ui.add_space(8.0);
                card().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{base} 进制（自定义）")).color(MUTED));
                        ui.add(egui::DragValue::new(&mut self.radix_custom_to).range(2..=36));
                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new(value).monospace().size(15.0));
                        });
                    });
                });
            }
        }

        ui.add_space(12.0);
        egui::CollapsingHeader::new("更多工具：补码 / IEEE-754 / 特殊数制")
            .default_open(false)
            .show(ui, |ui| {
                card().show(ui, |ui| {
                    ui.label(RichText::new("二进制补码").strong().color(INK));
                    ui.horizontal(|ui| {
                        ui.label("十六进制");
                        ui.add(TextEdit::singleline(&mut self.tc_input).desired_width(120.0));
                        for bits in [8u32, 16, 32, 64] {
                            if chip(ui, self.tc_bits == bits, &format!("{bits} 位"), "") {
                                self.tc_bits = bits;
                            }
                        }
                        ui.separator();
                        match radix::twos_complement_decimal(&self.tc_input, 16, self.tc_bits) {
                            Ok(value) => {
                                ui.label(
                                    RichText::new(format!("有符号值 {value}"))
                                        .monospace()
                                        .color(ACCENT),
                                );
                            }
                            Err(err) => {
                                ui.label(RichText::new(err).small().color(MUTED));
                            }
                        }
                    });
                });
                card().show(ui, |ui| {
                    ui.label(RichText::new("IEEE-754 浮点数").strong().color(INK));
                    ui.horizontal(|ui| {
                        ui.label("数值");
                        ui.add(TextEdit::singleline(&mut self.float_input).desired_width(120.0));
                        ui.checkbox(&mut self.float_single, "单精度");
                    });
                    if let Ok(value) = self.float_input.trim().parse::<f64>() {
                        let view = radix::ieee754(value, self.float_single);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(view.hex).monospace().color(ACCENT));
                            ui.label(RichText::new(view.binary).monospace().small().color(MUTED));
                        });
                    }
                });
                card().show(ui, |ui| {
                    ui.label(RichText::new("特殊数制").strong().color(INK));
                    ui.horizontal(|ui| {
                        ui.add(TextEdit::singleline(&mut self.numeral_input).desired_width(120.0));
                        ComboBox::from_id_salt("numeral-system")
                            .selected_text(self.numeral_system.label())
                            .show_ui(ui, |ui| {
                                for system in NumeralSystem::ALL {
                                    ui.selectable_value(
                                        &mut self.numeral_system,
                                        system,
                                        system.label(),
                                    );
                                }
                            });
                        ui.separator();
                        match convert_numeral(&self.numeral_input, self.numeral_system) {
                            Ok(text) => {
                                ui.monospace(text);
                            }
                            Err(err) => {
                                ui.label(RichText::new(err).small().color(MUTED));
                            }
                        }
                    });
                });
            });
    }

    fn base_page(&mut self, ui: &mut Ui) {
        page_header(ui, Tab::BaseN);
        ui.horizontal_wrapped(|ui| {
            if chip(ui, self.base_direction == Direction::Encode, "编码", "字节 → 文本") {
                self.base_direction = Direction::Encode;
                self.base_key = 0;
            }
            if chip(ui, self.base_direction == Direction::Decode, "解码", "文本 → 字节") {
                self.base_direction = Direction::Decode;
                self.base_key = 0;
            }
            ui.add_space(8.0);
            ComboBox::from_id_salt("base-kind")
                .width(260.0)
                .selected_text(self.base_kind.name())
                .show_ui(ui, |ui| {
                    for kind in BaseN::ALL {
                        if ui.selectable_value(&mut self.base_kind, kind, kind.name()).clicked() {
                            self.base_key = 0;
                        }
                    }
                });
        });
        ui.label(RichText::new(self.base_kind.note()).small().color(MUTED));
        ui.add_space(8.0);

        let key = {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            self.base_input.hash(&mut hasher);
            self.base_kind.id().hash(&mut hasher);
            (self.base_direction == Direction::Encode).hash(&mut hasher);
            hasher.finish()
        };
        if key != self.base_key {
            self.base_key = key;
            self.recalc_base();
        }

        ui.columns(2, |cols| {
            card().show(&mut cols[0], |ui| {
                ui.label(RichText::new("输入").strong().color(INK));
                ui.add(
                    TextEdit::multiline(&mut self.base_input)
                        .desired_rows(10)
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace),
                );
            });
            card().show(&mut cols[1], |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("输出").strong().color(INK));
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("复制").clicked() {
                            ui.ctx().copy_text(self.base_output.clone());
                        }
                    });
                });
                scroll_box(ui, &self.base_output);
            });
        });
        if let Some(err) = &self.base_error {
            ui.add_space(8.0);
            error_banner(ui, err);
        }
    }

    fn escape_page(&mut self, ui: &mut Ui) {
        page_header(ui, Tab::Escape);
        ui.horizontal_wrapped(|ui| {
            if chip(ui, self.esc_direction == Direction::Encode, "编码", "原文 → 转义") {
                self.esc_direction = Direction::Encode;
                self.esc_key = 0;
            }
            if chip(ui, self.esc_direction == Direction::Decode, "解码", "转义 → 原文") {
                self.esc_direction = Direction::Decode;
                self.esc_key = 0;
            }
            ui.add_space(8.0);
            ComboBox::from_id_salt("escape-kind")
                .width(240.0)
                .selected_text(self.esc_kind.name())
                .show_ui(ui, |ui| {
                    for kind in EscapeKind::ALL {
                        if ui.selectable_value(&mut self.esc_kind, kind, kind.name()).clicked() {
                            self.esc_key = 0;
                        }
                    }
                });
            ui.separator();
            if ui.small_button("摩尔斯：文本→码").clicked() {
                self.esc_output = morse::encode(&self.esc_input);
            }
            if ui.small_button("码→文本").clicked() {
                match morse::decode(&self.esc_input) {
                    Ok(text) => self.esc_output = text,
                    Err(err) => self.esc_error = Some(err),
                }
            }
        });
        ui.label(RichText::new(self.esc_kind.note()).small().color(MUTED));
        ui.add_space(8.0);

        let key = {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            self.esc_input.hash(&mut hasher);
            self.esc_kind.id().hash(&mut hasher);
            (self.esc_direction == Direction::Encode).hash(&mut hasher);
            hasher.finish()
        };
        if key != self.esc_key {
            self.esc_key = key;
            self.recalc_escape();
        }

        ui.columns(2, |cols| {
            card().show(&mut cols[0], |ui| {
                ui.label(RichText::new("输入").strong().color(INK));
                ui.add(
                    TextEdit::multiline(&mut self.esc_input)
                        .desired_rows(10)
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace),
                );
            });
            card().show(&mut cols[1], |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("输出").strong().color(INK));
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("复制").clicked() {
                            ui.ctx().copy_text(self.esc_output.clone());
                        }
                    });
                });
                scroll_box(ui, &self.esc_output);
            });
        });
        if let Some(err) = &self.esc_error {
            ui.add_space(8.0);
            error_banner(ui, err);
        }
    }

    fn batch_page(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        page_header(ui, Tab::Batch);
        let running = self.batch_rx.is_some();

        let (rect, _response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 104.0), Sense::hover());
        let hovered = ctx.input(|i| !i.raw.hovered_files.is_empty());
        dashed_rect(
            ui.painter(),
            rect,
            Stroke::new(1.5, if hovered { ACCENT } else { Color32::from_rgb(0xB9, 0xE4, 0xDE) }),
        );
        ui.painter().rect_filled(
            rect.shrink(1.0),
            CornerRadius::same(14),
            if hovered {
                Color32::from_rgba_unmultiplied(255, 255, 255, 235)
            } else {
                Color32::from_rgba_unmultiplied(255, 255, 255, 150)
            },
        );
        ui.painter().text(
            rect.center() - vec2(0.0, 13.0),
            Align2::CENTER_CENTER,
            "把文件拖到这里",
            FontId::proportional(18.0),
            INK,
        );
        ui.painter().text(
            rect.center() + vec2(0.0, 14.0),
            Align2::CENTER_CENTER,
            "支持 .txt / 无扩展名 / 任意文本文件，可一次拖多个",
            FontId::proportional(12.0),
            MUTED,
        );

        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button("＋ 添加文件").clicked() {
                if let Some(files) = rfd::FileDialog::new().pick_files() {
                    self.batch_items.extend(batch::scan(&files, false));
                    self.batch_total = self.batch_items.len();
                }
            }
            if ui.button("＋ 添加文件夹").clicked() {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    self.batch_items.extend(batch::scan(&[dir], true));
                    self.batch_total = self.batch_items.len();
                }
            }
            if ui.button("清空队列").clicked() && !running {
                self.batch_items.clear();
                self.batch_summary = None;
                self.batch_done = 0;
                self.batch_total = 0;
            }
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("队列 {} 个文件", self.batch_items.len()))
                        .small()
                        .color(MUTED),
                );
            });
        });

        ui.add_space(10.0);
        card().show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("转换成").color(MUTED));
                encoding_combo(ui, "batch-target", &mut self.batch_target);
                ui.checkbox(&mut self.batch_source_auto, "自动识别源编码");
                if !self.batch_source_auto {
                    encoding_combo(ui, "batch-source", &mut self.batch_source);
                }
                ui.separator();
                ui.checkbox(&mut self.batch_overwrite, "覆盖已存在");
                ui.checkbox(&mut self.batch_keep_bom, "保留 BOM");
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("输出到").color(MUTED));
                ui.add(
                    TextEdit::singleline(&mut self.batch_output_dir)
                        .desired_width(300.0)
                        .hint_text("留空 = 原目录（自动加编码后缀）"),
                );
                if ui.button("选择…").clicked() {
                    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                        self.batch_output_dir = dir.display().to_string();
                    }
                }
                ui.separator();
                let can_start = !self.batch_items.is_empty() && !running;
                let button = egui::Button::new(RichText::new("开始转换").color(Color32::WHITE))
                    .fill(if can_start { ACCENT } else { MUTED })
                    .corner_radius(CornerRadius::same(10))
                    .min_size(vec2(124.0, 34.0));
                if ui.add_enabled(can_start, button).clicked() {
                    self.start_batch(ctx);
                }
            });
        });

        if running || self.batch_summary.is_some() {
            ui.add_space(8.0);
            let ratio = if self.batch_total == 0 {
                0.0
            } else {
                self.batch_done as f32 / self.batch_total as f32
            };
            let text = match self.batch_summary {
                Some((done, skipped, failed)) => {
                    format!("完成 {done} · 跳过 {skipped} · 失败 {failed}")
                }
                None => format!("{}/{}", self.batch_done, self.batch_total),
            };
            ui.add(
                egui::ProgressBar::new(ratio)
                    .text(RichText::new(text).size(12.0))
                    .animate(running),
            );
        }

        if self.batch_items.is_empty() {
            ui.add_space(16.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("队列是空的，拖入文件就能开始").color(MUTED));
            });
            return;
        }

        ui.add_space(10.0);
        card().show(ui, |ui| {
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                for item in &self.batch_items {
                    let (dot_color, status_text) = match item.status {
                        ItemStatus::Done => (ACCENT, item.status.label()),
                        ItemStatus::Failed => (DANGER, item.status.label()),
                        ItemStatus::Skipped => {
                            (Color32::from_rgb(0xC9, 0x92, 0x3D), item.status.label())
                        }
                        ItemStatus::Pending => (MUTED, item.status.label()),
                    };
                    ui.horizontal(|ui| {
                        let (dot_rect, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                        ui.painter().circle_filled(dot_rect.center(), 4.0, dot_color);
                        ui.label(
                            RichText::new(
                                item.path
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_else(|| item.path.display().to_string()),
                            )
                            .size(13.0),
                        );
                        ui.label(RichText::new(status_text).small().color(dot_color));
                        if !item.message.is_empty() {
                            ui.label(RichText::new(&item.message).small().color(MUTED));
                        }
                    });
                }
            });
        });
    }

    fn detect_page(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        page_header(ui, Tab::Detect);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 96.0), Sense::hover());
        let hovered = ctx.input(|i| !i.raw.hovered_files.is_empty());
        dashed_rect(
            ui.painter(),
            rect,
            Stroke::new(1.5, if hovered { ACCENT } else { Color32::from_rgb(0xB9, 0xE4, 0xDE) }),
        );
        ui.painter().rect_filled(
            rect.shrink(1.0),
            CornerRadius::same(14),
            if hovered {
                Color32::from_rgba_unmultiplied(255, 255, 255, 235)
            } else {
                Color32::from_rgba_unmultiplied(255, 255, 255, 150)
            },
        );
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            "把文件拖到这里，或点下面的按钮",
            FontId::proportional(15.0),
            INK,
        );
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button("选择文件…").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_file() {
                    self.run_detect(&path);
                }
            }
            if let Some(path) = &self.detect_path {
                ui.label(RichText::new(path).small().color(MUTED));
            }
        });

        if let Some(result) = &self.detect_result {
            ui.add_space(10.0);
            card().show(ui, |ui| {
                if let Some(best) = result.best {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("最可能的编码").color(MUTED).size(12.5));
                        ui.label(RichText::new(best.name).strong().size(20.0).color(INK));
                        ui.label(
                            RichText::new(format!("{:.0}% 置信度", result.confidence * 100.0))
                                .small()
                                .color(ACCENT),
                        );
                    });
                    ui.label(RichText::new(best.note).small().color(MUTED));
                } else {
                    ui.label("没能判断出编码，试试手动选择");
                }
                if let Some(bom) = result.bom {
                    ui.label(RichText::new(format!("检测到 BOM：{bom}")).small().color(MUTED));
                }
                ui.add_space(6.0);
                for candidate in &result.candidates {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(candidate.name).size(13.5));
                        ui.add(
                            egui::ProgressBar::new(candidate.score)
                                .desired_width(140.0)
                                .text(
                                    RichText::new(format!("{:.0}%", candidate.score * 100.0))
                                        .size(11.0),
                                ),
                        );
                        ui.label(RichText::new(&candidate.note).small().color(MUTED));
                    });
                }
            });
        } else {
            ui.add_space(12.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("还没有分析任何文件").color(MUTED));
            });
        }
    }
}

// ---------------- eframe 接线 ----------------

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        paint_gradient(ui);
        self.handle_dropped(&ctx);
        self.poll_batch();
        if self.batch_rx.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }

        egui::Panel::left("nav")
            .exact_size(240.0)
            .frame(Frame::NONE)
            .show(ui, |ui| {
                sidebar(ui, self);
            });

        egui::Panel::bottom("status")
            .frame(Frame::NONE)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    ui.label(RichText::new(&self.status).size(11.5).color(MUTED));
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(10.0);
                        ui.label(
                            RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                                .size(11.0)
                                .color(MUTED),
                        );
                    });
                });
            });

        egui::CentralPanel::default()
            .frame(Frame::new().inner_margin(Margin::same(22)))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| match self.tab {
                        Tab::Text => self.text_page(ui),
                        Tab::Radix => self.radix_page(ui),
                        Tab::BaseN => self.base_page(ui),
                        Tab::Escape => self.escape_page(ui),
                        Tab::Batch => self.batch_page(ui, &ctx),
                        Tab::Detect => self.detect_page(ui, &ctx),
                    });
            });

        self.maybe_save_settings();
    }
}

// ---------------- 组件与工具 ----------------

fn paint_gradient(ui: &Ui) {
    let rect = ui.max_rect();
    let painter = ui.painter();
    let steps = 128;
    let band = rect.height() / steps as f32;
    for index in 0..steps {
        let t = index as f32 / steps as f32;
        let y0 = rect.top() + band * index as f32;
        painter.rect_filled(
            Rect::from_min_max(pos2(rect.left(), y0), pos2(rect.right(), y0 + band + 1.0)),
            0,
            mix(GRADIENT_TOP, GRADIENT_BOTTOM, t),
        );
    }
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let r = a.r() as f32 + (b.r() as f32 - a.r() as f32) * t;
    let g = a.g() as f32 + (b.g() as f32 - a.g() as f32) * t;
    let bl = a.b() as f32 + (b.b() as f32 - a.b() as f32) * t;
    Color32::from_rgb(r as u8, g as u8, bl as u8)
}

fn apply_theme(ctx: &egui::Context) {
    let mut style = (*ctx.global_style()).clone();
    style.visuals.dark_mode = false;
    style.visuals.panel_fill = Color32::TRANSPARENT;
    style.visuals.window_fill = Color32::from_rgb(255, 255, 255);
    style.visuals.extreme_bg_color = Color32::from_rgb(246, 252, 251);
    style.visuals.selection.bg_fill = Color32::from_rgb(0xC9, 0xEF, 0xE9);
    style.visuals.selection.stroke = Stroke::new(1.0, INK);
    style.visuals.hyperlink_color = ACCENT;
    style.spacing.item_spacing = vec2(9.0, 9.0);
    style.spacing.button_padding = vec2(12.0, 7.0);
    style.spacing.interact_size.y = 28.0;

    for widget in [
        &mut style.visuals.widgets.noninteractive,
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
    ] {
        widget.corner_radius = CornerRadius::same(9);
    }
    style.visuals.widgets.noninteractive.bg_fill = Color32::TRANSPARENT;
    style.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(255, 255, 255);
    style.visuals.widgets.inactive.bg_stroke =
        Stroke::new(1.0, Color32::from_rgb(0xDD, 0xEC, 0xEA));
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(0xF0, 0xFB, 0xF8);
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    style.visuals.widgets.active.weak_bg_fill = ACCENT_SOFT;

    use egui::TextStyle;
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::new(23.0, egui::FontFamily::Proportional));
    style
        .text_styles
        .insert(TextStyle::Body, FontId::new(14.0, egui::FontFamily::Proportional));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::new(13.5, egui::FontFamily::Proportional));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::new(11.5, egui::FontFamily::Proportional));
    style
        .text_styles
        .insert(TextStyle::Monospace, FontId::new(13.0, egui::FontFamily::Monospace));
    ctx.set_global_style(style);
}

fn card() -> Frame {
    Frame::new()
        .fill(CARD)
        .corner_radius(CornerRadius::same(16))
        .inner_margin(Margin::same(16))
        .stroke(Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 170)))
        .shadow(Shadow {
            offset: [0, 5],
            blur: 14,
            spread: 0,
            color: Color32::from_rgba_unmultiplied(40, 90, 100, 24),
        })
}

fn page_header(ui: &mut Ui, tab: Tab) {
    ui.label(RichText::new(tab.title()).size(26.0).strong().color(INK));
    ui.label(RichText::new(tab.subtitle()).size(13.0).color(MUTED));
    ui.add_space(10.0);
}

fn chip(ui: &mut Ui, selected: bool, label: &str, tooltip: &str) -> bool {
    let text = RichText::new(label)
        .size(13.0)
        .color(if selected { Color32::WHITE } else { INK });
    let button = egui::Button::new(text)
        .fill(if selected {
            ACCENT
        } else {
            Color32::from_rgba_unmultiplied(255, 255, 255, 220)
        })
        .stroke(Stroke::new(
            1.0,
            if selected {
                ACCENT
            } else {
                Color32::from_rgb(0xDD, 0xEC, 0xEA)
            },
        ))
        .corner_radius(CornerRadius::same(9));
    let response = ui.add(button);
    if !tooltip.is_empty() {
        let _ = response.clone().on_hover_text(tooltip);
    }
    response.clicked()
}

fn encoding_combo(ui: &mut Ui, id: &str, current: &mut String) -> bool {
    let selected = charset::find(current)
        .map(|e| e.name.to_string())
        .unwrap_or_else(|| current.clone());
    let mut changed = false;
    ComboBox::from_id_salt(id)
        .selected_text(selected)
        .width(180.0)
        .show_ui(ui, |ui| {
            for group in Group::ALL {
                let encodings: Vec<&EncodingInfo> =
                    charset::LIST.iter().filter(|e| e.group == group).collect();
                if encodings.is_empty() {
                    continue;
                }
                ui.label(RichText::new(group.label()).small().strong().color(MUTED));
                for info in encodings {
                    if ui.selectable_value(current, info.id.to_string(), info.name).clicked() {
                        changed = true;
                    }
                }
                ui.separator();
            }
        });
    changed
}

fn sidebar(ui: &mut Ui, app: &mut App) {
    let rect = ui.max_rect().shrink2(vec2(10.0, 10.0));
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(22),
        Color32::from_rgba_unmultiplied(255, 255, 255, 218),
    );
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(22),
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 210)),
        egui::StrokeKind::Inside,
    );

    ui.scope_builder(
        egui::UiBuilder::new().max_rect(rect.shrink(16.0)),
        |ui| {
            ui.set_width(rect.width() - 32.0);
            ui.horizontal(|ui| {
                let (icon_rect, _) = ui.allocate_exact_size(vec2(42.0, 42.0), Sense::hover());
                ui.painter()
                    .rect_filled(icon_rect, CornerRadius::same(13), ACCENT);
                ui.painter().text(
                    icon_rect.center(),
                    Align2::CENTER_CENTER,
                    "⇄",
                    FontId::proportional(20.0),
                    Color32::WHITE,
                );
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new("编码转换器").size(16.0).strong().color(INK));
                    ui.label(RichText::new("Encoding Converter").size(10.0).color(MUTED));
                });
            });
            ui.add_space(14.0);
            for tab in Tab::ALL {
                if nav_item(ui, app.tab == tab, tab) {
                    app.tab = tab;
                }
                ui.add_space(2.0);
            }
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(6.0);
            ui.label(RichText::new("小提示").size(11.5).strong().color(MUTED));
            ui.label(
                RichText::new("拖入文件会自动跳到批量页；所有设置自动保存。")
                    .size(11.0)
                    .color(MUTED),
            );
            ui.with_layout(egui::Layout::bottom_up(Align::LEFT), |ui| {
                ui.add_space(4.0);
                ui.hyperlink_to(RichText::new("GitHub 仓库").size(11.0), REPO_URL);
                ui.label(
                    RichText::new(format!("v{} · Rust + egui", env!("CARGO_PKG_VERSION")))
                        .size(10.5)
                        .color(MUTED),
                );
            });
        },
    );
}

fn nav_item(ui: &mut Ui, selected: bool, tab: Tab) -> bool {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 54.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        let bg = if selected {
            ACCENT
        } else if hovered {
            Color32::from_rgba_unmultiplied(255, 255, 255, 240)
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect_filled(rect, CornerRadius::same(13), bg);
        let icon_rect = Rect::from_min_size(rect.min + vec2(10.0, 12.0), Vec2::splat(30.0));
        let icon_bg = if selected {
            Color32::from_rgba_unmultiplied(255, 255, 255, 46)
        } else {
            ACCENT_SOFT
        };
        ui.painter().rect_filled(icon_rect, CornerRadius::same(9), icon_bg);
        ui.painter().text(
            icon_rect.center(),
            Align2::CENTER_CENTER,
            tab.icon(),
            FontId::proportional(14.0),
            if selected { Color32::WHITE } else { ACCENT },
        );
        let title_pos = Pos2::new(rect.left() + 50.0, rect.center().y - 8.0);
        ui.painter().text(
            title_pos,
            Align2::LEFT_CENTER,
            tab.title(),
            FontId::proportional(13.5),
            if selected { Color32::WHITE } else { INK },
        );
        ui.painter().text(
            Pos2::new(title_pos.x, title_pos.y + 17.0),
            Align2::LEFT_CENTER,
            short_hint(tab),
            FontId::proportional(10.5),
            if selected {
                Color32::from_rgba_unmultiplied(255, 255, 255, 190)
            } else {
                MUTED
            },
        );
    }
    response.clicked()
}

fn short_hint(tab: Tab) -> &'static str {
    match tab {
        Tab::Text => "文字 ⇄ 编码",
        Tab::Radix => "二进制 / 十六进制",
        Tab::BaseN => "Base64 等",
        Tab::Escape => "URL / HTML / 摩尔斯",
        Tab::Batch => "文件转码",
        Tab::Detect => "识别编码",
    }
}

fn scroll_box(ui: &mut Ui, text: &str) {
    egui::ScrollArea::vertical()
        .max_height(240.0)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add(
                TextEdit::multiline(&mut text.to_string())
                    .desired_width(f32::INFINITY)
                    .interactive(false)
                    .font(egui::TextStyle::Monospace),
            );
        });
}

fn error_banner(ui: &mut Ui, text: &str) {
    Frame::new()
        .fill(DANGER_SOFT)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.label(RichText::new(format!("⚠ {text}")).size(12.5).color(DANGER));
        });
}

fn quick_actions(ui: &mut Ui, app: &mut App) {
    ui.label(RichText::new("常用操作").size(12.5).strong().color(MUTED));
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        if chip(ui, false, "批量转文件", "跳到文件批量转码页") {
            app.tab = Tab::Batch;
        }
        if chip(ui, false, "进制换算", "2–36 进制互转") {
            app.tab = Tab::Radix;
        }
        if chip(ui, false, "Base64 编解码", "文本与 Base64 互转") {
            app.tab = Tab::BaseN;
        }
        if chip(ui, false, "识别文件编码", "看看文件是什么编码") {
            app.tab = Tab::Detect;
        }
    });
}

fn dashed_rect(painter: &egui::Painter, rect: Rect, stroke: Stroke) {
    dashed_line(painter, rect.left_top(), rect.right_top(), stroke);
    dashed_line(painter, rect.right_top(), rect.right_bottom(), stroke);
    dashed_line(painter, rect.right_bottom(), rect.left_bottom(), stroke);
    dashed_line(painter, rect.left_bottom(), rect.left_top(), stroke);
}

fn dashed_line(painter: &egui::Painter, a: Pos2, b: Pos2, stroke: Stroke) {
    let delta = b - a;
    let length = delta.length();
    if length <= 0.0 {
        return;
    }
    let dir = delta / length;
    let (dash, gap) = (7.0, 5.0);
    let mut t = 0.0;
    while t < length {
        let end = (t + dash).min(length);
        painter.line_segment([a + dir * t, a + dir * end], stroke);
        t = end + gap;
    }
}

fn hex_bytes(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "（结果会实时显示在这里）".into();
    }
    let shown = &bytes[..bytes.len().min(512)];
    let mut out = shown
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ");
    if bytes.len() > 512 {
        out.push_str(&format!(" …（共 {} 字节，仅显示前 512）", bytes.len()));
    }
    out
}

fn parse_hex_bytes(input: &str) -> Result<Vec<u8>, String> {
    let cleaned: String = input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ',' && *c != '-')
        .collect();
    if cleaned.is_empty() {
        return Ok(Vec::new());
    }
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

fn convert_numeral(input: &str, system: NumeralSystem) -> Result<String, String> {
    let text = input.trim();
    match system {
        NumeralSystem::Roman => {
            let n: u32 = text.parse().map_err(|_| "请输入 1–3999 的整数")?;
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

/// 启动 GUI。
pub fn run_gui() -> anyhow::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 820.0])
            .with_min_inner_size([900.0, 620.0])
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
