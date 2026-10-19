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

use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};

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
pub(crate) const DOCS_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("nosqlite_docs");
/// 集合名与 doc_id 的分隔符(集合名不允许包含该字节)。
const KEY_SEPARATOR: u8 = 0x00;

/// 索引定义表:key = `集合名\0索引名`,value = 索引定义 JSON。
///
/// 定义形如 `{"field": "age", "unique": false, "sparse": false, "encoding_version": 1}`。
pub(crate) const INDEX_META_TABLE: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("nosqlite_index_meta");

/// 索引数据表:key = `集合名\0字段路径\0编码值\0doc_id`,value = 空。
///
/// - `doc_id` 拼在末尾:保证 key 唯一,且相同索引值下天然按 doc_id 有序
///   (这是「索引条目必须有唯一后缀」的传统做法);
/// - multikey 索引:数组字段的每个元素各生成一条,key 结构不变。
pub(crate) const INDEX_DATA_TABLE: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("nosqlite_index_data");

/// 索引编码版本:编码规则变更时递增,用于检测并重建索引。
pub(crate) const INDEX_ENCODING_VERSION: u32 = 1;

/// 一个有效 `.nodb` 文件的最小长度。
///
/// 低于这个值几乎必然是被截断的文件,提前拒绝以免 redb 内部断言 panic。
const MIN_VALID_FILE_LEN: u64 = 4096;

/// 读取 redb 文件头部声明的**文件总长度**,用于检测截断(v0.6.2)。
///
/// redb 3.x 头部(`header.rs`)布局:
/// - `[0..9)`   魔数 `redb\x1A\n\xA9\r\n`
/// - `[9]`      god byte
/// - `[12..16)` page_size(u32 LE)
/// - `[16..20)` region_header_pages(u32 LE)
/// - `[20..24)` region_max_data_pages(u32 LE)
/// - `[24..28)` num_full_regions(u32 LE)
/// - `[28..32)` trailing_region_data_pages(u32 LE)
///
/// 总长 = `page_size + (regions-1) * full_region_len + last_region_len`。
///
/// **为什么要自己算**:redb 3.1.3 在文件被截断时走的是
/// `assert!(raw_file_len >= header.layout().len())`,直接 panic 而非返回错误。
/// 文件损坏是真实场景,必须在封装层挡住。
fn read_redb_declared_len(path: &Path) -> Option<u64> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut head = [0u8; 32];
    f.read_exact(&mut head).ok()?;
    if &head[0..9] != b"redb\x1a\n\xa9\r\n" {
        return None;
    }
    let u32_at = |off: usize| u32::from_le_bytes(head[off..off + 4].try_into().unwrap());
    let page_size = u64::from(u32_at(12));
    if page_size == 0 || !page_size.is_power_of_two() || page_size > (1 << 20) {
        return None;
    }
    let header_pages = u64::from(u32_at(16));
    let max_data_pages = u64::from(u32_at(20));
    let full_regions = u64::from(u32_at(24));
    let trailing_pages = u64::from(u32_at(28));

    let full_region_len = (header_pages + max_data_pages) * page_size;
    let regions = if trailing_pages > 0 {
        full_regions + 1
    } else {
        full_regions
    };
    if regions == 0 {
        return None;
    }
    let last_len = if trailing_pages > 0 {
        (header_pages + trailing_pages) * page_size
    } else {
        full_region_len
    };
    let expected = page_size + (regions - 1) * full_region_len + last_len;
    (expected > 0 && expected < (1u64 << 48)).then_some(expected)
}

/// 校验集合名:非空,且不包含分隔符 `\0`。
fn validate_collection(collection: &str) -> Result<(), StorageError> {
    if collection.is_empty() {
        return Err(StorageError::InvalidCollection(
            "集合名不能为空".to_string(),
        ));
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
                ));
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

