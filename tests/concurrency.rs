//! 并发访问测试(M9 压测基线)。
//!
//! 目标:确认 nosqlite 可作为多线程 HTTP 服务的存储层 ——
//! `Database` 能跨线程共享(`Send + Sync`),并发读写不丢数据、不返回错误,
//! 主键冲突在竞争下仍能被正确检测。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;

use nosqlite::engine::StorageError;
use nosqlite::query::{Filter, Update};
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

/// 辅助:匹配全部文档的 filter。
fn all_filter() -> Filter {
    Filter::from_value(&serde_json::Value::Null).expect("空 filter 应解析成功")
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

/// 编译期断言:`Database` 必须同时满足 `Send` 与 `Sync`(否则无法用 `Arc` 跨线程共享)。
#[test]
fn database_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Database>();
}

/// 多线程向同一集合并发插入:全部落盘,条数精确。
#[test]
fn concurrent_inserts_same_collection() {
    const THREADS: usize = 8;
    const PER_THREAD: usize = 100;

    let path = temp_nodb("conc_insert");
    let db = Arc::new(Database::create(&path).expect("创建数据库应成功"));
    let barrier = Arc::new(Barrier::new(THREADS));

    let handles: Vec<_> = (0..THREADS)
        .map(|t| {
            let db = Arc::clone(&db);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                // 让所有线程尽可能同时开始,制造写入竞争
                barrier.wait();
                for i in 0..PER_THREAD {
                    let json = format!(r#"{{"_id":"t{t}-{i}","value":{i}}}"#);
                    let mut doc = Document::from_json(&json).expect("文档应合法");
                    db.insert("items", &mut doc).expect("并发插入不应失败");
                }
            })
        })
        .collect();

    for h in handles {
        h.join().expect("线程不应 panic");
    }

    let total = db
        .count("items", &all_filter())
        .expect("计数应成功");
    assert_eq!(total, THREADS * PER_THREAD, "并发插入不得丢数据");

    // 抽查每个线程的首尾文档确实存在
    for t in 0..THREADS {
        for i in [0, PER_THREAD - 1] {
            let docs = db
                .find("items", &filter(&format!(r#"{{"_id":"t{t}-{i}"}}"#)))
                .expect("查询应成功");
            assert_eq!(docs.len(), 1, "文档 t{}-{} 应存在", t, i);
        }
    }

    drop(db);
    cleanup(&path);
}

/// 多线程写各自独立的集合:集合之间互不干扰。
#[test]
fn concurrent_inserts_distinct_collections() {
    const THREADS: usize = 8;
    const PER_THREAD: usize = 50;

    let path = temp_nodb("conc_collections");
    let db = Arc::new(Database::create(&path).expect("创建数据库应成功"));
    let barrier = Arc::new(Barrier::new(THREADS));

    let handles: Vec<_> = (0..THREADS)
        .map(|t| {
            let db = Arc::clone(&db);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                let collection = format!("col_{t}");
                for i in 0..PER_THREAD {
                    let json = format!(r#"{{"_id":"d{i}","t":{t}}}"#);
                    let mut doc = Document::from_json(&json).expect("文档应合法");
                    db.insert(&collection, &mut doc).expect("插入不应失败");
                }
            })
        })
        .collect();

    for h in handles {
        h.join().expect("线程不应 panic");
    }

    let collections = db.collections().expect("列出集合应成功");
    assert_eq!(collections.len(), THREADS, "应恰好有 {} 个集合", THREADS);
    for t in 0..THREADS {
        let n = db
            .count(&format!("col_{t}"), &all_filter())
            .expect("计数应成功");
        assert_eq!(n, PER_THREAD, "集合 col_{} 条数应正确", t);
    }

    drop(db);
    cleanup(&path);
}

/// 竞争下插入相同 `_id`:恰好一个成功,其余全部报主键冲突。
#[test]
fn concurrent_duplicate_id_detected() {
    const THREADS: usize = 8;

    let path = temp_nodb("conc_dup");
    let db = Arc::new(Database::create(&path).expect("创建数据库应成功"));
    let barrier = Arc::new(Barrier::new(THREADS));

    let handles: Vec<_> = (0..THREADS)
        .map(|_| {
            let db = Arc::clone(&db);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                let mut doc =
                    Document::from_json(r#"{"_id":"same","value":1}"#).expect("文档应合法");
                match db.insert("items", &mut doc) {
                    Ok(_) => true,
                    Err(DatabaseError::Storage(StorageError::DuplicateKey { .. })) => false,
                    Err(other) => panic!("应报主键冲突,实际: {:?}", other),
                }
            })
        })
        .collect();

    let successes = handles
        .into_iter()
        .map(|h| h.join().expect("线程不应 panic"))
        .filter(|ok| *ok)
        .count();

    assert_eq!(successes, 1, "竞争下应恰好一个线程插入成功");
    assert_eq!(
        db.count("items", &all_filter()).expect("计数应成功"),
        1,
        "集合内应只有一条文档"
    );

    drop(db);
    cleanup(&path);
}

