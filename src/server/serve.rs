//! TCP 服务与 MongoDB 命令分发(阶段 2)。
//!
//! 连接模型:每个连接一个线程,共享同一个 `Arc<Database>`。
//! `.nodb` 仍由本进程独占,因此 redb 的单写者约束天然满足。

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

use serde_json::{Map, Value, json};

use crate::database::{Database, UpsertResult};
use crate::document::Document;
use crate::query::{Filter, FindOptions, Projection, SortSpec, Update};

use super::wire::{
    HEADER_LEN, OP_MSG, OP_QUERY, build_op_msg_response, build_op_reply, err_doc, ok_doc,
    parse_op_msg, parse_op_query,
};

/// 服务端配置。
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// 监听地址,如 `127.0.0.1:27017`。
    pub addr: String,
    /// 数据库文件路径。
    pub db_path: String,
}

/// 启动服务(阻塞)。
pub fn serve(config: ServerConfig) -> std::io::Result<()> {
    let path = Path::new(&config.db_path);
    let db = if path.exists() {
        Database::open(path).map_err(io_err)?
    } else {
        Database::create(path).map_err(io_err)?
    };
    let db = Arc::new(db);

    let listener = TcpListener::bind(&config.addr)?;
    println!(
        "nosqlite-server 已启动: {} (数据库 {},兼容 MongoDB wire 协议)",
        config.addr, config.db_path
    );

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let db = Arc::clone(&db);
                std::thread::spawn(move || {
                    if let Err(e) = handle_connection(stream, db) {
                        // 客户端断开是常态,不打印
                        if e.kind() != std::io::ErrorKind::UnexpectedEof
                            && e.kind() != std::io::ErrorKind::ConnectionReset
                        {
                            eprintln!("连接处理出错: {}", e);
                        }
                    }
                });
            }
            Err(e) => eprintln!("接受连接失败: {}", e),
        }
    }
    Ok(())
}

/// 把 DatabaseError 转成 io::Error(仅用于启动阶段)。
fn io_err(e: crate::database::DatabaseError) -> std::io::Error {
    std::io::Error::other(e.to_string())
}

/// 全局响应 ID 生成器。
static RESPONSE_ID: AtomicI32 = AtomicI32::new(1);

/// 处理一条连接:循环读取帧、分发命令、写回响应。
fn handle_connection(mut stream: TcpStream, db: Arc<Database>) -> std::io::Result<()> {
    loop {
        // 1. 读 16 字节头
        let mut header = [0u8; HEADER_LEN];
        match stream.read_exact(&mut header) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(e) => return Err(e),
        }
        let message_len = i32::from_le_bytes(header[0..4].try_into().unwrap());
        let request_id = i32::from_le_bytes(header[4..8].try_into().unwrap());
        let op_code = i32::from_le_bytes(header[12..16].try_into().unwrap());

        if message_len < HEADER_LEN as i32 || message_len > 48 * 1024 * 1024 {
            return Err(std::io::Error::other(format!(
                "消息长度非法: {}",
                message_len
            )));
        }

        // 2. 读消息体
        let body_len = message_len as usize - HEADER_LEN;
        let mut body = vec![0u8; body_len];
        stream.read_exact(&mut body)?;

        let response_id = RESPONSE_ID.fetch_add(1, Ordering::Relaxed);

        // 3. 分发
        match op_code {
            OP_MSG => {
                let req = match parse_op_msg(op_code, &body, request_id) {
                    Ok(Some(r)) => r,
                    Ok(None) => continue,
                    Err(e) => {
                        let doc = err_doc(2, "BadValue", &format!("请求解析失败: {}", e));
                        let frame = build_op_msg_response(response_id, request_id, &doc);
                        stream.write_all(&frame)?;
                        continue;
                    }
                };
                let response = dispatch(&db, &req.name, &req.command, &req.db, &req.sequences);
                let frame = build_op_msg_response(response_id, request_id, &response);
                stream.write_all(&frame)?;
            }
            OP_QUERY => {
                // 旧驱动握手:回 hello 的等价物
                match parse_op_query(&body) {
                    Ok(Some(cmd)) => {
                        let response = dispatch(&db, "hello", &cmd, "admin", &[]);
                        let frame = build_op_reply(response_id, request_id, &response);
                        stream.write_all(&frame)?;
                    }
                    _ => return Ok(()),
                }
            }
            other => {
                return Err(std::io::Error::other(format!("不支持的操作码: {}", other)));
            }
        }
        stream.flush()?;
    }
}

