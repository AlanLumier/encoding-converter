//! 表驱动的单字节代码页：DOS OEM、EBCDIC、老 Mac 等。
//!
//! 表数据由 `tools/gen-codepage-tables.ps1` 生成到 `tables.rs`；
//! 这里只负责按表做“字节 ↔ 字符”映射。

use super::{DecodeResult, EncodeResult};

/// 按表解码：`table[byte]` 是该字节对应的 Unicode 码点。
pub fn decode_table(table: &[u16; 256], bytes: &[u8]) -> DecodeResult {
    let mut text = String::with_capacity(bytes.len());
    let mut errors = Vec::new();
    for (index, &byte) in bytes.iter().enumerate() {
        match char::from_u32(table[byte as usize] as u32) {
            Some(ch) => text.push(ch),
            None => {
                if errors.len() < 64 {
                    errors.push(index);
                }
                text.push('\u{FFFD}');
            }
        }
    }
    DecodeResult {
        text,
        had_errors: !errors.is_empty(),
        error_offsets: errors,
        bom_removed: None,
    }
}

/// 按表编码：反查表一次性建立（256 项），遇到重复码点保留最小字节值。
pub fn encode_table(table: &[u16; 256], text: &str) -> EncodeResult {
    let mut reverse: Vec<(char, u8)> = Vec::with_capacity(256);
    for (byte, &code_point) in table.iter().enumerate() {
        if let Some(ch) = char::from_u32(code_point as u32) {
            if !reverse.iter().any(|(existing, _)| *existing == ch) {
                reverse.push((ch, byte as u8));
            }
        }
    }
    let mut bytes = Vec::with_capacity(text.len());
    let mut unmappable = Vec::new();
    for ch in text.chars() {
        match reverse.iter().find(|(c, _)| *c == ch) {
            Some((_, byte)) => bytes.push(*byte),
            None => unmappable.push(ch),
        }
    }
    EncodeResult { bytes, unmappable }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::charset::tables;

    #[test]
    fn cp437_known_bytes() {
        assert_eq!(decode_table(&tables::CP437, &[0x82]).text, "é");
        assert_eq!(encode_table(&tables::CP437, "é").bytes, vec![0x82]);
        assert_eq!(encode_table(&tables::CP437, "中").unmappable, vec!['中']);
    }

    #[test]
    fn ebcdic_037_letters() {
        assert_eq!(decode_table(&tables::EBCDIC_037, &[0xC1, 0xC2, 0xC3]).text, "ABC");
        assert_eq!(
            encode_table(&tables::EBCDIC_037, "ABC").bytes,
            vec![0xC1, 0xC2, 0xC3]
        );
    }

    #[test]
    fn mac_roman_accent() {
        assert_eq!(decode_table(&tables::MAC_ROMAN, &[0x8E]).text, "é");
    }

    #[test]
    fn every_table_round_trips_its_own_bytes() {
        for table in [
            &tables::CP437,
            &tables::CP850,
            &tables::CP852,
            &tables::CP866,
            &tables::EBCDIC_037,
            &tables::EBCDIC_500,
            &tables::MAC_ROMAN,
            &tables::MAC_TURKISH,
        ] {
            let decoded = decode_table(table, &(0u8..=255).collect::<Vec<u8>>());
            assert!(!decoded.text.is_empty());
            let encoded = encode_table(table, &decoded.text);
            assert!(encoded.unmappable.is_empty(), "表里应有全部字符的反向映射");
            let redecoded = decode_table(table, &encoded.bytes);
            assert_eq!(redecoded.text, decoded.text);
        }
    }
}
