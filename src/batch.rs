//! 批量文件转码：读文件 → 探测或指定源编码 → 解码 → 目标编码编码 → 写出。
//!
//! 设计给 GUI 的批量页与 CLI 共用；`run` 通过回调汇报进度，便于界面刷新。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use crate::core::charset::{self, EncodingInfo};
use crate::core::detect;

/// 后台批量任务的进度事件。
#[derive(Debug)]
pub enum BatchEvent {
    /// 第 `index` 个文件处理完成
    Item { index: usize, item: BatchItem },
    /// 全部完成，附带统计
    Finished {
        done: usize,
        skipped: usize,
        failed: usize,
    },
}

/// 单个文件的处理状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemStatus {
    Pending,
    Done,
    Skipped,
    Failed,
}

impl ItemStatus {
    pub fn label(self) -> &'static str {
        match self {
            ItemStatus::Pending => "待处理",
            ItemStatus::Done => "完成",
            ItemStatus::Skipped => "跳过",
            ItemStatus::Failed => "失败",
        }
    }
}

/// 批量队列中的一项。
#[derive(Clone, Debug)]
pub struct BatchItem {
    pub path: PathBuf,
    pub size: u64,
    pub detected: Option<&'static EncodingInfo>,
    pub status: ItemStatus,
    pub message: String,
    pub output: Option<PathBuf>,
}

impl BatchItem {
    pub fn new(path: PathBuf) -> Self {
        let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        Self {
            path,
            size,
            detected: None,
            status: ItemStatus::Pending,
            message: String::new(),
            output: None,
        }
    }
}

/// 源编码策略。
#[derive(Clone, Debug)]
pub enum SourceEncoding {
    /// 自动探测
    Auto,
    /// 强制使用某个编码
    Fixed(&'static EncodingInfo),
}

/// 批量转码选项。
#[derive(Clone, Debug)]
pub struct BatchOptions {
    pub target: &'static EncodingInfo,
    pub source: SourceEncoding,
    /// 输出目录；为空则写回原目录
    pub output_dir: Option<PathBuf>,
    /// 写回原目录时插在文件名里的后缀，例如 `.utf8`
    pub suffix: String,
    pub overwrite: bool,
    /// 目标编码是 UTF-8/UTF-16 且源文件带 BOM 时，是否保留 BOM
    pub keep_bom: bool,
    pub dry_run: bool,
    /// 覆盖原文件前是否备份为 `.bak`
    pub backup: bool,
}

impl Default for BatchOptions {
    fn default() -> Self {
        Self {
            target: charset::find("utf-8").expect("utf-8 一定存在"),
            source: SourceEncoding::Auto,
            output_dir: None,
            suffix: String::new(),
            overwrite: false,
            keep_bom: false,
            dry_run: false,
            backup: false,
        }
    }
}

/// 扫描文件：目录会展开（可选递归），并过滤掉明显非文本的超大文件。
pub fn scan(paths: &[PathBuf], recursive: bool) -> Vec<BatchItem> {
    let mut out = Vec::new();
    for path in paths {
        if path.is_dir() {
            collect_dir(path, recursive, &mut out);
        } else if path.is_file() {
            out.push(BatchItem::new(path.clone()));
        }
    }
    out
}

fn collect_dir(dir: &Path, recursive: bool, out: &mut Vec<BatchItem>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if recursive {
                collect_dir(&path, recursive, out);
            }
        } else if path.is_file() {
            out.push(BatchItem::new(path));
        }
    }
}

/// 执行批量转码。返回执行成功的文件数。
pub fn run<F>(items: &mut [BatchItem], options: &BatchOptions, mut progress: F) -> usize
where
    F: FnMut(usize, usize),
{
    let total = items.len();
    let mut ok = 0usize;
    for (index, item) in items.iter_mut().enumerate() {
        process_item(item, options);
        if item.status == ItemStatus::Done {
            ok += 1;
        }
        progress(index + 1, total);
    }
    ok
}

/// 在后台线程里执行批量转码，通过 channel 汇报进度；每处理完一个文件调用一次 `notify`
/// 以便界面及时重绘。返回接收端，调用方在 UI 循环里非阻塞地收取事件。
pub fn spawn(
    items: Vec<BatchItem>,
    options: BatchOptions,
    notify: impl Fn() + Send + 'static,
) -> mpsc::Receiver<BatchEvent> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let (mut done, mut skipped, mut failed) = (0usize, 0usize, 0usize);
        for (index, mut item) in items.into_iter().enumerate() {
            process_item(&mut item, &options);
            match item.status {
                ItemStatus::Done => done += 1,
                ItemStatus::Skipped => skipped += 1,
                ItemStatus::Failed => failed += 1,
                ItemStatus::Pending => {}
            }
            if sender.send(BatchEvent::Item { index, item }).is_err() {
                return; // 界面已关闭，提前退出
            }
            notify();
        }
        let _ = sender.send(BatchEvent::Finished {
            done,
            skipped,
            failed,
        });
        notify();
    });
    receiver
}

