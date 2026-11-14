//! 自研 codec：encoding_rs 不提供的编码（Latin-1、ASCII、UTF-32、UCS-2、CESU-8 系列）。

use super::{DecodeResult, EncodeResult};

/// 自研编码器集合。每个变体都是无状态的，可以安全地在多线程间共享。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CustomCodec {
    /// 真正的 ISO-8859-1（与 windows-1252 不同，0x80–0x9F 是控制字符）
    Latin1,
    /// 7 位 ASCII，>0x7F 视为错误
    Ascii,
    /// UTF-32 小端，4 字节定长
    Utf32Le,
    /// UTF-32 大端，4 字节定长
    Utf32Be,
    /// UTF-32，按 BOM 判断字节序，无 BOM 默认小端
    Utf32,
    /// UCS-2 小端（仅 BMP，遇到代理项报错）
    Ucs2Le,
    /// UCS-2 大端
    Ucs2Be,
    /// CESU-8：非 BMP 字符以两个 3 字节代理项编码
    Cesu8,
    /// Modified UTF-8（Java）：CESU-8 且 NUL 写作 C0 80
    Mutf8,
    /// WTF-8：允许代理项单独出现，本项目不表示孤立代理项，按错误处理
    Wtf8,
}

pub static LATIN1: CustomCodec = CustomCodec::Latin1;
pub static ASCII: CustomCodec = CustomCodec::Ascii;
pub static UTF32_LE: CustomCodec = CustomCodec::Utf32Le;
pub static UTF32_BE: CustomCodec = CustomCodec::Utf32Be;
pub static UTF32: CustomCodec = CustomCodec::Utf32;
pub static UCS2_LE: CustomCodec = CustomCodec::Ucs2Le;
pub static UCS2_BE: CustomCodec = CustomCodec::Ucs2Be;
pub static CESU8: CustomCodec = CustomCodec::Cesu8;
pub static MUTF8: CustomCodec = CustomCodec::Mutf8;
pub static WTF8: CustomCodec = CustomCodec::Wtf8;

impl CustomCodec {
    pub fn decode(self, bytes: &[u8]) -> DecodeResult {
        match self {
            CustomCodec::Latin1 => decode_latin1(bytes),
            CustomCodec::Ascii => decode_ascii(bytes),
            CustomCodec::Utf32Le => decode_utf32(bytes, true),
            CustomCodec::Utf32Be => decode_utf32(bytes, false),
            CustomCodec::Utf32 => decode_utf32_auto(bytes),
            CustomCodec::Ucs2Le => decode_ucs2(bytes, true),
            CustomCodec::Ucs2Be => decode_ucs2(bytes, false),
            CustomCodec::Cesu8 => decode_modified_utf8(bytes, false),
            CustomCodec::Mutf8 => decode_modified_utf8(bytes, true),
            CustomCodec::Wtf8 => decode_modified_utf8(bytes, false),
        }
    }

    pub fn encode(self, text: &str) -> EncodeResult {
        match self {
            CustomCodec::Latin1 => encode_latin1(text),
            CustomCodec::Ascii => encode_ascii(text),
            CustomCodec::Utf32Le => encode_utf32(text, true),
            CustomCodec::Utf32Be => encode_utf32(text, false),
            CustomCodec::Utf32 => encode_utf32(text, true),
            CustomCodec::Ucs2Le => encode_ucs2(text, true),
            CustomCodec::Ucs2Be => encode_ucs2(text, false),
            CustomCodec::Cesu8 => encode_modified_utf8(text, false),
            CustomCodec::Mutf8 => encode_modified_utf8(text, true),
            CustomCodec::Wtf8 => encode_modified_utf8(text, false),
        }
    }
}

fn decode_latin1(bytes: &[u8]) -> DecodeResult {
    let text: String = bytes.iter().map(|&b| b as char).collect();
    DecodeResult {
        text,
        ..Default::default()
    }
}

fn encode_latin1(text: &str) -> EncodeResult {
    let mut bytes = Vec::with_capacity(text.len());
    let mut unmappable = Vec::new();
    for ch in text.chars() {
        if (ch as u32) <= 0xFF {
            bytes.push(ch as u8);
        } else {
            unmappable.push(ch);
        }
    }
    EncodeResult { bytes, unmappable }
}

fn decode_ascii(bytes: &[u8]) -> DecodeResult {
    let mut text = String::with_capacity(bytes.len());
    let mut errors = Vec::new();
    for (i, &b) in bytes.iter().enumerate() {
        if b < 0x80 {
            text.push(b as char);
        } else {
            if errors.len() < 64 {
                errors.push(i);
            }
            text.push('\u{FFFD}');
        }
    }
    DecodeResult {
        text,
        had_errors: !errors.is_empty(),
        error_offsets: errors,
        bom_removed: None,
    }
}

fn encode_ascii(text: &str) -> EncodeResult {
    let mut bytes = Vec::with_capacity(text.len());
    let mut unmappable = Vec::new();
    for ch in text.chars() {
        if (ch as u32) < 0x80 {
            bytes.push(ch as u8);
        } else {
            unmappable.push(ch);
        }
    }
    EncodeResult { bytes, unmappable }
}

