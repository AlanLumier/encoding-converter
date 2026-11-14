//! 转义 / 传输编码（URL、HTML、JSON、Unicode 转义、Quoted-Printable 等）。
//!
//! 统一约定：`encode` 把“人能读的原文”变成“适合放进 URL / HTML / JSON / 邮件
//! 等容器的文本”，`decode` 严格校验后还原。任何非法输入都返回带中文说明的
//! `Err`，绝不 panic，也不用 `unwrap` 处理用户数据。
//!
//! 几处容易踩坑、因此单独说明的取舍：
//! - URL：`+` 在这里**不**还原成空格。工具面向“百分号编码”本身，
//!   把 `+` 当空格是 application/x-www-form-urlencoded 的历史包袱，混在一起会
//!   让 `a+b` 的往返结果随环境变化。
//! - JSON / Unicode / C 字符串：转义只作用于文本内容，不加首尾引号；
//!   解码时若整串被一对未转义的 `"` 包住则先剥掉，方便直接粘贴 JSON 字面量。
//! - Quoted-Printable：换行也按 `=0D=0A` 编码，而不是变成硬换行。
//!   这样处理后往返是逐字节一致的；硬换行会让“原文里的 \r 是否保留”变得不可判。
//! - `\xNN` 与 URL 的 `%XX` 一样按**字节**还原，最后统一做 UTF-8 校验，
//!   因此 `\xE4\xB8\xAD` 能还原成「中」。

/// 转义编码种类。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EscapeKind {
    UrlPercent,
    HtmlEntity,
    HtmlNumericHex,
    HtmlNumericDec,
    Json,
    UnicodeEscape,
    CString,
    QuotedPrintable,
}

const HEX_UPPER: &[u8; 16] = b"0123456789ABCDEF";

impl EscapeKind {
    pub const ALL: [EscapeKind; 8] = [
        EscapeKind::UrlPercent,
        EscapeKind::HtmlEntity,
        EscapeKind::HtmlNumericHex,
        EscapeKind::HtmlNumericDec,
        EscapeKind::Json,
        EscapeKind::UnicodeEscape,
        EscapeKind::CString,
        EscapeKind::QuotedPrintable,
    ];

    /// 稳定的字符串标识，用于配置持久化。
    pub fn id(self) -> &'static str {
        match self {
            EscapeKind::UrlPercent => "url",
            EscapeKind::HtmlEntity => "html",
            EscapeKind::HtmlNumericHex => "html-hex",
            EscapeKind::HtmlNumericDec => "html-dec",
            EscapeKind::Json => "json",
            EscapeKind::UnicodeEscape => "unicode",
            EscapeKind::CString => "cstring",
            EscapeKind::QuotedPrintable => "qp",
        }
    }

    /// 界面展示名称。
    pub fn name(self) -> &'static str {
        match self {
            EscapeKind::UrlPercent => "URL 百分号编码",
            EscapeKind::HtmlEntity => "HTML 实体",
            EscapeKind::HtmlNumericHex => "HTML 数字实体（&#xXXXX;）",
            EscapeKind::HtmlNumericDec => "HTML 数字实体（&#DDDD;）",
            EscapeKind::Json => "JSON 字符串转义",
            EscapeKind::UnicodeEscape => "Unicode 转义（\\uXXXX）",
            EscapeKind::CString => "C 字符串转义",
            EscapeKind::QuotedPrintable => "Quoted-Printable（RFC 2045）",
        }
    }

    /// 一句话说明，用于界面上的灰色提示。
    pub fn note(self) -> &'static str {
        match self {
            EscapeKind::UrlPercent => {
                "按 UTF-8 字节转义，保留 A-Za-z0-9-_.~；解码时 '+' 不当作空格"
            }
            EscapeKind::HtmlEntity => {
                "只转义 & < > \" '；解码支持常见命名实体与十进制/十六进制数字引用，未知实体原样保留"
            }
            EscapeKind::HtmlNumericHex => {
                "非 ASCII、ASCII 控制字符以及 & < > \" ' 全部转成 &#xXXXX;"
            }
            EscapeKind::HtmlNumericDec => {
                "非 ASCII、ASCII 控制字符以及 & < > \" ' 全部转成 &#DDDD;"
            }
            EscapeKind::Json => {
                "转义引号、反斜杠与控制字符，非 ASCII 保留原文；不添加首尾引号，解码容忍包裹引号"
            }
            EscapeKind::UnicodeEscape => {
                "BMP 字符输出 \\uXXXX，增补平面输出代理对；解码另支持 \\u{...} 与 \\xXX"
            }
            EscapeKind::CString => {
                "可见字符原样，控制字符用 \\xNN，非 ASCII 用 \\uXXXX；可解回原文"
            }
            EscapeKind::QuotedPrintable => {
                "非打印字节写成 =XX；行宽不超过 76，软换行用 =CRLF；解码同时处理 =\\r\\n 与 =\\n"
            }
        }
    }

    /// 原文 → 转义文本。
    pub fn encode(self, text: &str) -> String {
        match self {
            EscapeKind::UrlPercent => encode_url_percent(text),
            EscapeKind::HtmlEntity => encode_html_entity(text),
            EscapeKind::HtmlNumericHex => encode_numeric_entities(text, true),
            EscapeKind::HtmlNumericDec => encode_numeric_entities(text, false),
            EscapeKind::Json => encode_json(text),
            EscapeKind::UnicodeEscape => encode_unicode_escape(text),
            EscapeKind::CString => encode_c_string(text),
            EscapeKind::QuotedPrintable => encode_quoted_printable(text),
        }
    }

    /// 转义文本 → 原文。
    pub fn decode(self, text: &str) -> Result<String, String> {
        match self {
            EscapeKind::UrlPercent => decode_url_percent(text),
            EscapeKind::HtmlEntity => decode_entities(text, NamedPolicy::Decode, "HTML 实体"),
            EscapeKind::HtmlNumericHex => {
                decode_entities(text, NamedPolicy::Keep, "HTML 数字实体（十六进制）")
            }
            EscapeKind::HtmlNumericDec => {
                decode_entities(text, NamedPolicy::Keep, "HTML 数字实体（十进制）")
            }
            EscapeKind::Json => decode_json(text),
            EscapeKind::UnicodeEscape => decode_unicode_like(text, "Unicode 转义"),
            EscapeKind::CString => decode_unicode_like(text, "C 字符串"),
            EscapeKind::QuotedPrintable => decode_quoted_printable(text),
        }
    }
}

