//! 投影位置运算符 `{"arr.$": 1}`(v0.6.1 M29)。
//!
//! 语义:只保留 **filter 命中的那个元素**,结果为单元素数组。

use std::path::PathBuf;

use nodblite::query::{Filter, FindOptions, Projection};
use nodblite::{Database, Document};
use serde_json::{Map, Value, json};

fn temp_nodb(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "nodblite_m29p_{}_{}_{}.nodb",
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

/// 用给定 filter + projection 查询单条文档。
fn query(tag: &str, doc: Value, filter: Value, projection: Value) -> Map<String, Value> {
    let path = temp_nodb(tag);
    let db = Database::create(&path).unwrap();
    let mut d = Document::from_json(&doc.to_string()).unwrap();
    db.insert("items", &mut d).unwrap();

    let opts = FindOptions {
        projection: Some(Projection::from_value(&projection).unwrap()),
        ..Default::default()
    };
    let mut docs = db
        .find_with_options("items", &Filter::from_value(&filter).unwrap(), &opts)
        .unwrap();
    let out = docs.pop().unwrap();
    drop(db);
    let _ = std::fs::remove_file(path);
    out
}

/// 位置运算符只保留 filter 命中的元素。
#[test]
fn position_keeps_matching_element() {
    let got = query(
        "pos",
        json!({"items": [
            {"name": "a", "qty": 1},
            {"name": "b", "qty": 10},
            {"name": "c", "qty": 20}
        ]}),
        json!({"items.qty": {"$gte": 10}}),
        json!({"items.$": 1}),
    );
    assert_eq!(got["items"], json!([{"name": "b", "qty": 10}]));
}

/// 多个元素命中时只取**第一个**。
#[test]
fn position_takes_first_match() {
    let got = query(
        "pos_first",
        json!({"items": [
            {"qty": 10, "n": 1},
            {"qty": 20, "n": 2}
        ]}),
        json!({"items.qty": {"$gte": 10}}),
        json!({"items.$": 1}),
    );
    assert_eq!(got["items"], json!([{"qty": 10, "n": 1}]));
}

/// 数组为空时返回空数组(不报错)。
#[test]
fn position_empty_array_gives_empty() {
    let got = query(
        "pos_none",
        json!({"items": [], "other": 5}),
        json!({"other": 5}),
        json!({"items.$": 1}),
    );
    assert_eq!(got["items"], json!([]));
}

/// 与其他投影字段共存。
#[test]
fn position_with_other_fields() {
    let got = query(
        "pos_mixed",
        json!({"items": [{"qty": 5}, {"qty": 15}], "title": "t", "secret": "s"}),
        json!({"items.qty": {"$gt": 10}}),
        json!({"items.$": 1, "title": 1}),
    );
    assert_eq!(got["items"], json!([{"qty": 15}]));
    assert_eq!(got["title"], json!("t"));
    assert!(!got.contains_key("secret"));
}

/// `_id` 默认保留。
#[test]
fn position_keeps_id() {
    let got = query(
        "pos_id",
        json!({"items": [{"qty": 7}]}),
        json!({"items.qty": 7}),
        json!({"items.$": 1}),
    );
    assert!(got.contains_key("_id"));
}

/// 非数组字段按原样返回。
#[test]
fn position_on_non_array() {
    let got = query(
        "pos_nonarr",
        json!({"items": "scalar"}),
        json!({"items": "scalar"}),
        json!({"items.$": 1}),
    );
    assert_eq!(got["items"], json!("scalar"));
}

/// 位置运算符在索引规划路径下同样生效。
#[test]
fn position_works_with_planned_path() {
    let path = temp_nodb("pos_planned");
    let db = Database::create(&path).unwrap();
    let mut d = Document::from_json(r#"{"k": 1, "items": [{"qty": 5}, {"qty": 15}]}"#).unwrap();
    db.insert("items", &mut d).unwrap();
    db.create_index("items", "k_1", "k", false, false).unwrap();

    let filter_json = json!({"k": 1, "items.qty": {"$gt": 10}});
    let opts = FindOptions {
        projection: Some(Projection::from_value(&json!({"items.$": 1})).unwrap()),
        ..Default::default()
    };
    let docs = db
        .find_planned(
            "items",
            &Filter::from_value(&filter_json).unwrap(),
            &filter_json,
            &opts,
        )
        .unwrap();
    assert_eq!(docs[0]["items"], json!([{"qty": 15}]));

    drop(db);
    let _ = std::fs::remove_file(path);
}
