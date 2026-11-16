//! Web 管理台的静态资源服务测试。
//!
//! 需要 `studio` feature:
//! ```text
//! cargo test --features studio --test studio_http
//! ```

#![cfg(feature = "studio")]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

use nodblite::studio::serve::{Asset, serve_listener};

static ASSETS: &[Asset] = &[
    Asset {
        path: "/index.html",
        mime: "text/html; charset=utf-8",
        body: b"<html>studio</html>",
    },
    Asset {
        path: "/nodblite.wasm",
        mime: "application/wasm",
        body: b"\0asm",
    },
];

/// 启动一个随机端口的服务,返回地址。
fn start() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").expect("绑定端口失败");
    let addr = listener.local_addr().expect("读取地址失败");
    std::thread::spawn(move || {
        let _ = serve_listener(listener, ASSETS);
    });
    addr
}

fn request(addr: std::net::SocketAddr, raw: &str) -> String {
    let mut stream = TcpStream::connect(addr).expect("连接失败");
    stream.write_all(raw.as_bytes()).expect("发送失败");
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).expect("读取失败");
    String::from_utf8_lossy(&buf).to_string()
}

fn get(addr: std::net::SocketAddr, path: &str) -> String {
    request(addr, &format!("GET {} HTTP/1.1\r\nHost: x\r\n\r\n", path))
}

#[test]
fn serves_index_at_root() {
    let addr = start();
    let resp = get(addr, "/");
    assert!(resp.starts_with("HTTP/1.1 200 OK"), "{resp}");
    assert!(resp.contains("text/html; charset=utf-8"));
    assert!(resp.contains("Content-Length: 19"));
    assert!(resp.ends_with("<html>studio</html>"));
}

#[test]
fn serves_wasm_with_correct_mime() {
    let addr = start();
    let resp = get(addr, "/nodblite.wasm");
    assert!(resp.starts_with("HTTP/1.1 200 OK"), "{resp}");
    assert!(resp.contains("application/wasm"));
}

#[test]
fn unknown_path_is_404() {
    let addr = start();
    let resp = get(addr, "/missing.js");
    assert!(resp.starts_with("HTTP/1.1 404 Not Found"), "{resp}");
}

#[test]
fn head_has_no_body_and_post_is_rejected() {
    let addr = start();
    let head = request(addr, "HEAD / HTTP/1.1\r\nHost: x\r\n\r\n");
    assert!(head.starts_with("HTTP/1.1 200 OK"));
    assert!(head.ends_with("\r\n\r\n"));

    let post = request(addr, "POST / HTTP/1.1\r\nHost: x\r\n\r\n");
    assert!(
        post.starts_with("HTTP/1.1 405 Method Not Allowed"),
        "{post}"
    );
}

#[test]
fn query_string_is_ignored() {
    let addr = start();
    let resp = get(addr, "/?v=1");
    assert!(resp.starts_with("HTTP/1.1 200 OK"));
}
