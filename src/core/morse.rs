//! 摩尔斯电码：文本 ↔ 点划序列。
//!
//! 采用国际摩尔斯表（ITU-R M.1677），字母之间用一个空格分隔、单词之间用
//! ` / ` 分隔——这是最常见、也最适合在纯文本里手工编辑的写法。
//!
//! 编码是“尽力而为”：表里没有的字符（中文、Emoji、生僻符号）直接跳过，
//! 不报错也不产生空的码组，这样用户把一整段中英混排贴进来时不会整个失败。
//! 解码则是严格的：任何一个码组不在表里都会返回中文错误，并指出是第几个
//! 码组，避免把打错的点划静默解成别的字母。

/// 国际摩尔斯电码表：大写字母、数字与常用标点。
const MORSE_TABLE: &[(char, &str)] = &[
    ('A', ".-"),
    ('B', "-..."),
    ('C', "-.-."),
    ('D', "-.."),
    ('E', "."),
    ('F', "..-."),
    ('G', "--."),
    ('H', "...."),
    ('I', ".."),
    ('J', ".---"),
    ('K', "-.-"),
    ('L', ".-.."),
    ('M', "--"),
    ('N', "-."),
    ('O', "---"),
    ('P', ".--."),
    ('Q', "--.-"),
    ('R', ".-."),
    ('S', "..."),
    ('T', "-"),
    ('U', "..-"),
    ('V', "...-"),
    ('W', ".--"),
    ('X', "-..-"),
    ('Y', "-.--"),
    ('Z', "--.."),
    ('0', "-----"),
    ('1', ".----"),
    ('2', "..---"),
    ('3', "...--"),
    ('4', "....-"),
    ('5', "....."),
    ('6', "-...."),
    ('7', "--..."),
    ('8', "---.."),
    ('9', "----."),
    ('.', ".-.-.-"),
    (',', "--..--"),
    ('?', "..--.."),
    ('\'', ".----."),
    ('!', "-.-.--"),
    ('/', "-..-."),
    ('(', "-.--."),
    (')', "-.--.-"),
    ('&', ".-..."),
    (':', "---..."),
    (';', "-.-.-."),
    ('=', "-...-"),
    ('+', ".-.-."),
    ('-', "-....-"),
    ('_', "..--.-"),
    ('"', ".-..-."),
    ('$', "...-..-"),
    ('@', ".--.-."),
];

/// 字符 → 点划序列（大小写不敏感，只认 ASCII 字母）。
fn morse_of(ch: char) -> Option<&'static str> {
    let upper = ch.to_ascii_uppercase();
    MORSE_TABLE
        .iter()
        .find(|(symbol, _)| *symbol == upper)
        .map(|(_, code)| *code)
}

/// 点划序列 → 字符。
fn char_of(code: &str) -> Option<char> {
    MORSE_TABLE
        .iter()
        .find(|(_, morse)| *morse == code)
        .map(|(symbol, _)| *symbol)
}

