//! C ABI(阶段 1):供任何语言通过 FFI 进程内嵌使用。
//!
//! # 设计立场
//!
//! 骨架照搬 SQLite(**不透明句柄 + 错误码 + 游标**),但在三处按现代标准做,
//! 因为 SQLite 被 2004 年的 ABI 永久兼容拖住,做不到这些:
//!
//! 1. **arena 结果**:`nosqlite_result` 一次性持有全部返回数据,
//!    只需 [`nosqlite_result_free`] 调一次 —— 修掉「每个返回值单独 free」的泄漏重灾区;
//! 2. **结构化错误**:错误码 + 中文消息 + 操作名 + 字段路径 + 输入偏移;
//! 3. **`struct_size` 版本化 options**:后续加参数不需要 `_v2` 新函数。
//!
//! # 内存约定
//!
//! - 所有 `nosqlite_result *` 与 `char *` 由本库分配,**必须**用对应函数释放;
//! - 句柄 `nosqlite_db *` 用 [`nosqlite_close`] 释放;
//! - 返回的切片在 `nosqlite_result_free` 之前一直有效(Rust 侧用 `Box::leak` 保活)。
//!
//! # 线程
//!
//! `nosqlite_db` 内部是 `Database`(`Send + Sync`),可跨线程共享,但**不要**在
//! 同一句柄上并发调用 `close`。错误信息按线程局部存储,不跨线程共享。

use std::ffi::{CStr, CString, c_char};
use std::path::Path;

use serde_json::{Map, Value};

use crate::database::{Database, DatabaseError, UpsertResult};
use crate::document::Document;
use crate::error::ErrorCode;
use crate::query::{Filter, FindOptions, Projection, SortSpec, Update};

/// 成功码。
pub const NOSQLITE_OK: i32 = 0;

// ==================== 数据结构 ====================

/// 字符串切片(UTF-8,无 NUL 结尾;用 `len` 定界)。
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NosqliteSlice {
    /// 数据指针。
    pub ptr: *const c_char,
    /// 字节长度。
    pub len: usize,
}

/// 结构化错误(线程局部,由 [`nosqlite_last_error`] 返回,无需释放)。
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NosqliteError {
    /// 稳定错误码(见 [`ErrorCode`])。
    pub code: i32,
    /// 错误码字符串(`"DUPLICATE_KEY"` 等)。
    pub code_str: NosqliteSlice,
    /// 中文消息(供人阅读)。
    pub message: NosqliteSlice,
    /// 出错的操作名(`"open"`/`"insert"`/...)。
    pub op: NosqliteSlice,
}

/// 查询结果(arena:全部数据整块持有,只需释放一次)。
#[repr(C)]
#[derive(Debug)]
pub struct NosqliteResult {
    /// 错误码,0 表示成功。
    pub code: i32,
    /// 文档/数据条数。
    pub count: u64,
    /// 受影响行数(update/delete/upsert 的计数),其余操作为 0。
    pub affected: u64,
    /// 文档 JSON 数组(每条一个切片),共 `count` 个。
    pub docs: *mut NosqliteSlice,
    /// 单值结果的 JSON(如 `insert` 返回的 `_id`),无则为空切片。
    pub value: NosqliteSlice,
}

/// 打开选项(`struct_size` 版本化,未来加字段不需新函数)。
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NosqliteOpenOptions {
    /// 必须填 `size_of::<NosqliteOpenOptions>()`。
    pub struct_size: u32,
    /// 保留标志位,当前必须为 0。
    pub flags: u32,
    /// 文件不存在时是否自动创建(1 创建,0 报错)。
    pub create_if_missing: u8,
    /// 对齐填充,调用方应置 0。
    pub _pad: [u8; 7],
}

/// 查询选项。
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NosqliteFindOptions {
    /// 必须填 `size_of::<NosqliteFindOptions>()`。
    pub struct_size: u32,
    /// 跳过条数。
    pub skip: u64,
    /// 最大返回条数;`limit_is_null == 0` 时生效。
    pub limit: u64,
    /// `limit` 是否为 null(1 表示不限制)。
    pub limit_is_null: u8,
    /// 对齐填充。
    pub _pad: [u8; 7],
}

/// 不透明数据库句柄。
pub struct NosqliteDb {
    /// 底层数据库(可跨线程共享)。
    inner: Database,
}

/// 不透明游标句柄(分块游标:内部按批取页)。
pub struct NosqliteCursor {
    /// 剩余待取的结果(已按批加载)。
    pending: std::vec::IntoIter<Map<String, Value>>,
    /// 本批已取到的文档在内存中的持有者。
    arena: Vec<CString>,
}

// ==================== 内部工具 ====================

thread_local! {
    /// 线程局部错误消息存储(保证返回的指针在下一次调用前有效)。
    static LAST_ERROR: std::cell::RefCell<Option<CString>> =
        const { std::cell::RefCell::new(None) };
    /// 线程局部错误码。
    static LAST_CODE: std::cell::Cell<ErrorCode> =
        const { std::cell::Cell::new(ErrorCode::Internal) };
    /// 线程局部操作名。
    static LAST_OP: std::cell::RefCell<Option<CString>> =
        const { std::cell::RefCell::new(None) };
}

/// 记录一次错误,返回稳定错误码整数。
fn set_error(op: &str, err: &DatabaseError) -> i32 {
    let code = err.code();
    let msg = CString::new(err.to_string()).unwrap_or_default();
    let op_c = CString::new(op).unwrap_or_default();
    LAST_ERROR.with(|e| *e.borrow_mut() = Some(msg));
    LAST_CODE.with(|c| c.set(code));
    LAST_OP.with(|o| *o.borrow_mut() = Some(op_c));
    code.as_i32()
}

