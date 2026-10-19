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
    /// 字段点号路径列表。单字段索引为 1 个元素;复合索引按顺序排列。
    ///
    /// 复合索引遵循 **MongoDB 的前缀原则**:`{a,b}` 可服务 `{a}` 查询,反之不行。
    pub fields: Vec<String>,
    /// 是否唯一索引。
    pub unique: bool,
    /// 是否稀疏索引(缺失/null 不入索引)。
    pub sparse: bool,
    /// 编码版本(与当前 `INDEX_ENCODING_VERSION` 不一致时需要重建)。
    pub encoding_version: u32,
}

impl IndexDef {
    /// 构造一个单字段索引定义。
    pub fn new(collection: &str, name: &str, field: &str, unique: bool, sparse: bool) -> Self {
        Self::new_composite(collection, name, &[field], unique, sparse)
    }

    /// 构造一个复合索引定义。
    pub fn new_composite(
        collection: &str,
        name: &str,
        fields: &[&str],
        unique: bool,
        sparse: bool,
    ) -> Self {
        IndexDef {
            collection: collection.to_string(),
            name: name.to_string(),
            fields: fields.iter().map(|s| s.to_string()).collect(),
            unique,
            sparse,
            encoding_version: crate::engine::redb_engine::INDEX_ENCODING_VERSION,
        }
    }

    /// 单字段索引的字段路径(复合索引返回第一个)。
    ///
    /// 保留该访问器是为了让既有调用点平滑过渡。
    pub fn field(&self) -> &str {
        self.fields.first().map(|s| s.as_str()).unwrap_or("")
    }

    /// 是否为复合索引。
    pub fn is_composite(&self) -> bool {
        self.fields.len() > 1
    }

    /// 字段路径拼接串(用于构造条目 key 的字段段)。
    ///
    /// 复合索引用 `,` 连接;单字段就是自身。该串唯一标识「哪些字段、什么顺序」。
    pub fn field_signature(&self) -> String {
        self.fields.join(",")
    }

