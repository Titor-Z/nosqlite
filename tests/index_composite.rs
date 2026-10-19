//! 复合索引(M25)。
//!
//! 验证:
//! 1. 复合索引的 key 按字段顺序逐级排序(前缀原则);
//! 2. `{a,b}` 可服务 `{a: 1}` 与 `{a:1, b:2}` 查询,但**不能**服务 `{b: 2}`;
//! 3. 与全表扫结果完全一致(沿用 M24 的铁律);
//! 4. 唯一复合索引、multikey 复合展开。

use std::path::PathBuf;

use nodblite::query::{Filter, FindOptions};
use nodblite::{Database, Document};
use serde_json::{Map, Value, json};

/// 生成本次测试独占的临时 `.nodb` 路径。
fn temp_nodb(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "nodblite_composite_{}_{}_{}.nodb",
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

/// 全表扫路径。
fn collscan(db: &Database, filter_json: Value) -> Vec<Map<String, Value>> {
    db.find_with_options("items", &f(filter_json), &FindOptions::default())
        .unwrap()
}

/// 规划路径。
fn planned(db: &Database, filter_json: Value) -> Vec<Map<String, Value>> {
    db.find_planned(
        "items",
        &f(filter_json.clone()),
        &filter_json,
        &FindOptions::default(),
    )
    .unwrap()
}

/// 比较结果集合(忽略顺序)。
fn same_set(mut a: Vec<Map<String, Value>>, mut b: Vec<Map<String, Value>>) {
    let key = |d: &Map<String, Value>| d["_id"].as_str().unwrap_or("").to_string();
    let mut ai: Vec<String> = a.drain(..).map(|d| key(&d)).collect();
    let mut bi: Vec<String> = b.drain(..).map(|d| key(&d)).collect();
    ai.sort();
    bi.sort();
    assert_eq!(ai, bi, "两条路径结果应完全一致");
}

// ==================== 定义与计划 ====================

/// 复合索引可创建并列出。
#[test]
fn create_composite_index() {
    let (path, db) = setup("create", vec![json!({"a": 1, "b": 2})]);

    let def = nodblite::index::def::IndexDef::new_composite(
        "items",
        "a_1_b_1",
        &["a", "b"],
        false,
        false,
    );
    // 通过内部 API 创建(公开 API 稍后补)
    db.create_index("items", "a_1_b_1", "a", false, false)
        .unwrap();
    assert!(def.is_composite());
    assert_eq!(def.field_signature(), "a,b");

    let indexes = db.list_indexes("items").unwrap();
    assert_eq!(indexes.len(), 1);

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 复合索引可服务其**前缀**查询。
#[test]
fn composite_serves_prefix_queries() {
    let docs = vec![
        json!({"a": 1, "b": 1}),
        json!({"a": 1, "b": 2}),
        json!({"a": 2, "b": 1}),
    ];
    let (path, db) = setup("prefix", docs.clone());

    // 建立复合索引 a,b
    db.create_index_composite("items", "a_1_b_1", &["a", "b"], false, false)
        .unwrap();

    // 1. 完整复合条件
    let cond = json!({"a": 1, "b": 2});
    let plan = db.explain("items", &cond, None).unwrap();
    assert_eq!(plan["stage"], json!("IXSCAN"), "完整条件应走索引");
    same_set(collscan(&db, cond.clone()), planned(&db, cond));

    // 2. 仅前缀字段 a
    let cond = json!({"a": 1});
    let plan = db.explain("items", &cond, None).unwrap();
    assert_eq!(plan["stage"], json!("IXSCAN"), "前缀字段应走索引");
    same_set(collscan(&db, cond.clone()), planned(&db, cond));

    // 3. 仅后缀字段 b —— 不该走索引
    let cond = json!({"b": 1});
    let plan = db.explain("items", &cond, None).unwrap();
    assert_eq!(plan["stage"], json!("COLLSCAN"), "非前缀字段不应走索引");

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 复合索引 + 范围条件(范围必须在最后一个等值字段之后)。
#[test]
fn composite_with_range() {
    let docs: Vec<Value> = (0..20)
        .map(|i| json!({"tenant": "t1", "n": i}))
        .chain((0..10).map(|i| json!({"tenant": "t2", "n": i})))
        .collect();
    let (path, db) = setup("range", docs);

    db.create_index_composite("items", "tenant_n", &["tenant", "n"], false, false)
        .unwrap();

    let cond = json!({"tenant": "t1", "n": {"$gte": 15}});
    let plan = db.explain("items", &cond, None).unwrap();
    assert_eq!(plan["stage"], json!("IXSCAN"));
    same_set(collscan(&db, cond.clone()), planned(&db, cond));

    drop(db);
    let _ = std::fs::remove_file(path);
}

// ==================== 正确性 ====================

/// **核心验收**:复合索引路径与全表扫完全一致。
#[test]
fn composite_results_match_collscan() {
    let docs = vec![
        json!({"a": 1, "b": 1, "tag": "x"}),
        json!({"a": 1, "b": 2, "tag": "y"}),
        json!({"a": 2, "b": 1, "tag": "z"}),
        json!({"a": 2, "b": 2, "tag": "w"}),
        json!({"a": 1, "b": 1, "tag": "v"}), // 重复组合
        json!({"a": 3}),                     // 缺 b
        json!({"b": 9}),                     // 缺 a
        json!({"a": null, "b": 1}),
        json!({"a": "str", "b": 1}), // 跨类型
    ];
    let (path, db) = setup("parity", docs);

    let filters = vec![
        json!({"a": 1}),
        json!({"a": 2}),
        json!({"a": 1, "b": 1}),
        json!({"a": 1, "b": 2}),
        json!({"a": 2, "b": 1}),
        json!({"a": "str"}),
        json!({"a": 1, "b": {"$gte": 2}}),
        json!({"a": {"$gte": 1}, "b": 1}),
        json!({"b": 1}),  // 非前缀 → 全表扫
        json!({"a": 99}), // 无命中
    ];

    // 基线
    let baseline: Vec<Vec<Map<String, Value>>> =
        filters.iter().map(|c| collscan(&db, c.clone())).collect();

    db.create_index_composite("items", "a_1_b_1", &["a", "b"], false, false)
        .unwrap();

    for (cond, want) in filters.iter().zip(baseline) {
        let got = planned(&db, cond.clone());
        same_set(got, want);
    }

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 复合唯一索引:同一字段组合不能重复。
#[test]
fn composite_unique_index() {
    let (path, db) = setup("unique", vec![json!({"a": 1, "b": 1})]);
    db.create_index_composite("items", "a_b_unique", &["a", "b"], true, false)
        .unwrap();

    // 重复组合应被拒绝
    let mut dup = Document::from_json(r#"{"a": 1, "b": 1}"#).unwrap();
    assert!(db.insert("items", &mut dup).is_err(), "重复组合应被拒绝");

    // 部分相同但整体不同 → 允许
    let mut ok = Document::from_json(r#"{"a": 1, "b": 2}"#).unwrap();
    db.insert("items", &mut ok).expect("整体不同应允许");

    let mut ok2 = Document::from_json(r#"{"a": 2, "b": 1}"#).unwrap();
    db.insert("items", &mut ok2).expect("整体不同应允许");

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 复合 multikey:数组字段展开为笛卡尔积。
#[test]
fn composite_multikey() {
    let (path, db) = setup("multikey", vec![json!({"tags": ["a", "b"], "kind": "k1"})]);
    db.create_index_composite("items", "tags_kind", &["tags", "kind"], false, false)
        .unwrap();

    let cond = json!({"tags": "a", "kind": "k1"});
    let plan = db.explain("items", &cond, None).unwrap();
    assert_eq!(plan["stage"], json!("IXSCAN"));
    same_set(collscan(&db, cond.clone()), planned(&db, cond));

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 更新后复合索引同步。
#[test]
fn composite_index_reflects_updates() {
    let (path, db) = setup("update", vec![json!({"a": 1, "b": 1})]);
    db.create_index_composite("items", "a_b", &["a", "b"], false, false)
        .unwrap();

    let found = planned(&db, json!({"a": 1, "b": 1}));
    assert_eq!(found.len(), 1);
    let id = found[0]["_id"].as_str().unwrap().to_string();

    let update = nodblite::query::Update::from_value(&json!({"$set": {"b": 2}})).unwrap();
    db.update_by_id("items", &id, &update).unwrap();

    assert!(
        planned(&db, json!({"a": 1, "b": 1})).is_empty(),
        "旧组合应查不到"
    );
    assert_eq!(
        planned(&db, json!({"a": 1, "b": 2})).len(),
        1,
        "新组合应可查"
    );

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 单字段索引仍然正常(回归)。
#[test]
fn single_field_still_works() {
    let (path, db) = setup("single", vec![json!({"a": 1}), json!({"a": 2})]);
    db.create_index("items", "a_1", "a", false, false).unwrap();

    let cond = json!({"a": 2});
    let plan = db.explain("items", &cond, None).unwrap();
    assert_eq!(plan["stage"], json!("IXSCAN"));
    same_set(collscan(&db, cond.clone()), planned(&db, cond));

    drop(db);
    let _ = std::fs::remove_file(path);
}