// ---------------------------------------------------------------------------
// 通用小工具
// ---------------------------------------------------------------------------

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// 从字符数组的 `index` 处连续读取 `count` 位十六进制，读完自动前移下标。
fn read_hex_digits(
    chars: &[char],
    index: &mut usize,
    count: usize,
    label: &str,
    prefix: &str,
) -> Result<u32, String> {
    let mut value: u32 = 0;
    for _ in 0..count {
        let ch = match chars.get(*index) {
            Some(&ch) => ch,
            None => {
                return Err(format!(
                    "{label}解码失败：{prefix} 后面不足 {count} 位十六进制"
                ))
            }
        };
        *index += 1;
        let digit = ch.to_digit(16).ok_or_else(|| {
            format!("{label}解码失败：{prefix} 后面的字符 '{ch}' 不是十六进制数字")
        })?;
        value = (value << 4) | digit;
    }
    Ok(value)
}

// ---------------------------------------------------------------------------
// URL 百分号编码
// ---------------------------------------------------------------------------

fn is_url_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~')
}

fn encode_url_percent(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    for &byte in text.as_bytes() {
        if is_url_unreserved(byte) {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push(HEX_UPPER[(byte >> 4) as usize] as char);
            out.push(HEX_UPPER[(byte & 0x0F) as usize] as char);
        }
    }
    out
}

fn decode_url_percent(text: &str) -> Result<String, String> {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            out.push(bytes[index]);
            index += 1;
            continue;
        }
        if index + 2 >= bytes.len() {
            return Err(format!(
                "URL 百分号解码失败：第 {} 个字符 '%' 后面不足两位十六进制",
                index + 1
            ));
        }
        let high = hex_value(bytes[index + 1]).ok_or_else(|| {
            format!(
                "URL 百分号解码失败：第 {} 个字符 '%' 后面的 '{}' 不是十六进制数字",
                index + 1,
                bytes[index + 1] as char
            )
        })?;
        let low = hex_value(bytes[index + 2]).ok_or_else(|| {
            format!(
                "URL 百分号解码失败：第 {} 个字符 '%' 后面的 '{}' 不是十六进制数字",
                index + 2,
                bytes[index + 2] as char
            )
        })?;
        out.push((high << 4) | low);
        index += 3;
    }
    String::from_utf8(out).map_err(|err| {
        format!(
            "URL 百分号解码失败：还原出的字节不是合法 UTF-8（{}）",
            err.utf8_error()
        )
    })
}

// ---------------------------------------------------------------------------
// HTML 实体
// ---------------------------------------------------------------------------

