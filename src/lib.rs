//! encoding-converter 库入口：核心转换逻辑、GUI 与 CLI 共用。

pub mod app;
pub mod batch;
pub mod core;
pub mod settings;

/// 程序入口：无参数启动 GUI；带参数走 CLI。
pub fn run() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        return app::run_gui();
    }
    cli::run(&args)
}

/// 简单的命令行接口（批量脚本用）。
pub mod cli {
    use std::path::PathBuf;

    use crate::batch::{self, BatchOptions, ItemStatus, SourceEncoding};
    use crate::core::charset;
    use crate::core::detect;

    pub fn run(args: &[String]) -> anyhow::Result<()> {
        // 子命令也接受 --help，避免把帮助参数当成文件路径
        if args.len() > 1 && args.iter().any(|a| a == "-h" || a == "--help") {
            print_help();
            return Ok(());
        }
        match args.first().map(String::as_str) {
            Some("-h" | "--help" | "help") => {
                print_help();
                Ok(())
            }
            Some("-V" | "--version") => {
                println!("encoding-converter {}", env!("CARGO_PKG_VERSION"));
                Ok(())
            }
            Some("detect") => {
                let path = args.get(1).ok_or_else(|| anyhow::anyhow!("用法：detect <文件>"))?;
                let bytes = std::fs::read(path)?;
                let result = detect::detect(&bytes);
                match result.best {
                    Some(best) => {
                        println!("最佳匹配：{}（{}）", best.name, best.note);
                        println!("置信度：{:.0}%", result.confidence * 100.0);
                        if let Some(bom) = result.bom {
                            println!("BOM：{bom}");
                        }
                        for candidate in &result.candidates {
                            println!(
                                "  - {:<14} {:>5.0}%  {}",
                                candidate.name,
                                candidate.score * 100.0,
                                candidate.note
                            );
                        }
                    }
                    None => println!("无法判断编码"),
                }
                Ok(())
            }
            Some("convert") => {
                let mut from = String::new();
                let mut to = String::new();
                let mut input: Option<PathBuf> = None;
                let mut output: Option<PathBuf> = None;
                let mut index = 1;
                while index < args.len() {
                    match args[index].as_str() {
                        "--from" => {
                            from = args.get(index + 1).cloned().unwrap_or_default();
                            index += 2;
                        }
                        "--to" => {
                            to = args.get(index + 1).cloned().unwrap_or_default();
                            index += 2;
                        }
                        "-o" | "--output" => {
                            output = args.get(index + 1).map(PathBuf::from);
                            index += 2;
                        }
                        other => {
                            input = Some(PathBuf::from(other));
                            index += 1;
                        }
                    }
                }
                let input = input.ok_or_else(|| {
                    anyhow::anyhow!("用法：convert --from <编码> --to <编码> <文件> [-o 输出]")
                })?;
                let source = if from.is_empty() {
                    None
                } else {
                    Some(charset::find(&from).ok_or_else(|| anyhow::anyhow!("未知编码：{from}"))?)
                };
                let target = charset::find(&to).ok_or_else(|| anyhow::anyhow!("未知编码：{to}"))?;
                let bytes = std::fs::read(&input)?;
                let source = match source {
                    Some(info) => info,
                    None => detect::detect(&bytes)
                        .best
                        .ok_or_else(|| anyhow::anyhow!("无法自动判断源编码，请用 --from 指定"))?,
                };
                let decoded = charset::decode(source, &bytes);
                if decoded.had_errors {
                    anyhow::bail!("源编码 {} 解码失败，请换 --from", source.name);
                }
                let encoded = charset::encode(target, &decoded.text);
                if !encoded.unmappable.is_empty() {
                    anyhow::bail!(
                        "{} 个字符无法用 {} 表示",
                        encoded.unmappable.len(),
                        target.name
                    );
                }
                match output {
                    Some(path) => std::fs::write(path, encoded.bytes)?,
                    None => print!("{}", String::from_utf8_lossy(&encoded.bytes)),
                }
                Ok(())
            }
            Some("batch") => {
                let mut to = String::from("utf-8");
                let mut out_dir: Option<PathBuf> = None;
                let mut inputs: Vec<PathBuf> = Vec::new();
                let mut overwrite = false;
                let mut index = 1;
                while index < args.len() {
                    match args[index].as_str() {
                        "--to" => {
                            to = args.get(index + 1).cloned().unwrap_or_default();
                            index += 2;
                        }
                        "--out" => {
                            out_dir = args.get(index + 1).map(PathBuf::from);
                            index += 2;
                        }
                        "--overwrite" => {
                            overwrite = true;
                            index += 1;
                        }
                        other => {
                            inputs.push(PathBuf::from(other));
                            index += 1;
                        }
                    }
                }
                if inputs.is_empty() {
                    anyhow::bail!(
                        "用法：batch --to <编码> [--out 目录] [--overwrite] <文件或目录>…"
                    );
                }
                let target = charset::find(&to).ok_or_else(|| anyhow::anyhow!("未知编码：{to}"))?;
                let mut items = batch::scan(&inputs, true);
                let options = BatchOptions {
                    target,
                    source: SourceEncoding::Auto,
                    output_dir: out_dir,
                    overwrite,
                    ..BatchOptions::default()
                };
                let total = items.len();
                batch::run(&mut items, &options, |done, _| {
                    eprintln!("[{done}/{total}]");
                });
                let mut failed = 0;
                let mut skipped = 0;
                for item in &items {
                    match item.status {
                        ItemStatus::Failed => {
                            failed += 1;
                            eprintln!("失败：{} — {}", item.path.display(), item.message);
                        }
                        ItemStatus::Skipped => {
                            skipped += 1;
                            eprintln!("跳过：{} — {}", item.path.display(), item.message);
                        }
                        _ => {}
                    }
                }
                println!(
                    "完成 {} 个文件，跳过 {} 个，失败 {} 个",
                    total - failed - skipped,
                    skipped,
                    failed
                );
                if failed > 0 {
                    anyhow::bail!("有 {failed} 个文件失败");
                }
                Ok(())
            }
            Some(other) => {
                eprintln!("未知命令：{other}\n");
                print_help();
                std::process::exit(2);
            }
            None => {
                print_help();
                Ok(())
            }
        }
    }

    fn print_help() {
        println!(
            r#"encoding-converter {}

用法：
  encoding-converter                 启动图形界面
  encoding-converter detect <文件>   显示编码探测结果
  encoding-converter convert --from <编码> --to <编码> <文件> [-o 输出]
                                     （--from 省略时自动探测）
  encoding-converter batch --to <编码> [--out 目录] [--overwrite] <文件或目录>…
  encoding-converter --help          显示帮助

示例：
  encoding-converter convert --from gbk --to utf-8 note.txt -o note.utf8.txt
  encoding-converter batch --to utf-8 --out ./utf8 ./downloads
  encoding-converter batch --to utf-8 --out ./utf8 --overwrite ./downloads
"#,
            env!("CARGO_PKG_VERSION")
        );
    }
}
