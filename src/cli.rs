//! 类 sqlite3 的命令行工具。
//!
//! M1 提供两个子命令:
//! - `nosqlite create <file.nodb>`:创建新数据库;
//! - `nosqlite open   <file.nodb>`:打开并校验已有数据库。
//!
//! 后续里程碑会扩展 insert/find/update/delete 等完整命令。

use std::path::Path;
use std::process::ExitCode;

use crate::engine::{RedbEngine, Storage};

/// 命令行子命令。
enum Command {
    /// 创建新数据库(目标文件必须不存在)。
    Create(String),
    /// 打开并校验已有数据库。
    Open(String),
    /// 显示帮助信息。
    Help,
}

/// CLI 程序入口:解析参数 → 执行子命令 → 返回退出码。
pub fn run() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match parse(&args) {
        Ok(cmd) => execute(cmd),
        Err(msg) => {
            // 参数解析失败:打印错误并提示帮助
            eprintln!("错误: {}", msg);
            eprintln!("输入 `nosqlite help` 查看用法。");
            ExitCode::FAILURE
        }
    }
}

/// 解析命令行参数为 [`Command`]。
fn parse(args: &[String]) -> Result<Command, String> {
    match args {
        [] => Ok(Command::Help),
        [flag] if flag == "-h" || flag == "--help" || flag == "help" => Ok(Command::Help),
        [sub, path] if sub == "create" => Ok(Command::Create(path.clone())),
        [sub, path] if sub == "open" => Ok(Command::Open(path.clone())),
        [sub] => Err(format!("子命令 `{}` 需要一个 <file.nodb> 路径参数", sub)),
        [unknown, ..] => Err(format!("未知子命令或参数过多: `{}`", unknown)),
    }
}

/// 执行解析出的子命令。
fn execute(cmd: Command) -> ExitCode {
    match cmd {
        Command::Create(path) => {
            let engine = RedbEngine::create(Path::new(&path));
            report(&path, "创建", engine)
        }
        Command::Open(path) => {
            let engine = RedbEngine::open(Path::new(&path));
            report(&path, "打开", engine)
        }
        Command::Help => {
            print_help();
            ExitCode::SUCCESS
        }
    }
}

/// 统一处理引擎操作结果:成功打印中文提示,失败打印错误。
fn report(path: &str, action: &str, result: Result<RedbEngine, crate::engine::StorageError>) -> ExitCode {
    match result {
        Ok(engine) => {
            // 输出成功信息(中文,符合项目规范)
            println!("已{}数据库: {}", action, path);
            println!("格式版本: 1,文件后缀: .nodb,单文件可直接复制粘贴迁移。");
            let _ = engine.close();
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{}失败: {}", action, e);
            ExitCode::FAILURE
        }
    }
}

/// 打印帮助信息。
fn print_help() {
    println!("nosqlite —— 像 SQLite 一样小巧的单文件 NoSQL 数据库");
    println!();
    println!("用法:");
    println!("  nosqlite create <file.nodb>   创建新数据库");
    println!("  nosqlite open   <file.nodb>   打开并校验已有数据库");
    println!("  nosqlite help                  显示本帮助");
    println!();
    println!("说明:");
    println!("  - 文件后缀必须为 .nodb,不支持其他后缀或无后缀");
    println!("  - 数据库为单文件,可直接复制粘贴迁移");
}
