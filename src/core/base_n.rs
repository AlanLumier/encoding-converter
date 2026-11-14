//! Base-N 编码（字节流 ↔ 可打印文本）。
//!
//! 十几种 Base 家族被收敛到同一个 `BaseN` 枚举上：`encode` 只负责把任意字节
//! 变成文本（因此不能失败、也绝不能panic），`decode` 负责把文本还原成字节，
//! 并对非法字符、长度、填充规则做严格校验，错误信息用中文指明是哪一种编码、
//! 哪一步出了什么问题。
//!
//! 实现按“进位方式”分成四类，便于复用：
//! 1. 按位分组：Base16 / Base32 系。每 4 或 5 位取一个字符，末尾不足时按
//!    RFC 4648 补 `=`（z-base-32 / Crockford 不使用填充）。
//! 2. 大整数换算：Base58 / Base62。必须单独统计前导零字节，否则 `0x00`
//!    前缀会在“整数化”时被吞掉（`0x0001` 与 `0x01` 会编码成同一个数）。
//! 3. 32 位分组：ASCII85 / Z85 / RFC1924。4 字节当成大端 u32，再按 85 进制
//!    展开成 5 个字符；末组不足 4 字节时补零，并只输出“够用”的字符数。
//! 4. 行式编码：uuencode / xxencode。行首字符同时编码了该行的有效字节数，
//!    解码时必须据此截断掉补位产生的多余字节。

/// 支持的 Base-N 编码种类。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BaseN {
    Base16,
    Base32,
    Base32Hex,
    ZBase32,
    Crockford32,
    Base58Bitcoin,
    Base58Flickr,
    Base58Ripple,
    Base62,
    Base64,
    Base64Url,
    Base64Mime,
    Ascii85,
    Z85,
    Rfc1924,
    Base91,
    Uuencode,
    Xxencode,
}

// ---------------------------------------------------------------------------
// 字母表
// ---------------------------------------------------------------------------

const BASE16_ALPHABET: &[u8] = b"0123456789abcdef";
const BASE32_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
const BASE32HEX_ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUV";
const ZBASE32_ALPHABET: &[u8] = b"ybndrfg8ejkmcpqxot1uwisza345h769";
/// Crockford 去掉了容易看错的 I、L、O、U。
const CROCKFORD_ALPHABET: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const BASE58_BITCOIN_ALPHABET: &[u8] =
    b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
const BASE58_FLICKR_ALPHABET: &[u8] =
    b"123456789abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ";
/// Ripple 的 58 字母表，与 Bitcoin 完全不同（首字符是 `r`，因此 `0x00` 编成 `r`）。
const BASE58_RIPPLE_ALPHABET: &[u8] =
    b"rpshnaf39wBUDNEGHJKLM4PQRST7VWXYZ2bcdeCg65jkm8oFqi1tuvAxyz";
