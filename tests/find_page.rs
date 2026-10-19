//! 排序分页两遍法测试(M11)。
//!
//! `Database::find_page` 用两遍法把排序分页的内存从 O(整个集合的完整文档 DOM)
//! 降到 O(n × 排序键大小)。本文件保证其**语义与 `find_with_options` 完全一致**,
//! 否则「省内存」就失去了意义。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use serde_json::{Map, Value};

use nodblite::query::{Filter, FindOptions, SortSpec, Update};
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

/// 辅助:解析 filter。
fn filter(json: &str) -> Filter {
    let v: Value = serde_json::from_str(json).expect("测试 JSON 应合法");
    Filter::from_value(&v).expect("filter 解析应成功")
}

/// 辅助:解析排序规格。
fn sort(json: &str) -> SortSpec {
    let v: Value = serde_json::from_str(json).expect("测试 JSON 应合法");
    SortSpec::from_value(&v).expect("排序解析应成功")
}

/// 辅助:创建数据库并插入 `n` 条文档(age = i,name = `user{i:02}`)。
fn setup(path: &Path, n: usize) -> Database {
    let db = Database::create(path).expect("创建数据库应成功");
    for i in 0..n {
        let mut doc = Document::from_json(&format!(
            r#"{{"_id":"d{i:03}","name":"user{i:02}","age":{i}}}"#
        ))
        .expect("文档应合法");
        db.insert("users", &mut doc).expect("插入应成功");
    }
    db
}

/// 辅助:按 `_id` 序号提取一份结果集的 id 序列,便于比对。
fn ids_of(docs: &[Map<String, Value>]) -> Vec<String> {
    docs.iter()
        .map(|d| d["_id"].as_str().unwrap_or("").to_string())
        .collect()
}

