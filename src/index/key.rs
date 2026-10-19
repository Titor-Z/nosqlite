//! 索引键编码(M23b):order-preserving encoding。
//!
//! # 为什么不直接复用 BSON 字节
//!
//! BSON 的 `int32`/`int64` 是**小端序**,字节序 ≠ 数值序(负数尤其错乱),
//! 无法直接做 B+树 range 扫描。MongoDB 实际在 WiredTiger 内部也是先做
//! order-preserving 编码再存索引键。
//!
//! # 编码规则
//!
//! 首字节是类型等级,直接来自 [`crate::query::compare::rank`] —— 这是**单一真相源**:
//! 比较与编码必须用同一套等级,否则「走索引」与「全表扫」会给出不同结果。
//!
//! | 类型 | 首字节 | 后续编码 |
//! |---|---|---|
//! | MinKey | `0x00` | 无 |
//! | Null | `0x01` | 无 |
//! | Number | `0x02` | f64 的 IEEE754 位翻转(大端) |
//! | String | `0x03` | UTF-8 原字节 |
//! | Object | `0x04` | 键值对序列 |
//! | Array | `0x05` | 元素序列 |
//! | Binary | `0x06` | 子类型 + 数据 |
//! | ObjectId | `0x07` | 12 字节 |
//! | Bool | `0x08` | 0x00 / 0x01 |
//! | Date | `0x09` | i64 + 2^63(大端),使负数也可排序 |
//! | Timestamp | `0x0A` | t 与 i 各 4 字节大端 |
//! | Regex | `0x0B` | pattern + 0x00 + options |
//! | MaxKey | `0x0C` | 无 |
//!
//! # 正确性保证
//!
//! `tests/index_encoding.rs` 锁死:
//! 「对任意两个值,`compare_values(a, b)` 的结果 == `encode(a).cmp(encode(b))`」。

use serde_json::Value;

use crate::query::compare::rank;
use crate::query::{as_date, as_oid, as_timestamp, type_rank};

/// 把 JSON 值编码为可用于 B+树 range 扫描的字节串(字节序 == 值序)。
pub fn encode_key(value: Option<&Value>) -> Vec<u8> {
    let mut out = Vec::with_capacity(16);
    encode_into(&mut out, value);
    out
}

