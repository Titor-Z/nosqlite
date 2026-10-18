//! 复合值(array/object)比较(0.2.0)。
//!
//! 0.1.x 里 array/object 一律返回 `Equal`,导致数组无法排序、`$eq` 对它们不可靠。
//! 现在按 BSON 风格子集递归比较:
//! - 数组逐元素比较,公共部分相等时短者在前;
//! - 对象按键名有序逐对比较(同键比值,键数少者在前)。

use std::path::PathBuf;

use nosqlite::query::{Filter, SortSpec, compare_values};
use nosqlite::{Database, Document};
use serde_json::{Value, json};

/// 生成本次测试独占的临时 `.nodb` 路径。
fn temp_nodb(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "nosqlite_cmp_{}_{}_{}.nodb",
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

/// 便捷比较两个 JSON 值。
fn cmp(a: Value, b: Value) -> std::cmp::Ordering {
    compare_values(Some(&a), Some(&b))
}

/// 数组:逐元素比较,首个不同元素决定大小。
#[test]
fn array_elementwise() {
    use std::cmp::Ordering::*;
    assert_eq!(cmp(json!([1, 2]), json!([1, 2])), Equal);
    assert_eq!(cmp(json!([1, 2]), json!([1, 3])), Less);
    assert_eq!(cmp(json!([1, 3]), json!([1, 2])), Greater);
    assert_eq!(cmp(json!([2]), json!([1, 9, 9])), Greater, "首个元素即分胜负");
}

/// 数组:公共部分相等时,短者在前。
#[test]
fn array_prefix_shorter_first() {
    use std::cmp::Ordering::*;
    assert_eq!(cmp(json!([1, 2]), json!([1, 2, 3])), Less);
    assert_eq!(cmp(json!([1, 2, 3]), json!([1, 2])), Greater);
    assert_eq!(cmp(json!([]), json!([0])), Less);
}

/// 对象:按键名有序逐对比较。
#[test]
fn object_by_keys() {
    use std::cmp::Ordering::*;
    assert_eq!(cmp(json!({"a": 1}), json!({"a": 1})), Equal);
    assert_eq!(cmp(json!({"a": 1}), json!({"a": 2})), Less);
    assert_eq!(cmp(json!({"a": 2}), json!({"a": 1})), Greater);
    // 键名不同:按 key 排序后比较键名
    assert_eq!(cmp(json!({"a": 1}), json!({"b": 1})), Less);
}

/// 对象:键数不同,键数少者在前(公共键值相同时)。
#[test]
fn object_fewer_keys_first() {
    assert_eq!(
        cmp(json!({"a": 1}), json!({"a": 1, "b": 2})),
        std::cmp::Ordering::Less
    );
}

/// 嵌套复合值递归比较。
#[test]
fn nested_composites() {
    use std::cmp::Ordering::*;
    assert_eq!(cmp(json!({"a": [1, {"b": 2}]}), json!({"a": [1, {"b": 3}]})), Less);
    assert_eq!(cmp(json!({"a": [1, {"b": 3}]}), json!({"a": [1, {"b": 2}]})), Greater);
}

/// 类型序不变:array < object。
#[test]
fn type_order_still_holds() {
    assert_eq!(cmp(json!([1]), json!({"a": 1})), std::cmp::Ordering::Less);
    assert_eq!(
        cmp(json!("z"), json!([1])),
        std::cmp::Ordering::Less,
        "string < array"
    );
}

/// 数组可排序(0.1.x 下会全部判等、顺序不确定)。
#[test]
fn arrays_are_sortable() {
    let spec = SortSpec::from_value(&json!({"v": 1})).unwrap();
    let mut docs: Vec<_> = vec![json!({"v": [2]}), json!({"v": [1, 5]}), json!({"v": [1]})]
        .into_iter()
        .map(|v| v.as_object().unwrap().clone())
        .collect();
    spec.sort(&mut docs);
    let vs: Vec<Value> = docs.iter().map(|d| d["v"].clone()).collect();
    assert_eq!(vs, vec![json!([1]), json!([1, 5]), json!([2])]);
}

/// 对象可排序。
#[test]
fn objects_are_sortable() {
    let spec = SortSpec::from_value(&json!({"v": 1})).unwrap();
    let mut docs: Vec<_> = vec![json!({"v": {"k": 2}}), json!({"v": {"k": 1}})]
        .into_iter()
        .map(|v| v.as_object().unwrap().clone())
        .collect();
    spec.sort(&mut docs);
    let vs: Vec<Value> = docs.iter().map(|d| d["v"].clone()).collect();
    assert_eq!(vs, vec![json!({"k": 1}), json!({"k": 2})]);
}

/// `$eq` 对数组做深度相等匹配(0.1.x 下任何数组都「相等」)。
#[test]
fn eq_on_arrays_is_exact() {
    let filter = Filter::from_value(&json!({"tags": ["a", "b"]})).unwrap();
    let matches = |doc: Value| filter.matches(doc.as_object().unwrap());

    assert!(matches(json!({"tags": ["a", "b"]})));
    assert!(!matches(json!({"tags": ["a", "c"]})));
    assert!(!matches(json!({"tags": ["a"]})), "长度不同不应相等");
    assert!(!matches(json!({"tags": ["a", "b", "c"]})));
}

/// `$eq` 对对象做深度相等匹配。
#[test]
fn eq_on_objects_is_exact() {
    let filter = Filter::from_value(&json!({"meta": {"k": 1}})).unwrap();
    let matches = |doc: Value| filter.matches(doc.as_object().unwrap());

    assert!(matches(json!({"meta": {"k": 1}})));
    assert!(!matches(json!({"meta": {"k": 2}})));
    assert!(!matches(json!({"meta": {"k": 1, "x": 1}})));
}

/// `$in` 里的复合元素走同一套比较。
#[test]
fn in_with_composite_elements() {
    let filter = Filter::from_value(&json!({"v": {"$in": [[1, 2], {"a": 1}]}})).unwrap();
    let matches = |doc: Value| filter.matches(doc.as_object().unwrap());

    assert!(matches(json!({"v": [1, 2]})));
    assert!(matches(json!({"v": {"a": 1}})));
    assert!(!matches(json!({"v": [1, 3]})));
    assert!(!matches(json!({"v": {"a": 2}})));
}

/// 端到端:按数组字段排序并由 `find_page_total` 取页。
#[test]
fn sort_by_array_field_end_to_end() {
    let path = temp_nodb("e2e");
    let db = Database::create(&path).unwrap();
    let mut docs: Vec<Document> = vec![
        r#"{"v": [3]}"#,
        r#"{"v": [1, 9]}"#,
        r#"{"v": [1]}"#,
        r#"{"v": [2]}"#,
    ]
    .into_iter()
    .map(|s| Document::from_json(s).unwrap())
    .collect();
    db.insert_many("items", &mut docs).unwrap();

    let spec = SortSpec::from_value(&json!({"v": 1})).unwrap();
    let (total, page) = db
        .find_page_total("items", &Filter::match_all(), Some(&spec), 0, None, None)
        .unwrap();
    assert_eq!(total, 4);
    let vs: Vec<Value> = page.iter().map(|d| d["v"].clone()).collect();
    assert_eq!(vs, vec![json!([1]), json!([1, 9]), json!([2]), json!([3])]);

    drop(db);
    let _ = std::fs::remove_file(path);
}
