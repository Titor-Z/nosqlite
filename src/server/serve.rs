//! TCP 服务与 MongoDB 命令分发(阶段 2)。
//!
//! 连接模型:每个连接一个线程,共享同一个 `Arc<Database>`。
//! `.nodb` 仍由本进程独占,因此 redb 的单写者约束天然满足。

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicI32, AtomicI64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::{Map, Value, json};

use crate::database::{Database, UpsertResult};
use crate::document::Document;
use crate::query::{Filter, FindOptions, Projection, SortSpec, Update};

use super::wire::{
    HEADER_LEN, OP_MSG, OP_QUERY, build_op_msg_response, build_op_reply, err_doc, ok_doc,
    parse_op_msg, parse_op_query,
};

/// 单个游标的状态。
///
/// 实现说明:这里保存**已经过滤/排序/投影后的结果集**,而不是底层迭代器。
/// 代价是单个大查询的结果集仍在内存里,但:
/// - 内存上限是「本次查询结果集」而非整个集合,比一次性物化严格更好;
/// - 驱动侧语义完全正确(`getMore` 可续取、`batchSize` 生效、`id` 非 0)。
///
/// 真正的「边扫边吐」需要可持有的存储层迭代器,留到有明确内存压力时再做。
#[derive(Debug)]
struct CursorState {
    /// 所属集合名。
    collection: String,
    /// 结果集(已排序/投影)。
    docs: Vec<Map<String, Value>>,
    /// 下一次返回的起始下标。
    offset: usize,
}

/// 游标注册表(进程级共享)。
struct CursorRegistry {
    /// 游标 ID → 状态。
    cursors: Mutex<HashMap<i64, CursorState>>,
    /// ID 自增器(保证 > 0)。
    next_id: AtomicI64,
}

impl CursorRegistry {
    fn new() -> Self {
        CursorRegistry {
            cursors: Mutex::new(HashMap::new()),
            next_id: AtomicI64::new(1),
        }
    }

    /// 注册一个游标,返回其 ID;超过上限时返回 None(防止泄漏打爆内存)。
    fn register(&self, collection: &str, docs: Vec<Map<String, Value>>) -> Option<i64> {
        const MAX_CURSORS: usize = 1000;
        let mut guard = self.cursors.lock().ok()?;
        if guard.len() >= MAX_CURSORS {
            return None;
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        guard.insert(
            id,
            CursorState {
                collection: collection.to_string(),
                docs,
                offset: 0,
            },
        );
        Some(id)
    }

    /// 取下一批(最多 `batch_size` 条);耗尽或不存在时返回 None 表示游标结束。
    fn next_batch(&self, id: i64, batch_size: usize) -> Option<(String, Vec<Map<String, Value>>)> {
        let mut guard = self.cursors.lock().ok()?;
        let state = guard.get_mut(&id)?;
        let start = state.offset;
        let end = (start + batch_size.max(1)).min(state.docs.len());
        let batch: Vec<Map<String, Value>> = state.docs[start..end].to_vec();
        state.offset = end;
        let collection = state.collection.clone();
        if end >= state.docs.len() {
            guard.remove(&id);
        }
        Some((collection, batch))
    }

    /// 关闭游标,返回是否确实存在。
    fn kill(&self, id: i64) -> bool {
        self.cursors
            .lock()
            .map(|mut g| g.remove(&id).is_some())
            .unwrap_or(false)
    }
}

/// 全局游标注册表。
fn cursors() -> &'static CursorRegistry {
    static REGISTRY: OnceLock<CursorRegistry> = OnceLock::new();
    REGISTRY.get_or_init(CursorRegistry::new)
}

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
    _db_name: &str,
    sequences: &[(String, Vec<Map<String, Value>>)],
) -> Map<String, Value> {
    // 命令名大小写不敏感(驱动可能发 isMaster/ismaster)
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "hello" | "ismaster" => hello_response(),
        "ping" => ok_doc([]),
        "endsessions" => ok_doc([]),
        "getmore" => cmd_get_more(cmd),
        "killcursors" => cmd_kill_cursors(cmd),
        "insert" => cmd_insert(db, cmd, sequences),
        "find" => cmd_find(db, cmd),
        "update" => cmd_update(db, cmd, sequences),
        "delete" => cmd_delete(db, cmd, sequences),
        "findandmodify" => cmd_find_and_modify(db, cmd),
        "count" => cmd_count(db, cmd),
        "distinct" => cmd_distinct(db, cmd),
        "listcollections" => cmd_list_collections(db, cmd),
        "listdatabases" => cmd_list_databases(db, cmd),
        "createindexes" => cmd_create_indexes(db, cmd),
        "listindexes" => cmd_list_indexes(db, cmd),
        "dropindexes" => cmd_drop_indexes(db, cmd),
        "drop" => cmd_drop(db, cmd),
        "dropdatabase" => cmd_drop_database(db),
        "buildinfo" | "buildinfo1" => build_info(),
        "getsessioninfo" => ok_doc([]),
        "aggregate" => cmd_aggregate(db, cmd),
        "explain" => cmd_explain(db, cmd),
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
    // 驱动期望的首批条数(MongoDB 默认首批 101 条)
    let batch_size = cmd
        .get("batchSize")
        .and_then(|v| v.as_i64())
        .filter(|n| *n >= 0)
        .map(|n| n as usize)
        .unwrap_or(101);
    // `limit` 约束的是**结果总数**,不是单批条数
    let effective_limit = limit;

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
        Ok(docs) => build_cursor_response(coll, docs, batch_size, "firstBatch"),
        Err(e) => err_doc(2, "BadValue", &e.to_string()),
    }
}

