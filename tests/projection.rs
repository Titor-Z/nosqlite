//! 字段投影(0.2.0):`Projection` 解析与应用。
//!
//! 语义对齐 MongoDB:
//! - 只出现 `1`/`true` → 包含模式;只出现 `0`/`false` → 排除模式;
//! - 混用报错(仅 `_id` 可例外);
//! - `_id` 默认包含,除非显式 `"_id": 0`;
//! - 支持点号路径。

use std::path::PathBuf;

use nosqlite::query::{Filter, FindOptions, Projection, SortSpec};
use nosqlite::{Database, Document};
use serde_json::{Map, Value, json};

/// 生成本次测试独占的临时 `.nodb` 路径。
fn temp_nodb(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "nosqlite_proj_{}_{}_{}.nodb",
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

/// 构造投影。
fn p(value: Value) -> Projection {
    Projection::from_value(&value).expect("投影应解析成功")
}

/// 把 JSON 转成文档 Map。
fn d(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

/// 包含模式:只保留列入的字段,`_id` 默认保留。
#[test]
fn include_mode_keeps_listed_and_id() {
    let proj = p(json!({"name": 1}));
    let out = proj.apply(&d(json!({"name": "a", "age": 1, "_id": "x"})));
    assert_eq!(out, d(json!({"name": "a", "_id": "x"})));
}

/// 包含模式下显式排除 `_id`。
#[test]
fn include_mode_can_drop_id() {
    let proj = p(json!({"name": 1, "_id": 0}));
    let out = proj.apply(&d(json!({"name": "a", "age": 1, "_id": "x"})));
    assert_eq!(out, d(json!({"name": "a"})));
}

/// 排除模式:保留未列入的字段。
#[test]
fn exclude_mode_removes_listed() {
    let proj = p(json!({"age": 0}));
    let out = proj.apply(&d(json!({"name": "a", "age": 1, "_id": "x"})));
    assert_eq!(out, d(json!({"name": "a", "_id": "x"})));
}

/// 排除模式下显式排除 `_id`。
#[test]
fn exclude_mode_can_drop_id() {
    let proj = p(json!({"age": 0, "_id": 0}));
    let out = proj.apply(&d(json!({"name": "a", "age": 1, "_id": "x"})));
    assert_eq!(out, d(json!({"name": "a"})));
}

/// 点号路径:包含模式保留嵌套结构。
#[test]
fn include_mode_nested_path() {
    let proj = p(json!({"addr.city": 1}));
    let out = proj.apply(&d(json!({"addr": {"city": "上海", "zip": "1"}, "_id": "x"})));
    assert_eq!(out, d(json!({"addr": {"city": "上海"}, "_id": "x"})));
}

/// 点号路径:排除模式只删嵌套子字段。
#[test]
fn exclude_mode_nested_path() {
    let proj = p(json!({"addr.zip": 0}));
    let out = proj.apply(&d(json!({"addr": {"city": "上海", "zip": "1"}, "_id": "x"})));
    assert_eq!(out, d(json!({"addr": {"city": "上海"}, "_id": "x"})));
}

/// 包含与排除混用应报错。
#[test]
fn mixed_modes_rejected() {
    let err = Projection::from_value(&json!({"name": 1, "age": 0}))
        .expect_err("混用应报错");
    assert!(err.to_string().contains("不能混用"), "实际: {}", err);
}

/// `_id` 可以与任一种模式反向出现(不算混用)。
#[test]
fn id_is_exempt_from_mixed_check() {
    assert!(Projection::from_value(&json!({"name": 1, "_id": 0})).is_ok());
    assert!(Projection::from_value(&json!({"age": 0, "_id": 1})).is_ok());
}

/// 非 0/1 的值应报错。
#[test]
fn invalid_value_rejected() {
    for bad in [json!({"name": 2}), json!({"name": "yes"}), json!({"name": null})] {
        assert!(
            Projection::from_value(&bad).is_err(),
            "应报错: {}",
            bad
        );
    }
}

/// 空路径段应报错。
#[test]
fn empty_path_rejected() {
    for bad in [json!({"": 1}), json!({"a..b": 1}), json!({".a": 1})] {
        assert!(Projection::from_value(&bad).is_err(), "应报错: {}", bad);
    }
}

/// 顶层非对象应报错。
#[test]
fn non_object_rejected() {
    assert!(Projection::from_value(&json!([1, 2])).is_err());
}

/// 端到端:`find_with_options` 应用投影。
#[test]
fn find_with_projection() {
    let path = temp_nodb("find");
    let db = Database::create(&path).unwrap();
    let mut doc = Document::from_json(r#"{"name": "张三", "age": 30, "secret": "x"}"#).unwrap();
    db.insert("users", &mut doc).unwrap();

    let options = FindOptions {
        sort: None,
        projection: Some(p(json!({"name": 1, "age": 1}))),
        skip: 0,
        limit: None,
    };
    let docs = db
        .find_with_options("users", &Filter::match_all(), &options)
        .unwrap();
    assert_eq!(docs.len(), 1);
    assert!(!docs[0].contains_key("secret"), "未列入的字段应被裁掉");
    assert_eq!(docs[0]["name"], json!("张三"));
    assert!(docs[0].contains_key("_id"), "_id 默认保留");

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 端到端:`find_page_total` 应用投影(排序/分页之后再裁剪)。
#[test]
fn find_page_total_with_projection_and_sort() {
    let path = temp_nodb("page");
    let db = Database::create(&path).unwrap();
    let mut docs: Vec<Document> = (0..10)
        .map(|i| Document::from_json(&format!(r#"{{"n": {}, "extra": "e"}}"#, i)).unwrap())
        .collect();
    db.insert_many("items", &mut docs).unwrap();

    let sort = SortSpec::from_value(&json!({"n": -1})).unwrap();
    let proj = p(json!({"n": 1}));
    let (total, page) = db
        .find_page_total("items", &Filter::match_all(), Some(&sort), 0, Some(3), Some(&proj))
        .unwrap();

    assert_eq!(total, 10, "total 应是匹配总数而非页内条数");
    assert_eq!(page.len(), 3);
    let ns: Vec<i64> = page.iter().map(|d| d["n"].as_i64().unwrap()).collect();
    assert_eq!(ns, vec![9, 8, 7], "排序应生效");
    assert!(page.iter().all(|d| !d.contains_key("extra")));

    drop(db);
    let _ = std::fs::remove_file(path);
}
