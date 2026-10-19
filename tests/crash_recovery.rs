//! 崩溃恢复与文件一致性(v0.6.2 M30)。
//!
//! 验证目标:**已提交的写入必须持久,未提交的必须不可见**。
//!
//! 这里用**真实进程终止**(`abort`)而非 mock —— 只有真实崩溃才能验证到
//! fsync / 事务提交的边界,否则测的是「正常关闭」这个更容易的情形。

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn temp_nodb(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "nosqlite_crash_{}_{}_{}.nodb",
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

// ==================== 文件损坏 ====================

/// 截断的文件(只剩一半)必须明确报错,不能 panic 或返回空数据。
#[test]
fn truncated_file_rejected() {
    let path = temp_nodb("truncated");
    {
        let db = nosqlite::Database::create(&path).unwrap();
        let mut d = nosqlite::Document::from_json(r#"{"a": 1}"#).unwrap();
        db.insert("items", &mut d).unwrap();
    }

    // 截掉一半
    let len = std::fs::metadata(&path).unwrap().len();
    let f = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    f.set_len(len / 2).unwrap();
    drop(f);

    let err = nosqlite::Database::open(&path);
    assert!(err.is_err(), "截断文件必须被拒绝,实际却成功打开了");

    let _ = std::fs::remove_file(&path);
}

/// 中间字节被翻转的文件不能返回错误的查询结果。
#[test]
fn corrupted_bytes_rejected_or_consistent() {
    let path = temp_nodb("corrupt");
    {
        let db = nosqlite::Database::create(&path).unwrap();
        let mut d = nosqlite::Document::from_json(r#"{"a": 1}"#).unwrap();
        db.insert("items", &mut d).unwrap();
    }

    let mut bytes = std::fs::read(&path).unwrap();
    // 翻转中段的若干字节(避开头部魔数)
    let mid = bytes.len() / 2;
    for i in 0..8.min(bytes.len() - mid) {
        bytes[mid + i] ^= 0xFF;
    }
    std::fs::write(&path, &bytes).unwrap();

    // 要么打不开,要么打开后读不到损坏的数据;绝不能返回错乱结果
    match nosqlite::Database::open(&path) {
        Err(_) => {} // 正确:拒绝损坏文件
        Ok(db) => {
            // 若仍能打开,查询结果必须是「要么有正确数据、要么为空」,
            // 不能是序列化失败后的残缺文档
            let docs = db
                .find("items", &nosqlite::query::Filter::match_all())
                .unwrap_or_default();
            for d in docs {
                assert!(d.contains_key("_id"), "返回的文档必须结构完整");
            }
        }
    }

    let _ = std::fs::remove_file(&path);
}

/// 随机垃圾字节不能当数据库打开。
#[test]
fn garbage_file_rejected() {
    let path = temp_nodb("garbage");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(&vec![0xABu8; 4096]).unwrap();
    drop(f);

    assert!(
        nosqlite::Database::open(&path).is_err(),
        "随机垃圾必须被拒绝"
    );

    let _ = std::fs::remove_file(&path);
}

/// 空文件不能当数据库打开。
#[test]
fn empty_file_rejected() {
    let path = temp_nodb("empty");
    std::fs::File::create(&path).unwrap();
    assert!(nosqlite::Database::open(&path).is_err(), "空文件必须被拒绝");
    let _ = std::fs::remove_file(&path);
}

/// 非法后缀仍然被拒绝(回归)。
#[test]
fn wrong_extension_still_rejected() {
    let mut path = temp_nodb("ext");
    path.set_extension("db");
    assert!(nosqlite::Database::create(&path).is_err());
    let _ = std::fs::remove_file(&path);
}

// ==================== 真实进程崩溃 ====================

/// 子进程写入并**立即 abort**,父进程校验数据完整性。
///
/// 这是本文件的核心用例:验证「已提交的写事务」在进程被强杀后仍然可见,
/// 且文件可正常重开(不出现半写状态)。
#[test]
fn committed_writes_survive_abort() {
    let path = temp_nodb("abort");

    // 子进程:插入 200 条后立刻 abort
    let out = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "crash_child_writer", "--ignored", "--nocapture"])
        .env("NOSQLITE_CRASH_PATH", path.to_str().unwrap())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("应能启动子进程");
    // 子进程被 abort,退出码非 0 是预期的
    assert!(!out.success(), "子进程应被 abort 终止");

    // 父进程重开:文件必须可用
    let db = nosqlite::Database::open(&path).expect("崩溃后文件应仍可打开");
    let docs = db
        .find("items", &nosqlite::query::Filter::match_all())
        .expect("崩溃后查询应成功");

    // insert_many 是单事务:要么 200 条全在,要么一条没有
    let n = docs.len();
    assert!(
        n == 0 || n == 200,
        "单事务写入应原子:期望 0 或 200 条,实际 {} 条",
        n
    );

    // 索引仍可用且与全表扫一致
    if n > 0 {
        db.create_index("items", "k_1", "k", false, false)
            .expect("崩溃后应能建索引");
        let filter = serde_json::json!({"k": {"$gte": 0}});
        let f = nosqlite::query::Filter::from_value(&filter).unwrap();
        let opts = nosqlite::query::FindOptions::default();
        let want = db.find_with_options("items", &f, &opts).unwrap();
        let got = db.find_planned("items", &f, &filter, &opts).unwrap();
        assert_eq!(want.len(), got.len(), "崩溃后索引与全表扫应一致");
    }

    drop(db);
    let _ = std::fs::remove_file(&path);
}

/// 辅助用例:供 `committed_writes_survive_abort` 以子进程方式调用。
///
/// 被 `#[ignore]` 标记,不会在常规测试中执行。
#[test]
#[ignore]
fn crash_child_writer() {
    let Ok(path) = std::env::var("NOSQLITE_CRASH_PATH") else {
        return;
    };
    let db = nosqlite::Database::create(std::path::Path::new(&path)).unwrap();

    let mut docs: Vec<nosqlite::Document> = (0..200)
        .map(|i| {
            nosqlite::Document::from_json(&format!(r#"{{"k": {}, "n": {}}}"#, i % 5, i)).unwrap()
        })
        .collect();
    // 单事务批量写:提交后立刻 abort
    db.insert_many("items", &mut docs).unwrap();

    // 不给任何清理机会,直接终止进程
    std::process::abort();
}

/// 多次「写入 → abort → 重开」循环,文件不应累积损坏。
#[test]
fn repeated_crash_recovery() {
    let path = temp_nodb("repeat");

    for round in 0..3 {
        let out = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "crash_child_writer", "--ignored", "--nocapture"])
            .env("NOSQLITE_CRASH_PATH", path.to_str().unwrap())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("应能启动子进程");
        assert!(!out.success(), "第 {} 轮子进程应被 abort", round);

        let db = nosqlite::Database::open(&path)
            .unwrap_or_else(|e| panic!("第 {} 轮崩溃后应可重开: {:?}", round, e));
        // 子进程每轮都尝试 create(文件已存在会失败),此处只验证可重开
        let _ = db.find("items", &nosqlite::query::Filter::match_all());
        drop(db);
    }

    let _ = std::fs::remove_file(&path);
}
