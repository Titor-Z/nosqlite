//! 更新操作符扩展(v0.1.4):`$inc`/`$mul`/`$min`/`$max`/`$rename`/`$push`/`$addToSet`/`$currentDate`。

use nodblite::query::{Update, UpdateError, as_date};
use serde_json::{Map, Value, json};

/// 构造更新。
fn u(value: Value) -> Update {
    Update::from_value(&value).expect("update 应解析成功")
}

/// 把 JSON 转成文档 Map。
fn d(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

/// 应用更新并返回结果文档。
fn apply(doc: Value, update: Value) -> Map<String, Value> {
    let mut m = d(doc);
    u(update).apply(&mut m).expect("apply 应成功");
    m
}

/// 应用更新并返回错误。
fn apply_err(doc: Value, update: Value) -> UpdateError {
    let mut m = d(doc);
    u(update).apply(&mut m).expect_err("应失败")
}

// ==================== $inc ====================

/// 数值自增,整数保持整数。
#[test]
fn inc_basic() {
    let out = apply(json!({"views": 10}), json!({"$inc": {"views": 1}}));
    assert_eq!(out["views"], json!(11));
}

/// 字段缺失时按 0 处理。
#[test]
fn inc_missing_field_starts_from_zero() {
    let out = apply(json!({"name": "x"}), json!({"$inc": {"views": 5}}));
    assert_eq!(out["views"], json!(5));
}

/// 负数即自减。
#[test]
fn inc_negative() {
    let out = apply(json!({"n": 10}), json!({"$inc": {"n": -3}}));
    assert_eq!(out["n"], json!(7));
}

/// 小数与负结果。
#[test]
fn inc_float() {
    let out = apply(json!({"n": 1.5}), json!({"$inc": {"n": 0.25}}));
    assert_eq!(out["n"], json!(1.75));
}

/// 嵌套路径。
#[test]
fn inc_nested_path() {
    let out = apply(
        json!({"stat": {"views": 1}}),
        json!({"$inc": {"stat.views": 2}}),
    );
    assert_eq!(out["stat"]["views"], json!(3));
}

/// 数组元素自增。
#[test]
fn inc_array_element() {
    let out = apply(json!({"scores": [1, 2]}), json!({"$inc": {"scores.1": 10}}));
    assert_eq!(out["scores"], json!([1, 12]));
}

/// 非数值字段报错。
#[test]
fn inc_non_numeric_fails() {
    let err = apply_err(json!({"n": "abc"}), json!({"$inc": {"n": 1}}));
    assert!(matches!(err, UpdateError::TypeMismatch { .. }), "{:?}", err);
}

/// `$inc` 的值必须是数字。
#[test]
fn inc_value_must_be_number() {
    let err = Update::from_value(&json!({"$inc": {"n": "1"}})).expect_err("应报错");
    assert!(matches!(err, UpdateError::InvalidUpdate(_)), "{:?}", err);
}

/// 不允许修改 `_id`。
#[test]
fn inc_rejects_id() {
    let err = Update::from_value(&json!({"$inc": {"_id": 1}})).expect_err("应报错");
    assert!(matches!(err, UpdateError::InvalidUpdate(_)));
}

// ==================== $mul ====================

/// 乘法;字段缺失按 0(MongoDB 行为)。
#[test]
fn mul_basic_and_missing() {
    assert_eq!(
        apply(json!({"n": 3}), json!({"$mul": {"n": 2}}))["n"],
        json!(6)
    );
    assert_eq!(apply(json!({}), json!({"$mul": {"n": 5}}))["n"], json!(0));
}

// ==================== $min / $max ====================

/// 新值更小才写入。
#[test]
fn min_only_writes_when_smaller() {
    assert_eq!(
        apply(json!({"n": 10}), json!({"$min": {"n": 5}}))["n"],
        json!(5)
    );
    assert_eq!(
        apply(json!({"n": 10}), json!({"$min": {"n": 20}}))["n"],
        json!(10)
    );
}

/// 字段缺失时直接写入。
#[test]
fn min_missing_writes() {
    assert_eq!(apply(json!({}), json!({"$min": {"n": 7}}))["n"], json!(7));
}

/// 新值更大才写入。
#[test]
fn max_only_writes_when_larger() {
    assert_eq!(
        apply(json!({"n": 10}), json!({"$max": {"n": 20}}))["n"],
        json!(20)
    );
    assert_eq!(
        apply(json!({"n": 10}), json!({"$max": {"n": 5}}))["n"],
        json!(10)
    );
}

/// `$min`/`$max` 也可用于字符串(按字典序)。
#[test]
fn min_max_on_strings() {
    assert_eq!(
        apply(json!({"s": "b"}), json!({"$min": {"s": "a"}}))["s"],
        json!("a")
    );
    assert_eq!(
        apply(json!({"s": "b"}), json!({"$max": {"s": "c"}}))["s"],
        json!("c")
    );
}

// ==================== $rename ====================

/// 字段改名,原字段消失。
#[test]
fn rename_basic() {
    let out = apply(
        json!({"old": 1, "keep": 2}),
        json!({"$rename": {"old": "new"}}),
    );
    assert_eq!(out, d(json!({"new": 1, "keep": 2})));
}

/// 嵌套路径改名。
#[test]
fn rename_nested() {
    let out = apply(json!({"a": {"b": 1}}), json!({"$rename": {"a.b": "a.c"}}));
    assert_eq!(out, d(json!({"a": {"c": 1}})));
}

/// 源字段不存在时无操作。
#[test]
fn rename_missing_source_is_noop() {
    let out = apply(json!({"keep": 1}), json!({"$rename": {"nope": "new"}}));
    assert_eq!(out, d(json!({"keep": 1})));
}

/// 目标必须是字符串。
#[test]
fn rename_target_must_be_string() {
    let err = Update::from_value(&json!({"$rename": {"a": 1}})).expect_err("应报错");
    assert!(matches!(err, UpdateError::InvalidUpdate(_)));
}

// ==================== $push ====================

/// 追加单个元素;字段缺失自动建数组。
#[test]
fn push_basic_and_missing() {
    assert_eq!(
        apply(json!({"tags": ["a"]}), json!({"$push": {"tags": "b"}}))["tags"],
        json!(["a", "b"])
    );
    assert_eq!(
        apply(json!({}), json!({"$push": {"tags": "a"}}))["tags"],
        json!(["a"])
    );
}

/// `$each` 批量追加。
#[test]
fn push_each() {
    let out = apply(
        json!({"tags": ["a"]}),
        json!({"$push": {"tags": {"$each": ["b", "c"]}}}),
    );
    assert_eq!(out["tags"], json!(["a", "b", "c"]));
}

/// `$position` 指定插入位置。
#[test]
fn push_position() {
    let out = apply(
        json!({"t": ["a", "d"]}),
        json!({"$push": {"t": {"$each": ["b", "c"], "$position": 1}}}),
    );
    assert_eq!(out["t"], json!(["a", "b", "c", "d"]));
}

/// `$position` 超出长度时追加到末尾。
#[test]
fn push_position_beyond_len() {
    let out = apply(
        json!({"t": ["a"]}),
        json!({"$push": {"t": {"$each": ["b"], "$position": 99}}}),
    );
    assert_eq!(out["t"], json!(["a", "b"]));
}

/// `$slice` 正数保留前 N / 负数保留后 N / 0 清空。
#[test]
fn push_slice() {
    let out = apply(
        json!({"t": [1, 2, 3]}),
        json!({"$push": {"t": {"$each": [4, 5], "$slice": 3}}}),
    );
    assert_eq!(out["t"], json!([1, 2, 3]));

    let out = apply(
        json!({"t": [1, 2, 3]}),
        json!({"$push": {"t": {"$each": [4, 5], "$slice": -2}}}),
    );
    assert_eq!(out["t"], json!([4, 5]));

    let out = apply(
        json!({"t": [1, 2]}),
        json!({"$push": {"t": {"$each": [], "$slice": 0}}}),
    );
    assert_eq!(out["t"], json!([]));
}

/// `$sort` 整体排序与按字段排序。
#[test]
fn push_sort() {
    let out = apply(
        json!({"t": [3, 1]}),
        json!({"$push": {"t": {"$each": [2], "$sort": 1}}}),
    );
    assert_eq!(out["t"], json!([1, 2, 3]));

    let out = apply(
        json!({"t": [3, 1]}),
        json!({"$push": {"t": {"$each": [2], "$sort": -1}}}),
    );
    assert_eq!(out["t"], json!([3, 2, 1]));

    // 按字段排序
    let out = apply(
        json!({"t": [{"v": 3}, {"v": 1}]}),
        json!({"$push": {"t": {"$each": [{"v": 2}], "$sort": {"v": 1}}}}),
    );
    let vs: Vec<i64> = out["t"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["v"].as_i64().unwrap())
        .collect();
    assert_eq!(vs, vec![1, 2, 3]);
}

/// `$position` + `$sort` + `$slice` 组合(CMS 常见「最近 N 条」模式)。
#[test]
fn push_combined_modifiers() {
    let out = apply(
        json!({"log": [{"n": 1}, {"n": 2}]}),
        json!({"$push": {"log": {
            "$each": [{"n": 3}],
            "$sort": {"n": -1},
            "$slice": 2
        }}}),
    );
    let ns: Vec<i64> = out["log"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["n"].as_i64().unwrap())
        .collect();
    assert_eq!(ns, vec![3, 2], "降序保留前 2 条");
}

/// 对非数组字段 `$push` 报错。
#[test]
fn push_non_array_fails() {
    let err = apply_err(json!({"t": "abc"}), json!({"$push": {"t": "x"}}));
    assert!(matches!(err, UpdateError::TypeMismatch { .. }), "{:?}", err);
}

/// 修饰符拼写错误报错(不静默失效)。
#[test]
fn push_unknown_modifier_fails() {
    let err = Update::from_value(&json!({"$push": {"t": {"$each": [1], "$positon": 0}}}))
        .expect_err("应报错");
    assert!(matches!(err, UpdateError::InvalidModifier(_)), "{:?}", err);
}

/// 修饰符缺少 `$each` 报错。
#[test]
fn push_modifier_requires_each() {
    let err = Update::from_value(&json!({"$push": {"t": {"$slice": 2}}})).expect_err("应报错");
    assert!(matches!(err, UpdateError::InvalidModifier(_)));
}

/// 想追加对象本身时可包在 `$each` 里。
#[test]
fn push_object_via_each() {
    let out = apply(
        json!({"t": []}),
        json!({"$push": {"t": {"$each": [{"a": 1}]}}}),
    );
    assert_eq!(out["t"], json!([{"a": 1}]));
}

// ==================== $addToSet ====================

/// 去重追加。
#[test]
fn add_to_set_dedupes() {
    let out = apply(json!({"t": ["a"]}), json!({"$addToSet": {"t": "a"}}));
    assert_eq!(out["t"], json!(["a"]), "已存在不应重复添加");

    let out = apply(json!({"t": ["a"]}), json!({"$addToSet": {"t": "b"}}));
    assert_eq!(out["t"], json!(["a", "b"]));
}

/// 字段缺失自动建数组。
#[test]
fn add_to_set_missing() {
    assert_eq!(
        apply(json!({}), json!({"$addToSet": {"t": "a"}}))["t"],
        json!(["a"])
    );
}

/// `$each` 批量去重。
#[test]
fn add_to_set_each() {
    let out = apply(
        json!({"t": ["a"]}),
        json!({"$addToSet": {"t": {"$each": ["a", "b", "b", "c"]}}}),
    );
    assert_eq!(out["t"], json!(["a", "b", "c"]));
}

/// 对象元素按深度相等去重。
#[test]
fn add_to_set_objects() {
    let out = apply(
        json!({"t": [{"a": 1}]}),
        json!({"$addToSet": {"t": {"a": 1}}}),
    );
    assert_eq!(out["t"], json!([{"a": 1}]));

    let out = apply(
        json!({"t": [{"a": 1}]}),
        json!({"$addToSet": {"t": {"a": 2}}}),
    );
    assert_eq!(out["t"], json!([{"a": 1}, {"a": 2}]));
}

/// 非数组字段报错。
#[test]
fn add_to_set_non_array_fails() {
    let err = apply_err(json!({"t": 1}), json!({"$addToSet": {"t": "x"}}));
    assert!(matches!(err, UpdateError::TypeMismatch { .. }));
}

// ==================== $currentDate ====================

/// 写入当前时间,格式为 `{"$date": 毫秒}`。
#[test]
fn current_date_writes_date_literal() {
    let before = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let out = apply(json!({}), json!({"$currentDate": {"updatedAt": true}}));
    let after = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    let ts = as_date(&out["updatedAt"]).expect("应为 Date 字面量");
    assert!(ts >= before && ts <= after, "时间应在调用区间内");
}

// ==================== 组合与顺序 ====================

/// 多个操作符可同时使用。
#[test]
fn multiple_operators_together() {
    let out = apply(
        json!({"views": 1, "old": "x"}),
        json!({
            "$inc": {"views": 2},
            "$set": {"name": "n"},
            "$rename": {"old": "moved"},
            "$push": {"tags": "t"}
        }),
    );
    assert_eq!(out["views"], json!(3));
    assert_eq!(out["name"], json!("n"));
    assert_eq!(out["moved"], json!("x"));
    assert_eq!(out["tags"], json!(["t"]));
    assert!(!out.contains_key("old"));
}

/// `$set` 与 `$unset` 同时作用于同一路径时,删除生效(沿用 M10 语义)。
#[test]
fn set_and_unset_same_path() {
    let out = apply(
        json!({"a": 1}),
        json!({"$set": {"a": 2}, "$unset": {"a": ""}}),
    );
    assert!(!out.contains_key("a"));
}

/// 全空操作符仍被拒绝。
#[test]
fn all_empty_ops_rejected() {
    for bad in [
        json!({"$inc": {}}),
        json!({"$push": {}}),
        json!({"$addToSet": {}}),
        json!({"$currentDate": {}}),
    ] {
        assert!(Update::from_value(&bad).is_err(), "应报错: {}", bad);
    }
}