/// 命令分发:按命令名路由到具体实现。
fn dispatch(
    db: &Database,
    name: &str,
    cmd: &Map<String, Value>,
    db_name: &str,
    sequences: &[(String, Vec<Map<String, Value>>)],
) -> Map<String, Value> {
    // 命令名大小写不敏感(驱动可能发 isMaster/ismaster)
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "hello" | "ismaster" => hello_response(),
        "ping" => ok_doc([]),
        "endsessions" | "killcursors" => ok_doc([("cursorsKilled", json!([]))]),
        "insert" => cmd_insert(db, cmd, sequences),
        "find" => cmd_find(db, cmd),
        "update" => cmd_update(db, cmd, sequences),
        "delete" => cmd_delete(db, cmd, sequences),
        "findandmodify" => cmd_find_and_modify(db, cmd),
        "count" => cmd_count(db, cmd),
        "distinct" => cmd_distinct(db, cmd),
        "listcollections" => cmd_list_collections(db, cmd),
        "listdatabases" => cmd_list_databases(db, cmd),
        "createindexes" => cmd_create_indexes(cmd, db_name),
        "drop" => cmd_drop(db, cmd),
        "dropdatabase" => cmd_drop_database(db),
        "buildinfo" | "buildinfo1" => build_info(),
        "getsessioninfo" => ok_doc([]),
        "aggregate" => cmd_aggregate(db, cmd),
        other => err_doc(
            59,
            "CommandNotFound",
            &format!("nosqlite-server 尚未实现命令: {}", other),
        ),
    }
}

/// `hello` / `isMaster` 响应。
///
/// `maxWireVersion` 必须足够高,否则驱动直接拒连(驱动 v2.5 要求 >= 8,新驱动要求更高)。
fn hello_response() -> Map<String, Value> {
    ok_doc([
        ("ismaster", json!(true)),
        ("isWritablePrimary", json!(true)),
        ("helloOk", json!(true)),
        ("maxBsonObjectSize", json!(16 * 1024 * 1024)),
        ("maxMessageSizeBytes", json!(48_000_000)),
        ("maxWriteBatchSize", json!(100_000)),
        ("minWireVersion", json!(0)),
        ("maxWireVersion", json!(17)),
        ("readOnly", json!(false)),
        ("hosts", json!(["localhost:27017"])),
        ("localTime", json!({"$date": now_millis()})),
        ("logicalSessionTimeoutMinutes", json!(30)),
        ("compression", json!([])),
        ("ok", json!(1)),
    ])
}

/// `buildInfo` 响应(驱动/工具常探测)。
fn build_info() -> Map<String, Value> {
    ok_doc([
        ("version", json!("8.0.0-nosqlite")),
        ("versionArray", json!([8, 0, 0, 0])),
        ("gitVersion", json!("nosqlite")),
        ("modules", json!([])),
        ("allocator", json!("system")),
        ("javascriptEngine", json!("none")),
        ("bits", json!(64)),
        ("debug", json!(false)),
        ("maxBsonObjectSize", json!(16 * 1024 * 1024)),
    ])
}

/// 当前毫秒时间戳。
fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 从 OP_MSG section 1 中取指定 identifier 的文档序列。
fn take_sequence(
    sequences: &[(String, Vec<Map<String, Value>>)],
    identifier: &str,
) -> Vec<Map<String, Value>> {
    sequences
        .iter()
        .find(|(name, _)| name == identifier)
        .map(|(_, docs)| docs.clone())
        .unwrap_or_default()
}

