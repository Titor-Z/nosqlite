//! OP_MSG 帧编解码与命令分发(阶段 2)。
//!
//! # 帧结构
//!
//! ```text
//! +----------------+----------------+----------------+----------------+
//! | messageLength  |   requestID    |  responseTo    |    opCode      |  16 字节头
//! |    int32       |    int32       |    int32       |    int32       |
//! +----------------+----------------+----------------+----------------+
//! |                          OP_MSG 体 ...                              |
//! +-------------------------------------------------------------------+
//! ```
//!
//! OP_MSG(opCode 2013)体:
//!
//! ```text
//! flagBits(int32) | section0: kind=0x00 + BSON | [section1...]
//! ```
//!
//! 现代驱动只用 `section 0`(单文档)。本实现忽略 checksum 与 section 1。

use serde_json::{Map, Value};

use super::bson::{decode_document, encode_document};

/// 消息头长度。
pub const HEADER_LEN: usize = 16;
/// OP_MSG 操作码。
pub const OP_MSG: i32 = 2013;
/// OP_QUERY(旧驱动握手会用)。
pub const OP_QUERY: i32 = 2004;
/// OP_REPLY(对 OP_QUERY 的响应)。
pub const OP_REPLY: i32 = 1;

/// 解析后的请求。
#[derive(Debug)]
pub struct Request {
    /// 请求 ID(响应需回填到 `responseTo`)。
    pub request_id: i32,
    /// 命令文档。
    pub command: Map<String, Value>,
    /// 数据库名(来自 `$db`)。
    pub db: String,
    /// 命令名(命令文档的第一个键)。
    pub name: String,
    /// OP_MSG **section 1** 的文档序列,按 identifier 分组。
    ///
    /// 驱动把批量数据放在这里而非命令文档里,且不同命令用不同 identifier:
    ///
    /// | 命令 | identifier |
    /// |---|---|
    /// | `insert` | `documents` |
    /// | `update` | `updates` |
    /// | `delete` | `deletes` |
    ///
    /// 只读 section 0 会看到「插入 0 条 / 修改 0 条」。
    pub sequences: Vec<(String, Vec<Map<String, Value>>)>,
}

impl Request {
    /// 取指定 identifier 的文档序列。
    pub fn sequence(&self, identifier: &str) -> &[Map<String, Value>] {
        self.sequences
            .iter()
            .find(|(name, _)| name == identifier)
            .map(|(_, docs)| docs.as_slice())
            .unwrap_or(&[])
    }
}

/// 把 16 字节头 + BSON 命令体组装成 OP_MSG 响应帧。
///
/// `response_to` 应对应请求的 `request_id`。
pub fn build_op_msg_response(
    request_id: i32,
    response_to: i32,
    doc: &Map<String, Value>,
) -> Vec<u8> {
    let bson = encode_document(doc);
    let total = HEADER_LEN + 4 + 1 + bson.len();
    let mut out = Vec::with_capacity(total);

    out.extend_from_slice(&(total as i32).to_le_bytes());
    out.extend_from_slice(&request_id.to_le_bytes());
    out.extend_from_slice(&response_to.to_le_bytes());
    out.extend_from_slice(&OP_MSG.to_le_bytes());

    // flagBits = 0
    out.extend_from_slice(&0i32.to_le_bytes());
    // section kind = 0(单文档)
    out.push(0);
    out.extend_from_slice(&bson);
    out
}