/// 边写边读:读线程不报错,且条数单调不减。
#[test]
fn concurrent_reads_during_writes() {
    const READERS: usize = 4;
    const WRITES: usize = 200;

    let path = temp_nodb("conc_rw");
    let db = Arc::new(Database::create(&path).expect("创建数据库应成功"));
    let done = Arc::new(AtomicBool::new(false));

    let reader_handles: Vec<_> = (0..READERS)
        .map(|_| {
            let db = Arc::clone(&db);
            let done = Arc::clone(&done);
            thread::spawn(move || {
                let mut last = 0usize;
                while !done.load(Ordering::SeqCst) {
                    let n = db.count("items", &all_filter()).expect("并发读不应失败");
                    assert!(n >= last, "计数应单调不减: {} -> {}", last, n);
                    last = n;
                    // 顺带验证带排序/分页的读路径在并发下也稳定
                    db.find("items", &all_filter()).expect("并发查询不应失败");
                }
                last
            })
        })
        .collect();

    for i in 0..WRITES {
        let json = format!(r#"{{"_id":"w{i}","value":{i}}}"#);
        let mut doc = Document::from_json(&json).expect("文档应合法");
        db.insert("items", &mut doc).expect("写入不应失败");
    }

    done.store(true, Ordering::SeqCst);
    for h in reader_handles {
        h.join().expect("读线程不应 panic");
    }

    assert_eq!(
        db.count("items", &all_filter()).expect("计数应成功"),
        WRITES,
        "写入完成后的条数应精确"
    );

    drop(db);
    cleanup(&path);
}

/// 多线程更新各自负责的文档:全部生效,修改条数之和精确。
#[test]
fn concurrent_updates_distinct_docs() {
    const THREADS: usize = 8;
    const PER_THREAD: usize = 50;

    let path = temp_nodb("conc_update");
    let db = Arc::new(Database::create(&path).expect("创建数据库应成功"));

    // 预置数据
    for t in 0..THREADS {
        for i in 0..PER_THREAD {
            let json = format!(r#"{{"_id":"t{t}-{i}","done":false}}"#);
            let mut doc = Document::from_json(&json).expect("文档应合法");
            db.insert("tasks", &mut doc).expect("预置插入应成功");
        }
    }

    let barrier = Arc::new(Barrier::new(THREADS));
    let handles: Vec<_> = (0..THREADS)
        .map(|t| {
            let db = Arc::clone(&db);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                let mut modified = 0usize;
                for i in 0..PER_THREAD {
                    // 每个线程只更新自己负责的文档,避免相互覆盖
                    modified += db
                        .update(
                            "tasks",
                            &filter(&format!(r#"{{"_id":"t{t}-{i}"}}"#)),
                            &update(r#"{"$set":{"done":true}}"#),
                        )
                        .expect("并发更新不应失败");
                }
                modified
            })
        })
        .collect();

    let total_modified: usize = handles
        .into_iter()
        .map(|h| h.join().expect("线程不应 panic"))
        .sum();
    assert_eq!(
        total_modified,
        THREADS * PER_THREAD,
        "所有文档都应被更新一次"
    );

    let remaining = db
        .count("tasks", &filter(r#"{"done":false}"#))
        .expect("计数应成功");
    assert_eq!(remaining, 0, "不应有未完成的文档");

    drop(db);
    cleanup(&path);
}
