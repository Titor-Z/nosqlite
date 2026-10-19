//! 主键直接寻址测试(M11)。
//!
//! 覆盖 `Database::get_by_id` / `update_by_id` / `delete_by_id`,以及
//! `Database::update` / `delete` 在 filter 为 `{"_id": "xxx"}` 时的**快速路径**。
//!
//! 核心断言:
//! - 快速路径与全表扫描路径**行为完全一致**(返回值、计数、无变化不计数、错误语义);
//! - 非「`_id` 等值」形态的 filter(多字段、`$in`、非字符串 `_id`)**不得**误入快速路径。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use nodblite::query::{Filter, Update, UpdateError};
use nodblite::{Database, DatabaseError, Document};

/// 全局自增计数器,用于生成唯一的临时文件路径。
static COUNTER: AtomicU32 = AtomicU32::new(0);

/// 生成唯一的临时 `.nodb` 测试文件路径。
fn temp_nodb(name: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!(
        "nodblite_test_{}_{}_{}.nodb",
        name,
        std::process::id(),
        n
    ))
}

/// 清理测试遗留文件。
fn cleanup(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// 辅助:解析 filter。
fn filter(json: &str) -> Filter {
    let v: serde_json::Value = serde_json::from_str(json).expect("测试 JSON 应合法");
    Filter::from_value(&v).expect("filter 解析应成功")
}

/// 辅助:解析 update。
fn update(json: &str) -> Update {
    let v: serde_json::Value = serde_json::from_str(json).expect("测试 JSON 应合法");
    Update::from_value(&v).expect("update 解析应成功")
}

/// 辅助:创建数据库并插入三条文档。
fn setup(path: &Path) -> Database {
    let db = Database::create(path).expect("创建数据库应成功");
    for (id, name, age) in [("u1", "张三", 20), ("u2", "李四", 30), ("u3", "王五", 40)] {
        let mut doc =
            Document::from_json(&format!(r#"{{"_id":"{id}","name":"{name}","age":{age}}}"#))
                .expect("文档应合法");
        db.insert("users", &mut doc).expect("插入应成功");
    }
    db
}

/// `get_by_id` 能读到文档;不存在的主键返回 `None`。
#[test]
fn get_by_id_and_missing() {
    let path = temp_nodb("get_by_id");
    let db = setup(&path);

    let doc = db
        .get_by_id("users", "u2")
        .expect("读取应成功")
        .expect("u2 应存在");
    assert_eq!(doc["name"], "李四");
    assert_eq!(doc["age"], 30);

    assert!(
        db.get_by_id("users", "nobody")
            .expect("读取应成功")
            .is_none(),
        "不存在的主键应返回 None"
    );
    // 不存在的集合也不应报错
    assert!(
        db.get_by_id("no_such_collection", "u1")
            .expect("读取应成功")
            .is_none(),
        "不存在的集合应返回 None"
    );

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// `get_by_id` 能读到刚写入的更新结果。
#[test]
fn get_by_id_reflects_update() {
    let path = temp_nodb("get_after_update");
    let db = setup(&path);

    db.update_by_id("users", "u1", &update(r#"{"$set":{"age":99}}"#))
        .expect("更新应成功");

    let doc = db.get_by_id("users", "u1").expect("读取应成功").unwrap();
    assert_eq!(doc["age"], 99, "应读到更新后的值");

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// `update_by_id`:命中返回 `true`,不存在返回 `false`。
#[test]
fn update_by_id_hit_and_miss() {
    let path = temp_nodb("upd_by_id");
    let db = setup(&path);

    assert!(
        db.update_by_id("users", "u1", &update(r#"{"$set":{"vip":true}}"#))
            .expect("更新应成功"),
        "命中主键应返回 true"
    );
    assert!(
        !db.update_by_id("users", "nobody", &update(r#"{"$set":{"vip":true}}"#))
            .expect("更新应成功"),
        "主键不存在应返回 false"
    );

    let doc = db.get_by_id("users", "u1").expect("读取应成功").unwrap();
    assert_eq!(doc["vip"], true);
    // 其余文档不受影响
    let other = db.get_by_id("users", "u2").expect("读取应成功").unwrap();
    assert!(other.get("vip").is_none(), "其他文档不应被修改");

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// `update_by_id` 写入相同值时返回 `false`(不写回)。
#[test]
fn update_by_id_noop_returns_false() {
    let path = temp_nodb("upd_by_id_noop");
    let db = setup(&path);

    assert!(
        !db.update_by_id(
            "users",
            "u1",
            &update(r#"{"$set":{"name":"张三","age":20}}"#)
        )
        .expect("更新应成功"),
        "写入相同值应返回 false"
    );

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// `update_by_id` 不允许修改主键。
#[test]
fn update_by_id_rejects_id_change() {
    let path = temp_nodb("upd_by_id_pk");
    let db = setup(&path);

    let err = db
        .update_by_id("users", "u1", &update(r#"{"$set":{"_id":"u9"}}"#))
        .expect_err("修改主键应失败");
    assert!(
        matches!(err, DatabaseError::Update(UpdateError::InvalidUpdate(_))),
        "应报 Update 错误,实际: {:?}",
        err
    );

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// `delete_by_id`:命中返回 `true`,再删返回 `false`,文档确实消失。
#[test]
fn delete_by_id_hit_and_miss() {
    let path = temp_nodb("del_by_id");
    let db = setup(&path);

    assert!(
        db.delete_by_id("users", "u2").expect("删除应成功"),
        "命中主键应返回 true"
    );
    assert!(
        !db.delete_by_id("users", "u2").expect("删除应成功"),
        "重复删除应返回 false"
    );
    assert!(
        db.get_by_id("users", "u2").expect("读取应成功").is_none(),
        "文档应已被删除"
    );
    assert_eq!(
        db.count("users", &filter("{}")).expect("计数应成功"),
        2,
        "应只剩两条"
    );

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// **快速路径一致性**:`update` 用 `{"_id": ...}` 与用 `update_by_id` 行为一致。
#[test]
fn update_fast_path_matches_by_id() {
    let path = temp_nodb("fast_update");
    let db = setup(&path);

    // 命中:返回 1
    let n = db
        .update(
            "users",
            &filter(r#"{"_id":"u1"}"#),
            &update(r#"{"$set":{"age":21}}"#),
        )
        .expect("更新应成功");
    assert_eq!(n, 1, "命中应计 1 条");
    assert_eq!(
        db.get_by_id("users", "u1").unwrap().unwrap()["age"],
        21,
        "内容应已更新"
    );

    // 无变化:返回 0(与 update_by_id 的 false 语义一致)
    let n = db
        .update(
            "users",
            &filter(r#"{"_id":"u1"}"#),
            &update(r#"{"$set":{"age":21}}"#),
        )
        .expect("更新应成功");
    assert_eq!(n, 0, "无变化不应计数");

    // 未命中:返回 0
    let n = db
        .update(
            "users",
            &filter(r#"{"_id":"nobody"}"#),
            &update(r#"{"$set":{"age":1}}"#),
        )
        .expect("更新应成功");
    assert_eq!(n, 0, "未命中应返回 0");

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// **快速路径一致性**:`delete` 用 `{"_id": ...}` 与 `delete_by_id` 行为一致。
#[test]
fn delete_fast_path_matches_by_id() {
    let path = temp_nodb("fast_delete");
    let db = setup(&path);

    assert_eq!(
        db.delete("users", &filter(r#"{"_id":"u3"}"#))
            .expect("删除应成功"),
        1,
        "命中应计 1 条"
    );
    assert_eq!(
        db.delete("users", &filter(r#"{"_id":"u3"}"#))
            .expect("删除应成功"),
        0,
        "重复删除应返回 0"
    );
    assert_eq!(db.count("users", &filter("{}")).expect("计数应成功"), 2);

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// **不得误入快速路径**:`$in` 形态的 `_id` 条件仍按扫描语义处理。
#[test]
fn id_in_filter_uses_scan_path() {
    let path = temp_nodb("id_in");
    let db = setup(&path);

    // `{"_id": {"$in": [...]}}` 不是单一等值条件 → 必须走扫描,命中两条
    let n = db
        .update(
            "users",
            &filter(r#"{"_id":{"$in":["u1","u3"]}}"#),
            &update(r#"{"$set":{"tag":"hit"}}"#),
        )
        .expect("更新应成功");
    assert_eq!(n, 2, "$in 应命中两条");

    // `{"_id": {"$ne": ...}}` 同样不得误判
    let n = db
        .delete("users", &filter(r#"{"_id":{"$ne":"u2"}}"#))
        .expect("删除应成功");
    assert_eq!(n, 2, "$ne 应命中两条");

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// **不得误入快速路径**:`_id` 与非 `_id` 字段并存的 filter 仍走扫描。
#[test]
fn id_plus_other_field_uses_scan_path() {
    let path = temp_nodb("id_multi");
    let db = setup(&path);

    // 多字段条件:`_id` 等值 + `age` 条件 → 仍应正确过滤
    let n = db
        .update(
            "users",
            &filter(r#"{"_id":"u1","age":20}"#),
            &update(r#"{"$set":{"ok":true}}"#),
        )
        .expect("更新应成功");
    assert_eq!(n, 1, "两条件都满足才应命中");

    // 条件不满足 → 0
    let n = db
        .update(
            "users",
            &filter(r#"{"_id":"u1","age":999}"#),
            &update(r#"{"$set":{"ok":true}}"#),
        )
        .expect("更新应成功");
    assert_eq!(n, 0, "age 不匹配应返回 0");

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// **非字符串 `_id`**:`{"_id": 123}` 不得误入快速路径(键是字符串),应走扫描且不崩溃。
#[test]
fn non_string_id_filter_is_safe() {
    let path = temp_nodb("non_string_id");
    let db = setup(&path);

    let n = db
        .update(
            "users",
            &filter(r#"{"_id":123}"#),
            &update(r#"{"$set":{"x":1}}"#),
        )
        .expect("非字符串 _id 不应崩溃");
    assert_eq!(n, 0, "不应命中任何文档");

    let n = db
        .delete("users", &filter(r#"{"_id":123}"#))
        .expect("非字符串 _id 不应崩溃");
    assert_eq!(n, 0, "不应删除任何文档");
    assert_eq!(
        db.count("users", &filter("{}")).expect("计数应成功"),
        3,
        "原有文档应完好"
    );

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}
