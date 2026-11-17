//! 账号存储层(`admin.system.users`)验证。
//!
//! 关注三件事:
//! 1. 用户存在**真实集合** `admin.system.users` 里(MongoDB 客户端看得见);
//! 2. 密码只有摘要,读出来永远不带 `credentials`;
//! 3. 建库即写 `admin.system.version`,使 `admin` 库从创建那刻就存在。

use std::path::PathBuf;

use nodblite::auth::{USERS_COLLECTION, VERSION_COLLECTION, UserStore};
use nodblite::query::Filter;
use nodblite::{Database, Document};

fn temp_nodb(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "nodblite_auth_{}_{}_{}.nodb",
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

/// 新建的库没有任何账号 → 认证关闭(等同 MongoDB noauth)。
#[test]
fn fresh_db_has_no_users() {
    let path = temp_nodb("fresh");
    let db = Database::create(&path).unwrap();
    assert!(!UserStore::new(&db).has_users(), "新库不应有账号");

    // 但 `admin` 库已经存在:`system.version` 已写入
    let all = db.all_collections().unwrap();
    assert!(
        all.contains(&VERSION_COLLECTION.to_string()),
        "建库应写入 {}: {:?}",
        VERSION_COLLECTION,
        all
    );
    // 用户集合视图里看不到系统集合
    assert!(
        !db.collections().unwrap().iter().any(|n| n.starts_with("admin.")),
        "`collections()` 不应暴露 admin.* 系统集合"
    );

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 账号是真集合 + 凭据可校验 + 列表不泄漏摘要。
#[test]
fn user_is_real_collection_and_credentials_never_leak() {
    let path = temp_nodb("real");
    let db = Database::create(&path).unwrap();
    let store = UserStore::new(&db);

    store.create("alice", "pencil").unwrap();
    assert!(store.has_users());

    // 真集合:出现在 all_collections 里,并且能按普通集合查出文档
    assert!(db
        .all_collections()
        .unwrap()
        .contains(&USERS_COLLECTION.to_string()));
    let raw = db.find(USERS_COLLECTION, &Filter::match_all()).unwrap();
    assert_eq!(raw.len(), 1);
    assert!(
        raw[0].get("credentials").is_some(),
        "库里当然要存摘要(否则无法认证)"
    );

    // 对外列表必须剥掉摘要
    let listed = store.list();
    assert_eq!(listed.len(), 1);
    assert!(listed[0].get("credentials").is_none(), "list 不应带摘要");
    assert_eq!(listed[0].get("user").unwrap(), "alice");
    assert_eq!(listed[0].get("db").unwrap(), "admin");
    assert_eq!(listed[0].get("_id").unwrap(), "admin.alice");

    // 凭据能用来校验密码
    let cred = store.credential("alice").expect("应能取到凭据");
    assert_eq!(cred.iterations, nodblite::auth::ITERATIONS);
    assert!(store.credential("bob").is_none(), "不存在的用户应返回 None");

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 第一个账号是管理员(`root`),后续账号默认 `readWrite`。
#[test]
fn first_user_becomes_root() {
    let path = temp_nodb("roles");
    let db = Database::create(&path).unwrap();
    let store = UserStore::new(&db);

    store.create("admin_user", "p1").unwrap();
    store.create("app_user", "p2").unwrap();

    let listed = store.list();
    let role_of = |name: &str| -> String {
        listed
            .iter()
            .find(|d| d.get("user").and_then(|v| v.as_str()) == Some(name))
            .and_then(|d| d.get("roles"))
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .and_then(|r| r.get("role"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    assert_eq!(role_of("admin_user"), "root");
    assert_eq!(role_of("app_user"), "readWrite");

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 重复创建报错、改密码生效、删除后认证关闭。
#[test]
fn create_update_drop_lifecycle() {
    let path = temp_nodb("lifecycle");
    let db = Database::create(&path).unwrap();
    let store = UserStore::new(&db);

    store.create("alice", "old").unwrap();
    assert!(store.create("alice", "x").is_err(), "重复创建应报错");

    let before = store.credential("alice").unwrap().stored_key;
    store.set_password("alice", "new").unwrap();
    let after = store.credential("alice").unwrap().stored_key;
    assert_ne!(before, after, "改密码应换掉 StoredKey");

    assert!(store.set_password("ghost", "x").is_err(), "改不存在的用户应报错");

    assert!(store.drop_user("alice").unwrap());
    assert!(!store.drop_user("alice").unwrap(), "重复删除应返回 false");
    assert!(!store.has_users(), "删完账号后认证应关闭");

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 账号持久化:重开库后仍存在且能取到凭据。
#[test]
fn users_persist_across_reopen() {
    let path = temp_nodb("persist");
    {
        let db = Database::create(&path).unwrap();
        UserStore::new(&db).create("alice", "pencil").unwrap();
    }
    let db = Database::open(&path).unwrap();
    let store = UserStore::new(&db);
    assert!(store.has_users());
    assert!(store.credential("alice").is_some());

    drop(db);
    let _ = std::fs::remove_file(path);
}

/// 旧库(v0.6.x,没有 `admin.system.users` / 也没有 `system.version`)
/// 打开后依然可用:`has_users()` 为 false,`admin.system.version` 会按需存在。
#[test]
fn legacy_db_without_system_collections() {
    let path = temp_nodb("legacy");
    {
        let db = Database::create(&path).unwrap();
        // 模拟旧库:把建库时写入的 system.version 删掉
        let _ = db.delete_by_id(VERSION_COLLECTION, "featureCompatibilityVersion");
        let mut doc = Document::from_value(serde_json::json!({"n": 1})).unwrap();
        db.insert("items", &mut doc).unwrap();
    }
    let db = Database::open(&path).unwrap();
    assert!(!UserStore::new(&db).has_users(), "旧库无账号 → 认证关闭");
    assert_eq!(db.collections().unwrap(), vec!["items".to_string()]);

    // 旧库上也能直接建账号
    UserStore::new(&db).create("alice", "pencil").unwrap();
    assert!(UserStore::new(&db).has_users());

    drop(db);
    let _ = std::fs::remove_file(path);
}
