//! 任意进制（2–36）互转、补码与 IEEE-754 视图。
//!
//! 整数部分用 base-1e9 的大整数实现，支持任意长度；小数部分用“源进制数位数组反复乘以目标基数”
//! 的精确算法，结果在小数位耗尽时自然终止。

const DIGIT_CHARS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";

/// 字符 → 数位值（0–35），不区分大小写。
pub fn digit_value(c: char) -> Option<u32> {
    match c {
        '0'..='9' => Some(c as u32 - '0' as u32),
        'a'..='z' => Some(c as u32 - 'a' as u32 + 10),
        'A'..='Z' => Some(c as u32 - 'A' as u32 + 10),
        _ => None,
    }
}

/// 数位值（0–35）→ 字符。
pub fn digit_char(v: u32) -> Option<char> {
    DIGIT_CHARS.get(v as usize).map(|&b| b as char)
}

/// 从 `0x` / `0b` / `0o` / `0d` 前缀推断进制；返回去掉前缀后的文本。
pub fn strip_prefix(input: &str) -> (&str, Option<u32>) {
    let s = input.trim();
    let lower = s.to_ascii_lowercase();
    for (prefix, radix) in [("0x", 16u32), ("0b", 2), ("0o", 8), ("0d", 10)] {
        if lower.starts_with(prefix) {
            return (&s[2..], Some(radix));
        }
    }
    (s, None)
}

/// 进制转换：`input` 是 `from` 进制文本，返回 `to` 进制文本（小写字母、保留符号与小数）。
///
/// * 允许 `_`、空格作为分隔符，例如 `1010_1010`；
/// * 小数结果最多输出 `max_frac_digits` 位，超出后截断（不四舍五入）。
pub fn convert(input: &str, from: u32, to: u32, max_frac_digits: usize) -> Result<String, String> {
    if !(2..=36).contains(&from) {
        return Err(format!("源进制 {from} 不在 2–36 范围内"));
    }
    if !(2..=36).contains(&to) {
        return Err(format!("目标进制 {to} 不在 2–36 范围内"));
    }

    let (body, prefix_radix) = strip_prefix(input);
    if let Some(r) = prefix_radix {
        if r != from {
            return Err(format!("前缀表示 {r} 进制，与选择的源进制 {from} 不一致"));
        }
    }

    let cleaned: String = body
        .chars()
        .filter(|c| !matches!(c, '_' | ' ' | '\t' | ','))
        .collect();
    if cleaned.is_empty() {
        return Err("输入为空".into());
    }

    let negative = cleaned.starts_with('-');
    let cleaned = cleaned.trim_start_matches(['-', '+']);

    let (int_part, frac_part) = match cleaned.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (cleaned, None),
    };
    if int_part.is_empty() && frac_part.is_none_or(|f| f.is_empty()) {
        return Err("输入里没有数字".into());
    }
    if cleaned.matches('.').count() > 1 {
        return Err("小数点数量不对".into());
    }

    let int_digits = parse_digits(if int_part.is_empty() { "0" } else { int_part }, from)?;
    let mut big = Big::default();
    for d in int_digits {
        big.mul_add(from, d);
    }
    let mut out = big.into_radix(to);

    if let Some(frac) = frac_part {
        if !frac.is_empty() {
            let frac_digits = parse_digits(frac, from)?;
            let converted = frac_to_radix(frac_digits, from, to, max_frac_digits);
            if !converted.is_empty() {
                out.push('.');
                out.push_str(&converted);
            }
        }
    }

    if negative && out != "0" {
        out.insert(0, '-');
    }
    Ok(out)
}

fn parse_digits(s: &str, radix: u32) -> Result<Vec<u32>, String> {
    s.chars()
        .map(|c| match digit_value(c) {
            Some(v) if v < radix => Ok(v),
            Some(_) => Err(format!("字符 '{c}' 超出 {radix} 进制的取值范围")),
            None => Err(format!("非法字符 '{c}'")),
        })
        .collect()
}

/// 小数部分：把源进制数位反复乘以目标基数，进位即输出位。
fn frac_to_radix(mut digits: Vec<u32>, from: u32, to: u32, max_digits: usize) -> String {
    let mut out = String::new();
    for _ in 0..max_digits {
        if digits.iter().all(|&d| d == 0) {
            break;
        }
        let mut carry: u32 = 0;
        for d in digits.iter_mut().rev() {
            let v = *d * to + carry;
            *d = v % from;
            carry = v / from;
        }
        match digit_char(carry) {
            Some(c) => out.push(c),
            None => break,
        }
    }
    out
}

/// base-1e9 无符号大整数（limbs 小端）。
#[derive(Default, Clone)]
struct Big {
    limbs: Vec<u32>,
}

impl Big {
    fn mul_add(&mut self, mul: u32, add: u32) {
        let mut carry = add as u64;
        for limb in self.limbs.iter_mut() {
            let v = *limb as u64 * mul as u64 + carry;
            *limb = (v % 1_000_000_000) as u32;
            carry = v / 1_000_000_000;
        }
        while carry > 0 {
            self.limbs.push((carry % 1_000_000_000) as u32);
            carry /= 1_000_000_000;
        }
        self.trim();
    }

