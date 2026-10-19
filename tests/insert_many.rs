//! 单事务批量插入测试(M11)。
//!
//! 覆盖 `Database::insert_many`:
//! - 全部文档在一个写事务内写入并一次性提交;
//! - 自动生成 / 沿用 `_id` 并与入参顺序一致;
//! - 任一主键冲突则**整批失败**,不留下部分写入(原子性);
//! - 空批次、单条批次、大批次的边界行为。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use nodblite::engine::StorageError;
use nodblite::query::Filter;
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

/// 辅助:构造显式 `_id` 的文档。
fn doc_with_id(id: &str, v: i64) -> Document {
    Document::from_json(&format!(r#"{{"_id":"{id}","v":{v}}}"#)).expect("文档应合法")
}

/// 辅助:构造不带 `_id` 的文档(应被自动补全)。
fn doc_without_id(v: i64) -> Document {
    Document::from_json(&format!(r#"{{"v":{v}}}"#)).expect("文档应合法")
}

/// 批量插入显式主键:返回的 id 顺序与入参一致,数据全部落盘。
#[test]
fn insert_many_explicit_ids() {
    let path = temp_nodb("many_explicit");
    let db = Database::create(&path).expect("创建数据库应成功");

    let mut docs: Vec<Document> = (0..100)
        .map(|i| doc_with_id(&format!("k{i:03}"), i))
        .collect();
    let ids = db.insert_many("items", &mut docs).expect("批量插入应成功");

    assert_eq!(ids.len(), 100, "应返回 100 个主键");
    assert_eq!(ids[0], "k000", "主键顺序应与入参一致");
    assert_eq!(ids[99], "k099");
    assert_eq!(db.count("items", &filter("{}")).expect("计数应成功"), 100);

    // 抽查内容
    let doc = db.get_by_id("items", "k042").unwrap().unwrap();
    assert_eq!(doc["v"], 42);

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 自动生成主键:文档里被回填 `_id`,返回值与文档内一致。
#[test]
fn insert_many_auto_ids_backfilled() {
    let path = temp_nodb("many_auto");
    let db = Database::create(&path).expect("创建数据库应成功");

    let mut docs: Vec<Document> = (0..10).map(doc_without_id).collect();
    let ids = db.insert_many("items", &mut docs).expect("批量插入应成功");

    assert_eq!(ids.len(), 10);
    for (i, doc) in docs.iter().enumerate() {
        let embedded = doc.get_id().expect("应能取到主键").expect("主键应非空");
        assert_eq!(embedded, ids[i], "文档内回填的 _id 应与返回值一致");
        assert_eq!(embedded.len(), 24, "自动主键应为 24 字符 ObjectId");
    }

    // 全部可读回
    for id in &ids {
        assert!(
            db.get_by_id("items", id).unwrap().is_some(),
            "自动主键 {} 应可读回",
            id
        );
    }

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// **原子性**:批次内出现重复主键时整批失败,不留下部分写入。
#[test]
fn insert_many_duplicate_in_batch_is_atomic() {
    let path = temp_nodb("many_dup_in");
    let db = Database::create(&path).expect("创建数据库应成功");

    // 批次内部自相矛盾:k002 出现两次
    let mut docs = vec![
        doc_with_id("k001", 1),
        doc_with_id("k002", 2),
        doc_with_id("k002", 3),
        doc_with_id("k003", 4),
    ];
    let err = db
        .insert_many("items", &mut docs)
        .expect_err("批次内重复主键应失败");
    assert!(
        matches!(
            err,
            DatabaseError::Storage(StorageError::DuplicateKey { ref id, .. }) if id == "k002"
        ),
        "应报 k002 主键冲突,实际: {:?}",
        err
    );

    assert_eq!(
        db.count("items", &filter("{}")).expect("计数应成功"),
        0,
        "整批失败后集合必须为空(不得留下部分写入)"
    );

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// **原子性**:与已有文档冲突时整批失败。
#[test]
fn insert_many_duplicate_with_existing_is_atomic() {
    let path = temp_nodb("many_dup_exist");
    let db = Database::create(&path).expect("创建数据库应成功");

    let mut seed = doc_with_id("k002", 0);
    db.insert("items", &mut seed).expect("预置插入应成功");

    let mut docs = vec![
        doc_with_id("k001", 1),
        doc_with_id("k002", 2),
        doc_with_id("k003", 3),
    ];
    let err = db
        .insert_many("items", &mut docs)
        .expect_err("与已有文档冲突应失败");
    assert!(
        matches!(
            err,
            DatabaseError::Storage(StorageError::DuplicateKey { .. })
        ),
        "应报主键冲突,实际: {:?}",
        err
    );

    assert_eq!(
        db.count("items", &filter("{}")).expect("计数应成功"),
        1,
        "只应保留预置的那一条"
    );
    let doc = db.get_by_id("items", "k001").unwrap();
    assert!(doc.is_none(), "批次内的 k001 不应被写入");

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 空批次:返回空、不报错、不创建任何文档。
#[test]
fn insert_many_empty_batch() {
    let path = temp_nodb("many_empty");
    let db = Database::create(&path).expect("创建数据库应成功");

    let mut docs: Vec<Document> = Vec::new();
    let ids = db.insert_many("items", &mut docs).expect("空批次应成功");
    assert!(ids.is_empty());
    assert_eq!(db.count("items", &filter("{}")).expect("计数应成功"), 0);

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 单条批次与 `insert` 行为一致。
#[test]
fn insert_many_single_document() {
    let path = temp_nodb("many_single");
    let db = Database::create(&path).expect("创建数据库应成功");

    let mut docs = vec![doc_with_id("only", 1)];
    let ids = db.insert_many("items", &mut docs).expect("单条批次应成功");
    assert_eq!(ids, vec!["only".to_string()]);
    assert!(db.get_by_id("items", "only").unwrap().is_some());

    // 重复单条批次应报冲突
    let mut again = vec![doc_with_id("only", 2)];
    assert!(
        db.insert_many("items", &mut again).is_err(),
        "重复主键应失败"
    );

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 大批次(1,000 条)正确落盘且可持久化。
#[test]
fn insert_many_large_batch_persists() {
    let path = temp_nodb("many_large");
    let db = Database::create(&path).expect("创建数据库应成功");

    let mut docs: Vec<Document> = (0..1_000)
        .map(|i| doc_with_id(&format!("big{i:04}"), i))
        .collect();
    db.insert_many("items", &mut docs).expect("大批次应成功");
    assert_eq!(db.count("items", &filter("{}")).expect("计数应成功"), 1_000);

    db.close().expect("关闭应成功");
    drop(db); // redb 独占锁,必须先释放句柄

    let db = Database::open(&path).expect("重新打开应成功");
    assert_eq!(
        db.count("items", &filter("{}")).expect("计数应成功"),
        1_000,
        "重新打开后数据应完整"
    );
    assert_eq!(db.get_by_id("items", "big0999").unwrap().unwrap()["v"], 999);

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 多个批次可以连续写入同一集合。
#[test]
fn insert_many_multiple_batches() {
    let path = temp_nodb("many_multi");
    let db = Database::create(&path).expect("创建数据库应成功");

    for batch in 0..5 {
        let mut docs: Vec<Document> = (0..20)
            .map(|i| doc_with_id(&format!("b{batch}-{i:02}"), i))
            .collect();
        db.insert_many("items", &mut docs)
            .unwrap_or_else(|e| panic!("第 {} 批应成功: {:?}", batch, e));
    }

    assert_eq!(
        db.count("items", &filter("{}")).expect("计数应成功"),
        100,
        "5 批 × 20 条 = 100 条"
    );

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 批量写入后可用 `find_page` 与 `update_by_id` 正常操作。
#[test]
fn insert_many_then_query_and_update() {
    let path = temp_nodb("many_then_use");
    let db = Database::create(&path).expect("创建数据库应成功");

    let mut docs: Vec<Document> = (0..50)
        .map(|i| doc_with_id(&format!("q{i:02}"), i))
        .collect();
    db.insert_many("items", &mut docs).expect("批量插入应成功");

    // 分页
    let page = db
        .find_page("items", &filter("{}"), None, 10, Some(5))
        .expect("分页应成功");
    assert_eq!(page.len(), 5);

    // 主键更新
    let update = nodblite::query::Update::from_value(&serde_json::json!({"$set": {"done": true}}))
        .expect("更新应合法");
    assert!(
        db.update_by_id("items", "q25", &update)
            .expect("更新应成功"),
        "按主键更新应生效"
    );
    assert_eq!(db.get_by_id("items", "q25").unwrap().unwrap()["done"], true);

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}
