//! 基于 redb 的存储引擎实现。
//!
//! redb 是纯 Rust 实现的单文件 B+树数据库(自带 ACID 事务),非常适合作为
//! nosqlite 的底层存储:整个数据库就是一个 `.nodb` 文件,可直接复制粘贴迁移。
//!
//! # 魔数与版本校验
//!
//! redb 文件头本身有它自己的校验;在此基础上,我们在数据库内维护一个元数据表
//! (`nosqlite_meta`),写入 nosqlite 的魔数串与格式版本号。
//! 打开数据库时读取并校验:
//! - 魔数不匹配 → 说明文件虽然可能是 redb 数据库,但不是 nosqlite 创建的;
//! - 版本不兼容 → 说明文件由其他版本的 nosqlite 创建。

use std::path::Path;

use redb::{Database, ReadableDatabase, TableDefinition};

use super::storage::{Storage, StorageError};

/// 魔数串:用于标识数据库确实是 nosqlite 创建。
const MAGIC: &str = "NOSQLITE";
/// 当前文件格式版本号。
const VERSION: u32 = 1;

/// 元数据表:存放魔数串与格式版本号(键:字符串,值:字节)。
const META_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("nosqlite_meta");
/// 元数据表内魔数键名。
const KEY_MAGIC: &str = "magic";
/// 元数据表内版本键名。
const KEY_VERSION: &str = "version";

/// 文档表:存放所有集合的文档。
///
/// key = `集合名(0x00)doc_id` 字节拼接,value = 文档 JSON bytes。
/// redb B+树天然按 key 有序,集合内 doc_id 有序即为主键索引。
const DOCS_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("nosqlite_docs");
/// 集合名与 doc_id 的分隔符(集合名不允许包含该字节)。
const KEY_SEPARATOR: u8 = 0x00;

/// 校验集合名:非空,且不包含分隔符 `\0`。
fn validate_collection(collection: &str) -> Result<(), StorageError> {
    if collection.is_empty() {
        return Err(StorageError::InvalidCollection("集合名不能为空".to_string()));
    }
    if collection.as_bytes().contains(&KEY_SEPARATOR) {
        return Err(StorageError::InvalidCollection(format!(
            "集合名不能包含分隔符(\\0): {}",
            collection
        )));
    }
    Ok(())
}

/// 构造文档表 key:`集合名(0x00)doc_id`。
fn build_doc_key(collection: &str, doc_id: &str) -> Vec<u8> {
    let mut key = Vec::with_capacity(collection.len() + 1 + doc_id.len());
    key.extend_from_slice(collection.as_bytes());
    key.push(KEY_SEPARATOR);
    key.extend_from_slice(doc_id.as_bytes());
    key
}

/// redb 存储引擎。
#[derive(Debug)]
pub struct RedbEngine {
    /// 底层 redb 数据库句柄(M2 起用于读写数据)。
    #[allow(dead_code)]
    db: Database,
    /// 数据库文件路径(用于错误信息展示)。
    #[allow(dead_code)]
    path: String,
}

impl RedbEngine {
    /// 校验路径后缀必须是 `.nodb`(不区分大小写),否则拒绝。
    fn validate_path(path: &Path) -> Result<(), StorageError> {
        match path.extension().and_then(|ext| ext.to_str()) {
            Some(ext) if ext.eq_ignore_ascii_case("nodb") => Ok(()),
            _ => Err(StorageError::InvalidPath(format!(
                "文件后缀必须是 .nodb,不支持其他后缀或无后缀: {}",
                path.display()
            ))),
        }
    }

    /// 创建后立即写入魔数串与版本号元数据,完成一次事务提交。
    fn write_meta(db: &Database) -> Result<(), StorageError> {
        let write_tx = db
            .begin_write()
            .map_err(|e| StorageError::Other(format!("启动写入事务失败: {}", e)))?;
        {
            let mut table = write_tx
                .open_table(META_TABLE)
                .map_err(|e| StorageError::Other(format!("打开元数据表失败: {}", e)))?;
            table
                .insert(KEY_MAGIC, MAGIC.as_bytes())
                .map_err(|e| StorageError::Other(format!("写入魔数失败: {}", e)))?;
            table
                .insert(KEY_VERSION, &VERSION.to_le_bytes()[..])
                .map_err(|e| StorageError::Other(format!("写入版本失败: {}", e)))?;
        }
        write_tx
            .commit()
            .map_err(|e| StorageError::Other(format!("提交元数据失败: {}", e)))
    }

    /// 读取并校验元数据表中的魔数串与版本号。
    fn check_meta(db: &Database) -> Result<(), StorageError> {
        let read_tx = db
            .begin_read()
            .map_err(|e| StorageError::Other(format!("启动读取事务失败: {}", e)))?;
        let table = read_tx.open_table(META_TABLE).map_err(|_| {
            StorageError::InvalidFormat(
                "文件缺少 nosqlite 元数据,不是 nosqlite 创建的数据库".to_string(),
            )
        })?;

        // 1. 校验魔数串
        let magic = table
            .get(KEY_MAGIC)
            .map_err(|e| StorageError::Other(format!("读取魔数失败: {}", e)))?;
        match magic {
            Some(guard) if guard.value() == MAGIC.as_bytes() => {}
            _ => {
                return Err(StorageError::InvalidFormat(
                    "文件魔数不匹配,不是 nosqlite 创建的数据库".to_string(),
                ))
            }
        }

        // 2. 校验格式版本号
        let version = table
            .get(KEY_VERSION)
            .map_err(|e| StorageError::Other(format!("读取版本失败: {}", e)))?;
        if let Some(guard) = version {
            let bytes: [u8; 4] = guard
                .value()
                .try_into()
                .map_err(|_| StorageError::InvalidFormat("版本字段损坏".to_string()))?;
            let found = u32::from_le_bytes(bytes);
            if found != VERSION {
                return Err(StorageError::VersionMismatch {
                    found,
                    expected: VERSION,
                });
            }
        } else {
            return Err(StorageError::InvalidFormat(
                "文件缺少版本信息,不是 nosqlite 创建的数据库".to_string(),
            ));
        }
        Ok(())
    }
}