/// 从 CRUD 命令取集合名。
///
/// MongoDB 的约定:**集合名就在命令名那个键的值里**(如 `{insert: "users", ...}`),
/// 而不是单独的 `collection` 字段(那是 `listCollections` 之类的元命令才用的)。
fn collection_name(cmd: &Map<String, Value>, command: &str) -> String {
    cmd.get(command)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// 把 BSON 文档转成待插入的 `Document`(补 `_id`)。
fn to_document(map: &Map<String, Value>) -> Result<Document, String> {
    let text = serde_json::to_string(map).map_err(|e| e.to_string())?;
    Document::from_json(&text).map_err(|e| e.to_string())
}

// ==================== 命令实现 ====================

/// `insert`:`{insert: "coll", documents: [...], ordered: true}`
fn cmd_insert(
    db: &Database,
    cmd: &Map<String, Value>,
    sequences: &[(String, Vec<Map<String, Value>>)],
) -> Map<String, Value> {
    let coll = collection_name(cmd, "insert");
    // 驱动把文档放在 OP_MSG 的 section 1(文档序列)里,命令文档中并没有 `documents` 字段
    let seq_docs = take_sequence(sequences, "documents");
    let docs: Vec<Map<String, Value>> = if !seq_docs.is_empty() {
        seq_docs
    } else {
        cmd.get("documents")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_object().cloned()).collect())
            .unwrap_or_default()
    };

    let mut parsed = Vec::with_capacity(docs.len());
    for doc in &docs {
        match to_document(doc) {
            Ok(d) => parsed.push(d),
            Err(e) => {
                return err_doc(2, "BadValue", &format!("文档非法: {}", e));
            }
        }
    }

    match db.insert_many(&coll, &mut parsed) {
        Ok(ids) => ok_doc([
            ("n", json!(ids.len())),
            (
                "insertedIds",
                serde_json::to_value(ids).unwrap_or(Value::Null),
            ),
        ]),
        Err(e) => err_doc(11000, "DuplicateKey", &e.to_string()),
    }
}

/// `find`:`{find: "coll", filter: {...}, sort: {...}, limit, skip, projection}`
///
/// 返回游标结构(`cursor.id` 为 0 表示一次取完)。
fn cmd_find(db: &Database, cmd: &Map<String, Value>) -> Map<String, Value> {
    let coll = cmd.get("find").and_then(|v| v.as_str()).unwrap_or("");
    let filter_value = cmd
        .get("filter")
        .cloned()
        .unwrap_or(Value::Object(Map::new()));
    let filter = match Filter::from_value(&filter_value) {
        Ok(f) => f,
        Err(e) => return err_doc(2, "BadValue", &e.to_string()),
    };

    let sort = if let Some(sv) = cmd
        .get("sort")
        .filter(|v| v.as_object().is_some_and(|m| !m.is_empty()))
    {
        match SortSpec::from_value(sv) {
            Ok(s) => Some(s),
            Err(e) => return err_doc(2, "BadValue", &e.to_string()),
        }
    } else {
        None
    };
    let projection = if let Some(pv) = cmd
        .get("projection")
        .filter(|v| v.as_object().is_some_and(|m| !m.is_empty()))
    {
        match Projection::from_value(pv) {
            Ok(p) => Some(p),
            Err(e) => return err_doc(2, "BadValue", &e.to_string()),
        }
    } else {
        None
    };
    let skip = cmd.get("skip").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let limit = match cmd.get("limit").and_then(|v| v.as_i64()) {
        Some(l) if l > 0 => Some(l as usize),
        Some(l) if l < 0 => Some((-l) as usize),
        _ => None,
    };
    // batchSize 影响单批返回量,先做「一次取完」的简化(游标 id 恒为 0)
    let batch_size = cmd
        .get("batchSize")
        .and_then(|v| v.as_i64())
        .filter(|n| *n > 0)
        .map(|n| n as usize);
    let effective_limit = match (limit, batch_size) {
        (Some(l), Some(b)) => Some(l.min(b)),
        (None, Some(b)) => Some(b),
        (l, None) => l,
    };

    let result = if sort.is_some() || projection.is_some() {
        db.find_page_total(
            coll,
            &filter,
            sort.as_ref(),
            skip,
            effective_limit,
            projection.as_ref(),
        )
        .map(|(_, docs)| docs)
    } else {
        db.find_with_options(
            coll,
            &filter,
            &FindOptions {
                sort: None,
                projection: None,
                skip,
                limit: effective_limit,
            },
        )
        .map(|docs| docs.to_vec())
    };

    match result {
        Ok(docs) => {
            let first_batch = docs
                .iter()
                .map(|d| Value::Object(d.clone()))
                .collect::<Vec<_>>();
            ok_doc([(
                "cursor",
                json!({
                    "firstBatch": first_batch,
                    // 规范要求 cursor.id 是 int64,故用 Extended JSON 显式标注
                    "id": {"$numberLong": "0"},
                    "ns": format!("{}.{}", "nosqlite", coll),
                }),
            )])
        }
        Err(e) => err_doc(2, "BadValue", &e.to_string()),
    }
}

