//! 查询模块测试(M3):Filter 解析与匹配、SortSpec 解析与排序。
//!
//! 覆盖操作符子集、点号路径、逻辑组合、类型序比较与多字段排序。

use serde_json::json;

use nodblite::query::{Filter, SortError, SortSpec};

/// 辅助:从 JSON 字符串解析 filter。
fn filter(json: &str) -> Filter {
    let v: serde_json::Value = serde_json::from_str(json).expect("测试 JSON 应合法");
    Filter::from_value(&v).expect("filter 解析应成功")
}

/// 辅助:判断文档是否匹配。
fn matches(json: &str, filter_json: &str) -> bool {
    let doc: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(json).expect("测试文档应合法");
    filter(filter_json).matches(&doc)
}

// ──────────────── $eq / 隐式相等 ────────────────

/// 普通值视为隐式 $eq。
#[test]
fn implicit_eq() {
    assert!(matches(r#"{"name":"张三","age":30}"#, r#"{"name":"张三"}"#));
    assert!(!matches(
        r#"{"name":"张三","age":30}"#,
        r#"{"name":"李四"}"#
    ));
}

/// 数字 int/float 可互比相等。
#[test]
fn eq_number_cross_type() {
    assert!(matches(r#"{"age":30}"#, r#"{"age":30.0}"#));
}

/// 缺失字段不匹配 $eq。
#[test]
fn eq_missing_field_fails() {
    assert!(!matches(r#"{"name":"张三"}"#, r#"{"age":30}"#));
}

// ──────────────── $ne ────────────────

/// $ne:值不等则匹配。
#[test]
fn ne_not_equal() {
    assert!(matches(r#"{"age":25}"#, r#"{"age":{"$ne":30}}"#));
    assert!(!matches(r#"{"age":30}"#, r#"{"age":{"$ne":30}}"#));
}

/// $ne:字段缺失也匹配(MongoDB 语义)。
#[test]
fn ne_missing_matches() {
    assert!(matches(r#"{"name":"张三"}"#, r#"{"age":{"$ne":30}}"#));
}

// ──────────────── $gt / $gte / $lt / $lte ────────────────

/// $gt / $gte / $lt / $lte 数值比较。
#[test]
fn range_operators() {
    let doc = r#"{"age":30}"#;
    assert!(matches(doc, r#"{"age":{"$gt":18}}"#));
    assert!(!matches(doc, r#"{"age":{"$gt":30}}"#));
    assert!(matches(doc, r#"{"age":{"$gte":30}}"#));
    assert!(matches(doc, r#"{"age":{"$lt":40}}"#));
    assert!(!matches(doc, r#"{"age":{"$lt":30}}"#));
    assert!(matches(doc, r#"{"age":{"$lte":30}}"#));
}

/// 字符串比较按字典序。
#[test]
fn range_operators_string() {
    assert!(matches(r#"{"name":"b"}"#, r#"{"name":{"$gt":"a"}}"#));
    assert!(!matches(r#"{"name":"a"}"#, r#"{"name":{"$gt":"b"}}"#));
}

/// 缺失字段不匹配范围操作符。
#[test]
fn range_missing_field_fails() {
    assert!(!matches(r#"{"name":"张三"}"#, r#"{"age":{"$gt":18}}"#));
}

// ──────────────── $in / $nin ────────────────

/// $in:值等于数组任一元素。
#[test]
fn in_operator() {
    assert!(matches(r#"{"age":25}"#, r#"{"age":{"$in":[18,25,30]}}"#));
    assert!(!matches(r#"{"age":24}"#, r#"{"age":{"$in":[18,25,30]}}"#));
}

/// $nin:值不等于任一元素。
#[test]
fn nin_operator() {
    assert!(matches(r#"{"age":24}"#, r#"{"age":{"$nin":[18,25,30]}}"#));
    assert!(!matches(r#"{"age":25}"#, r#"{"age":{"$nin":[18,25,30]}}"#));
}

// ──────────────── $regex ────────────────

/// $regex:匹配字符串字段(正则语义)。
#[test]
fn regex_matches_string() {
    assert!(matches(
        r#"{"name":"张三丰"}"#,
        r#"{"name":{"$regex":"张三"}}"#
    ));
    assert!(matches(
        r#"{"name":"张三丰"}"#,
        r#"{"name":{"$regex":"^张"}}"#
    ));
    assert!(!matches(
        r#"{"name":"李四"}"#,
        r#"{"name":{"$regex":"^张"}}"#
    ));
}

/// $regex:非字符串字段不匹配。
#[test]
fn regex_non_string_field() {
    assert!(!matches(r#"{"age":30}"#, r#"{"age":{"$regex":"3"}}"#));
}

/// 非法正则应解析失败。
#[test]
fn regex_invalid_pattern() {
    let v: serde_json::Value = serde_json::from_str(r#"{"name":{"$regex":"["}}"#).unwrap();
    let err = Filter::from_value(&v).expect_err("非法正则应报错");
    assert!(
        matches!(err, nodblite::query::FilterError::InvalidRegex(_)),
        "实际: {:?}",
        err
    );
}

// ──────────────── 点号路径 ────────────────

/// 点号路径解析嵌套字段。
#[test]
fn dot_path() {
    let doc = r#"{"user":{"address":{"city":"北京"}}}"#;
    assert!(matches(doc, r#"{"user.address.city":"北京"}"#));
    assert!(!matches(doc, r#"{"user.address.city":"上海"}"#));
}

/// 点号路径 + 操作符。
#[test]
fn dot_path_with_operator() {
    assert!(matches(
        r#"{"user":{"age":30}}"#,
        r#"{"user.age":{"$gte":18}}"#
    ));
}

// ──────────────── 逻辑组合 ────────────────

/// 同字段多操作符按 AND 组合。
#[test]
fn same_field_ops_and() {
    let doc = r#"{"age":30}"#;
    assert!(matches(doc, r#"{"age":{"$gt":18,"$lt":60}}"#));
    assert!(!matches(doc, r#"{"age":{"$gt":18,"$lt":25}}"#));
}

/// 不同字段隐式 AND。
#[test]
fn multi_field_and() {
    let doc = r#"{"name":"张三","age":30}"#;
    assert!(matches(doc, r#"{"name":"张三","age":{"$gt":18}}"#));
    assert!(!matches(doc, r#"{"name":"张三","age":{"$gt":40}}"#));
}

/// 顶层 $or。
#[test]
fn top_level_or() {
    let doc = r#"{"age":30}"#;
    assert!(matches(
        doc,
        r#"{"$or":[{"age":{"$lt":18}},{"age":{"$gt":25}}]}"#
    ));
    assert!(!matches(
        doc,
        r#"{"$or":[{"age":{"$lt":18}},{"age":{"$gt":40}}]}"#
    ));
}

/// 顶层 $and。
#[test]
fn top_level_and() {
    assert!(matches(
        r#"{"age":30,"name":"张三"}"#,
        r#"{"$and":[{"age":{"$gt":18}},{"name":"张三"}]}"#
    ));
}

/// 顶层 $nor。
#[test]
fn top_level_nor() {
    assert!(matches(
        r#"{"age":30}"#,
        r#"{"$nor":[{"age":{"$lt":18}},{"age":{"$gt":60}}]}"#
    ));
    assert!(!matches(
        r#"{"age":30}"#,
        r#"{"$nor":[{"age":{"$lt":18}},{"age":{"$gt":25}}]}"#
    ));
}

/// 顶层 $not。
#[test]
fn top_level_not() {
    assert!(matches(r#"{"age":30}"#, r#"{"$not":{"age":{"$gt":60}}}"#));
    assert!(!matches(r#"{"age":30}"#, r#"{"$not":{"age":{"$gt":25}}}"#));
}

// ──────────────── 空 filter ────────────────

/// 空对象 filter 匹配所有。
#[test]
fn empty_filter_matches_all() {
    assert!(matches(r#"{"name":"张三"}"#, r#"{}"#));
}

/// null filter 匹配所有。
#[test]
fn null_filter_matches_all() {
    let doc: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(r#"{"name":"张三"}"#).unwrap();
    let f = Filter::from_value(&serde_json::Value::Null).unwrap();
    assert!(f.matches(&doc));
}

// ──────────────── 解析错误 ────────────────

/// filter 顶层非对象(数组)应报错。
#[test]
fn filter_rejects_non_object() {
    let v: serde_json::Value = serde_json::from_str("[1,2,3]").unwrap();
    let err = Filter::from_value(&v).expect_err("数组 filter 应报错");
    assert!(matches!(err, nodblite::query::FilterError::NotObject));
}

/// 未知操作符应报错。
#[test]
fn filter_rejects_unknown_operator() {
    let v: serde_json::Value = serde_json::from_str(r#"{"age":{"$magic":1}}"#).unwrap();
    let err = Filter::from_value(&v).expect_err("未知操作符应报错");
    assert!(
        matches!(err, nodblite::query::FilterError::InvalidOperator(_)),
        "实际: {:?}",
        err
    );
}

// ──────────────── SortSpec ────────────────

/// 非法 sort 顶层(非对象)应报错。
#[test]
fn sort_rejects_non_object() {
    let v: serde_json::Value = serde_json::from_str("[1,2]").unwrap();
    let err = SortSpec::from_value(&v).expect_err("数组 sort 应报错");
    assert!(matches!(err, SortError::NotObject));
}

/// 非法排序方向应报错。
#[test]
fn sort_rejects_invalid_direction() {
    let v: serde_json::Value = serde_json::from_str(r#"{"age":2}"#).unwrap();
    let err = SortSpec::from_value(&v).expect_err("方向 2 应报错");
    assert!(matches!(err, SortError::InvalidDirection(_)));
}

/// 空 sort 应报错。
#[test]
fn sort_rejects_empty() {
    let v: serde_json::Value = serde_json::from_str("{}").unwrap();
    let err = SortSpec::from_value(&v).expect_err("空 sort 应报错");
    assert!(matches!(err, SortError::Empty));
}

/// 多字段排序:先 age 升序,再 name 降序。
#[test]
fn sort_multi_field_order() {
    let v: serde_json::Value = serde_json::from_str(r#"{"age":1,"name":-1}"#).unwrap();
    let spec = SortSpec::from_value(&v).unwrap();
    let mut maps: Vec<serde_json::Map<String, serde_json::Value>> = vec![
        json!({"age":20,"name":"b"}).as_object().unwrap().clone(),
        json!({"age":20,"name":"a"}).as_object().unwrap().clone(),
        json!({"age":30,"name":"c"}).as_object().unwrap().clone(),
        json!({"age":25,"name":"d"}).as_object().unwrap().clone(),
    ];
    spec.sort(&mut maps);
    let names: Vec<&str> = maps.iter().map(|m| m["name"].as_str().unwrap()).collect();
    // age 升序:20,20,25,30;同 age=20 按 name 降序:b,a
    assert_eq!(names, vec!["b", "a", "d", "c"]);
}

/// 缺失字段排最前(类型序:缺失 < number)。
#[test]
fn sort_missing_first() {
    let v: serde_json::Value = serde_json::from_str(r#"{"age":1}"#).unwrap();
    let spec = SortSpec::from_value(&v).unwrap();
    let mut maps = vec![
        json!({"age":30}).as_object().unwrap().clone(),
        json!({"name":"x"}).as_object().unwrap().clone(),
    ];
    spec.sort(&mut maps);
    assert!(maps[0].get("age").is_none(), "缺失 age 应排最前");
}