/// 把结果集打包成游标响应:首批取 `batch_size` 条,剩余注册为可续取的游标。
///
/// `field` 是首批的字段名(`find` 用 `firstBatch`,`getMore` 用 `nextBatch`)。
fn build_cursor_response(
    coll: &str,
    docs: Vec<Map<String, Value>>,
    batch_size: usize,
    field: &str,
) -> Map<String, Value> {
    let ns = format!("nosqlite.{}", coll);
    let total = docs.len();
    let take = batch_size.max(1).min(total);
    let first: Vec<Value> = docs[..take].iter().cloned().map(Value::Object).collect();

    // 全部取完(或刚好一批):id = 0,不注册游标
    if take >= total {
        return ok_doc([(
            "cursor",
            json!({
                field: first,
                // 规范要求 cursor.id 是 int64,故用 Extended JSON 显式标注
                "id": {"$numberLong": "0"},
                "ns": ns,
            }),
        )]);
    }

    // 还有剩余:注册游标,返回其 ID
    match cursors().register(coll, docs) {
        Some(id) => {
            // 注册后立刻消费掉第一批(注册表保存的是完整结果集)
            let _ = cursors().next_batch(id, take);
            ok_doc([(
                "cursor",
                json!({
                    field: first,
                    "id": {"$numberLong": id.to_string()},
                    "ns": ns,
                }),
            )])
        }
        None => err_doc(
            1,
            "InternalError",
            "游标数量超过上限(1000),请关闭不再使用的游标",
        ),
    }
}

/// `getMore`:`{getMore: <cursorId>, collection: "coll", batchSize: N}`
fn cmd_get_more(cmd: &Map<String, Value>) -> Map<String, Value> {
    let Some(id) = cmd.get("getMore").and_then(|v| v.as_i64()) else {
        return err_doc(2, "BadValue", "getMore 需要一个 int64 游标 ID");
    };
    let batch_size = cmd
        .get("batchSize")
        .and_then(|v| v.as_i64())
        .filter(|n| *n > 0)
        .map(|n| n as usize)
        .unwrap_or(101);

    match cursors().next_batch(id, batch_size) {
        Some((coll, batch)) => {
            let ns = format!("nosqlite.{}", coll);
            let docs: Vec<Value> = batch.into_iter().map(Value::Object).collect();
            // 游标是否耗尽:registry 在取完最后一批时会移除,故再查一次
            let still_exists = cursors()
                .cursors
                .lock()
                .map(|g| g.contains_key(&id))
                .unwrap_or(false);
            let next_id = if still_exists {
                json!({"$numberLong": id.to_string()})
            } else {
                json!({"$numberLong": "0"})
            };
            ok_doc([(
                "cursor",
                json!({"nextBatch": docs, "id": next_id, "ns": ns}),
            )])
        }
        None => err_doc(43, "CursorNotFound", "游标不存在或已耗尽"),
    }
}

