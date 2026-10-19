//! 存储引擎模块。
//!
//! 策略:**先现成后自研** —— 先用 redb(纯 Rust 单文件 B+树引擎,带 ACID 事务)
//! 作为底层存储,通过 [`Storage`] trait 抽象出统一接口,
//! 后期如需替换为自研文件格式,只需新增一个实现即可,调用方无感知。
//!
//! # M1 范围
//!
//! 实现 `.nodb` 数据库的创建与打开:
//! - 强制校验文件后缀为 `.nodb`,其他后缀或无后缀一律拒绝。
//! - 打开时校验魔数串与格式版本,确保文件确实是 nosqlite 创建。

pub mod redb_engine;
pub mod scan;
pub mod storage;

pub use redb_engine::RedbEngine;
pub use scan::RedbScanSession;
pub use storage::{ScanSession, Storage, StorageError};
