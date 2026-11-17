//! `$group` 累加器(v0.7.0):`$sum/$avg/$min/$max/$first/$last/$push/$addToSet/$count`。

use std::path::PathBuf;

use nodblite::aggregate::run;
use nodblite::{Database, Document};
use serde_json::{Value, json};

fn temp_nodb(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "nodblite_aggacc_{}_{}_{}.nodb",
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

/// 建库并插入销售数据,按 `item` 分组。
fn fixture(tag: &str) -> (PathBuf, Database) {
    let path = temp_nodb(tag);
    let db = Database::create(&path).unwrap();
    let mut docs: Vec<Document> = vec![
        // 显式 `_id`:保证扫描顺序 = 插入顺序(`$first`/`$last` 依赖输入顺序)
        json!({"_id": "s1", "item": "a", "qty": 3, "price": 10}),
        json!({"_id": "s2", "item": "b", "qty": 1, "price": 20}),
        json!({"_id": "s3", "item": "a", "qty": 2, "price": 30}),
    ]
    .into_iter()
    .map(|v| Document::from_value(v).unwrap())
    .collect();
    db.insert_many("sales", &mut docs).unwrap();
    (path, db)
}

fn group(db: &Database, acc: Value) -> Vec<Value> {
    let mut body = serde_json::Map::new();
    body.insert("_id".to_string(), json!("$item"));
    if let Some(obj) = acc.as_object() {
        for (k, v) in obj {
            body.insert(k.clone(), v.clone());
        }
    }
    let out = run(db, "sales", &json!([{"$group": Value::Object(body)}, {"$sort": {"_id": 1}}]))
        .unwrap();
    out.into_iter().map(Value::Object).collect()
}

#[test]
fn sum_over_field_and_constant() {
    let (path, db) = fixture("sum");
    let out = group(&db, json!({"total": {"$sum": "$qty"}, "docs": {"$sum": 1}}));
    assert_eq!(out[0]["total"], json!(5), "a: 3 + 2");
    assert_eq!(out[0]["docs"], json!(2));
    assert_eq!(out[1]["total"], json!(1));

    drop(db);
    let _ = std::fs::remove_file(path);
}

#[test]
fn avg_min_max() {
    let (path, db) = fixture("avg");
    let out = group(
        &db,
        json!({
            "avgQty": {"$avg": "$qty"},
            "minPrice": {"$min": "$price"},
            "maxPrice": {"$max": "$price"}
        }),
    );
    assert_eq!(out[0]["avgQty"], json!(2.5));
    assert_eq!(out[0]["minPrice"], json!(10));
    assert_eq!(out[0]["maxPrice"], json!(30));

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// `$avg` 在组内无数值时为 `null`(不是 0,也不是报错)。
#[test]
fn avg_without_numbers_is_null() {
    let (path, db) = fixture("avg_null");
    let out = group(&db, json!({"missing": {"$avg": "$nope"}}));
    assert_eq!(out[0]["missing"], Value::Null);

    drop(db);
    let _ = std::fs::remove_file(path);
}

#[test]
fn first_last_push_addtoset() {
    let (path, db) = fixture("first_last");
    let out = group(
        &db,
        json!({
            "firstQty": {"$first": "$qty"},
            "lastQty": {"$last": "$qty"},
            "allPrices": {"$push": "$price"},
            "uniqueItems": {"$addToSet": "$item"}
        }),
    );
    // 插入顺序:a(3,10) → b(1,20) → a(2,30)
    assert_eq!(out[0]["firstQty"], json!(3));
    assert_eq!(out[0]["lastQty"], json!(2));
    assert_eq!(out[0]["allPrices"], json!([10, 30]));
    assert_eq!(out[1]["allPrices"], json!([20]));
    assert_eq!(out[0]["uniqueItems"], json!(["a"]), "$addToSet 去重");

    drop(db);
    let _ = std::fs::remove_file(path);
}

#[test]
fn count_accumulator() {
    let (path, db) = fixture("count");
    let out = group(&db, json!({"n": {"$count": {}}}));
    assert_eq!(out[0]["n"], json!(2));
    assert_eq!(out[1]["n"], json!(1));

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// `_id` 可以是表达式(此处按价格区间分组)。
#[test]
fn group_id_expression() {
    let (path, db) = fixture("expr_id");
    let out = run(
        &db,
        "sales",
        &json!([
            {"$group": {"_id": {"$gt": ["$price", 15]}, "n": {"$sum": 1}}},
            {"$sort": {"_id": 1}}
        ]),
    )
    .unwrap();
    assert_eq!(out[0]["_id"], json!(false));
    assert_eq!(out[0]["n"], json!(1), "price=10 → false 组");
    assert_eq!(out[1]["n"], json!(2), "price=20/30 → true 组");

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 未实现的累加器必须报 `40324`,不静默当 0。
#[test]
fn unknown_accumulator_errors() {
    let (path, db) = fixture("unknown_acc");
    let err = run(
        &db,
        "sales",
        &json!([{"$group": {"_id": "$item", "x": {"$stdDevPop": "$qty"}}}]),
    )
    .unwrap_err();
    assert_eq!(err.code, 40324);

    drop(db);
    let _ = std::fs::remove_file(path);
}