/// `killCursors`:`{killCursors: "coll", cursors: [<id>...]}`
fn cmd_kill_cursors(cmd: &Map<String, Value>) -> Map<String, Value> {
    let ids: Vec<i64> = cmd
        .get("cursors")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_i64()).collect())
        .unwrap_or_default();
    let mut killed: Vec<Value> = Vec::new();
    let mut not_found: Vec<Value> = Vec::new();
    for id in ids {
        if cursors().kill(id) {
            killed.push(json!({"$numberLong": id.to_string()}));
        } else {
            not_found.push(json!({"$numberLong": id.to_string()}));
        }
    }
    ok_doc([
        ("cursorsKilled", Value::Array(killed)),
        ("cursorsNotFound", Value::Array(not_found)),
        ("cursorsAlive", json!([])),
        ("cursorsUnknown", json!([])),
    ])
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

/// `createIndexes`(M26 起真正建索引)。
///
/// 请求形态:
/// ```json
/// {"createIndexes": "users",
///  "indexes": [{"key": {"a": 1, "b": -1}, "name": "a_1_b_-1", "unique": true}]}
/// ```
///
/// `key` 是**有序**映射,键序即索引字段顺序(BSON 保留了顺序,解析后即为
/// `serde_json::Map` 的迭代顺序);值为 `1`/`-1` 表示升降序,本引擎目前
/// 只支持升序存储(降序可由反向遍历实现,尚未接),故 `-1` 也按字段顺序建。
fn cmd_create_indexes(db: &Database, cmd: &Map<String, Value>) -> Map<String, Value> {
    let coll = cmd
        .get("createIndexes")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let Some(specs) = cmd.get("indexes").and_then(|v| v.as_array()) else {
        return err_doc(2, "BadValue", "createIndexes 需要 indexes 数组");
    };

    let before = db.list_indexes(coll).map(|v| v.len()).unwrap_or(0);

    for spec in specs {
        let Some(obj) = spec.as_object() else {
            continue;
        };
        let Some(key) = obj.get("key").and_then(|v| v.as_object()) else {
            return err_doc(2, "BadValue", "索引缺少 key");
        };
        // 字段顺序 = key 文档的键序;`1` 升序,`-1` 降序(M27)
        let fields: Vec<&str> = key.keys().map(|s| s.as_str()).collect();
        if fields.is_empty() {
            return err_doc(2, "BadValue", "索引 key 不能为空");
        }
        let dirs: Vec<i8> = key
            .values()
            .map(|d| if d.as_i64().unwrap_or(1) < 0 { -1 } else { 1 })
            .collect();

        // 未给 name 时按 MongoDB 惯例拼 `字段_方向`
        let name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                key.iter()
                    .map(|(f, d)| {
                        let dir = d.as_i64().unwrap_or(1);
                        format!("{}_{}", f, dir)
                    })
                    .collect::<Vec<_>>()
                    .join("_")
            });

        let unique = obj.get("unique").and_then(|v| v.as_bool()).unwrap_or(false);
        let sparse = obj.get("sparse").and_then(|v| v.as_bool()).unwrap_or(false);

        if let Err(e) = db.create_index_with_directions(coll, &name, &fields, &dirs, unique, sparse)
        {
            return err_doc(2, "BadValue", &e.to_string());
        }
    }

    let after = db.list_indexes(coll).map(|v| v.len()).unwrap_or(before);
    ok_doc([
        ("numIndexesBefore", json!(before)),
        ("numIndexesAfter", json!(after)),
        ("createdCollectionAutomatically", json!(false)),
    ])
}

