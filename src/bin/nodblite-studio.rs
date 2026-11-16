//! `nodblite-studio` 入口:单二进制 Web 管理台。
//!
//! 用法:
//! ```text
//! nodblite-studio [--addr 127.0.0.1:8787]
//! ```
//!
//! 浏览器打开后导入/拖入 `.nodb` 即可管理。数据库操作全部在浏览器内的
//! WASM 里执行,服务端只提供内嵌的静态资源。

use std::process::ExitCode;

use nodblite::studio::serve::{Asset, serve};

const DEFAULT_ADDR: &str = "127.0.0.1:8787";

/// 内嵌资源表。
///
/// `nodblite.wasm` 由 `scripts/build-studio.sh` 先构建到 `studio-ui/public/` 再编译本目标,
/// 所以这里能用 `include_bytes!` 把整个数据库引擎装进可执行文件。
static ASSETS: &[Asset] = &[
    Asset {
        path: "/index.html",
        mime: "text/html; charset=utf-8",
        body: include_bytes!("../../studio-ui/dist/index.html"),
    },
    Asset {
        path: "/assets/app.js",
        mime: "text/javascript; charset=utf-8",
        body: include_bytes!("../../studio-ui/dist/assets/app.js"),
    },
    Asset {
        path: "/assets/index.css",
        mime: "text/css; charset=utf-8",
        body: include_bytes!("../../studio-ui/dist/assets/index.css"),
    },
    Asset {
        path: "/nodblite.wasm",
        mime: "application/wasm",
        body: include_bytes!("../../studio-ui/dist/nodblite.wasm"),
    },
];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut addr = DEFAULT_ADDR.to_string();
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
                eprintln!("未知参数: {}", other);
                print_help();
                return ExitCode::FAILURE;
            }
        }
        i += 1;
    }

    match serve(&addr, ASSETS) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("启动失败: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn print_help() {
    println!("nodblite-studio —— 单二进制 Web 管理台");
    println!();
    println!("用法:");
    println!("  nodblite-studio [--addr <host:port>]");
    println!();
    println!("说明:");
    println!("  - 默认监听 {};启动后用浏览器打开该地址", DEFAULT_ADDR);
    println!("  - 在页面上导入或拖入 .nodb 文件;数据库操作在浏览器内的 WASM 中执行");
    println!("  - 服务端只提供内嵌静态资源,不接触任何数据库文件");
}
