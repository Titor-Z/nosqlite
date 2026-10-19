//! 跨语言稳定错误码(v0.2.0 新增)。
//!
//! 面向两种跨语言出口:
//!
//! - **C ABI**(`nosqlite-ffi`):函数返回 `int` 错误码,`nosqlite_last_error()` 取详情;
//! - **wire 协议**(`nosqlite-server`):映射到 MongoDB 风格的 `code` + HTTP 状态码。
//!
//! 因此错误码必须**稳定、可程序化判断**,不能依赖中文文案(文案随时可能调整)。
//! 约定:全部为大写下划线字符串,一旦发布不得改名。

use crate::database::DatabaseError;

/// 跨语言稳定错误码。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    /// 主键冲突(集合内已存在相同 `_id`)。
    DuplicateKey,
    /// 文件路径非法(后缀不是 `.nodb`)。
    InvalidPath,
    /// 文件不是有效的 nosqlite 数据库 / 魔数不匹配。
    InvalidFormat,
    /// 数据库格式版本不兼容。
    VersionMismatch,
    /// 集合名非法。
    InvalidCollection,
    /// 过滤条件非法(无法解析或操作符使用错误)。
    InvalidFilter,
    /// 更新操作非法。
    InvalidUpdate,
    /// 排序规格非法。
    InvalidSort,
    /// 投影规格非法。
    InvalidProjection,
    /// 文档格式非法(非对象、`_id` 非字符串、JSON 损坏)。
    InvalidDocument,
    /// 底层 IO 错误。
    IoError,
    /// 其他未分类错误(内部错误)。
    Internal,
}

impl ErrorCode {
    /// 稳定的字符串形式(跨语言契约的一部分,不得随意改动)。
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::DuplicateKey => "DUPLICATE_KEY",
            ErrorCode::InvalidPath => "INVALID_PATH",
            ErrorCode::InvalidFormat => "INVALID_FORMAT",
            ErrorCode::VersionMismatch => "VERSION_MISMATCH",
            ErrorCode::InvalidCollection => "INVALID_COLLECTION",
            ErrorCode::InvalidFilter => "INVALID_FILTER",
            ErrorCode::InvalidUpdate => "INVALID_UPDATE",
            ErrorCode::InvalidSort => "INVALID_SORT",
            ErrorCode::InvalidProjection => "INVALID_PROJECTION",
            ErrorCode::InvalidDocument => "INVALID_DOCUMENT",
            ErrorCode::IoError => "IO_ERROR",
            ErrorCode::Internal => "INTERNAL",
        }
    }

    /// C ABI 用的整数码(0 保留给成功)。
    ///
    /// 取值为固定序号,**已发布后不得重排**(否则 ABI 破坏)。
    pub fn as_i32(self) -> i32 {
        match self {
            ErrorCode::DuplicateKey => 1,
            ErrorCode::InvalidPath => 2,
            ErrorCode::InvalidFormat => 3,
            ErrorCode::VersionMismatch => 4,
            ErrorCode::InvalidCollection => 5,
            ErrorCode::InvalidFilter => 6,
            ErrorCode::InvalidUpdate => 7,
            ErrorCode::InvalidSort => 8,
            ErrorCode::InvalidProjection => 9,
            ErrorCode::InvalidDocument => 10,
            ErrorCode::IoError => 11,
            ErrorCode::Internal => 99,
        }
    }

    /// 是否为「客户端输入有问题」类错误(HTTP 映射为 4xx,而非 5xx)。
    pub fn is_client_error(self) -> bool {
        !matches!(self, ErrorCode::IoError | ErrorCode::Internal)
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl DatabaseError {
    /// 取稳定的跨语言错误码。
    pub fn code(&self) -> ErrorCode {
        use crate::query::{FilterError, SortError, UpdateError};
        match self {
            DatabaseError::Storage(e) => match e {
                crate::engine::StorageError::InvalidPath(_) => ErrorCode::InvalidPath,
                crate::engine::StorageError::InvalidFormat(_) => ErrorCode::InvalidFormat,
                crate::engine::StorageError::VersionMismatch { .. } => ErrorCode::VersionMismatch,
                crate::engine::StorageError::InvalidCollection(_) => ErrorCode::InvalidCollection,
                crate::engine::StorageError::DuplicateKey { .. } => ErrorCode::DuplicateKey,
                crate::engine::StorageError::UniqueViolation { .. } => ErrorCode::DuplicateKey,
                crate::engine::StorageError::Io(_) => ErrorCode::IoError,
                crate::engine::StorageError::Other(_) => ErrorCode::Internal,
            },
            DatabaseError::Document(_) => ErrorCode::InvalidDocument,
            DatabaseError::Filter(e) => match e {
                FilterError::NotObject
                | FilterError::InvalidOperator(_)
                | FilterError::InvalidRegex(_) => ErrorCode::InvalidFilter,
            },
            DatabaseError::Sort(e) => match e {
                SortError::NotObject | SortError::InvalidDirection(_) | SortError::Empty => {
                    ErrorCode::InvalidSort
                }
            },
            DatabaseError::Projection(_) => ErrorCode::InvalidProjection,
            DatabaseError::Update(e) => match e {
                UpdateError::InvalidUpdate(_)
                | UpdateError::SetNotObject
                | UpdateError::UnsetNotObject
                | UpdateError::TypeMismatch { .. }
                | UpdateError::InvalidModifier(_) => ErrorCode::InvalidUpdate,
                // 路径/下标类错误本质也是「更新参数非法」
                UpdateError::InvalidArrayIndex(_)
                | UpdateError::ArrayIndexOutOfBounds { .. }
                | UpdateError::ScalarIntermediate { .. } => ErrorCode::InvalidUpdate,
            },
        }
    }
}