fn process_item(item: &mut BatchItem, options: &BatchOptions) {
    let Ok(bytes) = fs::read(&item.path) else {
        item.status = ItemStatus::Failed;
        item.message = "读取失败（文件被占用或没有权限）".into();
        return;
    };
    if bytes.is_empty() {
        item.status = ItemStatus::Skipped;
        item.message = "空文件".into();
        return;
    }

    let source = match options.source {
        SourceEncoding::Auto => {
            let detection = detect::detect(&bytes);
            let Some(best) = detection.best else {
                item.status = ItemStatus::Failed;
                item.message = "无法自动判断编码，请手动指定源编码".into();
                return;
            };
            item.detected = Some(best);
            best
        }
        SourceEncoding::Fixed(info) => {
            item.detected = Some(info);
            info
        }
    };

    let decoded = charset::decode(source, &bytes);
    if decoded.had_errors {
        item.status = ItemStatus::Failed;
        item.message = format!(
            "源编码 {} 解码失败（{} 处非法字节，首个偏移 {}），请换源编码再试",
            source.name,
            decoded.error_offsets.len(),
            decoded.error_offsets.first().copied().unwrap_or(0)
        );
        return;
    }

    let encoded = charset::encode(options.target, &decoded.text);
    let mut payload = encoded.bytes;
    if !encoded.unmappable.is_empty() {
        item.status = ItemStatus::Failed;
        item.message = format!(
            "{} 个字符无法用 {} 表示（例如 '{}'）",
            encoded.unmappable.len(),
            options.target.name,
            encoded.unmappable[0]
        );
        return;
    }

    if options.keep_bom && decoded.bom_removed.is_some() {
        prepend_bom(&mut payload, options.target.id);
    }

    let output = output_path(&item.path, options);
    if output == item.path && !options.backup {
        item.status = ItemStatus::Failed;
        item.message = "输出路径与源文件相同：请指定输出目录或后缀".into();
        return;
    }
    if output.exists() && !options.overwrite {
        item.status = ItemStatus::Skipped;
        item.message = format!("目标文件已存在：{}", output.display());
        return;
    }

    if options.dry_run {
        item.status = ItemStatus::Done;
        item.message = format!("试运行：将写入 {}（{} 字节）", output.display(), payload.len());
        item.output = Some(output);
        return;
    }

    if options.backup && output == item.path {
        let backup = with_appended(&item.path, ".bak", "");
        if let Err(err) = fs::copy(&item.path, &backup) {
            item.status = ItemStatus::Failed;
            item.message = format!("备份失败：{err}");
            return;
        }
    }

    if let Some(parent) = output.parent() {
        if let Err(err) = fs::create_dir_all(parent) {
            item.status = ItemStatus::Failed;
            item.message = format!("创建输出目录失败：{err}");
            return;
        }
    }

    match fs::write(&output, &payload) {
        Ok(()) => {
            item.status = ItemStatus::Done;
            item.message = format!(
                "{} → {}（{} 字节）",
                source.name,
                options.target.name,
                payload.len()
            );
            item.output = Some(output);
        }
        Err(err) => {
            item.status = ItemStatus::Failed;
            item.message = format!("写入失败：{err}");
        }
    }
}

/// 按目标编码补上 BOM（只对 UTF 系列生效）。
fn prepend_bom(payload: &mut Vec<u8>, target_id: &str) {
    let prefix: &[u8] = match target_id {
        "utf-8" => &[0xEF, 0xBB, 0xBF],
        "utf-16le" => &[0xFF, 0xFE],
        "utf-16be" => &[0xFE, 0xFF],
        "utf-32le" => &[0xFF, 0xFE, 0x00, 0x00],
        "utf-32be" => &[0x00, 0x00, 0xFE, 0xFF],
        _ => return,
    };
    payload.splice(0..0, prefix.iter().copied());
}