impl Storage for RedbEngine {
    fn create(path: &Path) -> Result<Self, StorageError> {
        // 1. 后缀校验:非 .nodb 一律拒绝
        Self::validate_path(path)?;

        // 2. 防误覆盖:目标文件已存在则拒绝重建
        if path.exists() {
            return Err(StorageError::Other(format!(
                "数据库文件已存在,如需重建请先删除: {}",
                path.display()
            )));
        }

        // 3. 创建 redb 数据库文件
        let db = Database::create(path)
            .map_err(|e| StorageError::Other(format!("创建数据库文件失败: {}", e)))?;

        // 4. 写入魔数 + 版本元数据
        Self::write_meta(&db)?;

        Ok(RedbEngine {
            db,
            path: path.display().to_string(),
        })
    }

    fn open(path: &Path) -> Result<Self, StorageError> {
        // 1. 后缀校验:非 .nodb 一律拒绝
        Self::validate_path(path)?;

        // 2. 打开 redb 数据库(文件不存在或非 redb 格式都会失败)
        let db = Database::open(path).map_err(|_| {
            StorageError::InvalidFormat(format!(
                "无法打开 {}:文件不存在或不是有效的 nosqlite 数据库",
                path.display()
            ))
        })?;

        // 3. 校验魔数 + 版本
        Self::check_meta(&db)?;

        Ok(RedbEngine {
            db,
            path: path.display().to_string(),
        })
    }

    fn close(&self) -> Result<(), StorageError> {
        // redb 在 Drop 时会自动落盘并释放资源,此处保持接口完整
        Ok(())
    }

    fn insert(
        &self,
        collection: &str,
        doc_id: &str,
        doc_bytes: &[u8],
    ) -> Result<(), StorageError> {
        // 1. 校验集合名(非空、不含分隔符)
        validate_collection(collection)?;

        // 2. 构造 key = 集合名(0x00)doc_id
        let key = build_doc_key(collection, doc_id);

        // 3. 写入事务:打开文档表并插入;若主键已存在则返回冲突
        let write_tx = self
            .db
            .begin_write()
            .map_err(|e| StorageError::Other(format!("启动写入事务失败: {}", e)))?;
        {
            let mut table = write_tx
                .open_table(DOCS_TABLE)
                .map_err(|e| StorageError::Other(format!("打开文档表失败: {}", e)))?;
            let existing = table
                .insert(&key[..], doc_bytes)
                .map_err(|e| StorageError::Other(format!("写入文档失败: {}", e)))?;
            // redb 的 insert 若 key 已存在会返回旧值,借此检测主键冲突
            if existing.is_some() {
                return Err(StorageError::DuplicateKey {
                    collection: collection.to_string(),
                    id: doc_id.to_string(),
                });
            }
        }
        write_tx
            .commit()
            .map_err(|e| StorageError::Other(format!("提交写入事务失败: {}", e)))
    }

    fn scan(&self, collection: &str) -> Result<Vec<(String, Vec<u8>)>, StorageError> {
        // 1. 校验集合名
        validate_collection(collection)?;

        // 2. 构造前缀 = 集合名(0x00),集合内所有文档的 key 均以该前缀开头
        let prefix = build_doc_key(collection, "");

        // 3. 读取事务 + 打开文档表
        let read_tx = self
            .db
            .begin_read()
            .map_err(|e| StorageError::Other(format!("启动读取事务失败: {}", e)))?;
        let table = match read_tx.open_table(DOCS_TABLE) {
            Ok(table) => table,
            // 文档表尚不存在(从未插入过文档)→ 该集合为空
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => return Err(StorageError::Other(format!("打开文档表失败: {}", e))),
        };

        // 4. 从前缀处开始迭代,B+树按 key 有序,集合内 doc_id 即按序返回
        let mut results = Vec::new();
        let range = table
            .range(prefix.as_slice()..)
            .map_err(|e| StorageError::Other(format!("遍历文档表失败: {}", e)))?;
        for entry in range {
            let (key, value) = entry.map_err(|e| StorageError::Other(format!("读取文档失败: {}", e)))?;
            let key_bytes = key.value();
            // 不再是该集合前缀 → 该集合文档已遍历完毕
            if !key_bytes.starts_with(&prefix) {
                break;
            }
            // key = 前缀 + doc_id
            let doc_id_bytes = &key_bytes[prefix.len()..];
            let doc_id = String::from_utf8_lossy(doc_id_bytes).into_owned();
            results.push((doc_id, value.value().to_vec()));
        }
        Ok(results)
    }
}
