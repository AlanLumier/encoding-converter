//! 字符编码注册表：encoding_rs 官方表 + 自研 codec。
//!
//! 调用方（GUI/CLI/批量任务）只依赖本模块导出的 `list`/`find`/`decode`/`encode`。

pub mod custom;
pub mod table_codec;
pub mod tables;

use encoding_rs as ers;

/// 编码分组，用于界面分类展示。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Group {
    Unicode,
    Chinese,
    Japanese,
    Korean,
    Western,
    Legacy,
}

impl Group {
    pub const ALL: [Group; 6] = [
        Group::Unicode,
        Group::Chinese,
        Group::Japanese,
        Group::Korean,
        Group::Western,
        Group::Legacy,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Group::Unicode => "Unicode",
            Group::Chinese => "中文",
            Group::Japanese => "日文",
            Group::Korean => "韩文",
            Group::Western => "西欧 / 单字节",
            Group::Legacy => "传统 / 特殊",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Codec {
    Ers(&'static ers::Encoding),
    Custom(custom::CustomCodec),
    /// 表驱动的单字节代码页（DOS OEM / EBCDIC / 老 Mac）
    Table(&'static [u16; 256]),
}

#[derive(Clone, Copy, Debug)]
pub struct EncodingInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub group: Group,
    pub note: &'static str,
    pub codec: Codec,
}

#[derive(Default, Clone, Debug)]
pub struct DecodeResult {
    pub text: String,
    pub had_errors: bool,
    /// 出错的字节偏移（最多记录 64 个）
    pub error_offsets: Vec<usize>,
    pub bom_removed: Option<&'static str>,
}

#[derive(Default, Clone, Debug)]
pub struct EncodeResult {
    pub bytes: Vec<u8>,
    /// 目标编码无法表示的字符（去重前的顺序与原文一致）
    pub unmappable: Vec<char>,
}

const fn ers_entry(
    id: &'static str,
    name: &'static str,
    aliases: &'static [&'static str],
    group: Group,
    note: &'static str,
    enc: &'static ers::Encoding,
) -> EncodingInfo {
    EncodingInfo {
        id,
        name,
        aliases,
        group,
        note,
        codec: Codec::Ers(enc),
    }
}

const fn custom_entry(
    id: &'static str,
    name: &'static str,
    aliases: &'static [&'static str],
    group: Group,
    note: &'static str,
    codec: custom::CustomCodec,
) -> EncodingInfo {
    EncodingInfo {
        id,
        name,
        aliases,
        group,
        note,
        codec: Codec::Custom(codec),
    }
}

const fn table_entry(
    id: &'static str,
    name: &'static str,
    aliases: &'static [&'static str],
    group: Group,
    note: &'static str,
    table: &'static [u16; 256],
) -> EncodingInfo {
    EncodingInfo {
        id,
        name,
        aliases,
        group,
        note,
        codec: Codec::Table(table),
    }
}