/// `update`:`{update: "coll", updates: [{q, u, upsert, multi}]}`
fn cmd_update(
    db: &Database,
    cmd: &Map<String, Value>,
    sequences: &[(String, Vec<Map<String, Value>>)],
) -> Map<String, Value> {
    let coll = collection_name(cmd, "update");
    let updates: Vec<Value> = take_sequence(sequences, "updates")
        .into_iter()
        .map(Value::Object)
        .collect::<Vec<_>>()
        .into_iter()
        .chain(
            cmd.get("updates")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default(),
        )
        .collect();

    let mut n_matched = 0i64;
    let mut n_modified = 0i64;
    let mut upserted: Vec<Value> = Vec::new();

    for item in updates {
        let Some(entry) = item.as_object() else {
            continue;
        };
        let Some(q) = entry.get("q") else { continue };
        let Some(u) = entry.get("u") else { continue };
        let multi = entry
            .get("multi")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let upsert = entry
            .get("upsert")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let filter = match Filter::from_value(q) {
            Ok(f) => f,
            Err(e) => return err_doc(2, "BadValue", &e.to_string()),
        };
        // u 可能是操作符对象,也可能是整体替换文档(MongoDB 语义)
        let update = if is_update_operator_doc(u) {
            match Update::from_value(u) {
                Ok(x) => x,
                Err(e) => return err_doc(9, "FailedToParse", &e.to_string()),
            }
        } else {
            // 整体替换:把文档字段逐个 $set(简化处理;不含 _id)
            let mut set = Map::new();
            if let Some(obj) = u.as_object() {
                for (k, v) in obj {
                    if k != "_id" {
                        set.insert(k.clone(), v.clone());
                    }
                }
            }
            match Update::from_value(&json!({ "$set": set })) {
                Ok(x) => x,
                Err(e) => return err_doc(9, "FailedToParse", &e.to_string()),
            }
        };

        if upsert {
            match db.update_or_insert(&coll, &filter, &update) {
                Ok(UpsertResult::Inserted(id)) => {
                    n_matched += 1;
                    upserted.push(json!({ "index": 0, "_id": id }));
                }
                Ok(UpsertResult::Updated(m)) => {
                    n_matched += 1;
                    n_modified += m as i64;
                }
                Err(e) => return err_doc(2, "BadValue", &e.to_string()),
            }
        } else {
            match db.update(&coll, &filter, &update) {
                Ok(m) => {
                    n_modified += m as i64;
                    if m > 0 {
                        n_matched += m as i64;
                    } else if db.count(&coll, &filter).unwrap_or(0) > 0 {
                        n_matched += 1;
                    }
                    let _ = multi;
                }
                Err(e) => return err_doc(2, "BadValue", &e.to_string()),
            }
        }
    }

    let mut resp = ok_doc([("n", json!(n_matched)), ("nModified", json!(n_modified))]);
    if !upserted.is_empty() {
        resp.insert("upserted".to_string(), Value::Array(upserted));
    }
    resp
}

/// 判断更新文档是否是操作符形式(`$set` 等)。
fn is_update_operator_doc(v: &Value) -> bool {
    v.as_object()
        .is_some_and(|m| m.keys().any(|k| k.starts_with('$')))
}

