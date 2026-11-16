//! 极简静态资源服务。
//!
//! 只做三件事:读请求行、按路径取内嵌资源、回 `200`/`404`。
//! 不实现 keep-alive、range、压缩 —— 这是本地开发/自用工具,不是通用 Web 服务器。

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

/// 一个内嵌资源。
pub struct Asset {
    /// 请求路径,如 `/app.js`(不含查询串)。
    pub path: &'static str,
    /// 响应 Content-Type。
    pub mime: &'static str,
    /// 响应体。
    pub body: &'static [u8],
}

/// 启动服务并阻塞监听。
///
/// `assets` 必须以 `'static` 传入(通常是二进制里的 `include_bytes!` 常量)。
pub fn serve(addr: &str, assets: &'static [Asset]) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr)?;
    serve_listener(listener, assets)
}

/// 在已绑定的监听器上提供服务(测试用端口 0 时便于拿到实际地址)。
pub fn serve_listener(listener: TcpListener, assets: &'static [Asset]) -> std::io::Result<()> {
    let addr = listener.local_addr()?;
    println!("nodblite studio 已启动: http://{}", addr);
    println!("按 Ctrl+C 退出");
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                // 单机管理台并发很低,一连接一线程即可。
                std::thread::spawn(move || {
                    let _ = handle(stream, assets);
                });
            }
            Err(e) => eprintln!("连接失败: {}", e),
        }
    }
    Ok(())
}

fn handle(mut stream: TcpStream, assets: &'static [Asset]) -> std::io::Result<()> {
    stream.set_read_timeout(Some(std::time::Duration::from_secs(10)))?;
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf)?;
    let request = String::from_utf8_lossy(&buf[..n]);

    let mut parts = request.lines().next().unwrap_or("").split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");

    if method != "GET" && method != "HEAD" {
        return write_response(
            &mut stream,
            405,
            "text/plain; charset=utf-8",
            b"Method Not Allowed",
            method == "HEAD",
        );
    }

    // 去掉查询串;`/` 映射到首页。
    let path = target.split('?').next().unwrap_or("/");
    let path = if path == "/" { "/index.html" } else { path };

    match assets.iter().find(|a| a.path == path) {
        Some(asset) => write_response(&mut stream, 200, asset.mime, asset.body, method == "HEAD"),
        None => write_response(
            &mut stream,
            404,
            "text/plain; charset=utf-8",
            b"Not Found",
            method == "HEAD",
        ),
    }
}

fn write_response(
    stream: &mut TcpStream,
    status: u16,
    mime: &str,
    body: &[u8],
    head_only: bool,
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        405 => "Method Not Allowed",
        _ => "Not Found",
    };
    let header = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        status,
        reason,
        mime,
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    if !head_only {
        stream.write_all(body)?;
    }
    stream.flush()
}
