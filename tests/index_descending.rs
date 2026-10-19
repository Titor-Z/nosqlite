//! 降序索引(M27)。
//!
//! 核心语义:**降序索引的条目编码与升序完全相同,只有扫描方向相反**。
//! 因此这里的重点是「顺序正确性」—— 顺序错了不会报错,只会静默返回错误结果,
//! 必须逐个断言。

use std::path::PathBuf;

use nosqlite::query::{Filter, FindOptions, Update};
use nosqlite::{Database, Document};
use serde_json::{Map, Value, json};

fn temp_nodb(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "nosqlite_m27_{}_{}_{}.nodb",
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

/// 取出某字段的值序列(用于断言顺序)。
fn values(docs: &[Map<String, Value>], field: &str) -> Vec<i64> {
    docs.iter()
        .map(|d| d.get(field).and_then(|v| v.as_i64()).unwrap_or(-1))
        .collect()
}

/// 规划查询。
fn planned(db: &Database, cond: &Value, sort: &Value) -> Vec<Map<String, Value>> {
    let opts = FindOptions {
        sort: Some(nosqlite::query::SortSpec::from_value(sort).unwrap()),
        ..Default::default()
    };
    db.find_planned("items", &Filter::from_value(cond).unwrap(), cond, &opts)
        .unwrap()
}

// ==================== 方向语义 ====================

/// 降序索引 + 降序排序 → `sortedByIndex = true`,顺序正确。
#[test]
fn descending_index_matches_desc_sort() {
    let docs: Vec<Value> = (0..10).map(|i| json!({"n": i})).collect();
    let (path, db) = setup("desc", docs);

    let def = nosqlite::index::def::IndexDef::new_with_directions(
        "items",
        "n_-1",
        &["n"],
        &[-1],
        false,
        false,
    );
    assert!(def.is_descending());
    assert!(def.is_uniform_direction());
    db.create_index_with_directions("items", "n_-1", &["n"], &[-1], false, false)
        .unwrap();

    // 空 filter 无字段条件可索引 → 规划器走全表扫(这是既有行为)。
    // 用一个能从索引构造区间的条件来观察 sortedByIndex。
    let plan = db
        .explain(
            "items",
            &json!({"n": {"$gte": 0}}),
            Some(&nosqlite::query::SortSpec::from_value(&json!({"n": -1})).unwrap()),
        )
        .unwrap();
    assert_eq!(
        plan["sortedByIndex"],
        json!(true),
        "降序排序应能直接用降序索引"
    );

    let got = planned(&db, &json!({"n": {"$gte": 0}}), &json!({"n": -1}));
    assert_eq!(values(&got, "n"), vec![9, 8, 7, 6, 5, 4, 3, 2, 1, 0]);

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// **关键回归**:降序索引 + 升序排序 → 不能标记 sortedByIndex,但结果仍须正确。
#[test]
fn descending_index_with_asc_sort_is_not_sorted_by_index() {
    let docs: Vec<Value> = (0..10).map(|i| json!({"n": i})).collect();
    let (path, db) = setup("desc_asc", docs);
    db.create_index_with_directions("items", "n_-1", &["n"], &[-1], false, false)
        .unwrap();

    let sort = nosqlite::query::SortSpec::from_value(&json!({"n": 1})).unwrap();
    let plan = db
        .explain("items", &json!({"n": {"$gte": 0}}), Some(&sort))
        .unwrap();
    assert_eq!(plan["stage"], json!("IXSCAN"));
    assert_eq!(
        plan["sortedByIndex"],
        json!(false),
        "降序索引不能标记为满足升序排序"
    );

    // 结果必须仍是升序(走了内存排序)
    let got = planned(&db, &json!({"n": {"$gte": 0}}), &json!({"n": 1}));
    assert_eq!(values(&got, "n"), (0..10).collect::<Vec<i64>>());

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 升序索引 + 降序排序 → 同样不能标记,结果仍正确。
#[test]
fn ascending_index_with_desc_sort() {
    let docs: Vec<Value> = (0..10).map(|i| json!({"n": i})).collect();
    let (path, db) = setup("asc_desc", docs);
    db.create_index("items", "n_1", "n", false, false).unwrap();

    let sort = nosqlite::query::SortSpec::from_value(&json!({"n": -1})).unwrap();
    let plan = db
        .explain("items", &json!({"n": {"$gte": 0}}), Some(&sort))
        .unwrap();
    assert_eq!(plan["sortedByIndex"], json!(false));

    let got = planned(&db, &json!({"n": {"$gte": 0}}), &json!({"n": -1}));
    assert_eq!(values(&got, "n"), (0..10).rev().collect::<Vec<i64>>());

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 降序索引 + skip/limit:分页必须作用在正确方向的结果上。
#[test]
fn descending_index_with_pagination() {
    let docs: Vec<Value> = (0..20).map(|i| json!({"n": i})).collect();
    let (path, db) = setup("desc_page", docs);
    db.create_index_with_directions("items", "n_-1", &["n"], &[-1], false, false)
        .unwrap();

    let opts = FindOptions {
        sort: Some(nosqlite::query::SortSpec::from_value(&json!({"n": -1})).unwrap()),
        skip: 2,
        limit: Some(3),
        ..Default::default()
    };
    let got = db
        .find_planned(
            "items",
            &Filter::from_value(&json!({})).unwrap(),
            &json!({}),
            &opts,
        )
        .unwrap();
    assert_eq!(values(&got, "n"), vec![17, 16, 15], "降序第 3-5 条");

    drop(db);
    let _ = std::fs::remove_file(path);
}

// ==================== 与全表扫一致 ====================

/// 降序索引路径与全表扫结果完全一致(含范围条件)。
#[test]
fn descending_index_results_match_collscan() {
    let docs: Vec<Value> = (0..50).map(|i| json!({"n": i, "k": i % 3})).collect();
    let (path, db) = setup("desc_parity", docs);
    db.create_index_with_directions("items", "n_-1", &["n"], &[-1], false, false)
        .unwrap();

    let conds = vec![
        json!({}),
        json!({"n": 5}),
        json!({"n": {"$gte": 40}}),
        json!({"n": {"$lt": 3}}),
        json!({"n": {"$gte": 10, "$lte": 20}}),
        json!({"n": 999}),
    ];
    for cond in conds {
        let opts = FindOptions {
            sort: Some(nosqlite::query::SortSpec::from_value(&json!({"n": -1})).unwrap()),
            ..Default::default()
        };
        let want = db
            .find_with_options("items", &Filter::from_value(&cond).unwrap(), &opts)
            .unwrap();
        let got = db
            .find_planned("items", &Filter::from_value(&cond).unwrap(), &cond, &opts)
            .unwrap();
        assert_eq!(
            values(&got, "n"),
            values(&want, "n"),
            "条件 {:?} 下两条路径顺序与内容都应一致",
            cond
        );
    }

    drop(db);
    let _ = std::fs::remove_file(path);
}

// ==================== 复合与定义 ====================

/// 全降序的复合索引可用于对应的降序排序。
#[test]
fn composite_all_descending() {
    let docs: Vec<Value> = (0..12).map(|i| json!({"a": i % 3, "b": i})).collect();
    let (path, db) = setup("comp_desc", docs);
    db.create_index_with_directions("items", "a_-1_b_-1", &["a", "b"], &[-1, -1], false, false)
        .unwrap();

    let sort = nosqlite::query::SortSpec::from_value(&json!({"a": -1, "b": -1})).unwrap();
    let plan = db.explain("items", &json!({"a": 1}), Some(&sort)).unwrap();
    assert_eq!(plan["stage"], json!("IXSCAN"));
    assert_eq!(plan["sortedByIndex"], json!(true));

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 混向索引:不标记 sortedByIndex,但索引仍可用于过滤。
#[test]
fn mixed_direction_index_still_filters() {
    let docs: Vec<Value> = (0..12).map(|i| json!({"a": i % 3, "b": i})).collect();
    let (path, db) = setup("mixed", docs);
    db.create_index_with_directions("items", "a_1_b_-1", &["a", "b"], &[1, -1], false, false)
        .unwrap();

    let sort = nosqlite::query::SortSpec::from_value(&json!({"a": 1, "b": -1})).unwrap();
    let plan = db.explain("items", &json!({"a": 1}), Some(&sort)).unwrap();
    // 混向索引的方向无法用单次扫描满足 → 保守不标记
    assert_eq!(plan["sortedByIndex"], json!(false));

    let opts = FindOptions {
        sort: Some(sort),
        ..Default::default()
    };
    let got = db
        .find_planned(
            "items",
            &Filter::from_value(&json!({"a": 1})).unwrap(),
            &json!({"a": 1}),
            &opts,
        )
        .unwrap();
    assert_eq!(got.len(), 4, "混向索引仍需正确过滤");

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 方向在定义 JSON 中持久化;旧定义(无 directions)默认全升序。
#[test]
fn directions_persist_and_default() {
    let def = nosqlite::index::def::IndexDef::new_with_directions(
        "c",
        "i",
        &["a", "b"],
        &[1, -1],
        false,
        false,
    );
    let bytes = def.to_json();
    let back = nosqlite::index::def::IndexDef::from_json("c", "i", &bytes).unwrap();
    assert_eq!(back.directions, vec![1, -1]);

    // 模拟 M26 及更早的定义(只有 fields,没有 directions)
    let legacy = br#"{"fields":["a","b"],"unique":false,"sparse":false}"#;
    let old = nosqlite::index::def::IndexDef::from_json("c", "i", legacy).unwrap();
    assert_eq!(old.directions, vec![1, 1], "旧定义应默认为升序");

    drop((def, back));
}

/// 降序索引在更新后仍然正确(M27 回归)。
#[test]
fn descending_index_after_update() {
    let (path, db) = setup("desc_update", vec![json!({"n": 1}), json!({"n": 2})]);
    db.create_index_with_directions("items", "n_-1", &["n"], &[-1], false, false)
        .unwrap();

    let found = planned(&db, &json!({"n": 1}), &json!({"n": -1}));
    let id = found[0]["_id"].as_str().unwrap().to_string();

    let upd = Update::from_value(&json!({"$set": {"n": 100}})).unwrap();
    db.update_by_id("items", &id, &upd).unwrap();

    let got = planned(&db, &json!({}), &json!({"n": -1}));
    assert_eq!(values(&got, "n"), vec![100, 2], "更新后降序仍正确");

    drop(db);
    let _ = std::fs::remove_file(path);
}