/// 全部已实现的编码。顺序即界面展示顺序（按 Group 升序）。
pub static LIST: &[EncodingInfo] = &[
    // ---------- Unicode ----------
    ers_entry("utf-8", "UTF-8", &["utf8"], Group::Unicode, "变长 1–4 字节，Web 事实标准", ers::UTF_8),
    ers_entry("utf-16le", "UTF-16LE", &["utf16le"], Group::Unicode, "小端 2 字节（含代理对）", ers::UTF_16LE),
    ers_entry("utf-16be", "UTF-16BE", &["utf16be"], Group::Unicode, "大端 2 字节", ers::UTF_16BE),
    custom_entry("utf-32le", "UTF-32LE", &[], Group::Unicode, "小端 4 字节定长", custom::UTF32_LE),
    custom_entry("utf-32be", "UTF-32BE", &[], Group::Unicode, "大端 4 字节定长", custom::UTF32_BE),
    custom_entry("utf-32", "UTF-32（自动 BOM）", &["utf32"], Group::Unicode, "有 BOM 按 BOM，无 BOM 默认小端", custom::UTF32),
    custom_entry("ucs-2le", "UCS-2LE", &["ucs2le"], Group::Unicode, "仅 BMP，遇到代理项报错", custom::UCS2_LE),
    custom_entry("ucs-2be", "UCS-2BE", &["ucs2be"], Group::Unicode, "仅 BMP", custom::UCS2_BE),
    custom_entry("cesu-8", "CESU-8", &["cesu8"], Group::Unicode, "非 BMP 字符用两个 3 字节代理项", custom::CESU8),
    custom_entry("mutf-8", "Modified UTF-8", &["mutf8", "java-mutf8"], Group::Unicode, "Java 变体，NUL 写作 C0 80", custom::MUTF8),
    custom_entry("wtf-8", "WTF-8", &["wtf8"], Group::Unicode, "WTF-8；孤立代理项按错误处理", custom::WTF8),
    // ---------- 中文 ----------
    ers_entry("gbk", "GBK", &["cp936", "gb2312", "euc-cn"], Group::Chinese, "简体中文 Windows 默认", ers::GBK),
    ers_entry("gb18030", "GB18030", &["gb18030-2000", "gb18030-2005", "gb18030-2022"], Group::Chinese, "国标强制标准，覆盖全部 Unicode", ers::GB18030),
    ers_entry("big5", "Big5", &["cp950", "big5-hkscs"], Group::Chinese, "繁体中文（HKSCS 扩展字符部分支持）", ers::BIG5),
    // ---------- 日文 ----------
    ers_entry("shift_jis", "Shift_JIS", &["sjis", "cp932", "windows-31j", "ms-kanji"], Group::Japanese, "Windows 日文默认", ers::SHIFT_JIS),
    ers_entry("euc-jp", "EUC-JP", &["ujis"], Group::Japanese, "Unix 日文", ers::EUC_JP),
    ers_entry("iso-2022-jp", "ISO-2022-JP", &["jis"], Group::Japanese, "带转义序列的七位编码", ers::ISO_2022_JP),
    // ---------- 韩文 ----------
    ers_entry("euc-kr", "EUC-KR", &["cp949", "uhc", "ks_c_5601-1987"], Group::Korean, "韩文 Windows 默认", ers::EUC_KR),
    // ---------- 西欧 / 单字节 ----------
    custom_entry("iso-8859-1", "ISO-8859-1", &["latin1", "latin-1"], Group::Western, "真正的 Latin-1（0x80–0x9F 是控制字符）", custom::LATIN1),
    ers_entry("windows-1252", "Windows-1252", &["cp1252", "ansi"], Group::Western, "西欧 Windows 默认（€ 与弯引号）", ers::WINDOWS_1252),
    ers_entry("windows-1250", "Windows-1250", &["cp1250"], Group::Western, "中欧", ers::WINDOWS_1250),
    ers_entry("windows-1251", "Windows-1251", &["cp1251"], Group::Western, "西里尔（俄文）", ers::WINDOWS_1251),
    ers_entry("windows-1253", "Windows-1253", &["cp1253"], Group::Western, "希腊文", ers::WINDOWS_1253),
    ers_entry("windows-1254", "Windows-1254", &["cp1254"], Group::Western, "土耳其文", ers::WINDOWS_1254),
    ers_entry("windows-1255", "Windows-1255", &["cp1255"], Group::Western, "希伯来文", ers::WINDOWS_1255),
    ers_entry("windows-1256", "Windows-1256", &["cp1256"], Group::Western, "阿拉伯文", ers::WINDOWS_1256),
    ers_entry("windows-1257", "Windows-1257", &["cp1257"], Group::Western, "波罗的海", ers::WINDOWS_1257),
    ers_entry("windows-1258", "Windows-1258", &["cp1258"], Group::Western, "越南文", ers::WINDOWS_1258),
    ers_entry("windows-874", "Windows-874", &["cp874", "tis-620", "iso-8859-11"], Group::Western, "泰文", ers::WINDOWS_874),
    ers_entry("iso-8859-2", "ISO-8859-2", &["latin2"], Group::Western, "中欧", ers::ISO_8859_2),
    ers_entry("iso-8859-3", "ISO-8859-3", &["latin3"], Group::Western, "南欧", ers::ISO_8859_3),
    ers_entry("iso-8859-4", "ISO-8859-4", &["latin4"], Group::Western, "北欧", ers::ISO_8859_4),
    ers_entry("iso-8859-5", "ISO-8859-5", &["cyrillic"], Group::Western, "西里尔", ers::ISO_8859_5),
    ers_entry("iso-8859-6", "ISO-8859-6", &["arabic"], Group::Western, "阿拉伯文", ers::ISO_8859_6),
    ers_entry("iso-8859-7", "ISO-8859-7", &["greek"], Group::Western, "希腊文", ers::ISO_8859_7),
    ers_entry("iso-8859-8", "ISO-8859-8", &["hebrew"], Group::Western, "希伯来文", ers::ISO_8859_8),
    ers_entry("iso-8859-8-i", "ISO-8859-8-I", &[], Group::Western, "逻辑序希伯来文", ers::ISO_8859_8_I),
    ers_entry("iso-8859-10", "ISO-8859-10", &["latin6"], Group::Western, "北欧", ers::ISO_8859_10),
    ers_entry("iso-8859-13", "ISO-8859-13", &["latin7"], Group::Western, "波罗的海", ers::ISO_8859_13),
    ers_entry("iso-8859-14", "ISO-8859-14", &["latin8"], Group::Western, "凯尔特", ers::ISO_8859_14),
    ers_entry("iso-8859-15", "ISO-8859-15", &["latin9"], Group::Western, "Latin-9（含 €）", ers::ISO_8859_15),
    ers_entry("iso-8859-16", "ISO-8859-16", &["latin10"], Group::Western, "东南欧", ers::ISO_8859_16),
    ers_entry("koi8-r", "KOI8-R", &["koi8r"], Group::Western, "俄文（KOI8）", ers::KOI8_R),
    ers_entry("koi8-u", "KOI8-U", &["koi8u"], Group::Western, "乌克兰文", ers::KOI8_U),
    ers_entry("macintosh", "MacRoman", &["macroman", "mac"], Group::Western, "经典 Mac 西欧", ers::MACINTOSH),
    ers_entry("x-mac-cyrillic", "MacCyrillic", &["maccyrillic"], Group::Western, "Mac 西里尔", ers::X_MAC_CYRILLIC),
    ers_entry("ibm866", "IBM866", &["cp866", "866"], Group::Western, "DOS 俄文", ers::IBM866),
    // ---------- 传统 / 特殊：DOS OEM ----------
    table_entry("cp437", "CP437（DOS 美式）", &["ibm437", "437"], Group::Legacy, "DOS OEM：美国 / 西欧", &tables::CP437),
    table_entry("cp720", "CP720（DOS 阿拉伯）", &["ibm720"], Group::Legacy, "DOS OEM：阿拉伯文", &tables::CP720),
    table_entry("cp737", "CP737（DOS 希腊）", &["ibm737"], Group::Legacy, "DOS OEM：希腊文", &tables::CP737),
    table_entry("cp775", "CP775（DOS 波罗的海）", &["ibm775"], Group::Legacy, "DOS OEM：波罗的海", &tables::CP775),
    table_entry("cp850", "CP850（DOS 西欧）", &["ibm850"], Group::Legacy, "DOS OEM：西欧（含 box 绘图符）", &tables::CP850),
    table_entry("cp852", "CP852（DOS 中欧）", &["ibm852"], Group::Legacy, "DOS OEM：中欧", &tables::CP852),
    table_entry("cp855", "CP855（DOS 西里尔）", &["ibm855"], Group::Legacy, "DOS OEM：西里尔", &tables::CP855),
    table_entry("cp857", "CP857（DOS 土耳其）", &["ibm857"], Group::Legacy, "DOS OEM：土耳其文", &tables::CP857),
    table_entry("cp858", "CP858（DOS 西欧 + €）", &["ibm858"], Group::Legacy, "DOS OEM：CP850 加欧元符号", &tables::CP858),
    table_entry("cp860", "CP860（DOS 葡萄牙）", &["ibm860"], Group::Legacy, "DOS OEM：葡萄牙文", &tables::CP860),
    table_entry("cp861", "CP861（DOS 冰岛）", &["ibm861"], Group::Legacy, "DOS OEM：冰岛文", &tables::CP861),
    table_entry("cp862", "CP862（DOS 希伯来）", &["ibm862"], Group::Legacy, "DOS OEM：希伯来文", &tables::CP862),
    table_entry("cp863", "CP863（DOS 加拿大法语）", &["ibm863"], Group::Legacy, "DOS OEM：加拿大法语", &tables::CP863),
    table_entry("cp864", "CP864（DOS 阿拉伯）", &["ibm864"], Group::Legacy, "DOS OEM：阿拉伯文（IBM 变体）", &tables::CP864),
    table_entry("cp865", "CP865（DOS 北欧）", &["ibm865"], Group::Legacy, "DOS OEM：北欧", &tables::CP865),
    table_entry("cp869", "CP869（DOS 希腊）", &["ibm869"], Group::Legacy, "DOS OEM：希腊文（IBM）", &tables::CP869),
    // ---------- 传统 / 特殊：EBCDIC ----------
    table_entry("ebcdic-037", "EBCDIC 037（美国）", &["cp037", "ibm037"], Group::Legacy, "IBM 大型机：美国 / 加拿大", &tables::EBCDIC_037),
    table_entry("ebcdic-500", "EBCDIC 500（国际）", &["cp500", "ibm500"], Group::Legacy, "IBM 大型机：国际版", &tables::EBCDIC_500),
    table_entry("ebcdic-1047", "EBCDIC 1047（开放系统）", &["cp1047", "ibm1047"], Group::Legacy, "IBM 大型机：Unix 开放系统", &tables::EBCDIC_1047),
    // ---------- 传统 / 特殊：老 Mac ----------
    table_entry("mac-arabic", "MacArabic", &["macarabic"], Group::Western, "老 Mac 阿拉伯文", &tables::MAC_ARABIC),
    table_entry("mac-hebrew", "MacHebrew", &["machebrew"], Group::Western, "老 Mac 希伯来文", &tables::MAC_HEBREW),
    table_entry("mac-greek", "MacGreek", &["macgreek"], Group::Western, "老 Mac 希腊文", &tables::MAC_GREEK),
    table_entry("mac-romanian", "MacRomanian", &["macromanian"], Group::Western, "老 Mac 罗马尼亚文", &tables::MAC_ROMANIAN),
    table_entry("mac-ukrainian", "MacUkrainian", &["macukraine"], Group::Western, "老 Mac 乌克兰文", &tables::MAC_UKRAINIAN),
    table_entry("mac-thai", "MacThai", &["macthai"], Group::Western, "老 Mac 泰文", &tables::MAC_THAI),
    table_entry("mac-central-europe", "MacCentralEurope", &["maccentraleurope", "maccentraleuropean"], Group::Western, "老 Mac 中欧", &tables::MAC_CENTRAL_EUROPE),
    table_entry("mac-icelandic", "MacIcelandic", &["maciceland"], Group::Western, "老 Mac 冰岛文", &tables::MAC_ICELANDIC),
    table_entry("mac-turkish", "MacTurkish", &["macturkish"], Group::Western, "老 Mac 土耳其文", &tables::MAC_TURKISH),
    table_entry("mac-croatian", "MacCroatian", &["maccroatian"], Group::Western, "老 Mac 克罗地亚文", &tables::MAC_CROATIAN),
    // ---------- 传统 / 特殊：其它 ----------
    custom_entry("us-ascii", "US-ASCII", &["ascii"], Group::Legacy, "7 位，>0x7F 报错", custom::ASCII),
];

