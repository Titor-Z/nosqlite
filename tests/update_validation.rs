//! 更新参数校验与空更新语义测试(M9,缺陷 #2 回归)。
//!
//! M9 前:
//! - 空更新(`{}` 或 `{"$set": {}}`)被静默当成无操作,上层无从察觉写错了;
//! - `Database::update` 对"内容无变化"的文档也照样写回并计入修改条数,造成写放大与虚增。
//!
//! 本文件覆盖修复后的语义:
//! - 各种非法/空更新在**解析阶段**就报 [`UpdateError`];
//! - 内容无变化的文档不写回、不计数(与 MongoDB `modifiedCount` 一致)。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use nosqlite::query::{Filter, Update, UpdateError};
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

/// 辅助:解析 filter。
fn filter(json: &str) -> Filter {
    let v: serde_json::Value = serde_json::from_str(json).expect("测试 JSON 应合法");
    Filter::from_value(&v).expect("filter 解析应成功")
}

/// 辅助:解析 update(允许失败,便于断言错误)。
fn try_update(json: &str) -> Result<Update, UpdateError> {
    let v: serde_json::Value = serde_json::from_str(json).expect("测试 JSON 字面量本身应合法");
    Update::from_value(&v)
}

/// 辅助:解析 update 并断言成功。
fn update(json: &str) -> Update {
    try_update(json).expect("update 解析应成功")
}

/// 空对象更新被拒绝。
#[test]
fn empty_update_object_rejected() {
    let err = try_update("{}").expect_err("空对象 update 应被拒绝");
    assert!(
        matches!(err, UpdateError::InvalidUpdate(_)),
        "应报 InvalidUpdate,实际: {:?}",
        err
    );
}

/// 只有未知操作符的更新被拒绝。
#[test]
fn unknown_operator_rejected() {
    let err = try_update(r#"{"$inc":{"age":1}}"#).expect_err("未知操作符应被拒绝");
    assert!(
        matches!(err, UpdateError::InvalidUpdate(ref m) if m.contains("$inc")),
        "应报不支持的操作符并回显操作符名,实际: {:?}",
        err
    );
}

/// 空 `$set` 被拒绝。
#[test]
fn empty_set_rejected() {
    let err = try_update(r#"{"$set":{}}"#).expect_err("空 $set 应被拒绝");
    assert!(
        matches!(err, UpdateError::InvalidUpdate(_)),
        "应报 InvalidUpdate,实际: {:?}",
        err
    );
}

/// `$set` 的值不是对象时被拒绝。
#[test]
fn set_not_object_rejected() {
    let err = try_update(r#"{"$set":5}"#).expect_err("$set 非对象应被拒绝");
    assert!(
        matches!(err, UpdateError::SetNotObject),
        "应报 SetNotObject,实际: {:?}",
        err
    );
}

/// 含空路径段的字段名被拒绝(`a..b`、`.a`、`a.`)。
#[test]
fn empty_path_segment_rejected() {
    for json in [
        r#"{"$set":{"a..b":1}}"#,
        r#"{"$set":{".a":1}}"#,
        r#"{"$set":{"a.":1}}"#,
    ] {
        let err = try_update(json).expect_err(&format!("路径字面量 `{}` 应解析失败", json));
        assert!(
            matches!(err, UpdateError::InvalidUpdate(_)),
            "路径 `{}` 应报 InvalidUpdate,实际: {:?}",
            json,
            err
        );
    }
}

/// `$set` 写入相同值时:不计数(modified = 0)。
#[test]
fn noop_same_value_not_counted() {
    let path = temp_nodb("noop_same");
    let db = Database::create(&path).expect("创建数据库应成功");
    let mut doc = Document::from_json(r#"{"_id":"u1","vip":true,"age":20}"#).unwrap();
    db.insert("users", &mut doc).expect("插入应成功");

    let n = db
        .update(
            "users",
            &filter(r#"{"_id":"u1"}"#),
            &update(r#"{"$set":{"vip":true,"age":20}}"#),
        )
        .expect("更新应成功");
    assert_eq!(n, 0, "写入相同值不应计为修改");

    let docs = db
        .find("users", &filter(r#"{"_id":"u1"}"#))
        .expect("查询应成功");
    assert_eq!(docs[0]["vip"], true, "文档内容应保持原样");
    assert_eq!(docs[0]["age"], 20, "文档内容应保持原样");

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 部分文档无变化时:只统计真正变化的那些。
#[test]
fn only_changed_documents_counted() {
    let path = temp_nodb("partial_noop");
    let db = Database::create(&path).expect("创建数据库应成功");
    let mut u1 = Document::from_json(r#"{"_id":"u1","vip":true}"#).unwrap();
    let mut u2 = Document::from_json(r#"{"_id":"u2","vip":false}"#).unwrap();
    db.insert("users", &mut u1).expect("插入 u1 应成功");
    db.insert("users", &mut u2).expect("插入 u2 应成功");

    // filter 命中两条,但 u1 已是 vip:true,只有 u2 真正被修改
    let n = db
        .update(
            "users",
            &filter(r#"{"vip":{"$in":[true,false]}}"#),
            &update(r#"{"$set":{"vip":true}}"#),
        )
        .expect("更新应成功");
    assert_eq!(n, 1, "只有 u2 内容发生变化,应只计 1 条");

    let docs = db.find("users", &filter(r#"{}"#)).expect("查询应成功");
    assert_eq!(docs.len(), 2);
    assert!(docs.iter().all(|d| d["vip"] == true), "两条都应为 vip:true");

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 新增字段属于真实变化:正常计数。
#[test]
fn adding_field_counts_as_change() {
    let path = temp_nodb("add_field");
    let db = Database::create(&path).expect("创建数据库应成功");
    let mut doc = Document::from_json(r#"{"_id":"u1","name":"张三"}"#).unwrap();
    db.insert("users", &mut doc).expect("插入应成功");

    let n = db
        .update(
            "users",
            &filter(r#"{"_id":"u1"}"#),
            &update(r#"{"$set":{"name":"张三","newField":1}}"#),
        )
        .expect("更新应成功");
    assert_eq!(n, 1, "既有字段不变但新增了字段,应计为修改");

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}
