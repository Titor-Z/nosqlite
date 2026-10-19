//! update 功能测试(M4)。
//!
//! 覆盖 `Database::update` 的 `$set` 更新操作:单字段、多字段、点号路径嵌套赋值、
//! 中间路径自动创建、`_id` 不可修改、非匹配文档不受影响、持久化与返回条数。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use nodblite::query::{Filter, Update};
use nodblite::{Database, DatabaseError, Document};

/// 全局自增计数器,用于生成唯一的临时文件路径。
static COUNTER: AtomicU32 = AtomicU32::new(0);

/// 生成唯一的临时 `.nodb` 测试文件路径。
fn temp_nodb(name: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!(
        "nodblite_test_{}_{}_{}.nodb",
        name,
        std::process::id(),
        n
    ))
}

/// 清理测试遗留文件。
fn cleanup(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// 辅助:创建数据库并插入两条文档。
fn setup(path: &Path) -> Database {
    let db = Database::create(path).expect("创建数据库应成功");
    let mut a = Document::from_json(r#"{"_id":"u1","name":"张三","age":20}"#).unwrap();
    let mut b = Document::from_json(r#"{"_id":"u2","name":"李四","age":30}"#).unwrap();
    db.insert("users", &mut a).expect("插入 u1 应成功");
    db.insert("users", &mut b).expect("插入 u2 应成功");
    db
}

/// 辅助:解析 filter。
fn filter(json: &str) -> Filter {
    let v: serde_json::Value = serde_json::from_str(json).expect("测试 JSON 应合法");
    Filter::from_value(&v).expect("filter 解析应成功")
}

/// 辅助:解析 update。
fn update(json: &str) -> Update {
    let v: serde_json::Value = serde_json::from_str(json).expect("测试 JSON 应合法");
    Update::from_value(&v).expect("update 解析应成功")
}

/// 辅助:查询全部文档并按 _id 排序返回。
fn find_all(db: &Database) -> Vec<serde_json::Map<String, serde_json::Value>> {
    let mut docs = db
        .find(
            "users",
            &Filter::from_value(&serde_json::Value::Null).unwrap(),
        )
        .expect("查询应成功");
    docs.sort_by(|a, b| a["_id"].as_str().cmp(&b["_id"].as_str()));
    docs
}

/// 单字段 $set:更新匹配文档的字段。
#[test]
fn set_single_field() {
    let path = temp_nodb("set_single");
    let db = setup(&path);

    // 把 age 从 20 改成 21
    let n = db
        .update(
            "users",
            &filter(r#"{"_id":"u1"}"#),
            &update(r#"{"$set":{"age":21}}"#),
        )
        .expect("更新应成功");
    assert_eq!(n, 1, "应修改 1 条文档");

    let docs = find_all(&db);
    assert_eq!(docs[0]["age"], 21, "u1 的 age 应被更新为 21");
    assert_eq!(docs[1]["age"], 30, "u2 不应被修改");

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// filter 匹配多条:全部被更新,返回修改条数。
#[test]
fn set_multiple_matches() {
    let path = temp_nodb("set_multi");
    let db = setup(&path);

    // 所有 age >= 20 的用户打上 vip 标记
    let n = db
        .update(
            "users",
            &filter(r#"{"age":{"$gte":20}}"#),
            &update(r#"{"$set":{"vip":true}}"#),
        )
        .expect("更新应成功");
    assert_eq!(n, 2, "两条文档都应被修改");

    let docs = find_all(&db);
    assert_eq!(docs[0]["vip"], true);
    assert_eq!(docs[1]["vip"], true);

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// filter 无匹配:返回 0,文档不变。
#[test]
fn set_no_match() {
    let path = temp_nodb("set_nomatch");
    let db = setup(&path);

    let n = db
        .update(
            "users",
            &filter(r#"{"_id":"nobody"}"#),
            &update(r#"{"$set":{"vip":true}}"#),
        )
        .expect("更新应成功");
    assert_eq!(n, 0, "无匹配文档应返回 0");

    let docs = find_all(&db);
    assert_eq!(docs.len(), 2, "文档数量不应变化");

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 点号路径赋值:嵌套字段更新。
#[test]
fn set_dot_path() {
    let path = temp_nodb("set_dot");
    let db = Database::create(&path).expect("创建数据库应成功");
    let mut doc = Document::from_json(r#"{"_id":"o1","addr":{"city":"北京","code":100}}"#).unwrap();
    db.insert("orders", &mut doc).expect("插入应成功");

    let n = db
        .update(
            "orders",
            &filter(r#"{"_id":"o1"}"#),
            &update(r#"{"$set":{"addr.city":"上海"}}"#),
        )
        .expect("更新应成功");
    assert_eq!(n, 1);

    let docs = db
        .find("orders", &filter(r#"{"_id":"o1"}"#))
        .expect("查询应成功");
    assert_eq!(docs[0]["addr"]["city"], "上海", "嵌套字段应被更新");
    assert_eq!(docs[0]["addr"]["code"], 100, "兄弟字段不应被影响");

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 点号路径:中间对象不存在时自动创建。
#[test]
fn set_dot_path_auto_create() {
    let path = temp_nodb("set_autocreate");
    let db = Database::create(&path).expect("创建数据库应成功");
    let mut doc = Document::from_json(r#"{"_id":"o1"}"#).unwrap();
    db.insert("orders", &mut doc).expect("插入应成功");

    let n = db
        .update(
            "orders",
            &filter(r#"{"_id":"o1"}"#),
            &update(r#"{"$set":{"profile.nick":"小张"}}"#),
        )
        .expect("更新应成功");
    assert_eq!(n, 1);

    let docs = db
        .find("orders", &filter(r#"{"_id":"o1"}"#))
        .expect("查询应成功");
    assert_eq!(docs[0]["profile"]["nick"], "小张", "中间对象应自动创建");

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 多字段 $set:一次更新多个字段。
#[test]
fn set_multiple_fields() {
    let path = temp_nodb("set_fields");
    let db = setup(&path);

    let n = db
        .update(
            "users",
            &filter(r#"{"_id":"u1"}"#),
            &update(r#"{"$set":{"age":99,"name":"王五","tags":["a","b"]}}"#),
        )
        .expect("更新应成功");
    assert_eq!(n, 1);

    let docs = find_all(&db);
    assert_eq!(docs[0]["age"], 99);
    assert_eq!(docs[0]["name"], "王五");
    assert_eq!(docs[0]["tags"], serde_json::json!(["a", "b"]));

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 修改主键 _id 应被拒绝。
#[test]
fn set_id_rejected() {
    let path = temp_nodb("set_id");
    let db = setup(&path);

    let err = db
        .update(
            "users",
            &filter(r#"{"_id":"u1"}"#),
            &update(r#"{"$set":{"_id":"u9"}}"#),
        )
        .expect_err("修改 _id 应失败");
    assert!(
        matches!(err, DatabaseError::Update(_)),
        "应报 Update 错误,实际: {:?}",
        err
    );

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 未匹配的 filter 组合逻辑也能正确限制更新范围。
#[test]
fn set_with_logic_filter() {
    let path = temp_nodb("set_logic");
    let db = setup(&path);

    let n = db
        .update(
            "users",
            &filter(r#"{"$or":[{"_id":"u1"},{"_id":"u2"}]}"#),
            &update(r#"{"$set":{"active":true}}"#),
        )
        .expect("更新应成功");
    assert_eq!(n, 2, "$or 应命中两条");

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 更新后持久化:重新打开数据库仍能看到更新结果。
#[test]
fn update_persists_across_reopen() {
    let path = temp_nodb("update_persist");
    let db = setup(&path);
    db.update(
        "users",
        &filter(r#"{"_id":"u1"}"#),
        &update(r#"{"$set":{"age":21}}"#),
    )
    .expect("更新应成功");
    db.close().expect("关闭应成功");
    drop(db); // Windows 下 redb 持有独占锁,必须先释放句柄

    let db = Database::open(&path).expect("重新打开应成功");
    let docs = db
        .find("users", &filter(r#"{"_id":"u1"}"#))
        .expect("查询应成功");
    assert_eq!(docs[0]["age"], 21, "重新打开后应仍保留更新结果");

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}