/// 文本 → 摩尔斯（字母之间空格，单词之间 ` / `）。
///
/// 连续空白会被折叠成单个单词分隔；表中没有的字符直接跳过（不 panic），
/// 因此 `"中 a"` 会得到 `.-`。
pub fn encode(text: &str) -> String {
    let mut words: Vec<Vec<&'static str>> = Vec::new();
    let mut current: Vec<&'static str> = Vec::new();
    for ch in text.chars() {
        if ch.is_whitespace() {
            // 只有真正攒到过字母才另起一个单词，连续的空白因此被折叠。
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            continue;
        }
        if let Some(code) = morse_of(ch) {
            current.push(code);
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
        .iter()
        .map(|word| word.join(" "))
        .collect::<Vec<_>>()
        .join(" / ")
}

/// 摩尔斯 → 文本。
///
/// 单个空格分隔字母，`/`（两侧可有空白）分隔单词；未知码组返回中文错误。
pub fn decode(input: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut word_break = false;
    let mut position = 0usize;
    for token in input.split_whitespace() {
        // 单词分隔符只记账，等真正读出下一个字母时再落一个空格，
        // 这样首尾多余的 `/` 不会产生前导/尾随空格。
        // 顺带兼容 `..../-..-` 这种 `/` 两侧没有空格的写法。
        for (segment_index, segment) in token.split('/').enumerate() {
            if segment_index > 0 {
                word_break = true;
            }
            if segment.is_empty() {
                continue;
            }
            position += 1;
            let symbol = char_of(segment).ok_or_else(|| {
                format!(
                    "摩尔斯解码失败：第 {} 个码组「{}」不在国际摩尔斯表中",
                    position, segment
                )
            })?;
            if !out.is_empty() && word_break {
                out.push(' ');
            }
            out.push(symbol);
            word_break = false;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_internally_consistent() {
        assert_eq!(MORSE_TABLE.len(), 54, "字母 26 + 数字 10 + 标点 18");
        let mut symbols = std::collections::HashSet::new();
        let mut codes = std::collections::HashSet::new();
        for (symbol, code) in MORSE_TABLE {
            assert!(symbols.insert(*symbol), "字符重复：{symbol}");
            assert!(codes.insert(*code), "码组重复：{code}");
            assert!(
                code.chars().all(|c| c == '.' || c == '-'),
                "{symbol} 的码组含非点划字符"
            );
            assert!(!code.is_empty(), "{symbol} 的码组为空");
        }
    }

    #[test]
    fn every_table_entry_round_trips() {
        for (symbol, code) in MORSE_TABLE {
            assert_eq!(encode(&symbol.to_string()), *code, "{symbol} 编码错误");
            assert_eq!(
                decode(code).as_deref(),
                Ok(&symbol.to_string()[..]),
                "{code} 解码错误"
            );
        }
    }

    #[test]
    fn encodes_letters_words_and_punctuation() {
        assert_eq!(encode("SOS"), "... --- ...");
        assert_eq!(
            encode("hello world"),
            ".... . .-.. .-.. --- / .-- --- .-. .-.. -.."
        );
        assert_eq!(encode("Hello, World!"), ".... . .-.. .-.. --- --..-- / .-- --- .-. .-.. -.. -.-.--");
        assert_eq!(encode("E"), ".");
        assert_eq!(encode("5"), ".....");
        assert_eq!(encode(".,?'!/()&:;=+-_\"$@"), ".-.-.- --..-- ..--.. .----. -.-.-- -..-. -.--. -.--.- .-... ---... -.-.-. -...- .-.-. -....- ..--.- .-..-. ...-..- .--.-.");
    }

    #[test]
    fn whitespace_is_collapsed_and_unsupported_chars_are_skipped() {
        assert_eq!(encode("a  b"), ".- / -...", "连续空格应折叠成单词分隔");
        assert_eq!(encode("  a  "), ".-", "首尾空白不应产生多余分隔");
        assert_eq!(encode("a\tb\nc"), ".- / -... / -.-.", "制表符与换行也是单词分隔");
        assert_eq!(encode("中文"), "", "不支持的字符全部跳过");
        assert_eq!(encode("中 a 😀"), ".-", "被跳过的字符不该产生空码组");
        assert_eq!(encode(""), "");
        assert_eq!(encode("   "), "");
    }

    #[test]
    fn decodes_single_letters_words_and_punctuation() {
        assert_eq!(decode("... --- ...").as_deref(), Ok("SOS"));
        assert_eq!(
            decode(".... . .-.. .-.. --- / .-- --- .-. .-.. -..").as_deref(),
            Ok("HELLO WORLD")
        );
        assert_eq!(decode(".-.-.- --..-- ..--..").as_deref(), Ok(".,?"));
        assert_eq!(decode("").as_deref(), Ok(""));
        assert_eq!(decode("   ").as_deref(), Ok(""));
        // 多个分隔符只算一次；首尾分隔符不产生多余空格。
        assert_eq!(decode("/ ... / / --- /").as_deref(), Ok("S O"));
        assert_eq!(decode("...\t---").as_deref(), Ok("SO"), "制表符也可作分隔");
        // `/` 两侧即使没有空格也要能识别出单词分隔。
        assert_eq!(decode("..../-..-").as_deref(), Ok("H X"), "'/' 两侧没有空格也表示单词分隔");
        assert_eq!(decode("./-").as_deref(), Ok("E T"), "斜杠紧贴码组");
        assert_eq!(decode("-..-./.----").as_deref(), Ok("/ 1"));
    }

    #[test]
    fn sentence_round_trips() {
        let sentence = "THE QUICK BROWN FOX 123 .,?";
        let morse = encode(sentence);
        assert_eq!(decode(&morse).as_deref(), Ok(sentence));
    }

    #[test]
    fn decode_rejects_unknown_groups_with_position() {
        let err = decode("... ....... ...").expect_err("未知码组应报错");
        assert!(err.contains("摩尔斯解码失败"), "错误信息应为中文：{err}");
        assert!(err.contains("第 2 个码组"), "错误信息应指出位置：{err}");
        assert!(decode(".......").is_err());
        assert!(decode("... x ...").is_err());
        assert!(decode(".-/x").is_err(), "斜杠分隔后仍有非法码组时要报错");
    }
}