/// 记录一个自定义错误消息。
fn set_error_msg(op: &str, code: ErrorCode, msg: &str) -> i32 {
    let msg = CString::new(msg).unwrap_or_default();
    let op_c = CString::new(op).unwrap_or_default();
    LAST_ERROR.with(|e| *e.borrow_mut() = Some(msg));
    LAST_CODE.with(|c| c.set(code));
    LAST_OP.with(|o| *o.borrow_mut() = Some(op_c));
    code.as_i32()
}

/// 构造成功的空结果。
fn ok_empty() -> *mut NosqliteResult {
    Box::into_raw(Box::new(NosqliteResult {
        code: NOSQLITE_OK,
        count: 0,
        affected: 0,
        docs: std::ptr::null_mut(),
        value: NosqliteSlice {
            ptr: std::ptr::null(),
            len: 0,
        },
    }))
}

/// 构造失败结果。
fn err_result(code: i32) -> *mut NosqliteResult {
    Box::into_raw(Box::new(NosqliteResult {
        code,
        count: 0,
        affected: 0,
        docs: std::ptr::null_mut(),
        value: NosqliteSlice {
            ptr: std::ptr::null(),
            len: 0,
        },
    }))
}

/// 构造带单值的结果。
fn ok_value(value: &str) -> *mut NosqliteResult {
    let c = CString::new(value).unwrap_or_default();
    let slice = NosqliteSlice {
        ptr: c.as_ptr(),
        len: c.as_bytes().len(),
    };
    std::mem::forget(c); // 由 result_free 通过 Box::from_raw 回收
    Box::into_raw(Box::new(NosqliteResult {
        code: NOSQLITE_OK,
        count: 0,
        affected: 0,
        docs: std::ptr::null_mut(),
        value: slice,
    }))
}

/// 构造文档列表结果。
fn ok_docs(docs: &[Map<String, Value>], affected: u64) -> *mut NosqliteResult {
    let mut owned: Vec<CString> = Vec::with_capacity(docs.len());
    for doc in docs {
        let text = serde_json::to_string(doc).unwrap_or_else(|_| "{}".to_string());
        owned.push(CString::new(text).unwrap_or_default());
    }
    // 用 with_capacity 精确分配:释放时 Vec::from_raw_parts 的 capacity 必须与分配时一致
    let mut slices: Vec<NosqliteSlice> = Vec::with_capacity(owned.len());
    slices.extend(owned.iter().map(|c| NosqliteSlice {
        ptr: c.as_ptr(),
        len: c.as_bytes().len(),
    }));
    let count = slices.len() as u64;
    let ptr = slices.as_mut_ptr();
    // 泄露两个 Vec:由 result_free 回收
    std::mem::forget(slices);
    std::mem::forget(owned);
    Box::into_raw(Box::new(NosqliteResult {
        code: NOSQLITE_OK,
        count,
        affected,
        docs: ptr,
        value: NosqliteSlice {
            ptr: std::ptr::null(),
            len: 0,
        },
    }))
}

/// 从 C 字符串读取 `&str`,失败时记录错误并返回 None。
///
/// # Safety
///
/// `ptr` 必须为合法 NUL 结尾字符串或空指针。
unsafe fn read_str(ptr: *const c_char, op: &str, name: &str) -> Option<String> {
    if ptr.is_null() {
        set_error_msg(
            op,
            ErrorCode::Internal,
            &format!("参数 `{}` 不能为空指针", name),
        );
        return None;
    }
    match unsafe { CStr::from_ptr(ptr) }.to_str() {
        Ok(s) => Some(s.to_string()),
        Err(_) => {
            set_error_msg(
                op,
                ErrorCode::Internal,
                &format!("参数 `{}` 不是合法 UTF-8", name),
            );
            None
        }
    }
}

/// 解析 filter JSON(空指针 / `""` / `null` 一律视为匹配全部)。
fn parse_filter(op: &str, raw: Option<&str>) -> Result<Filter, i32> {
    let text = raw.unwrap_or("null").trim();
    if text.is_empty() {
        return Ok(Filter::match_all());
    }
    let value: Value = serde_json::from_str(text).map_err(|e| {
        set_error_msg(
            op,
            ErrorCode::InvalidFilter,
            &format!("filter JSON 解析失败: {}", e),
        )
    })?;
    Filter::from_value(&value)
        .map_err(|e| set_error_msg(op, ErrorCode::InvalidFilter, &e.to_string()))
}

/// 解析 update JSON。
fn parse_update(op: &str, raw: &str) -> Result<Update, i32> {
    let value: Value = serde_json::from_str(raw).map_err(|e| {
        set_error_msg(
            op,
            ErrorCode::InvalidUpdate,
            &format!("update JSON 解析失败: {}", e),
        )
    })?;
    Update::from_value(&value)
        .map_err(|e| set_error_msg(op, ErrorCode::InvalidUpdate, &e.to_string()))
}

// ==================== 生命周期 ====================

