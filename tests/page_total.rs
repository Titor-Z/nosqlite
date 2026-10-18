//! `find_page_total`(0.2.0):一次扫描同时给出匹配总数与当页文档。
//!
//! 列表页需要「总数 + 当页」,旧玩法是 `count` + `find_page` 各扫一遍全集合
//! (500k 档约 10 s);本接口把两遍扫描合并为一遍。

use std::path::PathBuf;

use nosqlite::query::{Filter, SortSpec};
use nosqlite::{Database, Document};
use serde_json::{Map, Value, json};

/// 生成本次测试独占的临时 `.nodb` 路径。
fn temp_nodb(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "nosqlite_pt_{}_{}_{}.nodb",
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

/// 建库并批量插入 `n` 条 `{"n": i, "grp": i % 3}`。
fn setup(tag: &str, n: i64) -> (PathBuf, Database) {
    let path = temp_nodb(tag);
    let db = Database::create(&path).unwrap();
    let mut docs: Vec<Document> = (0..n)
        .map(|i| Document::from_json(&format!(r#"{{"n": {}, "grp": {}}}"#, i, i % 3)).unwrap())
        .collect();
    db.insert_many("items", &mut docs).unwrap();
    (path, db)
}

/// 构造 filter。
fn f(value: Value) -> Filter {
    Filter::from_value(&value).unwrap()
}

/// 构造排序。
fn sort(value: Value) -> SortSpec {
    SortSpec::from_value(&value).unwrap()
}

/// `total` 是匹配总数,与 `count` 一致;页内条数由 limit 决定。
#[test]
fn total_matches_count() {
    let (path, db) = setup("basic", 50);
    let filter = f(json!({"grp": 1}));

    let expected = db.count("items", &filter).unwrap();
    let (total, page) = db
        .find_page_total("items", &filter, None, 0, Some(5), None)
        .unwrap();

    assert_eq!(total, expected, "total 应与 count 一致");
    assert_eq!(page.len(), 5, "页内条数受 limit 限制");

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 结果与「`count` + `find_page`」完全等价(含排序、分页)。
#[test]
fn equivalent_to_count_plus_find_page() {
    let (path, db) = setup("equiv", 80);
    let filter = f(json!({"n": {"$gte": 20}}));
    let spec = sort(json!({"n": -1}));

    for (skip, limit) in [(0usize, 10usize), (7, 10), (60, 10), (10, 0)] {
        let expected_total = db.count("items", &filter).unwrap();
        let expected_page: Vec<Map<String, Value>> = db
            .find_page("items", &filter, Some(&spec), skip, Some(limit))
            .unwrap();

        let (total, page) = db
            .find_page_total("items", &filter, Some(&spec), skip, Some(limit), None)
            .unwrap();

        assert_eq!(total, expected_total, "skip={} limit={}", skip, limit);
        assert_eq!(page, expected_page, "skip={} limit={}", skip, limit);
    }

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 无排序时按主键序取页,total 依旧正确。
#[test]
fn without_sort_uses_pk_order() {
    let (path, db) = setup("nosort", 30);

    let (total, page) = db
        .find_page_total("items", &f(json!({})), None, 10, Some(5), None)
        .unwrap();
    assert_eq!(total, 30);
    assert_eq!(page.len(), 5);

    // 主键序:与直接按 id 排序后取同样区间一致
    let all = db.find("items", &f(json!({}))).unwrap();
    let expected: Vec<&Value> = all.iter().skip(10).take(5).map(|d| &d["_id"]).collect();
    let actual: Vec<&Value> = page.iter().map(|d| &d["_id"]).collect();
    assert_eq!(actual, expected);

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 无匹配时 total 为 0、页为空。
#[test]
fn empty_result() {
    let (path, db) = setup("empty", 10);
    let (total, page) = db
        .find_page_total("items", &f(json!({"n": 9999})), None, 0, Some(10), None)
        .unwrap();
    assert_eq!(total, 0);
    assert!(page.is_empty());

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 空集合。
#[test]
fn empty_collection() {
    let path = temp_nodb("nocoll");
    let db = Database::create(&path).unwrap();
    let (total, page) = db
        .find_page_total(
            "nothing",
            &Filter::match_all(),
            Some(&sort(json!({"n": 1}))),
            0,
            Some(10),
            None,
        )
        .unwrap();
    assert_eq!(total, 0);
    assert!(page.is_empty());

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// `find_page` 与 `find_page_total` 同源:同一查询的文档部分一致。
#[test]
fn find_page_delegates_to_total() {
    let (path, db) = setup("delegate", 20);
    let spec = sort(json!({"n": 1}));
    let a = db
        .find_page("items", &f(json!({})), Some(&spec), 3, Some(4))
        .unwrap();
    let (_, b) = db
        .find_page_total("items", &f(json!({})), Some(&spec), 3, Some(4), None)
        .unwrap();
    assert_eq!(a, b);

    drop(db);
    let _ = std::fs::remove_file(path);
}
