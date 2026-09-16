//! `$unset` 更新操作符测试(M10)。
//!
//! 覆盖:顶层/嵌套字段删除、数组元素置 null、路径不存在静默无操作、
//! `_id` 不可删、与 `$set` 组合、空操作/非法参数报错。

use nosqlite::query::Update;
use serde_json::json;

/// 辅助:从 JSON 构造 update。
fn u(v: serde_json::Value) -> Update {
    Update::from_value(&v).expect("update 应合法")
}

/// 辅助:构造文档并应用 update。
fn apply(doc: serde_json::Value, update: serde_json::Value) -> serde_json::Value {
    let mut map = doc.as_object().unwrap().clone();
    u(update).apply(&mut map).expect("应用 update 应成功");
    serde_json::Value::Object(map)
}

/// 顶层字段删除。
#[test]
fn unset_top_level_field() {
    let doc = apply(
        json!({"name": "张三", "age": 30}),
        json!({"$unset": {"age": ""}}),
    );
    assert_eq!(doc, json!({"name": "张三"}));
}

/// 嵌套点号路径删除;路径不存在时静默无操作。
#[test]
fn unset_nested_and_missing() {
    let doc = apply(
        json!({"a": {"b": {"c": 1, "d": 2}}}),
        json!({"$unset": {"a.b.c": "", "x.y.z": ""}}),
    );
    assert_eq!(doc, json!({"a": {"b": {"d": 2}}}));

    // 中间路径是标量 → 无操作不报错
    let doc = apply(json!({"a": 1}), json!({"$unset": {"a.b.c": ""}}));
    assert_eq!(doc, json!({"a": 1}));
}

/// 数组元素 unset 后置 null(占位保留,对齐 MongoDB);越界无操作。
#[test]
fn unset_array_element() {
    let doc = apply(json!({"arr": [1, 2, 3]}), json!({"$unset": {"arr.1": ""}}));
    assert_eq!(doc, json!({"arr": [1, null, 3]}));

    // 越界:无操作
    let doc = apply(json!({"arr": [1, 2, 3]}), json!({"$unset": {"arr.5": ""}}));
    assert_eq!(doc, json!({"arr": [1, 2, 3]}));
}

/// 主键 `_id` 不可删除(解析期即报错)。
#[test]
fn cannot_unset_id() {
    let err = Update::from_value(&json!({"$unset": {"_id": ""}})).unwrap_err();
    assert!(err.to_string().contains("_id"), "错误应提及 _id: {}", err);
}

/// 与 `$set` 组合:同一文档先赋值后删除,`$unset` 同路径时以删除生效。
#[test]
fn combined_with_set() {
    let doc = apply(
        json!({"a": 1, "b": 2}),
        json!({"$set": {"c": 3}, "$unset": {"b": ""}}),
    );
    assert_eq!(doc, json!({"a": 1, "c": 3}));

    // 同一路径:先赋值后删除 → 最终不存在
    let doc = apply(
        json!({"a": {"x": 1}}),
        json!({"$set": {"a.y": 2}, "$unset": {"a": ""}}),
    );
    assert_eq!(doc, json!({}));
}

/// 空对象 / 缺操作符 / 空字段路径 / 非对象值 均报错。
#[test]
fn invalid_forms_rejected() {
    // 两个操作符都为空
    assert!(Update::from_value(&json!({"$set": {}, "$unset": {}})).is_err());
    // 缺少操作符
    assert!(Update::from_value(&json!({"a": 1})).is_err());
    // $unset 值不是对象
    assert!(Update::from_value(&json!({"$unset": "a"})).is_err());
    // 空路径段(解析期报错)
    assert!(Update::from_value(&json!({"$unset": {"a..b": ""}})).is_err());
    // 未知操作符仍然报错
    assert!(Update::from_value(&json!({"$bogus": {}})).is_err());
}

/// 经 Database::update 走 `$unset` 的端到端验证。
#[test]
fn end_to_end_via_database() {
    let path = std::env::temp_dir().join(format!("nosqlite_unset_{}.nodb", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let db = nosqlite::Database::create(&path).expect("创建库应成功");

    db.insert(
        "users",
        &mut nosqlite::Document::from_json(r#"{"_id":"u1","name":"张三","tmp":"待删","age":30}"#)
            .unwrap(),
    )
    .unwrap();

    let filter = nosqlite::query::Filter::from_value(&json!({"_id": "u1"})).unwrap();
    let update = Update::from_value(&json!({"$unset": {"tmp": ""}})).unwrap();
    let changed = db.update("users", &filter, &update).unwrap();
    assert_eq!(changed, 1, "应更新 1 条");

    let doc = db
        .find("users", &nosqlite::query::Filter::match_all())
        .unwrap();
    assert_eq!(doc.len(), 1);
    let map = &doc[0];
    assert!(map.get("tmp").is_none(), "tmp 字段应已被删除");
    assert!(map.get("name").is_some(), "其它字段应保留");

    drop(db);
    let _ = std::fs::remove_file(&path);
}