/// 按 id / 名称 / 别名查找（不区分大小写），也接受 IANA 标签。
pub fn find(query: &str) -> Option<&'static EncodingInfo> {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        return None;
    }
    if let Some(info) = LIST.iter().find(|e| {
        e.id == q
            || e.name.eq_ignore_ascii_case(&q)
            || e.aliases.iter().any(|a| a.eq_ignore_ascii_case(&q))
    }) {
        return Some(info);
    }
    ers::Encoding::for_label(q.as_bytes()).and_then(by_ers)
}

/// 根据 encoding_rs 的编码句柄反查注册表条目。
pub fn by_ers(enc: &'static ers::Encoding) -> Option<&'static EncodingInfo> {
    LIST.iter().find(|e| match e.codec {
        Codec::Ers(e_enc) => std::ptr::eq(e_enc, enc),
        Codec::Custom(_) | Codec::Table(_) => false,
    })
}

/// 按 BOM 嗅探出具体编码。
pub fn sniff_bom(bytes: &[u8]) -> Option<&'static EncodingInfo> {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return find("utf-8");
    }
    if bytes.starts_with(&[0xFF, 0xFE, 0x00, 0x00]) {
        return find("utf-32le");
    }
    if bytes.starts_with(&[0x00, 0x00, 0xFE, 0xFF]) {
        return find("utf-32be");
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return find("utf-16le");
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return find("utf-16be");
    }
    None
}

