//! 投影运算符 `$slice` / `$elemMatch`(v0.6.0 M28)。

use std::path::PathBuf;

use nosqlite::query::{Filter, FindOptions};
use nosqlite::{Database, Document};
use serde_json::{Map, Value, json};

fn temp_nodb(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "nosqlite_m28p_{}_{}_{}.nodb",
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

/// 插入单条文档,按投影返回结果。
fn project(tag: &str, doc: Value, projection: Value) -> Map<String, Value> {
    let path = temp_nodb(tag);
    let db = Database::create(&path).unwrap();
    let mut d = Document::from_json(&doc.to_string()).unwrap();
    db.insert("items", &mut d).unwrap();

    let opts = FindOptions {
        projection: Some(nosqlite::query::Projection::from_value(&projection).unwrap()),
        ..Default::default()
    };
    let mut docs = db
        .find_with_options("items", &Filter::match_all(), &opts)
        .unwrap();
    let out = docs.pop().unwrap();
    drop(db);
    let _ = std::fs::remove_file(path);
    out
}

// ==================== $slice ====================

/// 正数取前 n 个。
#[test]
fn slice_positive_prefix() {
    let got = project(
        "slice_pos",
        json!({"a": [1, 2, 3, 4, 5]}),
        json!({"a": {"$slice": 2}}),
    );
    assert_eq!(got["a"], json!([1, 2]));
}

/// 负数从尾部取 |n| 个。
#[test]
fn slice_negative_suffix() {
    let got = project(
        "slice_neg",
        json!({"a": [1, 2, 3, 4, 5]}),
        json!({"a": {"$slice": -2}}),
    );
    assert_eq!(got["a"], json!([4, 5]));
}

/// `[skip, limit]` 取中间一段。
#[test]
fn slice_skip_limit() {
    let got = project(
        "slice_range",
        json!({"a": [1, 2, 3, 4, 5]}),
        json!({"a": {"$slice": [1, 2]}}),
    );
    assert_eq!(got["a"], json!([2, 3]));
}

/// 越界时安全截断,不 panic。
#[test]
fn slice_out_of_bounds() {
    let got = project(
        "slice_oob",
        json!({"a": [1, 2]}),
        json!({"a": {"$slice": 10}}),
    );
    assert_eq!(got["a"], json!([1, 2]));

    let got = project(
        "slice_oob2",
        json!({"a": [1, 2]}),
        json!({"a": {"$slice": [5, 2]}}),
    );
    assert_eq!(got["a"], json!([]));
}

/// `_id` 默认保留。
#[test]
fn slice_keeps_id() {
    let got = project(
        "slice_id",
        json!({"a": [1, 2, 3]}),
        json!({"a": {"$slice": 1}}),
    );
    assert!(got.contains_key("_id"), "投影运算符不应丢掉 _id");
    assert!(!got.contains_key("b"), "未列入的字段不应返回");
}

/// 非数组字段按原样返回(不报错)。
#[test]
fn slice_on_non_array() {
    let got = project(
        "slice_nonarr",
        json!({"a": "hello"}),
        json!({"a": {"$slice": 2}}),
    );
    assert_eq!(got["a"], json!("hello"));
}

// ==================== $elemMatch 投影 ====================

/// 只保留第一个匹配的元素,结果为单元素数组。
#[test]
fn elem_match_projection_keeps_first() {
    let got = project(
        "elem_proj",
        json!({"items": [
            {"name": "x", "qty": 1},
            {"name": "y", "qty": 10},
            {"name": "z", "qty": 20}
        ]}),
        json!({"items": {"$elemMatch": {"qty": {"$gte": 10}}}}),
    );
    assert_eq!(got["items"], json!([{"name": "y", "qty": 10}]));
}

/// 无匹配元素时返回空数组。
#[test]
fn elem_match_projection_no_match() {
    let got = project(
        "elem_proj_none",
        json!({"items": [{"qty": 1}]}),
        json!({"items": {"$elemMatch": {"qty": {"$gt": 100}}}}),
    );
    assert_eq!(got["items"], json!([]));
}

/// 支持多字段条件。
#[test]
fn elem_match_projection_multiple_fields() {
    let got = project(
        "elem_proj_multi",
        json!({"items": [
            {"name": "x", "qty": 10},
            {"name": "y", "qty": 10}
        ]}),
        json!({"items": {"$elemMatch": {"name": "y", "qty": {"$gte": 5}}}}),
    );
    assert_eq!(got["items"], json!([{"name": "y", "qty": 10}]));
}

// ==================== 错误处理 ====================

/// 非法参数被拒绝。
#[test]
fn projection_operator_errors() {
    use nosqlite::query::Projection;
    // $slice 的 limit 必须为正
    assert!(Projection::from_value(&json!({"a": {"$slice": [0, 0]}})).is_err());
    // $slice 需要整数或数组
    assert!(Projection::from_value(&json!({"a": {"$slice": "x"}})).is_err());
    // 未知运算符
    assert!(Projection::from_value(&json!({"a": {"$nope": 1}})).is_err());
    // $elemMatch 需要对象
    assert!(Projection::from_value(&json!({"a": {"$elemMatch": 1}})).is_err());
}

/// 投影运算符与普通字段混用仍按包含模式工作。
#[test]
fn projection_operator_with_plain_fields() {
    let got = project(
        "mixed_proj",
        json!({"a": [1, 2, 3], "b": 42, "c": "drop"}),
        json!({"a": {"$slice": 1}, "b": 1}),
    );
    assert_eq!(got["a"], json!([1]));
    assert_eq!(got["b"], json!(42));
    assert!(!got.contains_key("c"));
}
