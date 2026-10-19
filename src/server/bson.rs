//! BSON 编解码(阶段 2)。
//!
//! MongoDB wire 协议的载荷是 **BSON**。本模块负责 BSON ↔ `serde_json::Value` 互转,
//! 以便复用已有的存储层与查询语义。
//!
//! # 类型映射
//!
//! | BSON 类型 | JSON 表示 |
//! |---|---|
//! | `double` | 数字(浮点) |
//! | `string` | 字符串 |
//! | `document` | 对象 |
//! | `array` | 数组 |
//! | `objectId` | 24 位 hex **字符串** —— 直接与存储层的主键一致 |
//! | `bool` / `null` | 同 JSON |
//! | `datetime` | `{"$date": 毫秒}`(M14 契约) |
//! | `int32` | 数字(整数) |
//! | `int64` | 数字;超出 `i32` 范围时与原值无损 |
//! | `timestamp` | `{"$timestamp": {"t": 秒, "i": 自增}}` |
//! | `binary` | `{"$binary": {"base64": "...", "subType": "00"}}` |
//! | `regex` | `{"$regularExpression": {"pattern": "...", "options": "..."}}` |
//! | `decimal128` | `{"$numberDecimal": "..."}`(保留原始字节) |
//! | `minKey` / `maxKey` | `{"$minKey": 1}` / `{"$maxKey": 1}` |
//!
//! `_id` 特例:编码时若字段名是 `_id` 且值为 24 位 hex 字符串,输出 BSON **objectId** 类型
//! (Go 驱动期望的类型);解码时 objectId 一律还原为 hex 字符串,与存储层无缝对接。

use serde_json::{Map, Value};

/// BSON 编解码错误。
#[derive(Debug)]
pub struct BsonError(pub String);

impl std::fmt::Display for BsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BSON 错误: {}", self.0)
    }
}

impl std::error::Error for BsonError {}

type Result<T> = std::result::Result<T, BsonError>;

// BSON 类型标记
const T_DOUBLE: u8 = 0x01;
const T_STRING: u8 = 0x02;
const T_DOCUMENT: u8 = 0x03;
const T_ARRAY: u8 = 0x04;
const T_BINARY: u8 = 0x05;
const T_UNDEFINED: u8 = 0x06;
const T_OBJECT_ID: u8 = 0x07;
const T_BOOL: u8 = 0x08;
const T_DATETIME: u8 = 0x09;
const T_NULL: u8 = 0x0A;
const T_REGEX: u8 = 0x0B;
const T_DBPOINTER: u8 = 0x0C;
const T_CODE: u8 = 0x0D;
const T_SYMBOL: u8 = 0x0E;
const T_CODE_W_SCOPE: u8 = 0x0F;
const T_INT32: u8 = 0x10;
const T_TIMESTAMP: u8 = 0x11;
const T_INT64: u8 = 0x12;
const T_DECIMAL128: u8 = 0x13;
const T_MAXKEY: u8 = 0x7F;
const T_MINKEY: u8 = 0xFF;