/// 解析一个 OP_MSG 请求。
///
/// 返回 `None` 表示该帧不含可用的命令文档(如仅 OP_QUERY 握手,由调用方另行处理)。
pub fn parse_op_msg(op_code: i32, body: &[u8], request_id: i32) -> Result<Option<Request>, String> {
    if op_code != OP_MSG {
        return Ok(None);
    }
    if body.len() < 5 {
        return Err("OP_MSG 体过短".to_string());
    }
    // 跳过 flagBits(4 字节)
    let mut pos = 4usize;
    let mut command: Option<Map<String, Value>> = None;
    let mut sequences: Vec<(String, Vec<Map<String, Value>>)> = Vec::new();

    while pos < body.len() {
        let kind = body[pos];
        pos += 1;
        match kind {
            0 => {
                // section 0:单个 BSON 文档
                let doc = decode_document(&body[pos..]).map_err(|e| e.to_string())?;
                // 用文档自带的长度推进
                let len = i32::from_le_bytes(body[pos..pos + 4].try_into().unwrap()) as usize;
                pos += len;
                if command.is_none() {
                    command = Some(doc);
                }
            }
            1 => {
                // section 1:文档序列
                //   int32 size | cstring identifier | 若干 BSON 文档 ...
                let seq_start = pos;
                let size = i32::from_le_bytes(
                    body[pos..pos + 4]
                        .try_into()
                        .map_err(|_| "section 1 长度不足".to_string())?,
                ) as usize;
                pos += 4;
                // 读取 identifier(如 "documents" / "updates" / "deletes")
                let id_start = pos;
                while pos < body.len() && body[pos] != 0 {
                    pos += 1;
                }
                let identifier = String::from_utf8_lossy(&body[id_start..pos]).into_owned();
                pos += 1;
                // 逐条解码文档,直到本 section 结束
                let seq_end = seq_start + size;
                let mut docs: Vec<Map<String, Value>> = Vec::new();
                while pos + 4 <= seq_end && pos < body.len() {
                    let doc = decode_document(&body[pos..]).map_err(|e| e.to_string())?;
                    let len = i32::from_le_bytes(body[pos..pos + 4].try_into().unwrap()) as usize;
                    if len == 0 {
                        break;
                    }
                    pos += len;
                    docs.push(doc);
                }
                pos = seq_end;
                sequences.push((identifier, docs));
            }
            _ => break,
        }
    }

    let Some(command) = command else {
        return Ok(None);
    };
    let db = command
        .get("$db")
        .and_then(|v| v.as_str())
        .unwrap_or("admin")
        .to_string();
    // 命令名 = 第一个键(按 BSON 顺序,已由 preserve_order 保持)
    let name = command.keys().next().cloned().unwrap_or_default();

    Ok(Some(Request {
        request_id,
        command,
        db,
        name,
        sequences,
    }))
}

/// 构造标准成功响应 `{ok: 1, ...}`。
pub fn ok_doc(extra: impl IntoIterator<Item = (&'static str, Value)>) -> Map<String, Value> {
    let mut doc = Map::new();
    for (k, v) in extra {
        doc.insert(k.to_string(), v);
    }
    doc.insert("ok".to_string(), Value::from(1));
    doc
}

/// 构造错误响应 `{ok: 0, errmsg, code, codeName}`。
pub fn err_doc(code: i32, code_name: &str, message: &str) -> Map<String, Value> {
    let mut doc = Map::new();
    doc.insert("ok".to_string(), Value::from(0));
    doc.insert("errmsg".to_string(), Value::String(message.to_string()));
    doc.insert("code".to_string(), Value::from(code));
    doc.insert("codeName".to_string(), Value::String(code_name.to_string()));
    doc
}

// ==================== OP_QUERY 兼容(旧驱动/旧驱动握手) ====================

/// 解析 OP_QUERY(opCode 2004)以兼容老驱动的握手。
///
/// 结构:`flags(4) | fullCollectionName(cstring) | numberToSkip(4) | numberToReturn(4) | query(BSON)`
pub fn parse_op_query(body: &[u8]) -> Result<Option<Map<String, Value>>, String> {
    if body.len() < 4 + 1 + 4 + 4 + 5 {
        return Err("OP_QUERY 体过短".to_string());
    }
    let mut pos = 4usize;
    // fullCollectionName
    while pos < body.len() && body[pos] != 0 {
        pos += 1;
    }
    pos += 1; // 跳过 0
    pos += 8; // numberToSkip + numberToReturn
    if pos >= body.len() {
        return Err("OP_QUERY 查询文档缺失".to_string());
    }
    let doc = decode_document(&body[pos..]).map_err(|e| e.to_string())?;
    Ok(Some(doc))
}

/// 构造 OP_REPLY 响应(对 OP_QUERY 的回复)。
pub fn build_op_reply(request_id: i32, response_to: i32, doc: &Map<String, Value>) -> Vec<u8> {
    let bson = encode_document(doc);
    // 头部 16 + responseFlags 4 + cursorID 8 + startingFrom 4 + numberReturned 4 + 文档
    let total = HEADER_LEN + 4 + 8 + 4 + 4 + bson.len();
    let mut out = Vec::with_capacity(total);

    out.extend_from_slice(&(total as i32).to_le_bytes());
    out.extend_from_slice(&request_id.to_le_bytes());
    out.extend_from_slice(&response_to.to_le_bytes());
    out.extend_from_slice(&OP_REPLY.to_le_bytes());

    // responseFlags = 0
    out.extend_from_slice(&0i32.to_le_bytes());
    // cursorID = 0
    out.extend_from_slice(&0i64.to_le_bytes());
    // startingFrom = 0
    out.extend_from_slice(&0i32.to_le_bytes());
    // numberReturned = 1
    out.extend_from_slice(&1i32.to_le_bytes());
    out.extend_from_slice(&bson);
    out
}