    fn div_mod(&mut self, d: u32) -> u32 {
        let mut rem: u64 = 0;
        for limb in self.limbs.iter_mut().rev() {
            let cur = rem * 1_000_000_000 + *limb as u64;
            *limb = (cur / d as u64) as u32;
            rem = cur % d as u64;
        }
        self.trim();
        rem as u32
    }

    fn is_zero(&self) -> bool {
        self.limbs.is_empty()
    }

    fn trim(&mut self) {
        while self.limbs.last() == Some(&0) {
            self.limbs.pop();
        }
    }

    /// 反复除以目标基数，把大整数转成目标进制字符串（会消耗 self）。
    fn into_radix(mut self, to: u32) -> String {
        if self.is_zero() {
            return "0".into();
        }
        let mut digits = Vec::new();
        while !self.is_zero() {
            let r = self.div_mod(to);
            digits.push(digit_char(r).unwrap_or('?'));
        }
        digits.iter().rev().collect()
    }
}

/// 按位宽解释二进制补码，返回有符号十进制值。`bits` 取 8/16/32/64。
pub fn twos_complement_decimal(input: &str, radix: u32, bits: u32) -> Result<String, String> {
    let parsed = convert(input, radix, 10, 0)?;
    let n: u128 = parsed
        .parse()
        .map_err(|_| "数值超出 128 位范围，无法做补码换算".to_string())?;
    if bits == 0 || bits > 128 {
        return Err("位宽需在 1–128 之间".into());
    }
    let m = 1u128 << bits;
    if n >= m {
        return Err(format!("数值超出 {bits} 位能表示的范围"));
    }
    let half = m / 2;
    let signed = if n >= half { n as i128 - m as i128 } else { n as i128 };
    Ok(signed.to_string())
}

/// 浮点数的 IEEE-754 分解视图（单精度或双精度）。
#[derive(Debug, Clone, PartialEq)]
pub struct FloatView {
    pub bits: u64,
    pub bit_width: u32,
    pub sign: u8,
    pub exponent: String,
    pub mantissa: String,
    pub binary: String,
    pub hex: String,
}

pub fn ieee754(value: f64, single: bool) -> FloatView {
    let (bits, width) = if single {
        ((value as f32).to_bits() as u64, 32u32)
    } else {
        (value.to_bits(), 64u32)
    };
    let frac_len = if single { 23 } else { 52 };
    let exp_len = if single { 8 } else { 11 };
    let exponent = (bits >> frac_len) & ((1u64 << exp_len) - 1);
    let mantissa = bits & ((1u64 << frac_len) - 1);
    let binary = format!("{bits:0width$b}", width = width as usize);
    FloatView {
        bits,
        bit_width: width,
        sign: (bits >> (width - 1)) as u8 & 1,
        exponent: format!("{exponent:0width$b}（{exponent}）", width = exp_len as usize),
        mantissa: format!("{mantissa:0width$b}", width = frac_len as usize),
        binary,
        hex: format!("0x{bits:0width$X}", width = (width / 4) as usize),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(input: &str, from: u32, to: u32) -> String {
        convert(input, from, to, 16).unwrap()
    }

    #[test]
    fn basic_bases() {
        assert_eq!(c("255", 10, 16), "ff");
        assert_eq!(c("ff", 16, 10), "255");
        assert_eq!(c("1010", 2, 10), "10");
        assert_eq!(c("0x1F", 16, 2), "11111");
        assert_eq!(c("777", 8, 16), "1ff");
    }

    #[test]
    fn big_integer() {
        assert_eq!(
            c("123456789012345678901234567890", 10, 16),
            "18ee90ff6c373e0ee4e3f0ad2"
        );
        assert_eq!(c("ffffffffffffffffffffffff", 16, 10), "79228162514264337593543950335");
    }

    #[test]
    fn negative_numbers() {
        assert_eq!(c("-255", 10, 16), "-ff");
        assert_eq!(c("+42", 10, 2), "101010");
    }

    #[test]
    fn fractions() {
        assert_eq!(c("0.5", 10, 2), "0.1");
        assert_eq!(c("10.1", 2, 10), "2.5");
        assert_eq!(c("0.75", 10, 2), "0.11");
    }

    #[test]
    fn errors() {
        assert!(convert("2", 2, 10, 8).is_err());
        assert!(convert("", 10, 2, 8).is_err());
        assert!(convert("12", 1, 10, 8).is_err());
        assert!(convert("0x10", 10, 2, 8).is_err());
    }

    #[test]
    fn separators() {
        assert_eq!(c("1010_1010", 2, 16), "aa");
        assert_eq!(c("1 000 000", 10, 16), "f4240");
    }

    #[test]
    fn twos_complement() {
        assert_eq!(twos_complement_decimal("255", 10, 8).unwrap(), "-1");
        assert_eq!(twos_complement_decimal("7f", 16, 8).unwrap(), "127");
        assert_eq!(twos_complement_decimal("10000000", 2, 8).unwrap(), "-128");
    }

    #[test]
    fn float_view() {
        let v = ieee754(1.5, true);
        assert_eq!(v.hex, "0x3FC00000");
        assert_eq!(v.bits, 0x3FC0_0000);
        assert_eq!(v.sign, 0);
    }
}