fn encode_html_entity(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

/// 数字实体（十六进制 / 十进制）编码。
///
/// 转义范围：非 ASCII 一律转义（避免页面编码问题），`& < > " '` 转义（避免
/// 与标签语法冲突），ASCII 控制字符转义（它们本身不可见，原样留下没有意义）。
fn encode_numeric_entities(text: &str, hex: bool) -> String {
    let mut out = String::with_capacity(text.len() * 4);
    for ch in text.chars() {
        let code = ch as u32;
        let needs_escape = !ch.is_ascii()
            || code < 0x20
            || code == 0x7F
            || matches!(ch, '&' | '<' | '>' | '"' | '\'');
        if !needs_escape {
            out.push(ch);
        } else if hex {
            out.push_str(&format!("&#x{code:X};"));
        } else {
            out.push_str(&format!("&#{code};"));
        }
    }
    out
}

/// 解码时的命名实体策略：`Decode` 还原已知命名实体，`Keep` 一律原样保留
/// （HTML 数字实体编码只负责数字引用，不能把 `&amp;` 这种文本悄悄改掉）。
#[derive(Clone, Copy, PartialEq, Eq)]
enum NamedPolicy {
    Decode,
    Keep,
}

/// 常见命名实体表（覆盖需求里的十来个，外加排版常用的几个）。
fn named_entity(name: &str) -> Option<char> {
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{a0}',
        "ensp" => '\u{2002}',
        "emsp" => '\u{2003}',
        "thinsp" => '\u{2009}',
        "shy" => '\u{ad}',
        "copy" => '©',
        "reg" => '®',
        "trade" => '™',
        "cent" => '¢',
        "pound" => '£',
        "yen" => '¥',
        "euro" => '€',
        "sect" => '§',
        "para" => '¶',
        "deg" => '°',
        "plusmn" => '±',
        "frac12" => '½',
        "times" => '×',
        "divide" => '÷',
        "middot" => '·',
        "bull" => '•',
        "dagger" => '†',
        "hellip" => '…',
        "mdash" => '—',
        "ndash" => '–',
        "lsquo" => '‘',
        "rsquo" => '’',
        "ldquo" => '“',
        "rdquo" => '”',
        "laquo" => '«',
        "raquo" => '»',
        "larr" => '←',
        "rarr" => '→',
        "uarr" => '↑',
        "darr" => '↓',
        "harr" => '↔',
        "hearts" => '♥',
        _ => return None,
    })
}

/// 找到与 `&` 配对的 `;`。实体名限定 ASCII 字母数字与 `#`，最长 32 字符，
/// 超过这个范围就认为是不需要处理的普通 `&`，避免把整段文本当实体吞掉。
fn entity_end(chars: &[char], amp_index: usize) -> Option<usize> {
    let limit = (amp_index + 34).min(chars.len());
    for (index, &ch) in chars.iter().enumerate().take(limit).skip(amp_index + 1) {
        match ch {
            ';' => return if index > amp_index + 1 { Some(index) } else { None },
            ch if ch.is_ascii_alphanumeric() || ch == '#' => {}
            _ => return None,
        }
    }
    None
}

/// 解析 `&` 与 `;` 之间的数字引用体（`#x1F600` / `#123`）。
fn parse_numeric_entity(body: &str, label: &str) -> Result<u32, String> {
    let digits = &body[1..]; // 去掉 '#'
    let (radix, digits) = match digits.strip_prefix('x').or_else(|| digits.strip_prefix('X')) {
        Some(rest) => (16, rest),
        None => (10, digits),
    };
    if digits.is_empty() || digits.len() > 8 {
        return Err(format!(
            "{label}解码失败：数字引用 '&{body};' 的十六进制位数不合法"
        ));
    }
    u32::from_str_radix(digits, radix).map_err(|_| {
        format!(
            "{label}解码失败：数字引用 '&{body};' 不是合法的 {radix} 进制数字"
        )
    })
}

fn decode_entities(text: &str, named: NamedPolicy, label: &str) -> Result<String, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < chars.len() {
        if chars[index] != '&' {
            out.push(chars[index]);
            index += 1;
            continue;
        }
        let end = match entity_end(&chars, index) {
            Some(end) => end,
            None => {
                // 找不到配对的 ';'，说明只是普通 &，原样输出。
                out.push('&');
                index += 1;
                continue;
            }
        };
        let body: String = chars[index + 1..end].iter().collect();
        if body.starts_with('#') {
            let code = parse_numeric_entity(&body, label)?;
            let decoded = char::from_u32(code).ok_or_else(|| {
                format!("{label}解码失败：码点 U+{code:X} 超出 Unicode 范围或落在代理区")
            })?;
            out.push(decoded);
            index = end + 1;
            continue;
        }
        if named == NamedPolicy::Decode {
            if let Some(decoded) = named_entity(&body) {
                out.push(decoded);
                index = end + 1;
                continue;
            }
        }
        // 未知实体（或非 Decode 策略下的命名实体）：原样保留 `&...;`。
        out.extend(chars[index..=end].iter());
        index = end + 1;
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// JSON 字符串转义
// ---------------------------------------------------------------------------

fn encode_json(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 4);
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // JSON 规定 U+0000..U+001F 必须转义；其余字符（含中文、Emoji）保持原样。
            ch if (ch as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", ch as u32)),
            other => out.push(other),
        }
    }
    out
}

/// 剥掉整串外面那一对未转义的 `"`（用户常常直接粘贴一整个 JSON 字符串字面量）。
fn strip_wrapping_quotes(text: &str) -> &str {
    let bytes = text.as_bytes();
    if bytes.len() < 2 || bytes[0] != b'"' || bytes[bytes.len() - 1] != b'"' {
        return text;
    }
    // 数一下结尾引号前面有几个反斜杠：奇数个说明它被转义了，不是收尾引号。
    let mut backslashes = 0usize;
    let mut index = bytes.len() - 1;
    while index > 0 && bytes[index - 1] == b'\\' {
        backslashes += 1;
        index -= 1;
    }
    if backslashes.is_multiple_of(2) {
        &text[1..text.len() - 1]
    } else {
        text
    }
}

