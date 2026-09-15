//! 数组下标路径更新测试(M9,缺陷 #1 回归)。
//!
//! M9 前 `set_by_path` 在路径中间遇到数组时会**用对象覆盖整个数组**(数据破坏缺陷)。
//! 本文件覆盖修复后的语义:
//!
//! - 数组按下标寻址,下标可以是多位数字;
//! - 下标越界、下标非数字一律报 [`UpdateError`],不修改文档;
//! - 路径中间是标量时报错,**原值不被覆盖**;
//! - 数组本身结构(长度、其余元素)保持不变。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use nosqlite::query::{Filter, Update, UpdateError};
use nosqlite::{Database, DatabaseError, Document};

/// 全局自增计数器,用于生成唯一的临时文件路径。
static COUNTER: AtomicU32 = AtomicU32::new(0);

/// 生成唯一的临时 `.nodb` 测试文件路径。
fn temp_nodb(name: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!(
        "nosqlite_test_{}_{}_{}.nodb",
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

/// 辅助:创建数据库并插入一条指定 JSON 的文档(集合固定为 `items`,_id 固定为 `i1`)。
fn setup_with(path: &Path, json: &str) -> Database {
    let db = Database::create(path).expect("创建数据库应成功");
    let mut doc = Document::from_json(json).expect("测试文档应合法");
    db.insert("items", &mut doc).expect("插入应成功");
    db
}

/// 辅助:取回唯一文档。
fn get_one(db: &Database) -> serde_json::Map<String, serde_json::Value> {
    let docs = db.find("items", &filter(r#"{"_id":"i1"}"#)).expect("查询应成功");
    assert_eq!(docs.len(), 1, "应恰好有一条文档");
    docs.into_iter().next().unwrap()
}

/// 数组元素赋值:按下标替换元素,数组长度与其余元素不变。
#[test]
fn set_array_element_by_index() {
    let path = temp_nodb("arr_index");
    let db = setup_with(&path, r#"{"_id":"i1","tags":["a","b","c"]}"#);

    let n = db
        .update("items", &filter(r#"{"_id":"i1"}"#), &update(r#"{"$set":{"tags.1":"X"}}"#))
        .expect("数组下标赋值应成功");
    assert_eq!(n, 1, "应修改 1 条文档");

    let doc = get_one(&db);
    let tags = doc["tags"].as_array().expect("tags 必须仍是数组");
    assert_eq!(
        tags,
        &vec![
            serde_json::json!("a"),
            serde_json::json!("X"),
            serde_json::json!("c")
        ],
        "只应替换下标 1,其余元素不变"
    );

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 数组元素的嵌套对象字段:走 `数组下标.字段` 路径。
///
/// 这是缺陷 #1 的核心回归:修复前会把整个数组覆盖成对象。
#[test]
fn set_nested_field_in_array_element() {
    let path = temp_nodb("arr_nested");
    let db = setup_with(
        &path,
        r#"{"_id":"i1","microApps":[{"id":"m1","name":"旧"},{"id":"m2","name":"另一个"}]}"#,
    );

    let n = db
        .update(
            "items",
            &filter(r#"{"_id":"i1"}"#),
            &update(r#"{"$set":{"microApps.0.name":"新"}}"#),
        )
        .expect("数组元素嵌套字段赋值应成功");
    assert_eq!(n, 1);

    let doc = get_one(&db);
    let apps = doc["microApps"]
        .as_array()
        .expect("microApps 必须仍是数组(修复前会被对象覆盖)");
    assert_eq!(apps.len(), 2, "数组长度必须保持不变");
    assert_eq!(apps[0]["name"], "新", "下标 0 的 name 应被更新");
    assert_eq!(apps[0]["id"], "m1", "同元素的兄弟字段应保留");
    assert_eq!(apps[1]["name"], "另一个", "下标 1 的元素不应受影响");

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 多位数字下标同样按索引寻址。
#[test]
fn set_multi_digit_array_index() {
    let path = temp_nodb("arr_multidigit");
    let db = setup_with(
        &path,
        r#"{"_id":"i1","list":[0,1,2,3,4,5,6,7,8,9,10,11]}"#,
    );

    db.update("items", &filter(r#"{"_id":"i1"}"#), &update(r#"{"$set":{"list.10":99}}"#))
        .expect("两位下标赋值应成功");

    let doc = get_one(&db);
    let list = doc["list"].as_array().expect("list 必须仍是数组");
    assert_eq!(list.len(), 12, "数组长度必须保持不变");
    assert_eq!(list[10], 99, "下标 10 应被更新为 99");
    assert_eq!(list[9], 9, "下标 9 不应受影响");

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 给数组元素对象新增字段(该字段原本不存在)。
#[test]
fn set_new_field_in_array_element() {
    let path = temp_nodb("arr_newfield");
    let db = setup_with(&path, r#"{"_id":"i1","rows":[{"a":1},{"a":2}]}"#);

    db.update("items", &filter(r#"{"_id":"i1"}"#), &update(r#"{"$set":{"rows.1.b":true}}"#))
        .expect("数组元素新增字段应成功");

    let doc = get_one(&db);
    let rows = doc["rows"].as_array().expect("rows 必须仍是数组");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1]["a"], 2, "既有字段应保留");
    assert_eq!(rows[1]["b"], true, "新字段应被创建");
    assert!(
        rows[0].get("b").is_none(),
        "下标 0 的元素不应被添加字段"
    );

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 多级混合路径:对象 → 数组 → 对象 → 数组。
#[test]
fn set_deep_mixed_path() {
    let path = temp_nodb("arr_deep");
    let db = setup_with(
        &path,
        r#"{"_id":"i1","a":{"b":[{"c":["x","y"]},{"c":["z"]}]}}"#,
    );

    db.update(
        "items",
        &filter(r#"{"_id":"i1"}"#),
        &update(r#"{"$set":{"a.b.0.c.1":"Y"}}"#),
    )
    .expect("多级混合路径赋值应成功");

    let doc = get_one(&db);
    let c = doc["a"]["b"][0]["c"]
        .as_array()
        .expect("c 必须仍是数组");
    assert_eq!(
        c,
        &vec![serde_json::json!("x"), serde_json::json!("Y")],
        "只有目标元素被替换"
    );
    assert_eq!(
        doc["a"]["b"][1]["c"],
        serde_json::json!(["z"]),
        "兄弟数组不应受影响"
    );

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 下标越界:报 `ArrayIndexOutOfBounds`,文档保持不变。
#[test]
fn set_array_index_out_of_bounds() {
    let path = temp_nodb("arr_oob");
    let db = setup_with(&path, r#"{"_id":"i1","tags":["a","b","c"]}"#);

    let err = db
        .update("items", &filter(r#"{"_id":"i1"}"#), &update(r#"{"$set":{"tags.5":"X"}}"#))
        .expect_err("越界下标应失败");
    assert!(
        matches!(
            err,
            DatabaseError::Update(UpdateError::ArrayIndexOutOfBounds { index: 5, len: 3 })
        ),
        "应报越界错误(索引 5 / 长度 3),实际: {:?}",
        err
    );

    let doc = get_one(&db);
    assert_eq!(
        doc["tags"],
        serde_json::json!(["a", "b", "c"]),
        "越界失败后文档必须原样保留"
    );

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 下标恰好等于数组长度(边界)同样越界。
#[test]
fn set_array_index_equal_to_len_is_out_of_bounds() {
    let path = temp_nodb("arr_edge");
    let db = setup_with(&path, r#"{"_id":"i1","tags":["a"]}"#);

    let err = db
        .update("items", &filter(r#"{"_id":"i1"}"#), &update(r#"{"$set":{"tags.1":"X"}}"#))
        .expect_err("下标等于长度应失败");
    assert!(
        matches!(
            err,
            DatabaseError::Update(UpdateError::ArrayIndexOutOfBounds { index: 1, len: 1 })
        ),
        "应报越界错误,实际: {:?}",
        err
    );

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 下标非数字:报 `InvalidArrayIndex`,文档保持不变。
#[test]
fn set_array_index_invalid() {
    let path = temp_nodb("arr_invalid");
    let db = setup_with(&path, r#"{"_id":"i1","tags":["a","b"]}"#);

    let err = db
        .update("items", &filter(r#"{"_id":"i1"}"#), &update(r#"{"$set":{"tags.x":"X"}}"#))
        .expect_err("非数字下标应失败");
    assert!(
        matches!(
            err,
            DatabaseError::Update(UpdateError::InvalidArrayIndex(ref s)) if s == "x"
        ),
        "应报下标非法,实际: {:?}",
        err
    );

    let doc = get_one(&db);
    assert_eq!(
        doc["tags"],
        serde_json::json!(["a", "b"]),
        "失败后文档必须原样保留"
    );

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 路径中间是标量:报 `ScalarIntermediate`,原值不被覆盖(修复前会用对象覆盖)。
#[test]
fn set_scalar_intermediate_rejected() {
    let path = temp_nodb("scalar_inter");
    let db = setup_with(&path, r#"{"_id":"i1","a":5}"#);

    let err = db
        .update("items", &filter(r#"{"_id":"i1"}"#), &update(r#"{"$set":{"a.b":1}}"#))
        .expect_err("标量中间值应失败");
    assert!(
        matches!(err, DatabaseError::Update(UpdateError::ScalarIntermediate { .. })),
        "应报标量中间值错误,实际: {:?}",
        err
    );

    let doc = get_one(&db);
    assert_eq!(doc["a"], 5, "标量原值必须保持不变,不得被覆盖成对象");

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 数组下标更新后持久化:重新打开数据库仍能看到结果。
#[test]
fn array_update_persists_across_reopen() {
    let path = temp_nodb("arr_persist");
    let db = setup_with(&path, r#"{"_id":"i1","tags":["a","b"]}"#);
    db.update("items", &filter(r#"{"_id":"i1"}"#), &update(r#"{"$set":{"tags.0":"A"}}"#))
        .expect("更新应成功");
    db.close().expect("关闭应成功");
    drop(db); // redb 独占锁,必须先释放句柄

    let db = Database::open(&path).expect("重新打开应成功");
    let doc = get_one(&db);
    let tags = doc["tags"].as_array().expect("tags 必须仍是数组");
    assert_eq!(
        tags,
        &vec![serde_json::json!("A"), serde_json::json!("b")],
        "重新打开后应仍保留数组更新结果"
    );

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}
