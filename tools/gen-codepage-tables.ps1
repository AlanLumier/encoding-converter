# 生成 src/core/charset/tables.rs：用 Windows 自带的代码页数据（.NET）导出
# 单字节代码页的“字节 → Unicode 码点”映射表。
#
# 用法（仓库根目录）：
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools/gen-codepage-tables.ps1
#
# 说明：只在需要新增/更新代码页时运行；生成结果是纯 Rust 常量，运行时不依赖 .NET。

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$outPath = Join-Path $root 'src/core/charset/tables.rs'

# 需要导出的代码页：DOS OEM、EBCDIC、老 Mac
$codePages = @(
    437, 720, 737, 775, 850, 852, 855, 857, 858,
    860, 861, 862, 863, 864, 865, 866, 869,
    37, 500, 1047,
    10000, 10004, 10005, 10006, 10007, 10010, 10017, 10021, 10029, 10079, 10081, 10082
)

$names = @{
    437 = 'CP437'; 720 = 'CP720'; 737 = 'CP737'; 775 = 'CP775'; 850 = 'CP850'
    852 = 'CP852'; 855 = 'CP855'; 857 = 'CP857'; 858 = 'CP858'; 860 = 'CP860'
    861 = 'CP861'; 862 = 'CP862'; 863 = 'CP863'; 864 = 'CP864'; 865 = 'CP865'
    866 = 'CP866'; 869 = 'CP869'; 37 = 'EBCDIC_037'; 500 = 'EBCDIC_500'
    1047 = 'EBCDIC_1047'; 10000 = 'MAC_ROMAN'; 10004 = 'MAC_ARABIC'
    10005 = 'MAC_HEBREW'; 10006 = 'MAC_GREEK'; 10007 = 'MAC_CYRILLIC'
    10010 = 'MAC_ROMANIAN'; 10017 = 'MAC_UKRAINIAN'; 10021 = 'MAC_THAI'
    10029 = 'MAC_CENTRAL_EUROPE'; 10079 = 'MAC_ICELANDIC'; 10081 = 'MAC_TURKISH'
    10082 = 'MAC_CROATIAN'
}

$sb = New-Object System.Text.StringBuilder
[void]$sb.AppendLine('//! 单字节代码页映射表：由 `tools/gen-codepage-tables.ps1` 自动生成，请勿手工编辑。')
[void]$sb.AppendLine('//!')
[void]$sb.AppendLine('//! 每张表 256 项，索引是字节值，值是对应的 Unicode 码点（0xFFFD 表示未定义）。')
[void]$sb.AppendLine()

foreach ($cp in $codePages) {
    $encoding = [System.Text.Encoding]::GetEncoding($cp)
    $text = $encoding.GetString([byte[]](0..255))
    if ($text.Length -ne 256) {
        throw "代码页 $cp 不是单字节编码（解码得到 $($text.Length) 个字符）"
    }
    $name = $names[$cp]
    [void]$sb.AppendLine("/// $name（代码页 $cp）")
    [void]$sb.AppendLine("pub static ${name}: [u16; 256] = [")
    for ($i = 0; $i -lt 256; $i += 8) {
        $values = @()
        for ($j = 0; $j -lt 8; $j++) {
            $values += ('0x{0:X4}' -f [int]$text[$i + $j])
        }
        [void]$sb.AppendLine('    ' + ($values -join ', ') + ',')
    }
    [void]$sb.AppendLine('];')
    [void]$sb.AppendLine()
}

[System.IO.File]::WriteAllText($outPath, $sb.ToString(), (New-Object System.Text.UTF8Encoding($false)))
Write-Output "已生成 $outPath（$($codePages.Count) 张表）"
