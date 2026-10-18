//! 与 MongoDB 的查询语义对齐(v0.1.2 修正 6 项不一致)。
//!
//! 这些用例固化的是**MongoDB 的行为**,防止旧语义回潮。
//!
//! | # | 语义 |
//! |---|---|
//! | 1 | 同层字段条件与 `$or` 是 **AND** |
//! | 2 | `$in` 对数组字段按元素展开 |
//! | 3 | 隐式 `$eq` 标量可命中「数组包含该标量」 |
//! | 4 | 点号路径穿透数组 |
//! | 5 | 缺失字段匹配 `null` |
//! | 6 | 字段级 `$not` |

use nosqlite::query::{Filter, FilterError};
use serde_json::{Map, Value, json};

/// 构造 filter。
fn f(value: Value) -> Filter {
    Filter::from_value(&value).expect("filter 应解析成功")
}

/// 把 JSON 转成文档 Map。
fn d(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

/// 便捷判断「filter 是否匹配该文档」。
fn hit(filter_json: Value, doc_json: Value) -> bool {
    f(filter_json).matches(&d(doc_json))
}

// ==================== #1 同层字段条件与 $or 是 AND ====================

/// `{"a": 1, "$or": [...]}` 要求字段条件与 `$or` 同时成立。
#[test]
fn field_condition_and_or_are_anded() {
    let filter = json!({"status": "ok", "$or": [{"n": 1}, {"n": 2}]});

    // 字段条件成立且 $or 成立 → 命中
    assert!(hit(filter.clone(), json!({"status": "ok", "n": 1})));
    assert!(hit(filter.clone(), json!({"status": "ok", "n": 2})));

    // $or 成立但字段条件不成立 → 不命中(旧版会误命中)
    assert!(!hit(filter.clone(), json!({"status": "bad", "n": 1})));

    // 字段条件成立但 $or 不成立 → 不命中
    assert!(!hit(filter.clone(), json!({"status": "ok", "n": 3})));
}

/// 字段条件 + `$and` 仍是 AND(回归)。
#[test]
fn field_condition_and_and_are_anded() {
    let filter = json!({"a": 1, "$and": [{"b": 2}, {"c": 3}]});
    assert!(hit(filter.clone(), json!({"a": 1, "b": 2, "c": 3})));
    assert!(!hit(filter.clone(), json!({"a": 1, "b": 2, "c": 4})));
    assert!(!hit(filter.clone(), json!({"a": 9, "b": 2, "c": 3})));
}

/// 字段条件 + `$nor`:字段条件成立且 `$nor` 各项都不成立。
#[test]
fn field_condition_and_nor() {
    let filter = json!({"a": 1, "$nor": [{"b": 2}]});
    assert!(hit(filter.clone(), json!({"a": 1, "b": 3})));
    assert!(!hit(filter.clone(), json!({"a": 1, "b": 2})));
    assert!(!hit(filter.clone(), json!({"a": 9, "b": 3})));
}

/// 单一 `$or` 仍然只在其内部做 OR。
#[test]
fn or_alone_still_or() {
    let filter = json!({"$or": [{"n": 1}, {"n": 2}]});
    assert!(hit(filter.clone(), json!({"n": 1})));
    assert!(hit(filter.clone(), json!({"n": 2})));
    assert!(!hit(filter.clone(), json!({"n": 3})));
}

// ==================== #2 $in 展开数组字段 ====================

/// 数组字段与 `$in` 求交:任一元素命中任一候选即可。
#[test]
fn in_expands_array_field() {
    let filter = json!({"tags": {"$in": ["rust", "go"]}});
    assert!(hit(filter.clone(), json!({"tags": ["rust", "db"]})));
    assert!(hit(filter.clone(), json!({"tags": ["c", "go"]})));
    // 整体相等的写法依然有效
    assert!(hit(filter.clone(), json!({"tags": ["rust", "go"]})));
    assert!(!hit(filter.clone(), json!({"tags": ["c", "java"]})));
    // 标量字段不受影响
    assert!(hit(filter.clone(), json!({"tags": "rust"})));
    assert!(!hit(filter.clone(), json!({"tags": "java"})));
}

/// `$nin` 是 `$in` 的取反。
#[test]
fn nin_is_negation_of_in() {
    let filter = json!({"tags": {"$nin": ["rust", "go"]}});
    assert!(!hit(filter.clone(), json!({"tags": ["rust", "db"]})));
    assert!(hit(filter.clone(), json!({"tags": ["c", "java"]})));
    assert!(hit(filter.clone(), json!({"tags": "java"})));
}

// ==================== #3 隐式 $eq 命中数组包含 ====================

/// 标量等值可以命中数组字段(数组包含该标量)。
#[test]
fn eq_scalar_hits_array_containment() {
    let filter = json!({"tags": "rust"});
    assert!(hit(filter.clone(), json!({"tags": "rust"})));
    assert!(
        hit(filter.clone(), json!({"tags": ["rust", "db"]})),
        "数组包含应命中"
    );
    assert!(!hit(filter.clone(), json!({"tags": ["db"]})));
}

/// 显式 `$eq` 同理。
#[test]
fn explicit_eq_scalar_hits_array_containment() {
    let filter = json!({"tags": {"$eq": "rust"}});
    assert!(hit(filter.clone(), json!({"tags": ["rust"]})));
    assert!(!hit(filter.clone(), json!({"tags": ["go"]})));
}

/// 数组对数组仍是整体深度相等(不是「有交集」)。
#[test]
fn eq_array_to_array_is_exact() {
    let filter = json!({"tags": ["a", "b"]});
    assert!(hit(filter.clone(), json!({"tags": ["a", "b"]})));
    assert!(!hit(filter.clone(), json!({"tags": ["a"]})));
    assert!(!hit(filter.clone(), json!({"tags": ["a", "b", "c"]})));
}

/// 嵌套数组:元素本身是数组时也按「包含」逐层展开。
#[test]
fn eq_recurses_nested_arrays() {
    let filter = json!({"v": "x"});
    assert!(hit(filter.clone(), json!({"v": [["x"], "y"]})));
}

// ==================== #4 点号路径穿透数组 ====================

/// `items.name` 遍历数组元素的同名字段。
#[test]
fn dotted_path_traverses_array() {
    let filter = json!({"items.name": "x"});
    let doc = json!({"items": [{"name": "a"}, {"name": "x"}]});
    assert!(hit(filter.clone(), doc));
    assert!(!hit(filter.clone(), json!({"items": [{"name": "a"}]})));
}

/// 穿透数组时,比较类操作符由任一命中元素满足。
#[test]
fn dotted_path_traverses_array_with_comparison() {
    let filter = json!({"items.qty": {"$gt": 5}});
    let doc = json!({"items": [{"qty": 1}, {"qty": 10}]});
    assert!(hit(filter.clone(), doc));
    assert!(!hit(
        filter.clone(),
        json!({"items": [{"qty": 1}, {"qty": 2}]})
    ));
}

/// 同一字段的多个操作符需分别被满足(可来自不同元素,同 MongoDB)。
#[test]
fn dotted_path_multiple_ops_separately_satisfied() {
    let filter = json!({"items.qty": {"$gt": 5, "$lt": 20}});
    // 两个条件分别由不同元素满足 → 命中
    assert!(hit(
        filter.clone(),
        json!({"items": [{"qty": 10}, {"qty": 15}]})
    ));
    // 无任何元素 > 5 → 不命中
    assert!(!hit(
        filter.clone(),
        json!({"items": [{"qty": 1}, {"qty": 2}]})
    ));
    // 无任何元素 < 20 → 不命中
    assert!(!hit(
        filter.clone(),
        json!({"items": [{"qty": 30}, {"qty": 40}]})
    ));
}

/// 多层数组路径。
#[test]
fn dotted_path_multi_level_array() {
    let filter = json!({"a.b.c": 3});
    let doc = json!({"a": [{"b": [{"c": 1}, {"c": 3}]}]});
    assert!(hit(filter.clone(), doc));
}

/// 路径不存在时仍不命中(空候选值列表)。
#[test]
fn dotted_path_missing_is_no_match() {
    assert!(!hit(json!({"a.b": 1}), json!({"x": 1})));
}

// ==================== #5 缺失字段匹配 null ====================

/// `$eq: null` 匹配字段缺失的文档。
#[test]
fn eq_null_matches_missing_field() {
    let filter = json!({"deleted_at": null});
    assert!(hit(filter.clone(), json!({"deleted_at": null})));
    assert!(
        hit(filter.clone(), json!({"name": "x"})),
        "缺失字段应匹配 null"
    );
    assert!(!hit(filter.clone(), json!({"deleted_at": 0})));
}

/// `$ne: null` 不匹配缺失字段。
#[test]
fn ne_null_rejects_missing_field() {
    let filter = json!({"deleted_at": {"$ne": null}});
    assert!(
        !hit(filter.clone(), json!({"name": "x"})),
        "缺失字段不应匹配 $ne: null"
    );
    assert!(hit(filter.clone(), json!({"deleted_at": 5})));
    assert!(!hit(filter.clone(), json!({"deleted_at": null})));
}

/// 一般 `$ne`(非 null)仍匹配缺失字段。
#[test]
fn ne_non_null_matches_missing_field() {
    let filter = json!({"a": {"$ne": 5}});
    assert!(hit(filter.clone(), json!({"a": 1})));
    assert!(
        hit(filter.clone(), json!({"b": 1})),
        "缺失字段应匹配 $ne: 5"
    );
    assert!(!hit(filter.clone(), json!({"a": 5})));
}

// ==================== #6 字段级 $not ====================

/// 字段级 `$not` 对单操作符取反。
#[test]
fn field_level_not_single_op() {
    let filter = json!({"age": {"$not": {"$gt": 18}}});
    assert!(hit(filter.clone(), json!({"age": 10})));
    assert!(!hit(filter.clone(), json!({"age": 20})));
    // 缺失字段:`$gt` 不成立 → `$not` 成立
    assert!(hit(filter.clone(), json!({"name": "x"})));
}

/// 字段级 `$not` 内部多个操作符为 AND,再整体取反。
#[test]
fn field_level_not_multiple_ops() {
    let filter = json!({"age": {"$not": {"$gte": 18, "$lt": 65}}});
    assert!(!hit(filter.clone(), json!({"age": 30})));
    assert!(hit(filter.clone(), json!({"age": 70})));
    assert!(hit(filter.clone(), json!({"age": 10})));
}

/// 字段级 `$not` 可与同字段其它操作符共存(AND)。
#[test]
fn field_level_not_with_other_ops() {
    let filter = json!({"age": {"$not": {"$gt": 100}, "$gt": 10}});
    assert!(hit(filter.clone(), json!({"age": 50})));
    assert!(!hit(filter.clone(), json!({"age": 5})));
}

/// 字段级 `$not` 内部是空对象或逻辑操作符时报错。
#[test]
fn field_level_not_invalid_forms() {
    for bad in [
        json!({"a": {"$not": {}}}),
        json!({"a": {"$not": {"$and": [{"b": 1}]}}}),
        json!({"a": {"$not": 5}}),
    ] {
        let err = Filter::from_value(&bad).expect_err("应报错");
        assert!(
            matches!(err, FilterError::InvalidOperator(_)),
            "实际: {:?}",
            err
        );
    }
}

/// 顶层 `$not` 不受影响(回归)。
#[test]
fn top_level_not_still_works() {
    let filter = json!({"$not": {"a": 1}});
    assert!(hit(filter.clone(), json!({"a": 2})));
    assert!(!hit(filter.clone(), json!({"a": 1})));
}