fn decode_json(text: &str) -> Result<String, String> {
    const LABEL: &str = "JSON 转义";
    let body = strip_wrapping_quotes(text);
    let chars: Vec<char> = body.chars().collect();
    let mut out = String::with_capacity(body.len());
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        index += 1;
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        let escape = match chars.get(index) {
            Some(&escape) => escape,
            None => return Err(format!("{LABEL}解码失败：文本以孤立的 '\\' 结尾")),
        };
        index += 1;
        match escape {
            '"' => out.push('"'),
            '\\' => out.push('\\'),
            '/' => out.push('/'),
            'b' => out.push('\u{8}'),
            'f' => out.push('\u{c}'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            'u' => {
                let high = read_hex_digits(&chars, &mut index, 4, LABEL, "\\u")?;
                let code = if (0xD800..=0xDBFF).contains(&high) {
                    // 增补平面字符在 JSON 里必须写成一对代理。
                    if chars.get(index) != Some(&'\\') || chars.get(index + 1) != Some(&'u') {
                        return Err(format!(
                            "{LABEL}解码失败：高位代理 \\u{high:04X} 之后缺少低位代理"
                        ));
                    }
                    index += 2;
                    let low = read_hex_digits(&chars, &mut index, 4, LABEL, "\\u")?;
                    if !(0xDC00..=0xDFFF).contains(&low) {
                        return Err(format!(
                            "{LABEL}解码失败：代理对低位 \\u{low:04X} 不在 DC00-DFFF 区间"
                        ));
                    }
                    0x1_0000 + ((high - 0xD800) << 10) + (low - 0xDC00)
                } else if (0xDC00..=0xDFFF).contains(&high) {
                    return Err(format!("{LABEL}解码失败：孤立的低位代理 \\u{high:04X}"));
                } else {
                    high
                };
                let decoded = char::from_u32(code).ok_or_else(|| {
                    format!("{LABEL}解码失败：码点 U+{code:X} 超出 Unicode 范围")
                })?;
                out.push(decoded);
            }
            other => {
                return Err(format!(
                    "{LABEL}解码失败：未知转义序列 '\\{other}'"
                ))
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Unicode 转义 / C 字符串
// ---------------------------------------------------------------------------

/// 把一个非 ASCII 字符写成 `\uXXXX`；增补平面拆成代理对。
fn push_utf16_escape(out: &mut String, ch: char) {
    let code = ch as u32;
    if code <= 0xFFFF {
        out.push_str(&format!("\\u{code:04X}"));
    } else {
        let offset = code - 0x1_0000;
        let high = 0xD800 + (offset >> 10);
        let low = 0xDC00 + (offset & 0x3FF);
        out.push_str(&format!("\\u{high:04X}\\u{low:04X}"));
    }
}

fn encode_unicode_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    for ch in text.chars() {
        let code = ch as u32;
        match ch {
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ if code < 0x20 || code == 0x7F => {
                out.push_str(&format!("\\u{code:04X}"));
            }
            ch if ch.is_ascii() => out.push(ch),
            ch => push_utf16_escape(&mut out, ch),
        }
    }
    out
}

fn encode_c_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    for ch in text.chars() {
        let code = ch as u32;
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // 控制字符与 DEL 没有可打印形式，用 \xNN 表示。
            ch if ch.is_ascii() && !ch.is_ascii_graphic() && ch != ' ' => {
                out.push_str(&format!("\\x{code:02X}"));
            }
            ch if ch.is_ascii() => out.push(ch),
            ch => push_utf16_escape(&mut out, ch),
        }
    }
    out
}

/// Unicode 转义与 C 字符串共用的解码器：`\x` 按字节累加，`\u` 按码点累加，
/// 最后统一做一次 UTF-8 校验，这样两种写法可以混用（例如 `\xE4\u00B8\xAD`）。
fn decode_unicode_like(text: &str, label: &str) -> Result<String, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut bytes: Vec<u8> = Vec::with_capacity(text.len());
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        index += 1;
        if ch != '\\' {
            let mut buffer = [0u8; 4];
            bytes.extend_from_slice(ch.encode_utf8(&mut buffer).as_bytes());
            continue;
        }
        let escape = match chars.get(index) {
            Some(&escape) => escape,
            None => return Err(format!("{label}解码失败：文本以孤立的 '\\' 结尾")),
        };
        index += 1;
        match escape {
            'n' => bytes.push(b'\n'),
            'r' => bytes.push(b'\r'),
            't' => bytes.push(b'\t'),
            'b' => bytes.push(0x08),
            'f' => bytes.push(0x0C),
            '0' => bytes.push(0),
            '\\' => bytes.push(b'\\'),
            '"' => bytes.push(b'"'),
            '\'' => bytes.push(b'\''),
            '/' => bytes.push(b'/'),
            'x' => {
                let value = read_hex_digits(&chars, &mut index, 2, label, "\\x")?;
                bytes.push(value as u8);
            }
            'u' => {
                let code = if chars.get(index) == Some(&'{') {
                    index += 1;
                    let start = index;
                    while index < chars.len() && chars[index] != '}' {
                        index += 1;
                    }
                    if index >= chars.len() {
                        return Err(format!("{label}解码失败：\\u{{ 缺少结尾的 '}}'"));
                    }
                    let digits: String = chars[start..index].iter().collect();
                    index += 1; // 跳过 '}'
                    if digits.is_empty() || digits.len() > 6 {
                        return Err(format!(
                            "{label}解码失败：\\u{{{digits}}} 的十六进制位数应为 1..=6"
                        ));
                    }
                    u32::from_str_radix(&digits, 16).map_err(|_| {
                        format!("{label}解码失败：\\u{{{digits}}} 不是合法的十六进制")
                    })?
                } else {
                    let high = read_hex_digits(&chars, &mut index, 4, label, "\\u")?;
                    if (0xD800..=0xDBFF).contains(&high) {
                        if chars.get(index) != Some(&'\\') || chars.get(index + 1) != Some(&'u') {
                            return Err(format!(
                                "{label}解码失败：高位代理 \\u{high:04X} 之后缺少低位代理"
                            ));
                        }
                        index += 2;
                        let low = read_hex_digits(&chars, &mut index, 4, label, "\\u")?;
                        if !(0xDC00..=0xDFFF).contains(&low) {
                            return Err(format!(
                                "{label}解码失败：代理对低位 \\u{low:04X} 不在 DC00-DFFF 区间"
                            ));
                        }
                        0x1_0000 + ((high - 0xD800) << 10) + (low - 0xDC00)
                    } else if (0xDC00..=0xDFFF).contains(&high) {
                        return Err(format!("{label}解码失败：孤立的低位代理 \\u{high:04X}"));
                    } else {
                        high
                    }
                };
                let decoded = char::from_u32(code).ok_or_else(|| {
                    format!("{label}解码失败：码点 U+{code:X} 超出 Unicode 范围")
                })?;
                let mut buffer = [0u8; 4];
                bytes.extend_from_slice(decoded.encode_utf8(&mut buffer).as_bytes());
            }
            other => {
                return Err(format!(
                    "{label}解码失败：未知转义序列 '\\{other}'"
                ))
            }
        }
    }
    String::from_utf8(bytes).map_err(|err| {
        format!(
            "{label}解码失败：还原出的字节不是合法 UTF-8（{}）",
            err.utf8_error()
        )
    })
}

