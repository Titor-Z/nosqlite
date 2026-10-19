//! 数组更新操作符 `$pull` / `$pop`(v0.6.1 M29)。

use std::path::PathBuf;

use nodblite::query::Update;
use nodblite::{Database, Document};
use serde_json::{Map, Value, json};

fn temp_nodb(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "nodblite_m29a_{}_{}_{}.nodb",
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

/// 插入一条文档,应用更新,返回更新后的文档。
fn run(tag: &str, doc: Value, update: Value) -> Map<String, Value> {
    let path = temp_nodb(tag);
    let db = Database::create(&path).unwrap();
    let mut d = Document::from_json(&doc.to_string()).unwrap();
    let id = db.insert("items", &mut d).unwrap();

    let upd = Update::from_value(&update).unwrap();
    db.update_by_id("items", &id, &upd).unwrap();

    let out = db.get_by_id("items", &id).unwrap().unwrap();
    drop(db);
    let _ = std::fs::remove_file(path);
    out
}

// ==================== $pull ====================

/// 标量等值:移除所有相等的元素。
#[test]
fn pull_scalar_equality() {
    let got = run(
        "pull_scalar",
        json!({"tags": ["a", "b", "a", "c"]}),
        json!({"$pull": {"tags": "a"}}),
    );
    assert_eq!(got["tags"], json!(["b", "c"]));
}

/// `$in`:移除所有在列表中的元素。
#[test]
fn pull_with_in() {
    let got = run(
        "pull_in",
        json!({"tags": ["a", "b", "c", "d"]}),
        json!({"$pull": {"tags": {"$in": ["a", "c"]}}}),
    );
    assert_eq!(got["tags"], json!(["b", "d"]));
}

/// 范围条件:移除满足比较的元素。
#[test]
fn pull_with_range() {
    let got = run(
        "pull_range",
        json!({"scores": [1, 5, 10, 15, 20]}),
        json!({"$pull": {"scores": {"$gte": 10}}}),
    );
    assert_eq!(got["scores"], json!([1, 5]));
}

/// 对象子集匹配:只按给定字段判断,不要求对象只有这些字段。
#[test]
fn pull_subdocument_subset() {
    let got = run(
        "pull_subdoc",
        json!({"results": [
            {"score": 8, "item": "A"},
            {"score": 9, "item": "B"},
            {"score": 8, "item": "C"}
        ]}),
        json!({"$pull": {"results": {"score": 8}}}),
    );
    assert_eq!(got["results"], json!([{"score": 9, "item": "B"}]));
}

/// 字段缺失 → 无操作;不报错。
#[test]
fn pull_missing_field_is_noop() {
    let got = run(
        "pull_missing",
        json!({"x": 1}),
        json!({"$pull": {"tags": "a"}}),
    );
    assert_eq!(got["x"], json!(1));
    assert!(!got.contains_key("tags"));
}

/// 字段不是数组 → 报错。
#[test]
fn pull_non_array_errors() {
    let err = Update::from_value(&json!({"$pull": {"a": "x"}}));
    assert!(err.is_ok(), "解析应通过,错误发生在应用阶段");

    let path = temp_nodb("pull_nonarr");
    let db = Database::create(&path).unwrap();
    let mut d = Document::from_json(r#"{"a": "notarray"}"#).unwrap();
    let id = db.insert("items", &mut d).unwrap();
    let upd = Update::from_value(&json!({"$pull": {"a": "x"}})).unwrap();
    assert!(db.update_by_id("items", &id, &upd).is_err(), "非数组应报错");

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 全删后得到空数组。
#[test]
fn pull_removes_all() {
    let got = run(
        "pull_all",
        json!({"tags": [1, 2, 3]}),
        json!({"$pull": {"tags": {"$gte": 0}}}),
    );
    assert_eq!(got["tags"], json!([]));
}

/// `_id` 不允许被 `$pull`。
#[test]
fn pull_rejects_id() {
    assert!(Update::from_value(&json!({"$pull": {"_id": "x"}})).is_err());
}

// ==================== $pop ====================

/// `1` 移除最后一个。
#[test]
fn pop_last() {
    let got = run(
        "pop_last",
        json!({"a": [1, 2, 3]}),
        json!({"$pop": {"a": 1}}),
    );
    assert_eq!(got["a"], json!([1, 2]));
}

/// `-1` 移除第一个。
#[test]
fn pop_first() {
    let got = run(
        "pop_first",
        json!({"a": [1, 2, 3]}),
        json!({"$pop": {"a": -1}}),
    );
    assert_eq!(got["a"], json!([2, 3]));
}

/// 空数组无操作。
#[test]
fn pop_empty_array_is_noop() {
    let got = run("pop_empty", json!({"a": []}), json!({"$pop": {"a": 1}}));
    assert_eq!(got["a"], json!([]));
}

/// 字段缺失无操作。
#[test]
fn pop_missing_field_is_noop() {
    let got = run("pop_missing", json!({"x": 1}), json!({"$pop": {"a": 1}}));
    assert!(!got.contains_key("a"));
}

/// 只接受 1 与 -1。
#[test]
fn pop_invalid_value() {
    assert!(Update::from_value(&json!({"$pop": {"a": 0}})).is_err());
    assert!(Update::from_value(&json!({"$pop": {"a": 2}})).is_err());
    assert!(Update::from_value(&json!({"$pop": {"a": "1"}})).is_err());
}

/// 单元素数组 pop 后为空。
#[test]
fn pop_single_element() {
    let got = run("pop_single", json!({"a": [1]}), json!({"$pop": {"a": -1}}));
    assert_eq!(got["a"], json!([]));
}

// ==================== 组合与持久化 ====================

/// `$pull` 与 `$push` 可在同一更新中出现(按固定执行顺序)。
#[test]
fn pull_and_push_together() {
    let got = run(
        "pull_push",
        json!({"a": [1, 2, 3]}),
        json!({"$pull": {"a": 2}, "$push": {"a": 4}}),
    );
    assert_eq!(got["a"], json!([1, 3, 4]));
}

/// 无实际变化时不虚增 modified 条数。
#[test]
fn pull_no_change_not_counted() {
    let path = temp_nodb("pull_nocount");
    let db = Database::create(&path).unwrap();
    let mut d = Document::from_json(r#"{"a": [1, 2]}"#).unwrap();
    db.insert("items", &mut d).unwrap();

    let upd = Update::from_value(&json!({"$pull": {"a": 999}})).unwrap();
    let n = db
        .update("items", &nodblite::query::Filter::match_all(), &upd)
        .unwrap();
    assert_eq!(n, 0, "无元素被移除时不应计入 modified");

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 更新结果可持久化(重开库后仍生效)。
#[test]
fn pull_persists() {
    let path = temp_nodb("pull_persist");
    let db = Database::create(&path).unwrap();
    let mut d = Document::from_json(r#"{"a": [1, 2, 3]}"#).unwrap();
    let id = db.insert("items", &mut d).unwrap();
    let upd = Update::from_value(&json!({"$pull": {"a": 2}})).unwrap();
    db.update_by_id("items", &id, &upd).unwrap();
    drop(db);

    let db = Database::open(&path).unwrap();
    let doc = db.get_by_id("items", &id).unwrap().unwrap();
    assert_eq!(doc["a"], json!([1, 3]));

    drop(db);
    let _ = std::fs::remove_file(path);
}