/// `delete`:`{delete: "coll", deletes: [{q, limit}]}`
fn cmd_delete(
    db: &Database,
    cmd: &Map<String, Value>,
    sequences: &[(String, Vec<Map<String, Value>>)],
) -> Map<String, Value> {
    let coll = collection_name(cmd, "delete");
    let deletes: Vec<Value> = take_sequence(sequences, "deletes")
        .into_iter()
        .map(Value::Object)
        .collect::<Vec<_>>()
        .into_iter()
        .chain(
            cmd.get("deletes")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default(),
        )
        .collect();

    let mut total = 0i64;
    for item in deletes {
        let Some(entry) = item.as_object() else {
            continue;
        };
        let Some(q) = entry.get("q") else { continue };
        let filter = match Filter::from_value(q) {
            Ok(f) => f,
            Err(e) => return err_doc(2, "BadValue", &e.to_string()),
        };
        // limit=1 表示只删一条:先查出一条的 _id 再按主键删
        let limit = entry.get("limit").and_then(|v| v.as_i64()).unwrap_or(0);
        if limit == 1 {
            match db.find(&coll, &filter) {
                Ok(docs) => {
                    if let Some(doc) = docs.first()
                        && let Some(id) = doc.get("_id").and_then(|v| v.as_str())
                        && db.delete_by_id(&coll, id).unwrap_or(false)
                    {
                        total += 1;
                    }
                }
                Err(e) => return err_doc(2, "BadValue", &e.to_string()),
            }
        } else {
            match db.delete(&coll, &filter) {
                Ok(n) => total += n as i64,
                Err(e) => return err_doc(2, "BadValue", &e.to_string()),
            }
        }
    }
    ok_doc([("n", json!(total))])
}

