//! 更新补充(v0.7.0):`$bit` 与数组位置运算符 `$[]` / `$[id]`(配合 `arrayFilters`)。

use serde_json::{Map, Value, json};

use nodblite::query::Update;

fn u(value: Value) -> Update {
    Update::from_value(&value).expect("update 应解析成功")
}

fn u_with(value: Value, filters: Value) -> Update {
    let filters = filters.as_array().cloned().unwrap_or_default();
    Update::from_value_with_filters(&value, &filters).expect("update 应解析成功")
}

fn d(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

// ---------------------------- $bit ----------------------------

#[test]
fn bit_and_or_xor() {
    let mut doc = d(json!({"n": 0b1100}));
    u(json!({"$bit": {"n": {"and": 0b1010}}})).apply(&mut doc).unwrap();
    assert_eq!(doc["n"], json!(0b1000));

    let mut doc = d(json!({"n": 0b1100}));
    u(json!({"$bit": {"n": {"or": 0b0011}}})).apply(&mut doc).unwrap();
    assert_eq!(doc["n"], json!(0b1111));

    let mut doc = d(json!({"n": 0b1100}));
    u(json!({"$bit": {"n": {"xor": 0b1010}}})).apply(&mut doc).unwrap();
    assert_eq!(doc["n"], json!(0b0110));
}

/// 字段缺失按 0 参与运算(与 MongoDB 一致)。
#[test]
fn bit_missing_field_is_zero() {
    let mut doc = d(json!({}));
    u(json!({"$bit": {"n": {"or": 5}}})).apply(&mut doc).unwrap();
    assert_eq!(doc["n"], json!(5));
}

#[test]
fn bit_rejects_non_integer() {
    let mut doc = d(json!({"n": 1.5}));
    assert!(u(json!({"$bit": {"n": {"and": 1}}})).apply(&mut doc).is_err());
    // 操作数必须是整数
    assert!(Update::from_value(&json!({"$bit": {"n": {"and": 1.5}}})).is_err());
    // 未知操作
    assert!(Update::from_value(&json!({"$bit": {"n": {"nand": 1}}})).is_err());
}

// ---------------------------- $[] ----------------------------

#[test]
fn all_positional_sets_every_element() {
    let mut doc = d(json!({"items": [{"qty": 1}, {"qty": 2}]}));
    u(json!({"$set": {"items.$[].qty": 0}})).apply(&mut doc).unwrap();
    assert_eq!(doc["items"], json!([{"qty": 0}, {"qty": 0}]));
}

#[test]
fn all_positional_on_scalar_array() {
    let mut doc = d(json!({"tags": ["a", "b"]}));
    u(json!({"$set": {"tags.$[]": "x"}})).apply(&mut doc).unwrap();
    assert_eq!(doc["tags"], json!(["x", "x"]));
}

#[test]
fn all_positional_inc() {
    let mut doc = d(json!({"nums": [1, 2, 3]}));
    u(json!({"$inc": {"nums.$[]": 10}})).apply(&mut doc).unwrap();
    assert_eq!(doc["nums"], json!([11, 12, 13]));
}

/// 目标不是数组时静默无操作(MongoDB 行为)。
#[test]
fn all_positional_on_non_array_is_noop() {
    let mut doc = d(json!({"items": "not-array"}));
    u(json!({"$set": {"items.$[].qty": 0}})).apply(&mut doc).unwrap();
    assert_eq!(doc["items"], json!("not-array"));
}

/// 字段缺失同样无操作。
#[test]
fn all_positional_missing_field_is_noop() {
    let mut doc = d(json!({}));
    u(json!({"$set": {"items.$[].qty": 0}})).apply(&mut doc).unwrap();
    assert!(doc.get("items").is_none());
}

// ---------------------------- $[id] + arrayFilters ----------------------------

#[test]
fn filtered_positional_only_matching() {
    let mut doc = d(json!({"items": [{"qty": 5}, {"qty": 15}, {"qty": 25}]}));
    u_with(
        json!({"$set": {"items.$[e].qty": 0}}),
        json!([{"e.qty": {"$gte": 15}}]),
    )
    .apply(&mut doc)
    .unwrap();
    assert_eq!(doc["items"], json!([{"qty": 5}, {"qty": 0}, {"qty": 0}]));
}

#[test]
fn filtered_positional_on_scalars() {
    let mut doc = d(json!({"nums": [1, 5, 10]}));
    u_with(json!({"$inc": {"nums.$[v]": 100}}), json!([{"v": {"$gt": 4}}]))
        .apply(&mut doc)
        .unwrap();
    assert_eq!(doc["nums"], json!([1, 105, 110]));
}

/// 多个占位符可串联(`a.$[].b.$[x]`)。
#[test]
fn chained_placeholders() {
    let mut doc = d(json!({
        "groups": [
            {"members": [{"age": 10}, {"age": 30}]},
            {"members": [{"age": 40}]}
        ]
    }));
    u_with(
        json!({"$set": {"groups.$[].members.$[m].age": 0}}),
        json!([{"m.age": {"$gte": 30}}]),
    )
    .apply(&mut doc)
    .unwrap();
    assert_eq!(
        doc["groups"],
        json!([
            {"members": [{"age": 10}, {"age": 0}]},
            {"members": [{"age": 0}]}
        ])
    );
}

/// 引用了未在 `arrayFilters` 定义的标识符 → 报错,不静默忽略。
#[test]
fn undefined_identifier_errors() {
    let mut doc = d(json!({"items": [{"qty": 1}]}));
    let update = u(json!({"$set": {"items.$[missing].qty": 0}}));
    let err = update.apply(&mut doc).unwrap_err();
    assert!(err.to_string().contains("arrayFilters"), "实际: {}", err);
}

/// `arrayFilters` 自身格式非法 → 解析期报错。
#[test]
fn malformed_array_filters_rejected() {
    assert!(Update::from_value_with_filters(
        &json!({"$set": {"a.$[e]": 1}}),
        &[json!("not-an-object")],
    )
    .is_err());
}

/// `$unset` 也支持位置运算符。
#[test]
fn unset_with_positional() {
    let mut doc = d(json!({"items": [{"qty": 1, "tmp": 1}, {"qty": 2, "tmp": 2}]}));
    u(json!({"$unset": {"items.$[].tmp": ""}})).apply(&mut doc).unwrap();
    assert_eq!(doc["items"], json!([{"qty": 1}, {"qty": 2}]));
}

/// `$push` 也能定位到元素内部的数组。
#[test]
fn push_with_positional() {
    let mut doc = d(json!({"items": [{"tags": ["a"]}, {"tags": ["b"]}]}));
    u(json!({"$push": {"items.$[].tags": "z"}})).apply(&mut doc).unwrap();
    assert_eq!(doc["items"], json!([{"tags": ["a", "z"]}, {"tags": ["b", "z"]}]));
}
