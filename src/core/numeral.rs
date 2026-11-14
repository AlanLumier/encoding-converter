//! 特殊数制表示：罗马数字、双射 26 进制、六十进制、平衡三进制、负二进制、阶乘进制、
//! 斐波那契/Zeckendorf 表示、玛雅数字。

/// 罗马数字（1–3999）。超出范围返回 None。
pub fn roman_encode(mut n: u32) -> Option<String> {
    if !(1..=3999).contains(&n) {
        return None;
    }
    const TABLE: [(u32, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::new();
    for (value, glyph) in TABLE {
        while n >= value {
            out.push_str(glyph);
            n -= value;
        }
    }
    Some(out)
}

/// 解析罗马数字，返回十进制值（支持 I/V/X/L/C/D/M 及减法组合）。
pub fn roman_decode(input: &str) -> Result<u32, String> {
    let s = input.trim().to_ascii_uppercase();
    if s.is_empty() {
        return Err("输入为空".into());
    }
    for c in s.chars() {
        if !"IVXLCDM".contains(c) {
            return Err(format!("罗马数字中不允许字符 '{c}'"));
        }
    }
    let value = |c: char| match c {
        'I' => 1,
        'V' => 5,
        'X' => 10,
        'L' => 50,
        'C' => 100,
        'D' => 500,
        'M' => 1000,
        _ => 0,
    };
    let chars: Vec<char> = s.chars().collect();
    let mut total: i64 = 0;
    for (i, &c) in chars.iter().enumerate() {
        let v = value(c);
        let next = chars.get(i + 1).map(|&n| value(n)).unwrap_or(0);
        if v < next {
            total -= v as i64;
        } else {
            total += v as i64;
        }
    }
    let total = total as u32;
    // 规范写法校验：转回罗马数字必须与原串一致
    match roman_encode(total) {
        Some(canonical) if canonical == s => Ok(total),
        Some(_) => Err("不是规范的罗马数字写法".into()),
        None => Err("超出罗马数字可表示范围（1–3999）".into()),
    }
}

/// 双射 26 进制（Excel 列名）：1→A，26→Z，27→AA。
pub fn bijective26_encode(mut n: u64) -> Option<String> {
    if n == 0 {
        return None;
    }
    let mut out = Vec::new();
    while n > 0 {
        n -= 1;
        out.push((b'A' + (n % 26) as u8) as char);
        n /= 26;
    }
    Some(out.iter().rev().collect())
}

pub fn bijective26_decode(input: &str) -> Result<u64, String> {
    let s = input.trim().to_ascii_uppercase();
    if s.is_empty() {
        return Err("输入为空".into());
    }
    let mut n: u64 = 0;
    for c in s.chars() {
        if !c.is_ascii_uppercase() {
            return Err(format!("只允许 A–Z，发现 '{c}'"));
        }
        n = n
            .checked_mul(26)
            .and_then(|v| v.checked_add(c as u64 - 'A' as u64 + 1))
            .ok_or("数值过大")?;
    }
    Ok(n)
}

/// 六十进制（古巴比伦/时间/角度）：按位用 `:` 分隔，可选小数点用 `;`。
pub fn sexagesimal_encode(mut n: u64) -> String {
    if n == 0 {
        return "0".into();
    }
    let mut parts = Vec::new();
    while n > 0 {
        parts.push((n % 60).to_string());
        n /= 60;
    }
    parts.reverse();
    parts.join(":")
}

pub fn sexagesimal_decode(input: &str) -> Result<u64, String> {
    let s = input.trim();
    if s.is_empty() {
        return Err("输入为空".into());
    }
    let mut total: u64 = 0;
    for part in s.split(':') {
        let part = part.trim();
        if part.is_empty() {
            return Err("位之间不能为空".into());
        }
        let v: u64 = part.parse().map_err(|_| format!("'{part}' 不是十进制数"))?;
        if v >= 60 {
            return Err(format!("每一位必须小于 60，发现 {v}"));
        }
        total = total
            .checked_mul(60)
            .and_then(|t| t.checked_add(v))
            .ok_or("数值过大")?;
    }
    Ok(total)
}

/// 平衡三进制：数位为 `-` / `0` / `+`（也可写作 `-1/0/1`）。
pub fn balanced_ternary_encode(mut n: i64) -> String {
    if n == 0 {
        return "0".into();
    }
    let mut out = Vec::new();
    while n != 0 {
        let rem = n.rem_euclid(3);
        match rem {
            0 => out.push('0'),
            1 => out.push('+'),
            _ => out.push('-'),
        }
        n = (n - if rem == 2 { -1 } else { rem }) / 3;
    }
    out.iter().rev().collect()
}

pub fn balanced_ternary_decode(input: &str) -> Result<i64, String> {
    let s = input.trim();
    if s.is_empty() {
        return Err("输入为空".into());
    }
    let mut total: i64 = 0;
    for c in s.chars() {
        let digit = match c {
            '-' => -1,
            '0' => 0,
            '+' => 1,
            _ => return Err(format!("平衡三进制只允许 '-'、'0'、'+'，发现 '{c}'")),
        };
        total = total
            .checked_mul(3)
            .and_then(|t| t.checked_add(digit))
            .ok_or("数值过大")?;
    }
    Ok(total)
}

/// 负二进制（基 -2），数位 0/1。
pub fn negabinary_encode(mut n: i64) -> String {
    if n == 0 {
        return "0".into();
    }
    let mut out = Vec::new();
    while n != 0 {
        let rem = n.rem_euclid(-2);
        out.push(if rem == 0 { '0' } else { '1' });
        n = (n - rem) / -2;
    }
    out.iter().rev().collect()
}

pub fn negabinary_decode(input: &str) -> Result<i64, String> {
    let s = input.trim();
    if s.is_empty() {
        return Err("输入为空".into());
    }
    let mut total: i64 = 0;
    for c in s.chars() {
        let digit = match c {
            '0' => 0,
            '1' => 1,
            _ => return Err(format!("负二进制只允许 0/1，发现 '{c}'")),
        };
        total = total
            .checked_mul(-2)
            .and_then(|t| t.checked_add(digit))
            .ok_or("数值过大")?;
    }
    Ok(total)
}

/// 阶乘进制：第 i 位（从右数第 1 位开始）权重为 i!。
pub fn factorial_encode(mut n: u64) -> String {
    if n == 0 {
        return "0".into();
    }
    let mut digits = Vec::new();
    let mut base = 1u64;
    while n > 0 {
        digits.push(n % (base + 1));
        n /= base + 1;
        base += 1;
    }
    digits.iter().rev().map(|d| d.to_string()).collect::<Vec<_>>().join(":")
}

pub fn factorial_decode(input: &str) -> Result<u64, String> {
    let s = input.trim();
    if s.is_empty() {
        return Err("输入为空".into());
    }
    let parts: Vec<&str> = s.split(':').collect();
    let mut total: u64 = 0;
    let mut weight: u64 = 1; // 第 1 位（最右）权重 1!
    let mut position: u64 = 1;
    for part in parts.iter().rev() {
        let digit: u64 = part
            .trim()
            .parse()
            .map_err(|_| format!("'{part}' 不是十进制数"))?;
        if digit > position {
            return Err(format!("第 {position} 位最大只能是 {position}，发现 {digit}"));
        }
        total = total.checked_add(digit * weight).ok_or("数值过大")?;
        position += 1;
        weight *= position;
    }
    Ok(total)
}

/// Zeckendorf / 斐波那契进制：用不连续的斐波那契数之和表示，输出 0/1 串。
pub fn zeckendorf_encode(mut n: u64) -> String {
    if n == 0 {
        return "0".into();
    }
    let mut fibs = vec![1u64, 2u64];
    while *fibs.last().unwrap() <= n {
        let len = fibs.len();
        let next = fibs[len - 1] + fibs[len - 2];
        if next > n {
            break;
        }
        fibs.push(next);
    }
    let mut bits = vec!['0'; fibs.len()];
    for i in (0..fibs.len()).rev() {
        if fibs[i] <= n {
            bits[i] = '1';
            n -= fibs[i];
        }
    }
    bits.iter().rev().collect()
}

pub fn zeckendorf_decode(input: &str) -> Result<u64, String> {
    let s = input.trim();
    if s.is_empty() {
        return Err("输入为空".into());
    }
    let chars: Vec<char> = s.chars().rev().collect();
    let mut total: u64 = 0;
    let mut a: u64 = 1;
    let mut b: u64 = 2;
    for c in chars {
        match c {
            '0' => {}
            '1' => total = total.checked_add(a).ok_or("数值过大")?,
            _ => return Err(format!("只允许 0/1，发现 '{c}'")),
        }
        let next = a + b;
        a = b;
        b = next;
    }
    Ok(total)
}

/// 玛雅数字：20 进制，0–19 用 Unicode 玛雅数字字符（U+1D2E0–U+1D2F3）表示，位之间用 `.`。
pub fn mayan_encode(mut n: u64) -> String {
    if n == 0 {
        return '\u{1D2E0}'.to_string();
    }
    let mut parts = Vec::new();
    while n > 0 {
        let d = (n % 20) as u32;
        parts.push(char::from_u32(0x1D2E0 + d).unwrap_or('?').to_string());
        n /= 20;
    }
    parts.reverse();
    parts.join(".")
}

pub fn mayan_decode(input: &str) -> Result<u64, String> {
    let s = input.trim();
    if s.is_empty() {
        return Err("输入为空".into());
    }
    let mut total: u64 = 0;
    for part in s.split('.') {
        let mut chars = part.chars();
        let c = chars.next().ok_or("位之间不能为空")?;
        if chars.next().is_some() {
            return Err("每个玛雅数字位只能有一个字符".into());
        }
        let code = c as u32;
        if !(0x1D2E0..=0x1D2F3).contains(&code) {
            return Err(format!("'{c}' 不是玛雅数字字符"));
        }
        total = total
            .checked_mul(20)
            .and_then(|t| t.checked_add((code - 0x1D2E0) as u64))
            .ok_or("数值过大")?;
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roman() {
        assert_eq!(roman_encode(1994).unwrap(), "MCMXCIV");
        assert_eq!(roman_decode("MCMXCIV").unwrap(), 1994);
        assert!(roman_decode("IIII").is_err());
        assert!(roman_encode(4000).is_none());
    }

    #[test]
    fn bijective26() {
        assert_eq!(bijective26_encode(1).unwrap(), "A");
        assert_eq!(bijective26_encode(26).unwrap(), "Z");
        assert_eq!(bijective26_encode(27).unwrap(), "AA");
        assert_eq!(bijective26_decode("AA").unwrap(), 27);
        assert_eq!(bijective26_decode("XFD").unwrap(), 16384);
    }

    #[test]
    fn sexagesimal() {
        assert_eq!(sexagesimal_encode(3725), "1:2:5");
        assert_eq!(sexagesimal_decode("1:2:5").unwrap(), 3725);
        assert!(sexagesimal_decode("1:60").is_err());
    }

    #[test]
    fn balanced_ternary() {
        assert_eq!(balanced_ternary_encode(5), "+--");
        assert_eq!(balanced_ternary_decode("+--").unwrap(), 5);
        assert_eq!(balanced_ternary_encode(0), "0");
    }

    #[test]
    fn negabinary() {
        assert_eq!(negabinary_encode(6), "11010");
        assert_eq!(negabinary_decode("11010").unwrap(), 6);
        assert_eq!(negabinary_encode(-3), "1101");
    }

    #[test]
    fn zeckendorf() {
        assert_eq!(zeckendorf_encode(10), "10010");
        assert_eq!(zeckendorf_decode("10010").unwrap(), 10);
    }

    #[test]
    fn mayan_roundtrip() {
        assert_eq!(mayan_decode(&mayan_encode(401)).unwrap(), 401);
    }

    #[test]
    fn factorial_roundtrip_small() {
        for n in 0..1000u64 {
            let encoded = factorial_encode(n);
            assert_eq!(factorial_decode(&encoded).unwrap(), n, "n={n} encoded={encoded}");
        }
    }
}