impl RedbEngine {
    /// 列出指定集合的全部索引定义。
    pub fn list_indexes(
        &self,
        collection: &str,
    ) -> Result<Vec<crate::index::def::IndexDef>, StorageError> {
        use crate::index::def::{IndexDef, build_meta_prefix};
        let read_tx = self
            .db
            .begin_read()
            .map_err(|e| StorageError::Other(format!("启动读取事务失败: {}", e)))?;
        let table = match read_tx.open_table(INDEX_META_TABLE) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => return Err(StorageError::Other(format!("打开索引定义表失败: {}", e))),
        };
        let prefix = build_meta_prefix(collection);
        let range = table
            .range(prefix.as_slice()..)
            .map_err(|e| StorageError::Other(format!("遍历索引定义失败: {}", e)))?;
        let mut out = Vec::new();
        for entry in range {
            let (k, v) =
                entry.map_err(|e| StorageError::Other(format!("读取索引定义失败: {}", e)))?;
            let key = k.value();
            if !key.starts_with(&prefix) {
                break;
            }
            let name = String::from_utf8_lossy(&key[prefix.len()..]).into_owned();
            if let Some(def) = IndexDef::from_json(collection, &name, v.value()) {
                out.push(def);
            }
        }
        Ok(out)
    }

    /// 创建索引:写定义 + 为既有文档批量回填条目(同一写事务,原子)。
    ///
    /// 若同名索引已存在,先删除旧定义与旧条目再重建(幂等)。
    pub fn create_index(&self, def: &crate::index::def::IndexDef) -> Result<(), StorageError> {
        use crate::index::def::{build_entry_key, build_meta_key, entry_values};
        validate_collection(&def.collection)?;

        // 1. 先读出既有文档(读事务独立,避免与写事务互相阻塞)
        let docs = self.scan(&def.collection)?;
        // 解析为 (doc_id, JSON 对象)
        let mut parsed: Vec<(String, serde_json::Map<String, serde_json::Value>)> = Vec::new();
        for (doc_id, bytes) in docs {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes)
                && let Some(map) = v.as_object()
            {
                parsed.push((doc_id, map.clone()));
            }
        }

        // 2. 唯一索引前置检查:重复值直接拒绝(不写任何东西)
        if def.unique {
            let mut seen: Vec<Vec<Vec<u8>>> = Vec::new();
            for (_, doc) in &parsed {
                for combo in entry_values(def, doc) {
                    if seen.contains(&combo) {
                        return Err(StorageError::UniqueViolation {
                            collection: def.collection.clone(),
                            index: def.name.clone(),
                        });
                    }
                    seen.push(combo);
                }
            }
        }

        // 3. 单写事务:清旧条目 → 写定义 → 回填条目
        let write_tx = self
            .db
            .begin_write()
            .map_err(|e| StorageError::Other(format!("启动写入事务失败: {}", e)))?;
        {
            let mut meta = write_tx
                .open_table(INDEX_META_TABLE)
                .map_err(|e| StorageError::Other(format!("打开索引定义表失败: {}", e)))?;
            let mut data = write_tx
                .open_table(INDEX_DATA_TABLE)
                .map_err(|e| StorageError::Other(format!("打开索引数据表失败: {}", e)))?;

            // 清掉该集合该字段的旧条目
            let prefix =
                crate::index::def::build_entry_prefix(&def.collection, &def.field_signature());
            let mut stale: Vec<Vec<u8>> = Vec::new();
            {
                let range = data
                    .range(prefix.as_slice()..)
                    .map_err(|e| StorageError::Other(format!("遍历索引条目失败: {}", e)))?;
                for entry in range {
                    let (k, _) = entry
                        .map_err(|e| StorageError::Other(format!("读取索引条目失败: {}", e)))?;
                    let key = k.value();
                    if !key.starts_with(&prefix) {
                        break;
                    }
                    stale.push(key.to_vec());
                }
            }
            for key in &stale {
                data.remove(key.as_slice())
                    .map_err(|e| StorageError::Other(format!("删除旧索引条目失败: {}", e)))?;
            }

            // 写定义
            meta.insert(
                build_meta_key(&def.collection, &def.name).as_slice(),
                def.to_json().as_slice(),
            )
            .map_err(|e| StorageError::Other(format!("写入索引定义失败: {}", e)))?;

            // 回填条目
            for (doc_id, doc) in &parsed {
                for combo in entry_values(def, doc) {
                    let key =
                        build_entry_key(&def.collection, &def.field_signature(), &combo, doc_id);
                    data.insert(key.as_slice(), b"".as_slice())
                        .map_err(|e| StorageError::Other(format!("写入索引条目失败: {}", e)))?;
                }
            }
        }
        write_tx
            .commit()
            .map_err(|e| StorageError::Other(format!("提交索引创建失败: {}", e)))
    }

    /// 按索引区间扫描,返回候选 doc_id 列表(去重,按索引序)。
    ///
    /// `lower`/`upper` 是**已编码**的边界;`None` 表示无界。区间语义为
    /// `[lower, upper)`(调用方负责把 `$lte` 转成「上界 + 0xFF 前缀」)。
    ///
    /// 返回的 doc_id 只保证「落在索引区间内」,**不保证满足完整 filter** ——
    /// 调用方必须再做一次 `filter.matches()` 二次过滤。这是刻意的安全设计:
    /// 即使区间构造有偏差,也只会多读,不会返回错误结果。
    pub fn scan_index_range(
        &self,
        collection: &str,
        field_signature: &str,
        segments: usize,
        lower: Option<&[u8]>,
        upper: Option<&[u8]>,
        descending: bool,
    ) -> Result<Vec<String>, StorageError> {
        let prefix = crate::index::def::build_entry_prefix(collection, field_signature);
        let read_tx = self
            .db
            .begin_read()
            .map_err(|e| StorageError::Other(format!("启动读取事务失败: {}", e)))?;
        let table = match read_tx.open_table(INDEX_DATA_TABLE) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => return Err(StorageError::Other(format!("打开索引数据表失败: {}", e))),
        };

        // 起点 = prefix + 编码值(长度后缀在编码值之后,不参与定位)
        let build_point = |enc: Option<&[u8]>| -> Vec<u8> {
            let mut k = prefix.clone();
            if let Some(v) = enc {
                k.extend_from_slice(v);
            }
            k
        };

        let start = build_point(lower);
        let range = match upper {
            Some(_) => {
                let end = build_point(upper);
                table
                    .range(start.as_slice()..end.as_slice())
                    .map_err(|e| StorageError::Other(format!("索引区间扫描失败: {}", e)))?
            }
            None => table
                .range(start.as_slice()..)
                .map_err(|e| StorageError::Other(format!("索引区间扫描失败: {}", e)))?,
        };

        let mut doc_ids: Vec<String> = Vec::new();
        for entry in range {
            let (k, _) =
                entry.map_err(|e| StorageError::Other(format!("读取索引条目失败: {}", e)))?;
            let key = k.value();
            if !key.starts_with(&prefix) {
                break;
            }
            // 解析:prefix | enc1..encN(共 `total` 字节) | len1..lenN(各 4 字节) | doc_id
            //
            // 由于编码值本身可含 0x00 且变长,这里用「遍历候选总长,校验各段长度自洽」定位。
            let rest = &key[prefix.len()..];
            let Some((encoded, doc_id_bytes)) =
                crate::index::def::split_entry_multi(rest, segments)
            else {
                continue;
            };

            // 精确区间判断(upper 为半开区间)
            if let Some(lo) = lower
                && encoded < lo
            {
                continue;
            }
            if let Some(hi) = upper
                && encoded >= hi
            {
                break;
            }
            let doc_id = String::from_utf8_lossy(doc_id_bytes).into_owned();
            if doc_ids.last() != Some(&doc_id) {
                doc_ids.push(doc_id);
            }
        }
        // 降序索引:条目仍按升序存储,逆序产出即得降序结果。
        // 区间语义不变(仍是 `[lower, upper)`),只反转遍历方向。
        if descending {
            doc_ids.reverse();
        }
        Ok(doc_ids)
    }

    /// 删除索引(定义 + 全部条目)。
    pub fn drop_index(&self, collection: &str, name: &str) -> Result<bool, StorageError> {
        use crate::index::def::{IndexDef, build_meta_key};
        // 先查定义,拿到字段路径
        let defs = self.list_indexes(collection)?;
        let Some(def) = defs.iter().find(|d| d.name == name) else {
            return Ok(false);
        };
        let field_sig = def.field_signature();

        let write_tx = self
            .db
            .begin_write()
            .map_err(|e| StorageError::Other(format!("启动写入事务失败: {}", e)))?;
        {
            let mut meta = write_tx
                .open_table(INDEX_META_TABLE)
                .map_err(|e| StorageError::Other(format!("打开索引定义表失败: {}", e)))?;
            let mut data = write_tx
                .open_table(INDEX_DATA_TABLE)
                .map_err(|e| StorageError::Other(format!("打开索引数据表失败: {}", e)))?;

            meta.remove(build_meta_key(collection, name).as_slice())
                .map_err(|e| StorageError::Other(format!("删除索引定义失败: {}", e)))?;

            let prefix = crate::index::def::build_entry_prefix(collection, &field_sig);
            let mut stale: Vec<Vec<u8>> = Vec::new();
            {
                let range = data
                    .range(prefix.as_slice()..)
                    .map_err(|e| StorageError::Other(format!("遍历索引条目失败: {}", e)))?;
                for entry in range {
                    let (k, _) = entry
                        .map_err(|e| StorageError::Other(format!("读取索引条目失败: {}", e)))?;
                    let key = k.value();
                    if !key.starts_with(&prefix) {
                        break;
                    }
                    stale.push(key.to_vec());
                }
            }
            for key in &stale {
                data.remove(key.as_slice())
                    .map_err(|e| StorageError::Other(format!("删除索引条目失败: {}", e)))?;
            }
            let _ = IndexDef::new(collection, name, &field_sig, false, false);
        }
        write_tx
            .commit()
            .map_err(|e| StorageError::Other(format!("提交索引删除失败: {}", e)))?;
        Ok(true)
    }
}