// ---------------------------------------------------------------------------
// Quoted-Printable（RFC 2045）
// ---------------------------------------------------------------------------

/// 把一个字节变成 QP 记号：可见 ASCII 原样（`=` 除外），其余写成 `=XX`。
/// `space_like` 标记该记号是空格——它如果落在行尾就必须显式编码成 `=20`。
fn qp_token(byte: u8) -> (String, bool) {
    match byte {
        b'=' => ("=3D".to_string(), false),
        0x21..=0x7E => ((byte as char).to_string(), false),
        b' ' => (" ".to_string(), true),
        _ => (format!("={byte:02X}"), false),
    }
}

fn encode_quoted_printable(text: &str) -> String {
    let tokens: Vec<(String, bool)> = text.as_bytes().iter().map(|&byte| qp_token(byte)).collect();
    let mut out = String::with_capacity(tokens.len() * 2);
    let mut line_len = 0usize;
    for (index, (token, space_like)) in tokens.iter().enumerate() {
        // QP 的 76 列上限里要预留行尾的软换行 '='，所以正文最多 75 列。
        let ends_line = match tokens.get(index + 1) {
            None => true,
            Some((next, _)) => line_len + token.len() + next.len() > 75,
        };
        let piece = if *space_like && ends_line {
            "=20".to_string()
        } else {
            token.clone()
        };
        if line_len > 0 && line_len + piece.len() > 75 {
            out.push_str("=\r\n");
            line_len = 0;
        }
        out.push_str(&piece);
        line_len += piece.len();
    }
    out
}

