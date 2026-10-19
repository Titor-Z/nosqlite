//! 索引定义与索引条目的存储操作(M23b)。
//!
//! # 布局
//!
//! - **定义表** `nosqlite_index_meta`:`集合名\0索引名` → 定义 JSON
//! - **数据表** `nosqlite_index_data`:`集合名\0字段路径\0编码值\0doc_id` → 空
//!
//! # 为什么 doc_id 拼在 key 末尾
//!
//! 索引条目必须唯一,否则同一字段值上的第二条文档会覆盖第一条。把 `doc_id`
//! 拼在末尾既保证唯一,又让「相同索引值」的条目天然按 doc_id 有序。
//!
//! # multikey
//!
//! 数组字段的每个元素各生成一条条目(同一 doc 写 N 条),key 结构不变。
//! 这是 MongoDB 的 multikey 语义,从第一天就进模型,后续加查询时不必改格式。

use serde_json::{Map, Value, json};

use crate::index::key::{encode_index_values, is_indexable};

/// 索引定义。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexDef {
    /// 集合名。
    pub collection: String,
    /// 索引名(集合内唯一)。
    pub name: String,
    /// 字段点号路径(如 `a.b`)。
    pub field: String,
    /// 是否唯一索引。
    pub unique: bool,
    /// 是否稀疏索引(缺失/null 不入索引)。
    pub sparse: bool,
    /// 编码版本(与当前 `INDEX_ENCODING_VERSION` 不一致时需要重建)。
    pub encoding_version: u32,
}

impl IndexDef {
    /// 构造一个新定义(编码版本取当前值)。
    pub fn new(collection: &str, name: &str, field: &str, unique: bool, sparse: bool) -> Self {
        IndexDef {
            collection: collection.to_string(),
            name: name.to_string(),
            field: field.to_string(),
            unique,
            sparse,
            encoding_version: crate::engine::redb_engine::INDEX_ENCODING_VERSION,
        }
    }

    /// 序列化为存储用的 JSON。
    pub fn to_json(&self) -> Vec<u8> {
        let v = json!({
            "field": self.field,
            "unique": self.unique,
            "sparse": self.sparse,
            "encoding_version": self.encoding_version,
        });
        serde_json::to_vec(&v).unwrap_or_default()
    }

    /// 从存储的 JSON 还原。
    pub fn from_json(collection: &str, name: &str, bytes: &[u8]) -> Option<Self> {
        let v: Value = serde_json::from_slice(bytes).ok()?;
        let obj = v.as_object()?;
        Some(IndexDef {
            collection: collection.to_string(),
            name: name.to_string(),
            field: obj.get("field")?.as_str()?.to_string(),
            unique: obj.get("unique").and_then(|v| v.as_bool()).unwrap_or(false),
            sparse: obj.get("sparse").and_then(|v| v.as_bool()).unwrap_or(false),
            encoding_version: obj
                .get("encoding_version")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32,
        })
    }
}

/// 构造定义表 key:`集合名\0索引名`。
pub(crate) fn build_meta_key(collection: &str, name: &str) -> Vec<u8> {
    let mut key = Vec::with_capacity(collection.len() + 1 + name.len());
    key.extend_from_slice(collection.as_bytes());
    key.push(0);
    key.extend_from_slice(name.as_bytes());
    key
}

/// 构造集合的定义表前缀:`集合名\0`。
pub(crate) fn build_meta_prefix(collection: &str) -> Vec<u8> {
    let mut key = Vec::with_capacity(collection.len() + 1);
    key.extend_from_slice(collection.as_bytes());
    key.push(0);
    key
}

/// 构造索引条目 key:`集合名\0字段路径\0编码值\0doc_id`。
///
/// `encoded_value` 是已经 order-preserving 编码后的字节。
pub(crate) fn build_entry_key(
    collection: &str,
    field: &str,
    encoded_value: &[u8],
    doc_id: &str,
) -> Vec<u8> {
    let mut key =
        Vec::with_capacity(collection.len() + field.len() + encoded_value.len() + doc_id.len() + 3);
    key.extend_from_slice(collection.as_bytes());
    key.push(0);
    key.extend_from_slice(field.as_bytes());
    key.push(0);
    // 编码值本身可能含 0x00,故用长度前缀而非分隔符 —— 保证可无歧义还原
    key.extend_from_slice(&(encoded_value.len() as u32).to_be_bytes());
    key.extend_from_slice(encoded_value);
    key.extend_from_slice(doc_id.as_bytes());
    key
}