/// 解码。BOM 会在匹配编码时被去掉，并在结果里标注。
pub fn decode(info: &EncodingInfo, bytes: &[u8]) -> DecodeResult {
    match info.codec {
        Codec::Ers(enc) => decode_ers(enc, bytes),
        Codec::Custom(codec) => codec.decode(bytes),
        Codec::Table(table) => table_codec::decode_table(table, bytes),
    }
}

fn decode_ers(enc: &'static ers::Encoding, bytes: &[u8]) -> DecodeResult {
    let name = enc.name();
    let (payload, bom_removed) = if name == "UTF-8" && bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        (&bytes[3..], Some("UTF-8"))
    } else if name == "UTF-16LE" && bytes.starts_with(&[0xFF, 0xFE]) {
        (&bytes[2..], Some("UTF-16LE"))
    } else if name == "UTF-16BE" && bytes.starts_with(&[0xFE, 0xFF]) {
        (&bytes[2..], Some("UTF-16BE"))
    } else {
        (bytes, None)
    };

    let mut decoder = enc.new_decoder_without_bom_handling();
    // decode_to_string 只使用 String 的剩余容量；容量不够会提前返回 OutputFull，
    // 所以按最坏情况（单字节编码 → 最多 3 字节 UTF-8）预留，避免结果被截断。
    let mut text = String::with_capacity(payload.len().saturating_mul(4).max(16));
    let mut offsets = Vec::new();
    let mut pos = 0usize;
    let mut had_any_error = false;
    loop {
        let last = pos >= payload.len();
        let (_, read, had_errors) = decoder.decode_to_string(&payload[pos..], &mut text, last);
        if had_errors {
            had_any_error = true;
            if offsets.len() < 64 {
                offsets.push(pos);
            }
        }
        pos += read;
        if last || read == 0 {
            break;
        }
    }

    DecodeResult {
        text,
        had_errors: had_any_error,
        error_offsets: offsets,
        bom_removed,
    }
}

