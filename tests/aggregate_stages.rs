//! 聚合阶段(v0.7.0):`$unwind` / `$lookup` / `$facet` / `$bucket` /
//! `$replaceRoot` / `$count` / `$unset` / `$project` 表达式 / `$stop` 等基础阶段。

use std::path::PathBuf;

use nodblite::aggregate::run;
use nodblite::{Database, Document};
use serde_json::{Value, json};

fn temp_nodb(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "nodblite_agg_{}_{}_{}.nodb",
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

fn setup(tag: &str) -> (PathBuf, Database) {
    let path = temp_nodb(tag);
    let db = Database::create(&path).unwrap();
    (path, db)
}

fn insert(db: &Database, coll: &str, docs: Vec<Value>) {
    let mut docs: Vec<Document> = docs
        .into_iter()
        .map(|v| Document::from_value(v).unwrap())
        .collect();
    db.insert_many(coll, &mut docs).unwrap();
}

fn agg(db: &Database, coll: &str, pipeline: Value) -> Vec<Value> {
    run(db, coll, &pipeline)
        .unwrap()
        .into_iter()
        .map(Value::Object)
        .collect()
}

// ---------------------------- $unwind ----------------------------

#[test]
fn unwind_expands_array() {
    let (path, db) = setup("unwind");
    insert(&db, "posts", vec![json!({"title": "a", "tags": ["x", "y"]})]);

    let out = agg(&db, "posts", json!([{"$unwind": "$tags"}]));
    assert_eq!(out.len(), 2);
    assert_eq!(out[0]["tags"], json!("x"));
    assert_eq!(out[1]["tags"], json!("y"));
    // 其它字段被复制
    assert_eq!(out[0]["title"], json!("a"));

    drop(db);
    let _ = std::fs::remove_file(path);
}

#[test]
fn unwind_skips_empty_by_default() {
    let (path, db) = setup("unwind_empty");
    insert(
        &db,
        "posts",
        vec![
            json!({"_id": "1", "id": 1, "tags": []}),
            json!({"_id": "2", "id": 2}),
            json!({"_id": "3", "id": 3, "tags": ["z"]}),
        ],
    );
    let out = agg(&db, "posts", json!([{"$unwind": "$tags"}]));
    assert_eq!(out.len(), 1, "空数组与缺失字段默认丢弃");
    assert_eq!(out[0]["id"], json!(3));

    // preserveNullAndEmptyArrays:全部保留
    let kept = agg(
        &db,
        "posts",
        json!([{"$unwind": {"path": "$tags", "preserveNullAndEmptyArrays": true}}]),
    );
    assert_eq!(kept.len(), 3);
    assert_eq!(kept[0]["tags"], json!([]));
    assert!(kept[1].get("tags").is_none(), "缺失字段保留为缺失");

    drop(db);
    let _ = std::fs::remove_file(path);
}

#[test]
fn unwind_records_index() {
    let (path, db) = setup("unwind_index");
    insert(&db, "posts", vec![json!({"tags": ["x", "y"]})]);
    let out = agg(
        &db,
        "posts",
        json!([{"$unwind": {"path": "$tags", "includeArrayIndex": "i"}}]),
    );
    assert_eq!(out[0]["i"], json!(0));
    assert_eq!(out[1]["i"], json!(1));

    drop(db);
    let _ = std::fs::remove_file(path);
}

// ---------------------------- $lookup ----------------------------

#[test]
fn lookup_joins_by_field() {
    let (path, db) = setup("lookup");
    insert(&db, "authors", vec![json!({"_id": "a1", "name": "张三"})]);
    insert(
        &db,
        "books",
        vec![
            json!({"title": "b1", "author": "a1"}),
            json!({"title": "b2", "author": "missing"}),
        ],
    );

    let out = agg(
        &db,
        "books",
        json!([{
            "$lookup": {"from": "authors", "localField": "author", "foreignField": "_id", "as": "authorDoc"}
        }, {"$sort": {"title": 1}}]),
    );
    assert_eq!(out.len(), 2);
    assert_eq!(out[0]["authorDoc"][0]["name"], json!("张三"));
    assert_eq!(out[1]["authorDoc"], json!([]), "未匹配时输出空数组");

    drop(db);
    let _ = std::fs::remove_file(path);
}

#[test]
fn lookup_matches_array_local_field() {
    let (path, db) = setup("lookup_array");
    insert(
        &db,
        "tags",
        vec![json!({"_id": "t1", "label": "rust"}), json!({"_id": "t2", "label": "db"})],
    );
    insert(&db, "posts", vec![json!({"title": "p", "tagIds": ["t1", "t2"]})]);

    let out = agg(
        &db,
        "posts",
        json!([{"$lookup": {"from": "tags", "localField": "tagIds", "foreignField": "_id", "as": "tags"}}]),
    );
    assert_eq!(out[0]["tags"].as_array().unwrap().len(), 2);

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 子管道形式明确报错,不静默返回空。
#[test]
fn lookup_pipeline_form_rejected() {
    let (path, db) = setup("lookup_pipe");
    insert(&db, "books", vec![json!({"title": "b"})]);
    let err = run(
        &db,
        "books",
        &json!([{"$lookup": {"from": "x", "pipeline": [], "as": "y"}}]),
    )
    .unwrap_err();
    assert_eq!(err.code, 40324);

    drop(db);
    let _ = std::fs::remove_file(path);
}

// ---------------------------- $facet ----------------------------

#[test]
fn facet_runs_sub_pipelines() {
    let (path, db) = setup("facet");
    insert(
        &db,
        "sales",
        vec![
            json!({"item": "a", "qty": 3}),
            json!({"item": "b", "qty": 1}),
            json!({"item": "a", "qty": 2}),
        ],
    );

    let out = agg(
        &db,
        "sales",
        json!([{"$facet": {
            "byItem": [{"$group": {"_id": "$item", "total": {"$sum": "$qty"}}}, {"$sort": {"_id": 1}}],
            "top": [{"$match": {"qty": {"$gte": 2}}}, {"$count": "n"}]
        }}]),
    );
    assert_eq!(out.len(), 1, "$facet 输出单篇文档");
    assert_eq!(out[0]["byItem"][0]["total"], json!(5));
    assert_eq!(out[0]["top"][0]["n"], json!(2));

    drop(db);
    let _ = std::fs::remove_file(path);
}

// ---------------------------- $bucket ----------------------------

#[test]
fn bucket_groups_by_range() {
    let (path, db) = setup("bucket");
    insert(
        &db,
        "users",
        vec![
            json!({"age": 5}),
            json!({"age": 15}),
            json!({"age": 19}),
            json!({"age": 45}),
        ],
    );

    let out = agg(
        &db,
        "users",
        json!([{"$bucket": {
            "groupBy": "$age",
            "boundaries": [0, 18, 65],
            "output": {"n": {"$sum": 1}, "maxAge": {"$max": "$age"}}
        }}]),
    );
    assert_eq!(out.len(), 2);
    assert_eq!(out[0]["_id"], json!(0));
    assert_eq!(out[0]["n"], json!(2), "5 与 15 都落在 [0, 18)");
    assert_eq!(out[0]["maxAge"], json!(15));
    assert_eq!(out[1]["_id"], json!(18));
    assert_eq!(out[1]["n"], json!(2), "19 与 45 都落在 [18, 65)");
    assert_eq!(out[1]["maxAge"], json!(45));

    drop(db);
    let _ = std::fs::remove_file(path);
}

#[test]
fn bucket_default_catches_outliers() {
    let (path, db) = setup("bucket_default");
    insert(&db, "users", vec![json!({"age": 120})]);
    let out = agg(
        &db,
        "users",
        json!([{"$bucket": {"groupBy": "$age", "boundaries": [0, 18, 65], "default": "other"}}]),
    );
    assert_eq!(out[0]["_id"], json!("other"));

    drop(db);
    let _ = std::fs::remove_file(path);
}

// ---------------------------- $replaceRoot / $count / $unset ----------------------------

#[test]
fn replace_root_promotes_subdocument() {
    let (path, db) = setup("replace_root");
    insert(&db, "orders", vec![json!({"id": 1, "ship": {"city": "上海"}})]);
    let out = agg(&db, "orders", json!([{"$replaceRoot": {"newRoot": "$ship"}}]));
    assert_eq!(out[0], json!({"city": "上海"}));

    drop(db);
    let _ = std::fs::remove_file(path);
}

#[test]
fn replace_with_accepts_expression() {
    let (path, db) = setup("replace_with");
    insert(&db, "orders", vec![json!({"a": 1, "b": 2})]);
    let out = agg(&db, "orders", json!([{"$replaceWith": {"sum": {"$add": ["$a", "$b"]}}}]));
    assert_eq!(out[0], json!({"sum": 3}));

    drop(db);
    let _ = std::fs::remove_file(path);
}

#[test]
fn count_stage_yields_single_doc() {
    let (path, db) = setup("count_stage");
    insert(&db, "t", vec![json!({"a": 1}), json!({"a": 2})]);
    assert_eq!(
        agg(&db, "t", json!([{"$count": "total"}])),
        vec![json!({"total": 2})]
    );

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 空输入时 `$count` 产出空结果集(不是 `{total: 0}`)。
#[test]
fn count_stage_empty_input() {
    let (path, db) = setup("count_empty");
    insert(&db, "t", vec![json!({"a": 1})]);
    assert_eq!(
        agg(&db, "t", json!([{"$match": {"a": 99}}, {"$count": "n"}])),
        Vec::<Value>::new()
    );

    drop(db);
    let _ = std::fs::remove_file(path);
}

#[test]
fn unset_stage_removes_fields() {
    let (path, db) = setup("unset_stage");
    insert(&db, "t", vec![json!({"a": 1, "b": 2, "c": 3})]);
    let out = agg(&db, "t", json!([{"$unset": ["b", "c"]}]));
    assert_eq!(out[0], json!({"_id": out[0]["_id"], "a": 1}));

    drop(db);
    let _ = std::fs::remove_file(path);
}

// ---------------------------- $project 表达式 ----------------------------

#[test]
fn project_computes_fields() {
    let (path, db) = setup("project_expr");
    insert(&db, "t", vec![json!({"price": 100, "qty": 3, "name": "apple"})]);
    let out = agg(
        &db,
        "t",
        json!([{"$project": {
            "_id": 0,
            "name": 1,
            "total": {"$multiply": ["$price", "$qty"]},
            "label": {"$concat": ["$name", "!"]}
        }}]),
    );
    assert_eq!(out[0]["total"], json!(300));
    assert_eq!(out[0]["label"], json!("apple!"));
    assert!(out[0].get("_id").is_none(), "_id: 0 应排除主键");
    assert!(out[0].get("price").is_none(), "未列入的字段不输出");

    drop(db);
    let _ = std::fs::remove_file(path);
}

#[test]
fn project_exclusion_mode() {
    let (path, db) = setup("project_exclude");
    insert(&db, "t", vec![json!({"a": 1, "b": 2})]);
    let out = agg(&db, "t", json!([{"$project": {"b": 0}}]));
    assert_eq!(out[0]["a"], json!(1));
    assert!(out[0].get("b").is_none());

    drop(db);
    let _ = std::fs::remove_file(path);
}

#[test]
fn project_mixing_modes_rejected() {
    let (path, db) = setup("project_mixing");
    insert(&db, "t", vec![json!({"a": 1, "b": 2})]);
    let err = run(&db, "t", &json!([{"$project": {"a": 1, "b": 0}}])).unwrap_err();
    assert_eq!(err.code, 2);

    drop(db);
    let _ = std::fs::remove_file(path);
}

#[test]
fn add_fields_computes_and_overwrites() {
    let (path, db) = setup("add_fields");
    insert(&db, "t", vec![json!({"a": 1, "b": 2})]);
    let out = agg(
        &db,
        "t",
        json!([{"$addFields": {"sum": {"$add": ["$a", "$b"]}, "a": 100}}]),
    );
    assert_eq!(out[0]["sum"], json!(3));
    assert_eq!(out[0]["a"], json!(100));

    drop(db);
    let _ = std::fs::remove_file(path);
}

// ---------------------------- 前置 $match 下推 ----------------------------

/// 首个 `$match` 下推到存储层后,结果必须与内存过滤完全一致。
#[test]
fn leading_match_pushes_down_correctly() {
    let (path, db) = setup("leading_match");
    insert(
        &db,
        "t",
        vec![json!({"n": 1}), json!({"n": 2}), json!({"n": 3})],
    );
    let out = agg(
        &db,
        "t",
        json!([{"$match": {"n": {"$gte": 2}}}, {"$count": "n"}]),
    );
    assert_eq!(out[0]["n"], json!(2));

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 未知阶段必须报 `40324`,不静默跳过。
#[test]
fn unknown_stage_errors() {
    let (path, db) = setup("unknown_stage");
    insert(&db, "t", vec![json!({"a": 1})]);
    let err = run(&db, "t", &json!([{"$graphLookup": {}}])).unwrap_err();
    assert_eq!(err.code, 40324);

    drop(db);
    let _ = std::fs::remove_file(path);
}

