//! `nodblite-server` 入口:以 MongoDB wire 协议对外提供服务。
//!
//! 用法:
//! ```text
//! nodblite-server [--addr 127.0.0.1:27017] <file.nodb>
//! ```
//!
//! 启动后,任何 MongoDB 官方/社区驱动都可以用 `mongodb://127.0.0.1:27017`
//! 直接连接 —— 无需任何 nodblite 专属 SDK。

use std::process::ExitCode;

use nodblite::server::serve::{ServerConfig, serve};

/// 默认监听地址(MongoDB 标准端口)。
const DEFAULT_ADDR: &str = "127.0.0.1:27017";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut addr = DEFAULT_ADDR.to_string();
    let mut path: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" | "help" => {
                print_help();
                return ExitCode::SUCCESS;
            }
            "--addr" => {
                i += 1;
                match args.get(i) {
                    Some(a) => addr = a.clone(),
                    None => {
                        eprintln!("`--addr` 缺少参数");
                        return ExitCode::FAILURE;
                    }
                }
            }
            other if other.starts_with("--addr=") => {
                addr = other["--addr=".len()..].to_string();
            }
            other => {
                if path.is_some() {
                    eprintln!("参数过多: {}", other);
                    print_help();
                    return ExitCode::FAILURE;
                }
                path = Some(other.to_string());
            }
        }
        i += 1;
    }

    let Some(db_path) = path else {
        eprintln!("缺少数据库文件参数");
        print_help();
        return ExitCode::FAILURE;
    };
    if !db_path.to_ascii_lowercase().ends_with(".nodb") {
        eprintln!("数据库文件后缀必须是 .nodb: {}", db_path);
        return ExitCode::FAILURE;
    }

    match serve(ServerConfig { addr, db_path }) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("服务启动失败: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn print_help() {
    println!("nodblite-server —— 以 MongoDB wire 协议提供 .nodb 数据库");
    println!();
    println!("用法:");
    println!("  nodblite-server [--addr <host:port>] <file.nodb>");
    println!();
    println!("说明:");
    println!("  - 默认监听 {} (MongoDB 标准端口)", DEFAULT_ADDR);
    println!("  - 数据库文件不存在则自动创建;后缀必须是 .nodb");
    println!("  - 启动后可用任意 MongoDB driver 连接,如 Go:");
    println!(
        "      client, _ := mongo.Connect(options.Client().ApplyURI(\"mongodb://127.0.0.1:27017\"))"
    );
    println!();
    println!("示例:");
    println!("  nodblite-server app.nodb");
    println!("  nodblite-server --addr 0.0.0.0:27018 app.nodb");
}