/// 严格编码：无法映射的字符会被跳过并记录（不写入替代字符）。
///
/// 注意：对有状态的编码（如 ISO-2022-JP）逐字符编码会产生冗余转义序列，但不影响解码结果。
pub fn encode(info: &EncodingInfo, text: &str) -> EncodeResult {
    match info.codec {
        Codec::Ers(enc) => encode_ers(enc, text),
        Codec::Custom(codec) => codec.encode(text),
        Codec::Table(table) => table_codec::encode_table(table, text),
    }
}

fn encode_ers(enc: &'static ers::Encoding, text: &str) -> EncodeResult {
    let mut bytes = Vec::with_capacity(text.len() * 2);
    let mut unmappable = Vec::new();
    let mut buf = [0u8; 4];
    for ch in text.chars() {
        let single = ch.encode_utf8(&mut buf);
        let (cow, _, had_errors) = enc.encode(single);
        if had_errors {
            unmappable.push(ch);
        } else {
            bytes.extend_from_slice(&cow);
        }
    }
    EncodeResult { bytes, unmappable }
}

/// 宽松编码：整体编码，无法映射的字符按 WHATWG 规则写成数字字符引用。
/// 批量转码大文件时比逐字符快得多。
pub fn encode_lenient(info: &EncodingInfo, text: &str) -> Vec<u8> {
    match info.codec {
        Codec::Ers(enc) => {
            let (cow, _, _) = enc.encode(text);
            cow.into_owned()
        }
        Codec::Custom(codec) => {
            let mut out = codec.encode(text).bytes;
            // 自定义编码没有“数字引用”兜底，用 U+FFFD 的思路不适用，直接返回可映射部分
            out.shrink_to_fit();
            out
        }
        Codec::Table(table) => table_codec::encode_table(table, text).bytes,
    }
}

