//! 查询规划器(M24):索引是否被正确选用,且结果与全表扫**完全一致**。
//!
//! # 最重要的验收
//!
//! `index_results_match_collscan` 对同一个 filter 分别走「建索引」与「不建索引」
//! 两条路径,断言结果完全相同。这是索引正确性的最终保障 —— 一旦区间构造有偏差,
//! 这条测试会立刻失败,而不是等到生产环境静默返回错误数据。

use std::path::PathBuf;

use nodblite::query::{Filter, FindOptions, SortSpec};
use nodblite::{Database, Document};
use serde_json::{Map, Value, json};

/// 生成本次测试独占的临时 `.nodb` 路径。
fn temp_nodb(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "nodblite_plan_{}_{}_{}.nodb",
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

/// 建库并插入文档。
fn setup(tag: &str, docs: Vec<Value>) -> (PathBuf, Database) {
    let path = temp_nodb(tag);
    let db = Database::create(&path).unwrap();
    let mut list: Vec<Document> = docs
        .into_iter()
        .map(|v| Document::from_json(&v.to_string()).unwrap())
        .collect();
    db.insert_many("items", &mut list).unwrap();
    (path, db)
}

/// 构造 filter。
fn f(v: Value) -> Filter {
    Filter::from_value(&v).unwrap()
}

/// 用全表扫查询(不建索引)。
fn collscan(db: &Database, filter_json: Value) -> Vec<Map<String, Value>> {
    db.find_with_options(
        "items",
        &f(filter_json),
        &FindOptions {
            sort: None,
            projection: None,
            skip: 0,
            limit: None,
        },
    )
    .unwrap()
}

/// 用规划路径查询。
fn planned(db: &Database, filter_json: Value) -> Vec<Map<String, Value>> {
    db.find_planned(
        "items",
        &f(filter_json.clone()),
        &filter_json,
        &FindOptions::default(),
    )
    .unwrap()
}

// ==================== 计划选择 ====================

/// 无索引 → COLLSCAN。
#[test]
fn no_index_means_collscan() {
    let (path, db) = setup("noindex", vec![json!({"n": 1})]);
    let plan = db.explain("items", &json!({"n": 1}), None).unwrap();
    assert_eq!(plan["stage"], json!("COLLSCAN"));

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 等值条件命中索引 → IXSCAN。
#[test]
fn equality_uses_index() {
    let (path, db) = setup("eq", vec![json!({"n": 1}), json!({"n": 2})]);
    db.create_index("items", "n_1", "n", false, false).unwrap();

    let plan = db.explain("items", &json!({"n": 2}), None).unwrap();
    assert_eq!(plan["stage"], json!("IXSCAN"));
    assert_eq!(plan["indexName"], json!("n_1"));

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 范围条件命中索引。
#[test]
fn range_uses_index() {
    let (path, db) = setup("range", vec![json!({"n": 1})]);
    db.create_index("items", "n_1", "n", false, false).unwrap();

    for cond in [
        json!({"n": {"$gte": 1}}),
        json!({"n": {"$gt": 1}}),
        json!({"n": {"$lte": 5}}),
        json!({"n": {"$lt": 5}}),
        json!({"n": {"$gte": 1, "$lte": 5}}),
    ] {
        let plan = db.explain("items", &cond, None).unwrap();
        assert_eq!(plan["stage"], json!("IXSCAN"), "条件 {:?} 应走索引", cond);
    }

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 索引字段与 filter 字段不匹配 → COLLSCAN。
#[test]
fn unmatched_field_falls_back() {
    let (path, db) = setup("unmatched", vec![json!({"n": 1, "other": 2})]);
    db.create_index("items", "n_1", "n", false, false).unwrap();

    let plan = db.explain("items", &json!({"other": 2}), None).unwrap();
    assert_eq!(plan["stage"], json!("COLLSCAN"), "不在索引字段上应全表扫");

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 排序字段与索引字段一致时标记 sortedByIndex。
#[test]
fn sort_matches_index_field() {
    let (path, db) = setup("sorted", vec![json!({"n": 1})]);
    db.create_index("items", "n_1", "n", false, false).unwrap();

    let sort = SortSpec::from_value(&json!({"n": 1})).unwrap();
    let plan = db
        .explain("items", &json!({"n": {"$gte": 0}}), Some(&sort))
        .unwrap();
    assert_eq!(plan["stage"], json!("IXSCAN"));
    assert_eq!(plan["sortedByIndex"], json!(true));

    // 排序字段不同则不标记
    let other_sort = SortSpec::from_value(&json!({"x": 1})).unwrap();
    let plan = db
        .explain("items", &json!({"n": {"$gte": 0}}), Some(&other_sort))
        .unwrap();
    assert_eq!(plan["sortedByIndex"], json!(false));

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// explain 明确标注「回表后仍需完整 filter 过滤」。
#[test]
fn explain_marks_secondary_filter() {
    let (path, db) = setup("explainmark", vec![json!({"n": 1})]);
    db.create_index("items", "n_1", "n", false, false).unwrap();
    let plan = db.explain("items", &json!({"n": 1}), None).unwrap();
    assert_eq!(plan["filterApplied"], json!(true));
    drop(db);
    let _ = std::fs::remove_file(path);
}

// ==================== 正确性(最重要) ====================

/// **核心验收**:索引路径与全表扫路径结果完全一致。
#[test]
fn index_results_match_collscan() {
    // 构造覆盖多种类型的数据
    let docs = vec![
        json!({"n": 1, "s": "a"}),
        json!({"n": 2, "s": "b"}),
        json!({"n": 3, "s": "c"}),
        json!({"n": 10, "s": "d"}),
        json!({"n": -1, "s": "e"}),
        json!({"n": 2, "s": "f"}), // 重复值
        json!({"s": "no-n"}),      // 缺失 n
        json!({"n": null, "s": "null-n"}),
        json!({"n": 1.5, "s": "float"}),
        json!({"n": true, "s": "bool"}), // 跨类型
    ];
    let (path, db) = setup("parity", docs);

    let filters = vec![
        json!({"n": 2}),
        json!({"n": 1}),
        json!({"n": 1.5}),
        json!({"n": {"$gte": 2}}),
        json!({"n": {"$gt": 2}}),
        json!({"n": {"$lte": 2}}),
        json!({"n": {"$lt": 2}}),
        json!({"n": {"$gte": 2, "$lte": 10}}),
        json!({"n": {"$gte": 100}}), // 无命中
        json!({"n": -1}),
        json!({"n": true}), // bool 类型
        json!({"s": "a"}),  // 非索引字段
    ];

    // 1. 先在不建索引的情况跑一遍
    let baseline: Vec<Vec<Map<String, Value>>> = filters
        .iter()
        .map(|cond| collscan(&db, cond.clone()))
        .collect();

    // 2. 建索引后再跑
    db.create_index("items", "n_1", "n", false, false).unwrap();

    for (cond, want) in filters.iter().zip(baseline.iter()) {
        let got = planned(&db, cond.clone());
        // 比较 _id 集合(顺序可能不同,内容必须相同)
        let mut want_ids: Vec<&str> = want
            .iter()
            .filter_map(|d| d.get("_id").and_then(|v| v.as_str()))
            .collect();
        let mut got_ids: Vec<&str> = got
            .iter()
            .filter_map(|d| d.get("_id").and_then(|v| v.as_str()))
            .collect();
        want_ids.sort();
        got_ids.sort();
        assert_eq!(
            got_ids, want_ids,
            "索引路径与全表扫结果不一致,条件: {}",
            cond
        );
    }

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 索引路径与全表扫在「排序 + 分页」下也一致。
#[test]
fn index_results_match_with_sort_and_pagination() {
    let docs: Vec<Value> = (0..50).map(|i| json!({"n": i})).collect();
    let (path, db) = setup("parity_sort", docs);

    let filter_json = json!({"n": {"$gte": 10}});

    // 全表扫基线
    let opts = FindOptions {
        sort: Some(SortSpec::from_value(&json!({"n": -1})).unwrap()),
        projection: None,
        skip: 5,
        limit: Some(10),
    };
    let baseline = db
        .find_with_options("items", &f(filter_json.clone()), &opts)
        .unwrap();

    // 建索引后走规划路径
    db.create_index("items", "n_1", "n", false, false).unwrap();
    let got = db
        .find_planned("items", &f(filter_json.clone()), &filter_json, &opts)
        .unwrap();

    assert_eq!(got.len(), baseline.len());
    let want: Vec<&Value> = baseline.iter().map(|d| &d["n"]).collect();
    let have: Vec<&Value> = got.iter().map(|d| &d["n"]).collect();
    assert_eq!(have, want, "排序分页后结果应一致");

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 索引扫描也能正确处理 multikey(数组字段)。
#[test]
fn index_scan_on_array_field_matches_collscan() {
    let docs = vec![
        json!({"tags": ["a", "b"], "k": 1}),
        json!({"tags": ["b", "c"], "k": 2}),
        json!({"tags": ["d"], "k": 3}),
        json!({"tags": [], "k": 4}),
        json!({"k": 5}),
    ];
    let (path, db) = setup("parity_multi", docs);

    let filter_json = json!({"tags": "b"});
    let baseline = collscan(&db, filter_json.clone());

    db.create_index("items", "tags_1", "tags", false, false)
        .unwrap();
    let got = planned(&db, filter_json);

    let mut want: Vec<&str> = baseline
        .iter()
        .filter_map(|d| d.get("_id").and_then(|v| v.as_str()))
        .collect();
    let mut have: Vec<&str> = got
        .iter()
        .filter_map(|d| d.get("_id").and_then(|v| v.as_str()))
        .collect();
    want.sort();
    have.sort();
    assert_eq!(have, want, "multikey 索引结果应与全表扫一致");

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 更新后索引扫描结果同步变化。
#[test]
fn index_scan_reflects_updates() {
    let (path, db) = setup("update", vec![json!({"n": 1}), json!({"n": 2})]);
    db.create_index("items", "n_1", "n", false, false).unwrap();

    let before = planned(&db, json!({"n": 2}));
    assert_eq!(before.len(), 1);

    // 把 n=2 改成 n=3
    let id = before[0]["_id"].as_str().unwrap().to_string();
    let update = nodblite::query::Update::from_value(&json!({"$set": {"n": 3}})).unwrap();
    db.update_by_id("items", &id, &update).unwrap();

    assert!(planned(&db, json!({"n": 2})).is_empty(), "旧值应查不到");
    assert_eq!(planned(&db, json!({"n": 3})).len(), 1, "新值应可查到");

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 删除后索引扫描结果同步变化。
#[test]
fn index_scan_reflects_deletes() {
    let (path, db) = setup("delete", vec![json!({"n": 1}), json!({"n": 2})]);
    db.create_index("items", "n_1", "n", false, false).unwrap();

    let found = planned(&db, json!({"n": 1}));
    assert_eq!(found.len(), 1);
    let id = found[0]["_id"].as_str().unwrap().to_string();
    db.delete_by_id("items", &id).unwrap();

    assert!(planned(&db, json!({"n": 1})).is_empty());

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 无索引时 find_planned 等同全表扫。
#[test]
fn planned_falls_back_to_collscan() {
    let (path, db) = setup("fallback", vec![json!({"n": 1}), json!({"n": 2})]);
    let got = planned(&db, json!({"n": 2}));
    assert_eq!(got.len(), 1);
    drop(db);
    let _ = std::fs::remove_file(path);
}
