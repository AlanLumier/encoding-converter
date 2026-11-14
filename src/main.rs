//! encoding-converter：进制 / Base-N / 字符编码转换器。
//!
//! 无参数启动 GUI；带参数时进入 CLI 模式，详见 README。

fn main() -> anyhow::Result<()> {
    encoding_converter::run()
}
