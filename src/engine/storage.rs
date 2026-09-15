//! 存储引擎的统一抽象接口与错误类型。
//!
//! 通过 [`Storage`] trait 隔离底层引擎细节(当前为 redb,后期可替换自研),
//! 调用方只面向本 trait 编程,做到高内聚低耦合。

use std::fmt;
use std::path::Path;

/// 存储引擎统一错误类型。
///
/// 所有错误消息均为中文,便于 CLI 直接展示给用户。
#[derive(Debug)]
pub enum StorageError {
    /// 文件路径非法(后缀不是 `.nodb` 或没有后缀)。
    InvalidPath(String),
    /// 底层 IO 错误。
    Io(std::io::Error),
    /// 文件不是有效的 nosqlite 数据库(不存在/非 nosqlite 创建/魔数不匹配)。
    InvalidFormat(String),
    /// 数据库格式版本不兼容(由更高/更低版本创建)。
    VersionMismatch {
        /// 文件中记录的版本号。
        found: u32,
        /// 当前引擎支持的版本号。
        expected: u32,
    },
    /// 集合名非法(为空或包含分隔符 `\0`)。
    InvalidCollection(String),
    /// 主键冲突:集合中已存在相同 `_id`。
    DuplicateKey { collection: String, id: String },
    /// 其他未知错误。
    Other(String),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StorageError::InvalidPath(msg) => write!(f, "{}", msg),
            StorageError::Io(e) => write!(f, "IO 错误: {}", e),
            StorageError::InvalidFormat(msg) => write!(f, "{}", msg),
            StorageError::VersionMismatch { found, expected } => {
                if *found > *expected {
                    write!(
                        f,
                        "数据库格式版本({})高于当前引擎支持的版本({}),请升级 nosqlite",
                        found, expected
                    )
                } else {
                    write!(
                        f,
                        "数据库格式版本({})过低,当前引擎要求版本 {},请先升级数据库",
                        found, expected
                    )
                }
            }
            StorageError::InvalidCollection(msg) => write!(f, "{}", msg),
            StorageError::DuplicateKey { collection, id } => write!(
                f,
                "主键冲突:集合 `{}` 中已存在 _id = `{}`",
                collection, id
            ),
            StorageError::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for StorageError {}

impl From<std::io::Error> for StorageError {
    fn from(e: std::io::Error) -> Self {
        StorageError::Io(e)
    }
}

/// 存储引擎抽象接口。
///
/// 后期自研引擎只需实现本 trait,即可无缝替换 redb。
pub trait Storage {
    /// 创建一个新的 `.nodb` 数据库文件。
    fn create(path: &Path) -> Result<Self, StorageError>
    where
        Self: Sized;
    /// 打开一个已存在的 `.nodb` 数据库文件,并校验其魔数与版本。
    fn open(path: &Path) -> Result<Self, StorageError>
    where
        Self: Sized;
    /// 关闭数据库,释放底层资源。
    fn close(&self) -> Result<(), StorageError>;

    /// 向指定集合插入一条文档。
    ///
    /// 文档以序列化后的字节形式传入,主键 `doc_id` 由调用方决定(可为自动生成的
    /// ObjectId)。返回 [`StorageError::DuplicateKey`] 表示主键冲突。
    fn insert(&self, collection: &str, doc_id: &str, doc_bytes: &[u8])
        -> Result<(), StorageError>;

    /// 扫描指定集合内的全部文档,返回 `(doc_id, doc_bytes)` 列表。
    ///
    /// 结果按 doc_id 有序(利用 B+树主键索引的顺序性)。
    ///
    /// ⚠️ **此方法会把整个集合一次性载入内存**,仅适用于小集合或一次性工具。
    /// M11 起引擎内部查询已改用 [`Storage::for_each`] 流式遍历,集合越大内存峰值越高。
    /// 默认实现基于 [`Storage::for_each`](Self::for_each) 收集,子类可覆写。
    fn scan(&self, collection: &str) -> Result<Vec<(String, Vec<u8>)>, StorageError> {
        let mut out = Vec::new();
        self.for_each(collection, &mut |doc_id, bytes| {
            out.push((doc_id.to_string(), bytes.to_vec()));
            true
        })?;
        Ok(out)
    }

    /// **流式**遍历指定集合内的全部文档(M11 新增,内存 O(1))。
    ///
    /// 结果按 doc_id 有序(B+树主键索引顺序)。对每条文档调用 `visit(doc_id, doc_bytes)`,
    /// **回调返回 `false` 即提前终止**(不再读取后续文档)。
    ///
    /// 这是消除「全量物化」内存峰值的关键:调用方在回调内完成过滤/聚合,
    /// 只保留命中项,其余文档立即释放,内存不再随集合大小线性增长。
    fn for_each(
        &self,
        collection: &str,
        visit: &mut dyn FnMut(&str, &[u8]) -> bool,
    ) -> Result<(), StorageError>;

    /// 单事务批量插入(M11 新增,一次提交摊薄 fsync 成本)。
    ///
    /// 所有文档在**同一个写事务**内写入并一次性提交,写入吞吐可提升 1–2 个数量级。
    /// 任一主键冲突则整个批次失败(报 [`StorageError::DuplicateKey`]),已写入的内容随事务一并回滚。
    fn insert_many(&self, collection: &str, docs: &[(String, Vec<u8>)])
        -> Result<(), StorageError>;

    /// 按主键读取单条文档(M11 新增,O(log n),不经全表扫描)。
    ///
    /// 返回 `None` 表示该主键不存在。仅返回文档字节,由上层负责反序列化。
    fn get(&self, collection: &str, doc_id: &str) -> Result<Option<Vec<u8>>, StorageError>;

    /// 覆盖更新指定主键的文档(主键不变,仅替换内容)。
    ///
    /// 返回是否实际更新(主键不存在则返回 `false`,不报错)。
    fn update(
        &self,
        collection: &str,
        doc_id: &str,
        doc_bytes: &[u8],
    ) -> Result<bool, StorageError>;

    /// 删除指定主键的文档,返回是否实际删除。
    fn delete(&self, collection: &str, doc_id: &str) -> Result<bool, StorageError>;

    /// 列出数据库内全部集合名(去重,按集合名有序)。
    ///
    /// 利用 key = `集合名(0x00)doc_id` 的有序性,遍历时按分隔符提取集合名。
    /// 空库(文档表不存在)返回空列表,不报错。
    fn list_collections(&self) -> Result<Vec<String>, StorageError>;

    /// 删除整个集合及其全部文档(不可恢复),返回删除的文档条数。
    ///
    /// 集合不存在时返回 `0`,不报错。
    fn drop_collection(&self, collection: &str) -> Result<usize, StorageError>;
}
