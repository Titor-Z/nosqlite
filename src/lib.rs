//! nosqlite —— 一个像 SQLite 一样小巧的单文件 NoSQL 数据库。
//!
//! - 文件后缀固定为 `.nodb`,任何平台、任何场景都不接受其他后缀或无后缀。
//! - 存储保持嵌入式单文件,可直接复制粘贴迁移。
//! - 访问形态:CLI、Rust 内嵌 API(本 crate)、后期 Server 模式。
//!
//! # 模块结构
//!
//! - [`engine`]:存储引擎(单文件 `.nodb`,策略为"先现成后自研",通过 [`Storage`] trait 抽象)。
//! - [`document`]:文档模型(`Document`)与 ObjectId 主键生成。
//! - [`query`]:查询语义(filter 解析/匹配、sort、limit/skip)。
//! - [`database`]:对外门面 [`Database`],提供 `open/insert/find` 等文档级 API。
//! - [`cli`]:类 sqlite3 的命令行工具。
//!
//! [`Storage`]: engine::Storage

pub mod cli;
pub mod database;
pub mod document;
pub mod engine;
pub mod error;
pub mod query;

pub use database::{Database, DatabaseError, UpsertResult};
pub use document::{Document, DocumentError};
pub use error::ErrorCode;