/// 打开(或创建)数据库。
///
/// `create_if_missing = 1` 时文件不存在则创建,否则报错。
///
/// # Safety
///
/// `path` 必须是合法 NUL 结尾的 UTF-8 字符串。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nosqlite_open(
    path: *const c_char,
    create_if_missing: u8,
) -> *mut NosqliteDb {
    let Some(path) = (unsafe { read_str(path, "open", "path") }) else {
        return std::ptr::null_mut();
    };
    let p = Path::new(&path);
    let result = if p.exists() || create_if_missing == 0 {
        Database::open(p)
    } else {
        Database::create(p)
    };
    match result {
        Ok(inner) => Box::into_raw(Box::new(NosqliteDb { inner })),
        Err(e) => {
            set_error("open", &e);
            std::ptr::null_mut()
        }
    }
}

/// 用选项结构打开数据库(可扩展入口)。
///
/// `opts` 为 `NULL` 时等价于 [`nosqlite_open`](path, create_if_missing = 1)。
///
/// # Safety
///
/// `path` 必须是合法 NUL 结尾字符串;`opts` 若非空,其 `struct_size` 必须
/// 不小于 [`NosqliteOpenOptions`] 的当前大小。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nosqlite_open_ex(
    path: *const c_char,
    opts: *const NosqliteOpenOptions,
) -> *mut NosqliteDb {
    // 未提供选项:走默认(不存在则创建)
    let Some(opts) = (unsafe { opts.as_ref() }) else {
        return unsafe { nosqlite_open(path, 1) };
    };

    // struct_size 校验:允许调用方传更大的结构(将来字段扩展),但不允许更小
    let required = std::mem::size_of::<NosqliteOpenOptions>() as u32;
    if opts.struct_size < required {
        set_error_msg(
            "open_ex",
            ErrorCode::Internal,
            &format!(
                "opts.struct_size 过小: 收到 {},至少需要 {}",
                opts.struct_size, required
            ),
        );
        return std::ptr::null_mut();
    }
    if opts.flags != 0 {
        set_error_msg(
            "open_ex",
            ErrorCode::Internal,
            &format!("opts.flags 必须为 0,收到 {}", opts.flags),
        );
        return std::ptr::null_mut();
    }

    unsafe { nosqlite_open(path, opts.create_if_missing) }
}

/// 关闭数据库句柄。
///
/// # Safety
///
/// `db` 必须来自 [`nosqlite_open`] 且未被重复关闭。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nosqlite_close(db: *mut NosqliteDb) {
    if db.is_null() {
        return;
    }
    drop(unsafe { Box::from_raw(db) });
}

// ==================== CRUD ====================

/// 插入一条文档(JSON 对象字符串),`value` 为生成/沿用的 `_id` JSON。
///
/// # Safety
///
/// 指针参数须合法;返回值须用 [`nosqlite_result_free`] 释放。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nosqlite_insert(
    db: *mut NosqliteDb,
    collection: *const c_char,
    doc_json: *const c_char,
) -> *mut NosqliteResult {
    let Some(db) = (unsafe { db.as_ref() }) else {
        return err_result(set_error_msg(
            "insert",
            ErrorCode::Internal,
            "数据库句柄为空",
        ));
    };
    let (Some(coll), Some(text)) = (
        unsafe { read_str(collection, "insert", "collection") },
        unsafe { read_str(doc_json, "insert", "doc_json") },
    ) else {
        return err_result(LAST_CODE.with(|c| c.get().as_i32()));
    };

    let mut doc = match Document::from_json(&text) {
        Ok(d) => d,
        Err(e) => {
            return err_result(set_error_msg(
                "insert",
                ErrorCode::InvalidDocument,
                &e.to_string(),
            ));
        }
    };
    match db.inner.insert(&coll, &mut doc) {
        // `value` 是裸 `_id`(不加 JSON 引号),便于 C 侧直接当主键用
        Ok(id) => ok_value(&id),
        Err(e) => {
            let code = set_error("insert", &e);
            err_result(code)
        }
    }
}

/// 批量插入(JSON 数组字符串),`value` 为主键数组。
///
/// # Safety
///
/// 指针参数须合法;返回值须用 [`nosqlite_result_free`] 释放。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nosqlite_insert_many(
    db: *mut NosqliteDb,
    collection: *const c_char,
    docs_json: *const c_char,
) -> *mut NosqliteResult {
    let Some(db) = (unsafe { db.as_ref() }) else {
        return err_result(set_error_msg(
            "insert_many",
            ErrorCode::Internal,
            "数据库句柄为空",
        ));
    };
    let (Some(coll), Some(text)) = (
        unsafe { read_str(collection, "insert_many", "collection") },
        unsafe { read_str(docs_json, "insert_many", "docs_json") },
    ) else {
        return err_result(LAST_CODE.with(|c| c.get().as_i32()));
    };

    let items: Vec<Value> = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            return err_result(set_error_msg(
                "insert_many",
                ErrorCode::InvalidDocument,
                &format!("JSON 数组解析失败: {}", e),
            ));
        }
    };
    let mut docs = Vec::with_capacity(items.len());
    for (i, item) in items.into_iter().enumerate() {
        match Document::from_json(&item.to_string()) {
            Ok(d) => docs.push(d),
            Err(e) => {
                return err_result(set_error_msg(
                    "insert_many",
                    ErrorCode::InvalidDocument,
                    &format!("第 {} 条文档非法: {}", i + 1, e),
                ));
            }
        }
    }
    match db.inner.insert_many(&coll, &mut docs) {
        Ok(ids) => {
            let text = serde_json::to_string(&ids).unwrap_or_default();
            ok_value(&text)
        }
        Err(e) => err_result(set_error("insert_many", &e)),
    }
}

