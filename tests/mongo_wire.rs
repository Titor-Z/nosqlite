//! MongoDB wire 协议验收(阶段 2)。
//!
//! 这里不做「自己造客户端测自己」的自娱自乐 —— 而是**自动拉起 `nosqlite-server`,
//! 再用 MongoDB 官方 Go 驱动连接**,走完整协议栈。
//!
//! 若本机没有 Go 工具链或驱动未下载,测试会自动跳过(不阻塞 CI 的纯 Rust 阶段)。

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// 生成本次测试独占的临时路径。
fn temp_path(tag: &str, ext: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "nosqlite_wire_{}_{}_{}.{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        ext
    ));
    let _ = std::fs::remove_file(&path);
    path
}

/// 找一个空闲端口。
fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("应能绑定临时端口");
    listener.local_addr().expect("应能取得端口").port()
}

/// 服务端子进程守卫:测试结束(含 panic)时自动杀掉。
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

/// 启动 `nosqlite-server`,等待端口就绪。
fn start_server(db_path: &PathBuf) -> Option<ServerGuard> {
    let exe = env!("CARGO_BIN_EXE_nosqlite-server");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);

    let child = Command::new(exe)
        .arg("--addr")
        .arg(&addr)
        .arg(db_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("应能启动 nosqlite-server");

    let guard = ServerGuard { child, addr };

    // 等待端口可连接(最多 5 秒)
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if TcpStream::connect(&guard.addr).is_ok() {
            return Some(guard);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    None
}

/// 判断本机是否具备运行 Go 测试的条件。
fn go_available() -> bool {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/go_compat");
    if !dir.join("go.sum").exists() {
        return false;
    }
    Command::new("go")
        .arg("version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// 用 Go 官方驱动跑完整 CRUD 验收。
#[test]
fn go_driver_end_to_end() {
    if !go_available() {
        eprintln!("跳过:本机没有 Go 工具链或 go.sum 未就绪");
        return;
    }

    let db = temp_path("e2e", "nodb");
    let Some(server) = start_server(&db) else {
        panic!("nosqlite-server 未能在 5 秒内就绪");
    };

    let out = Command::new("go")
        .args(["test", "-count=1", "./..."])
        .current_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/go_compat"))
        .env("NOSQLITE_URI", format!("mongodb://{}", server.addr))
        .output()
        .expect("应能执行 go test");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "Go 驱动验收失败\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );

    drop(server);
    let _ = std::fs::remove_file(&db);
}

/// 协议层:裸手发一个 `hello` OP_MSG,校验响应能被解析且含 maxWireVersion。
///
/// 这条测试不依赖 Go,可在任何环境跑,防止握手字段被误删。
#[test]
fn raw_hello_handshake() {
    let db = temp_path("hello", "nodb");
    let Some(server) = start_server(&db) else {
        panic!("nosqlite-server 未能在 5 秒内就绪");
    };

    let mut stream = TcpStream::connect(&server.addr).expect("应能连接");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();

    // 构造 hello 命令的 OP_MSG:{hello: 1, $db: "admin"}
    let mut cmd = Vec::new();
    // 元素:hello -> int32 1
    cmd.push(0x10);
    cmd.extend_from_slice(b"hello\0");
    cmd.extend_from_slice(&1i32.to_le_bytes());
    // 元素:$db -> "admin"
    cmd.push(0x02);
    cmd.extend_from_slice(b"$db\0");
    cmd.extend_from_slice(&(6i32).to_le_bytes()); // "admin\0" 长度
    cmd.extend_from_slice(b"admin\0");
    cmd.push(0);
    let mut bson = Vec::new();
    bson.extend_from_slice(&((cmd.len() + 4) as i32).to_le_bytes());
    bson.extend_from_slice(&cmd);

    let body_len = 4 + 1 + bson.len();
    let total = 16 + body_len;
    let mut frame = Vec::with_capacity(total);
    frame.extend_from_slice(&(total as i32).to_le_bytes());
    frame.extend_from_slice(&7i32.to_le_bytes()); // requestID
    frame.extend_from_slice(&0i32.to_le_bytes()); // responseTo
    frame.extend_from_slice(&2013i32.to_le_bytes()); // OP_MSG
    frame.extend_from_slice(&0i32.to_le_bytes()); // flagBits
    frame.push(0); // section 0
    frame.extend_from_slice(&bson);

    stream.write_all(&frame).expect("应能写入");
    stream.flush().unwrap();

    let mut header = [0u8; 16];
    stream.read_exact(&mut header).expect("应能读到响应头");
    let len = i32::from_le_bytes(header[0..4].try_into().unwrap()) as usize;
    assert_eq!(
        i32::from_le_bytes(header[12..16].try_into().unwrap()),
        2013,
        "响应应为 OP_MSG"
    );
    assert_eq!(
        i32::from_le_bytes(header[8..12].try_into().unwrap()),
        7,
        "responseTo 应回填请求 ID"
    );

    let mut body = vec![0u8; len - 16];
    stream.read_exact(&mut body).expect("应能读到响应体");
    let text = String::from_utf8_lossy(&body);
    assert!(
        text.contains("maxWireVersion"),
        "hello 响应必须含 maxWireVersion,实际: {}",
        text
    );

    drop(server);
    let _ = std::fs::remove_file(&db);
}