fn decode_utf32(bytes: &[u8], little_endian: bool) -> DecodeResult {
    let (data, bom_removed) = match strip_utf32_bom(bytes) {
        Some((rest, label)) => (rest, Some(label)),
        None => (bytes, None),
    };
    let mut text = String::new();
    let mut errors = Vec::new();
    for (idx, chunk) in data.as_chunks::<4>().0.iter().enumerate() {
        let value = if little_endian {
            u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])
        } else {
            u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])
        };
        match char::from_u32(value) {
            Some(c) => text.push(c),
            None => {
                if errors.len() < 64 {
                    errors.push(idx * 4);
                }
                text.push('\u{FFFD}');
            }
        }
    }
    let tail = data.len() % 4;
    if tail != 0 {
        if errors.len() < 64 {
            errors.push(data.len() - tail);
        }
        text.push('\u{FFFD}');
    }
    DecodeResult {
        text,
        had_errors: !errors.is_empty(),
        error_offsets: errors,
        bom_removed,
    }
}

fn decode_utf32_auto(bytes: &[u8]) -> DecodeResult {
    match strip_utf32_bom(bytes) {
        Some((_, label)) => {
            let little = label == "UTF-32LE";
            decode_utf32_without_bom(bytes, little)
        }
        None => decode_utf32_without_bom(bytes, true),
    }
}

fn decode_utf32_without_bom(bytes: &[u8], little_endian: bool) -> DecodeResult {
    let mut result = decode_utf32(bytes, little_endian);
    result.bom_removed = None;
    result
}

fn strip_utf32_bom(bytes: &[u8]) -> Option<(&[u8], &'static str)> {
    if bytes.starts_with(&[0xFF, 0xFE, 0x00, 0x00]) {
        Some((&bytes[4..], "UTF-32LE"))
    } else if bytes.starts_with(&[0x00, 0x00, 0xFE, 0xFF]) {
        Some((&bytes[4..], "UTF-32BE"))
    } else {
        None
    }
}

fn encode_utf32(text: &str, little_endian: bool) -> EncodeResult {
    let mut bytes = Vec::with_capacity(text.len() * 4);
    for ch in text.chars() {
        let v = ch as u32;
        if little_endian {
            bytes.extend_from_slice(&v.to_le_bytes());
        } else {
            bytes.extend_from_slice(&v.to_be_bytes());
        }
    }
    EncodeResult {
        bytes,
        unmappable: Vec::new(),
    }
}

fn decode_ucs2(bytes: &[u8], little_endian: bool) -> DecodeResult {
    let mut text = String::new();
    let mut errors = Vec::new();
    for (idx, chunk) in bytes.as_chunks::<2>().0.iter().enumerate() {
        let value = if little_endian {
            u16::from_le_bytes([chunk[0], chunk[1]])
        } else {
            u16::from_be_bytes([chunk[0], chunk[1]])
        };
        if (0xD800..=0xDFFF).contains(&value) {
            if errors.len() < 64 {
                errors.push(idx * 2);
            }
            text.push('\u{FFFD}');
        } else {
            text.push(char::from_u32(value as u32).unwrap_or('\u{FFFD}'));
        }
    }
    if !bytes.len().is_multiple_of(2) {
        if errors.len() < 64 {
            errors.push(bytes.len() - 1);
        }
        text.push('\u{FFFD}');
    }
    DecodeResult {
        text,
        had_errors: !errors.is_empty(),
        error_offsets: errors,
        bom_removed: None,
    }
}

fn encode_ucs2(text: &str, little_endian: bool) -> EncodeResult {
    let mut bytes = Vec::with_capacity(text.len() * 2);
    let mut unmappable = Vec::new();
    for ch in text.chars() {
        let v = ch as u32;
        if v > 0xFFFF {
            unmappable.push(ch);
        } else if little_endian {
            bytes.extend_from_slice(&(v as u16).to_le_bytes());
        } else {
            bytes.extend_from_slice(&(v as u16).to_be_bytes());
        }
    }
    EncodeResult { bytes, unmappable }
}