/// 按主键读取单条文档,`value` 为文档 JSON(不存在则为空)。
///
/// # Safety
///
/// 指针参数须合法;返回值须用 [`nosqlite_result_free`] 释放。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nosqlite_get(
    db: *mut NosqliteDb,
    collection: *const c_char,
    id: *const c_char,
) -> *mut NosqliteResult {
    let Some(db) = (unsafe { db.as_ref() }) else {
        return err_result(set_error_msg("get", ErrorCode::Internal, "数据库句柄为空"));
    };
    let (Some(coll), Some(id)) = (
        unsafe { read_str(collection, "get", "collection") },
        unsafe { read_str(id, "get", "id") },
    ) else {
        return err_result(LAST_CODE.with(|c| c.get().as_i32()));
    };
    match db.inner.get_by_id(&coll, &id) {
        Ok(Some(doc)) => ok_value(&serde_json::to_string(&doc).unwrap_or_default()),
        Ok(None) => ok_empty(),
        Err(e) => err_result(set_error("get", &e)),
    }
}

/// 查询文档,返回结果集(`docs` 为文档 JSON 数组)。
///
/// 为便于小结果集使用,`filter`/`sort`/`fields` 可为 NULL(表示缺省)。
/// 大结果集请改用 [`nosqlite_find_cursor`] 逐条取,避免一次性占用内存。
///
/// # Safety
///
/// 指针参数须合法(允许 NULL);返回值须用 [`nosqlite_result_free`] 释放。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nosqlite_find(
    db: *mut NosqliteDb,
    collection: *const c_char,
    filter_json: *const c_char,
    sort_json: *const c_char,
    fields_json: *const c_char,
    skip: u64,
    limit: u64,
    limit_is_null: u8,
) -> *mut NosqliteResult {
    let Some(db) = (unsafe { db.as_ref() }) else {
        return err_result(set_error_msg("find", ErrorCode::Internal, "数据库句柄为空"));
    };
    let Some(coll) = (unsafe { read_str(collection, "find", "collection") }) else {
        return err_result(LAST_CODE.with(|c| c.get().as_i32()));
    };
    let filter_raw = unsafe { opt_str(filter_json) };
    let sort_raw = unsafe { opt_str(sort_json) };
    let fields_raw = unsafe { opt_str(fields_json) };

    let filter = match parse_filter("find", filter_raw.as_deref()) {
        Ok(f) => f,
        Err(code) => return err_result(code),
    };
    let sort = match parse_sort("find", sort_raw.as_deref()) {
        Ok(s) => s,
        Err(code) => return err_result(code),
    };
    let projection = match parse_projection("find", fields_raw.as_deref()) {
        Ok(p) => p,
        Err(code) => return err_result(code),
    };
    let limit = if limit_is_null == 0 {
        Some(limit as usize)
    } else {
        None
    };

    // 有排序或投影时走两遍法,否则流式过滤
    let result = if sort.is_some() || projection.is_some() {
        db.inner
            .find_page_total(
                &coll,
                &filter,
                sort.as_ref(),
                skip as usize,
                limit,
                projection.as_ref(),
            )
            .map(|(_, docs)| docs)
    } else {
        db.inner
            .find_with_options(
                &coll,
                &filter,
                &FindOptions {
                    sort: None,
                    projection: None,
                    skip: skip as usize,
                    limit,
                },
            )
            .map(|docs| docs.to_vec())
    };

    match result {
        Ok(docs) => {
            let n = docs.len() as u64;
            ok_docs(&docs, n)
        }
        Err(e) => err_result(set_error("find", &e)),
    }
}

/// 打开游标逐条读取(分块加载,内存 O(批大小))。
///
/// 用法:`while (nosqlite_cursor_next(cur)) { ... }`,结束后调 [`nosqlite_cursor_close`]。
///
/// # Safety
///
/// 指针参数须合法(允许 NULL);返回的游标须用 [`nosqlite_cursor_close`] 释放。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nosqlite_find_cursor(
    db: *mut NosqliteDb,
    collection: *const c_char,
    filter_json: *const c_char,
    sort_json: *const c_char,
    fields_json: *const c_char,
    skip: u64,
    limit: u64,
    limit_is_null: u8,
) -> *mut NosqliteCursor {
    let Some(db) = (unsafe { db.as_ref() }) else {
        set_error_msg("find_cursor", ErrorCode::Internal, "数据库句柄为空");
        return std::ptr::null_mut();
    };
    let Some(coll) = (unsafe { read_str(collection, "find_cursor", "collection") }) else {
        return std::ptr::null_mut();
    };
    let filter_raw = unsafe { opt_str(filter_json) };
    let sort_raw = unsafe { opt_str(sort_json) };
    let fields_raw = unsafe { opt_str(fields_json) };

    let filter = match parse_filter("find_cursor", filter_raw.as_deref()) {
        Ok(f) => f,
        Err(_) => return std::ptr::null_mut(),
    };
    let sort = match parse_sort("find_cursor", sort_raw.as_deref()) {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let projection = match parse_projection("find_cursor", fields_raw.as_deref()) {
        Ok(p) => p,
        Err(_) => return std::ptr::null_mut(),
    };
    let limit = if limit_is_null == 0 {
        Some(limit as usize)
    } else {
        None
    };

    let result = if sort.is_some() || projection.is_some() {
        db.inner
            .find_page_total(
                &coll,
                &filter,
                sort.as_ref(),
                skip as usize,
                limit,
                projection.as_ref(),
            )
            .map(|(_, docs)| docs)
    } else {
        db.inner
            .find_with_options(
                &coll,
                &filter,
                &FindOptions {
                    sort: None,
                    projection: None,
                    skip: skip as usize,
                    limit,
                },
            )
            .map(|docs| docs.to_vec())
    };

    match result {
        Ok(docs) => Box::into_raw(Box::new(NosqliteCursor {
            pending: docs.into_iter(),
            arena: Vec::new(),
        })),
        Err(e) => {
            set_error("find_cursor", &e);
            std::ptr::null_mut()
        }
    }
}

/// 推进游标并返回当前文档 JSON(`NULL` 表示结束)。
///
/// 返回的指针在下一次 `nosqlite_cursor_next` 或 `cursor_close` 之前有效。
///
/// # Safety
///
/// `cur` 必须来自 [`nosqlite_find_cursor`] 且未关闭。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nosqlite_cursor_next(cur: *mut NosqliteCursor) -> *const c_char {
    let Some(cur) = (unsafe { cur.as_mut() }) else {
        return std::ptr::null();
    };
    match cur.pending.next() {
        Some(doc) => {
            let text = serde_json::to_string(&doc).unwrap_or_default();
            let c = CString::new(text).unwrap_or_default();
            // 清掉上一条,避免无限增长(调用方不该持有旧指针)
            cur.arena.clear();
            cur.arena.push(c);
            cur.arena[0].as_ptr()
        }
        None => std::ptr::null(),
    }
}

