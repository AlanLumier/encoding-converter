//! 界面设置持久化：扁平 JSON 键值存储，不依赖 serde。
//!
//! 保存位置：Windows `%APPDATA%\encoding-converter\settings.json`；
//! Linux/其它 `$XDG_CONFIG_HOME/encoding-converter/settings.json`（缺省 `~/.config/...`）。
//! 可用环境变量 `ENCODING_CONVERTER_CONFIG_DIR` 覆盖目录，便于测试。

use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub tab: String,
    pub cs_source: String,
    pub cs_target: String,
    pub radix_from: u32,
    pub radix_to: u32,
    pub base_kind: String,
    pub escape_kind: String,
    pub batch_target: String,
    pub batch_output_dir: String,
    pub batch_suffix: String,
    pub batch_overwrite: bool,
    pub batch_keep_bom: bool,
    pub detect_path: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            tab: "text".into(),
            cs_source: "utf-8".into(),
            cs_target: "gbk".into(),
            radix_from: 10,
            radix_to: 16,
            base_kind: "base64".into(),
            escape_kind: "url".into(),
            batch_target: "utf-8".into(),
            batch_output_dir: String::new(),
            batch_suffix: String::new(),
            batch_overwrite: false,
            batch_keep_bom: false,
            detect_path: String::new(),
        }
    }
}

impl Settings {
    pub fn path() -> PathBuf {
        if let Ok(dir) = std::env::var("ENCODING_CONVERTER_CONFIG_DIR") {
            return PathBuf::from(dir).join("settings.json");
        }
        #[cfg(windows)]
        {
            if let Ok(appdata) = std::env::var("APPDATA") {
                return PathBuf::from(appdata)
                    .join("encoding-converter")
                    .join("settings.json");
            }
        }
        let base = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_default();
                PathBuf::from(home).join(".config")
            });
        base.join("encoding-converter").join("settings.json")
    }

    pub fn load() -> Self {
        let Ok(text) = std::fs::read_to_string(Self::path()) else {
            return Self::default();
        };
        let values = parse_flat_json(&text).unwrap_or_default();
        let mut settings = Self::default();
        if let Some(v) = values.get("tab") {
            settings.tab = v.clone();
        }
        if let Some(v) = values.get("cs_source") {
            settings.cs_source = v.clone();
        }
        if let Some(v) = values.get("cs_target") {
            settings.cs_target = v.clone();
        }
        if let Some(v) = values.get("radix_from").and_then(|v| v.parse().ok()) {
            settings.radix_from = v;
        }
        if let Some(v) = values.get("radix_to").and_then(|v| v.parse().ok()) {
            settings.radix_to = v;
        }
        if let Some(v) = values.get("base_kind") {
            settings.base_kind = v.clone();
        }
        if let Some(v) = values.get("escape_kind") {
            settings.escape_kind = v.clone();
        }
        if let Some(v) = values.get("batch_target") {
            settings.batch_target = v.clone();
        }
        if let Some(v) = values.get("batch_output_dir") {
            settings.batch_output_dir = v.clone();
        }
        if let Some(v) = values.get("batch_suffix") {
            settings.batch_suffix = v.clone();
        }
        if let Some(v) = values.get("batch_overwrite") {
            settings.batch_overwrite = v == "true";
        }
        if let Some(v) = values.get("batch_keep_bom") {
            settings.batch_keep_bom = v == "true";
        }
        if let Some(v) = values.get("detect_path") {
            settings.detect_path = v.clone();
        }
        settings
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, self.to_json())
    }

    fn to_json(&self) -> String {
        let fields: [(&str, String); 13] = [
            ("tab", self.tab.clone()),
            ("cs_source", self.cs_source.clone()),
            ("cs_target", self.cs_target.clone()),
            ("radix_from", self.radix_from.to_string()),
            ("radix_to", self.radix_to.to_string()),
            ("base_kind", self.base_kind.clone()),
            ("escape_kind", self.escape_kind.clone()),
            ("batch_target", self.batch_target.clone()),
            ("batch_output_dir", self.batch_output_dir.clone()),
            ("batch_suffix", self.batch_suffix.clone()),
            ("batch_overwrite", self.batch_overwrite.to_string()),
            ("batch_keep_bom", self.batch_keep_bom.to_string()),
            ("detect_path", self.detect_path.clone()),
        ];
        let body = fields
            .iter()
            .map(|(key, value)| format!("\"{}\":\"{}\"", key, escape_json(value)))
            .collect::<Vec<_>>()
            .join(",");
        format!("{{{body}}}\n")
    }
}

fn escape_json(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// 解析扁平的 `{"key":"value"}` 对象（只支持字符串值，数字/布尔以字符串保存）。
fn parse_flat_json(text: &str) -> Option<BTreeMap<String, String>> {
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0usize;
    skip_ws(&chars, &mut index);
    if chars.get(index) != Some(&'{') {
        return None;
    }
    index += 1;
    let mut map = BTreeMap::new();
    loop {
        skip_ws(&chars, &mut index);
        match chars.get(index) {
            Some('}') => break,
            Some('"') => {
                index += 1;
                let key = parse_string(&chars, &mut index)?;
                skip_ws(&chars, &mut index);
                if chars.get(index) != Some(&':') {
                    return None;
                }
                index += 1;
                skip_ws(&chars, &mut index);
                if chars.get(index) != Some(&'"') {
                    return None;
                }
                index += 1;
                let value = parse_string(&chars, &mut index)?;
                map.insert(key, value);
                skip_ws(&chars, &mut index);
                match chars.get(index) {
                    Some(',') => index += 1,
                    Some('}') => break,
                    _ => return None,
                }
            }
            _ => return None,
        }
    }
    Some(map)
}

fn skip_ws(chars: &[char], index: &mut usize) {
    while matches!(chars.get(*index), Some(c) if c.is_whitespace()) {
        *index += 1;
    }
}

fn parse_string(chars: &[char], index: &mut usize) -> Option<String> {
    let mut out = String::new();
    while let Some(&c) = chars.get(*index) {
        *index += 1;
        match c {
            '"' => return Some(out),
            '\\' => {
                let esc = *chars.get(*index)?;
                *index += 1;
                match esc {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    '/' => out.push('/'),
                    'u' => {
                        let hex: String = chars.get(*index..*index + 4)?.iter().collect();
                        *index += 4;
                        let code = u32::from_str_radix(&hex, 16).ok()?;
                        out.push(char::from_u32(code)?);
                    }
                    _ => return None,
                }
            }
            c => out.push(c),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_roundtrip() {
        let mut settings = Settings::default();
        settings.cs_source = "gb18030".into();
        settings.batch_output_dir = "C:\\导出\\带\"引号\"".into();
        settings.radix_from = 2;
        settings.batch_overwrite = true;
        let text = settings.to_json();
        let map = parse_flat_json(&text).expect("应当能解析自己写出的 JSON");
        assert_eq!(map.get("cs_source").map(String::as_str), Some("gb18030"));
        assert_eq!(map.get("radix_from").map(String::as_str), Some("2"));
        assert_eq!(map.get("batch_overwrite").map(String::as_str), Some("true"));
        assert!(map.get("batch_output_dir").unwrap().contains("引号"));
    }

    #[test]
    fn broken_json_falls_back() {
        assert!(parse_flat_json("{").is_none());
        assert!(parse_flat_json("not json").is_none());
    }
}