    /// 序列化为存储用的 JSON。
    pub fn to_json(&self) -> Vec<u8> {
        let v = json!({
            "fields": self.fields,
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
        // 兼容旧格式(单字段 `field`)与新增的 `fields` 数组
        let fields: Vec<String> = match obj.get("fields") {
            Some(Value::Array(arr)) => arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect(),
            // 兼容旧格式(单字段 `field`)
            _ => vec![obj.get("field").and_then(|v| v.as_str())?.to_string()],
        };
        if fields.is_empty() {
            return None;
        }
        Some(IndexDef {
            collection: collection.to_string(),
            name: name.to_string(),
            fields,
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

/// 构造索引条目 key:
/// `集合名\0字段签名\0[段编码]...[段编码][总编码长度]doc_id`
///
/// **字段签名** = 字段路径用 `,` 连接(如 `a,b.c`),唯一标识索引的字段与顺序。
///
/// **段编码**:每个字段的值 order-preserving 编码后**依次拼接**。由于
/// `encode_key` 的输出自定界(首字节是类型等级),拼接后的字节序等价于
/// 「按字段顺序逐级比较」—— 这正是 MongoDB 复合索引的顺序语义。
///
/// **长度与值序的关系(v0.5.1 教训)**:长度信息一律**不得**插在编码值之前,
/// 否则 B+树按字节比较时会破坏值序。这里把**各段长度**放在所有编码值之后,
/// 使排序完全由编码值字节决定。
pub(crate) fn build_entry_key(
    collection: &str,
    field_signature: &str,
    encoded_values: &[Vec<u8>],
    doc_id: &str,
) -> Vec<u8> {
    let total: usize = encoded_values.iter().map(|v| v.len()).sum();
    let mut key = Vec::with_capacity(
        collection.len()
            + field_signature.len()
            + total
            + 4 * encoded_values.len()
            + doc_id.len()
            + 2,
    );
    key.extend_from_slice(collection.as_bytes());
    key.push(0);
    key.extend_from_slice(field_signature.as_bytes());
    key.push(0);
    // 1. 全部编码值依次拼接(决定排序)
    for v in encoded_values {
        key.extend_from_slice(v);
    }
    // 2. 各段长度(定界用,不参与排序)
    for v in encoded_values {
        key.extend_from_slice(&(v.len() as u32).to_be_bytes());
    }
    // 3. doc_id
    key.extend_from_slice(doc_id.as_bytes());
    key
}

/// 集合+字段签名的条目前缀(用于扫描某索引的全部条目)。
pub(crate) fn build_entry_prefix(collection: &str, field_signature: &str) -> Vec<u8> {
    let mut key = Vec::with_capacity(collection.len() + field_signature.len() + 2);
    key.extend_from_slice(collection.as_bytes());
    key.push(0);
    key.extend_from_slice(field_signature.as_bytes());
    key.push(0);
    key
}

/// 从条目 key 的剩余部分切出 `(全部编码值, doc_id 字节)`。
///
/// 布局:`enc1..encN(总长 T) | len1..lenN(各 4 字节) | doc_id`。
///
/// 切分思路:遍历候选总长 `t`,读出 `segments` 个长度并校验其和等于 `t`,
/// 且每个长度 > 0。编码值总长很小(<= 256),遍历开销可忽略。
pub fn split_entry_multi(rest: &[u8], segments: usize) -> Option<(&[u8], &[u8])> {
    if segments == 0 {
        return None;
    }
    let lens_bytes = 4 * segments;
    if rest.len() < lens_bytes + 1 {
        return None;
    }
    let max_t = rest.len() - lens_bytes;
    for t in 1..=max_t {
        let mut sum = 0usize;
        let mut ok = true;
        for i in 0..segments {
            let off = t + i * 4;
            if off + 4 > rest.len() {
                ok = false;
                break;
            }
            let len = u32::from_be_bytes(rest[off..off + 4].try_into().ok()?) as usize;
            if len == 0 {
                ok = false;
                break;
            }
            sum += len;
        }
        if ok && sum == t {
            return Some((&rest[..t], &rest[t + lens_bytes..]));
        }
    }
    None
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

/// 计算一条文档在某个索引下的**全部条目编码值**。
///
/// 返回 `Vec<Vec<Vec<u8>>>`:外层是「多条目」(multikey 展开的结果),
/// 内层是「该条目各字段的编码值」(复合索引有多个)。
///
/// - 稀疏索引且任一字段为 null/缺失 → 返回空(不入索引);
/// - 任一字段是数组 → 按 multikey 展开(笛卡尔积,超过上限则整体不入索引);
/// - 其余 → 单条。
pub fn entry_values(def: &IndexDef, doc: &Map<String, Value>) -> Vec<Vec<Vec<u8>>> {
    // 每个字段的候选值列表(数组字段已展开)
    let mut per_field: Vec<Vec<Vec<u8>>> = Vec::with_capacity(def.fields.len());

    for field in &def.fields {
        let value = field_value(doc, field);
        if def.sparse && !is_indexable(value) {
            return Vec::new();
        }
        // multikey:数组字段每个元素一条
        let vals = encode_index_values(value);
        if vals.is_empty() {
            // 空数组:该字段无候选值,整条文档不入索引
            return Vec::new();
        }
        per_field.push(vals);
    }

    // 笛卡尔积(单字段时就是它自己)
    let mut combos: Vec<Vec<Vec<u8>>> = vec![Vec::new()];
    for field_vals in per_field {
        let mut next = Vec::with_capacity(combos.len() * field_vals.len());
        for combo in &combos {
            for v in &field_vals {
                let mut c = combo.clone();
                c.push(v.clone());
                next.push(c);
            }
        }
        // 上限保护:复合 multikey 的笛卡尔积可能爆炸
        const MAX_COMBOS: usize = 128;
        if next.len() > MAX_COMBOS {
            return Vec::new();
        }
        combos = next;
    }
    combos
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

    /// 条目 key 中的编码值含 0x00 时也能无歧义解析(长度后缀的意义)。
    #[test]
    fn entry_key_handles_zero_bytes() {
        let key = build_entry_key("c", "f", &[vec![0x02, 0x00, 0x01]], "doc1");
        let prefix = build_entry_prefix("c", "f");
        assert!(key.starts_with(&prefix));
        // 编码值在前,长度后缀在后
        let rest = &key[prefix.len()..];
        assert_eq!(&rest[0..3], &[0x02, 0x00, 0x01]);
        let len = u32::from_be_bytes(rest[3..7].try_into().unwrap()) as usize;
        assert_eq!(len, 3);
        assert_eq!(&rest[7..], b"doc1");
        // 解析回来应一致
        let (enc, doc) = split_entry_multi(rest, 1).unwrap();
        assert_eq!(enc, &[0x02, 0x00, 0x01]);
        assert_eq!(doc, b"doc1");
    }

    /// 长度后缀不得影响值序:不同编码长度的值仍按编码字节排序。
    #[test]
    fn length_suffix_preserves_value_order() {
        // number 编码 [0x02,...](长 9)应排在 bool [0x08,0x01](长 2)之前
        let num_key = build_entry_key("c", "f", &[vec![0x02, 0xc0, 0, 0, 0, 0, 0, 0, 0]], "docB");
        let bool_key = build_entry_key("c", "f", &[vec![0x08, 0x01]], "docA");
        assert!(
            num_key < bool_key,
            "number 应排在 bool 前:长度后缀不能破坏值序"
        );
    }

    /// 复合索引:多段编码依次拼接,顺序即字段顺序。
    #[test]
    fn composite_entry_key_order() {
        let a1b1 = build_entry_key("c", "a,b", &[vec![0x02, 0x01], vec![0x02, 0x01]], "d1");
        let a1b2 = build_entry_key("c", "a,b", &[vec![0x02, 0x01], vec![0x02, 0x02]], "d2");
        let a2b1 = build_entry_key("c", "a,b", &[vec![0x02, 0x02], vec![0x02, 0x01]], "d3");
        // 先按 a 排,再按 b 排
        assert!(a1b1 < a1b2, "a 相同则比 b");
        assert!(a1b2 < a2b1, "a 不同则先比 a");

        // 解析回来
        let prefix = build_entry_prefix("c", "a,b");
        let rest = &a1b2[prefix.len()..];
        let (enc, doc) = split_entry_multi(rest, 2).unwrap();
        assert_eq!(enc, &[0x02, 0x01, 0x02, 0x02]);
        assert_eq!(doc, b"d2");
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
        assert_eq!(
            vals[0],
            vec![crate::index::key::encode_key(Some(&json!(42)))]
        );
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
