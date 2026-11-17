//! 文件压缩与空间统计(v0.7.1)。
//!
//! 实测事实(本模块即回归护栏):
//! - 删除文档**不会**让 `.nodb` 变小(空闲页只被后续写入复用);
//! - 反复更新不会让文件增长(页复用有效);
//! - 「删光 → 再插入」会先翻倍再稳定(COW 分代);
//! - 只有显式 `.compact` 才会把空闲页还给文件系统。

use std::path::PathBuf;

use nodblite::query::Filter;
use nodblite::{Database, Document};
use serde_json::json;

fn temp_nodb(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "nodblite_compact_{}_{}_{}.nodb",
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

fn docs(n: usize) -> Vec<Document> {
    (0..n)
        .map(|i| {
            Document::from_value(json!({"n": i, "payload": "x".repeat(200)})).unwrap()
        })
        .collect()
}

fn size(path: &std::path::Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// 删除不会缩小文件(这是 `.compact` 存在的前提)。
#[test]
fn delete_does_not_shrink_file() {
    let path = temp_nodb("delete");
    let db = Database::create(&path).unwrap();
    let mut batch = docs(2000);
    db.insert_many("blob", &mut batch).unwrap();
    let full = size(&path);

    assert_eq!(db.delete("blob", &Filter::match_all()).unwrap(), 2000);
    assert_eq!(size(&path), full, "删除后文件大小应保持不变");

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 压缩后文件确实变小(核心断言)。
#[test]
fn compact_shrinks_file() {
    let path = temp_nodb("compact");
    let mut db = Database::create(&path).unwrap();
    let mut batch = docs(4000);
    db.insert_many("blob", &mut batch).unwrap();
    let full = size(&path);

    db.delete("blob", &Filter::match_all()).unwrap();
    // 只留 20 条
    let mut few = docs(20);
    db.insert_many("blob", &mut few).unwrap();
    let before = size(&path);
    assert_eq!(before, full, "删除本身不缩文件");

    db.compact().expect("压缩应成功");
    let after = size(&path);
    assert!(
        after < before / 2,
        "压缩后应显著变小: before={} after={}",
        before,
        after
    );

    // 数据仍然完好
    assert_eq!(db.count("blob", &Filter::match_all()).unwrap(), 20);

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 压缩是幂等的:再压一次不会报错,也不会丢数据。
#[test]
fn compact_is_idempotent_and_preserves_data() {
    let path = temp_nodb("idempotent");
    let mut db = Database::create(&path).unwrap();
    let mut batch = docs(500);
    db.insert_many("blob", &mut batch).unwrap();
    db.delete("blob", &Filter::match_all()).unwrap();
    let mut keep = docs(10);
    db.insert_many("blob", &mut keep).unwrap();

    db.compact().unwrap();
    let once = size(&path);
    db.compact().unwrap();
    assert!(size(&path) <= once, "再次压缩不应变大");

    assert_eq!(db.count("blob", &Filter::match_all()).unwrap(), 10);
    drop(db);

    // 重开后数据仍在(压缩不破坏格式)
    let db = Database::open(&path).unwrap();
    assert_eq!(db.count("blob", &Filter::match_all()).unwrap(), 10);
    drop(db);
    let _ = std::fs::remove_file(path);
}