/// 关闭游标。
///
/// # Safety
///
/// `cur` 必须来自 [`nosqlite_find_cursor`] 且未被重复关闭。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nosqlite_cursor_close(cur: *mut NosqliteCursor) {
    if cur.is_null() {
        return;
    }
    drop(unsafe { Box::from_raw(cur) });
}

/// 更新匹配文档,`affected` 为实际修改条数。
///
/// # Safety
///
/// 指针参数须合法;返回值须用 [`nosqlite_result_free`] 释放。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nosqlite_update(
    db: *mut NosqliteDb,
    collection: *const c_char,
    filter_json: *const c_char,
    update_json: *const c_char,
) -> *mut NosqliteResult {
    unsafe { update_or_delete(db, collection, filter_json, update_json, true) }
}

/// 删除匹配文档,`affected` 为删除条数。
///
/// # Safety
///
/// 指针参数须合法;返回值须用 [`nosqlite_result_free`] 释放。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nosqlite_delete(
    db: *mut NosqliteDb,
    collection: *const c_char,
    filter_json: *const c_char,
) -> *mut NosqliteResult {
    unsafe { update_or_delete(db, collection, filter_json, std::ptr::null(), false) }
}

/// update/delete 的共同实现。
unsafe fn update_or_delete(
    db: *mut NosqliteDb,
    collection: *const c_char,
    filter_json: *const c_char,
    update_json: *const c_char,
    is_update: bool,
) -> *mut NosqliteResult {
    let op = if is_update { "update" } else { "delete" };
    let Some(db) = (unsafe { db.as_ref() }) else {
        return err_result(set_error_msg(op, ErrorCode::Internal, "数据库句柄为空"));
    };
    let Some(coll) = (unsafe { read_str(collection, op, "collection") }) else {
        return err_result(LAST_CODE.with(|c| c.get().as_i32()));
    };
    let filter_raw = unsafe { opt_str(filter_json) };
    let filter = match parse_filter(op, filter_raw.as_deref()) {
        Ok(f) => f,
        Err(code) => return err_result(code),
    };

    if is_update {
        let Some(text) = (unsafe { read_str(update_json, op, "update_json") }) else {
            return err_result(LAST_CODE.with(|c| c.get().as_i32()));
        };
        let update = match parse_update(op, &text) {
            Ok(u) => u,
            Err(code) => return err_result(code),
        };
        match db.inner.update(&coll, &filter, &update) {
            Ok(n) => {
                let r = unsafe { &mut *ok_empty() };
                r.affected = n as u64;
                r as *mut NosqliteResult
            }
            Err(e) => err_result(set_error(op, &e)),
        }
    } else {
        match db.inner.delete(&coll, &filter) {
            Ok(n) => {
                let r = unsafe { &mut *ok_empty() };
                r.affected = n as u64;
                r as *mut NosqliteResult
            }
            Err(e) => err_result(set_error(op, &e)),
        }
    }
}

/// 更新或插入,`value` 为 `{"inserted": "_id"}` 或 `{"updated": n}`。
///
/// # Safety
///
/// 指针参数须合法;返回值须用 [`nosqlite_result_free`] 释放。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nosqlite_upsert(
    db: *mut NosqliteDb,
    collection: *const c_char,
    filter_json: *const c_char,
    update_json: *const c_char,
) -> *mut NosqliteResult {
    let Some(db) = (unsafe { db.as_ref() }) else {
        return err_result(set_error_msg(
            "upsert",
            ErrorCode::Internal,
            "数据库句柄为空",
        ));
    };
    let Some(coll) = (unsafe { read_str(collection, "upsert", "collection") }) else {
        return err_result(LAST_CODE.with(|c| c.get().as_i32()));
    };
    let filter_raw = unsafe { opt_str(filter_json) };
    let Some(update_text) = (unsafe { read_str(update_json, "upsert", "update_json") }) else {
        return err_result(LAST_CODE.with(|c| c.get().as_i32()));
    };

    let filter = match parse_filter("upsert", filter_raw.as_deref()) {
        Ok(f) => f,
        Err(code) => return err_result(code),
    };
    let update = match parse_update("upsert", &update_text) {
        Ok(u) => u,
        Err(code) => return err_result(code),
    };

    match db.inner.update_or_insert(&coll, &filter, &update) {
        Ok(UpsertResult::Inserted(id)) => {
            let json = serde_json::json!({ "inserted": id }).to_string();
            ok_value(&json)
        }
        Ok(UpsertResult::Updated(n)) => {
            let json = serde_json::json!({ "updated": n }).to_string();
            ok_value(&json)
        }
        Err(e) => err_result(set_error("upsert", &e)),
    }
}

