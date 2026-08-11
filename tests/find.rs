//! find 集成测试(M3)。
//!
//! 覆盖 `Database::find` 端到端流程:插入 → 过滤 → 排序 → skip/limit,
//! 以及集合隔离与数据持久化。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use serde_json::json;

use nosqlite::query::{Filter, FindOptions, SortSpec};
use nosqlite::{Database, Document};

/// 全局自增计数器,用于生成唯一的临时文件路径。
static COUNTER: AtomicU32 = AtomicU32::new(0);

/// 生成唯一的临时 `.nodb` 测试文件路径。
fn temp_nodb(name: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!(
        "nosqlite_test_{}_{}_{}.nodb",
        name,
        std::process::id(),
        n
    ))
}

/// 清理测试遗留文件。
fn cleanup(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// 从 JSON 字符串构造 filter。
fn filter(json: &str) -> Filter {
    let v: serde_json::Value = serde_json::from_str(json).unwrap();
    Filter::from_value(&v).unwrap()
}

/// 辅助:向数据库插入一条文档。
fn insert(db: &Database, collection: &str, json: &str) {
    let mut doc = Document::from_json(json).unwrap();
    db.insert(collection, &mut doc).expect("插入应成功");
}

/// 构造一个含三条用户的测试库。
fn setup_users(path: &Path) {
    let db = Database::create(path).expect("创建数据库应成功");
    insert(&db, "users", r#"{"_id":"u1","name":"张三","age":30,"city":"北京"}"#);
    insert(&db, "users", r#"{"_id":"u2","name":"李四","age":18,"city":"上海"}"#);
    insert(&db, "users", r#"{"_id":"u3","name":"王五","age":45,"city":"北京"}"#);
    insert(&db, "users", r#"{"_id":"u4","name":"赵六","age":22,"city":"广州"}"#);
    db.close().expect("关闭应成功");
    drop(db);
}

/// 空 filter 返回全部文档。
#[test]
fn find_all() {
    let path = temp_nodb("find_all");
    setup_users(&path);
    let db = Database::open(&path).unwrap();

    let docs = db.find("users", &Filter::match_all()).unwrap();
    assert_eq!(docs.len(), 4, "应返回全部 4 条");

    db.close().unwrap();
    drop(db);
    cleanup(&path);
}

/// $eq 过滤。
#[test]
fn find_eq() {
    let path = temp_nodb("find_eq");
    setup_users(&path);
    let db = Database::open(&path).unwrap();

    let docs = db.find("users", &filter(r#"{"city":"北京"}"#)).unwrap();
    assert_eq!(docs.len(), 2, "北京用户应为 2 条");
    assert!(docs.iter().all(|d| d["city"] == "北京"));

    db.close().unwrap();
    drop(db);
    cleanup(&path);
}

/// $gt 数值过滤。
#[test]
fn find_gt() {
    let path = temp_nodb("find_gt");
    setup_users(&path);
    let db = Database::open(&path).unwrap();

    let docs = db.find("users", &filter(r#"{"age":{"$gt":18}}"#)).unwrap();
    assert_eq!(docs.len(), 3, "age>18 应为 3 条");
    assert!(docs.iter().all(|d| d["age"].as_i64().unwrap() > 18));

    db.close().unwrap();
    drop(db);
    cleanup(&path);
}

/// $in 过滤。
#[test]
fn find_in() {
    let path = temp_nodb("find_in");
    setup_users(&path);
    let db = Database::open(&path).unwrap();

    let docs = db
        .find("users", &filter(r#"{"age":{"$in":[18,45]}}"#))
        .unwrap();
    assert_eq!(docs.len(), 2, "age in [18,45] 应为 2 条");

    db.close().unwrap();
    drop(db);
    cleanup(&path);
}

/// $regex 过滤。
#[test]
fn find_regex() {
    let path = temp_nodb("find_regex");
    setup_users(&path);
    let db = Database::open(&path).unwrap();

    let docs = db
        .find("users", &filter(r#"{"name":{"$regex":"^李"}}"#))
        .unwrap();
    assert_eq!(docs.len(), 1, "姓李的应只有 1 条");
    assert_eq!(docs[0]["name"], "李四");

    db.close().unwrap();
    drop(db);
    cleanup(&path);
}

/// 逻辑组合过滤($or)。
#[test]
fn find_or() {
    let path = temp_nodb("find_or");
    setup_users(&path);
    let db = Database::open(&path).unwrap();

    let docs = db
        .find(
            "users",
            &filter(r#"{"$or":[{"age":{"$gt":40}},{"city":"上海"}]}"#),
        )
        .unwrap();
    assert_eq!(docs.len(), 2, "age>40 或 上海 应为 2 条");

    db.close().unwrap();
    drop(db);
    cleanup(&path);
}

/// 点号路径过滤。
#[test]
fn find_dot_path() {
    let path = temp_nodb("find_dot");
    let db = Database::create(&path).unwrap();
    insert(&db, "users", r#"{"_id":"a","addr":{"city":"北京"}}"#);
    insert(&db, "users", r#"{"_id":"b","addr":{"city":"上海"}}"#);

    let docs = db.find("users", &filter(r#"{"addr.city":"北京"}"#)).unwrap();
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0]["_id"], "a");

    db.close().unwrap();
    drop(db);
    cleanup(&path);
}

/// 排序:age 降序。
#[test]
fn find_sort_desc() {
    let path = temp_nodb("find_sort");
    setup_users(&path);
    let db = Database::open(&path).unwrap();

    let sort = SortSpec::from_value(&json!({"age": -1})).unwrap();
    let options = FindOptions {
        sort: Some(sort),
        skip: 0,
        limit: None,
    };
    let docs = db.find_with_options("users", &Filter::match_all(), &options).unwrap();
    let ages: Vec<i64> = docs.iter().map(|d| d["age"].as_i64().unwrap()).collect();
    assert_eq!(ages, vec![45, 30, 22, 18], "age 降序");

    db.close().unwrap();
    drop(db);
    cleanup(&path);
}

/// limit 限制返回条数。
#[test]
fn find_limit() {
    let path = temp_nodb("find_limit");
    setup_users(&path);
    let db = Database::open(&path).unwrap();

    let sort = SortSpec::from_value(&json!({"age": -1})).unwrap();
    let options = FindOptions {
        sort: Some(sort),
        skip: 0,
        limit: Some(2),
    };
    let docs = db.find_with_options("users", &Filter::match_all(), &options).unwrap();
    assert_eq!(docs.len(), 2, "limit 2 应只返回 2 条");
    assert_eq!(docs[0]["age"].as_i64(), Some(45));
    assert_eq!(docs[1]["age"].as_i64(), Some(30));

    db.close().unwrap();
    drop(db);
    cleanup(&path);
}

/// skip + limit 组合分页(跳过前 2 条,取 1 条)。
#[test]
fn find_skip_limit() {
    let path = temp_nodb("find_page");
    setup_users(&path);
    let db = Database::open(&path).unwrap();

    let sort = SortSpec::from_value(&json!({"age": -1})).unwrap();
    let options = FindOptions {
        sort: Some(sort),
        skip: 2,
        limit: Some(1),
    };
    let docs = db.find_with_options("users", &Filter::match_all(), &options).unwrap();
    assert_eq!(docs.len(), 1, "跳过2取1 应只有 1 条");
    assert_eq!(docs[0]["age"].as_i64(), Some(22), "降序第3条 age=22");

    db.close().unwrap();
    drop(db);
    cleanup(&path);
}

/// filter + sort + limit 组合。
#[test]
fn find_filter_sort_limit() {
    let path = temp_nodb("find_combo");
    setup_users(&path);
    let db = Database::open(&path).unwrap();

    let f = filter(r#"{"city":"北京"}"#);
    let sort = SortSpec::from_value(&json!({"age": -1})).unwrap();
    let options = FindOptions {
        sort: Some(sort),
        skip: 0,
        limit: Some(1),
    };
    let docs = db.find_with_options("users", &f, &options).unwrap();
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0]["name"], "王五", "北京用户按年龄降序第一个是王五(45)");

    db.close().unwrap();
    drop(db);
    cleanup(&path);
}

/// 集合隔离:不同集合的文档互不影响。
#[test]
fn find_collection_isolation() {
    let path = temp_nodb("find_iso");
    let db = Database::create(&path).unwrap();
    insert(&db, "users", r#"{"_id":"a","name":"张三"}"#);
    insert(&db, "orders", r#"{"_id":"a","item":"手机"}"#);

    let users = db.find("users", &Filter::match_all()).unwrap();
    assert_eq!(users.len(), 1);
    assert!(users[0].contains_key("name"));

    let orders = db.find("orders", &Filter::match_all()).unwrap();
    assert_eq!(orders.len(), 1);
    assert!(orders[0].contains_key("item"));

    db.close().unwrap();
    drop(db);
    cleanup(&path);
}

/// 查询不存在的集合返回空列表(而非报错)。
#[test]
fn find_missing_collection_empty() {
    let path = temp_nodb("find_missing");
    let db = Database::create(&path).unwrap();
    let docs = db.find("nope", &Filter::match_all()).unwrap();
    assert!(docs.is_empty(), "不存在集合应返回空列表");

    db.close().unwrap();
    drop(db);
    cleanup(&path);
}
