//! 管理类功能测试(M5):count / collections / drop_collection。
//!
//! 覆盖集合级操作:统计、列出集合名(去重、有序、空库)、删除整个集合。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use nosqlite::query::Filter;
use nosqlite::{Database, Document};

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

/// 辅助:创建数据库并插入固定数据。
/// - `users`:u1(张三,20)、u2(李四,30)、u3(王五,10)
/// - `orders`:o1(100 元)、o2(200 元)
fn setup(path: &Path) -> Database {
    let db = Database::create(path).expect("创建数据库应成功");
    let mut u1 = Document::from_json(r#"{"_id":"u1","name":"张三","age":20}"#).unwrap();
    let mut u2 = Document::from_json(r#"{"_id":"u2","name":"李四","age":30}"#).unwrap();
    let mut u3 = Document::from_json(r#"{"_id":"u3","name":"王五","age":10}"#).unwrap();
    let mut o1 = Document::from_json(r#"{"_id":"o1","amount":100}"#).unwrap();
    let mut o2 = Document::from_json(r#"{"_id":"o2","amount":200}"#).unwrap();
    db.insert("users", &mut u1).expect("插入 u1 应成功");
    db.insert("users", &mut u2).expect("插入 u2 应成功");
    db.insert("users", &mut u3).expect("插入 u3 应成功");
    db.insert("orders", &mut o1).expect("插入 o1 应成功");
    db.insert("orders", &mut o2).expect("插入 o2 应成功");
    db
}

// ──────────────── count ────────────────

/// count 缺省 filter(null)统计集合内全部文档条数。
#[test]
fn count_all() {
    let path = temp_nodb("count_all");
    let db = setup(&path);

    assert_eq!(db.count("users", &filter("null")).unwrap(), 3);
    assert_eq!(db.count("orders", &filter("null")).unwrap(), 2);

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// count 带 filter:只统计满足条件的条数。
#[test]
fn count_with_filter() {
    let path = temp_nodb("count_filter");
    let db = setup(&path);

    assert_eq!(
        db.count("users", &filter(r#"{"age":{"$gte":18}}"#))
            .unwrap(),
        2,
        "age>=18 的应为 2 条"
    );
    assert_eq!(
        db.count("users", &filter(r#"{"name":{"$regex":"^张"}}"#))
            .unwrap(),
        1,
        "姓张的应为 1 条"
    );
    assert_eq!(
        db.count("users", &filter(r#"{"age":{"$lt":10}}"#)).unwrap(),
        0,
        "无匹配应为 0"
    );

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// count 空集合:返回 0。
#[test]
fn count_empty_collection() {
    let path = temp_nodb("count_empty");
    let db = Database::create(&path).expect("创建数据库应成功");

    assert_eq!(db.count("ghost", &filter("null")).unwrap(), 0);

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// count 与 find 结果一致:多条件 + 点号路径。
#[test]
fn count_matches_find() {
    let path = temp_nodb("count_find");
    let db = Database::create(&path).expect("创建数据库应成功");
    let mut a = Document::from_json(r#"{"_id":"a","addr":{"city":"北京"},"age":30}"#).unwrap();
    let mut b = Document::from_json(r#"{"_id":"b","addr":{"city":"上海"},"age":25}"#).unwrap();
    let mut c = Document::from_json(r#"{"_id":"c","addr":{"city":"北京"},"age":20}"#).unwrap();
    db.insert("users", &mut a).expect("插入 a 应成功");
    db.insert("users", &mut b).expect("插入 b 应成功");
    db.insert("users", &mut c).expect("插入 c 应成功");

    let f = filter(r#"{"addr.city":"北京","age":{"$gte":20}}"#);
    let n = db.count("users", &f).unwrap();
    let docs = db.find("users", &f).unwrap();
    assert_eq!(n, docs.len(), "count 应与 find 结果一致");
    assert_eq!(n, 2, "北京且 age>=20 应为 2 条");

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

// ──────────────── collections ────────────────

/// 列出集合名:去重、有序。
#[test]
fn collections_lists_names() {
    let path = temp_nodb("collections_list");
    let db = setup(&path);

    let names = db.collections().expect("列出集合应成功");
    assert_eq!(
        names,
        vec!["orders".to_string(), "users".to_string()],
        "应按集合名有序且去重"
    );

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 空库 collections:返回空列表,不报错。
#[test]
fn collections_empty_db() {
    let path = temp_nodb("collections_empty");
    let db = Database::create(&path).expect("创建数据库应成功");

    let names = db.collections().expect("空库列出集合应成功");
    assert!(names.is_empty(), "空库应返回空列表");

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 单集合下多条文档:集合名只出现一次。
#[test]
fn collections_no_duplicates() {
    let path = temp_nodb("collections_dup");
    let db = Database::create(&path).expect("创建数据库应成功");
    for i in 0..5 {
        let mut d = Document::from_json(&format!(r#"{{"_id":"d{}","i":{}}}"#, i, i)).unwrap();
        db.insert("solo", &mut d).expect("插入应成功");
    }

    let names = db.collections().expect("列出集合应成功");
    assert_eq!(names, vec!["solo".to_string()], "集合名应去重");

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

// ──────────────── drop_collection ────────────────

/// drop 删除整个集合:返回删除条数,集合名从列表中消失。
#[test]
fn drop_removes_collection() {
    let path = temp_nodb("drop_removes");
    let db = setup(&path);

    let n = db.drop_collection("users").expect("删除集合应成功");
    assert_eq!(n, 3, "users 应有 3 条文档被删除");

    // 集合名消失
    let names = db.collections().expect("列出集合应成功");
    assert_eq!(names, vec!["orders".to_string()]);

    // 原集合查询为空
    assert_eq!(db.count("users", &filter("null")).unwrap(), 0);

    // 其他集合不受影响
    assert_eq!(db.count("orders", &filter("null")).unwrap(), 2);

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// drop 不存在的集合:返回 0,不报错。
#[test]
fn drop_missing_collection_returns_zero() {
    let path = temp_nodb("drop_missing");
    let db = setup(&path);

    let n = db.drop_collection("ghost").expect("删除不存在集合应成功");
    assert_eq!(n, 0);

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// drop 后同主键可重新插入(集合真正清空,而非标记删除)。
#[test]
fn drop_then_reinsert() {
    let path = temp_nodb("drop_reinsert");
    let db = Database::create(&path).expect("创建数据库应成功");
    let mut a = Document::from_json(r#"{"_id":"k","v":1}"#).unwrap();
    db.insert("t", &mut a).expect("插入应成功");

    db.drop_collection("t").expect("删除集合应成功");

    let mut b = Document::from_json(r#"{"_id":"k","v":2}"#).unwrap();
    let id = db.insert("t", &mut b).expect("drop 后同主键再插入应成功");
    assert_eq!(id, "k");
    assert_eq!(db.count("t", &filter("null")).unwrap(), 1);

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// drop 持久化:重新打开数据库后删除仍生效。
#[test]
fn drop_persists_across_reopen() {
    let path = temp_nodb("drop_persist");
    let db = setup(&path);
    db.drop_collection("users").expect("删除集合应成功");
    db.close().expect("关闭应成功");
    drop(db); // Windows 下 redb 持有独占锁,必须先释放句柄

    let db = Database::open(&path).expect("重新打开应成功");
    let names = db.collections().expect("列出集合应成功");
    assert_eq!(
        names,
        vec!["orders".to_string()],
        "重新打开后 drop 应仍生效"
    );
    assert_eq!(db.count("users", &filter("null")).unwrap(), 0);

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}
