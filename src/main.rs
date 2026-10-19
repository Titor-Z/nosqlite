//! nodblite 命令行程序入口。
//!
//! 仅做薄壳,实际逻辑位于 `nodblite::cli` 模块,便于测试复用。

use std::process::ExitCode;

use nodblite::cli;

/// 程序入口:将 CLI 的运行结果转为进程退出码。
fn main() -> ExitCode {
    cli::run()
}