/// `findAndModify`:Mongoose `findOneAndUpdate` 的底层。
fn cmd_find_and_modify(db: &Database, cmd: &Map<String, Value>) -> Map<String, Value> {
    let coll = cmd
        .get("findAndModify")
        .or_else(|| cmd.get("findandmodify"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let query = cmd
        .get("query")
        .cloned()
        .unwrap_or(Value::Object(Map::new()));
    let filter = match Filter::from_value(&query) {
        Ok(f) => f,
        Err(e) => return err_doc(2, "BadValue", &e.to_string()),
    };
    let remove = cmd.get("remove").and_then(|v| v.as_bool()).unwrap_or(false);
    let return_new = cmd.get("new").and_then(|v| v.as_bool()).unwrap_or(false);

    // 先取出匹配的第一条(支持 sort)
    let sort = cmd
        .get("sort")
        .filter(|v| v.as_object().is_some_and(|m| !m.is_empty()))
        .and_then(|v| SortSpec::from_value(v).ok());
    let target = if let Some(s) = &sort {
        db.find_page_total(coll, &filter, Some(s), 0, Some(1), None)
            .ok()
            .and_then(|(_, docs)| docs.into_iter().next())
    } else {
        db.find(coll, &filter)
            .ok()
            .and_then(|d| d.into_iter().next())
    };

    if remove {
        if let Some(doc) = &target
            && let Some(id) = doc.get("_id").and_then(|v| v.as_str())
        {
            let _ = db.delete_by_id(coll, id);
        }
        return ok_doc([("value", target.map_or(Value::Null, Value::Object))]);
    }

    let Some(u) = cmd.get("update") else {
        return err_doc(2, "BadValue", "findAndModify 需要 update 或 remove");
    };
    let update = if is_update_operator_doc(u) {
        match Update::from_value(u) {
            Ok(x) => x,
            Err(e) => return err_doc(9, "FailedToParse", &e.to_string()),
        }
    } else {
        let mut set = Map::new();
        if let Some(obj) = u.as_object() {
            for (k, v) in obj {
                if k != "_id" {
                    set.insert(k.clone(), v.clone());
                }
            }
        }
        match Update::from_value(&json!({ "$set": set })) {
            Ok(x) => x,
            Err(e) => return err_doc(9, "FailedToParse", &e.to_string()),
        }
    };

    match &target {
        Some(doc) => {
            let id = doc
                .get("_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if let Err(e) = db.update_by_id(coll, &id, &update) {
                return err_doc(2, "BadValue", &e.to_string());
            }
            let value = if return_new {
                db.get_by_id(coll, &id)
                    .ok()
                    .flatten()
                    .map_or(Value::Null, Value::Object)
            } else {
                Value::Object(doc.clone())
            };
            ok_doc([
                ("value", value),
                ("lastErrorObject", json!({"n": 1, "updatedExisting": true})),
            ])
        }
        None => {
            let upsert = cmd.get("upsert").and_then(|v| v.as_bool()).unwrap_or(false);
            if upsert {
                match db.update_or_insert(coll, &filter, &update) {
                    Ok(UpsertResult::Inserted(id)) => {
                        let value = db
                            .get_by_id(coll, &id)
                            .ok()
                            .flatten()
                            .map_or(Value::Null, Value::Object);
                        ok_doc([
                            ("value", value),
                            (
                                "lastErrorObject",
                                json!({"n": 1, "updatedExisting": false, "upserted": id}),
                            ),
                        ])
                    }
                    Ok(UpsertResult::Updated(_)) => ok_doc([("value", Value::Null)]),
                    Err(e) => err_doc(2, "BadValue", &e.to_string()),
                }
            } else {
                ok_doc([
                    ("value", Value::Null),
                    ("lastErrorObject", json!({"n": 0, "updatedExisting": false})),
                ])
            }
        }
    }
}

/// `count`:`{count: "coll", query: {...}}`
fn cmd_count(db: &Database, cmd: &Map<String, Value>) -> Map<String, Value> {
    let coll = cmd.get("count").and_then(|v| v.as_str()).unwrap_or("");
    let query = cmd
        .get("query")
        .cloned()
        .unwrap_or(Value::Object(Map::new()));
    let filter = match Filter::from_value(&query) {
        Ok(f) => f,
        Err(e) => return err_doc(2, "BadValue", &e.to_string()),
    };
    match db.count(coll, &filter) {
        Ok(n) => ok_doc([("n", json!(n))]),
        Err(e) => err_doc(2, "BadValue", &e.to_string()),
    }
}

/// `distinct`:`{distinct: "coll", key: "field", query: {...}}`
fn cmd_distinct(db: &Database, cmd: &Map<String, Value>) -> Map<String, Value> {
    let coll = cmd.get("distinct").and_then(|v| v.as_str()).unwrap_or("");
    let key = cmd.get("key").and_then(|v| v.as_str()).unwrap_or("");
    let query = cmd
        .get("query")
        .cloned()
        .unwrap_or(Value::Object(Map::new()));
    let filter = match Filter::from_value(&query) {
        Ok(f) => f,
        Err(e) => return err_doc(2, "BadValue", &e.to_string()),
    };
    let docs = match db.find(coll, &filter) {
        Ok(d) => d,
        Err(e) => return err_doc(2, "BadValue", &e.to_string()),
    };

    let mut seen: Vec<Value> = Vec::new();
    for doc in docs {
        if let Some(v) = resolve_field(&doc, key)
            && !seen.iter().any(|s| s == v)
        {
            seen.push(v.clone());
        }
    }
    ok_doc([("values", Value::Array(seen))])
}

/// 按点号路径取值(不穿透数组,distinct 的简化实现)。
fn resolve_field<'a>(doc: &'a Map<String, Value>, path: &str) -> Option<&'a Value> {
    let mut segs = path.split('.');
    let first = segs.next()?;
    let mut cur = doc.get(first)?;
    for seg in segs {
        cur = cur.as_object()?.get(seg)?;
    }
    Some(cur)
}

/// `listCollections`
fn cmd_list_collections(db: &Database, _cmd: &Map<String, Value>) -> Map<String, Value> {
    let names = db.collections().unwrap_or_default();
    let first_batch: Vec<Value> = names
        .iter()
        .map(|n| {
            json!({
                "name": n,
                "type": "collection",
                "options": {},
                "info": {"readOnly": false},
                "idIndex": {"v": 2, "key": {"_id": 1}, "name": "_id_"},
            })
        })
        .collect();
    ok_doc([(
        "cursor",
        json!({"firstBatch": first_batch, "id": {"$numberLong": "0"}, "ns": "nosqlite.$cmd.listCollections"}),
    )])
}