fn decode_quoted_printable(text: &str) -> Result<String, String> {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'=' {
            out.push(bytes[index]);
            index += 1;
            continue;
        }
        // 软换行：=CRLF（RFC）或 =\n（很多工具的实际输出）。
        match bytes.get(index + 1) {
            Some(b'\n') => {
                index += 2;
                continue;
            }
            Some(b'\r') => {
                if bytes.get(index + 2) == Some(&b'\n') {
                    index += 3;
                    continue;
                }
                return Err(format!(
                    "Quoted-Printable 解码失败：第 {} 个字符 '=' 后面跟了孤立的 CR",
                    index + 1
                ));
            }
            Some(_) => {}
            None => {
                return Err("Quoted-Printable 解码失败：文本以孤立的 '=' 结尾（缺少软换行或十六进制）".to_string())
            }
        }
        if index + 2 >= bytes.len() {
            return Err(format!(
                "Quoted-Printable 解码失败：第 {} 个字符 '=' 后面不足两位十六进制",
                index + 1
            ));
        }
        let high = hex_value(bytes[index + 1]).ok_or_else(|| {
            format!(
                "Quoted-Printable 解码失败：第 {} 个字符 '=' 后面的 '{}' 不是十六进制数字",
                index + 1,
                bytes[index + 1] as char
            )
        })?;
        let low = hex_value(bytes[index + 2]).ok_or_else(|| {
            format!(
                "Quoted-Printable 解码失败：第 {} 个字符 '=' 后面的 '{}' 不是十六进制数字",
                index + 2,
                bytes[index + 2] as char
            )
        })?;
        out.push((high << 4) | low);
        index += 3;
    }
    String::from_utf8(out).map_err(|err| {
        format!(
            "Quoted-Printable 解码失败：还原出的字节不是合法 UTF-8（{}）",
            err.utf8_error()
        )
    })
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 覆盖各转义方式的“难缠”输入：空、引号、控制字符、中文、Emoji、百分号、
    /// 行尾空格、长行。
    fn tricky_samples() -> Vec<(&'static str, String)> {
        vec![
            ("空", String::new()),
            ("纯 ASCII", "hello world".to_string()),
            ("引号与反斜杠", "a\"b'c\\d".to_string()),
            ("HTML 特殊字符", "&<>\"'".to_string()),
            ("URL 特殊字符", "a b&c=中%?#/+".to_string()),
            ("中文", "中文测试".to_string()),
            ("Emoji", "😀🎉".to_string()),
            ("控制字符", "\u{0}\u{1}\u{7F}\u{8}\u{C}".to_string()),
            ("换行与制表", "line1\nline2\r\ttab".to_string()),
            ("行尾空格", "abc ".to_string()),
            ("行尾制表", "abc\t".to_string()),
            (
                "长行",
                "The quick brown fox jumps over the lazy dog. ".repeat(6),
            ),
            ("混合", "aA0 !\"#$%&'()*+,-./ 中文😀\n\t".to_string()),
        ]
    }

    #[test]
    fn metadata_is_complete_and_unique() {
        assert_eq!(EscapeKind::ALL.len(), 8);
        let mut ids = std::collections::HashSet::new();
        let mut names = std::collections::HashSet::new();
        for kind in EscapeKind::ALL {
            assert!(!kind.id().is_empty());
            assert!(!kind.name().is_empty());
            assert!(!kind.note().is_empty());
            assert!(ids.insert(kind.id()), "id 重复：{}", kind.id());
            assert!(names.insert(kind.name()), "name 重复：{}", kind.name());
        }
    }

    #[test]
    fn round_trip_every_kind_and_sample() {
        for kind in EscapeKind::ALL {
            for (name, sample) in tricky_samples() {
                let encoded = kind.encode(&sample);
                let decoded = kind
                    .decode(&encoded)
                    .unwrap_or_else(|err| panic!("{} 编码「{name}」后无法解码：{err}", kind.id()));
                assert_eq!(
                    decoded, sample,
                    "{} 往返失败（{name}）：编码结果 {encoded:?}",
                    kind.id()
                );
            }
        }
    }

    #[test]
    fn empty_input_round_trips_for_every_kind() {
        for kind in EscapeKind::ALL {
            assert_eq!(kind.encode(""), "", "{} 空输入应编码为空串", kind.id());
            assert_eq!(
                kind.decode("").as_deref(),
                Ok(""),
                "{} 空文本应解出空串",
                kind.id()
            );
        }
    }

    #[test]
    fn url_percent_matches_reference() {
        assert_eq!(
            EscapeKind::UrlPercent.encode("a b&c=中"),
            "a%20b%26c%3D%E4%B8%AD"
        );
        assert_eq!(EscapeKind::UrlPercent.encode("~-_.!"), "~-_.%21");
        assert_eq!(EscapeKind::UrlPercent.encode("😀"), "%F0%9F%98%80");
        assert_eq!(
            EscapeKind::UrlPercent.decode("hello%20world"),
            Ok("hello world".to_string())
        );
        // 小写十六进制也要能解。
        assert_eq!(
            EscapeKind::UrlPercent.decode("%e4%b8%ad"),
            Ok("中".to_string())
        );
        // '+' 不是空格。
        assert_eq!(
            EscapeKind::UrlPercent.decode("a+b"),
            Ok("a+b".to_string())
        );
        // 未转义的非 ASCII 字符原样保留。
        assert_eq!(
            EscapeKind::UrlPercent.decode("中%20文"),
            Ok("中 文".to_string())
        );
    }

    #[test]
    fn html_entity_encode_and_decode() {
        assert_eq!(
            EscapeKind::HtmlEntity.encode("&<>\"'"),
            "&amp;&lt;&gt;&quot;&#39;"
        );
        assert_eq!(
            EscapeKind::HtmlEntity.encode("中文😀"),
            "中文😀",
            "命名实体模式只处理 HTML 特殊字符"
        );
        assert_eq!(
            EscapeKind::HtmlEntity.decode("&amp;&lt;&gt;&quot;&#39;&nbsp;"),
            Ok("&<>\"'\u{a0}".to_string())
        );
        assert_eq!(
            EscapeKind::HtmlEntity.decode("&#x4E2D;&#25991;"),
            Ok("中文".to_string())
        );
        // 未知实体原样保留；孤立的 & 也原样保留。
        assert_eq!(
            EscapeKind::HtmlEntity.decode("&unknown; &amp; &"),
            Ok("&unknown; & &".to_string())
        );
        assert_eq!(
            EscapeKind::HtmlEntity.decode("&#xZZ;"),
            Err("HTML 实体解码失败：数字引用 '&#xZZ;' 不是合法的 16 进制数字".to_string())
        );
    }

    #[test]
    fn numeric_entities_encode_and_decode() {
        assert_eq!(EscapeKind::HtmlNumericHex.encode("中"), "&#x4E2D;");
        assert_eq!(EscapeKind::HtmlNumericDec.encode("中"), "&#20013;");
        assert_eq!(EscapeKind::HtmlNumericHex.encode("😀"), "&#x1F600;");
        assert_eq!(EscapeKind::HtmlNumericDec.encode("😀"), "&#128512;");
        assert_eq!(EscapeKind::HtmlNumericHex.encode("<"), "&#x3C;");
        assert_eq!(EscapeKind::HtmlNumericDec.encode("<"), "&#60;");
        assert_eq!(EscapeKind::HtmlNumericHex.encode("a"), "a");
        assert_eq!(EscapeKind::HtmlNumericHex.encode("\u{0}"), "&#x0;");
        assert_eq!(EscapeKind::HtmlNumericDec.encode("\u{7F}"), "&#127;");
        // 两种写法都能解；命名实体在这种模式下不动。
        assert_eq!(
            EscapeKind::HtmlNumericHex.decode("&#x4E2D;&#20013;&amp;"),
            Ok("中中&amp;".to_string())
        );
        assert!(EscapeKind::HtmlNumericDec.decode("&#x110000;").is_err());
    }

    #[test]
    fn json_matches_reference_rules() {
        assert_eq!(EscapeKind::Json.encode("a\"b\\c"), "a\\\"b\\\\c");
        assert_eq!(EscapeKind::Json.encode("line\n\ttab"), "line\\n\\ttab");
        assert_eq!(EscapeKind::Json.encode("\u{0}\u{1F}"), "\\u0000\\u001f");
        assert_eq!(
            EscapeKind::Json.encode("中😀/"),
            "中😀/",
            "非 ASCII 默认不转义，'/' 也不需要转义"
        );
        assert_eq!(EscapeKind::Json.encode("\u{7F}"), "\u{7F}");
        assert_eq!(
            EscapeKind::Json.decode("\\u4E2D\\uD83D\\uDE00"),
            Ok("中😀".to_string())
        );
        assert_eq!(
            EscapeKind::Json.decode("\\/\\b\\f\\n\\r\\t"),
            Ok("/\u{8}\u{c}\n\r\t".to_string())
        );
        // 直接粘贴带引号的 JSON 字面量。
        assert_eq!(
            EscapeKind::Json.decode("\"hello\\nworld\""),
            Ok("hello\nworld".to_string())
        );
        // 结尾引号被转义时不能误剥。
        assert_eq!(
            EscapeKind::Json.decode("\\\""),
            Ok("\"".to_string())
        );
        assert!(EscapeKind::Json.decode("\\q").is_err());
        assert!(EscapeKind::Json.decode("\\u12").is_err());
        assert!(EscapeKind::Json.decode("\\uD800").is_err());
        assert!(EscapeKind::Json.decode("abc\\").is_err());
    }

    #[test]
    fn unicode_escape_handles_bmp_and_supplementary_plane() {
        assert_eq!(EscapeKind::UnicodeEscape.encode("中"), "\\u4E2D");
        assert_eq!(EscapeKind::UnicodeEscape.encode("😀"), "\\uD83D\\uDE00");
        assert_eq!(EscapeKind::UnicodeEscape.encode("a"), "a");
        assert_eq!(EscapeKind::UnicodeEscape.encode("\n"), "\\n");
        assert_eq!(EscapeKind::UnicodeEscape.encode("\u{1}"), "\\u0001");
        assert_eq!(
            EscapeKind::UnicodeEscape.decode("\\u4E2D"),
            Ok("中".to_string())
        );
        assert_eq!(
            EscapeKind::UnicodeEscape.decode("\\uD83D\\uDE00"),
            Ok("😀".to_string())
        );
        assert_eq!(
            EscapeKind::UnicodeEscape.decode("\\u{1F600}"),
            Ok("😀".to_string())
        );
        assert_eq!(
            EscapeKind::UnicodeEscape.decode("\\xE4\\xB8\\xAD"),
            Ok("中".to_string())
        );
        assert_eq!(
            EscapeKind::UnicodeEscape.decode("\\u4E2D\\xE6\\x96\\x87"),
            Ok("中文".to_string())
        );
        assert!(EscapeKind::UnicodeEscape.decode("\\uD800").is_err());
        assert!(EscapeKind::UnicodeEscape.decode("\\x4").is_err());
        assert!(EscapeKind::UnicodeEscape.decode("\\xZZ").is_err());
        assert!(EscapeKind::UnicodeEscape.decode("\\u{110000}").is_err());
    }

    #[test]
    fn c_string_escapes_controls_and_non_ascii() {
        assert_eq!(EscapeKind::CString.encode("\n\t\r\\\""), "\\n\\t\\r\\\\\\\"");
        assert_eq!(EscapeKind::CString.encode("\u{7}"), "\\x07");
        assert_eq!(EscapeKind::CString.encode("\u{7F}"), "\\x7F");
        assert_eq!(EscapeKind::CString.encode("中"), "\\u4E2D");
        assert_eq!(EscapeKind::CString.encode("😀"), "\\uD83D\\uDE00");
        assert_eq!(EscapeKind::CString.encode("a b"), "a b");
        assert_eq!(
            EscapeKind::CString.decode("\\xE4\\xB8\\xAD"),
            Ok("中".to_string())
        );
        assert_eq!(
            EscapeKind::CString.decode("\\u4E2D"),
            Ok("中".to_string())
        );
        assert!(EscapeKind::CString.decode("\\x80").is_err(), "孤立字节不是合法 UTF-8");
    }

    #[test]
    fn quoted_printable_matches_rfc() {
        assert_eq!(EscapeKind::QuotedPrintable.encode("café"), "caf=C3=A9");
        assert_eq!(EscapeKind::QuotedPrintable.encode("a=b"), "a=3Db");
        assert_eq!(EscapeKind::QuotedPrintable.encode("abc "), "abc=20");
        assert_eq!(EscapeKind::QuotedPrintable.encode("abc\t"), "abc=09");
        assert_eq!(
            EscapeKind::QuotedPrintable.encode("中文"),
            "=E4=B8=AD=E6=96=87"
        );
        assert_eq!(
            EscapeKind::QuotedPrintable.encode("\u{0}\u{1}\u{7F}"),
            "=00=01=7F"
        );
        assert_eq!(
            EscapeKind::QuotedPrintable.encode("a  b   c"),
            "a  b   c",
            "行内空格保持可读"
        );
        // 76 列软换行：首行 = 75 个正文 + '='。
        let long = "a".repeat(200);
        let encoded = EscapeKind::QuotedPrintable.encode(&long);
        assert_eq!(
            encoded.split("\r\n").next().map(str::len),
            Some(76),
            "首行应为 75 个字符加行尾 '='"
        );
        assert!(encoded.contains("=\r\n"));
        assert_eq!(EscapeKind::QuotedPrintable.decode(&encoded), Ok(long));
        // 软换行与等号转义。
        assert_eq!(
            EscapeKind::QuotedPrintable.decode("Soft=\r\nBreak"),
            Ok("SoftBreak".to_string())
        );
        assert_eq!(
            EscapeKind::QuotedPrintable.decode("Soft=\nBreak"),
            Ok("SoftBreak".to_string())
        );
        assert_eq!(
            EscapeKind::QuotedPrintable.decode("=E4=B8=AD"),
            Ok("中".to_string())
        );
        // 行内任意位置都不会出现行尾空白（否则会被传输层吞掉）。
        let sample = "ab ".repeat(80);
        let encoded = EscapeKind::QuotedPrintable.encode(&sample);
        for line in encoded.split("\r\n") {
            let content = line.strip_suffix('=').unwrap_or(line);
            assert!(
                !content.ends_with(' ') && !content.ends_with('\t'),
                "行尾不应有未转义的空白：{content:?}"
            );
            assert!(line.chars().count() <= 76, "行宽超限：{}", line.chars().count());
        }
        assert_eq!(EscapeKind::QuotedPrintable.decode(&encoded), Ok(sample));
        // 错误输入。
        assert!(EscapeKind::QuotedPrintable.decode("=G1").is_err());
        assert!(EscapeKind::QuotedPrintable.decode("abc=").is_err());
        assert!(EscapeKind::QuotedPrintable.decode("=\r").is_err());
        assert!(EscapeKind::QuotedPrintable.decode("=FF").is_err(), "非法 UTF-8 应报错");
    }

    #[test]
    fn decoding_never_panics_on_arbitrary_text() {
        let inputs = [
            "",
            "&",
            "&;",
            "&#",
            "&#;",
            "&#x;",
            "\\",
            "\\u",
            "\\u{",
            "\\x",
            "%",
            "%2",
            "=",
            "==",
            "\u{FFFD}",
            "😀",
            "&#999999999;",
        ];
        for kind in EscapeKind::ALL {
            for input in inputs {
                let _ = kind.decode(input);
            }
        }
    }
}
