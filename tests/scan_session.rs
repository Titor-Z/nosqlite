//! 流式扫描会话(M22)。
//!
//! 验证:
//! 1. 分批推进能取全量,且**不重复、不遗漏、顺序稳定**;
//! 2. 会话跨多次调用持续有效(持有读事务),这是索引扫描与真流式游标的前提;
//! 3. 空集合/不存在的集合返回空批次,不报错;
//! 4. 与 `find` 的结果集完全一致(同一套过滤语义)。

use std::path::PathBuf;

use nodblite::query::Filter;
use nodblite::{Database, Document};
use serde_json::json;

/// 生成本次测试独占的临时 `.nodb` 路径。
fn temp_nodb(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "nodblite_scan_{}_{}_{}.nodb",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&path);
    path
}

/// 建库并插入 `n` 条 `{"n": i}`。
fn setup(tag: &str, n: i64) -> (PathBuf, Database) {
    let path = temp_nodb(tag);
    let db = Database::create(&path).unwrap();
    let mut docs: Vec<Document> = (0..n)
        .map(|i| Document::from_json(&format!(r#"{{"n": {}}}"#, i)).unwrap())
        .collect();
    db.insert_many("items", &mut docs).unwrap();
    (path, db)
}

/// 分批能取全量,且没有重复/遗漏。
#[test]
fn batches_cover_everything_exactly_once() {
    let (path, db) = setup("cover", 100);
    let mut cur = db.scan("items").unwrap();

    let mut ids: Vec<String> = Vec::new();
    loop {
        let batch = cur.next_batch(7).unwrap();
        if batch.is_empty() {
            break;
        }
        for (id, _) in batch {
            ids.push(id);
        }
    }

    assert_eq!(ids.len(), 100, "应取到全部 100 条");
    // 无重复
    let mut sorted = ids.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), 100, "不应有重复");
    // 与 find 的主键集合一致
    let mut from_find: Vec<String> = db
        .find("items", &Filter::match_all())
        .unwrap()
        .iter()
        .map(|d| d["_id"].as_str().unwrap().to_string())
        .collect();
    from_find.sort();
    assert_eq!(sorted, from_find);
    drop(db);
    let _ = std::fs::remove_file(path);
}

/// `next_docs` 与 `find` 返回的文档内容一致(含字段序)。
#[test]
fn next_docs_matches_find() {
    let path = temp_nodb("docs");
    let db = Database::create(&path).unwrap();
    let mut docs: Vec<Document> = vec![
        Document::from_json(r#"{"zebra": 1, "apple": 2}"#).unwrap(),
        Document::from_json(r#"{"zebra": 3, "apple": 4}"#).unwrap(),
    ];
    db.insert_many("c", &mut docs).unwrap();

    let mut cur = db.scan("c").unwrap();
    let from_scan = cur.next_docs(10).unwrap();
    let from_find = db.find("c", &Filter::match_all()).unwrap();
    assert_eq!(from_scan, from_find, "两条路径应产出相同文档");

    // 字段序保持
    let keys: Vec<&String> = from_scan[0].keys().collect();
    assert_eq!(keys, vec!["zebra", "apple", "_id"]);

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 耗尽后继续调用仍返回空(幂等),不 panic。
#[test]
fn exhausted_is_idempotent() {
    let (path, db) = setup("idem", 5);
    let mut cur = db.scan("items").unwrap();

    // 一次取完
    let all = cur.next_batch(100).unwrap();
    assert_eq!(all.len(), 5);

    // 后续调用一律返回空
    for _ in 0..3 {
        assert!(cur.next_batch(100).unwrap().is_empty());
    }

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 空集合 / 不存在的集合:返回空批次,不报错。
#[test]
fn empty_collection_yields_nothing() {
    let path = temp_nodb("empty");
    let db = Database::create(&path).unwrap();

    let mut cur = db.scan("nothing").unwrap();
    assert!(cur.next_batch(10).unwrap().is_empty());

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// `max = 0` 不推进(返回空但不标记耗尽),避免调用方误以为结束。
#[test]
fn zero_batch_does_not_advance() {
    let (path, db) = setup("zero", 3);
    let mut cur = db.scan("items").unwrap();

    assert!(cur.next_batch(0).unwrap().is_empty());
    // 仍能取到全部数据
    let all = cur.next_batch(10).unwrap();
    assert_eq!(all.len(), 3);

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 集合隔离:扫描 A 不会读到 B 的文档。
#[test]
fn collections_are_isolated() {
    let path = temp_nodb("iso");
    let db = Database::create(&path).unwrap();
    let mut a: Vec<Document> = (0..3)
        .map(|i| Document::from_json(&format!(r#"{{"n": {}}}"#, i)).unwrap())
        .collect();
    let mut b: Vec<Document> = (0..7)
        .map(|i| Document::from_json(&format!(r#"{{"n": {}}}"#, i)).unwrap())
        .collect();
    db.insert_many("a", &mut a).unwrap();
    db.insert_many("b", &mut b).unwrap();

    let mut cur = db.scan("a").unwrap();
    assert_eq!(cur.next_batch(100).unwrap().len(), 3);

    let mut cur = db.scan("b").unwrap();
    assert_eq!(cur.next_batch(100).unwrap().len(), 7);

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 会话持有读事务:会话期间写入不影响已打开的会话快照(MVCC)。
#[test]
fn session_sees_consistent_snapshot() {
    let (path, db) = setup("snapshot", 10);
    let mut cur = db.scan("items").unwrap();

    // 先取一批,确认会话已建立
    let first = cur.next_batch(4).unwrap();
    assert_eq!(first.len(), 4);

    // 会话期间插入新文档
    let mut extra = Document::from_json(r#"{"n": 999}"#).unwrap();
    db.insert("items", &mut extra).unwrap();

    // 继续读完:总数应仍是原来的 10(快照隔离),不包含新插入的
    let mut seen = first.len();
    loop {
        let batch = cur.next_batch(100).unwrap();
        if batch.is_empty() {
            break;
        }
        seen += batch.len();
    }
    assert_eq!(seen, 10, "会话应看到开启时的快照,实际 {}", seen);

    // 新开一个会话则能看到 11 条
    let mut fresh = db.scan("items").unwrap();
    let mut total = 0;
    loop {
        let batch = fresh.next_batch(100).unwrap();
        if batch.is_empty() {
            break;
        }
        total += batch.len();
    }
    assert_eq!(total, 11, "新会话应看到最新数据");

    drop(fresh);
    drop(cur);
    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 大集合分批读取:验证不会一次性物化(用极小批次模拟)。
#[test]
fn many_small_batches() {
    let (path, db) = setup("many", 500);
    let mut cur = db.scan("items").unwrap();

    let mut count = 0;
    let mut batches = 0;
    loop {
        let batch = cur.next_batch(13).unwrap();
        if batch.is_empty() {
            break;
        }
        count += batch.len();
        batches += 1;
        // 批次上限不被突破
        assert!(batch.len() <= 13);
    }
    assert_eq!(count, 500);
    assert_eq!(batches, 500usize.div_ceil(13), "批次数应正确");

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 扫描结果的顺序与主键序一致(B+树 key 序)。
#[test]
fn order_follows_primary_key() {
    let (path, db) = setup("order", 20);
    let mut cur = db.scan("items").unwrap();
    let mut ids = Vec::new();
    loop {
        let batch = cur.next_batch(6).unwrap();
        if batch.is_empty() {
            break;
        }
        ids.extend(batch.into_iter().map(|(id, _)| id));
    }

    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted, "扫描应按主键升序返回");

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 文档内容可正常反序列化(不被分批截断)。
#[test]
fn documents_are_not_truncated() {
    let path = temp_nodb("whole");
    let db = Database::create(&path).unwrap();
    let payload = "x".repeat(8000);
    let mut doc = Document::from_json(&json!({"big": payload}).to_string()).unwrap();
    db.insert("c", &mut doc).unwrap();

    let mut cur = db.scan("c").unwrap();
    let docs = cur.next_docs(10).unwrap();
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0]["big"].as_str().unwrap().len(), 8000);

    drop(db);
    let _ = std::fs::remove_file(path);
}
