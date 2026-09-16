//! 单文件复制粘贴迁移验证(M5)。
//!
//! 核心承诺:整个数据库就是一个 `.nodb` 文件,可直接复制粘贴迁移。
//! 本测试将这一承诺固化为自动化回归:
//! 创建 → 插入多集合多文档 → 关闭 → 复制文件副本 → 打开副本校验数据完整一致。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use nosqlite::query::Filter;
use nosqlite::{Database, Document};

/// 全局自增计数器,用于生成唯一的临时文件路径。
static COUNTER: AtomicU32 = AtomicU32::new(0);

/// 生成唯一的临时 `.nodb` 测试文件路径。
fn temp_nodb(name: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!(
        "nosqlite_test_{}_{}_{}.nodb",
        name,
        std::process::id(),
        n
    ))
}

/// 清理测试遗留文件。
fn cleanup(path: &Path) {
    let _ = fs::remove_file(path);
}

/// 辅助:解析 filter。
fn filter(json: &str) -> Filter {
    let v: serde_json::Value = serde_json::from_str(json).expect("测试 JSON 应合法");
    Filter::from_value(&v).expect("filter 解析应成功")
}

/// 辅助:查询集合全部文档并返回按 _id 排序的 (id, json) 列表。
fn dump(db: &Database, collection: &str) -> Vec<(String, String)> {
    let mut docs = db.find(collection, &filter("null")).expect("查询应成功");
    docs.sort_by(|a, b| a["_id"].as_str().cmp(&b["_id"].as_str()));
    docs.into_iter()
        .map(|d| {
            let id = d["_id"].as_str().unwrap().to_string();
            let json = serde_json::to_string(&d).expect("序列化应成功");
            (id, json)
        })
        .collect()
}

/// 复制粘贴后,副本数据与原库完整一致(多集合、多文档、自动/显式 _id)。
#[test]
fn copy_paste_preserves_all_data() {
    let src = temp_nodb("copy_src");
    let dst = temp_nodb("copy_dst");

    // 1. 创建数据库并插入多集合多文档(混合自动/显式 _id)
    {
        let db = Database::create(&src).expect("创建数据库应成功");

        let mut u1 = Document::from_json(r#"{"_id":"user_001","name":"张三","age":30}"#).unwrap();
        let mut u2 = Document::from_json(r#"{"name":"李四","age":25}"#).unwrap();
        let mut u3 = Document::from_json(
            r#"{"_id":"user_003","name":"王五","age":18,"addr":{"city":"上海"}}"#,
        )
        .unwrap();
        db.insert("users", &mut u1).expect("插入 u1 应成功");
        db.insert("users", &mut u2).expect("插入 u2 应成功");
        db.insert("users", &mut u3).expect("插入 u3 应成功");

        let mut o1 = Document::from_json(r#"{"_id":"order_001","amount":100}"#).unwrap();
        db.insert("orders", &mut o1).expect("插入 o1 应成功");

        // 快照:关闭前 dump 原库内容
        let expected_users = dump(&db, "users");
        let expected_orders = dump(&db, "orders");

        db.close().expect("关闭应成功");
        drop(db); // Windows 下 redb 持有独占锁,必须先释放句柄

        // 2. 复制粘贴:把 .nodb 文件复制为副本(模拟用户复制粘贴迁移)
        fs::copy(&src, &dst).expect("复制文件应成功");

        // 3. 打开副本,校验数据完整一致
        let db2 = Database::open(&dst).expect("打开副本应成功");
        let copied_users = dump(&db2, "users");
        let copied_orders = dump(&db2, "orders");

        assert_eq!(copied_users, expected_users, "users 集合数据应完整一致");
        assert_eq!(copied_orders, expected_orders, "orders 集合数据应完整一致");

        db2.close().expect("关闭应成功");
        drop(db2);
    }

    cleanup(&src);
    cleanup(&dst);
}

/// 复制粘贴后,查询/更新/删除等操作在副本上仍可正常工作(副本是独立完整数据库)。
#[test]
fn copy_paste_copy_is_fully_usable() {
    let src = temp_nodb("copy_src2");
    let dst = temp_nodb("copy_dst2");

    {
        let db = Database::create(&src).expect("创建数据库应成功");
        let mut a = Document::from_json(r#"{"_id":"a","v":1}"#).unwrap();
        let mut b = Document::from_json(r#"{"_id":"b","v":2}"#).unwrap();
        db.insert("t", &mut a).expect("插入 a 应成功");
        db.insert("t", &mut b).expect("插入 b 应成功");
        db.close().expect("关闭应成功");
        drop(db);

        fs::copy(&src, &dst).expect("复制文件应成功");
        let db2 = Database::open(&dst).expect("打开副本应成功");

        // count 正常
        assert_eq!(
            db2.count("t", &filter("null")).unwrap(),
            2,
            "副本 count 应正常"
        );

        // 更新副本上的文档,不影响原库
        let upd = serde_json::from_str::<serde_json::Value>(r#"{"$set":{"v":99}}"#).unwrap();
        let up = nosqlite::query::Update::from_value(&upd).unwrap();
        db2.update("t", &filter(r#"{"_id":"a"}"#), &up)
            .expect("副本更新应成功");
        let docs = db2.find("t", &filter(r#"{"_id":"a"}"#)).unwrap();
        assert_eq!(docs[0]["v"], 99, "副本更新应生效");

        // 删除副本上的文档,不影响原库
        db2.delete("t", &filter(r#"{"_id":"b"}"#))
            .expect("副本删除应成功");
        assert_eq!(
            db2.count("t", &filter("null")).unwrap(),
            1,
            "副本删除应生效"
        );

        db2.close().expect("关闭应成功");
        drop(db2);

        // 原库仍保持 2 条,未被副本操作影响(副本是独立数据库)
        let db3 = Database::open(&src).expect("重新打开原库应成功");
        assert_eq!(
            db3.count("t", &filter("null")).unwrap(),
            2,
            "原库不应受影响"
        );
        db3.close().expect("关闭应成功");
        drop(db3);
    }

    cleanup(&src);
    cleanup(&dst);
}
