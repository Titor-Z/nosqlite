//! 过滤操作符补充(v0.7.0):`$expr`、`$mod`、`$regex` 的 `$options`。

use nodblite::query::Filter;
use serde_json::{Map, Value, json};

fn f(v: Value) -> Filter {
    Filter::from_value(&v).expect("filter 应合法")
}

fn d(v: Value) -> Map<String, Value> {
    v.as_object().unwrap().clone()
}

// ---------------------------- $expr ----------------------------

/// `$expr`:同层字段之间比较(MongoDB 最常见用法)。
#[test]
fn expr_compares_two_fields() {
    let filter = f(json!({"$expr": {"$gt": ["$spent", "$budget"]}}));
    assert!(filter.matches(&d(json!({"spent": 120, "budget": 100}))));
    assert!(!filter.matches(&d(json!({"spent": 80, "budget": 100}))));
}

/// `$expr` 与普通字段条件按 AND 组合。
#[test]
fn expr_combines_with_field_conditions() {
    let filter = f(json!({"status": "ok", "$expr": {"$gte": ["$qty", 10]}}));
    assert!(filter.matches(&d(json!({"status": "ok", "qty": 10}))));
    assert!(!filter.matches(&d(json!({"status": "bad", "qty": 10}))));
    assert!(!filter.matches(&d(json!({"status": "ok", "qty": 9}))));
}

/// 缺失字段在表达式里视作 null,不会报错也不会匹配。
#[test]
fn expr_missing_field_is_null() {
    let filter = f(json!({"$expr": {"$gt": ["$missing", 5]}}));
    assert!(!filter.matches(&d(json!({"a": 1}))));
}

/// `$expr` 支持嵌套表达式与 `$cond`。
#[test]
fn expr_nested_expression() {
    let filter = f(json!({
        "$expr": {"$eq": [{"$cond": [{"$gt": ["$a", 5]}, "big", "small"]}, "big"]}
    }));
    assert!(filter.matches(&d(json!({"a": 9}))));
    assert!(!filter.matches(&d(json!({"a": 1}))));
}

/// 未知的表达式操作符不应让进程崩溃,按不匹配处理。
#[test]
fn expr_unknown_operator_does_not_match() {
    let filter = f(json!({"$expr": {"$function": {"body": "1"}}}));
    assert!(!filter.matches(&d(json!({"a": 1}))));
}

// ---------------------------- $mod ----------------------------

#[test]
fn mod_matches_remainder() {
    let filter = f(json!({"n": {"$mod": [4, 1]}}));
    assert!(filter.matches(&d(json!({"n": 9}))));
    assert!(filter.matches(&d(json!({"n": 5}))));
    assert!(!filter.matches(&d(json!({"n": 8}))));
}

#[test]
fn mod_rejects_bad_arguments() {
    assert!(Filter::from_value(&json!({"n": {"$mod": [4]}})).is_err());
    assert!(Filter::from_value(&json!({"n": {"$mod": 4}})).is_err());
    assert!(Filter::from_value(&json!({"n": {"$mod": [0, 1]}})).is_err());
}

/// 非数值字段不匹配(不 panic)。
#[test]
fn mod_ignores_non_numeric() {
    let filter = f(json!({"n": {"$mod": [2, 0]}}));
    assert!(!filter.matches(&d(json!({"n": "abc"}))));
    assert!(!filter.matches(&d(json!({}))));
}

// ---------------------------- $regex $options ----------------------------

#[test]
fn regex_case_insensitive() {
    let filter = f(json!({"name": {"$regex": "^ada", "$options": "i"}}));
    assert!(filter.matches(&d(json!({"name": "Ada"}))));
    assert!(filter.matches(&d(json!({"name": "ADAM"}))));
    assert!(!filter.matches(&d(json!({"name": "bob"}))));
}

#[test]
fn regex_multiline_and_dotall() {
    let multi = f(json!({"text": {"$regex": "^second", "$options": "m"}}));
    assert!(multi.matches(&d(json!({"text": "first\nsecond"}))));

    let dotall = f(json!({"text": {"$regex": "a.b", "$options": "s"}}));
    assert!(dotall.matches(&d(json!({"text": "a\nb"}))));
}

/// 未知修饰符显式报错,不静默忽略。
#[test]
fn regex_unknown_option_rejected() {
    assert!(Filter::from_value(&json!({"a": {"$regex": "x", "$options": "z"}})).is_err());
}

/// 只有 `$options` 而没有 `$regex` 时,它作为修饰符被忽略,
/// 该字段条件退化为空操作符集(恒真),而不是报「未知操作符」。
#[test]
fn options_alone_is_ignored() {
    let filter = f(json!({"a": {"$options": "i"}}));
    assert!(filter.matches(&d(json!({"a": 1}))));
    assert!(filter.matches(&d(json!({}))));
}
