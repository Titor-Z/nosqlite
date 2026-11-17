//! 账号认证验收(v0.7.0)。
//!
//! 只做一件事:**用 MongoDB 官方的 Go / Node 驱动,带账号密码连上来**。
//! 自己写的 SCRAM 对拍自己写的客户端,错了也测不出来 —— 必须换真实驱动。
//!
//! 依赖缺失时显式跳过并打印原因(不静默通过)。

use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use nodblite::Database;
use nodblite::auth::UserStore;

fn temp_path(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "nodblite_auth_{}_{}_{}.nodb",
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
    let l = std::net::TcpListener::bind("127.0.0.1:0").expect("应能绑定临时端口");
    l.local_addr().expect("应能取端口").port()
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

/// 建库 + 建账号,再启动服务(账号必须在服务起来之前写好)。
fn start_server_with_user(tag: &str, user: &str, pwd: &str) -> (ServerGuard, PathBuf) {
    let db_path = temp_path(tag);
    {
        let db = Database::create(&db_path).expect("建库应成功");
        UserStore::new(&db).create(user, pwd).expect("建账号应成功");
    }

    let exe = env!("CARGO_BIN_EXE_nodblite-server");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let child = Command::new(exe)
        .arg("--addr")
        .arg(&addr)
        .arg(&db_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("应能启动 nodblite-server");
    let guard = ServerGuard { child, addr };

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if TcpStream::connect(&guard.addr).is_ok() {
            return (guard, db_path);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("nodblite-server 未能在 5 秒内就绪");
}

fn go_available() -> bool {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/go_compat");
    dir.join("go.sum").exists()
        && Command::new("go")
            .arg("version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
}

fn node_available(dir: &std::path::Path) -> Result<(), String> {
    if !dir.join("node_modules").join("mongodb").join("package.json").exists() {
        return Err(format!(
            "缺少依赖 mongodb,请在 {} 执行: npm install --cache ./.npm-cache",
            dir.display()
        ));
    }
    Command::new("node")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .map_err(|e| format!("没有 node: {}", e))?;
    Ok(())
}

/// Go 官方驱动带账号密码验收(含错误密码、匿名连接被拒、createUser 全链路)。
#[test]
fn go_driver_authenticates() {
    if !go_available() {
        eprintln!("跳过 Go 认证验收:本机没有 Go 工具链或 go.sum 未就绪");
        return;
    }
    let (server, db_path) = start_server_with_user("go", "alice", "secret");
    let uri = format!(
        "mongodb://alice:secret@{}/?authSource=admin",
        server.addr
    );

    let out = Command::new("go")
        .args(["test", "-count=1", "-run", "^TestAuth", "./..."])
        .current_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/go_compat"))
        .env("NOSQLITE_AUTH_URI", &uri)
        .output()
        .expect("应能执行 go test");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "Go 认证验收失败\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains("ok"), "Go 输出应含 ok,实际:\n{}", stdout);

    drop(server);
    let _ = std::fs::remove_file(&db_path);
}

/// Node 官方驱动带账号密码验收。
#[test]
fn node_driver_authenticates() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/node_compat");
    if let Err(why) = node_available(&dir) {
        eprintln!("跳过 Node 认证验收: {}", why);
        return;
    }
    let (server, db_path) = start_server_with_user("node", "bob", "s3cret");
    let uri = format!("mongodb://bob:s3cret@{}/?authSource=admin", server.addr);

    let out = Command::new("node")
        .args(["--test", "auth.test.mjs"])
        .current_dir(&dir)
        .env("NOSQLITE_AUTH_URI", &uri)
        .output()
        .expect("应能执行 node --test");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "Node 认证验收失败\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );

    drop(server);
    let _ = std::fs::remove_file(&db_path);
}
