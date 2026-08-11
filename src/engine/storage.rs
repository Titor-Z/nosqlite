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
}
