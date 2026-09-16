//! `$elemMatch` 过滤操作符测试(M10)。
//!
//! 覆盖:操作符形态(标量数组)、文档形态(对象数组)、嵌套路径、
//! 非数组字段/缺失字段不匹配、与其它条件组合。

use nosqlite::query::Filter;
use serde_json::json;

/// 辅助:从 JSON 构造 filter。
fn f(v: serde_json::Value) -> Filter {
    Filter::from_value(&v).expect("filter 应合法")
}

/// 辅助:构造文档。
fn d(v: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    v.as_object().unwrap().clone()
}

/// 操作符形态:标量数组,至少一个元素满足区间。
#[test]
fn ops_form_scalar_array() {
    let filter = f(json!({"scores": {"$elemMatch": {"$gte": 80, "$lt": 90}}}));
    // 85 在区间内 → 匹配
    assert!(filter.matches(&d(json!({"scores": [70, 85, 95]}))));
    // 全部不在区间 → 不匹配
    assert!(!filter.matches(&d(json!({"scores": [70, 95]}))));
    // 全部满足任一子条件不够,必须同一元素同时满足(AND):85+95 无 80~90 → 不匹配
    assert!(!filter.matches(&d(json!({"scores": [75, 95]}))));
}

/// 操作符形态对对象元素的整体 $eq:引擎对 object/array 的比较返回 Equal(仅保证排序稳定),
/// 因此对对象做深度相等匹配应使用文档形态(见 doc_form_object_array)。
#[test]
fn ops_form_object_element_limitation() {
    // 标量元素的操作符形态可靠
    let filter = f(json!({"items": {"$elemMatch": {"$eq": 5}}}));
    assert!(filter.matches(&d(json!({"items": [5]}))));
    assert!(!filter.matches(&d(json!({"items": [6]}))));
}

/// 文档形态:对象数组,元素文档需满足字段条件(含操作符)。
#[test]
fn doc_form_object_array() {
    let filter = f(json!({"items": {"$elemMatch": {"name": "x", "qty": {"$gt": 5}}}}));
    // 同一元素同时满足 name=x 且 qty>5
    assert!(filter.matches(&d(
        json!({"items": [{"name": "a", "qty": 1}, {"name": "x", "qty": 10}]})
    )));
    // 两个元素各满足一个条件,但无同一元素同时满足 → 不匹配
    assert!(!filter.matches(&d(
        json!({"items": [{"name": "x", "qty": 1}, {"name": "a", "qty": 10}]})
    )));
}

/// 文档形态:元素内嵌套路径与逻辑组合。
///
/// 注:本引擎同层字段条件与 `$or` 为 OR 关系(见 `combine_conditions` 文档),
/// 要表达 AND 需显式 `$and`。
#[test]
fn doc_form_nested_and_logic() {
    let filter = f(json!({"users": {"$elemMatch": {"$and": [
        {"addr.city": "上海"},
        {"$or": [{"vip": true}, {"age": {"$gte": 18}}]}
    ]}}}));
    assert!(filter.matches(&d(json!({"users": [
        {"addr": {"city": "上海"}, "vip": false, "age": 20}
    ]}))));
    assert!(!filter.matches(&d(json!({"users": [
        {"addr": {"city": "北京"}, "vip": true, "age": 20}
    ]}))));
    // 城市满足但 vip/age 均不满足 → 不匹配
    assert!(!filter.matches(&d(json!({"users": [
        {"addr": {"city": "上海"}, "vip": false, "age": 10}
    ]}))));
}

/// 字段缺失或不是数组 → 不匹配(数组为空也不匹配)。
#[test]
fn non_array_or_missing_field() {
    let filter = f(json!({"tags": {"$elemMatch": {"$eq": "a"}}}));
    // 缺失字段
    assert!(!filter.matches(&d(json!({"other": 1}))));
    // 空数组
    assert!(!filter.matches(&d(json!({"tags": []}))));
    // 字段不是数组(标量)
    assert!(!filter.matches(&d(json!({"tags": "a"}))));
}

/// 与其它顶层字段条件 AND 组合。
#[test]
fn combined_with_other_conditions() {
    let filter = f(json!({
        "name": "张三",
        "scores": {"$elemMatch": {"$gte": 60}}
    }));
    assert!(filter.matches(&d(json!({"name": "张三", "scores": [59, 60]}))));
    assert!(!filter.matches(&d(json!({"name": "李四", "scores": [90]}))));
    assert!(!filter.matches(&d(json!({"name": "张三", "scores": [59]}))));
}

/// `$elemMatch` 的值必须是对象。
#[test]
fn value_must_be_object() {
    assert!(Filter::from_value(&json!({"tags": {"$elemMatch": "a"}})).is_err());
    assert!(Filter::from_value(&json!({"tags": {"$elemMatch": [1]}})).is_err());
}

/// 操作符形态内部不允许未知操作符(错误向上传递)。
#[test]
fn ops_form_rejects_unknown_operator() {
    assert!(Filter::from_value(&json!({"tags": {"$elemMatch": {"$bogus": 1}}})).is_err());
}
