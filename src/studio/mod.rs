//! Web 管理台的静态服务端。
//!
//! 本模块不依赖任何前端构建工具:前端资源(WASM/HTML/CSS/JS)由二进制
//! `nodblite-studio` 通过 `include_bytes!` 内嵌,启动后以纯 `std` 的 TCP
//! 服务提供。数据库操作全部发生在浏览器内的 WASM 里,服务端不碰 `.nodb`。
//!
//! 因此分发形态是**单个可执行文件**:无运行时依赖、无资源目录。

pub mod serve;
