//! upsert(v0.1.4):`Database::update_or_insert`。
//!
//! 语义对齐 MongoDB `updateOne(filter, update, {upsert: true})`:
//! 命中则更新,未命中则用 filter 的等值字段构造新文档并插入。

use std::path::PathBuf;

use nosqlite::query::{Filter, Update};
use nosqlite::{Database, Document, UpsertResult};
use serde_json::{Map, Value, json};

/// 生成本次测试独占的临时 `.nodb` 路径。
fn temp_nodb(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "nosqlite_upsert_{}_{}_{}.nodb",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&path);
    path
}

/// 构造 filter。
fn f(value: Value) -> Filter {
    Filter::from_value(&value).expect("filter 应解析成功")
}

/// 构造更新。
fn u(value: Value) -> Update {
    Update::from_value(&value).expect("update 应解析成功")
}

/// 建一个空库。
fn setup(tag: &str) -> (PathBuf, Database) {
    let path = temp_nodb(tag);
    let db = Database::create(&path).unwrap();
    (path, db)
}

/// 取集合内唯一文档。
fn only(db: &Database, coll: &str) -> Map<String, Value> {
    let all = db.find(coll, &f(json!({}))).unwrap();
    assert_eq!(all.len(), 1, "应只有一条文档");
    all.into_iter().next().unwrap()
}