/// 在写事务内**删除**某条文档的全部索引条目(基于旧文档内容)。
///
/// 找不到旧文档(新增场景)时无操作。
fn remove_index_entries(
    data: &mut redb::Table<'_, &[u8], &[u8]>,
    defs: &[crate::index::def::IndexDef],
    collection: &str,
    doc_id: &str,
    old_bytes: Option<&[u8]>,
) -> Result<(), StorageError> {
    use crate::index::def::{build_entry_key, entry_values};
    let Some(bytes) = old_bytes else {
        return Ok(());
    };
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return Ok(());
    };
    let Some(map) = v.as_object() else {
        return Ok(());
    };
    for def in defs {
        for combo in entry_values(def, map) {
            let key = build_entry_key(collection, &def.field_signature(), &combo, doc_id);
            data.remove(key.as_slice())
                .map_err(|e| StorageError::Other(format!("删除索引条目失败: {}", e)))?;
        }
    }
    Ok(())
}

/// 在写事务内**写入**某条文档的全部索引条目(基于新文档内容)。
fn write_index_entries(
    data: &mut redb::Table<'_, &[u8], &[u8]>,
    defs: &[crate::index::def::IndexDef],
    collection: &str,
    doc_id: &str,
    new_bytes: &[u8],
) -> Result<(), StorageError> {
    use crate::index::def::{build_entry_key, entry_values};
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(new_bytes) else {
        return Ok(());
    };
    let Some(map) = v.as_object() else {
        return Ok(());
    };
    for def in defs {
        for combo in entry_values(def, map) {
            let key = build_entry_key(collection, &def.field_signature(), &combo, doc_id);
            // 唯一索引:同值组合已存在其他文档 → 冲突。
            //
            // 判断方式:用「prefix + 本组合全部编码值」作 probe 前缀扫描。
            // 由于 key 布局为 `prefix | enc1..encN | len1..lenN | doc_id`,
            // probe 恰好覆盖到编码值末尾,后续是长度数组与 doc_id。
            if def.unique {
                let mut probe = Vec::new();
                probe.extend_from_slice(collection.as_bytes());
                probe.push(0);
                probe.extend_from_slice(def.field_signature().as_bytes());
                probe.push(0);
                for seg in &combo {
                    probe.extend_from_slice(seg);
                }

                let range = data
                    .range(probe.as_slice()..)
                    .map_err(|e| StorageError::Other(format!("扫描索引条目失败: {}", e)))?;
                let lens_bytes = 4 * combo.len();
                let mut conflict = false;
                for entry in range {
                    let (k, _) = entry
                        .map_err(|e| StorageError::Other(format!("读取索引条目失败: {}", e)))?;
                    let k2 = k.value();
                    if !k2.starts_with(&probe) {
                        break;
                    }
                    let rest = &k2[probe.len()..];
                    if rest.len() < lens_bytes {
                        continue;
                    }
                    // 校验各段长度与当前组合一致,排除「更长值以该编码为前缀」
                    let mut ok = true;
                    for (i, seg) in combo.iter().enumerate() {
                        let off = i * 4;
                        let len =
                            u32::from_be_bytes(rest[off..off + 4].try_into().unwrap()) as usize;
                        if len != seg.len() {
                            ok = false;
                            break;
                        }
                    }
                    if !ok {
                        continue;
                    }
                    let doc = &rest[lens_bytes..];
                    if doc != doc_id.as_bytes() {
                        conflict = true;
                        break;
                    }
                }
                if conflict {
                    return Err(StorageError::UniqueViolation {
                        collection: collection.to_string(),
                        index: def.name.clone(),
                    });
                }
            }
            data.insert(key.as_slice(), b"".as_slice())
                .map_err(|e| StorageError::Other(format!("写入索引条目失败: {}", e)))?;
        }
    }
    Ok(())
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

        // 1.5 损坏预检(v0.6.2):redb 在文件被截断时会 assert panic,
        //     提前用头部声明的长度比对实际长度,把崩溃变成明确错误。
        if let Ok(meta) = std::fs::metadata(path) {
            if meta.len() < MIN_VALID_FILE_LEN {
                return Err(StorageError::InvalidFormat(format!(
                    "{} 文件过短({} 字节),疑似被截断,不是有效的 nosqlite 数据库",
                    path.display(),
                    meta.len()
                )));
            }
            if let Some(expected) = read_redb_declared_len(path)
                && meta.len() < expected
            {
                return Err(StorageError::InvalidFormat(format!(
                    "{} 被截断(声明 {} 字节,实际只有 {} 字节)",
                    path.display(),
                    expected,
                    meta.len()
                )));
            }
        }

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

    fn insert(&self, collection: &str, doc_id: &str, doc_bytes: &[u8]) -> Result<(), StorageError> {
        // 1. 校验集合名(非空、不含分隔符)
        validate_collection(collection)?;

        // 2. 构造 key = 集合名(0x00)doc_id
        let key = build_doc_key(collection, doc_id);

        // 索引定义必须在开启写事务**之前**读取:
        // redb 不允许写事务存活期间再开读事务,否则索引维护会静默失效。
        let defs = self.list_indexes(collection)?;

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

            // 维护索引:新增场景没有旧条目,只要写新条目
            if !defs.is_empty() {
                let mut data = write_tx
                    .open_table(INDEX_DATA_TABLE)
                    .map_err(|e| StorageError::Other(format!("打开索引数据表失败: {}", e)))?;
                write_index_entries(&mut data, &defs, collection, doc_id, doc_bytes)?;
            }
        }
        write_tx
            .commit()
            .map_err(|e| StorageError::Other(format!("提交写入事务失败: {}", e)))
    }

    /// **流式**遍历集合内全部文档:逐条产出并交由回调处理,不物化整个集合。
    ///
    /// 实现:在单个读事务内用 redb `range(prefix..)` 迭代器按序遍历,
    /// 回调返回 `false` 时提前结束(后续文档不再读取)。
    fn for_each(
        &self,
        collection: &str,
        visit: &mut dyn FnMut(&str, &[u8]) -> bool,
    ) -> Result<(), StorageError> {
        // M22 起内部改为基于 scan_session:保持回调语义,但复用同一套流式实现,
        // 避免两份几乎相同的遍历代码各自演化。
        const BATCH: usize = 1024;
        let mut session = self.scan_session(collection)?;
        loop {
            let batch = session.next_batch(BATCH)?;
            if batch.is_empty() {
                return Ok(());
            }
            for (doc_id, bytes) in &batch {
                if !visit(doc_id, bytes) {
                    return Ok(());
                }
            }
        }
    }

    fn get(&self, collection: &str, doc_id: &str) -> Result<Option<Vec<u8>>, StorageError> {
        // 1. 校验集合名
        validate_collection(collection)?;

        // 2. 读取事务 + 打开文档表
        let read_tx = self
            .db
            .begin_read()
            .map_err(|e| StorageError::Other(format!("启动读取事务失败: {}", e)))?;
        let table = match read_tx.open_table(DOCS_TABLE) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(e) => return Err(StorageError::Other(format!("打开文档表失败: {}", e))),
        };

        // 3. 主键直接定位
        let key = build_doc_key(collection, doc_id);
        let value = table
            .get(&key[..])
            .map_err(|e| StorageError::Other(format!("读取文档失败: {}", e)))?;
        Ok(value.map(|guard| guard.value().to_vec()))
    }

    fn scan_session(
        &self,
        collection: &str,
    ) -> Result<Box<dyn super::storage::ScanSession>, StorageError> {
        // 1. 校验集合名
        validate_collection(collection)?;

        // 2. 构造前缀 = 集合名(0x00)
        let prefix = build_doc_key(collection, "");

        // 3. 开启读事务;redb 的 Range 自持读事务,故可安全地跨调用存活
        let read_tx = self
            .db
            .begin_read()
            .map_err(|e| StorageError::Other(format!("启动读取事务失败: {}", e)))?;

        match super::scan::open_range(&read_tx, &prefix)? {
            Some(iter) => Ok(Box::new(super::scan::RedbScanSession::new(
                prefix, read_tx, iter,
            ))),
            // 文档表不存在(空库)→ 返回一个立即耗尽的会话
            None => Ok(Box::new(super::scan::RedbScanSession::empty(
                prefix, read_tx,
            ))),
        }
    }

    /// 单事务批量插入:所有文档在同一写事务内写入并一次性提交。
    ///
    /// 这是写入吞吐的关键:逐条插入每条都要经历一次「开启事务 + 提交 + fsync」,
    /// 而批量把 N 条的风险成本合并为一次提交。任一主键冲突则整个批次回滚。
    fn insert_many(
        &self,
        collection: &str,
        docs: &[(String, Vec<u8>)],
    ) -> Result<(), StorageError> {
        // 1. 校验集合名
        validate_collection(collection)?;

        // 2. 空批次直接成功(避免无意义地开事务)
        if docs.is_empty() {
            return Ok(());
        }

        // 索引定义必须在开启写事务**之前**读取:
        // redb 不允许在写事务存活期间再开读事务,否则会静默出错。
        let defs = self.list_indexes(collection)?;

        // 3. 单写事务内写入全部文档,一次性提交
        let write_tx = self
            .db
            .begin_write()
            .map_err(|e| StorageError::Other(format!("启动写入事务失败: {}", e)))?;
        {
            let mut table = write_tx
                .open_table(DOCS_TABLE)
                .map_err(|e| StorageError::Other(format!("打开文档表失败: {}", e)))?;
            for (doc_id, bytes) in docs {
                let key = build_doc_key(collection, doc_id);
                let existing = table
                    .insert(&key[..], &bytes[..])
                    .map_err(|e| StorageError::Other(format!("写入文档失败: {}", e)))?;
                // 主键冲突 → 整批失败(事务随 drop/错误路径回滚)
                if existing.is_some() {
                    return Err(StorageError::DuplicateKey {
                        collection: collection.to_string(),
                        id: doc_id.clone(),
                    });
                }
                if !defs.is_empty() {
                    let mut data = write_tx
                        .open_table(INDEX_DATA_TABLE)
                        .map_err(|e| StorageError::Other(format!("打开索引数据表失败: {}", e)))?;
                    write_index_entries(&mut data, &defs, collection, doc_id, bytes)?;
                }
            }
        }
        write_tx
            .commit()
            .map_err(|e| StorageError::Other(format!("提交写入事务失败: {}", e)))
    }

    fn update(
        &self,
        collection: &str,
        doc_id: &str,
        doc_bytes: &[u8],
    ) -> Result<bool, StorageError> {
        // 1. 校验集合名
        validate_collection(collection)?;

        // 2. 构造 key = 集合名(0x00)doc_id
        let key = build_doc_key(collection, doc_id);

        // 索引定义必须在写事务之前读取(redb 不允许写事务期间开读事务)
        let defs = self.list_indexes(collection)?;

        // 3. 写事务:覆盖写入(redb insert 同 key 即覆盖,返回旧值)
        let write_tx = self
            .db
            .begin_write()
            .map_err(|e| StorageError::Other(format!("启动写入事务失败: {}", e)))?;
        let updated = {
            let mut table = write_tx
                .open_table(DOCS_TABLE)
                .map_err(|e| StorageError::Other(format!("打开文档表失败: {}", e)))?;
            // 必须先拿到旧文档内容,才能删掉它对应的旧索引条目
            let old_bytes = table
                .get(&key[..])
                .map_err(|e| StorageError::Other(format!("读取旧文档失败: {}", e)))?
                .map(|g| g.value().to_vec());
            let old = table
                .insert(&key[..], doc_bytes)
                .map_err(|e| StorageError::Other(format!("更新文档失败: {}", e)))?;

            if !defs.is_empty() {
                let mut data = write_tx
                    .open_table(INDEX_DATA_TABLE)
                    .map_err(|e| StorageError::Other(format!("打开索引数据表失败: {}", e)))?;
                // 先删旧条目,再写新条目 —— 顺序不能反,否则会留下垃圾
                remove_index_entries(&mut data, &defs, collection, doc_id, old_bytes.as_deref())?;
                write_index_entries(&mut data, &defs, collection, doc_id, doc_bytes)?;
            }
            old.is_some()
        };
        write_tx
            .commit()
            .map_err(|e| StorageError::Other(format!("提交写入事务失败: {}", e)))?;
        Ok(updated)
    }

    fn delete(&self, collection: &str, doc_id: &str) -> Result<bool, StorageError> {
        // 1. 校验集合名
        validate_collection(collection)?;

        // 2. 构造 key
        let key = build_doc_key(collection, doc_id);

        // 索引定义必须在写事务之前读取(redb 不允许写事务期间开读事务)
        let defs = self.list_indexes(collection)?;

        // 3. 写事务:删除该 key(返回是否实际删除)
        let write_tx = self
            .db
            .begin_write()
            .map_err(|e| StorageError::Other(format!("启动写入事务失败: {}", e)))?;
        let deleted = {
            let mut table = write_tx
                .open_table(DOCS_TABLE)
                .map_err(|e| StorageError::Other(format!("打开文档表失败: {}", e)))?;
            // 先取旧文档内容,用于删除其索引条目
            let old_bytes = table
                .get(&key[..])
                .map_err(|e| StorageError::Other(format!("读取旧文档失败: {}", e)))?
                .map(|g| g.value().to_vec());
            let removed = table
                .remove(&key[..])
                .map_err(|e| StorageError::Other(format!("删除文档失败: {}", e)))?;

            if !defs.is_empty() {
                let mut data = write_tx
                    .open_table(INDEX_DATA_TABLE)
                    .map_err(|e| StorageError::Other(format!("打开索引数据表失败: {}", e)))?;
                remove_index_entries(&mut data, &defs, collection, doc_id, old_bytes.as_deref())?;
            }
            removed.is_some()
        };
        write_tx
            .commit()
            .map_err(|e| StorageError::Other(format!("提交写入事务失败: {}", e)))?;
        Ok(deleted)
    }

    fn update_many(
        &self,
        collection: &str,
        docs: &[(String, Vec<u8>)],
    ) -> Result<(), StorageError> {
        // 1. 校验集合名
        validate_collection(collection)?;

        // 2. 空批次直接成功(避免无意义地开事务)
        if docs.is_empty() {
            return Ok(());
        }

        // 索引定义必须在写事务之前读取(redb 不允许写事务期间开读事务)
        let defs = self.list_indexes(collection)?;

        // 3. 单写事务内覆盖写入全部文档,一次性提交
        let write_tx = self
            .db
            .begin_write()
            .map_err(|e| StorageError::Other(format!("启动写入事务失败: {}", e)))?;
        {
            let mut table = write_tx
                .open_table(DOCS_TABLE)
                .map_err(|e| StorageError::Other(format!("打开文档表失败: {}", e)))?;
            for (doc_id, bytes) in docs {
                let key = build_doc_key(collection, doc_id);
                let old_bytes = table
                    .get(&key[..])
                    .map_err(|e| StorageError::Other(format!("读取旧文档失败: {}", e)))?
                    .map(|g| g.value().to_vec());
                table
                    .insert(&key[..], &bytes[..])
                    .map_err(|e| StorageError::Other(format!("更新文档失败: {}", e)))?;
                if !defs.is_empty() {
                    let mut data = write_tx
                        .open_table(INDEX_DATA_TABLE)
                        .map_err(|e| StorageError::Other(format!("打开索引数据表失败: {}", e)))?;
                    remove_index_entries(
                        &mut data,
                        &defs,
                        collection,
                        doc_id,
                        old_bytes.as_deref(),
                    )?;
                    write_index_entries(&mut data, &defs, collection, doc_id, bytes)?;
                }
            }
        }
        write_tx
            .commit()
            .map_err(|e| StorageError::Other(format!("提交写入事务失败: {}", e)))
    }

    fn delete_many(&self, collection: &str, doc_ids: &[String]) -> Result<(), StorageError> {
        // 1. 校验集合名
        validate_collection(collection)?;

        // 2. 空批次直接成功
        if doc_ids.is_empty() {
            return Ok(());
        }

        // 索引定义必须在写事务之前读取(redb 不允许写事务期间开读事务)
        let defs = self.list_indexes(collection)?;

        // 3. 单写事务内删除全部主键,一次性提交
        let write_tx = self
            .db
            .begin_write()
            .map_err(|e| StorageError::Other(format!("启动写入事务失败: {}", e)))?;
        {
            let mut table = write_tx
                .open_table(DOCS_TABLE)
                .map_err(|e| StorageError::Other(format!("打开文档表失败: {}", e)))?;
            for doc_id in doc_ids {
                let key = build_doc_key(collection, doc_id);
                let old_bytes = table
                    .get(&key[..])
                    .map_err(|e| StorageError::Other(format!("读取旧文档失败: {}", e)))?
                    .map(|g| g.value().to_vec());
                table
                    .remove(&key[..])
                    .map_err(|e| StorageError::Other(format!("删除文档失败: {}", e)))?;
                if !defs.is_empty() {
                    let mut data = write_tx
                        .open_table(INDEX_DATA_TABLE)
                        .map_err(|e| StorageError::Other(format!("打开索引数据表失败: {}", e)))?;
                    remove_index_entries(
                        &mut data,
                        &defs,
                        collection,
                        doc_id,
                        old_bytes.as_deref(),
                    )?;
                }
            }
        }
        write_tx
            .commit()
            .map_err(|e| StorageError::Other(format!("提交写入事务失败: {}", e)))
    }

    fn list_collections(&self) -> Result<Vec<String>, StorageError> {
        // 读取事务 + 打开文档表
        let read_tx = self
            .db
            .begin_read()
            .map_err(|e| StorageError::Other(format!("启动读取事务失败: {}", e)))?;
        let table = match read_tx.open_table(DOCS_TABLE) {
            Ok(table) => table,
            // 文档表尚不存在 → 空库
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => return Err(StorageError::Other(format!("打开文档表失败: {}", e))),
        };

        // 遍历全部 key,按 `集合名(0x00)` 前缀提取集合名并去重(B+树按 key 有序,
        // 相同集合的 key 相邻,因此只需与上一个集合名比较即可去重)。
        let mut collections: Vec<String> = Vec::new();
        let range = table
            .iter()
            .map_err(|e| StorageError::Other(format!("遍历文档表失败: {}", e)))?;
        for entry in range {
            let (key, _value) =
                entry.map_err(|e| StorageError::Other(format!("读取文档失败: {}", e)))?;
            let key_bytes = key.value();
            // 找到分隔符 0x00,其前部分即为集合名
            let sep_pos = match key_bytes.iter().position(|b| *b == KEY_SEPARATOR) {
                Some(p) => p,
                // 理论上不会发生:key 始终由 build_doc_key 构造,必含分隔符
                None => continue,
            };
            let name = String::from_utf8_lossy(&key_bytes[..sep_pos]).into_owned();
            // 集合名不得为空(防御性跳过)
            if name.is_empty() {
                continue;
            }
            // 去重:与上一个集合名不同才追加
            if collections.last() != Some(&name) {
                collections.push(name);
            }
        }
        Ok(collections)
    }

    fn drop_collection(&self, collection: &str) -> Result<usize, StorageError> {
        // 1. 校验集合名
        validate_collection(collection)?;

        // 2. 构造前缀
        let prefix = build_doc_key(collection, "");

        // 3. 写事务:遍历该前缀并逐个删除
        let write_tx = self
            .db
            .begin_write()
            .map_err(|e| StorageError::Other(format!("启动写入事务失败: {}", e)))?;
        let mut dropped = 0usize;
        {
            let mut table = write_tx
                .open_table(DOCS_TABLE)
                .map_err(|e| StorageError::Other(format!("打开文档表失败: {}", e)))?;
            // 收集待删除的 key(避免在迭代中直接修改表)
            let mut keys_to_delete: Vec<Vec<u8>> = Vec::new();
            let range = table
                .range(prefix.as_slice()..)
                .map_err(|e| StorageError::Other(format!("遍历文档表失败: {}", e)))?;
            for entry in range {
                let (key, _value) =
                    entry.map_err(|e| StorageError::Other(format!("读取文档失败: {}", e)))?;
                let key_bytes = key.value();
                if !key_bytes.starts_with(&prefix) {
                    break;
                }
                keys_to_delete.push(key_bytes.to_vec());
            }
            // 逐个删除
            for key in &keys_to_delete {
                let removed = table
                    .remove(key.as_slice())
                    .map_err(|e| StorageError::Other(format!("删除文档失败: {}", e)))?;
                if removed.is_some() {
                    dropped += 1;
                }
            }
            // 顺带清理该集合的全部索引定义与条目(M26)。
            // 索引表 key 前缀同样是 `集合名\0`,与文档表的布局一致。
            let idx_prefix = {
                let mut k = Vec::new();
                k.extend_from_slice(collection.as_bytes());
                k.push(0);
                k
            };
            let mut meta = write_tx
                .open_table(INDEX_META_TABLE)
                .map_err(|e| StorageError::Other(format!("打开索引定义表失败: {}", e)))?;
            let mut data = write_tx
                .open_table(INDEX_DATA_TABLE)
                .map_err(|e| StorageError::Other(format!("打开索引数据表失败: {}", e)))?;

            let mut stale_meta: Vec<Vec<u8>> = Vec::new();
            let mut stale_data: Vec<Vec<u8>> = Vec::new();
            {
                let range = meta
                    .range(idx_prefix.as_slice()..)
                    .map_err(|e| StorageError::Other(format!("遍历索引定义失败: {}", e)))?;
                for entry in range {
                    let (k, _) = entry
                        .map_err(|e| StorageError::Other(format!("读取索引定义失败: {}", e)))?;
                    let kb = k.value();
                    if !kb.starts_with(&idx_prefix) {
                        break;
                    }
                    stale_meta.push(kb.to_vec());
                }
            }
            {
                let range = data
                    .range(idx_prefix.as_slice()..)
                    .map_err(|e| StorageError::Other(format!("遍历索引条目失败: {}", e)))?;
                for entry in range {
                    let (k, _) = entry
                        .map_err(|e| StorageError::Other(format!("读取索引条目失败: {}", e)))?;
                    let kb = k.value();
                    if !kb.starts_with(&idx_prefix) {
                        break;
                    }
                    stale_data.push(kb.to_vec());
                }
            }
            for k in &stale_meta {
                meta.remove(k.as_slice())
                    .map_err(|e| StorageError::Other(format!("删除索引定义失败: {}", e)))?;
            }
            for k in &stale_data {
                data.remove(k.as_slice())
                    .map_err(|e| StorageError::Other(format!("删除索引条目失败: {}", e)))?;
            }
        }
        write_tx
            .commit()
            .map_err(|e| StorageError::Other(format!("提交写入事务失败: {}", e)))?;
        Ok(dropped)
    }
}
