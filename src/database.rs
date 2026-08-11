//! 对外门面:`Database` —— Rust 内嵌 API 的入口。
//!
//! 对底层 [`Storage`] 引擎做文档级封装,提供与 SQLite 类似的简单 API:
//!
//! - [`Database::create`] / [`Database::open`]:创建/打开 `.nodb` 数据库。
//! - [`Database::insert`]:向集合插入一条文档(自动生成或沿用 `_id`)。
//!
//! 后续里程碑将扩展 `find/update/delete` 等操作。

use std::path::Path;

use crate::document::{Document, DocumentError};
use crate::engine::{RedbEngine, Storage, StorageError};

/// nosqlite 数据库门面。
///
/// 持有具体存储引擎,对外暴露文档级操作。
#[derive(Debug)]
pub struct Database {
    /// 底层存储引擎(当前为 redb 实现)。
    engine: RedbEngine,
    /// 数据库文件路径(用于错误信息展示)。
    path: String,
}

/// 门面层统一错误:融合引擎错误与文档模型错误。
#[derive(Debug)]
pub enum DatabaseError {
    /// 存储引擎错误(见 [`StorageError`])。
    Storage(StorageError),
    /// 文档模型错误(见 [`DocumentError`])。
    Document(DocumentError),
}

impl std::fmt::Display for DatabaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DatabaseError::Storage(e) => write!(f, "{}", e),
            DatabaseError::Document(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for DatabaseError {}

impl From<StorageError> for DatabaseError {
    fn from(e: StorageError) -> Self {
        DatabaseError::Storage(e)
    }
}

impl From<DocumentError> for DatabaseError {
    fn from(e: DocumentError) -> Self {
        DatabaseError::Document(e)
    }
}

impl Database {
    /// 创建新的 `.nodb` 数据库文件(目标文件必须不存在)。
    pub fn create(path: &Path) -> Result<Self, DatabaseError> {
        let engine = RedbEngine::create(path)?;
        Ok(Database {
            engine,
            path: path.display().to_string(),
        })
    }

    /// 打开已存在的 `.nodb` 数据库文件,并校验魔数与版本。
    pub fn open(path: &Path) -> Result<Self, DatabaseError> {
        let engine = RedbEngine::open(path)?;
        Ok(Database {
            engine,
            path: path.display().to_string(),
        })
    }

    /// 向指定集合插入一条文档,返回其主键 `_id`。
    ///
    /// - 文档顶层必须是 JSON 对象;
    /// - `_id` 缺省时自动生成 24 字符 ObjectId,并回填到文档中;
    /// - `_id` 重复时返回 [`DatabaseError::Storage`](`StorageError::DuplicateKey`) 主键冲突。
    pub fn insert(&self, collection: &str, document: &mut Document) -> Result<String, DatabaseError> {
        // 1. 确保主键:缺省则自动生成并写入文档
        let doc_id = document.ensure_id()?;
        // 2. 序列化文档为 JSON 字节
        let bytes = document.to_bytes()?;
        // 3. 交给引擎插入(重复主键由引擎报冲突)
        self.engine.insert(collection, &doc_id, &bytes)?;
        Ok(doc_id)
    }

    /// 关闭数据库(redb 在 Drop 时自动落盘,此处保留接口便于后续扩展)。
    pub fn close(&self) -> Result<(), DatabaseError> {
        self.engine.close()?;
        Ok(())
    }

    /// 数据库文件路径。
    pub fn path(&self) -> &str {
        &self.path
    }
}
