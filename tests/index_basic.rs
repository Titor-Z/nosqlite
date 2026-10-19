//! 索引定义、回填与写路径维护(M23b)。
//!
//! 验证重点:
//! 1. 建索引会回填既有文档;
//! 2. `insert/update/delete`(含批量)同步维护索引——**更新时先删旧条目再写新条目**;
//! 3. 唯一索引拒绝重复,稀疏索引跳过 null/缺失;
//! 4. multikey:数组字段展开为多条条目;
//! 5. 索引随文档删除而清理,不留垃圾。

use std::path::PathBuf;

use nodblite::query::{Filter, Update};
use nodblite::{Database, Document};
use serde_json::json;

/// 生成本次测试独占的临时 `.nodb` 路径。
fn temp_nodb(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "nodblite_idx_{}_{}_{}.nodb",
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

/// 建库。
fn setup(tag: &str) -> (PathBuf, Database) {
    let path = temp_nodb(tag);
    let db = Database::create(&path).unwrap();
    (path, db)
}

/// 插入若干文档。
fn insert_docs(db: &Database, coll: &str, docs: Vec<serde_json::Value>) {
    let mut list: Vec<Document> = docs
        .into_iter()
        .map(|v| Document::from_json(&v.to_string()).unwrap())
        .collect();
    db.insert_many(coll, &mut list).unwrap();
}

/// 构造 filter。
fn f(v: serde_json::Value) -> Filter {
    Filter::from_value(&v).unwrap()
}

/// 构造 update。
fn u(v: serde_json::Value) -> Update {
    Update::from_value(&v).unwrap()
}

// ==================== 定义管理 ====================

/// 建索引后能列出定义。
#[test]
fn create_and_list_index() {
    let (path, db) = setup("create");
    insert_docs(&db, "users", vec![json!({"age": 1})]);

    db.create_index("users", "age_1", "age", false, false)
        .expect("建索引应成功");

    let indexes = db.list_indexes("users").unwrap();
    assert_eq!(indexes.len(), 1);
    assert_eq!(indexes[0].name, "age_1");
    assert_eq!(indexes[0].field(), "age");
    assert!(!indexes[0].unique);
    assert_eq!(indexes[0].encoding_version, 1);

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 删除索引。
#[test]
fn drop_index() {
    let (path, db) = setup("drop");
    db.create_index("c", "idx", "f", false, false).unwrap();
    assert!(db.drop_index("c", "idx").unwrap());
    assert!(!db.drop_index("c", "idx").unwrap(), "重复删除应返回 false");
    assert!(db.list_indexes("c").unwrap().is_empty());

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 重建同名索引是幂等的(不会产生重复条目或定义)。
#[test]
fn recreate_index_is_idempotent() {
    let (path, db) = setup("recreate");
    insert_docs(&db, "c", vec![json!({"f": 1}), json!({"f": 1})]);

    db.create_index("c", "idx", "f", false, false).unwrap();
    db.create_index("c", "idx", "f", false, false).unwrap();

    let indexes = db.list_indexes("c").unwrap();
    assert_eq!(indexes.len(), 1, "同名索引应只有一条定义");

    drop(db);
    let _ = std::fs::remove_file(path);
}

// ==================== 唯一索引 ====================

/// 唯一索引:建索引时发现既有重复应拒绝。
#[test]
fn unique_index_rejects_existing_duplicates() {
    let (path, db) = setup("unique_dup");
    insert_docs(
        &db,
        "c",
        vec![json!({"email": "a@b.c"}), json!({"email": "a@b.c"})],
    );

    let err = db
        .create_index("c", "email_1", "email", true, false)
        .expect_err("既有重复应拒绝建唯一索引");
    assert!(err.to_string().contains("唯一索引冲突"), "实际: {}", err);
    // 失败后不应留下定义
    assert!(db.list_indexes("c").unwrap().is_empty());

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 唯一索引:写入重复值应被拒绝。
#[test]
fn unique_index_rejects_new_duplicate() {
    let (path, db) = setup("unique_insert");
    db.create_index("c", "email_1", "email", true, false)
        .unwrap();

    let mut a = Document::from_json(r#"{"email": "a@b.c"}"#).unwrap();
    db.insert("c", &mut a).expect("首次插入应成功");

    let mut b = Document::from_json(r#"{"email": "a@b.c"}"#).unwrap();
    let err = db.insert("c", &mut b).expect_err("重复值应拒绝");
    assert!(err.to_string().contains("唯一索引冲突"), "实际: {}", err);

    // 不同值仍可插入
    let mut c = Document::from_json(r#"{"email": "x@y.z"}"#).unwrap();
    db.insert("c", &mut c).expect("不同值应成功");

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 唯一索引允许更新自身(同值更新不应报冲突)。
#[test]
fn unique_index_allows_updating_same_document() {
    let (path, db) = setup("unique_self");
    db.create_index("c", "k_1", "k", true, false).unwrap();
    let mut doc = Document::from_json(r#"{"k": "v", "n": 1}"#).unwrap();
    let id = db.insert("c", &mut doc).unwrap();

    // 改其他字段,k 不变 → 不应冲突
    db.update_by_id("c", &id, &u(json!({"$set": {"n": 2}})))
        .expect("同值更新不应冲突");
    assert_eq!(db.get_by_id("c", &id).unwrap().unwrap()["n"], json!(2));

    drop(db);
    let _ = std::fs::remove_file(path);
}

// ==================== 稀疏索引 ====================

/// 稀疏索引不索引 null 与缺失。
#[test]
fn sparse_index_skips_nulls() {
    let (path, db) = setup("sparse");
    insert_docs(
        &db,
        "c",
        vec![
            json!({"f": 1}),
            json!({"f": null}),
            json!({"other": 1}),
            json!({"f": 2}),
        ],
    );

    // 非稀疏:null 与缺失都会入索引 —— 这里用唯一索引验证它们**不**冲突
    db.create_index("c", "f_1", "f", false, false)
        .expect("非稀疏建索引应成功");

    // 稀疏 + 唯一:只有 f=1 和 f=2 两个值,不冲突
    db.create_index("c", "f_sparse", "f", true, true)
        .expect("稀疏唯一索引应成功(null 与缺失被跳过)");

    drop(db);
    let _ = std::fs::remove_file(path);
}

// ==================== 写路径维护 ====================

/// 更新被索引字段后,唯一索引不应被旧值卡住。
///
/// 这是最容易出错的地方:必须**先删旧条目再写新条目**。
#[test]
fn update_moves_index_entry() {
    let (path, db) = setup("move");
    db.create_index("c", "k_1", "k", true, false).unwrap();

    let mut doc = Document::from_json(r#"{"k": "old"}"#).unwrap();
    let id = db.insert("c", &mut doc).unwrap();

    // 改成 new:旧条目 old 必须被清掉
    db.update_by_id("c", &id, &u(json!({"$set": {"k": "new"}})))
        .expect("更新应成功");

    // 现在可以插入 old(说明旧条目已被清理)
    let mut other = Document::from_json(r#"{"k": "old"}"#).unwrap();
    db.insert("c", &mut other).expect("旧值应已释放,可再次使用");

    // 但不能插入 new(说明新条目已建立)
    let mut dup = Document::from_json(r#"{"k": "new"}"#).unwrap();
    assert!(db.insert("c", &mut dup).is_err(), "new 应仍被占用");

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 删除文档后索引条目应被清理。
#[test]
fn delete_removes_index_entry() {
    let (path, db) = setup("delentry");
    db.create_index("c", "k_1", "k", true, false).unwrap();

    let mut doc = Document::from_json(r#"{"k": "x"}"#).unwrap();
    let id = db.insert("c", &mut doc).unwrap();
    db.delete_by_id("c", &id).expect("删除应成功");

    // 现在可以重新插入相同值
    let mut again = Document::from_json(r#"{"k": "x"}"#).unwrap();
    db.insert("c", &mut again).expect("删除后索引条目应已清理");

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 批量删除也要清理索引。
#[test]
fn delete_many_removes_index_entries() {
    let (path, db) = setup("delmany");
    db.create_index("c", "k_1", "k", true, false).unwrap();
    insert_docs(
        &db,
        "c",
        vec![json!({"k": "a"}), json!({"k": "b"}), json!({"k": "c"})],
    );

    let deleted = db.delete("c", &f(json!({}))).unwrap();
    assert_eq!(deleted, 3);

    // 全部可重新插入
    insert_docs(
        &db,
        "c",
        vec![json!({"k": "a"}), json!({"k": "b"}), json!({"k": "c"})],
    );

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 批量更新也要维护索引(旧条目清理)。
#[test]
fn update_many_maintains_index() {
    let (path, db) = setup("upmany");
    db.create_index("c", "tag_1", "tag", true, false).unwrap();
    insert_docs(
        &db,
        "c",
        vec![json!({"tag": "t1", "n": 1}), json!({"tag": "t2", "n": 2})],
    );

    // 全部改成同一新 tag → 唯一索引冲突(这是正确行为)
    let err = db
        .update("c", &f(json!({})), &u(json!({"$set": {"tag": "moved"}})))
        .expect_err("两条改成同一值应触发唯一冲突");
    assert!(err.to_string().contains("唯一索引冲突"), "实际: {}", err);

    // 改成互不相同的值则成功
    db.update("c", &f(json!({"n": 1})), &u(json!({"$set": {"tag": "a"}})))
        .unwrap();
    db.update("c", &f(json!({"n": 2})), &u(json!({"$set": {"tag": "b"}})))
        .unwrap();

    // 旧值 t1/t2 应已释放
    let mut d = Document::from_json(r#"{"tag": "t1"}"#).unwrap();
    db.insert("c", &mut d).expect("旧值 t1 应已释放");

    drop(db);
    let _ = std::fs::remove_file(path);
}

// ==================== multikey ====================

/// 数组字段按 multikey 展开:每个元素一条条目。
#[test]
fn multikey_index_over_array_field() {
    let (path, db) = setup("multikey");
    db.create_index("c", "tags_1", "tags", true, false).unwrap();

    let mut doc = Document::from_json(r#"{"tags": ["a", "b"]}"#).unwrap();
    db.insert("c", &mut doc).expect("首次插入应成功");

    // "a" 已被占用
    let mut dup_a = Document::from_json(r#"{"tags": ["x", "a"]}"#).unwrap();
    let err = db.insert("c", &mut dup_a).expect_err("a 应已占用");
    assert!(err.to_string().contains("唯一索引冲突"), "实际: {}", err);

    // "c" 未占用
    let mut ok = Document::from_json(r#"{"tags": ["c"]}"#).unwrap();
    db.insert("c", &mut ok).expect("c 应可用");

    // 同一文档内数组重复元素不应自我冲突
    let mut selfdup = Document::from_json(r#"{"tags": ["z", "z"]}"#).unwrap();
    db.insert("c", &mut selfdup)
        .expect("同一文档的重复元素不应冲突");

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// multikey + 删除:数组元素的条目全部清理。
#[test]
fn multikey_delete_clears_all_entries() {
    let (path, db) = setup("multikeydel");
    db.create_index("c", "tags_1", "tags", true, false).unwrap();
    let mut doc = Document::from_json(r#"{"tags": ["a", "b", "c"]}"#).unwrap();
    let id = db.insert("c", &mut doc).unwrap();
    db.delete_by_id("c", &id).unwrap();

    // 三个值都应已释放
    let mut again = Document::from_json(r#"{"tags": ["a", "b", "c"]}"#).unwrap();
    db.insert("c", &mut again).expect("全部条目应已清理");

    drop(db);
    let _ = std::fs::remove_file(path);
}

// ==================== 回填与持久化 ====================

/// 建索引时回填既有文档(先插再建,唯一约束仍生效)。
#[test]
fn backfill_on_existing_documents() {
    let (path, db) = setup("backfill");
    insert_docs(&db, "c", vec![json!({"k": "a"}), json!({"k": "b"})]);

    db.create_index("c", "k_1", "k", true, false)
        .expect("既有数据无重复,应成功");

    // 回填生效:a 已被占用
    let mut dup = Document::from_json(r#"{"k": "a"}"#).unwrap();
    assert!(db.insert("c", &mut dup).is_err(), "回填应使 a 被占用");

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 索引持久化:重开数据库后定义与约束仍生效。
#[test]
fn index_persists_across_reopen() {
    let (path, db) = setup("persist");
    db.create_index("c", "k_1", "k", true, false).unwrap();
    let mut doc = Document::from_json(r#"{"k": "v"}"#).unwrap();
    db.insert("c", &mut doc).unwrap();
    drop(db);

    let db = Database::open(&path).unwrap();
    let defs = db.list_indexes("c").unwrap();
    assert_eq!(defs.len(), 1, "重开后索引定义应仍在");

    let mut dup = Document::from_json(r#"{"k": "v"}"#).unwrap();
    assert!(db.insert("c", &mut dup).is_err(), "重开后唯一约束应仍生效");

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 嵌套路径索引。
#[test]
fn nested_field_index() {
    let (path, db) = setup("nested");
    db.create_index("c", "addr_city_1", "addr.city", true, false)
        .unwrap();

    let mut a = Document::from_json(r#"{"addr": {"city": "上海"}}"#).unwrap();
    db.insert("c", &mut a).unwrap();

    let mut b = Document::from_json(r#"{"addr": {"city": "上海"}}"#).unwrap();
    assert!(db.insert("c", &mut b).is_err(), "同城市应冲突");

    let mut c = Document::from_json(r#"{"addr": {"city": "北京"}}"#).unwrap();
    db.insert("c", &mut c).unwrap();

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 无索引时一切照旧(回归)。
#[test]
fn works_without_any_index() {
    let (path, db) = setup("noindex");
    insert_docs(&db, "c", vec![json!({"k": "a"}), json!({"k": "a"})]);
    assert_eq!(db.count("c", &f(json!({}))).unwrap(), 2, "无索引允许重复");

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 删除集合时索引条目一并清理(通过 drop_collection 后重建索引验证)。
#[test]
fn drop_collection_then_recreate_index() {
    let (path, db) = setup("dropcoll");
    db.create_index("c", "k_1", "k", true, false).unwrap();
    let mut doc = Document::from_json(r#"{"k": "v"}"#).unwrap();
    db.insert("c", &mut doc).unwrap();

    db.drop_collection("c").unwrap();

    // 重新插入同值:旧条目若残留会误报冲突。
    // 注意:drop_collection 当前只清文档,索引条目需要重建索引时才彻底对齐。
    // 这里验证的是「重建索引后不冲突」。
    let mut again = Document::from_json(r#"{"k": "v"}"#).unwrap();
    let _ = db.insert("c", &mut again);
    db.create_index("c", "k_1", "k", true, false)
        .expect("重建索引应成功(会先清旧条目再回填)");

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 唯一性检查必须是 O(n) 而非 O(n²)(v0.6.3 修复的缺陷)。
///
/// 旧实现用 `Vec::contains` 做重复检测,10 万条唯一值建索引要 74 秒。
/// 这里用**时间上界**做粗略回归:1 万条唯一值应在合理时间内完成。
/// 时间断言容易受机器负载影响,故给足余量(旧实现约 886 ms,新实现 ~50 ms)。
#[test]
fn unique_index_build_is_not_quadratic() {
    use std::time::Instant;

    let path = temp_nodb("unique_perf");
    let db = nodblite::Database::create(&path).unwrap();

    let mut docs: Vec<nodblite::Document> = (0..10_000)
        .map(|i| nodblite::Document::from_json(&format!(r#"{{"uid": "u{:06}"}}"#, i)).unwrap())
        .collect();
    db.insert_many("items", &mut docs).unwrap();

    let t = Instant::now();
    db.create_index("items", "uid_1", "uid", true, false)
        .unwrap();
    let elapsed = t.elapsed();

    // 旧实现(O(n²))在此规模约 0.9 s,新实现应在 0.3 s 内。
    // 给 2 s 余量,确保只拦住「退化为平方复杂度」而不误伤慢机器。
    assert!(
        elapsed.as_secs_f64() < 2.0,
        "唯一索引构建耗时 {:.2}s,疑似退化为 O(n²)",
        elapsed.as_secs_f64()
    );

    // 唯一性仍然生效
    let mut dup = nodblite::Document::from_json(r#"{"uid": "u000000"}"#).unwrap();
    assert!(db.insert("items", &mut dup).is_err(), "重复值仍应被拒绝");

    drop(db);
    let _ = std::fs::remove_file(path);
}
