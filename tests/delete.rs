//! delete 功能测试(M4)。
//!
//! 覆盖 `Database::delete` 的按 filter 删除:精确匹配、条件删除、无匹配、
//! 删除后集合隔离、删除后插入同主键成功、持久化与返回条数。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use nodblite::query::{Filter, Update};
use nodblite::{Database, Document};

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

/// 辅助:创建数据库并插入两条文档。
fn setup(path: &Path) -> Database {
    let db = Database::create(path).expect("创建数据库应成功");
    let mut a = Document::from_json(r#"{"_id":"u1","name":"张三","age":20}"#).unwrap();
    let mut b = Document::from_json(r#"{"_id":"u2","name":"李四","age":30}"#).unwrap();
    let mut c = Document::from_json(r#"{"_id":"u3","name":"王五","age":10}"#).unwrap();
    db.insert("users", &mut a).expect("插入 u1 应成功");
    db.insert("users", &mut b).expect("插入 u2 应成功");
    db.insert("users", &mut c).expect("插入 u3 应成功");
    db
}

/// 辅助:解析 filter。
fn filter(json: &str) -> Filter {
    let v: serde_json::Value = serde_json::from_str(json).expect("测试 JSON 应合法");
    Filter::from_value(&v).expect("filter 解析应成功")
}

/// 辅助:查询全部文档并返回 _id 列表(已排序)。
fn ids(db: &Database) -> Vec<String> {
    let mut docs = db
        .find(
            "users",
            &Filter::from_value(&serde_json::Value::Null).unwrap(),
        )
        .expect("查询应成功");
    docs.sort_by(|a, b| a["_id"].as_str().cmp(&b["_id"].as_str()));
    docs.into_iter()
        .map(|d| d["_id"].as_str().unwrap().to_string())
        .collect()
}

/// 按 _id 精确删除一条文档,返回删除条数。
#[test]
fn delete_by_id() {
    let path = temp_nodb("del_id");
    let db = setup(&path);

    let n = db
        .delete("users", &filter(r#"{"_id":"u2"}"#))
        .expect("删除应成功");
    assert_eq!(n, 1, "应删除 1 条");
    assert_eq!(ids(&db), vec!["u1".to_string(), "u3".to_string()]);

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 按条件删除:所有 age < 18 的文档被删除。
#[test]
fn delete_by_condition() {
    let path = temp_nodb("del_cond");
    let db = setup(&path);

    let n = db
        .delete("users", &filter(r#"{"age":{"$lt":18}}"#))
        .expect("删除应成功");
    assert_eq!(n, 1, "只有 u3(age=10)应被删除");
    assert_eq!(ids(&db), vec!["u1".to_string(), "u2".to_string()]);

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 删除全部匹配:filter 匹配所有文档。
#[test]
fn delete_all() {
    let path = temp_nodb("del_all");
    let db = setup(&path);

    let n = db
        .delete("users", &filter(r#"{"age":{"$gte":0}}"#))
        .expect("删除应成功");
    assert_eq!(n, 3, "应删除全部 3 条");
    assert!(ids(&db).is_empty(), "删除后集合应为空");

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// filter 无匹配:返回 0,文档不变。
#[test]
fn delete_no_match() {
    let path = temp_nodb("del_nomatch");
    let db = setup(&path);

    let n = db
        .delete("users", &filter(r#"{"_id":"nobody"}"#))
        .expect("删除应成功");
    assert_eq!(n, 0, "无匹配应返回 0");
    assert_eq!(ids(&db).len(), 3, "文档数量不应变化");

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 删除后主键可复用:插入同 _id 文档应成功。
#[test]
fn delete_then_reinsert_same_id() {
    let path = temp_nodb("del_reinsert");
    let db = setup(&path);

    db.delete("users", &filter(r#"{"_id":"u1"}"#))
        .expect("删除应成功");
    let mut new_doc = Document::from_json(r#"{"_id":"u1","name":"新用户"}"#).unwrap();
    let id = db
        .insert("users", &mut new_doc)
        .expect("同主键再插入应成功");
    assert_eq!(id, "u1");

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 集合隔离:删除 users 不影响 orders。
#[test]
fn delete_collection_isolation() {
    let path = temp_nodb("del_iso");
    let db = Database::create(&path).expect("创建数据库应成功");
    let mut a = Document::from_json(r#"{"_id":"x","c":"users"}"#).unwrap();
    let mut b = Document::from_json(r#"{"_id":"x","c":"orders"}"#).unwrap();
    db.insert("users", &mut a).expect("users 插入应成功");
    db.insert("orders", &mut b).expect("orders 插入应成功");

    db.delete("users", &filter(r#"{"_id":"x"}"#))
        .expect("删除应成功");

    let orders = db
        .find("orders", &filter(r#"{"_id":"x"}"#))
        .expect("查询应成功");
    assert_eq!(orders.len(), 1, "orders 集合不应受影响");

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 空集合删除:返回 0,不报错。
#[test]
fn delete_from_empty_collection() {
    let path = temp_nodb("del_empty");
    let db = Database::create(&path).expect("创建数据库应成功");

    let n = db
        .delete("empty", &filter(r#"{"_id":"x"}"#))
        .expect("空集合删除应成功");
    assert_eq!(n, 0);

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 删除后持久化:重新打开数据库,删除结果仍生效。
#[test]
fn delete_persists_across_reopen() {
    let path = temp_nodb("del_persist");
    let db = setup(&path);
    db.delete("users", &filter(r#"{"_id":"u2"}"#))
        .expect("删除应成功");
    db.close().expect("关闭应成功");
    drop(db); // Windows 下 redb 持有独占锁,必须先释放句柄

    let db = Database::open(&path).expect("重新打开应成功");
    assert_eq!(
        ids(&db),
        vec!["u1".to_string(), "u3".to_string()],
        "重新打开后删除应仍生效"
    );

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 与 update 组合:先删除部分文档再查询。
#[test]
fn delete_after_update_flow() {
    let path = temp_nodb("del_flow");
    let db = setup(&path);

    // 先把 u3 的 age 从 10 更新为 15
    let up = serde_json::from_str::<serde_json::Value>(r#"{"$set":{"age":15}}"#).unwrap();
    let upd = Update::from_value(&up).unwrap();
    db.update("users", &filter(r#"{"_id":"u3"}"#), &upd)
        .expect("更新应成功");

    // 再删除 age < 18 的文档
    let n = db
        .delete("users", &filter(r#"{"age":{"$lt":18}}"#))
        .expect("删除应成功");
    assert_eq!(n, 1, "u3 更新后 age=15 仍 <18,应被删除");
    assert_eq!(ids(&db), vec!["u1".to_string(), "u2".to_string()]);

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}