/// 集合的条目前缀:`集合名\0字段路径\0`(用于扫描某索引的全部条目)。
pub(crate) fn build_entry_prefix(collection: &str, field: &str) -> Vec<u8> {
    let mut key = Vec::with_capacity(collection.len() + field.len() + 2);
    key.extend_from_slice(collection.as_bytes());
    key.push(0);
    key.extend_from_slice(field.as_bytes());
    key.push(0);
    key
}

/// 按点号路径取文档中的值(不穿透数组,与索引维护语义一致)。
pub fn field_value<'a>(doc: &'a Map<String, Value>, path: &str) -> Option<&'a Value> {
    let mut segs = path.split('.');
    let first = segs.next()?;
    let mut cur = doc.get(first)?;
    for seg in segs {
        cur = cur.as_object()?.get(seg)?;
    }
    Some(cur)
}

/// 计算一条文档在某个索引下的**全部条目编码值**(multikey 展开)。
///
/// - 稀疏索引且值为 null/缺失 → 返回空(不入索引);
/// - 数组字段 → 每个元素一条;
/// - 其余 → 单条。
pub fn entry_values(def: &IndexDef, doc: &Map<String, Value>) -> Vec<Vec<u8>> {
    let value = field_value(doc, &def.field);
    if def.sparse && !is_indexable(value) {
        return Vec::new();
    }
    encode_index_values(value)
}

/// 判断两个条目值列表是否有交集(用于唯一索引冲突前置检查的辅助)。
pub fn has_duplicate(values: &[Vec<u8>]) -> bool {
    for (i, a) in values.iter().enumerate() {
        for b in values.iter().skip(i + 1) {
            if a == b {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 条目 key 中的编码值含 0x00 时也能无歧义解析(长度前缀的意义)。
    #[test]
    fn entry_key_handles_zero_bytes() {
        let key = build_entry_key("c", "f", &[0x02, 0x00, 0x01], "doc1");
        let prefix = build_entry_prefix("c", "f");
        assert!(key.starts_with(&prefix));
        // 前缀之后是长度
        let rest = &key[prefix.len()..];
        let len = u32::from_be_bytes(rest[0..4].try_into().unwrap()) as usize;
        assert_eq!(len, 3);
        assert_eq!(&rest[4..4 + len], &[0x02, 0x00, 0x01]);
        // 之后是 doc_id
        assert_eq!(&rest[4 + len..], b"doc1");
    }

    /// 定义 JSON 往返。
    #[test]
    fn index_def_roundtrip() {
        let def = IndexDef::new("users", "age_1", "age", true, false);
        let bytes = def.to_json();
        let back = IndexDef::from_json("users", "age_1", &bytes).unwrap();
        assert_eq!(def, back);
    }

    /// multikey:数组字段展开为多条条目。
    #[test]
    fn entry_values_multikey() {
        let def = IndexDef::new("c", "idx", "tags", false, false);
        let doc = json!({"tags": ["a", "b"]}).as_object().unwrap().clone();
        assert_eq!(entry_values(&def, &doc).len(), 2);

        let doc = json!({"tags": "a"}).as_object().unwrap().clone();
        assert_eq!(entry_values(&def, &doc).len(), 1);

        // 空数组不产生条目
        let doc = json!({"tags": []}).as_object().unwrap().clone();
        assert_eq!(entry_values(&def, &doc).len(), 0);
    }

    /// 稀疏索引跳过 null 与缺失。
    #[test]
    fn sparse_skips_null() {
        let sparse = IndexDef::new("c", "idx", "f", false, true);
        let normal = IndexDef::new("c", "idx2", "f", false, false);

        let doc = json!({"f": null}).as_object().unwrap().clone();
        assert_eq!(entry_values(&sparse, &doc).len(), 0);
        assert_eq!(entry_values(&normal, &doc).len(), 1, "非稀疏应索引 null");

        let doc = json!({"other": 1}).as_object().unwrap().clone();
        assert_eq!(entry_values(&sparse, &doc).len(), 0);
        assert_eq!(entry_values(&normal, &doc).len(), 1, "非稀疏应索引缺失");
    }

    /// 嵌套路径取值。
    #[test]
    fn nested_field_path() {
        let def = IndexDef::new("c", "idx", "a.b", false, false);
        let doc = json!({"a": {"b": 42}}).as_object().unwrap().clone();
        let vals = entry_values(&def, &doc);
        assert_eq!(vals.len(), 1);
        assert_eq!(vals[0], crate::index::key::encode_key(Some(&json!(42))));
    }

    /// 重复检测(唯一索引前置检查用)。
    #[test]
    fn duplicate_detection() {
        let a = vec![1u8, 2];
        assert!(has_duplicate(&[a.clone(), a.clone()]));
        assert!(!has_duplicate(&[vec![1, 2], vec![1, 3]]));
        assert!(!has_duplicate(&[]));
    }
}