/// 未命中 → 按 filter 等值字段 + update 新建文档。
#[test]
fn inserts_when_no_match() {
    let (path, db) = setup("insert");

    let result = db
        .update_or_insert(
            "users",
            &f(json!({"email": "a@b.c"})),
            &u(json!({"$set": {"name": "张三"}})),
        )
        .expect("upsert 应成功");

    let UpsertResult::Inserted(id) = result else {
        panic!("应返回 Inserted,实际: {:?}", result);
    };
    let doc = db.get_by_id("users", &id).unwrap().unwrap();
    assert_eq!(doc["email"], json!("a@b.c"), "filter 等值字段应作为初始值");
    assert_eq!(doc["name"], json!("张三"));
    assert_eq!(doc["_id"], json!(id));

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 命中 → 更新,不新建。
#[test]
fn updates_when_match() {
    let (path, db) = setup("update");
    let mut doc = Document::from_json(r#"{"email": "a@b.c", "name": "旧"}"#).unwrap();
    db.insert("users", &mut doc).unwrap();

    let result = db
        .update_or_insert(
            "users",
            &f(json!({"email": "a@b.c"})),
            &u(json!({"$set": {"name": "新"}})),
        )
        .expect("upsert 应成功");

    assert_eq!(result, UpsertResult::Updated(1));
    assert_eq!(db.count("users", &f(json!({}))).unwrap(), 1, "不应新增");
    assert_eq!(only(&db, "users")["name"], json!("新"));

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 命中但内容无变化 → 返回 `Updated(0)`,不新建。
#[test]
fn matched_but_unchanged_is_updated_zero() {
    let (path, db) = setup("noop");
    let mut doc = Document::from_json(r#"{"email": "a@b.c", "name": "同一值"}"#).unwrap();
    db.insert("users", &mut doc).unwrap();

    let result = db
        .update_or_insert(
            "users",
            &f(json!({"email": "a@b.c"})),
            &u(json!({"$set": {"name": "同一值"}})),
        )
        .expect("upsert 应成功");

    assert_eq!(
        result,
        UpsertResult::Updated(0),
        "命中但未改应判为 Updated(0)"
    );
    assert_eq!(db.count("users", &f(json!({}))).unwrap(), 1);

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 多字段等值全部作为初始值。
#[test]
fn multiple_eq_fields_become_initial_values() {
    let (path, db) = setup("multi");

    db.update_or_insert(
        "users",
        &f(json!({"tenant": "t1", "email": "a@b.c"})),
        &u(json!({"$set": {"role": "admin"}})),
    )
    .unwrap();

    let doc = only(&db, "users");
    assert_eq!(doc["tenant"], json!("t1"));
    assert_eq!(doc["email"], json!("a@b.c"));
    assert_eq!(doc["role"], json!("admin"));

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// filter 里的非等值条件无法反推初始值,应报错(不静默忽略)。
#[test]
fn non_eq_filter_rejected() {
    let (path, db) = setup("non_eq");

    let err = db
        .update_or_insert(
            "users",
            &f(json!({"age": {"$gt": 18}})),
            &u(json!({"$set": {"name": "x"}})),
        )
        .expect_err("含操作符的 filter 应报错");
    assert!(err.to_string().contains("等值"), "实际: {}", err);
    assert_eq!(db.count("users", &f(json!({}))).unwrap(), 0, "不应插入");

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// filter 含 `$or` 时同样报错。
#[test]
fn logical_filter_rejected() {
    let (path, db) = setup("logic");

    let err = db
        .update_or_insert(
            "users",
            &f(json!({"$or": [{"a": 1}]})),
            &u(json!({"$set": {"name": "x"}})),
        )
        .expect_err("含逻辑组合的 filter 应报错");
    assert!(err.to_string().contains("等值"), "实际: {}", err);

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// filter 给了 `_id` 时沿用该主键。
#[test]
fn explicit_id_is_used() {
    let (path, db) = setup("id");

    let result = db
        .update_or_insert(
            "users",
            &f(json!({"_id": "u1"})),
            &u(json!({"$set": {"name": "x"}})),
        )
        .unwrap();
    assert_eq!(result, UpsertResult::Inserted("u1".to_string()));
    assert!(db.get_by_id("users", "u1").unwrap().is_some());

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// upsert 与新的更新操作符配合($inc 建新文档时从 0 开始)。
#[test]
fn upsert_with_inc_starts_from_zero() {
    let (path, db) = setup("inc");

    db.update_or_insert(
        "stats",
        &f(json!({"page": "/home"})),
        &u(json!({"$inc": {"views": 1}})),
    )
    .unwrap();

    let doc = only(&db, "stats");
    assert_eq!(doc["page"], json!("/home"));
    assert_eq!(doc["views"], json!(1), "新文档 $inc 从 0 起算");

    // 再次 upsert 应命中并累加
    let result = db
        .update_or_insert(
            "stats",
            &f(json!({"page": "/home"})),
            &u(json!({"$inc": {"views": 1}})),
        )
        .unwrap();
    assert_eq!(result, UpsertResult::Updated(1));
    assert_eq!(only(&db, "stats")["views"], json!(2));

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// upsert 与 `$push`/`$addToSet` 配合:未命中时建数组。
#[test]
fn upsert_with_push_creates_array() {
    let (path, db) = setup("push");

    db.update_or_insert(
        "tags",
        &f(json!({"user": "u1"})),
        &u(json!({"$addToSet": {"tags": "rust"}})),
    )
    .unwrap();

    let doc = only(&db, "tags");
    assert_eq!(doc["user"], json!("u1"));
    assert_eq!(doc["tags"], json!(["rust"]));

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 空 filter(匹配全部)在非空集合上会命中;空集合上则新建空文档。
#[test]
fn empty_filter_on_empty_collection() {
    let (path, db) = setup("empty");

    let result = db
        .update_or_insert("c", &f(json!({})), &u(json!({"$set": {"a": 1}})))
        .unwrap();
    assert!(matches!(result, UpsertResult::Inserted(_)));
    let doc = only(&db, "c");
    assert_eq!(doc["a"], json!(1));

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 结果持久化(重新打开仍存在)。
#[test]
fn upsert_persists() {
    let (path, db) = setup("persist");
    db.update_or_insert(
        "users",
        &f(json!({"email": "a@b.c"})),
        &u(json!({"$set": {"name": "张三"}})),
    )
    .unwrap();
    drop(db);

    let db = Database::open(&path).unwrap();
    let doc = only(&db, "users");
    assert_eq!(doc["email"], json!("a@b.c"));
    assert_eq!(doc["name"], json!("张三"));

    drop(db);
    let _ = std::fs::remove_file(path);
}
