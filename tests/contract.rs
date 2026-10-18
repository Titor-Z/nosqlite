//! 跨语言契约(v0.2.0 阶段 0):错误码、字段序保持、Extended JSON。
//!
//! 这三样是 C ABI 与 MongoDB wire 协议**共用**的基础,
//! 一旦发布就不能随意改动,故单独立文件锁死。

use std::path::PathBuf;

use nosqlite::query::{
    Filter, Projection, SortSpec, Update, as_number_double, as_number_long, as_oid,
};
use nosqlite::{Database, Document, ErrorCode};
use serde_json::{Value, json};

/// 生成本次测试独占的临时 `.nodb` 路径。
fn temp_nodb(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "nosqlite_contract_{}_{}_{}.nodb",
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

// ==================== ① 字段序保持 ====================

/// 写入顺序在读取后保持不变(不能按字典序重排)。
#[test]
fn field_order_is_preserved() {
    let path = temp_nodb("order");
    let db = Database::create(&path).unwrap();

    let mut doc = Document::from_json(r#"{"zebra": 1, "apple": 2, "name": "张三"}"#).unwrap();
    let id = db.insert("c", &mut doc).unwrap();

    let out = db.get_by_id("c", &id).unwrap().unwrap();
    let keys: Vec<&String> = out.keys().collect();
    assert_eq!(
        keys,
        vec!["zebra", "apple", "name", "_id"],
        "字段序应保持写入顺序,实际: {:?}",
        keys
    );

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 排序/投影等操作不破坏字段序。
#[test]
fn field_order_survives_projection() {
    let path = temp_nodb("order_proj");
    let db = Database::create(&path).unwrap();
    let mut docs: Vec<Document> = vec![Document::from_json(r#"{"c": 1, "a": 2, "b": 3}"#).unwrap()];
    db.insert_many("c", &mut docs).unwrap();

    let proj = Projection::from_value(&json!({"c": 1, "b": 1})).unwrap();
    let out = db
        .find_with_options(
            "c",
            &Filter::match_all(),
            &nosqlite::query::FindOptions {
                sort: None,
                projection: Some(proj),
                skip: 0,
                limit: None,
            },
        )
        .unwrap();
    let keys: Vec<&String> = out[0].keys().collect();
    // `_id` 恒在最前(MongoDB 惯例),其余字段保持投影声明顺序
    assert_eq!(keys, vec!["_id", "c", "b"], "投影后顺序不对: {:?}", keys);

    drop(db);
    let _ = std::fs::remove_file(path);
}

// ==================== ② 错误码 ====================

/// 主键冲突 → `DUPLICATE_KEY`。
#[test]
fn duplicate_key_code() {
    let path = temp_nodb("dup");
    let db = Database::create(&path).unwrap();

    let mut a = Document::from_json(r#"{"_id": "x", "n": 1}"#).unwrap();
    db.insert("c", &mut a).unwrap();
    let err = db
        .insert(
            "c",
            &mut Document::from_json(r#"{"_id": "x", "n": 2}"#).unwrap(),
        )
        .expect_err("重复主键应报错");
    assert_eq!(err.code(), ErrorCode::DuplicateKey);
    assert_eq!(err.code().as_str(), "DUPLICATE_KEY");
    // 中文文案保留,便于 CLI 展示
    assert!(err.to_string().contains("主键冲突"));

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// filter 非法 → `INVALID_FILTER`。
#[test]
fn invalid_filter_code() {
    let err = Filter::from_value(&json!([])).expect_err("顶层非对象应报错");
    let db_err: nosqlite::DatabaseError = err.into();
    assert_eq!(db_err.code(), ErrorCode::InvalidFilter);
    assert_eq!(db_err.code().as_str(), "INVALID_FILTER");
}

/// 更新非法 → `INVALID_UPDATE`。
#[test]
fn invalid_update_code() {
    let err = Update::from_value(&json!({"$pull": {"a": 1}})).expect_err("未知操作符应报错");
    let db_err: nosqlite::DatabaseError = err.into();
    assert_eq!(db_err.code(), ErrorCode::InvalidUpdate);
    assert_eq!(db_err.code().as_str(), "INVALID_UPDATE");
}

/// 排序非法 → `INVALID_SORT`。
#[test]
fn invalid_sort_code() {
    let err = SortSpec::from_value(&json!({"a": 2})).expect_err("方向非法应报错");
    let db_err: nosqlite::DatabaseError = err.into();
    assert_eq!(db_err.code(), ErrorCode::InvalidSort);
}

/// 投影非法 → `INVALID_PROJECTION`。
#[test]
fn invalid_projection_code() {
    let err = Projection::from_value(&json!({"a": 1, "b": 0})).expect_err("混用应报错");
    let db_err: nosqlite::DatabaseError = err.into();
    assert_eq!(db_err.code(), ErrorCode::InvalidProjection);
}

/// 路径后缀非法 → `INVALID_PATH`。
#[test]
fn invalid_path_code() {
    let path = std::env::temp_dir().join(format!("nosqlite_bad_{}.db", std::process::id()));
    let err = Database::create(&path).expect_err("非法后缀应报错");
    assert_eq!(err.code(), ErrorCode::InvalidPath);
}

/// 错误码的字符串与整数形式稳定(跨语言契约)。
#[test]
fn error_code_representations_are_stable() {
    assert_eq!(ErrorCode::DuplicateKey.as_i32(), 1);
    assert_eq!(ErrorCode::InvalidPath.as_i32(), 2);
    assert_eq!(ErrorCode::InvalidFormat.as_i32(), 3);
    assert_eq!(ErrorCode::VersionMismatch.as_i32(), 4);
    assert_eq!(ErrorCode::InvalidCollection.as_i32(), 5);
    assert_eq!(ErrorCode::InvalidFilter.as_i32(), 6);
    assert_eq!(ErrorCode::InvalidUpdate.as_i32(), 7);
    assert_eq!(ErrorCode::InvalidSort.as_i32(), 8);
    assert_eq!(ErrorCode::InvalidProjection.as_i32(), 9);
    assert_eq!(ErrorCode::InvalidDocument.as_i32(), 10);
    assert_eq!(ErrorCode::IoError.as_i32(), 11);
    assert_eq!(ErrorCode::Internal.as_i32(), 99);
    // 0 保留给成功,不能有错误码占用
    for code in [
        ErrorCode::DuplicateKey,
        ErrorCode::InvalidPath,
        ErrorCode::InvalidFormat,
        ErrorCode::VersionMismatch,
        ErrorCode::InvalidCollection,
        ErrorCode::InvalidFilter,
        ErrorCode::InvalidUpdate,
        ErrorCode::InvalidSort,
        ErrorCode::InvalidProjection,
        ErrorCode::InvalidDocument,
        ErrorCode::IoError,
        ErrorCode::Internal,
    ] {
        assert_ne!(code.as_i32(), 0, "0 必须保留给成功");
    }
}

/// 客户端错误与服务端错误的划分(供 HTTP/wire 映射)。
#[test]
fn error_code_client_server_split() {
    assert!(ErrorCode::DuplicateKey.is_client_error());
    assert!(ErrorCode::InvalidFilter.is_client_error());
    // IO / 内部错误属于服务端问题
    assert!(!ErrorCode::IoError.is_client_error());
    assert!(!ErrorCode::Internal.is_client_error());
}

// ==================== ③ Extended JSON ====================

/// `{"$oid": "..."}` 识别为 ObjectId 字符串。
#[test]
fn extended_json_oid_recognition() {
    assert_eq!(
        as_oid(&json!({"$oid": "6ad53bfa49caf1e813000000"})),
        Some("6ad53bfa49caf1e813000000")
    );
    // 带别的键不算
    assert_eq!(as_oid(&json!({"$oid": "x", "a": 1})), None);
    // 值不是字符串
    assert_eq!(as_oid(&json!({"$oid": 1})), None);
    // 普通字符串
    assert_eq!(as_oid(&json!("abc")), None);
}

/// `$oid` 与普通 24 位 hex 字符串**等价**(可互相查询)。
#[test]
fn extended_json_oid_equals_plain_string() {
    let filter = Filter::from_value(&json!({"_id": {"$oid": "6ad53bfa49caf1e813000000"}}))
        .expect("filter 应解析成功");
    let doc = json!({"_id": "6ad53bfa49caf1e813000000"})
        .as_object()
        .unwrap()
        .clone();
    assert!(filter.matches(&doc), "$oid 应等价于同值字符串");

    // 反向:用普通字符串查带 $oid 的文档
    let filter = Filter::from_value(&json!({"_id": "6ad53bfa49caf1e813000000"})).unwrap();
    let doc = json!({"_id": {"$oid": "6ad53bfa49caf1e813000000"}})
        .as_object()
        .unwrap()
        .clone();
    assert!(filter.matches(&doc));
}

/// `$numberLong` / `$numberDouble` 识别与比较。
#[test]
fn extended_json_numbers() {
    assert_eq!(
        as_number_long(&json!({"$numberLong": "123"})),
        Some(json!(123))
    );
    assert_eq!(
        as_number_double(&json!({"$numberDouble": "1.5"})),
        Some(json!(1.5))
    );
    // 非法数字
    assert_eq!(as_number_long(&json!({"$numberLong": "abc"})), None);
    // 普通数字不算包装
    assert_eq!(as_number_long(&json!(123)), None);

    // 与普通数字等价
    let filter = Filter::from_value(&json!({"n": {"$numberLong": "42"}})).unwrap();
    let doc = json!({"n": 42}).as_object().unwrap().clone();
    assert!(filter.matches(&doc));
}

/// `$oID` 包装可按字符串排序,不与对象混为一类。
#[test]
fn extended_json_oid_sorts_as_string() {
    let spec = SortSpec::from_value(&json!({"k": 1})).unwrap();
    let mut docs: Vec<_> = vec![json!({"k": {"$oid": "bbb"}}), json!({"k": {"$oid": "aaa"}})]
        .into_iter()
        .map(|v| v.as_object().unwrap().clone())
        .collect();
    spec.sort(&mut docs);
    assert_eq!(docs[0]["k"], json!({"$oid": "aaa"}));
}

/// 组合校验:一次比对 $oid 与 $numberLong 的 `$eq` 行为。
#[test]
fn extended_json_in_filter_operators() {
    let filter =
        Filter::from_value(&json!({"items": {"$elemMatch": {"v": {"$numberLong": "7"}}}})).unwrap();
    let doc = json!({"items": [{"v": 7}, {"v": 9}]})
        .as_object()
        .unwrap()
        .clone();
    assert!(filter.matches(&doc));
}

/// 端到端:`$oid` 可用于 upsert 的初始字段并原样存储。
#[test]
fn extended_json_persists_as_written() {
    let path = temp_nodb("ejson");
    let db = Database::create(&path).unwrap();

    let filter = Filter::from_value(&json!({"ref": {"$oid": "6ad53bfa49caf1e813000000"}})).unwrap();
    let update = Update::from_value(&json!({"$set": {"n": 1}})).unwrap();
    let result = db.update_or_insert("c", &filter, &update).unwrap();

    // filter 里的 $oid 包装作为初始值写入(存储层不改写)
    let id = match result {
        nosqlite::UpsertResult::Inserted(id) => id,
        other => panic!("应插入,实际: {:?}", other),
    };
    let doc = db.get_by_id("c", &id).unwrap().unwrap();
    let ref_value: &Value = &doc["ref"];
    assert_eq!(as_oid(ref_value), Some("6ad53bfa49caf1e813000000"));

    drop(db);
    let _ = std::fs::remove_file(path);
}