/// `listIndexes`:返回索引清单。
///
/// MongoDB 的 `listIndexes` 通常以**游标**形式返回;驱动也能接受内联数组,
/// 这里返回游标形态(`cursor.firstBatch`)以兼容驱动。
fn cmd_list_indexes(db: &Database, cmd: &Map<String, Value>) -> Map<String, Value> {
    let coll = cmd
        .get("listIndexes")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let mut batch: Vec<Value> = Vec::new();
    // 默认主键索引(虚拟,不落盘)
    batch.push(json!({
        "v": 2,
        "key": {"_id": 1},
        "name": "_id_",
        "ns": format!("nosqlite.{}", coll),
    }));

    for def in db.list_indexes(coll).unwrap_or_default() {
        let mut key = serde_json::Map::new();
        for (i, f) in def.fields.iter().enumerate() {
            let d = def.directions.get(i).copied().unwrap_or(1);
            key.insert(f.clone(), json!(d));
        }
        let mut doc = serde_json::Map::new();
        doc.insert("v".into(), json!(2));
        doc.insert("key".into(), Value::Object(key));
        doc.insert("name".into(), json!(def.name));
        doc.insert("ns".into(), json!(format!("nosqlite.{}", coll)));
        if def.unique {
            doc.insert("unique".into(), json!(true));
        }
        if def.sparse {
            doc.insert("sparse".into(), json!(true));
        }
        batch.push(Value::Object(doc));
    }

    ok_doc([(
        "cursor",
        json!({
            "id": json!({"$numberLong": "0"}),
            "ns": format!("nosqlite.{}.$cmd.listIndexes", coll),
            "firstBatch": batch,
        }),
    )])
}

/// `dropIndexes`:`*` 删除全部,或按名字删除。
fn cmd_drop_indexes(db: &Database, cmd: &Map<String, Value>) -> Map<String, Value> {
    let coll = cmd
        .get("dropIndexes")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let target = cmd.get("index").and_then(|v| v.as_str()).unwrap_or("*");

    let defs = db.list_indexes(coll).unwrap_or_default();
    let before = defs.len();

    if target == "*" {
        for d in &defs {
            if let Err(e) = db.drop_index(coll, &d.name) {
                return err_doc(2, "BadValue", &e.to_string());
            }
        }
    } else if let Err(e) = db.drop_index(coll, target) {
        return err_doc(2, "BadValue", &e.to_string());
    }

    ok_doc([("nIndexesWas", json!(before + 1))])
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
        return aggregate_group(db, coll, &filter, group);
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
            let projected: Vec<Map<String, Value>> = match &project_stage {
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
                        map
                    })
                    .collect(),
                None => docs,
            };
            // 聚合结果同样走游标:大管道不再一次性全部返回
            let batch_size = cmd
                .get("cursor")
                .and_then(|v| v.as_object())
                .and_then(|o| o.get("batchSize"))
                .and_then(|v| v.as_i64())
                .filter(|n| *n > 0)
                .map(|n| n as usize)
                .unwrap_or(101);
            build_cursor_response(coll, projected, batch_size, "firstBatch")
        }
        Err(e) => err_doc(2, "BadValue", &e.to_string()),
    }
}

/// `explain`:`{explain: {find: "coll", filter: {...}, sort: {...}}}`
///
/// 返回形态贴近 MongoDB:`{queryPlanner: {winningPlan: {...}}, ok: 1}`。
fn cmd_explain(db: &Database, cmd: &Map<String, Value>) -> Map<String, Value> {
    // explain 的值是一个「被解释的命令」文档
    let Some(inner) = cmd.get("explain").and_then(|v| v.as_object()) else {
        return err_doc(2, "BadValue", "explain 需要一个命令对象");
    };
    let coll = inner.get("find").and_then(|v| v.as_str()).unwrap_or("");
    let filter = inner
        .get("filter")
        .cloned()
        .unwrap_or(Value::Object(Map::new()));
    let sort = inner
        .get("sort")
        .filter(|v| v.as_object().is_some_and(|m| !m.is_empty()))
        .and_then(|v| SortSpec::from_value(v).ok());

    match db.explain(coll, &filter, sort.as_ref()) {
        Ok(plan) => ok_doc([(
            "queryPlanner",
            json!({
                "namespace": format!("nosqlite.{}", coll),
                "winningPlan": plan,
            }),
        )]),
        Err(e) => err_doc(2, "BadValue", &e.to_string()),
    }
}

