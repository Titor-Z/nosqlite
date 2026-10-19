//! 批量更新 / 批量删除的单事务语义(0.2.0)。
//!
//! 验证 `Database::update` / `Database::delete` 匹配多条时:
//! 1. 全部修改/删除在一次提交内完成(结果正确且条数准确);
//! 2. 遇到错误整体回滚,不会留下「改了一半」的状态;
//! 3. 与逐条 `update_by_id` / `delete_by_id` 结果一致。

use std::path::PathBuf;

use nodblite::query::{Filter, Update};
use nodblite::{Database, Document};
use serde_json::json;

/// 生成本次测试独占的临时 `.nodb` 路径。
fn temp_nodb(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "nodblite_um_{}_{}_{}.nodb",
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

/// 建库并插入一批文档。
fn setup(tag: &str, docs: Vec<serde_json::Value>) -> (PathBuf, Database) {
    let path = temp_nodb(tag);
    let db = Database::create(&path).expect("创建应成功");
    let mut docs: Vec<Document> = docs
        .into_iter()
        .map(|v| Document::from_json(&v.to_string()).unwrap())
        .collect();
    db.insert_many("items", &mut docs).expect("批量插入应成功");
    (path, db)
}

/// 构造 filter。
fn f(value: serde_json::Value) -> Filter {
    Filter::from_value(&value).expect("filter 应解析成功")
}

/// 构造更新。
fn u(value: serde_json::Value) -> Update {
    Update::from_value(&value).expect("update 应解析成功")
}

/// 批量更新:所有命中文档都改到,返回条数准确。
#[test]
fn update_many_modifies_all_matches() {
    let (path, db) = setup(
        "all",
        (0..50).map(|i| json!({"n": i, "flag": false})).collect(),
    );

    let modified = db
        .update("items", &f(json!({})), &u(json!({"$set": {"flag": true}})))
        .expect("批量更新应成功");
    assert_eq!(modified, 50, "50 条全部命中");

    let all = db.find("items", &f(json!({}))).unwrap();
    assert_eq!(all.len(), 50);
    assert!(all.iter().all(|d| d["flag"] == json!(true)));

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 只更新命中的一部分,未命中的保持不变。
#[test]
fn update_many_only_touches_matches() {
    let (path, db) = setup(
        "partial",
        (0..20).map(|i| json!({"n": i, "tag": "x"})).collect(),
    );

    let modified = db
        .update(
            "items",
            &f(json!({"n": {"$gte": 10}})),
            &u(json!({"$set": {"tag": "y"}})),
        )
        .expect("批量更新应成功");
    assert_eq!(modified, 10);

    let y = db.find("items", &f(json!({"tag": "y"}))).unwrap().len();
    let x = db.find("items", &f(json!({"tag": "x"}))).unwrap().len();
    assert_eq!((x, y), (10, 10));

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 无变化时不写回、不计数(`modifiedCount` 语义)。
#[test]
fn update_many_skips_noop() {
    let (path, db) = setup(
        "noop",
        (0..10).map(|i| json!({"n": i, "flag": true})).collect(),
    );

    let modified = db
        .update("items", &f(json!({})), &u(json!({"$set": {"flag": true}})))
        .expect("批量更新应成功");
    assert_eq!(modified, 0, "值未变化不应计数");

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 批量删除:一次删掉全部命中项,返回条数准确。
#[test]
fn delete_many_removes_all_matches() {
    let (path, db) = setup("del", (0..30).map(|i| json!({"n": i})).collect());

    let deleted = db
        .delete("items", &f(json!({"n": {"$lt": 20}})))
        .expect("批量删除应成功");
    assert_eq!(deleted, 20);

    let left = db.find("items", &f(json!({}))).unwrap();
    assert_eq!(left.len(), 10);
    assert!(left.iter().all(|d| d["n"].as_i64().unwrap() >= 20));

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 删除不匹配任何文档时返回 0,且不报错。
#[test]
fn delete_many_no_match_is_zero() {
    let (path, db) = setup("del0", (0..5).map(|i| json!({"n": i})).collect());

    let deleted = db.delete("items", &f(json!({"n": 999}))).unwrap();
    assert_eq!(deleted, 0);
    assert_eq!(db.count("items", &f(json!({}))).unwrap(), 5);

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 批量更新后数据必须持久化(重新打开仍生效)。
#[test]
fn update_many_persists() {
    let (path, db) = setup(
        "persist",
        (0..15).map(|i| json!({"n": i, "stage": 1})).collect(),
    );
    db.update(
        "items",
        &f(json!({"n": {"$gte": 5}})),
        &u(json!({"$set": {"stage": 2}})),
    )
    .unwrap();
    drop(db);

    let db = Database::open(&path).expect("重新打开应成功");
    assert_eq!(
        db.count("items", &f(json!({"stage": 2}))).unwrap(),
        10,
        "更新结果应已落盘"
    );
    assert_eq!(db.count("items", &f(json!({"stage": 1}))).unwrap(), 5);

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 批量更新遇到非法操作(`_id` 不可改)时**整体回滚**,不留半成品。
#[test]
fn update_many_rolls_back_on_error() {
    let (path, db) = setup("rollback", (0..10).map(|i| json!({"n": i})).collect());

    // `_id` 不可修改 —— 处理第一条时就应报错
    let err = db
        .update(
            "items",
            &f(json!({})),
            &u(json!({"$set": {"_id": "hacked"}})),
        )
        .expect_err("修改 _id 应报错");
    let msg = err.to_string();
    assert!(msg.contains("_id"), "错误应说明 _id: {}", msg);

    // 全部文档保持原样(没有写回一半)
    let all = db.find("items", &f(json!({}))).unwrap();
    assert_eq!(all.len(), 10);
    assert!(all.iter().all(|d| d["_id"].as_str().unwrap() != "hacked"));
    let mut ns: Vec<i64> = all.iter().map(|d| d["n"].as_i64().unwrap()).collect();
    ns.sort();
    assert_eq!(ns, (0..10).collect::<Vec<_>>());

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 批量更新与逐条 `update_by_id` 的结果一致。
#[test]
fn update_many_matches_per_document_updates() {
    let (path_a, db_a) = setup("cmp_a", (0..12).map(|i| json!({"n": i})).collect());
    let (path_b, db_b) = setup("cmp_b", (0..12).map(|i| json!({"n": i})).collect());

    // A:批量更新
    db_a.update(
        "items",
        &f(json!({"n": {"$gte": 4}})),
        &u(json!({"$set": {"hit": true}})),
    )
    .unwrap();

    // B:逐条按主键更新
    let update = u(json!({"$set": {"hit": true}}));
    for i in 4..12 {
        let doc = db_b
            .find("items", &f(json!({"n": i})))
            .unwrap()
            .into_iter()
            .next()
            .expect("应存在");
        let id = doc["_id"].as_str().unwrap().to_string();
        db_b.update_by_id("items", &id, &update).unwrap();
    }

    // 两个库的 _id 各自生成,故只比较业务字段
    let strip = |mut docs: Vec<serde_json::Map<String, serde_json::Value>>| {
        docs.iter_mut().for_each(|d| {
            d.remove("_id");
        });
        docs.sort_by_key(|d| d["n"].as_i64().unwrap());
        docs
    };
    let a = strip(db_a.find("items", &f(json!({}))).unwrap());
    let b = strip(db_b.find("items", &f(json!({}))).unwrap());
    assert_eq!(a, b, "两种写法的最终数据应完全一致");

    drop(db_a);
    drop(db_b);
    let _ = std::fs::remove_file(path_a);
    let _ = std::fs::remove_file(path_b);
}