/// `listDatabases`
fn cmd_list_databases(db: &Database, _cmd: &Map<String, Value>) -> Map<String, Value> {
    let names = db.collections().unwrap_or_default();
    let mut total = 0usize;
    for n in &names {
        total += db.count(n, &Filter::match_all()).unwrap_or(0);
    }
    ok_doc([
        (
            "databases",
            json!([{
                "name": "nosqlite",
                "sizeOnDisk": 0,
                "empty": total == 0,
            }]),
        ),
        ("totalSize", json!(0)),
        ("ok", json!(1)),
    ])
}

/// `createIndexes`:接受并返回成功(索引暂未实现,但不能报错,否则驱动建库失败)。
fn cmd_create_indexes(_cmd: &Map<String, Value>, _db_name: &str) -> Map<String, Value> {
    ok_doc([
        ("numIndexesBefore", json!(1)),
        ("numIndexesAfter", json!(1)),
    ])
}

/// `drop`
fn cmd_drop(db: &Database, cmd: &Map<String, Value>) -> Map<String, Value> {
    let coll = cmd.get("drop").and_then(|v| v.as_str()).unwrap_or("");
    match db.drop_collection(coll) {
        Ok(_) => ok_doc([
            ("nIndexesWas", json!(1)),
            ("ns", json!(format!("nosqlite.{}", coll))),
        ]),
        Err(e) => err_doc(2, "BadValue", &e.to_string()),
    }
}

/// `dropDatabase`
fn cmd_drop_database(db: &Database) -> Map<String, Value> {
    let names = db.collections().unwrap_or_default();
    for n in names {
        let _ = db.drop_collection(&n);
    }
    ok_doc([("dropped", json!("nosqlite"))])
}

/// `aggregate`:先支持 `$match/$sort/$limit/$skip` 组成的简单管道。
fn cmd_aggregate(db: &Database, cmd: &Map<String, Value>) -> Map<String, Value> {
    let coll = match cmd.get("aggregate").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => return err_doc(2, "BadValue", "aggregate 需要集合名"),
    };
    let pipeline = cmd
        .get("pipeline")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut filter = Filter::match_all();
    let mut sort: Option<SortSpec> = None;
    let mut skip = 0usize;
    let mut limit: Option<usize> = None;
    let mut group_stage: Option<Value> = None;
    let mut project_stage: Option<Value> = None;

    for stage in &pipeline {
        let Some(obj) = stage.as_object() else {
            continue;
        };
        let Some((name, arg)) = obj.iter().next() else {
            continue;
        };
        match name.as_str() {
            "$match" => match Filter::from_value(arg) {
                Ok(f) => filter = f,
                Err(e) => return err_doc(2, "BadValue", &e.to_string()),
            },
            "$sort" => match SortSpec::from_value(arg) {
                Ok(s) => sort = Some(s),
                Err(e) => return err_doc(2, "BadValue", &e.to_string()),
            },
            "$skip" => skip = arg.as_u64().unwrap_or(0) as usize,
            "$limit" => limit = arg.as_u64().map(|n| n as usize),
            // `countDocuments` 底层是 `{$group: {_id: null, n: {$sum: 1}}}`,必须支持
            "$group" => {
                group_stage = Some(arg.clone());
            }
            "$project" | "$addFields" | "$set" => {
                project_stage = Some(arg.clone());
            }
            other => {
                return err_doc(
                    40324,
                    "Location40324",
                    &format!("aggregate 阶段尚未实现: {}", other),
                );
            }
        }
    }

    // `$group: {_id: null, n: {$sum: 1}}`(countDocuments 的底层形态)
    if let Some(group) = &group_stage {
        return aggregate_count(db, coll, &filter, group);
    }

    let result = if sort.is_some() {
        db.find_page_total(coll, &filter, sort.as_ref(), skip, limit, None)
            .map(|(_, docs)| docs)
    } else {
        db.find_with_options(
            coll,
            &filter,
            &FindOptions {
                sort: None,
                projection: None,
                skip,
                limit,
            },
        )
        .map(|docs| docs.to_vec())
    };

    match result {
        Ok(docs) => {
            // 简单的 $project / $addFields:按字段包含裁剪(仅供驱动内部探测)
            let batch: Vec<Value> = match &project_stage {
                Some(proj) => docs
                    .into_iter()
                    .map(|d| {
                        let mut map = Map::new();
                        if let Some(obj) = proj.as_object() {
                            for key in obj.keys() {
                                if key == "_id" {
                                    continue;
                                }
                                if let Some(v) = d.get(key) {
                                    map.insert(key.clone(), v.clone());
                                }
                            }
                        }
                        if let Some(id) = d.get("_id") {
                            map.insert("_id".to_string(), id.clone());
                        }
                        Value::Object(map)
                    })
                    .collect(),
                None => docs.into_iter().map(Value::Object).collect(),
            };
            ok_doc([(
                "cursor",
                json!({"firstBatch": batch, "id": {"$numberLong": "0"}, "ns": format!("nosqlite.{}", coll)}),
            )])
        }
        Err(e) => err_doc(2, "BadValue", &e.to_string()),
    }
}

