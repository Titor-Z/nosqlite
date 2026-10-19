//! `$or` 走索引与索引交集(M26)。
//!
//! 核心验收:**所有计划路径的结果必须与全表扫逐一一致**。
//! 索引只负责缩小候选集,正确性由完整 filter 二次过滤保证 —— 这条铁律
//! 在 Union/Intersect 这类组合计划上尤其容易破,必须逐条比对。

use std::path::PathBuf;

use nosqlite::query::{Filter, FindOptions};
use nosqlite::{Database, Document};
use serde_json::{Map, Value, json};

fn temp_nodb(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "nosqlite_m26_{}_{}_{}.nodb",
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

fn collscan(db: &Database, cond: &Value) -> Vec<Map<String, Value>> {
    db.find_with_options(
        "items",
        &Filter::from_value(cond).unwrap(),
        &FindOptions::default(),
    )
    .unwrap()
}

fn planned(db: &Database, cond: &Value) -> Vec<Map<String, Value>> {
    db.find_planned(
        "items",
        &Filter::from_value(cond).unwrap(),
        cond,
        &FindOptions::default(),
    )
    .unwrap()
}

/// 断言两条路径结果集完全相同(忽略顺序)。
fn same(db: &Database, cond: &Value) -> Vec<Map<String, Value>> {
    let want = collscan(db, cond);
    let got = planned(db, cond);
    let key = |d: &Map<String, Value>| d["_id"].as_str().unwrap_or("").to_string();
    let mut a: Vec<String> = want.iter().map(key).collect();
    let mut b: Vec<String> = got.iter().map(key).collect();
    a.sort();
    b.sort();
    assert_eq!(a, b, "条件 {:?} 两条路径结果不一致", cond);
    got
}

// ==================== $or 走索引 ====================

/// `$or` 全部分支可走索引 → 生成 OR 计划,结果与全表扫一致。
#[test]
fn or_uses_index_union() {
    let docs: Vec<Value> = (0..30).map(|i| json!({"n": i})).collect();
    let (path, db) = setup("or_union", docs);
    db.create_index("items", "n_1", "n", false, false).unwrap();

    let cond = json!({"$or": [{"n": 1}, {"n": 2}, {"n": 3}]});
    let plan = db.explain("items", &cond, None).unwrap();
    assert_eq!(plan["stage"], json!("OR"), "全分支可索引时应生成 OR 计划");
    assert_eq!(plan["branches"].as_array().unwrap().len(), 3);

    let got = same(&db, &cond);
    assert_eq!(got.len(), 3);

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// `$or` 中有分支用不上索引 → 整体退回全表扫(避免混合计划的复杂度)。
#[test]
fn or_with_unindexed_branch_falls_back() {
    let (path, db) = setup("or_fallback", vec![json!({"n": 1, "x": 1})]);
    db.create_index("items", "n_1", "n", false, false).unwrap();

    let cond = json!({"$or": [{"n": 1}, {"x": 1}]});
    let plan = db.explain("items", &cond, None).unwrap();
    assert_eq!(plan["stage"], json!("COLLSCAN"), "有分支不可索引应全表扫");
    same(&db, &cond);

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// `$or` 分支内带范围条件。
#[test]
fn or_with_range_branches() {
    let docs: Vec<Value> = (0..50).map(|i| json!({"n": i})).collect();
    let (path, db) = setup("or_range", docs);
    db.create_index("items", "n_1", "n", false, false).unwrap();

    let cond = json!({"$or": [{"n": {"$lt": 3}}, {"n": {"$gte": 47}}]});
    let plan = db.explain("items", &cond, None).unwrap();
    assert_eq!(plan["stage"], json!("OR"));
    let got = same(&db, &cond);
    assert_eq!(got.len(), 6, "0,1,2 + 47,48,49");

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// `$or` 分支的值有重叠时,并集必须去重。
#[test]
fn or_union_deduplicates() {
    let docs: Vec<Value> = (0..10).map(|i| json!({"n": i})).collect();
    let (path, db) = setup("or_dup", docs);
    db.create_index("items", "n_1", "n", false, false).unwrap();

    // $gte:8 与 n:9 都命中 n=9,不能返回两次
    let cond = json!({"$or": [{"n": {"$gte": 8}}, {"n": 9}]});
    let got = same(&db, &cond);
    assert_eq!(got.len(), 2, "8 与 9,9 不能重复计数");

    drop(db);
    let _ = std::fs::remove_file(path);
}

// ==================== 索引交集 ====================

/// 两个独立单字段索引 + 两个顶层等值条件 → AND 计划。
#[test]
fn two_indexes_intersect() {
    let docs: Vec<Value> = (0..40)
        .map(|i| json!({"a": i % 4, "b": i % 5, "n": i}))
        .collect();
    let (path, db) = setup("intersect", docs);
    db.create_index("items", "a_1", "a", false, false).unwrap();
    db.create_index("items", "b_1", "b", false, false).unwrap();

    let cond = json!({"a": 1, "b": 2});
    let plan = db.explain("items", &cond, None).unwrap();
    assert_eq!(plan["stage"], json!("AND"), "两字段各有索引应取交集");
    assert_eq!(plan["branches"].as_array().unwrap().len(), 2);

    let got = same(&db, &cond);
    // a=1 且 b=2 → i % 4 == 1 && i % 5 == 2 → i ∈ {17, 37}
    assert_eq!(got.len(), 2);

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 交集与全表扫在大量组合下逐一一致。
#[test]
fn intersect_matches_collscan_across_conditions() {
    let docs: Vec<Value> = (0..60)
        .map(|i| json!({"a": i % 3, "b": i % 7, "c": i}))
        .collect();
    let (path, db) = setup("intersect_parity", docs);
    db.create_index("items", "a_1", "a", false, false).unwrap();
    db.create_index("items", "b_1", "b", false, false).unwrap();

    for a in 0..3 {
        for b in 0..7 {
            same(&db, &json!({"a": a, "b": b}));
        }
    }
    // 有索引的字段 + 无索引的字段(交集只覆盖有索引部分,剩余由 filter 过滤)
    same(&db, &json!({"a": 1, "b": 2, "c": {"$gte": 20}}));

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 只有一个索引时不生成交集(变成普通 IXSCAN)。
#[test]
fn single_index_no_intersection() {
    let (path, db) = setup("one_idx", vec![json!({"a": 1, "b": 2})]);
    db.create_index("items", "a_1", "a", false, false).unwrap();

    let plan = db.explain("items", &json!({"a": 1, "b": 2}), None).unwrap();
    assert_eq!(plan["stage"], json!("IXSCAN"), "单索引不应生成 AND 计划");

    drop(db);
    let _ = std::fs::remove_file(path);
}

// ==================== drop_collection 清理索引 ====================

/// 删除集合时索引定义与条目一起清理(M26 补的遗留项)。
#[test]
fn drop_collection_clears_indexes() {
    let (path, db) = setup("drop", vec![json!({"a": 1})]);
    db.create_index("items", "a_1", "a", false, false).unwrap();
    assert_eq!(db.list_indexes("items").unwrap().len(), 1);

    db.drop_collection("items").unwrap();
    assert!(
        db.list_indexes("items").unwrap().is_empty(),
        "删集合后索引定义应一并清理"
    );

    // 重建同名集合 + 同名索引应能成功(旧条目不能干扰)
    let mut d = Document::from_json(r#"{"a": 2}"#).unwrap();
    db.insert("items", &mut d).unwrap();
    db.create_index("items", "a_1", "a", false, false)
        .expect("旧索引条目残留会导致重建失败");
    assert_eq!(planned(&db, &json!({"a": 2})).len(), 1);

    drop(db);
    let _ = std::fs::remove_file(path);
}