/// **等价性**:`find_page` 与 `find_with_options` 在「排序 + skip + limit」下结果一致。
#[test]
fn find_page_matches_find_with_options() {
    let path = temp_nodb("page_equiv");
    let db = setup(&path, 50);

    for (sort_json, skip, limit) in [
        (r#"{"age":1}"#, 0, 10),
        (r#"{"age":-1}"#, 0, 10),
        (r#"{"age":1}"#, 20, 10),
        (r#"{"age":-1}"#, 25, 5),
        (r#"{"name":-1}"#, 5, 20),
        (r#"{"age":1,"name":-1}"#, 10, 10),
    ] {
        let spec = sort(sort_json);
        let expected = db
            .find_with_options(
                "users",
                &filter("{}"),
                &FindOptions {
                    sort: Some(sort(sort_json)),
                    projection: None,
                    skip,
                    limit: Some(limit),
                },
            )
            .expect("find_with_options 应成功");

        let actual = db
            .find_page("users", &filter("{}"), Some(&spec), skip, Some(limit))
            .expect("find_page 应成功");

        assert_eq!(
            ids_of(&actual),
            ids_of(&expected),
            "排序 {} skip={} limit={} 的结果应与 find_with_options 一致",
            sort_json,
            skip,
            limit
        );
    }

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// **等价性**:带 filter 时同样一致。
#[test]
fn find_page_matches_with_filter() {
    let path = temp_nodb("page_filter");
    let db = setup(&path, 40);

    let f = filter(r#"{"age":{"$gte":10,"$lt":30}}"#);
    let spec = sort(r#"{"age":-1}"#);

    let expected = db
        .find_with_options(
            "users",
            &f,
            &FindOptions {
                sort: Some(sort(r#"{"age":-1}"#)),
                projection: None,
                skip: 5,
                limit: Some(7),
            },
        )
        .expect("find_with_options 应成功");

    let actual = db
        .find_page("users", &f, Some(&spec), 5, Some(7))
        .expect("find_page 应成功");

    assert_eq!(ids_of(&actual), ids_of(&expected), "带 filter 的结果应一致");
    assert_eq!(actual.len(), 7, "应返回 7 条");

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 无排序时按主键序分页(零额外内存路径)。
#[test]
fn find_page_without_sort_uses_pk_order() {
    let path = temp_nodb("page_nosort");
    let db = setup(&path, 20);

    let page = db
        .find_page("users", &filter("{}"), None, 5, Some(3))
        .expect("分页应成功");
    assert_eq!(
        ids_of(&page),
        vec!["d005", "d006", "d007"],
        "无排序应按主键序取页"
    );

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 边界:`skip` 超出总数返回空、`limit` 超出剩余返回剩余、`limit = None` 返回 skip 之后全部。
#[test]
fn find_page_boundaries() {
    let path = temp_nodb("page_bounds");
    let db = setup(&path, 10);
    let spec = sort(r#"{"age":1}"#);

    // skip 超出总数
    let p = db
        .find_page("users", &filter("{}"), Some(&spec), 100, Some(5))
        .expect("分页应成功");
    assert!(p.is_empty(), "skip 超出总数应返回空");

    // limit 超出剩余
    let p = db
        .find_page("users", &filter("{}"), Some(&spec), 8, Some(100))
        .expect("分页应成功");
    assert_eq!(p.len(), 2, "应返回剩余 2 条");

    // limit = None
    let p = db
        .find_page("users", &filter("{}"), Some(&spec), 7, None)
        .expect("分页应成功");
    assert_eq!(p.len(), 3, "limit 为 None 应返回 skip 之后全部");

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 空集合与无命中 filter 返回空,不报错。
#[test]
fn find_page_empty_and_no_match() {
    let path = temp_nodb("page_empty");
    let db = Database::create(&path).expect("创建数据库应成功");

    let spec = sort(r#"{"age":1}"#);
    assert!(
        db.find_page("users", &filter("{}"), Some(&spec), 0, Some(10))
            .expect("空集合分页应成功")
            .is_empty(),
        "空集合应返回空"
    );

    let mut doc = Document::from_json(r#"{"_id":"a","age":1}"#).unwrap();
    db.insert("users", &mut doc).expect("插入应成功");
    assert!(
        db.find_page("users", &filter(r#"{"age":999}"#), Some(&spec), 0, Some(10))
            .expect("无命中分页应成功")
            .is_empty(),
        "无命中应返回空"
    );

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 排序字段缺失的文档应排在最前(与 `compare_values` 的类型序一致)。
#[test]
fn find_page_missing_sort_field_first() {
    let path = temp_nodb("page_missing");
    let db = Database::create(&path).expect("创建数据库应成功");
    let mut a = Document::from_json(r#"{"_id":"has","age":1}"#).unwrap();
    let mut b = Document::from_json(r#"{"_id":"missing"}"#).unwrap();
    db.insert("users", &mut a).expect("插入应成功");
    db.insert("users", &mut b).expect("插入应成功");

    let spec = sort(r#"{"age":1}"#);
    let page = db
        .find_page("users", &filter("{}"), Some(&spec), 0, None)
        .expect("分页应成功");
    assert_eq!(ids_of(&page), vec!["missing", "has"], "缺失字段应排最前");

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 页内文档内容是完整的(两遍法第二遍按主键回取)。
#[test]
fn find_page_returns_full_documents() {
    let path = temp_nodb("page_full");
    let db = setup(&path, 5);
    let spec = sort(r#"{"age":1}"#);

    let page = db
        .find_page("users", &filter("{}"), Some(&spec), 0, Some(2))
        .expect("分页应成功");
    assert_eq!(page.len(), 2);
    // 不只是主键:其余字段也应完整
    assert_eq!(page[0]["name"], "user00");
    assert_eq!(page[0]["age"], 0);
    assert_eq!(page[1]["name"], "user01");
    assert_eq!(page[1]["age"], 1);

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}

/// 分页结果与后续更新互不干扰(先分页后改数据,再分页应看到新值)。
#[test]
fn find_page_reflects_updates() {
    let path = temp_nodb("page_after_update");
    let db = setup(&path, 5);
    let spec = sort(r#"{"age":-1}"#);

    let first = db
        .find_page("users", &filter("{}"), Some(&spec), 0, Some(1))
        .expect("分页应成功");
    assert_eq!(first[0]["_id"], "d004", "降序首条应是 age 最大者");

    // 把 d000 的 age 提到最大
    db.update_by_id(
        "users",
        "d000",
        &Update::from_value(&serde_json::json!({"$set": {"age": 100}})).unwrap(),
    )
    .expect("更新应成功");

    let second = db
        .find_page("users", &filter("{}"), Some(&spec), 0, Some(1))
        .expect("分页应成功");
    assert_eq!(second[0]["_id"], "d000", "更新后应成为降序首条");

    db.close().expect("关闭应成功");
    drop(db);
    cleanup(&path);
}