const BASE62_ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const BASE64_STANDARD_ALPHABET: &[u8] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const BASE64_URL_ALPHABET: &[u8] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
/// ZeroMQ Z85 字母表：比 Base64 多了 `. - : + = ^ ! / * ? & < > ( ) [ ] { } @ % $ #`。
const Z85_ALPHABET: &[u8] =
    b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ.-:+=^!/*?&<>()[]{}@%$#";
/// RFC 1924（IPv6 地址建议的那套）字母表。
const RFC1924_ALPHABET: &[u8] =
    b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz!#$%&()*+-;<=>?@^_`{|}~";
/// basE91 官方字母表（末尾是双引号）。
const BASE91_ALPHABET: &[u8] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!#$%&()*+,./:;<=>?@[]^_`{|}~\"";
/// xxencode 字母表：按 ASCII 顺序排列（`+` `-` 在 0-9A-Za-z 之前）。
/// 它最大的特点是“整行都落在字母表内”，连行首的长度字符也取自这张表。
const XXENCODE_ALPHABET: &[u8] =
    b"+-0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// Adobe ASCII85 的 85 个字符就是 ASCII 33..=117（`!`..=`u`），用 const fn 生成，
/// 避免手写 85 个转义字符时漏掉 `"` 或反斜杠。
const fn ascii85_alphabet() -> [u8; 85] {
    let mut table = [0u8; 85];
    let mut i = 0;
    while i < 85 {
        table[i] = 33 + i as u8;
        i += 1;
    }
    table
}
const ASCII85_ALPHABET: [u8; 85] = ascii85_alphabet();

// ---------------------------------------------------------------------------
// 公共入口
// ---------------------------------------------------------------------------

impl BaseN {
    pub const ALL: [BaseN; 18] = [
        BaseN::Base16,
        BaseN::Base32,
        BaseN::Base32Hex,
        BaseN::ZBase32,
        BaseN::Crockford32,
        BaseN::Base58Bitcoin,
        BaseN::Base58Flickr,
        BaseN::Base58Ripple,
        BaseN::Base62,
        BaseN::Base64,
        BaseN::Base64Url,
        BaseN::Base64Mime,
        BaseN::Ascii85,
        BaseN::Z85,
        BaseN::Rfc1924,
        BaseN::Base91,
        BaseN::Uuencode,
        BaseN::Xxencode,
    ];

    /// 稳定的字符串标识，用于配置持久化（UI 里切换编码后仍能找回）。
    pub fn id(self) -> &'static str {
        match self {
            BaseN::Base16 => "base16",
            BaseN::Base32 => "base32",
            BaseN::Base32Hex => "base32hex",
            BaseN::ZBase32 => "zbase32",
            BaseN::Crockford32 => "crockford32",
            BaseN::Base58Bitcoin => "base58btc",
            BaseN::Base58Flickr => "base58flickr",
            BaseN::Base58Ripple => "base58ripple",
            BaseN::Base62 => "base62",
            BaseN::Base64 => "base64",
            BaseN::Base64Url => "base64url",
            BaseN::Base64Mime => "base64mime",
            BaseN::Ascii85 => "ascii85",
            BaseN::Z85 => "z85",
            BaseN::Rfc1924 => "rfc1924",
            BaseN::Base91 => "base91",
            BaseN::Uuencode => "uuencode",
            BaseN::Xxencode => "xxencode",
        }
    }

    /// 界面展示名称。
    pub fn name(self) -> &'static str {
        match self {
            BaseN::Base16 => "Base16（Hex）",
            BaseN::Base32 => "Base32（RFC 4648）",
            BaseN::Base32Hex => "Base32Hex（RFC 4648）",
            BaseN::ZBase32 => "z-base-32",
            BaseN::Crockford32 => "Crockford Base32",
            BaseN::Base58Bitcoin => "Base58（Bitcoin）",
            BaseN::Base58Flickr => "Base58（Flickr）",
            BaseN::Base58Ripple => "Base58（Ripple）",
            BaseN::Base62 => "Base62",
            BaseN::Base64 => "Base64（标准）",
            BaseN::Base64Url => "Base64URL",
            BaseN::Base64Mime => "Base64（MIME 折行）",
            BaseN::Ascii85 => "ASCII85（Adobe）",
            BaseN::Z85 => "Z85（ZeroMQ）",
            BaseN::Rfc1924 => "Base85（RFC 1924）",
            BaseN::Base91 => "basE91",
            BaseN::Uuencode => "uuencode",
            BaseN::Xxencode => "xxencode",
        }
    }

    /// 一句话说明，用于界面上的灰色提示。
    pub fn note(self) -> &'static str {
        match self {
            BaseN::Base16 => "小写十六进制，每字节两位；解码忽略大小写与空白，长度必须为偶数",
            BaseN::Base32 => "RFC 4648 标准字母表 A-Z2-7，`=` 填充；解码忽略大小写、可省略填充",
            BaseN::Base32Hex => "RFC 4648 扩展十六进制字母表 0-9A-V，`=` 填充；解码可省略填充",
            BaseN::ZBase32 => "面向人工抄写的字母表 ybndrfg8…，无填充",
            BaseN::Crockford32 => "易读字母表，无填充；解码把 I/L 当 1、O 当 0，忽略连字符与大小写",
            BaseN::Base58Bitcoin => "Bitcoin 字母表，前导 0x00 编成前导 '1'，解码还原",
            BaseN::Base58Flickr => "Flickr 字母表，前导 0x00 编成前导 '1'，解码还原",
            BaseN::Base58Ripple => "Ripple 字母表（首字符 'r'），前导 0x00 编成前导 'r'",
            BaseN::Base62 => "0-9A-Za-z 大整数编码；前导 0x00 一一对应前导 '0'",
            BaseN::Base64 => "RFC 4648 标准字母表，带 `=` 填充；解码容忍空白与省略填充",
            BaseN::Base64Url => "URL 安全变体，用 `-` `_` 代替 `+` `/`；解码容忍省略填充",
            BaseN::Base64Mime => "标准 Base64，每 76 个字符插入 CRLF，便于邮件传输",
            BaseN::Ascii85 => "Adobe ASCII85，输出带 `<~ ~>` 包裹；`z` 代表 4 个零字节",
            BaseN::Z85 => "ZeroMQ Z85，输入需为 4 字节倍数（不足处补零，解码会多出零字节）",
            BaseN::Rfc1924 => "RFC 1924 Base85：4 字节 → 5 字符（大端 u32），末组补零截断",
            BaseN::Base91 => "basE91 位累加器算法，约 1.23 字节/字符，无填充",
            BaseN::Uuencode => "经典数据行（行首长度字符 + 每 3 字节 4 字符），零值用 '`'，末尾 '`' 收尾",
            BaseN::Xxencode => "与 uuencode 同构，字母表为 +-0-9A-Za-z；行首长度字符同样取自该表（0 即 '+'），末尾 '+' 收尾",
        }
    }

    /// 字节流 → 编码文本。任何输入长度都能编码，不 panic。
    pub fn encode(self, data: &[u8]) -> String {
        match self {
            BaseN::Base16 => encode_base16(data),
            BaseN::Base32 => encode_bit_groups(data, BASE32_ALPHABET, 5, true),
            BaseN::Base32Hex => encode_bit_groups(data, BASE32HEX_ALPHABET, 5, true),
            BaseN::ZBase32 => encode_bit_groups(data, ZBASE32_ALPHABET, 5, false),
            BaseN::Crockford32 => encode_bit_groups(data, CROCKFORD_ALPHABET, 5, false),
            BaseN::Base58Bitcoin => encode_bigint(data, BASE58_BITCOIN_ALPHABET),
            BaseN::Base58Flickr => encode_bigint(data, BASE58_FLICKR_ALPHABET),
            BaseN::Base58Ripple => encode_bigint(data, BASE58_RIPPLE_ALPHABET),
            BaseN::Base62 => encode_bigint(data, BASE62_ALPHABET),
            BaseN::Base64 => encode_base64(data, BASE64_STANDARD_ALPHABET),
            BaseN::Base64Url => encode_base64(data, BASE64_URL_ALPHABET),
            BaseN::Base64Mime => wrap_mime(&encode_base64(data, BASE64_STANDARD_ALPHABET)),
            BaseN::Ascii85 => {
                let body = encode_base85(data, &ASCII85_ALPHABET, true);
                let mut wrapped = String::with_capacity(body.len() + 4);
                wrapped.push_str("<~");
                wrapped.push_str(&body);
                wrapped.push_str("~>");
                wrapped
            }
            BaseN::Z85 => encode_z85(data),
            BaseN::Rfc1924 => encode_base85(data, RFC1924_ALPHABET, false),
            BaseN::Base91 => encode_base91(data),
            BaseN::Uuencode => encode_line_based(data, false),
            BaseN::Xxencode => encode_line_based(data, true),
        }
    }

    /// 编码文本 → 字节流，输入非法时返回中文错误。
    pub fn decode(self, text: &str) -> Result<Vec<u8>, String> {
        match self {
            BaseN::Base16 => decode_base16(text),
            BaseN::Base32 => decode_bit_groups(&text.to_ascii_uppercase(), 5, true, "Base32", |c| {
                lookup_byte(BASE32_ALPHABET, c)
            }),
            BaseN::Base32Hex => decode_bit_groups(
                &text.to_ascii_uppercase(),
                5,
                true,
                "Base32Hex",
                |c| lookup_byte(BASE32HEX_ALPHABET, c),
            ),
            BaseN::ZBase32 => {
                decode_bit_groups(&text.to_ascii_lowercase(), 5, false, "z-base-32", |c| {
                    lookup_byte(ZBASE32_ALPHABET, c)
                })
            }
            BaseN::Crockford32 => {
                let normalized = normalize_crockford(text);
                decode_bit_groups(&normalized, 5, false, "Crockford32", |c| {
                    lookup_byte(CROCKFORD_ALPHABET, c)
                })
            }
            BaseN::Base58Bitcoin => {
                decode_bigint(text, BASE58_BITCOIN_ALPHABET, "Base58（Bitcoin）")
            }
            BaseN::Base58Flickr => decode_bigint(text, BASE58_FLICKR_ALPHABET, "Base58（Flickr）"),
            BaseN::Base58Ripple => decode_bigint(text, BASE58_RIPPLE_ALPHABET, "Base58（Ripple）"),
            BaseN::Base62 => decode_bigint(text, BASE62_ALPHABET, "Base62"),
            BaseN::Base64 => decode_base64(text, BASE64_STANDARD_ALPHABET, "Base64"),
            BaseN::Base64Url => decode_base64(text, BASE64_URL_ALPHABET, "Base64URL"),
            BaseN::Base64Mime => {
                decode_base64(text, BASE64_STANDARD_ALPHABET, "Base64（MIME 折行）")
            }
            BaseN::Ascii85 => decode_base85(text, &ASCII85_ALPHABET, true, true, "ASCII85"),
            BaseN::Z85 => decode_z85(text),
            BaseN::Rfc1924 => decode_base85(text, RFC1924_ALPHABET, false, false, "RFC 1924"),
            BaseN::Base91 => decode_base91(text),
            BaseN::Uuencode => decode_line_based(text, false),
            BaseN::Xxencode => decode_line_based(text, true),
        }
    }
}

// ---------------------------------------------------------------------------
// 通用小工具
// ---------------------------------------------------------------------------

/// 把一个字符当作单个 ASCII 字节在字母表里查下标。
/// 非 ASCII 字符（中文、Emoji 等）直接判为非法，避免按字节截断产生误判。
fn lookup_byte(alphabet: &[u8], ch: char) -> Option<u8> {
    let mut buf = [0u8; 4];
    let text = ch.encode_utf8(&mut buf);
    if text.len() != 1 {
        return None;
    }
    let byte = text.as_bytes()[0];
    alphabet.iter().position(|&a| a == byte).map(|p| p as u8)
}

/// Crockford 的“防呆”归一化：忽略连字符与空白，I/L → 1、O → 0，统一大写。
fn normalize_crockford(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch == '-' || ch.is_whitespace() {
            continue;
        }
        let mapped = match ch.to_ascii_uppercase() {
            'I' | 'L' => '1',
            'O' => '0',
            other => other,
        };
        out.push(mapped);
    }
    out
}

// ---------------------------------------------------------------------------
// Base16
// ---------------------------------------------------------------------------

fn encode_base16(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len() * 2);
    for &byte in data {
        out.push(BASE16_ALPHABET[(byte >> 4) as usize] as char);
        out.push(BASE16_ALPHABET[(byte & 0x0F) as usize] as char);
    }
    out
}

fn decode_base16(text: &str) -> Result<Vec<u8>, String> {
    let mut digits = Vec::with_capacity(text.len());
    for (idx, ch) in text.chars().enumerate() {
        if ch.is_whitespace() {
            continue;
        }
        match ch.to_digit(16) {
            Some(value) => digits.push(value as u8),
            None => {
                return Err(format!(
                    "Base16 解码失败：第 {} 个字符 '{}' 不是十六进制数字",
                    idx + 1,
                    ch
                ))
            }
        }
    }
    if digits.len() % 2 != 0 {
        return Err(format!(
            "Base16 解码失败：共 {} 个十六进制字符，长度为奇数，无法凑成完整字节",
            digits.len()
        ));
    }
    Ok(digits.chunks(2).map(|pair| (pair[0] << 4) | pair[1]).collect())
}

// ---------------------------------------------------------------------------
// Base32 系：按位分组
// ---------------------------------------------------------------------------

/// 按 `bits` 位分组编码。`padded` 只对 RFC 4648 的 Base32 系有意义：
/// 补齐到 8 的倍数个字符（8 字符 = 40 位 = 5 字节），其余编码不补。
fn encode_bit_groups(data: &[u8], alphabet: &[u8], bits: u32, padded: bool) -> String {
    let mut out = String::with_capacity(data.len() * 2 + 8);
    let mut buffer: u32 = 0;
    let mut filled: u32 = 0;
    for &byte in data {
        buffer = ((buffer << 8) | byte as u32) & 0xFFFF;
        filled += 8;
        while filled >= bits {
            filled -= bits;
            let index = ((buffer >> filled) & ((1 << bits) - 1)) as usize;
            out.push(alphabet[index] as char);
        }
    }
    if filled > 0 {
        // 剩余不足一个字符的位，右侧补 0 后仍然输出一个字符。
        let index = ((buffer << (bits - filled)) & ((1 << bits) - 1)) as usize;
        out.push(alphabet[index] as char);
        if padded {
            while !out.len().is_multiple_of(8) {
                out.push('=');
            }
        }
    }
    out
}

/// 按位分组解码。`lookup` 由调用方决定大小写与“防呆”映射，
/// 这样 Base32、z-base-32、Crockford 能共享同一套位拼装与校验逻辑。
fn decode_bit_groups(
    text: &str,
    bits: u32,
    allow_padding: bool,
    label: &str,
    lookup: impl Fn(char) -> Option<u8>,
) -> Result<Vec<u8>, String> {
    let mut chars: Vec<char> = text.chars().filter(|ch| !ch.is_whitespace()).collect();
    let mut pad = 0usize;
    while chars.last() == Some(&'=') {
        chars.pop();
        pad += 1;
    }
    if chars.contains(&'=') {
        return Err(format!("{label} 解码失败：'=' 填充只允许出现在末尾"));
    }
    if pad > 0 && !allow_padding {
        return Err(format!("{label} 解码失败：该编码不使用 '=' 填充"));
    }
    let count = chars.len();
    // 5 位一组时，数据字符数 n 必须让 5n 位恰好填满若干字节并留下少于 5 位的尾巴；
    // n % 8 ∈ {1,3,6} 时尾巴多于一个字符能表示的范围，说明数据被截断或长度非法。
    if matches!(count % 8, 1 | 3 | 6) {
        return Err(format!(
            "{label} 解码失败：{} 个数据字符无法按 {bits} 位一组凑成完整字节",
            count
        ));
    }
    if allow_padding && pad > 0 {
        let required = (8 - count % 8) % 8;
        if pad != required {
            return Err(format!(
                "{label} 解码失败：填充错误（{} 个数据字符应补 {} 个 '='，实际补了 {} 个）",
                count, required, pad
            ));
        }
    }

    let mut out = Vec::with_capacity(count * bits as usize / 8 + 1);
    let mut buffer: u32 = 0;
    let mut filled: u32 = 0;
    for (idx, &ch) in chars.iter().enumerate() {
        let value = match lookup(ch) {
            Some(value) => value as u32,
            None => {
                return Err(format!(
                    "{label} 解码失败：第 {} 个字符 '{}' 不在字母表中",
                    idx + 1,
                    ch
                ))
            }
        };
        buffer = (buffer << bits) | value;
        filled += bits;
        while filled >= 8 {
            filled -= 8;
            out.push(((buffer >> filled) & 0xFF) as u8);
        }
        // 丢掉已经取走的低位，只保留尚未凑够一个字节的位。
        buffer = if filled == 0 {
            0
        } else {
            buffer & ((1u32 << filled) - 1)
        };
    }
    if filled > 0 && buffer != 0 {
        return Err(format!(
            "{label} 解码失败：末尾有 {} 位非零填充位（数据被截断或不属于该编码）",
            filled
        ));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Base58 / Base62：大整数换算
// ---------------------------------------------------------------------------

/// 把字节串看成大端大整数，反复除以进制取余数。
/// 前导零字节单独映射成字母表首字符，个数一一对应——这是 Base58/62 的通行约定，
/// 也是唯一能在“去掉前导零等同于去掉整数高位”的换算中保留原始长度的办法。
fn encode_bigint(data: &[u8], alphabet: &[u8]) -> String {
    if data.is_empty() {
        return String::new();
    }
    let radix = alphabet.len() as u32;
    let mut digits: Vec<u32> = Vec::new(); // 低位在前
    for &byte in data {
        let mut carry = byte as u32;
        for digit in digits.iter_mut() {
            let current = (*digit << 8) + carry;
            *digit = current % radix;
            carry = current / radix;
        }
        while carry > 0 {
            digits.push(carry % radix);
            carry /= radix;
        }
    }
    let leading_zeros = data.iter().take_while(|&&byte| byte == 0).count();
    let mut out = String::with_capacity(leading_zeros + digits.len());
    for _ in 0..leading_zeros {
        out.push(alphabet[0] as char);
    }
    for &digit in digits.iter().rev() {
        out.push(alphabet[digit as usize] as char);
    }
    out
}

fn decode_bigint(text: &str, alphabet: &[u8], label: &str) -> Result<Vec<u8>, String> {
    let radix = alphabet.len() as u32;
    let chars: Vec<char> = text.chars().filter(|ch| !ch.is_ascii_whitespace()).collect();
    // 前导“零字符”直接对应前导零字节，不参与整数运算。
    let mut leading_zeros = 0usize;
    while leading_zeros < chars.len() && chars[leading_zeros] as u32 == alphabet[0] as u32 {
        leading_zeros += 1;
    }
    let mut bytes: Vec<u8> = Vec::new(); // 小端二进制位，低位在前
    for (idx, &ch) in chars.iter().enumerate() {
        let digit = match lookup_byte(alphabet, ch) {
            Some(value) => value as u32,
            None => {
                return Err(format!(
                    "{label} 解码失败：第 {} 个字符 '{}' 不在字母表中",
                    idx + 1,
                    ch
                ))
            }
        };
        let mut carry = digit;
        for byte in bytes.iter_mut() {
            let current = (*byte as u32) * radix + carry;
            *byte = (current & 0xFF) as u8;
            carry = current >> 8;
        }
        while carry > 0 {
            bytes.push((carry & 0xFF) as u8);
            carry >>= 8;
        }
    }
    let mut out = Vec::with_capacity(leading_zeros + bytes.len());
    out.resize(leading_zeros, 0u8);
    out.extend(bytes.iter().rev());
    Ok(out)
}

// ---------------------------------------------------------------------------
// Base64 系
// ---------------------------------------------------------------------------

fn encode_base64(data: &[u8], alphabet: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let value = (b0 << 16) | (b1 << 8) | b2;
        out.push(alphabet[((value >> 18) & 0x3F) as usize] as char);
        out.push(alphabet[((value >> 12) & 0x3F) as usize] as char);
        if chunk.len() == 1 {
            out.push('=');
            out.push('=');
            continue;
        }
        out.push(alphabet[((value >> 6) & 0x3F) as usize] as char);
        if chunk.len() == 2 {
            out.push('=');
            continue;
        }
        out.push(alphabet[(value & 0x3F) as usize] as char);
    }
    out
}

/// MIME 折行：每 76 个字符插入 CRLF（只在行间插入，末尾不补 CRLF，方便解码往返）。
fn wrap_mime(text: &str) -> String {
    if text.chars().count() <= 76 {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() + text.len() / 76 * 2);
    for (idx, chunk) in chars.chunks(76).enumerate() {
        if idx > 0 {
            out.push_str("\r\n");
        }
        out.extend(chunk.iter());
    }
    out
}

fn decode_base64(text: &str, alphabet: &[u8], label: &str) -> Result<Vec<u8>, String> {
    // 折行、手抄时插入的空白都应被忽略。
    let chars: Vec<char> = text.chars().filter(|ch| !ch.is_whitespace()).collect();
    let mut data_end = chars.len();
    let mut pad = 0usize;
    while data_end > 0 && chars[data_end - 1] == '=' {
        data_end -= 1;
        pad += 1;
    }
    if pad > 2 {
        return Err(format!(
            "{label} 解码失败：末尾有 {} 个 '='，最多只允许 2 个",
            pad
        ));
    }
    if chars[..data_end].contains(&'=') {
        return Err(format!("{label} 解码失败：'=' 只能出现在末尾"));
    }
    let remainder = data_end % 4;
    if remainder == 1 {
        return Err(format!(
            "{label} 解码失败：去掉填充后剩 {} 个字符，无法凑成完整分组",
            data_end
        ));
    }
    if pad > 0 {
        let required = (4 - remainder) % 4;
        if pad != required {
            return Err(format!(
                "{label} 解码失败：填充错误（{} 个数据字符应补 {} 个 '='，实际补了 {} 个）",
                data_end, required, pad
            ));
        }
    }

    let mut table = [-1i16; 256];
    for (index, &byte) in alphabet.iter().enumerate() {
        table[byte as usize] = index as i16;
    }
    let mut out = Vec::with_capacity(data_end / 4 * 3 + 2);
    let mut accumulator: u32 = 0;
    let mut group_len = 0usize;
    for (idx, &ch) in chars[..data_end].iter().enumerate() {
        let mut buf = [0u8; 4];
        let encoded = ch.encode_utf8(&mut buf);
        let value = if encoded.len() == 1 {
            table[encoded.as_bytes()[0] as usize]
        } else {
            -1
        };
        if value < 0 {
            return Err(format!(
                "{label} 解码失败：第 {} 个字符 '{}' 不在字母表中",
                idx + 1,
                ch
            ));
        }
        accumulator = (accumulator << 6) | value as u32;
        group_len += 1;
        if group_len == 4 {
            out.push((accumulator >> 16) as u8);
            out.push((accumulator >> 8) as u8);
            out.push(accumulator as u8);
            accumulator = 0;
            group_len = 0;
        }
    }
    match group_len {
        0 => {}
        2 => out.push((accumulator >> 4) as u8),
        3 => {
            out.push((accumulator >> 10) as u8);
            out.push((accumulator >> 2) as u8);
        }
        _ => return Err(format!("{label} 解码失败：末尾分组不完整")),
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// ASCII85 / Z85 / RFC1924：4 字节 → 5 字符
// ---------------------------------------------------------------------------

/// 把 4 字节大端 u32 按 85 进制展开成 5 个字符。
/// - `zero_shortcut`：ASCII85 的 `z` 缩写（仅对完整的 4 个零字节生效）；
/// - 末组不足 4 字节时补零，并且只输出 `字节数 + 1` 个字符（85^4 < 2^32，
///   首字符至多携带 8 位信息，所以 n 个字节一定落在 n+1 个字符内）。
fn encode_base85(data: &[u8], alphabet: &[u8], zero_shortcut: bool) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(4) * 5);
    for chunk in data.chunks(4) {
        let mut block = [0u8; 4];
        block[..chunk.len()].copy_from_slice(chunk);
        let value = u32::from_be_bytes(block);
        if zero_shortcut && chunk.len() == 4 && value == 0 {
            out.push('z');
            continue;
        }
        let mut digits = [0u8; 5];
        let mut rest = value;
        for slot in digits.iter_mut().rev() {
            *slot = (rest % 85) as u8;
            rest /= 85;
        }
        let keep = if chunk.len() == 4 { 5 } else { chunk.len() + 1 };
        for &digit in digits.iter().take(keep) {
            out.push(alphabet[digit as usize] as char);
        }
    }
    out
}

fn decode_base85(
    text: &str,
    alphabet: &[u8],
    zero_shortcut: bool,
    accept_wrapper: bool,
    label: &str,
) -> Result<Vec<u8>, String> {
    let trimmed = text.trim();
    // Adobe 风格可带 `<~` … `~>`；不带包裹也接受，便于粘贴裸数据。
    let body = if accept_wrapper {
        if let Some(rest) = trimmed.strip_prefix("<~") {
            match rest.strip_suffix("~>") {
                Some(inner) => inner,
                None => {
                    return Err(format!(
                        "{label} 解码失败：以 '<~' 开头却没有对应的结尾 '~>'"
                    ))
                }
            }
        } else {
            trimmed.strip_suffix("~>").unwrap_or(trimmed)
        }
    } else {
        trimmed
    };

    let mut out = Vec::with_capacity(body.len());
    let mut group = [0u32; 5];
    let mut group_len = 0usize;
    for (idx, ch) in body.chars().enumerate() {
        if ch.is_whitespace() {
            continue;
        }
        if zero_shortcut && ch == 'z' {
            if group_len != 0 {
                return Err(format!(
                    "{label} 解码失败：'z' 只能出现在 4 字节分组边界上（第 {} 个字符）",
                    idx + 1
                ));
            }
            out.extend_from_slice(&[0, 0, 0, 0]);
            continue;
        }
        let value = match lookup_byte(alphabet, ch) {
            Some(value) => value as u32,
            None => {
                return Err(format!(
                    "{label} 解码失败：第 {} 个字符 '{}' 不在字母表中",
                    idx + 1,
                    ch
                ))
            }
        };
        group[group_len] = value;
        group_len += 1;
        if group_len == 5 {
            let value = base85_value(&group, 5);
            if value > u32::MAX as u64 {
                return Err(format!("{label} 解码失败：5 个字符组成的值超出 32 位"));
            }
            out.extend_from_slice(&(value as u32).to_be_bytes());
            group_len = 0;
        }
    }
    match group_len {
        0 => {}
        1 => {
            return Err(format!(
                "{label} 解码失败：末组只有 1 个字符，无法还原出任何字节"
            ))
        }
        _ => {
            // 末组用最大值补齐（ASCII85 用 'u'=84，RFC1924 用 '~'=84）。
            let value = base85_value(&group, group_len);
            if value > u32::MAX as u64 {
                return Err(format!("{label} 解码失败：末组字符组成的值超出 32 位"));
            }
            let bytes = (value as u32).to_be_bytes();
            out.extend_from_slice(&bytes[..group_len - 1]);
        }
    }
    Ok(out)
}

/// 把长度不足 5 的数字组补成 5 位后求值；补位用 84（85 进制最大值）。
fn base85_value(group: &[u32; 5], used: usize) -> u64 {
    let mut value: u64 = 0;
    for (idx, &digit) in group.iter().enumerate() {
        let digit = if idx < used { digit } else { 84 };
        value = value * 85 + digit as u64;
    }
    value
}

fn encode_z85(data: &[u8]) -> String {
    // Z85 官方要求输入长度为 4 的倍数；encode 的签名不允许报错，
    // 因此不足时补零（解码会多出对应零字节，note 中已说明）。
    let padded_len = data.len().div_ceil(4) * 4;
    let mut padded = Vec::with_capacity(padded_len);
    padded.extend_from_slice(data);
    padded.resize(padded_len, 0u8);
    encode_base85(&padded, Z85_ALPHABET, false)
}

fn decode_z85(text: &str) -> Result<Vec<u8>, String> {
    let count = text.chars().filter(|ch| !ch.is_whitespace()).count();
    if count % 5 != 0 {
        return Err(format!(
            "Z85 解码失败：数据共 {} 个字符，不是 5（即 4 字节）的整数倍",
            count
        ));
    }
    decode_base85(text, Z85_ALPHABET, false, false, "Z85")
}

// ---------------------------------------------------------------------------
// basE91
// ---------------------------------------------------------------------------

/// basE91 的位累加器编码：把 8 位输入位流切成 13/14 位的分组，
/// 每个分组拆成两个 91 进制数字。分组取 13 还是 14 位由分组值是否 > 88 决定，
/// 这样能避免把数字对映射到不可能的取值区间（91^2 = 8281 > 2^13）。
fn encode_base91(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len() * 2 + 2);
    let mut queue: u32 = 0;
    let mut bits: u32 = 0;
    for &byte in data {
        queue |= (byte as u32) << bits;
        bits += 8;
        if bits > 13 {
            let mut chunk = queue & 8191;
            if chunk > 88 {
                queue >>= 13;
                bits -= 13;
            } else {
                chunk = queue & 16383;
                queue >>= 14;
                bits -= 14;
            }
            out.push(BASE91_ALPHABET[(chunk % 91) as usize] as char);
            out.push(BASE91_ALPHABET[(chunk / 91) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(BASE91_ALPHABET[(queue % 91) as usize] as char);
        if bits > 7 || queue > 90 {
            out.push(BASE91_ALPHABET[(queue / 91) as usize] as char);
        }
    }
    out
}

fn decode_base91(text: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(text.len());
    let mut queue: u32 = 0;
    let mut bits: u32 = 0;
    let mut pending: i64 = -1; // 上一个数字，配对后才产生位分组
    for (idx, ch) in text.chars().enumerate() {
        if ch.is_whitespace() {
            continue;
        }
        let value = match lookup_byte(BASE91_ALPHABET, ch) {
            Some(value) => value as i64,
            None => {
                return Err(format!(
                    "basE91 解码失败：第 {} 个字符 '{}' 不在字母表中",
                    idx + 1,
                    ch
                ))
            }
        };
        if pending < 0 {
            pending = value;
            continue;
        }
        let pair = pending + value * 91;
        queue |= (pair as u32) << bits;
        bits += if pair & 8191 > 88 { 13 } else { 14 };
        while bits > 7 {
            out.push((queue & 0xFF) as u8);
            queue >>= 8;
            bits -= 8;
        }
        pending = -1;
    }
    if pending >= 0 {
        out.push(((queue | ((pending as u32) << bits)) & 0xFF) as u8);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// uuencode / xxencode：行式编码
// ---------------------------------------------------------------------------

/// uuencode 的数据字符：6 位值 + 32；值为 0 时改用 '`'，
/// 因为长串空格在邮件/打印链路上容易被吞掉（解码端两种写法都认）。
fn uuencode_data_char(value: u8) -> char {
    if value == 0 {
        '`'
    } else {
        (value + 0x20) as char
    }
}

/// 行首长度字符。uuencode 用“字节数 + 32”，0 写成 '`' 收尾；
/// xxencode 则直接把字节数当作 64 字母表的下标（0 即 '+'），
/// 这样整行都不会出现空格或反引号——这正是两套编码“行首字符不同”的来源。
fn line_length_char(bytes: usize, xx: bool) -> char {
    if xx {
        XXENCODE_ALPHABET[bytes] as char
    } else if bytes == 0 {
        '`'
    } else {
        (bytes as u8 + 0x20) as char
    }
}

fn encode_line_based(data: &[u8], xx: bool) -> String {
    let mut lines: Vec<String> = Vec::with_capacity(data.len() / 45 + 1);
    for chunk in data.chunks(45) {
        let mut line = String::with_capacity(1 + 60);
        line.push(line_length_char(chunk.len(), xx));
        for group in chunk.chunks(3) {
            let mut block = [0u8; 3];
            block[..group.len()].copy_from_slice(group);
            let value =
                ((block[0] as u32) << 16) | ((block[1] as u32) << 8) | block[2] as u32;
            for shift in [18u32, 12, 6, 0] {
                let digit = ((value >> shift) & 0x3F) as u8;
                line.push(if xx {
                    XXENCODE_ALPHABET[digit as usize] as char
                } else {
                    uuencode_data_char(digit)
                });
            }
        }
        lines.push(line);
    }
    // 经典格式里数据段以一行“长度 0”结束：uuencode 写 '`'，xxencode 写 '+'。
    lines.push(line_length_char(0, xx).to_string());
    lines.join("\n")
}

fn decode_line_based(text: &str, xx: bool) -> Result<Vec<u8>, String> {
    let label = if xx { "xxencode" } else { "uuencode" };
    let mut out = Vec::with_capacity(text.len());
    let mut seen_data = false;
    for (line_no, raw) in text.split('\n').enumerate() {
        let line = raw.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        // 允许直接粘贴带 begin/end 的完整文件，但二者本身不含数据。
        // 只有还没读到数据行时才把 begin 当头部，避免误跳过真正的数据行。
        if !seen_data && (line.starts_with("begin ") || line == "begin") {
            continue;
        }
        if line == "end" {
            break;
        }
        let bytes = line.as_bytes();
        let length_char = bytes[0];
        let count = if xx {
            // xxencode 的长度字符同样来自字母表，'+' 表示 0。
            match lookup_byte(XXENCODE_ALPHABET, length_char as char) {
                Some(value) => value as usize,
                None => {
                    return Err(format!(
                        "{label} 解码失败：第 {} 行的长度字符 '{}' 不在 xxencode 字母表中",
                        line_no + 1,
                        length_char as char
                    ))
                }
            }
        } else {
            if !((0x20..=0x5F).contains(&length_char) || length_char == 0x60) {
                return Err(format!(
                    "{label} 解码失败：第 {} 行的长度字符 {:#04X} 非法（应为 0x20..=0x60）",
                    line_no + 1,
                    length_char
                ));
            }
            (length_char as usize - 0x20) & 0x3F
        };
        if count == 0 {
            break; // 长度 0 的行表示数据段结束
        }
        if count > 45 {
            return Err(format!(
                "{label} 解码失败：第 {} 行声明了 {} 个字节，超过单行上限 45",
                line_no + 1,
                count
            ));
        }
        let payload = &bytes[1..];
        let expected = count.div_ceil(3) * 4;
        if payload.len() != expected {
            return Err(format!(
                "{label} 解码失败：第 {} 行声明 {} 个字节，应有 {} 个数据字符，实际 {} 个",
                line_no + 1,
                count,
                expected,
                payload.len()
            ));
        }
        let mut decoded = Vec::with_capacity(count);
        for group in payload.chunks(4) {
            let mut digits = [0u32; 4];
            for (slot, &byte) in group.iter().enumerate() {
                let digit = if xx {
                    lookup_byte(XXENCODE_ALPHABET, byte as char)
                } else if (0x20..=0x5F).contains(&byte) || byte == 0x60 {
                    Some(((byte as usize - 0x20) & 0x3F) as u8)
                } else {
                    None
                };
                match digit {
                    Some(value) => digits[slot] = value as u32,
                    None => {
                        return Err(format!(
                            "{label} 解码失败：第 {} 行出现非法字符 '{}'",
                            line_no + 1,
                            byte as char
                        ))
                    }
                }
            }
            let value = (digits[0] << 18) | (digits[1] << 12) | (digits[2] << 6) | digits[3];
            decoded.push((value >> 16) as u8);
            decoded.push((value >> 8) as u8);
            decoded.push(value as u8);
        }
        decoded.truncate(count);
        out.extend_from_slice(&decoded);
        seen_data = true;
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 简易线性同余随机字节，用于往返一致性测试（不引入额外依赖）。
    fn pseudorandom_bytes(len: usize, seed: u32) -> Vec<u8> {
        let mut state = seed;
        (0..len)
            .map(|_| {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                (state >> 24) as u8
            })
            .collect()
    }

    /// 覆盖各编码的边界输入：空、单字节、全零、全 FF、UTF-8 中文与 Emoji、随机数据。
    fn round_trip_datasets() -> Vec<(&'static str, Vec<u8>)> {
        let mut datasets: Vec<(&'static str, Vec<u8>)> = vec![
            ("空", Vec::new()),
            ("单字节 00", vec![0x00]),
            ("单字节 FF", vec![0xFF]),
            ("四字节全零", vec![0, 0, 0, 0]),
            ("Man", b"Man".to_vec()),
            ("foobar", b"foobar".to_vec()),
            ("hello world", b"hello world".to_vec()),
            ("中文 + Emoji", "中文😀".as_bytes().to_vec()),
            ("混合控制字符", vec![0x00, 0x01, 0x7F, 0x80, 0xFE, 0xFF]),
            ("四字节对齐", b"abcd".to_vec()),
            ("8 字节对齐", b"abcdefgh".to_vec()),
        ];
        datasets.push(("随机 512 字节", pseudorandom_bytes(512, 0x5EED)));
        datasets.push(("随机 45 字节", pseudorandom_bytes(45, 0x1234)));
        datasets
    }

    #[test]
    fn metadata_is_complete_and_unique() {
        assert_eq!(BaseN::ALL.len(), 18);
        let mut ids = std::collections::HashSet::new();
        let mut names = std::collections::HashSet::new();
        for kind in BaseN::ALL {
            assert!(!kind.id().is_empty(), "id 不能为空");
            assert!(!kind.name().is_empty(), "name 不能为空");
            assert!(!kind.note().is_empty(), "note 不能为空");
            assert!(ids.insert(kind.id()), "id 重复：{}", kind.id());
            assert!(names.insert(kind.name()), "name 重复：{}", kind.name());
        }
    }

    #[test]
    fn alphabets_have_expected_shape() {
        // 字母表一旦手抖打错（缺字符/重复字符），这里会立刻暴露。
        for (label, alphabet, expected) in [
            ("base16", BASE16_ALPHABET, 16usize),
            ("base32", BASE32_ALPHABET, 32),
            ("base32hex", BASE32HEX_ALPHABET, 32),
            ("zbase32", ZBASE32_ALPHABET, 32),
            ("crockford", CROCKFORD_ALPHABET, 32),
            ("base58btc", BASE58_BITCOIN_ALPHABET, 58),
            ("base58flickr", BASE58_FLICKR_ALPHABET, 58),
            ("base58ripple", BASE58_RIPPLE_ALPHABET, 58),
            ("base62", BASE62_ALPHABET, 62),
            ("base64", BASE64_STANDARD_ALPHABET, 64),
            ("base64url", BASE64_URL_ALPHABET, 64),
            ("xxencode", XXENCODE_ALPHABET, 64),
            ("z85", Z85_ALPHABET, 85),
            ("rfc1924", RFC1924_ALPHABET, 85),
            ("base91", BASE91_ALPHABET, 91),
            ("ascii85", &ASCII85_ALPHABET, 85),
        ] {
            assert_eq!(alphabet.len(), expected, "{label} 字母表长度不对");
            let unique: std::collections::HashSet<u8> = alphabet.iter().copied().collect();
            assert_eq!(unique.len(), expected, "{label} 字母表存在重复字符");
            assert!(
                alphabet.iter().all(|byte| byte.is_ascii_graphic()),
                "{label} 字母表含不可打印字符"
            );
        }
        // Ripple 的首字符决定了前导零的写法，必须是 'r'。
        assert_eq!(BASE58_RIPPLE_ALPHABET[0], b'r');
        assert!(CROCKFORD_ALPHABET
            .iter()
            .all(|byte| !matches!(*byte, b'I' | b'L' | b'O' | b'U')));
    }

    #[test]
    fn reference_vectors_match_published_encodings() {
        // 期望值来自 RFC 4648 / RFC 1924 / Adobe / ZeroMQ / basE91 的公开示例或
        // Python 标准库（base64、binascii）的独立实现。
        let cases: &[(BaseN, &[u8], &str)] = &[
            (BaseN::Base16, b"Man", "4d616e"),
            (BaseN::Base16, b"foobar", "666f6f626172"),
            (BaseN::Base32, b"", ""),
            (BaseN::Base32, b"\x00", "AA======"),
            (BaseN::Base32, b"foobar", "MZXW6YTBOI======"),
            (
                BaseN::Base32,
                b"hello world",
                "NBSWY3DPEB3W64TMMQ======",
            ),
            (BaseN::Base32Hex, b"\x00", "00======"),
            (BaseN::Base32Hex, b"foobar", "CPNMUOJ1E8======"),
            (BaseN::ZBase32, b"\x00", "yy"),
            (BaseN::ZBase32, b"Man", "jiosh"),
            (BaseN::ZBase32, b"foobar", "c3zs6aubqe"),
            (BaseN::Crockford32, b"\x00", "00"),
            (BaseN::Crockford32, b"Man", "9NGPW"),
            (BaseN::Crockford32, b"foobar", "CSQPYRK1E8"),
            (BaseN::Base58Bitcoin, b"\x00", "1"),
            (BaseN::Base58Bitcoin, b"\x00\x00\x00\x00", "1111"),
            (BaseN::Base58Bitcoin, b"Man", "SzVj"),
            (BaseN::Base58Bitcoin, b"foobar", "t1Zv2yaZ"),
            (BaseN::Base58Bitcoin, b"hello world", "StV1DL6CwTryKyV"),
            (BaseN::Base58Flickr, b"foobar", "T1yV2Yzy"),
            (BaseN::Base58Flickr, b"hello world", "rTu1dk6cWsRYjYu"),
            (BaseN::Base58Ripple, b"\x00", "r"),
            (BaseN::Base58Ripple, b"foobar", "trZvpy2Z"),
            (BaseN::Base62, b"\x00", "0"),
            (BaseN::Base62, b"Man", "LHFm"),
            (BaseN::Base62, b"foobar", "VytN8Wjy"),
            (BaseN::Base64, b"Man", "TWFu"),
            (BaseN::Base64, b"foobar", "Zm9vYmFy"),
            (BaseN::Base64, b"Cat", "Q2F0"),
            (BaseN::Base64Url, "中文😀".as_bytes(), "5Lit5paH8J-YgA=="),
            (BaseN::Ascii85, b"", "<~~>"),
            (BaseN::Ascii85, b"\x00", "<~!!~>"),
            (BaseN::Ascii85, b"\x00\x00\x00\x00", "<~z~>"),
            (BaseN::Ascii85, b"Man", "<~9jqo~>"),
            (BaseN::Ascii85, b"Cat", "<~6Xb'~>"),
            (BaseN::Ascii85, b"foobar", "<~AoDTs@<)~>"),
            (BaseN::Z85, b"\x00\x00\x00\x00", "00000"),
            (BaseN::Rfc1924, b"Man", "O<`^"),
            (BaseN::Rfc1924, b"Cat", "Lt%6"),
            (BaseN::Rfc1924, b"foobar", "W^Zp|VR8"),
            (BaseN::Base91, b"\x00", "AA"),
            (BaseN::Base91, b"\x00\x00\x00\x00", "AAAAA"),
            (BaseN::Base91, b"Man", "8D$J"),
            (BaseN::Base91, b"Cat", "yDVK"),
            (BaseN::Base91, b"foobar", "dr/2s)uC"),
            (BaseN::Base91, b"hello world", "TPwJh>Io2Tv!lE"),
            (BaseN::Uuencode, b"Cat", "#0V%T\n`"),
            (BaseN::Uuencode, b"hello\n", "&:&5L;&\\*\n`"),
            (BaseN::Xxencode, b"", "+"),
            (BaseN::Xxencode, b"\x00", "-++++\n+"),
            (BaseN::Xxencode, b"Cat", "1Eq3o\n+"),
            (BaseN::Xxencode, b"foobar", "4NaxjMa3m\n+"),
        ];
        for (kind, input, expected) in cases {
            assert_eq!(
                &kind.encode(input),
                expected,
                "{} 编码 {:?} 的结果与参考实现不一致",
                kind.id(),
                input
            );
            // 参考值同样要能被自己解回来。
            assert_eq!(
                kind.decode(expected),
                Ok(input.to_vec()),
                "{} 无法解码参考值 {:?}",
                kind.id(),
                expected
            );
        }
    }

    #[test]
    fn round_trip_every_kind_and_dataset() {
        for kind in BaseN::ALL {
            for (name, data) in round_trip_datasets() {
                let encoded = kind.encode(&data);
                let decoded = match kind.decode(&encoded) {
                    Ok(bytes) => bytes,
                    Err(err) => panic!("{} 编码「{}」后无法解码：{}", kind.id(), name, err),
                };
                if kind == BaseN::Z85 {
                    // Z85 没有部分分组，encode 会补零，解码必然多出零字节。
                    let padded_len = data.len().div_ceil(4) * 4;
                    assert_eq!(
                        decoded.len(),
                        padded_len,
                        "{} 解码长度不符（{}）",
                        kind.id(),
                        name
                    );
                    assert_eq!(&decoded[..data.len()], &data[..], "{} 前段数据不符", kind.id());
                    assert!(
                        decoded[data.len()..].iter().all(|&byte| byte == 0),
                        "{} 补位应全为零字节",
                        kind.id()
                    );
                } else {
                    assert_eq!(&decoded, &data, "{} 往返失败（{}）", kind.id(), name);
                }
            }
        }
    }

    #[test]
    fn empty_input_round_trips_for_every_kind() {
        for kind in BaseN::ALL {
            let encoded = kind.encode(b"");
            assert_eq!(
                kind.decode(&encoded),
                Ok(Vec::new()),
                "{} 空输入往返失败（编码结果 {encoded:?}）",
                kind.id()
            );
            assert_eq!(kind.decode(""), Ok(Vec::new()), "{} 空文本应解出空字节", kind.id());
        }
        assert_eq!(BaseN::Ascii85.encode(b""), "<~~>");
        assert_eq!(BaseN::Uuencode.encode(b""), "`");
        assert_eq!(BaseN::Xxencode.encode(b""), "+");
    }

    #[test]
    fn leading_zero_bytes_are_preserved() {
        let samples: &[&[u8]] = &[
            &[0x00],
            &[0x00, 0x00],
            &[0x00, 0x01],
            &[0x00, 0x00, 0x01],
            &[0x00, 0x00, 0x00, 0xFF],
            &[0x00, 0x42, 0x00],
        ];
        for kind in [
            BaseN::Base58Bitcoin,
            BaseN::Base58Flickr,
            BaseN::Base58Ripple,
            BaseN::Base62,
        ] {
            for sample in samples {
                let encoded = kind.encode(sample);
                assert_eq!(
                    encoded.chars().next(),
                    Some(match kind {
                        BaseN::Base58Ripple => 'r',
                        BaseN::Base58Bitcoin | BaseN::Base58Flickr => '1',
                        _ => '0',
                    }),
                    "{} 的前导零应映射为字母表首字符",
                    kind.id()
                );
                assert_eq!(
                    kind.decode(&encoded),
                    Ok(sample.to_vec()),
                    "{} 丢失前导零字节",
                    kind.id()
                );
            }
        }
        // 0x0001 与 0x01 是不同字节串，编码结果也必须不同。
        assert_ne!(
            BaseN::Base58Bitcoin.encode(&[0x00, 0x01]),
            BaseN::Base58Bitcoin.encode(&[0x01])
        );
    }

    #[test]
    fn base32_family_is_case_insensitive_and_tolerates_whitespace() {
        let encoded = BaseN::Base32.encode(b"foobar");
        assert_eq!(BaseN::Base32.decode(&encoded.to_lowercase()), Ok(b"foobar".to_vec()));
        assert_eq!(
            BaseN::Base32.decode("  MZXW6YTB\nOI======  "),
            Ok(b"foobar".to_vec())
        );
        // 允许省略填充。
        assert_eq!(BaseN::Base32.decode("MZXW6YTBOI"), Ok(b"foobar".to_vec()));
        assert_eq!(BaseN::Base32Hex.decode("cpnmuoj1e8"), Ok(b"foobar".to_vec()));
        // z-base-32 无填充，但也容忍大小写。
        let zb = BaseN::ZBase32.encode(b"foobar");
        assert_eq!(BaseN::ZBase32.decode(&zb.to_uppercase()), Ok(b"foobar".to_vec()));
    }

    #[test]
    fn crockford_decoding_normalizes_confusable_characters() {
        let encoded = BaseN::Crockford32.encode(b"foobar");
        assert_eq!(BaseN::Crockford32.decode(&encoded), Ok(b"foobar".to_vec()));
        // 小写、连字符、I/L→1、O→0 都要能读。
        let friendly: String = encoded
            .to_lowercase()
            .chars()
            .map(|ch| match ch {
                '0' => 'o',
                '1' => 'l',
                other => other,
            })
            .collect();
        let with_hyphen = friendly
            .chars()
            .map(|ch| ch.to_string())
            .collect::<Vec<_>>()
            .join("-");
        assert_eq!(
            BaseN::Crockford32.decode(&with_hyphen),
            Ok(b"foobar".to_vec()),
            "Crockford 解码应容忍小写、连字符与 I/L/O 混淆"
        );
        assert_eq!(
            BaseN::Crockford32.decode("csqpyrk1e8"),
            Ok(b"foobar".to_vec())
        );
    }

    #[test]
    fn base64_variants_handle_padding_and_wrapping() {
        // 省略填充也能解。
        assert_eq!(BaseN::Base64.decode("TWFu"), Ok(b"Man".to_vec()));
        assert_eq!(BaseN::Base64.decode("TWE"), Ok(b"Ma".to_vec()));
        assert_eq!(BaseN::Base64.decode("TQ=="), Ok(b"M".to_vec()));
        // URL 变体与标准变体的差异只在 +/ 与 -_。
        let bytes = [0xFB_u8, 0xFF, 0xBF];
        assert!(BaseN::Base64.encode(&bytes).contains('+'));
        assert!(BaseN::Base64Url.encode(&bytes).contains('-'));
        assert_eq!(
            BaseN::Base64Url.decode(&BaseN::Base64Url.encode(&bytes)),
            Ok(bytes.to_vec())
        );
        // MIME 每 76 字符折行，且行宽不超过 76。
        let data = pseudorandom_bytes(200, 7);
        let mime = BaseN::Base64Mime.encode(&data);
        assert!(mime.contains("\r\n"), "MIME 变体应插入 CRLF 折行");
        for line in mime.split("\r\n") {
            assert!(line.len() <= 76, "MIME 行宽超限：{}", line.len());
        }
        assert_eq!(BaseN::Base64Mime.decode(&mime), Ok(data));
    }

    #[test]
    fn ascii85_accepts_optional_wrapper_and_z() {
        assert_eq!(BaseN::Ascii85.decode("<~9jqo~>"), Ok(b"Man".to_vec()));
        assert_eq!(BaseN::Ascii85.decode("9jqo"), Ok(b"Man".to_vec()));
        assert_eq!(BaseN::Ascii85.decode("  <~9jq\n o~>  "), Ok(b"Man".to_vec()));
        assert_eq!(
            BaseN::Ascii85.decode("<~z~>"),
            Ok(vec![0, 0, 0, 0])
        );
        assert!(BaseN::Ascii85.decode("<~9jqo").is_err(), "缺少 ~> 应报错");
        assert!(
            BaseN::Ascii85.decode("<~9zqo~>").is_err(),
            "'z' 不能出现在分组中间"
        );
    }

    #[test]
    fn z85_matches_zeromq_reference_vector() {
        // ZeroMQ 官方文档的示例：这 8 个字节编码恰为 "HelloWorld"。
        let bytes = [0x86u8, 0x4F, 0xD2, 0x6F, 0xB5, 0x59, 0xF7, 0x5B];
        assert_eq!(BaseN::Z85.encode(&bytes), "HelloWorld");
        assert_eq!(BaseN::Z85.decode("HelloWorld"), Ok(bytes.to_vec()));
    }

    #[test]
    fn line_based_encodings_wrap_at_45_bytes_and_tolerate_headers() {
        let data = vec![b'a'; 100];

        // ---- uuencode：行首字符 = 字节数 + 32，收尾行 '`' ----
        let encoded = BaseN::Uuencode.encode(&data);
        let lines: Vec<&str> = encoded.split('\n').collect();
        assert_eq!(lines.len(), 4, "100 字节应切成 45/45/10 三行 + 收尾行");
        assert_eq!(lines[3], "`");
        assert_eq!(lines[0].len(), 1 + 60);
        assert_eq!(lines[2].len(), 1 + 16);
        assert_eq!(lines[0].as_bytes()[0], 45 + 32, "uuencode 长度字符 = 字节数 + 32");
        assert_eq!(BaseN::Uuencode.decode(&encoded), Ok(data.clone()));

        // ---- xxencode：行首字符同样取自 64 字母表，收尾行 '+' ----
        let encoded_xx = BaseN::Xxencode.encode(&data);
        let xx_lines: Vec<&str> = encoded_xx.split('\n').collect();
        assert_eq!(xx_lines.len(), 4);
        assert_eq!(xx_lines[3], "+");
        assert_eq!(xx_lines[0].len(), 1 + 60);
        assert_eq!(
            xx_lines[0].as_bytes()[0], XXENCODE_ALPHABET[45],
            "xxencode 长度字符是字母表下标"
        );
        assert_eq!(BaseN::Xxencode.decode(&encoded_xx), Ok(data.clone()));

        // 同一份数据，两种编码的行首字符必然不同（定义性差异）。
        assert_ne!(
            BaseN::Uuencode.encode(b"Cat").as_bytes()[0],
            BaseN::Xxencode.encode(b"Cat").as_bytes()[0]
        );

        // 直接粘贴带 begin/end 的完整文件也能解（两种编码都接受）。
        let uu_body = lines[..lines.len() - 1].join("\n");
        let uu_wrapped = format!("begin 644 demo.txt\n{uu_body}\nend\n");
        assert_eq!(
            BaseN::Uuencode.decode(&uu_wrapped),
            Ok(data.clone()),
            "带 begin/end 头尾时应能正常解码"
        );
        let xx_body = xx_lines[..xx_lines.len() - 1].join("\n");
        let xx_wrapped = format!("begin 644 demo.txt\n{xx_body}\nend\n");
        assert_eq!(BaseN::Xxencode.decode(&xx_wrapped), Ok(data.clone()));

        // 经典 uuencode 编码器用空格表示 0，我们的解码也必须接受。
        assert_eq!(BaseN::Uuencode.decode("!    \n`"), Ok(vec![0]));
        // xxencode 里 0 就是字母表首字符 '+'。
        assert_eq!(BaseN::Xxencode.decode("-++++\n+"), Ok(vec![0]));
    }

    #[test]
    fn decode_rejects_invalid_input_with_chinese_errors() {
        let cases: &[(BaseN, &str)] = &[
            (BaseN::Base16, "abc"),
            (BaseN::Base16, "zz"),
            (BaseN::Base32, "MZXW6YTB!!"),
            (BaseN::Base32, "A"),
            (BaseN::Base32, "AA=A"),
            (BaseN::Base32, "AA="),
            (BaseN::ZBase32, "yyyy===="),
            (BaseN::Crockford32, "UUUU"),
            (BaseN::Base58Bitcoin, "0OIl"),
            (BaseN::Base58Flickr, "-"),
            (BaseN::Base58Ripple, "0"),
            (BaseN::Base62, "-1"),
            (BaseN::Base64, "A"),
            (BaseN::Base64, "AB=C"),
            (BaseN::Base64, "A==="),
            (BaseN::Base64Url, "a+b/"),
            (BaseN::Ascii85, "<~a~>"),
            (BaseN::Ascii85, "<~uuuuu~>"),
            (BaseN::Ascii85, "<~9j"),
            (BaseN::Z85, "AAAA"),
            (BaseN::Z85, "AAA;A"),
            (BaseN::Rfc1924, "O\""),
            (BaseN::Base91, "Cat's"),
            (BaseN::Uuencode, "!AB"),
            (BaseN::Uuencode, "!   ~"),
            (BaseN::Xxencode, "!AB"),
            (BaseN::Xxencode, "!***"),
        ];
        for (kind, input) in cases {
            let err = kind.decode(input).expect_err(&format!(
                "{} 解码 {input:?} 本应失败却成功了",
                kind.id()
            ));
            assert!(
                err.contains("解码失败"),
                "{} 的错误信息应说明失败原因，实际：{err}",
                kind.id()
            );
        }
    }

    #[test]
    fn decoding_never_panics_on_arbitrary_text() {
        // 用户可能把任何东西粘进解码框：这些输入要么解出字节，要么返回 Err，
        // 但绝不能 panic。
        let inputs = [
            "",
            "=",
            "==",
            "z",
            "~>",
            "<~",
            "😀",
            "中文字符",
            "\u{0}\u{1}",
            "  \r\n\t ",
            "uuuuu",
            "zzzzz",
            "\u{FFFD}",
        ];
        for kind in BaseN::ALL {
            for input in inputs {
                let _ = kind.decode(input);
            }
        }
    }
}