/// CESU-8 / MUTF-8 / WTF-8 共用解码：按 UTF-8 的字节结构解析，但允许代理项用两个 3 字节序列表示。
fn decode_modified_utf8(bytes: &[u8], nul_c0_80: bool) -> DecodeResult {
    let mut units: Vec<u16> = Vec::with_capacity(bytes.len());
    let mut unit_offsets: Vec<usize> = Vec::with_capacity(bytes.len());
    let mut errors = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        let (cu, len) = if b < 0x80 {
            (b as u16, 1)
        } else if b & 0xE0 == 0xC0 {
            if i + 1 >= bytes.len() {
                if errors.len() < 64 {
                    errors.push(i);
                }
                (0xFFFD, 1)
            } else {
                let v = ((b as u16 & 0x1F) << 6) | (bytes[i + 1] as u16 & 0x3F);
                (v, 2)
            }
        } else if b & 0xF0 == 0xE0 {
            if i + 2 >= bytes.len() {
                if errors.len() < 64 {
                    errors.push(i);
                }
                (0xFFFD, 1)
            } else {
                let v = ((b as u16 & 0x0F) << 12)
                    | ((bytes[i + 1] as u16 & 0x3F) << 6)
                    | (bytes[i + 2] as u16 & 0x3F);
                (v, 3)
            }
        } else if b & 0xF8 == 0xF0 {
            // 4 字节序列在 CESU-8 中不合法，按错误处理并尝试按单个 U+FFFD 前进
            if errors.len() < 64 {
                errors.push(i);
            }
            (0xFFFD, 1)
        } else {
            if errors.len() < 64 {
                errors.push(i);
            }
            (0xFFFD, 1)
        };
        if nul_c0_80 && cu == 0 && len == 2 {
            // MUTF-8 的 C0 80 表示 NUL
        }
        units.push(cu);
        unit_offsets.push(i);
        i += len;
    }
    let mut text = String::with_capacity(units.len());
    let mut valid = 0usize;
    while valid < units.len() {
        let u = units[valid];
        if (0xD800..=0xDBFF).contains(&u) && valid + 1 < units.len() {
            let low = units[valid + 1];
            if (0xDC00..=0xDFFF).contains(&low) {
                let scalar = 0x10000 + (((u as u32 - 0xD800) << 10) | (low as u32 - 0xDC00));
                text.push(char::from_u32(scalar).unwrap_or('\u{FFFD}'));
                valid += 2;
                continue;
            }
        }
        if (0xD800..=0xDFFF).contains(&u) {
            if errors.len() < 64 {
                errors.push(unit_offsets[valid]);
            }
            text.push('\u{FFFD}');
        } else {
            text.push(char::from_u32(u as u32).unwrap_or('\u{FFFD}'));
        }
        valid += 1;
    }
    errors.sort_unstable();
    errors.dedup();
    DecodeResult {
        text,
        had_errors: !errors.is_empty(),
        error_offsets: errors,
        bom_removed: None,
    }
}

fn encode_modified_utf8(text: &str, nul_c0_80: bool) -> EncodeResult {
    let mut bytes = Vec::with_capacity(text.len() * 3);
    for ch in text.chars() {
        let v = ch as u32;
        if nul_c0_80 && v == 0 {
            bytes.extend_from_slice(&[0xC0, 0x80]);
            continue;
        }
        if v <= 0xFFFF {
            push_utf8_unit(&mut bytes, v as u16);
        } else {
            let scalar = v - 0x10000;
            let high = 0xD800 + ((scalar >> 10) as u16);
            let low = 0xDC00 + ((scalar & 0x3FF) as u16);
            push_utf8_unit(&mut bytes, high);
            push_utf8_unit(&mut bytes, low);
        }
    }
    EncodeResult {
        bytes,
        unmappable: Vec::new(),
    }
}

fn push_utf8_unit(out: &mut Vec<u8>, unit: u16) {
    if unit < 0x80 {
        out.push(unit as u8);
    } else if unit < 0x800 {
        out.push(0xC0 | (unit >> 6) as u8);
        out.push(0x80 | (unit & 0x3F) as u8);
    } else {
        out.push(0xE0 | (unit >> 12) as u8);
        out.push(0x80 | ((unit >> 6) & 0x3F) as u8);
        out.push(0x80 | (unit & 0x3F) as u8);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latin1_roundtrip() {
        let bytes = vec![0xE4, 0xB8, 0xAD];
        assert_eq!(CustomCodec::Latin1.decode(&bytes).text, "ä¸­");
        assert_eq!(CustomCodec::Latin1.encode("ä¸­").bytes, bytes);
    }

    #[test]
    fn ascii_reports_errors() {
        let r = CustomCodec::Ascii.decode(b"ab\xffc");
        assert!(r.had_errors);
        assert_eq!(r.error_offsets, vec![2]);
        assert!(CustomCodec::Ascii.encode("中").unmappable == vec!['中']);
    }

    #[test]
    fn utf32_roundtrip() {
        let r = CustomCodec::Utf32Le.encode("中文A");
        assert_eq!(r.bytes.len(), 12);
        assert_eq!(CustomCodec::Utf32Le.decode(&r.bytes).text, "中文A");
        let be = CustomCodec::Utf32Be.encode("中文A");
        assert_eq!(CustomCodec::Utf32Be.decode(&be.bytes).text, "中文A");
    }

    #[test]
    fn ucs2_surrogates_are_errors() {
        let r = CustomCodec::Ucs2Le.decode(&[0x00, 0xD8]);
        assert!(r.had_errors);
    }

    #[test]
    fn cesu8_roundtrip_emoji() {
        let encoded = CustomCodec::Cesu8.encode("😀");
        assert_eq!(encoded.bytes.len(), 6);
        assert_eq!(CustomCodec::Cesu8.decode(&encoded.bytes).text, "😀");
    }

    #[test]
    fn mutf8_nul() {
        let encoded = CustomCodec::Mutf8.encode("\0");
        assert_eq!(encoded.bytes, vec![0xC0, 0x80]);
        assert_eq!(CustomCodec::Mutf8.decode(&[0xC0, 0x80]).text, "\0");
    }
}
