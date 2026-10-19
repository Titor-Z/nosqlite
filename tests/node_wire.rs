//! Node.js 官方 `mongodb` 驱动验收(v0.6.2 M30)。
//!
//! 与 `tests/mongo_wire.rs` 的 Go 验收对齐:拉起 `nodblite-server`,再用
//! **官方 Node 驱动**走完整 wire 协议栈。
//!
//! 若本机没有 Node 或依赖未安装,测试**显式跳过并打印原因**,不静默通过。

use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn temp_path(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "nodblite_node_{}_{}_{}.nodb",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&path);
    path
}

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("应能绑定临时端口");
    listener.local_addr().expect("应能取得端口").port()
}

struct ServerGuard {
    child: Child,
    addr: String,
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn start_server(db_path: &std::path::Path) -> Option<ServerGuard> {
    let exe = env!("CARGO_BIN_EXE_nodblite-server");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);

    let child = Command::new(exe)
        .arg("--addr")
        .arg(&addr)
        .arg(db_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("应能启动 nodblite-server");

    let guard = ServerGuard { child, addr };
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if TcpStream::connect(&guard.addr).is_ok() {
            return Some(guard);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    None
}

/// Node 与 `mongodb` 依赖是否就绪。
fn node_available(dir: &std::path::Path) -> Result<(), String> {
    let node_ok = Command::new("node")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !node_ok {
        return Err("本机没有 node".to_string());
    }
    if !dir.join("node_modules/mongodb/package.json").exists() {
        return Err(format!(
            "依赖未安装,请在 {} 执行: npm install --cache ./.npm-cache",
            dir.display()
        ));
    }
    Ok(())
}

/// 用 Node 官方驱动跑完整验收。
#[test]
fn node_driver_end_to_end() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/node_compat");
    if let Err(why) = node_available(&dir) {
        eprintln!("跳过 Node 驱动验收: {}", why);
        return;
    }

    let db = temp_path("e2e");
    let Some(server) = start_server(&db) else {
        panic!("nodblite-server 未能在 5 秒内就绪");
    };

    let out = Command::new("node")
        .args(["--test", "compat.test.mjs"])
        .current_dir(&dir)
        .env("NOSQLITE_URI", format!("mongodb://{}", server.addr))
        .output()
        .expect("应能执行 node");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "Node 驱动验收失败\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );

    // 防止「一个用例都没跑」也算通过
    assert!(
        stdout.contains("# pass") || stdout.contains("pass "),
        "未看到测试统计输出,可能未真正执行:\n{}",
        stdout
    );

    drop(server);
    let _ = std::fs::remove_file(&db);
}