/// 把字节按 `info` 解码后，检测是否出现替换字符（用于探测打分）。
pub fn decodes_cleanly(info: &EncodingInfo, bytes: &[u8]) -> bool {
    !decode(info, bytes).had_errors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_by_alias() {
        assert_eq!(find("gb2312").unwrap().id, "gbk");
        assert_eq!(find("CP936").unwrap().id, "gbk");
        assert_eq!(find("latin1").unwrap().id, "iso-8859-1");
        assert_eq!(find("cp1252").unwrap().id, "windows-1252");
        assert_eq!(find("UTF8").unwrap().id, "utf-8");
        assert!(find("不存在的编码").is_none());
    }

    #[test]
    fn table_codecs_are_registered() {
        let cp437 = find("ibm437").expect("CP437 应已注册");
        assert_eq!(decode(cp437, &[0x82]).text, "é");
        assert_eq!(encode(cp437, "é").bytes, vec![0x82]);

        let ebcdic = find("cp037").expect("EBCDIC 037 应已注册");
        assert_eq!(decode(ebcdic, &[0xC1, 0xC2, 0xC3]).text, "ABC");

        let mac = find("macthai").expect("MacThai 应已注册");
        assert_eq!(mac.id, "mac-thai");

        // 表驱动编码都能往返自己声明的字节
        for id in ["cp850", "cp852", "cp866", "ebcdic-1047", "mac-turkish"] {
            let info = find(id).unwrap_or_else(|| panic!("缺少编码 {id}"));
            let text = decode(info, &(0u8..=255).collect::<Vec<u8>>()).text;
            let encoded = encode(info, &text);
            assert!(encoded.unmappable.is_empty(), "{id} 反查表不完整");
            assert_eq!(decode(info, &encoded.bytes).text, text, "{id} 往返失败");
        }
    }

    #[test]
    fn gbk_roundtrip() {
        let gbk = find("gbk").unwrap();
        let encoded = encode(gbk, "中文测试");
        assert!(encoded.unmappable.is_empty());
        assert_eq!(encoded.bytes, vec![0xD6, 0xD0, 0xCE, 0xC4, 0xB2, 0xE2, 0xCA, 0xD4]);
        assert_eq!(decode(gbk, &encoded.bytes).text, "中文测试");
    }

    #[test]
    fn utf8_bom_is_stripped() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice("hi".as_bytes());
        let r = decode(find("utf-8").unwrap(), &bytes);
        assert_eq!(r.text, "hi");
        assert_eq!(r.bom_removed, Some("UTF-8"));
    }

    #[test]
    fn invalid_utf8_reports_errors() {
        let r = decode(find("utf-8").unwrap(), &[0xFF, 0xFE, 0x41]);
        assert!(r.had_errors);
        assert!(!r.error_offsets.is_empty());
    }

    #[test]
    fn unmappable_chars_are_reported() {
        let gbk = find("gbk").unwrap();
        let r = encode(gbk, "😀中");
        assert_eq!(r.unmappable, vec!['😀']);
        assert_eq!(r.bytes, vec![0xD6, 0xD0]);
    }
}
