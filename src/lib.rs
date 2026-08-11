//! nosqlite —— 一个像 SQLite 一样小巧的单文件 NoSQL 数据库。
//!
//! - 文件后缀固定为 `.nodb`,任何平台、任何场景都不接受其他后缀或无后缀。
//! - 存储保持嵌入式单文件,可直接复制粘贴迁移。
//! - 访问形态:CLI、Rust 内嵌 API(本 crate)、后期 Server 模式。
//!
//! # 模块结构
//!
//! - [`engine`]:存储引擎(单文件 `.nodb`,策略为"先现成后自研",通过 [`Storage`] trait 抽象)。
//! - [`cli`]:类 sqlite3 的命令行工具。
//!
//! 后续里程碑将新增 `document`(文档模型)与 `query`(查询语义)模块。
//!
//! [`Storage`]: engine::Storage

pub mod cli;
pub mod engine;
