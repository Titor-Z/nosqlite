//! insert 功能测试(M2)。
//!
//! 覆盖 `Database::insert` 的文档插入、自动/显式主键、主键冲突、
//! 集合隔离与持久化(重新打开后数据仍在)。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

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

/// 插入无 `_id` 文档:自动生成 24 字符 ObjectId 并回填。
#[test]
fn insert_auto_generates_id() {
    let path = temp_nodb("auto_id");
    let db = Database::create(&path).expect("创建数据库应成功");

    let mut doc = Document::from_json(r#"{"name":"张三","age":30}"#).unwrap();
    let id = db.insert("users", &mut doc).expect("插入应成功");
    assert_eq!(id.len(), 24, "自动生成的 _id 应为 24 字符");
    assert_eq!(doc.get_id().unwrap(), Some(id), "_id 应回填到文档");

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 插入带显式 `_id` 的文档:使用用户提供的 ID。
#[test]
fn insert_uses_explicit_id() {
    let path = temp_nodb("explicit_id");
    let db = Database::create(&path).expect("创建数据库应成功");

    let mut doc = Document::from_json(r#"{"_id":"user_001","name":"李四"}"#).unwrap();
    let id = db.insert("users", &mut doc).expect("插入应成功");
    assert_eq!(id, "user_001");

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 重复 `_id` 插入应报主键冲突错误。
#[test]
fn insert_duplicate_id_fails() {
    let path = temp_nodb("dup_id");
    let db = Database::create(&path).expect("创建数据库应成功");

    let mut doc1 = Document::from_json(r#"{"_id":"same_id","v":1}"#).unwrap();
    db.insert("users", &mut doc1).expect("首次插入应成功");

    let mut doc2 = Document::from_json(r#"{"_id":"same_id","v":2}"#).unwrap();
    let err = db.insert("users", &mut doc2).expect_err("重复主键应失败");
    match err {
        DatabaseError::Storage(nosqlite::engine::StorageError::DuplicateKey { collection, id }) => {
            assert_eq!(collection, "users");
            assert_eq!(id, "same_id");
        }
        other => panic!("应为主键冲突,实际: {:?}", other),
    }

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 不同集合使用相同 `_id` 互不冲突(集合间隔离)。
#[test]
fn same_id_in_different_collections_ok() {
    let path = temp_nodb("iso");
    let db = Database::create(&path).expect("创建数据库应成功");

    let mut a = Document::from_json(r#"{"_id":"x","from":"users"}"#).unwrap();
    db.insert("users", &mut a).expect("users 插入应成功");
    let mut b = Document::from_json(r#"{"_id":"x","from":"orders"}"#).unwrap();
    db.insert("orders", &mut b).expect("orders 插入应成功");

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 空集合名应被拒绝。
#[test]
fn insert_rejects_empty_collection() {
    let path = temp_nodb("empty_col");
    let db = Database::create(&path).expect("创建数据库应成功");

    let mut doc = Document::from_json(r#"{"_id":"a"}"#).unwrap();
    let err = db.insert("", &mut doc).expect_err("空集合名应失败");
    assert!(
        matches!(
            err,
            DatabaseError::Storage(nosqlite::engine::StorageError::InvalidCollection(_))
        ),
        "应报 InvalidCollection"
    );

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 插入后重新打开数据库,重复插入同一主键仍应冲突 → 证明数据已持久化。
#[test]
fn insert_persists_across_reopen() {
    let path = temp_nodb("persist");
    let db = Database::create(&path).expect("创建数据库应成功");
    let mut doc = Document::from_json(r#"{"_id":"p1","v":1}"#).unwrap();
    db.insert("users", &mut doc).expect("插入应成功");
    db.close().expect("关闭应成功");
    drop(db); // Windows 下 redb 持有独占锁,必须先释放句柄

    let db = Database::open(&path).expect("重新打开应成功");
    let mut dup = Document::from_json(r#"{"_id":"p1","v":2}"#).unwrap();
    let err = db.insert("users", &mut dup).expect_err("重新打开后重复主键应仍冲突");
    assert!(
        matches!(
            err,
            DatabaseError::Storage(nosqlite::engine::StorageError::DuplicateKey { .. })
        ),
        "持久化后应仍能检测到主键冲突"
    );

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}
