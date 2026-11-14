//! 编码自动探测：BOM → ASCII → UTF-8 校验 → UTF-16 特征 → chardetng 统计打分。

use super::charset::{self, EncodingInfo};
use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};

/// 参与打分的最大采样字节数，避免大文件拖慢界面。
pub const MAX_ANALYZE: usize = 256 * 1024;

#[derive(Clone, Debug)]
pub struct Candidate {
    pub id: &'static str,
    pub name: &'static str,
    pub score: f32,
    pub note: String,
}

#[derive(Clone, Debug, Default)]
pub struct Detection {
    pub best: Option<&'static EncodingInfo>,
    pub confidence: f32,
    pub candidates: Vec<Candidate>,
    pub bom: Option<&'static str>,
}

/// 探测编码。`bytes` 可以是完整文件，内部只取前 256 KiB 做分析。
pub fn detect(bytes: &[u8]) -> Detection {
    let sample = &bytes[..bytes.len().min(MAX_ANALYZE)];
    let mut detection = Detection::default();
    if sample.is_empty() {
        return detection;
    }

    // 1) BOM 最可靠
    if let Some(info) = charset::sniff_bom(bytes) {
        detection.best = Some(info);
        detection.confidence = 1.0;
        detection.bom = Some(info.name);
        detection.candidates.push(Candidate {
            id: info.id,
            name: info.name,
            score: 1.0,
            note: "BOM 字节标记".into(),
        });
        return detection;
    }

    // 2) 纯 ASCII：UTF-8 与 ASCII 等价，直接给 UTF-8。
    //    不能用“所有字节 < 0x80”判断，因为无 BOM 的 UTF-16 里 00 字节也满足这个条件。
    if sample.iter().all(|&b| b < 0x80) && !sample.contains(&0) {
        let info = charset::find("utf-8").expect("utf-8 一定存在");
        detection.best = Some(info);
        detection.confidence = 1.0;
        detection.candidates.push(Candidate {
            id: info.id,
            name: info.name,
            score: 1.0,
            note: "全部是 7 位 ASCII，可视为 UTF-8".into(),
        });
        return detection;
    }

    // 3) 无 BOM 的 UTF-16 特征：一半字节是 00。必须在 UTF-8 校验之前判断，
    //    因为 UTF-16 里的 00 字节恰好是合法的 UTF-8 NUL，会被误判成 UTF-8。
    if let Some(info) = guess_utf16(sample) {
        let score = decode_score(info, sample);
        if score > 0.9 {
            detection.best = Some(info);
            detection.confidence = score;
            detection.candidates.push(Candidate {
                id: info.id,
                name: info.name,
                score,
                note: "大量 00 字节，判定为无 BOM 的 UTF-16".into(),
            });
            return detection;
        }
    }

    // 4) 合法 UTF-8（含多字节序列）几乎可以确定
    if looks_like_utf8(bytes) {
        let info = charset::find("utf-8").expect("utf-8 一定存在");
        detection.best = Some(info);
        detection.confidence = 0.95;
        detection.candidates.push(Candidate {
            id: info.id,
            name: info.name,
            score: 0.95,
            note: "字节序列是合法 UTF-8".into(),
        });
        return detection;
    }

    // 5) chardetng 统计模型
    let mut detector = EncodingDetector::new(Iso2022JpDetection::Deny);
    detector.feed(sample, true);
    let guessed = detector.guess(None, Utf8Detection::Allow);

    let mut candidates = Vec::new();
    if let Some(info) = charset::by_ers(guessed) {
        let score = decode_score(info, sample);
        candidates.push(Candidate {
            id: info.id,
            name: info.name,
            score,
            note: "chardetng 统计模型首选".into(),
        });
    }

    // 6) 补充常见候选，便于用户手动改选
    for id in [
        "utf-8",
        "gb18030",
        "gbk",
        "big5",
        "shift_jis",
        "euc-kr",
        "windows-1252",
        "iso-8859-2",
        "koi8-r",
    ] {
        if candidates.iter().any(|c| c.id == id) {
            continue;
        }
        if let Some(info) = charset::find(id) {
            let score = decode_score(info, sample);
            if score >= 0.85 {
                candidates.push(Candidate {
                    id: info.id,
                    name: info.name,
                    score: score * 0.95,
                    note: "解码无替换字符".into(),
                });
            }
        }
    }

    candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    candidates.truncate(6);

    if let Some(first) = candidates.first() {
        detection.confidence = first.score;
        detection.best = charset::find(first.id);
    }
    detection.candidates = candidates;
    detection
}

fn decode_score(info: &EncodingInfo, sample: &[u8]) -> f32 {
    let decoded = charset::decode(info, sample);
    let total = decoded.text.chars().count().max(1);
    let bad = decoded.text.chars().filter(|&c| c == '\u{FFFD}').count();
    1.0 - (bad as f32 / total as f32)
}

fn looks_like_utf8(bytes: &[u8]) -> bool {
    // 含较多 00 字节的多半是 UTF-16/二进制，交给后续判断
    let nul_ratio = bytes.iter().filter(|&&b| b == 0).count() as f32 / bytes.len().max(1) as f32;
    if nul_ratio > 0.1 {
        return false;
    }
    match std::str::from_utf8(bytes) {
        Ok(_) => true,
        Err(e) => {
            // 末尾被截断的多字节序列不算错误
            e.valid_up_to() + 4 >= bytes.len()
        }
    }
}

fn guess_utf16(sample: &[u8]) -> Option<&'static EncodingInfo> {
    if sample.len() < 8 {
        return None;
    }
    let even_nul = sample.iter().step_by(2).filter(|&&b| b == 0).count();
    let odd_nul = sample.iter().skip(1).step_by(2).filter(|&&b| b == 0).count();
    let half = sample.len() / 2;
    if odd_nul > half * 3 / 10 && even_nul < half / 10 {
        charset::find("utf-16le")
    } else if even_nul > half * 3 / 10 && odd_nul < half / 10 {
        charset::find("utf-16be")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_utf8() {
        let d = detect("你好，世界".as_bytes());
        assert_eq!(d.best.map(|e| e.id), Some("utf-8"));
        assert!(d.confidence > 0.9);
    }

    #[test]
    fn detects_ascii_as_utf8() {
        let d = detect(b"hello world");
        assert_eq!(d.best.map(|e| e.id), Some("utf-8"));
        assert_eq!(d.confidence, 1.0);
    }

    #[test]
    fn detects_bom() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice("中文".as_bytes());
        let d = detect(&bytes);
        assert_eq!(d.bom, Some("UTF-8"));
        assert_eq!(d.best.map(|e| e.id), Some("utf-8"));
    }

    #[test]
    fn detects_gbk_text() {
        let gbk = charset::find("gbk").unwrap();
        let bytes = charset::encode(gbk, "简体中文测试，这是一段较长的文本用于统计判断。").bytes;
        let d = detect(&bytes);
        assert!(
            d.best.map(|e| e.id).is_some(),
            "应该给出候选编码，实际：{:?}",
            d.candidates
        );
        let ids: Vec<_> = d.candidates.iter().map(|c| c.id).collect();
        assert!(ids.contains(&"gbk") || ids.contains(&"gb18030"), "候选={ids:?}");
    }

    #[test]
    fn detects_utf16_without_bom() {
        let text = "Hello, this is a UTF-16 sample with NUL bytes.";
        let mut bytes = Vec::new();
        for u in text.encode_utf16() {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        let d = detect(&bytes);
        assert_eq!(d.best.map(|e| e.id), Some("utf-16le"));
    }
}