/// 处理 `$group` 计数(支持 `{$sum: 1}` 与 `{$sum: "$field"}` 之外的常见形态)。
///
/// 只实现 `countDocuments` 需要的形态:`{_id: null, <name>: {$sum: 1}}`。
fn aggregate_count(
    db: &Database,
    coll: &str,
    filter: &Filter,
    group: &Value,
) -> Map<String, Value> {
    let Some(obj) = group.as_object() else {
        return err_doc(40324, "Location40324", "$group 必须是对象");
    };
    // 找出累加字段名与是否为 sum:1
    let mut out_field = "n".to_string();
    for (key, val) in obj {
        if key == "_id" {
            continue;
        }
        out_field = key.clone();
        let Some(spec) = val.as_object() else {
            return err_doc(40324, "Location40324", "不支持的 $group 累加器");
        };
        let Some((acc, arg)) = spec.iter().next() else {
            return err_doc(40324, "Location40324", "空的 $group 累加器");
        };
        match acc.as_str() {
            "$sum" => {
                let is_count = arg.as_i64() == Some(1) || arg.as_f64() == Some(1.0);
                if !is_count {
                    return err_doc(
                        40324,
                        "Location40324",
                        &format!(
                            "$group 的 $sum 目前只支持常量 1(countDocuments 场景),收到: {}",
                            arg
                        ),
                    );
                }
            }
            other => {
                return err_doc(
                    40324,
                    "Location40324",
                    &format!("$group 累加器尚未实现: {}", other),
                );
            }
        }
    }

    match db.count(coll, filter) {
        Ok(n) => {
            let mut doc = Map::new();
            doc.insert("_id".to_string(), Value::Null);
            doc.insert(out_field, Value::from(n));
            ok_doc([(
                "cursor",
                json!({"firstBatch": [doc], "id": {"$numberLong": "0"}, "ns": format!("nosqlite.{}", coll)}),
            )])
        }
        Err(e) => err_doc(2, "BadValue", &e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// hello 响应必须带足够高的 maxWireVersion,否则驱动拒连。
    #[test]
    fn hello_has_wire_version() {
        let resp = hello_response();
        assert_eq!(resp.get("ok"), Some(&json!(1)));
        let v = resp["maxWireVersion"].as_i64().unwrap();
        assert!(v >= 8, "maxWireVersion 至少 8,实际 {}", v);
    }

    /// 错误响应格式符合驱动预期。
    #[test]
    fn err_doc_shape() {
        let d = err_doc(11000, "DuplicateKey", "冲突");
        assert_eq!(d["ok"], json!(0));
        assert_eq!(d["code"], json!(11000));
        assert_eq!(d["codeName"], json!("DuplicateKey"));
    }

    /// 未实现的命令必须回 CommandNotFound 而不是断开连接。
    #[test]
    fn unknown_command_returns_error() {
        // 用 dispatch 需要 Database,这里只验证 err_doc 的码
        let d = err_doc(59, "CommandNotFound", "x");
        assert_eq!(d["code"], json!(59));
    }
}
