//! 文档模型模块(M2)。
//!
//! 文档顶层为 JSON 对象,支持嵌套任意 [`serde_json::Value`]。
//! 主键 `_id`:用户可显式提供;未提供时自动生成 12 字节 ObjectId 风格 ID。
//!
//! # 主键规则(与 MongoDB 对齐)
//!
//! - `_id` 由 [`ObjectId::generate`] 生成,12 字节 = 4 字节时间戳 + 5 字节随机 + 3 字节计数器,
//!   hex 编码为 24 字符小写字符串,如 `66f1a2b3c4d5e6f7a8b9c0d1`。

mod object_id;

pub use object_id::ObjectId;

use serde_json::{Map, Value};

/// 一条文档:顶层为 JSON 对象。
///
/// 内部用 `serde_json::Map<String, Value>` 表示,保证顶层是对象而非数组/标量,
/// 符合 MongoDB 文档模型(支持嵌套任意 JSON Value)。
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    /// 文档内部 JSON 对象。
    inner: Map<String, Value>,
}

impl Document {
    /// 创建一条空文档(内部为空的 JSON 对象)。
    pub fn new() -> Self {
        Document { inner: Map::new() }
    }

    /// 从 JSON 字符串创建文档。
    ///
    /// 要求顶层必须是 JSON 对象,否则返回 [`DocumentError::NotAnObject`]。
    pub fn from_json(json: &str) -> Result<Self, DocumentError> {
        let value: Value = serde_json::from_str(json).map_err(DocumentError::Json)?;
        Self::from_value(value)
    }

    /// 从已解析的 JSON 值创建文档(避免「先序列化再解析」的往返)。
    ///
    /// 要求顶层必须是 JSON 对象,否则返回 [`DocumentError::NotAnObject`]。
    pub fn from_value(value: Value) -> Result<Self, DocumentError> {
        match value {
            Value::Object(map) => Ok(Document { inner: map }),
            _ => Err(DocumentError::NotAnObject),
        }
    }

    /// 从字节反序列化文档(与 [`Document::to_bytes`] 互逆)。
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DocumentError> {
        let value: Value = serde_json::from_slice(bytes).map_err(DocumentError::Json)?;
        Self::from_value(value)
    }

    /// 序列化为 JSON 字节(用作存储引擎的 value)。
    pub fn to_bytes(&self) -> Result<Vec<u8>, DocumentError> {
        serde_json::to_vec(&self.inner).map_err(DocumentError::Json)
    }

    /// 获取字段值。
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.inner.get(key)
    }

    /// 插入字段。
    pub fn insert(&mut self, key: impl Into<String>, value: Value) {
        self.inner.insert(key.into(), value);
    }

    /// 获取主键 `_id`(字符串形式)。
    ///
    /// 若 `_id` 不存在则返回 `None`;若存在但不是字符串,返回 [`DocumentError::IdNotString`]。
    pub fn get_id(&self) -> Result<Option<String>, DocumentError> {
        match self.inner.get("_id") {
            None => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(_) => Err(DocumentError::IdNotString),
        }
    }

    /// 获取主键 `_id`,若不存在则自动生成 ObjectId 并写入文档,返回生成的 ID。
    pub fn ensure_id(&mut self) -> Result<String, DocumentError> {
        match self.get_id()? {
            Some(id) => Ok(id),
            None => {
                let id = ObjectId::generate();
                self.inner
                    .insert("_id".to_string(), Value::String(id.clone()));
                Ok(id)
            }
        }
    }

    /// 获取底层 JSON 对象(只读)。
    pub fn as_map(&self) -> &Map<String, Value> {
        &self.inner
    }

    /// 取文档内部对象的可变引用(供 upsert 在插入前应用更新操作)。
    pub fn as_map_mut(&mut self) -> &mut Map<String, Value> {
        &mut self.inner
    }
}

impl Default for Document {
    fn default() -> Self {
        Document::new()
    }
}

/// 文档模型错误。
#[derive(Debug)]
pub enum DocumentError {
    /// JSON 解析或序列化失败。
    Json(serde_json::Error),
    /// 顶层不是 JSON 对象。
    NotAnObject,
    /// `_id` 存在但不是字符串。
    IdNotString,
}

impl std::fmt::Display for DocumentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DocumentError::Json(e) => write!(f, "JSON 错误: {}", e),
            DocumentError::NotAnObject => write!(f, "文档顶层必须是 JSON 对象"),
            DocumentError::IdNotString => write!(f, "主键 _id 必须是字符串"),
        }
    }
}

impl std::error::Error for DocumentError {}