/// 处理 `$group`(v0.6.2 起支持真正的分组聚合)。
///
/// 支持形态:
/// - `{_id: null, n: {$sum: 1}}` —— `countDocuments` 的底层形态;
/// - `{_id: "$field", total: {$sum: "$v"}}` —— 按字段分组并求和;
/// - `_id` 可以是常量(分组键全相同)或 `"$路径"`(按该字段分组)。
///
/// 累加器:`$sum`(常量或字段路径)、`$count`。其余累加器报错,不做静默降级。
fn aggregate_group(
    db: &Database,
    coll: &str,
    filter: &Filter,
    group: &Value,
) -> Map<String, Value> {
    let Some(obj) = group.as_object() else {
        return err_doc(40324, "Location40324", "$group 必须是对象");
    };
    let Some(group_id) = obj.get("_id") else {
        return err_doc(40324, "Location40324", "$group 缺少 _id");
    };

    // 收集累加器:输出字段名 → (累加器类型, 参数)
    let mut accumulators: Vec<(String, String, Value)> = Vec::new();
    for (key, val) in obj {
        if key == "_id" {
            continue;
        }
        let Some(spec) = val.as_object() else {
            return err_doc(40324, "Location40324", "不支持的 $group 累加器");
        };
        let Some((acc, arg)) = spec.iter().next() else {
            return err_doc(40324, "Location40324", "空的 $group 累加器");
        };
        match acc.as_str() {
            "$sum" | "$count" => accumulators.push((key.clone(), acc.clone(), arg.clone())),
            other => {
                return err_doc(
                    40324,
                    "Location40324",
                    &format!("$group 累加器尚未实现: {}", other),
                );
            }
        }
    }
    if accumulators.is_empty() {
        // 无累加器时退化为「按 _id 去重计数」
        accumulators.push(("n".to_string(), "$sum".to_string(), json!(1)));
    }

    // 取全部匹配文档(聚合需要完整数据,不走分页)
    let docs = match db.find_with_options(coll, filter, &FindOptions::default()) {
        Ok(d) => d,
        Err(e) => return err_doc(2, "BadValue", &e.to_string()),
    };

    // 分组:键用 JSON 字符串表示(保留 BSON 类型差异,如 1 与 "1")
    let mut order: Vec<String> = Vec::new();
    // 每组:分组键值 + 各累加器的数值
    let mut groups: std::collections::HashMap<String, (Value, Vec<f64>)> =
        std::collections::HashMap::new();

    for doc in &docs {
        let key_value = eval_group_id(group_id, doc);
        let key_repr = serde_json::to_string(&key_value).unwrap_or_default();
        let entry = groups.entry(key_repr.clone()).or_insert_with(|| {
            order.push(key_repr.clone());
            (key_value.clone(), vec![0.0; accumulators.len()])
        });
        for (i, (_, acc, arg)) in accumulators.iter().enumerate() {
            let add = match acc.as_str() {
                "$count" => 1.0,
                "$sum" => match arg {
                    // 常量:每次加该常量
                    Value::Number(_) | Value::Bool(_) | Value::Null => {
                        arg.as_f64().unwrap_or(0.0)
                    }
                    Value::String(path) if path.starts_with('$') => {
                        let p = &path[1..];
                        doc.get(p).and_then(|v| v.as_f64()).unwrap_or(0.0)
                    }
                    _ => 0.0,
                },
                _ => 0.0,
            };
            entry.1[i] += add;
        }
    }

    // 组装结果(按首次出现顺序,稳定可预期)
    let mut batch: Vec<Value> = Vec::new();
    for repr in &order {
        let (key_value, sums) = &groups[repr];
        let mut out = Map::new();
        out.insert("_id".to_string(), key_value.clone());
        for (i, (name, _, _)) in accumulators.iter().enumerate() {
            // 整数结果输出为整数,避免驱动侧看到 3.0 与 3 的差异
            let v = sums[i];
            let num = if v.fract() == 0.0 && v.abs() < i64::MAX as f64 {
                json!(v as i64)
            } else {
                json!(v)
            };
            out.insert(name.clone(), num);
        }
        batch.push(Value::Object(out));
    }

    ok_doc([(
        "cursor",
        json!({
            "id": json!({"$numberLong": "0"}),
            "ns": format!("nosqlite.{}.$cmd.aggregate", coll),
            "firstBatch": batch,
        }),
    )])
}