/// 递归编码到给定缓冲区。
fn encode_into(out: &mut Vec<u8>, value: Option<&Value>) {
    let Some(v) = value else {
        // 缺失按 null 处理(与 compare_values 一致)
        out.push(rank::NULL);
        return;
    };

    // 特殊字面量优先(它们的表示是对象,不能落进 Object 分支)
    if let Some((t, i)) = as_timestamp(v) {
        out.push(rank::TIMESTAMP);
        out.extend_from_slice(&t.to_be_bytes());
        out.extend_from_slice(&i.to_be_bytes());
        return;
    }
    if let Some(ms) = as_date(v) {
        out.push(rank::DATE);
        // 偏移 2^63:使 i64 的有符号范围映射到 u64 单调区间
        out.extend_from_slice(&((ms as u64) ^ (1u64 << 63)).to_be_bytes());
        return;
    }
    if let Some(oid) = as_oid(v) {
        // 与 compare_values 保持一致:`$oid` 归一为**字符串**参与比较(M23a 的契约)。
        // 若这里用 OBJECT_ID 等级,索引序会与比较序分叉,范围查询走索引即出错。
        out.push(rank::STRING);
        out.extend_from_slice(oid.as_bytes());
        return;
    }
    if let Some(obj) = single_key(v, "$minKey") {
        let _ = obj;
        out.push(rank::MIN_KEY);
        return;
    }
    if let Some(obj) = single_key(v, "$maxKey") {
        let _ = obj;
        out.push(rank::MAX_KEY);
        return;
    }
    if let Some(inner) = single_key(v, "$regularExpression") {
        out.push(rank::REGEX);
        if let Some(m) = inner.as_object() {
            let pattern = m.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
            let options = m.get("options").and_then(|v| v.as_str()).unwrap_or("");
            out.extend_from_slice(pattern.as_bytes());
            out.push(0); // 分隔,避免 "a" + "bc" 与 "ab" + "c" 混淆
            out.extend_from_slice(options.as_bytes());
        }
        return;
    }
    if let Some(inner) = single_key(v, "$binary") {
        out.push(rank::BINARY);
        if let Some(m) = inner.as_object() {
            let sub = m.get("subType").and_then(|v| v.as_str()).unwrap_or("00");
            out.push(u8::from_str_radix(sub, 16).unwrap_or(0));
            let data = m.get("base64").and_then(|v| v.as_str()).unwrap_or("");
            out.extend_from_slice(data.as_bytes());
        }
        return;
    }
    // Extended JSON 数字归一
    if let Some(n) = crate::query::as_number_long(v) {
        out.push(rank::NUMBER);
        encode_f64(out, n.as_f64().unwrap_or(0.0));
        return;
    }
    if let Some(n) = crate::query::as_number_double(v) {
        out.push(rank::NUMBER);
        encode_f64(out, n.as_f64().unwrap_or(0.0));
        return;
    }

    // 基础类型
    match v {
        Value::Null => out.push(rank::NULL),
        Value::Bool(b) => {
            out.push(rank::BOOL);
            out.push(if *b { 1 } else { 0 });
        }
        Value::Number(n) => {
            out.push(rank::NUMBER);
            encode_f64(out, n.as_f64().unwrap_or(f64::NAN));
        }
        Value::String(s) => {
            out.push(rank::STRING);
            out.extend_from_slice(s.as_bytes());
        }
        Value::Array(items) => {
            out.push(rank::ARRAY);
            for item in items {
                // 每个元素长度前缀,避免拼接歧义
                let mut sub = Vec::new();
                encode_into(&mut sub, Some(item));
                out.extend_from_slice(&(sub.len() as u32).to_be_bytes());
                out.extend_from_slice(&sub);
            }
            // 短数组排在长数组前:与 compare_values 的「公共前缀相等时短者胜」一致
            out.push(0);
        }
        Value::Object(map) => {
            out.push(rank::OBJECT);
            for (k, val) in map {
                out.extend_from_slice(k.as_bytes());
                out.push(0); // 键分隔
                let mut sub = Vec::new();
                encode_into(&mut sub, Some(val));
                out.extend_from_slice(&(sub.len() as u32).to_be_bytes());
                out.extend_from_slice(&sub);
            }
            out.push(0);
        }
    }
}

/// 编码 f64 为 order-preserving 的 8 字节。
///
/// 规则:符号位为 0(正数)时翻最高位;符号位为 1(负数)时全体取反。
/// 结果为大端字节序,可直接按字节比较。
fn encode_f64(out: &mut Vec<u8>, value: f64) {
    let bits = value.to_bits();
    let ordered = if bits & (1u64 << 63) != 0 {
        // 负数:全体取反
        !bits
    } else {
        // 正数:翻最高位
        bits ^ (1u64 << 63)
    };
    out.extend_from_slice(&ordered.to_be_bytes());
}

/// 取「恰好只有一个指定键」的包装对象内的值。
fn single_key<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    let obj = value.as_object()?;
    if obj.len() != 1 {
        return None;
    }
    obj.get(key)
}

/// 编码一个字段值列表(用于 multikey 索引:数组元素各生成一个键)。
///
/// 返回的每个元素都是一个独立索引条目值;非数组值返回单元素。
pub fn encode_index_values(value: Option<&Value>) -> Vec<Vec<u8>> {
    match value {
        // 数组字段:每个元素各一条(multikey)
        Some(Value::Array(items)) => items.iter().map(|v| encode_key(Some(v))).collect(),
        other => vec![encode_key(other)],
    }
}

/// 该值是否应进入索引(稀疏索引用:null/缺失不入)。
pub fn is_indexable(value: Option<&Value>) -> bool {
    match value {
        None => false,
        Some(Value::Null) => false,
        Some(v) => type_rank(Some(v)) != rank::NULL,
    }
}