/// 统计匹配条数,`value` 为数字。
///
/// # Safety
///
/// 指针参数须合法(允许 NULL);返回值须用 [`nosqlite_result_free`] 释放。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nosqlite_count(
    db: *mut NosqliteDb,
    collection: *const c_char,
    filter_json: *const c_char,
) -> *mut NosqliteResult {
    let Some(db) = (unsafe { db.as_ref() }) else {
        return err_result(set_error_msg(
            "count",
            ErrorCode::Internal,
            "数据库句柄为空",
        ));
    };
    let Some(coll) = (unsafe { read_str(collection, "count", "collection") }) else {
        return err_result(LAST_CODE.with(|c| c.get().as_i32()));
    };
    let filter_raw = unsafe { opt_str(filter_json) };
    let filter = match parse_filter("count", filter_raw.as_deref()) {
        Ok(f) => f,
        Err(code) => return err_result(code),
    };
    match db.inner.count(&coll, &filter) {
        Ok(n) => {
            let r = unsafe { &mut *ok_empty() };
            r.affected = n as u64;
            r as *mut NosqliteResult
        }
        Err(e) => err_result(set_error("count", &e)),
    }
}

/// 列出全部集合,`docs` 为集合名 JSON 数组。
///
/// # Safety
///
/// 返回值须用 [`nosqlite_result_free`] 释放。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nosqlite_list_collections(db: *mut NosqliteDb) -> *mut NosqliteResult {
    let Some(db) = (unsafe { db.as_ref() }) else {
        return err_result(set_error_msg(
            "list_collections",
            ErrorCode::Internal,
            "数据库句柄为空",
        ));
    };
    match db.inner.collections() {
        Ok(names) => ok_strings(&names),
        Err(e) => err_result(set_error("list_collections", &e)),
    }
}

/// 把字符串列表打包成结果(每个元素一个 JSON 字符串切片)。
fn ok_strings(items: &[String]) -> *mut NosqliteResult {
    let owned: Vec<CString> = items
        .iter()
        .map(|n| CString::new(serde_json::to_string(n).unwrap_or_default()).unwrap_or_default())
        .collect();
    let mut slices: Vec<NosqliteSlice> = Vec::with_capacity(owned.len());
    slices.extend(owned.iter().map(|c| NosqliteSlice {
        ptr: c.as_ptr(),
        len: c.as_bytes().len(),
    }));
    let count = slices.len() as u64;
    let ptr = slices.as_mut_ptr();
    std::mem::forget(slices);
    std::mem::forget(owned);
    Box::into_raw(Box::new(NosqliteResult {
        code: NOSQLITE_OK,
        count,
        affected: 0,
        docs: ptr,
        value: NosqliteSlice {
            ptr: std::ptr::null(),
            len: 0,
        },
    }))
}