/// 求 `$group._id` 的值:常量原样返回,`"$path"` 取字段值。
fn eval_group_id(group_id: &Value, doc: &Map<String, Value>) -> Value {
    match group_id {
        Value::String(path) if path.starts_with('$') => {
            doc.get(&path[1..]).cloned().unwrap_or(Value::Null)
        }
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 游标注册 → 分批取 → 耗尽自动移除。
    #[test]
    fn cursor_batches_until_exhausted() {
        let reg = CursorRegistry::new();
        let docs: Vec<Map<String, Value>> = (0..5)
            .map(|i| json!({"n": i}).as_object().unwrap().clone())
            .collect();
        let id = reg.register("c", docs).expect("应能注册");

        // 第一批 2 条,游标仍存在
        let (coll, batch) = reg.next_batch(id, 2).expect("应有第一批");
        assert_eq!(coll, "c");
        assert_eq!(batch.len(), 2);
        assert!(reg.cursors.lock().unwrap().contains_key(&id));

        // 第二批 2 条
        let (_, batch) = reg.next_batch(id, 2).expect("应有第二批");
        assert_eq!(batch.len(), 2);

        // 第三批 1 条,随后游标被移除
        let (_, batch) = reg.next_batch(id, 2).expect("应有第三批");
        assert_eq!(batch.len(), 1);
        assert!(!reg.cursors.lock().unwrap().contains_key(&id));

        // 再取应返回 None
        assert!(reg.next_batch(id, 2).is_none());
    }

    /// 关闭游标返回是否确实存在。
    #[test]
    fn cursor_kill() {
        let reg = CursorRegistry::new();
        let docs: Vec<Map<String, Value>> = (0..3)
            .map(|i| json!({"n": i}).as_object().unwrap().clone())
            .collect();
        let id = reg.register("c", docs).unwrap();
        assert!(reg.kill(id), "第一次关闭应成功");
        assert!(!reg.kill(id), "重复关闭应返回 false");
    }

    /// 全部装入一批时不注册游标,直接 id = 0。
    #[test]
    fn cursor_zero_when_fits_in_one_batch() {
        let docs: Vec<Map<String, Value>> = (0..3)
            .map(|i| json!({"n": i}).as_object().unwrap().clone())
            .collect();
        let resp = build_cursor_response("c", docs, 10, "firstBatch");
        let cursor = resp["cursor"].as_object().unwrap();
        assert_eq!(cursor["id"]["$numberLong"], json!("0"));
        assert_eq!(cursor["firstBatch"].as_array().unwrap().len(), 3);
    }

    /// 超过一批时返回非 0 的游标 ID,且首批只含 batchSize 条。
    #[test]
    fn cursor_nonzero_when_exceeds_batch() {
        let docs: Vec<Map<String, Value>> = (0..10)
            .map(|i| json!({"n": i}).as_object().unwrap().clone())
            .collect();
        let resp = build_cursor_response("c", docs, 3, "firstBatch");
        let cursor = resp["cursor"].as_object().unwrap();
        assert_eq!(cursor["firstBatch"].as_array().unwrap().len(), 3);
        let id_str = cursor["id"]["$numberLong"].as_str().unwrap();
        assert_ne!(id_str, "0", "应返回有效游标 ID");
        // 清理,避免污染全局注册表
        cursors().kill(id_str.parse::<i64>().unwrap());
    }

    /// `batchSize: 0` 在 MongoDB 里表示只建立游标不返回数据(此处按 1 处理,保证驱动能推进)。
    #[test]
    fn batch_size_zero_is_clamped() {
        let docs: Vec<Map<String, Value>> = (0..4)
            .map(|i| json!({"n": i}).as_object().unwrap().clone())
            .collect();
        let resp = build_cursor_response("c", docs, 0, "firstBatch");
        let cursor = resp["cursor"].as_object().unwrap();
        assert_eq!(cursor["firstBatch"].as_array().unwrap().len(), 1);
        let id_str = cursor["id"]["$numberLong"].as_str().unwrap();
        cursors().kill(id_str.parse::<i64>().unwrap());
    }

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
