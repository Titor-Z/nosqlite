//! 过滤操作符 `$exists` / `$type` / `$all` / `$size`(v0.6.0 M28)。
//!
//! 重点覆盖**边界与易混淆点**:`$exists` 与 `$eq: null` 的区别、`$type` 对缺失
//! 字段的行为、`$all` 空数组、`$size` 非数组。

use std::path::PathBuf;

use nodblite::query::{Filter, FindOptions};
use nodblite::{Database, Document};
use serde_json::{Map, Value, json};

fn temp_nodb(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "nodblite_m28f_{}_{}_{}.nodb",
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

fn setup(tag: &str, docs: Vec<Value>) -> (PathBuf, Database) {
    let path = temp_nodb(tag);
    let db = Database::create(&path).unwrap();
    let mut list: Vec<Document> = docs
        .into_iter()
        .map(|v| Document::from_json(&v.to_string()).unwrap())
        .collect();
    db.insert_many("items", &mut list).unwrap();
    (path, db)
}

/// 执行 filter,返回命中文档的 `tag` 字段列表(升序,便于断言)。
fn tags(db: &Database, cond: &Value) -> Vec<String> {
    let f = Filter::from_value(cond).unwrap();
    let docs = db
        .find_with_options("items", &f, &FindOptions::default())
        .unwrap();
    let mut out: Vec<String> = docs
        .iter()
        .map(|d| d["tag"].as_str().unwrap_or("?").to_string())
        .collect();
    out.sort();
    out
}

// ==================== $exists ====================

/// `$exists` 区分「字段缺失」与「字段值为 null」。
#[test]
fn exists_vs_null() {
    let (path, db) = setup(
        "exists",
        vec![
            json!({"tag": "缺失"}),
            json!({"tag": "为null", "a": null}),
            json!({"tag": "有值", "a": 1}),
            json!({"tag": "空串", "a": ""}),
            json!({"tag": "false", "a": false}),
        ],
    );

    // 字段缺失 → $exists:false;字段为 null 不算缺失
    assert_eq!(tags(&db, &json!({"a": {"$exists": false}})), vec!["缺失"]);
    // 而 $eq: null 会同时命中「缺失」与「值为 null」—— 与 $exists 语义不同
    assert_eq!(tags(&db, &json!({"a": null})), vec!["为null", "缺失"]);

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// `$exists: true` 命中「字段存在」的全部情况(含 null、false、空串)。
#[test]
fn exists_true_includes_null_and_false() {
    let (path, db) = setup(
        "exists_true",
        vec![
            json!({"tag": "missing"}),
            json!({"tag": "null", "a": null}),
            json!({"tag": "zero", "a": 0}),
            json!({"tag": "false", "a": false}),
            json!({"tag": "empty", "a": ""}),
        ],
    );

    assert_eq!(
        tags(&db, &json!({"a": {"$exists": true}})),
        vec!["empty", "false", "null", "zero"]
    );

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// `$exists: false` 只命中字段缺失的文档。
#[test]
fn exists_false_only_missing() {
    let (path, db) = setup(
        "exists_false",
        vec![
            json!({"tag": "missing"}),
            json!({"tag": "null", "a": null}),
            json!({"tag": "zero", "a": 0}),
        ],
    );

    assert_eq!(
        tags(&db, &json!({"a": {"$exists": false}})),
        vec!["missing"]
    );

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// `$exists` 支持点号路径。
#[test]
fn exists_nested_path() {
    let (path, db) = setup(
        "exists_nested",
        vec![
            json!({"tag": "有", "a": {"b": 1}}),
            json!({"tag": "无", "a": {}}),
            json!({"tag": "a缺失"}),
        ],
    );

    assert_eq!(tags(&db, &json!({"a.b": {"$exists": true}})), vec!["有"]);
    assert_eq!(
        tags(&db, &json!({"a.b": {"$exists": false}})),
        vec!["a缺失", "无"]
    );

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// `$exists` 参数必须是布尔值。
#[test]
fn exists_requires_bool() {
    assert!(Filter::from_value(&json!({"a": {"$exists": 1}})).is_err());
    assert!(Filter::from_value(&json!({"a": {"$exists": "true"}})).is_err());
}

// ==================== $type ====================

/// 各类型的识别。
#[test]
fn type_basic_types() {
    let (path, db) = setup(
        "type",
        vec![
            json!({"tag": "str", "v": "abc"}),
            json!({"tag": "int", "v": 42}),
            json!({"tag": "bool", "v": true}),
            json!({"tag": "null", "v": null}),
            json!({"tag": "arr", "v": [1, 2]}),
            json!({"tag": "obj", "v": {"x": 1}}),
        ],
    );

    assert_eq!(tags(&db, &json!({"v": {"$type": "string"}})), vec!["str"]);
    assert_eq!(tags(&db, &json!({"v": {"$type": "bool"}})), vec!["bool"]);
    assert_eq!(tags(&db, &json!({"v": {"$type": "null"}})), vec!["null"]);
    assert_eq!(tags(&db, &json!({"v": {"$type": "array"}})), vec!["arr"]);
    assert_eq!(tags(&db, &json!({"v": {"$type": "object"}})), vec!["obj"]);

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// `$type: "number"` 接受整数与浮点。
#[test]
fn type_number_alias() {
    let (path, db) = setup(
        "type_num",
        vec![
            json!({"tag": "int", "v": 42}),
            json!({"tag": "float", "v": 1.5}),
            json!({"tag": "str", "v": "42"}),
        ],
    );

    assert_eq!(
        tags(&db, &json!({"v": {"$type": "number"}})),
        vec!["float", "int"]
    );
    assert_eq!(
        tags(&db, &json!({"v": {"$type": ["string", "bool"]}})),
        vec!["str"]
    );

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// `$type` **不匹配字段缺失**。
#[test]
fn type_does_not_match_missing() {
    let (path, db) = setup(
        "type_missing",
        vec![json!({"tag": "missing"}), json!({"tag": "null", "v": null})],
    );

    // 任何类型都不该命中「字段缺失」
    for t in ["string", "null", "number", "bool", "object"] {
        assert!(
            !tags(&db, &json!({"v": {"$type": t}})).contains(&"missing".to_string()),
            "$type: {} 不应匹配缺失字段",
            t
        );
    }

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 非法类型名报错。
#[test]
fn type_invalid_name() {
    assert!(Filter::from_value(&json!({"a": {"$type": "nosuchtype"}})).is_err());
    assert!(Filter::from_value(&json!({"a": {"$type": 42}})).is_err());
}

// ==================== $all ====================

/// `$all` 要求数组**同时包含**全部元素。
#[test]
fn all_requires_every_element() {
    let (path, db) = setup(
        "all",
        vec![
            json!({"tag": "ab", "tags": ["a", "b"]}),
            json!({"tag": "abc", "tags": ["a", "b", "c"]}),
            json!({"tag": "a", "tags": ["a"]}),
            json!({"tag": "scalar", "tags": "a"}),
        ],
    );

    assert_eq!(
        tags(&db, &json!({"tags": {"$all": ["a", "b"]}})),
        vec!["ab", "abc"]
    );
    assert_eq!(
        tags(&db, &json!({"tags": {"$all": ["a"]}})),
        vec!["a", "ab", "abc", "scalar"]
    );

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// `$all: []` 不匹配任何文档(MongoDB 语义)。
#[test]
fn all_empty_matches_nothing() {
    let (path, db) = setup("all_empty", vec![json!({"tag": "x", "tags": ["a"]})]);
    assert!(tags(&db, &json!({"tags": {"$all": []}})).is_empty());
    drop(db);
    let _ = std::fs::remove_file(path);
}

/// `$all` 参数必须是数组。
#[test]
fn all_requires_array() {
    assert!(Filter::from_value(&json!({"a": {"$all": "x"}})).is_err());
}

// ==================== $size ====================

/// `$size` 是精确长度匹配。
#[test]
fn size_exact_length() {
    let (path, db) = setup(
        "size",
        vec![
            json!({"tag": "0", "v": []}),
            json!({"tag": "1", "v": [1]}),
            json!({"tag": "2", "v": [1, 2]}),
            json!({"tag": "scalar", "v": "aa"}),
            json!({"tag": "missing"}),
        ],
    );

    assert_eq!(tags(&db, &json!({"v": {"$size": 0}})), vec!["0"]);
    assert_eq!(tags(&db, &json!({"v": {"$size": 2}})), vec!["2"]);
    // 非数组不匹配(哪怕字符串长度相同)
    assert!(!tags(&db, &json!({"v": {"$size": 3}})).contains(&"scalar".to_string()));

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// `$size` 不接受负数或非整数。
#[test]
fn size_invalid() {
    assert!(Filter::from_value(&json!({"a": {"$size": -1}})).is_err());
    assert!(Filter::from_value(&json!({"a": {"$size": 1.5}})).is_err());
    assert!(Filter::from_value(&json!({"a": {"$size": "2"}})).is_err());
}

// ==================== 组合与索引一致性 ====================

/// 新操作符可与既有操作符组合。
#[test]
fn new_operators_combine() {
    let (path, db) = setup(
        "combine",
        vec![
            json!({"tag": "hit", "v": ["a", "b"], "n": 5}),
            json!({"tag": "wrong_n", "v": ["a", "b"], "n": 50}),
            json!({"tag": "wrong_arr", "v": ["a"], "n": 5}),
        ],
    );

    let cond = json!({"v": {"$all": ["a", "b"], "$size": 2}, "n": {"$lt": 10}});
    assert_eq!(tags(&db, &cond), vec!["hit"]);

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 新操作符在索引路径与全表扫路径下结果一致。
#[test]
fn new_operators_match_planned_path() {
    let docs: Vec<Value> = (0..20)
        .map(|i| json!({"tag": format!("d{}", i), "k": i % 3, "arr": [i, i + 1]}))
        .collect();
    let (path, db) = setup("planned", docs);
    db.create_index("items", "k_1", "k", false, false).unwrap();

    let conds = vec![
        json!({"k": 1, "arr": {"$size": 2}}),
        json!({"k": 2, "arr": {"$all": [5, 6]}}),
        json!({"k": 0, "tag": {"$type": "string"}}),
        json!({"k": 1, "nope": {"$exists": false}}),
    ];
    for cond in conds {
        let f = Filter::from_value(&cond).unwrap();
        let opts = FindOptions::default();
        let want = db.find_with_options("items", &f, &opts).unwrap();
        let got = db.find_planned("items", &f, &cond, &opts).unwrap();
        let key = |d: &Map<String, Value>| d["tag"].as_str().unwrap_or("").to_string();
        let mut a: Vec<String> = want.iter().map(key).collect();
        let mut b: Vec<String> = got.iter().map(key).collect();
        a.sort();
        b.sort();
        assert_eq!(a, b, "条件 {:?} 索引路径与全表扫应一致", cond);
    }

    drop(db);
    let _ = std::fs::remove_file(path);
}