/// 编码一个 BSON 文档(含 4 字节长度前缀与结尾 0x00)。
pub fn encode_document(doc: &Map<String, Value>) -> Vec<u8> {
    let mut body = Vec::with_capacity(128);
    for (key, value) in doc {
        encode_element(&mut body, key, value);
    }
    body.push(0);

    let mut out = Vec::with_capacity(body.len() + 4);
    out.extend_from_slice(&((body.len() + 4) as i32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

/// 编码单个元素(类型字节 + 键名 + 值)。
fn encode_element(buf: &mut Vec<u8>, key: &str, value: &Value) {
    // `_id` 且为 24 位 hex 字符串 → BSON objectId(驱动期望的类型)
    if key == "_id"
        && let Value::String(s) = value
        && is_object_id_hex(s)
    {
        buf.push(T_OBJECT_ID);
        write_cstring(buf, key);
        buf.extend_from_slice(&hex_decode(s));
        return;
    }

    // Extended JSON 包装
    if let Value::Object(map) = value
        && map.len() == 1
        && let Some((tag, inner)) = map.iter().next()
    {
        match tag.as_str() {
            "$oid" => {
                if let Some(s) = inner.as_str().filter(|s| is_object_id_hex(s)) {
                    buf.push(T_OBJECT_ID);
                    write_cstring(buf, key);
                    buf.extend_from_slice(&hex_decode(s));
                    return;
                }
            }
            "$date" => {
                if let Some(ms) = date_millis(inner) {
                    buf.push(T_DATETIME);
                    write_cstring(buf, key);
                    buf.extend_from_slice(&ms.to_le_bytes());
                    return;
                }
            }
            "$numberLong" => {
                if let Some(n) = inner.as_str().and_then(|s| s.parse::<i64>().ok()) {
                    buf.push(T_INT64);
                    write_cstring(buf, key);
                    buf.extend_from_slice(&n.to_le_bytes());
                    return;
                }
            }
            "$numberDouble" => {
                if let Some(f) = inner.as_str().and_then(|s| s.parse::<f64>().ok()) {
                    buf.push(T_DOUBLE);
                    write_cstring(buf, key);
                    buf.extend_from_slice(&f.to_le_bytes());
                    return;
                }
            }
            "$minKey" => {
                buf.push(T_MINKEY);
                write_cstring(buf, key);
                return;
            }
            "$maxKey" => {
                buf.push(T_MAXKEY);
                write_cstring(buf, key);
                return;
            }
            "$timestamp" => {
                if let Some((t, i)) = timestamp_parts(inner) {
                    buf.push(T_TIMESTAMP);
                    write_cstring(buf, key);
                    buf.extend_from_slice(&i.to_le_bytes());
                    buf.extend_from_slice(&t.to_le_bytes());
                    return;
                }
            }
            _ => {}
        }
    }

    match value {
        Value::Null => {
            buf.push(T_NULL);
            write_cstring(buf, key);
        }
        Value::Bool(b) => {
            buf.push(T_BOOL);
            write_cstring(buf, key);
            buf.push(if *b { 1 } else { 0 });
        }
        // 扩展 JSON 的 `{"$numberLong": "0"}` 强制编码成 int64 ——
        // 游标 id 等字段驱动要求 64 位,普通整数会被写成 int32 导致驱动报错。
        Value::Object(m) if m.len() == 1 && m.contains_key("$numberLong") => {
            let i = m
                .get("$numberLong")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(0);
            buf.push(T_INT64);
            write_cstring(buf, key);
            buf.extend_from_slice(&i.to_le_bytes());
        }
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                // 整数优先 int32,超出范围用 int64
                if i >= i32::MIN as i64 && i <= i32::MAX as i64 {
                    buf.push(T_INT32);
                    write_cstring(buf, key);
                    buf.extend_from_slice(&(i as i32).to_le_bytes());
                } else {
                    buf.push(T_INT64);
                    write_cstring(buf, key);
                    buf.extend_from_slice(&i.to_le_bytes());
                }
            } else {
                buf.push(T_DOUBLE);
                write_cstring(buf, key);
                buf.extend_from_slice(&n.as_f64().unwrap_or(0.0).to_le_bytes());
            }
        }
        Value::String(s) => {
            buf.push(T_STRING);
            write_cstring(buf, key);
            write_string(buf, s);
        }
        Value::Array(arr) => {
            buf.push(T_ARRAY);
            write_cstring(buf, key);
            // 数组编码为「键为序号的文档」
            let mut body = Vec::new();
            for (i, item) in arr.iter().enumerate() {
                encode_element(&mut body, &i.to_string(), item);
            }
            body.push(0);
            buf.extend_from_slice(&((body.len() + 4) as i32).to_le_bytes());
            buf.extend_from_slice(&body);
        }
        Value::Object(map) => {
            buf.push(T_DOCUMENT);
            write_cstring(buf, key);
            let inner = encode_document(map);
            buf.extend_from_slice(&inner);
        }
    }
}

/// 写 C 风格字符串(UTF-8 + 0x00 结尾)。
fn write_cstring(buf: &mut Vec<u8>, s: &str) {
    buf.extend_from_slice(s.as_bytes());
    buf.push(0);
}

/// 写 BSON string(4 字节长度 + UTF-8 + 0x00)。
fn write_string(buf: &mut Vec<u8>, s: &str) {
    buf.extend_from_slice(&((s.len() + 1) as i32).to_le_bytes());
    buf.extend_from_slice(s.as_bytes());
    buf.push(0);
}

/// 解码一个 BSON 文档(含长度前缀)。
pub fn decode_document(bytes: &[u8]) -> Result<Map<String, Value>> {
    let (map, _) = decode_document_at(bytes, 0)?;
    Ok(map)
}

/// 从 `start` 位置解码文档,返回 (文档, 结束位置)。
fn decode_document_at(bytes: &[u8], start: usize) -> Result<(Map<String, Value>, usize)> {
    if start + 4 > bytes.len() {
        return Err(BsonError("文档长度不足".to_string()));
    }
    let len = i32::from_le_bytes(bytes[start..start + 4].try_into().unwrap());
    if len < 5 {
        return Err(BsonError(format!("文档长度非法: {}", len)));
    }
    let end = start + len as usize;
    if end > bytes.len() {
        return Err(BsonError("文档长度超出缓冲区".to_string()));
    }

    let mut map = Map::new();
    let mut pos = start + 4;
    while pos < end - 1 {
        let (key, value, next) = decode_element(bytes, pos, end)?;
        map.insert(key, value);
        pos = next;
    }
    Ok((map, end))
}

/// 解码单个元素,返回 (键, 值, 下一个位置)。
fn decode_element(bytes: &[u8], pos: usize, end: usize) -> Result<(String, Value, usize)> {
    let tag = bytes[pos];
    let (key, mut p) = read_cstring(bytes, pos + 1, end)?;

    let value = match tag {
        T_DOUBLE => {
            let f = f64::from_le_bytes(read_fixed::<8>(bytes, p, end)?);
            p += 8;
            number_from_f64(f)
        }
        T_STRING => {
            let s = read_string(bytes, &mut p, end)?;
            Value::String(s)
        }
        T_DOCUMENT => {
            let (m, next) = decode_document_at(bytes, p)?;
            p = next;
            Value::Object(m)
        }
        T_ARRAY => {
            let (m, next) = decode_document_at(bytes, p)?;
            p = next;
            // 数组编码为序号文档,按数值键排序还原
            let mut items: Vec<(usize, Value)> = m
                .into_iter()
                .map(|(k, v)| (k.parse::<usize>().unwrap_or(0), v))
                .collect();
            items.sort_by_key(|(i, _)| *i);
            Value::Array(items.into_iter().map(|(_, v)| v).collect())
        }
        T_BINARY => {
            let len = i32::from_le_bytes(read_fixed::<4>(bytes, p, end)?) as usize;
            p += 4;
            if p >= end {
                return Err(BsonError("binary 子类型缺失".to_string()));
            }
            let subtype = bytes[p];
            p += 1;
            if p + len > end {
                return Err(BsonError("binary 长度越界".to_string()));
            }
            let data = &bytes[p..p + len];
            p += len;
            let mut m = Map::new();
            let mut inner = Map::new();
            inner.insert("base64".to_string(), Value::String(base64_encode(data)));
            inner.insert(
                "subType".to_string(),
                Value::String(format!("{:02x}", subtype)),
            );
            m.insert("$binary".to_string(), Value::Object(inner));
            Value::Object(m)
        }
        T_UNDEFINED => Value::Null,
        T_OBJECT_ID => {
            let raw = read_fixed::<12>(bytes, p, end)?;
            p += 12;
            // 还原为 24 位 hex 字符串,与存储层主键一致
            Value::String(raw.iter().map(|b| format!("{:02x}", b)).collect())
        }
        T_BOOL => {
            if p >= end {
                return Err(BsonError("bool 越界".to_string()));
            }
            let b = bytes[p] != 0;
            p += 1;
            Value::Bool(b)
        }
        T_DATETIME => {
            let ms = i64::from_le_bytes(read_fixed::<8>(bytes, p, end)?);
            p += 8;
            let mut m = Map::new();
            m.insert("$date".to_string(), Value::from(ms));
            Value::Object(m)
        }
        T_NULL => Value::Null,
        T_REGEX => {
            let (pattern, next) = read_cstring(bytes, p, end)?;
            let (options, next2) = read_cstring(bytes, next, end)?;
            p = next2;
            let mut inner = Map::new();
            inner.insert("pattern".to_string(), Value::String(pattern));
            inner.insert("options".to_string(), Value::String(options));
            let mut m = Map::new();
            m.insert("$regularExpression".to_string(), Value::Object(inner));
            Value::Object(m)
        }
        T_DBPOINTER => {
            let s = read_string(bytes, &mut p, end)?;
            let _ = read_fixed::<12>(bytes, p, end)?;
            p += 12;
            Value::String(s)
        }
        T_CODE | T_SYMBOL => {
            let s = read_string(bytes, &mut p, end)?;
            Value::String(s)
        }
        T_CODE_W_SCOPE => {
            // 跳过总长度,取代码字符串,忽略作用域
            let total = i32::from_le_bytes(read_fixed::<4>(bytes, p, end)?) as usize;
            let scope_end = p + total;
            p += 4;
            let s = read_string(bytes, &mut p, end)?;
            p = scope_end;
            Value::String(s)
        }
        T_INT32 => {
            let i = i32::from_le_bytes(read_fixed::<4>(bytes, p, end)?);
            p += 4;
            Value::from(i)
        }
        T_TIMESTAMP => {
            let inc = u32::from_le_bytes(read_fixed::<4>(bytes, p, end)?);
            p += 4;
            let time = u32::from_le_bytes(read_fixed::<4>(bytes, p, end)?);
            p += 4;
            let mut inner = Map::new();
            inner.insert("t".to_string(), Value::from(time));
            inner.insert("i".to_string(), Value::from(inc));
            let mut m = Map::new();
            m.insert("$timestamp".to_string(), Value::Object(inner));
            Value::Object(m)
        }
        T_INT64 => {
            let i = i64::from_le_bytes(read_fixed::<8>(bytes, p, end)?);
            p += 8;
            Value::from(i)
        }
        T_DECIMAL128 => {
            let raw = read_fixed::<16>(bytes, p, end)?;
            p += 16;
            let mut m = Map::new();
            m.insert(
                "$numberDecimal".to_string(),
                Value::String(decimal128_to_string(&raw)),
            );
            Value::Object(m)
        }
        T_MAXKEY => {
            let mut m = Map::new();
            m.insert("$maxKey".to_string(), Value::from(1));
            Value::Object(m)
        }
        T_MINKEY => {
            let mut m = Map::new();
            m.insert("$minKey".to_string(), Value::from(1));
            Value::Object(m)
        }
        other => {
            return Err(BsonError(format!(
                "不支持的 BSON 类型标记: 0x{:02x}",
                other
            )));
        }
    };
    Ok((key, value, p))
}

/// 读取固定长度字节数组。
fn read_fixed<const N: usize>(bytes: &[u8], pos: usize, end: usize) -> Result<[u8; N]> {
    if pos + N > end {
        return Err(BsonError("读取越界".to_string()));
    }
    Ok(bytes[pos..pos + N].try_into().unwrap())
}

/// 读取 C 风格字符串,返回 (字符串, 下一个位置)。
fn read_cstring(bytes: &[u8], pos: usize, end: usize) -> Result<(String, usize)> {
    let start = pos;
    let mut p = pos;
    while p < end && bytes[p] != 0 {
        p += 1;
    }
    if p >= end {
        return Err(BsonError("C 字符串未终止".to_string()));
    }
    let s = std::str::from_utf8(&bytes[start..p])
        .map_err(|e| BsonError(format!("非法 UTF-8: {}", e)))?
        .to_string();
    Ok((s, p + 1))
}

/// 读取 BSON string(长度前缀 + 内容 + 结尾 0)。
fn read_string(bytes: &[u8], pos: &mut usize, end: usize) -> Result<String> {
    let len = i32::from_le_bytes(read_fixed::<4>(bytes, *pos, end)?) as usize;
    if len < 1 || *pos + 4 + len > end {
        return Err(BsonError("字符串长度非法".to_string()));
    }
    let s = std::str::from_utf8(&bytes[*pos + 4..*pos + 4 + len - 1])
        .map_err(|e| BsonError(format!("非法 UTF-8: {}", e)))?
        .to_string();
    *pos += 4 + len;
    Ok(s)
}

/// f64 → JSON 数字(整数优先,便于驱动侧得到整数类型)。
fn number_from_f64(f: f64) -> Value {
    if f.fract() == 0.0 && f.abs() < 9e15 {
        Value::from(f as i64)
    } else {
        serde_json::Number::from_f64(f).map_or(Value::Null, Value::Number)
    }
}

/// 判断是否为 24 位小写/大写 hex(ObjectId 字符串形式)。
fn is_object_id_hex(s: &str) -> bool {
    s.len() == 24 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// 24 位 hex → 12 字节。
fn hex_decode(s: &str) -> [u8; 12] {
    let mut out = [0u8; 12];
    for (i, chunk) in s.as_bytes().chunks(2).take(12).enumerate() {
        let hi = (chunk[0] as char).to_digit(16).unwrap_or(0);
        let lo = (chunk[1] as char).to_digit(16).unwrap_or(0);
        out[i] = ((hi << 4) | lo) as u8;
    }
    out
}

/// 取 `$date` 的毫秒值(数字或 ISO-8601 字符串)。
fn date_millis(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => crate::query::parse_iso8601(s),
        _ => None,
    }
}

/// 取 `$timestamp` 的 (秒, 自增)。
fn timestamp_parts(v: &Value) -> Option<(u32, u32)> {
    let m = v.as_object()?;
    let t = m.get("t")?.as_u64()? as u32;
    let i = m.get("i")?.as_u64()? as u32;
    Some((t, i))
}

/// 极简 base64 编码(binary 类型用,避免引入依赖)。
fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Decimal128 → 字符串(简化:十六进制表示;完整的十进制解析留待需要时)。
///
/// 绝大多数场景下驱动不会发送 decimal128;若收到,这里保证**不丢字节**,
/// 以十六进制字符串保留,避免静默改值。
fn decimal128_to_string(raw: &[u8]) -> String {
    format!(
        "0x{}",
        raw.iter().map(|b| format!("{:02x}", b)).collect::<String>()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 编解码往返:基础标量类型。
    #[test]
    fn roundtrip_scalars() {
        let doc = json!({
            "s": "hello",
            "i": 42,
            "big": 9_000_000_000i64,
            "f": 1.5,
            "b": true,
            "n": null,
        });
        let map = doc.as_object().unwrap().clone();
        let bytes = encode_document(&map);
        let back = decode_document(&bytes).expect("应能解码");
        assert_eq!(back["s"], json!("hello"));
        assert_eq!(back["i"], json!(42));
        assert_eq!(back["big"], json!(9_000_000_000i64));
        assert_eq!(back["f"], json!(1.5));
        assert_eq!(back["b"], json!(true));
        assert_eq!(back["n"], json!(null));
    }

    /// 嵌套文档与数组。
    #[test]
    fn roundtrip_nested() {
        let doc = json!({
            "a": {"b": {"c": [1, 2, {"d": "x"}]}},
            "arr": [true, null, 3.5],
        });
        let map = doc.as_object().unwrap().clone();
        let back = decode_document(&encode_document(&map)).expect("应能解码");
        assert_eq!(back["a"]["b"]["c"][2]["d"], json!("x"));
        assert_eq!(back["arr"], json!([true, null, 3.5]));
    }

    /// `_id` 为 24 位 hex 时必须编码成 BSON objectId(驱动期望的类型)。
    #[test]
    fn id_encodes_as_object_id() {
        let map = json!({"_id": "6ad54cd9bcd4e32cb134e111"})
            .as_object()
            .unwrap()
            .clone();
        let bytes = encode_document(&map);
        // 类型标记 0x07 应出现在 `_id` 之前
        let pos = bytes
            .windows(4)
            .position(|w| w == b"_id\0")
            .expect("应能找到 _id");
        assert_eq!(bytes[pos - 1], T_OBJECT_ID, "_id 应编码为 objectId");
        // 解码回来仍是同一个 hex 字符串
        let back = decode_document(&bytes).unwrap();
        assert_eq!(back["_id"], json!("6ad54cd9bcd4e32cb134e111"));
    }

    /// Date:编码为 BSON datetime,解码为 `{"$date": ms}`。
    #[test]
    fn roundtrip_date() {
        let map = json!({"at": {"$date": 1789524000000i64}})
            .as_object()
            .unwrap()
            .clone();
        let bytes = encode_document(&map);
        let back = decode_document(&bytes).unwrap();
        assert_eq!(back["at"]["$date"], json!(1789524000000i64));
    }

    /// Date 也可用 ISO 字符串输入(服务端归一为 datetime)。
    #[test]
    fn date_accepts_iso_string() {
        let map = json!({"at": {"$date": "2026-09-16T10:00:00Z"}})
            .as_object()
            .unwrap()
            .clone();
        let back = decode_document(&encode_document(&map)).unwrap();
        assert_eq!(back["at"]["$date"], json!(1789552800000i64));
    }

    /// `$numberLong` 显式编码为 int64。
    #[test]
    fn number_long_is_int64() {
        let map = json!({"n": {"$numberLong": "5"}})
            .as_object()
            .unwrap()
            .clone();
        let bytes = encode_document(&map);
        let pos = bytes.windows(2).position(|w| w == b"n\0").unwrap();
        assert_eq!(bytes[pos - 1], T_INT64);
        let back = decode_document(&bytes).unwrap();
        assert_eq!(back["n"], json!(5));
    }

    /// 空文档与空数组。
    #[test]
    fn empty_containers() {
        let map = json!({"d": {}, "a": []}).as_object().unwrap().clone();
        let back = decode_document(&encode_document(&map)).unwrap();
        assert_eq!(back["d"], json!({}));
        assert_eq!(back["a"], json!([]));
    }

    /// 损坏输入应报错而不是 panic。
    #[test]
    fn rejects_corrupt_input() {
        assert!(decode_document(&[]).is_err());
        assert!(decode_document(&[1, 0, 0, 0]).is_err());
        assert!(decode_document(&[0xff, 0xff, 0xff, 0x7f, 0]).is_err());
    }
}