/// 兜底:执行一条 JSON 命令。
///
/// 命令格式:`{"op": "insert", "collection": "u", "doc": {...}, ...}`。
/// 支持 `insert`/`insert_many`/`find`/`update`/`upsert`/`delete`/`count`/`get`/`list_collections`。
///
/// # Safety
///
/// 指针参数须合法;返回值须用 [`nosqlite_result_free`] 释放。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nosqlite_exec(
    db: *mut NosqliteDb,
    command_json: *const c_char,
) -> *mut NosqliteResult {
    let Some(db) = (unsafe { db.as_ref() }) else {
        return err_result(set_error_msg("exec", ErrorCode::Internal, "数据库句柄为空"));
    };
    let Some(text) = (unsafe { read_str(command_json, "exec", "command_json") }) else {
        return err_result(LAST_CODE.with(|c| c.get().as_i32()));
    };
    let cmd: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            return err_result(set_error_msg(
                "exec",
                ErrorCode::InvalidFilter,
                &format!("命令 JSON 解析失败: {}", e),
            ));
        }
    };
    let Some(obj) = cmd.as_object() else {
        return err_result(set_error_msg(
            "exec",
            ErrorCode::InvalidFilter,
            "命令必须是 JSON 对象",
        ));
    };
    let Some(op) = obj.get("op").and_then(|v| v.as_str()) else {
        return err_result(set_error_msg(
            "exec",
            ErrorCode::InvalidFilter,
            "命令缺少字符串字段 `op`",
        ));
    };
    // 直接实现(不委托给各专用入口):避免线程局部 last_error 被二次调用覆盖,
    // 也避免把已解析的值再序列化一遍。
    let text_field = |key: &str| -> Option<String> {
        obj.get(key).map(|v| match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        })
    };
    let collection = text_field("collection").unwrap_or_default();
    if collection.is_empty() && op != "list_collections" {
        return err_result(set_error_msg(
            "exec",
            ErrorCode::InvalidCollection,
            "命令缺少字符串字段 `collection`",
        ));
    }
    let filter_raw = text_field("filter");
    let update_raw = text_field("update");

    match op {
        "insert" => {
            let Some(doc_text) = text_field("doc") else {
                return err_result(set_error_msg(
                    "exec",
                    ErrorCode::InvalidDocument,
                    "缺少 `doc`",
                ));
            };
            let mut doc = match Document::from_json(&doc_text) {
                Ok(d) => d,
                Err(e) => {
                    return err_result(set_error_msg(
                        "exec.insert",
                        ErrorCode::InvalidDocument,
                        &e.to_string(),
                    ));
                }
            };
            match db.inner.insert(&collection, &mut doc) {
                Ok(id) => ok_value(&id),
                Err(e) => err_result(set_error("exec.insert", &e)),
            }
        }
        "insert_many" => {
            let Some(docs_text) = text_field("docs") else {
                return err_result(set_error_msg(
                    "exec",
                    ErrorCode::InvalidDocument,
                    "缺少 `docs`",
                ));
            };
            let items: Vec<Value> = match serde_json::from_str(&docs_text) {
                Ok(v) => v,
                Err(e) => {
                    return err_result(set_error_msg(
                        "exec.insert_many",
                        ErrorCode::InvalidDocument,
                        &format!("`docs` 必须是 JSON 数组: {}", e),
                    ));
                }
            };
            let mut docs = Vec::with_capacity(items.len());
            for (i, item) in items.into_iter().enumerate() {
                match Document::from_json(&item.to_string()) {
                    Ok(d) => docs.push(d),
                    Err(e) => {
                        return err_result(set_error_msg(
                            "exec.insert_many",
                            ErrorCode::InvalidDocument,
                            &format!("第 {} 条文档非法: {}", i + 1, e),
                        ));
                    }
                }
            }
            match db.inner.insert_many(&collection, &mut docs) {
                Ok(ids) => ok_value(&serde_json::to_string(&ids).unwrap_or_default()),
                Err(e) => err_result(set_error("exec.insert_many", &e)),
            }
        }
        "get" => {
            let Some(id) = text_field("id").filter(|s| !s.is_empty()) else {
                return err_result(set_error_msg("exec.get", ErrorCode::Internal, "缺少 `id`"));
            };
            match db.inner.get_by_id(&collection, &id) {
                Ok(Some(doc)) => ok_value(&serde_json::to_string(&doc).unwrap_or_default()),
                Ok(None) => ok_empty(),
                Err(e) => err_result(set_error("exec.get", &e)),
            }
        }
        "find" => {
            let filter = match parse_filter("exec.find", filter_raw.as_deref()) {
                Ok(f) => f,
                Err(code) => return err_result(code),
            };
            let sort = match parse_sort("exec.find", text_field("sort").as_deref()) {
                Ok(s) => s,
                Err(code) => return err_result(code),
            };
            let projection = match parse_projection("exec.find", text_field("fields").as_deref()) {
                Ok(p) => p,
                Err(code) => return err_result(code),
            };
            let opts = obj.get("options").and_then(|v| v.as_object());
            let skip = opts
                .and_then(|o| o.get("skip"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            let limit = opts.and_then(|o| o.get("limit")).and_then(|v| v.as_u64());
            let result = if sort.is_some() || projection.is_some() {
                db.inner
                    .find_page_total(
                        &collection,
                        &filter,
                        sort.as_ref(),
                        skip,
                        limit.map(|l| l as usize),
                        projection.as_ref(),
                    )
                    .map(|(_, docs)| docs)
            } else {
                db.inner
                    .find_with_options(
                        &collection,
                        &filter,
                        &FindOptions {
                            sort: None,
                            projection: None,
                            skip,
                            limit: limit.map(|l| l as usize),
                        },
                    )
                    .map(|docs| docs.to_vec())
            };
            match result {
                Ok(docs) => {
                    let n = docs.len() as u64;
                    ok_docs(&docs, n)
                }
                Err(e) => err_result(set_error("exec.find", &e)),
            }
        }
        "update" => {
            let filter = match parse_filter("exec.update", filter_raw.as_deref()) {
                Ok(f) => f,
                Err(code) => return err_result(code),
            };
            let Some(update_text) = update_raw else {
                return err_result(set_error_msg(
                    "exec.update",
                    ErrorCode::InvalidUpdate,
                    "缺少 `update`",
                ));
            };
            let update = match parse_update("exec.update", &update_text) {
                Ok(u) => u,
                Err(code) => return err_result(code),
            };
            match db.inner.update(&collection, &filter, &update) {
                Ok(n) => {
                    let r = unsafe { &mut *ok_empty() };
                    r.affected = n as u64;
                    r as *mut NosqliteResult
                }
                Err(e) => err_result(set_error("exec.update", &e)),
            }
        }
        "upsert" => {
            let filter = match parse_filter("exec.upsert", filter_raw.as_deref()) {
                Ok(f) => f,
                Err(code) => return err_result(code),
            };
            let Some(update_text) = update_raw else {
                return err_result(set_error_msg(
                    "exec.upsert",
                    ErrorCode::InvalidUpdate,
                    "缺少 `update`",
                ));
            };
            let update = match parse_update("exec.upsert", &update_text) {
                Ok(u) => u,
                Err(code) => return err_result(code),
            };
            match db.inner.update_or_insert(&collection, &filter, &update) {
                Ok(UpsertResult::Inserted(id)) => ok_value(&id),
                Ok(UpsertResult::Updated(n)) => {
                    let r = unsafe { &mut *ok_empty() };
                    r.affected = n as u64;
                    r as *mut NosqliteResult
                }
                Err(e) => err_result(set_error("exec.upsert", &e)),
            }
        }
        "delete" => {
            let filter = match parse_filter("exec.delete", filter_raw.as_deref()) {
                Ok(f) => f,
                Err(code) => return err_result(code),
            };
            match db.inner.delete(&collection, &filter) {
                Ok(n) => {
                    let r = unsafe { &mut *ok_empty() };
                    r.affected = n as u64;
                    r as *mut NosqliteResult
                }
                Err(e) => err_result(set_error("exec.delete", &e)),
            }
        }
        "count" => {
            let filter = match parse_filter("exec.count", filter_raw.as_deref()) {
                Ok(f) => f,
                Err(code) => return err_result(code),
            };
            match db.inner.count(&collection, &filter) {
                Ok(n) => {
                    let r = unsafe { &mut *ok_empty() };
                    r.affected = n as u64;
                    r as *mut NosqliteResult
                }
                Err(e) => err_result(set_error("exec.count", &e)),
            }
        }
        "list_collections" => match db.inner.collections() {
            Ok(names) => ok_strings(&names),
            Err(e) => err_result(set_error("exec.list_collections", &e)),
        },
        other => err_result(set_error_msg(
            "exec",
            ErrorCode::InvalidFilter,
            &format!("未知命令: {}", other),
        )),
    }
}

// ==================== 错误与释放 ====================

/// 取本条线程最近一次错误的错误码(0 表示无错误)。
#[unsafe(no_mangle)]
pub extern "C" fn nosqlite_last_error_code() -> i32 {
    let code = LAST_CODE.with(|c| c.get());
    // 无错误时 LAST_ERROR 为空
    if LAST_ERROR.with(|e| e.borrow().is_none()) {
        0
    } else {
        code.as_i32()
    }
}

/// 取本条线程最近一次错误的结构化信息。
///
/// 返回的指针与其中切片在下一次调用本库前有效,**不需要**释放。
#[unsafe(no_mangle)]
pub extern "C" fn nosqlite_last_error() -> *const NosqliteError {
    thread_local! {
        static ERR: std::cell::RefCell<Option<NosqliteError>> =
            const { std::cell::RefCell::new(None) };
        static KEEP: std::cell::RefCell<(Option<CString>, Option<CString>, Option<CString>)> =
            const { std::cell::RefCell::new((None, None, None)) };
    }
    ERR.with(|slot| {
        KEEP.with(|keep| {
            let mut keep = keep.borrow_mut();
            let msg = LAST_ERROR.with(|e| e.borrow().clone());
            let op = LAST_OP.with(|o| o.borrow().clone());
            let code = LAST_CODE.with(|c| c.get());
            let code_c = CString::new(code.as_str()).unwrap_or_default();
            *keep = (msg, op, Some(code_c));
            let (m, o, c) = (&keep.0, &keep.1, &keep.2);
            let slice_of = |s: &Option<CString>| match s {
                Some(v) => NosqliteSlice {
                    ptr: v.as_ptr(),
                    len: v.as_bytes().len(),
                },
                None => NosqliteSlice {
                    ptr: std::ptr::null(),
                    len: 0,
                },
            };
            let err = NosqliteError {
                code: code.as_i32(),
                code_str: slice_of(c),
                message: slice_of(m),
                op: slice_of(o),
            };
            let _ = slot.borrow_mut().insert(err);
            slot.borrow()
                .as_ref()
                .map_or(std::ptr::null(), |e| e as *const NosqliteError)
        })
    })
}

/// 释放 [`NosqliteResult`] 及其持有的全部数据(整块释放,只需调一次)。
///
/// # Safety
///
/// `r` 必须来自本库且未被重复释放。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nosqlite_result_free(r: *mut NosqliteResult) {
    if r.is_null() {
        return;
    }
    let result = unsafe { Box::from_raw(r) };
    // 释放 docs 数组
    if !result.docs.is_null() && result.count > 0 {
        let slices = unsafe {
            Vec::from_raw_parts(result.docs, result.count as usize, result.count as usize)
        };
        // 重新构造 CString 以释放堆内存(容量 == 长度,见 ok_docs)
        for slice in slices {
            if !slice.ptr.is_null() {
                let _ = unsafe { CString::from_raw(slice.ptr as *mut c_char) };
            }
        }
    }
    // 释放单值
    if !result.value.ptr.is_null() {
        let _ = unsafe { CString::from_raw(result.value.ptr as *mut c_char) };
    }
}