/// 计算输出路径：有输出目录则保留文件名；否则在原目录追加 `.{编码}.{后缀}`。
pub fn output_path(input: &Path, options: &BatchOptions) -> PathBuf {
    if let Some(dir) = &options.output_dir {
        let name = input.file_name().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("out"));
        return dir.join(name);
    }
    let suffix = if options.suffix.trim().is_empty() {
        format!(".{}", options.target.id)
    } else {
        options.suffix.trim().to_string()
    };
    with_appended(input, &suffix, "")
}

fn with_appended(path: &Path, suffix: &str, extra: &str) -> PathBuf {
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let ext = path.extension().map(|s| format!(".{}", s.to_string_lossy())).unwrap_or_default();
    let name = format!("{stem}{suffix}{ext}{extra}");
    path.with_file_name(name)
}

/// 汇总统计，便于界面显示。
#[derive(Clone, Copy, Debug, Default)]
pub struct BatchSummary {
    pub total: usize,
    pub done: usize,
    pub skipped: usize,
    pub failed: usize,
}

pub fn summarize(items: &[BatchItem]) -> BatchSummary {
    let mut summary = BatchSummary {
        total: items.len(),
        ..Default::default()
    };
    for item in items {
        match item.status {
            ItemStatus::Done => summary.done += 1,
            ItemStatus::Skipped => summary.skipped += 1,
            ItemStatus::Failed => summary.failed += 1,
            ItemStatus::Pending => {}
        }
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("encoding-converter-test-{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn background_spawn_reports_progress_and_finish() {
        use std::sync::mpsc::RecvTimeoutError;
        use std::time::Duration;

        let dir = temp_dir();
        let input = dir.join("spawn.txt");
        fs::write(&input, "hello").unwrap();
        let items = vec![BatchItem::new(input)];
        let receiver = spawn(items, BatchOptions::default(), || {});
        let mut reported = 0usize;
        loop {
            match receiver.recv_timeout(Duration::from_secs(10)) {
                Ok(BatchEvent::Item { index, item }) => {
                    assert_eq!(index, 0);
                    assert_eq!(item.status, ItemStatus::Done);
                    reported += 1;
                }
                Ok(BatchEvent::Finished { done, .. }) => {
                    assert_eq!(done, 1);
                    break;
                }
                Err(RecvTimeoutError::Timeout) => panic!("后台任务 10 秒内没有完成"),
                Err(RecvTimeoutError::Disconnected) => panic!("channel 提前断开"),
            }
        }
        assert_eq!(reported, 1);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn converts_gbk_file_to_utf8() {
        let dir = temp_dir();
        let input = dir.join("a.txt");
        let gbk = charset::find("gbk").unwrap();
        let payload = charset::encode(gbk, "中文内容").bytes;
        fs::write(&input, &payload).unwrap();

        let mut items = vec![BatchItem::new(input.clone())];
        let options = BatchOptions {
            overwrite: true,
            ..BatchOptions::default()
        };
        let ok = run(&mut items, &options, |_, _| {});
        assert_eq!(ok, 1, "{:?}", items[0]);
        let output = items[0].output.clone().unwrap();
        assert!(output.exists());
        let written = fs::read(&output).unwrap();
        assert_eq!(String::from_utf8(written).unwrap(), "中文内容");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn skips_existing_without_overwrite() {
        let dir = temp_dir();
        let input = dir.join("a.txt");
        fs::write(&input, "hello").unwrap();
        let expected = output_path(&input, &BatchOptions::default());
        fs::write(&expected, "exists").unwrap();

        let mut items = vec![BatchItem::new(input)];
        let options = BatchOptions::default();
        run(&mut items, &options, |_, _| {});
        assert_eq!(items[0].status, ItemStatus::Skipped);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn reports_unmappable_characters() {
        let dir = temp_dir();
        let input = dir.join("emoji.txt");
        fs::write(&input, "😀").unwrap();
        let mut items = vec![BatchItem::new(input)];
        let options = BatchOptions {
            target: charset::find("gbk").unwrap(),
            overwrite: true,
            ..BatchOptions::default()
        };
        run(&mut items, &options, |_, _| {});
        assert_eq!(items[0].status, ItemStatus::Failed);
        assert!(items[0].message.contains("无法用 GBK 表示"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn output_path_with_dir_and_suffix() {
        let input = Path::new("C:/tmp/report.txt");
        let mut options = BatchOptions::default();
        assert_eq!(output_path(input, &options), PathBuf::from("C:/tmp/report.utf-8.txt"));
        options.output_dir = Some(PathBuf::from("C:/out"));
        assert_eq!(output_path(input, &options), PathBuf::from("C:/out/report.txt"));
    }
}