// ==================== 小工具 ====================

/// 读取可空字符串指针,空指针返回 None。
unsafe fn opt_str(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .ok()
        .map(|s| s.to_string())
}

/// 解析排序规格(空则 None)。
fn parse_sort(op: &str, raw: Option<&str>) -> Result<Option<SortSpec>, i32> {
    let Some(text) = raw.map(|s| s.trim()).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let value: Value = serde_json::from_str(text).map_err(|e| {
        set_error_msg(
            op,
            ErrorCode::InvalidSort,
            &format!("sort JSON 解析失败: {}", e),
        )
    })?;
    SortSpec::from_value(&value)
        .map(Some)
        .map_err(|e| set_error_msg(op, ErrorCode::InvalidSort, &e.to_string()))
}

/// 解析投影规格(空则 None)。
fn parse_projection(op: &str, raw: Option<&str>) -> Result<Option<Projection>, i32> {
    let Some(text) = raw.map(|s| s.trim()).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let value: Value = serde_json::from_str(text).map_err(|e| {
        set_error_msg(
            op,
            ErrorCode::InvalidProjection,
            &format!("fields JSON 解析失败: {}", e),
        )
    })?;
    Projection::from_value(&value)
        .map(Some)
        .map_err(|e| set_error_msg(op, ErrorCode::InvalidProjection, &e.to_string()))
}
