//! 存储引擎模块测试(M1)。
//!
//! 覆盖 `.nodb` 文件的创建/打开、后缀校验、魔数与版本校验。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use nodblite::engine::{RedbEngine, Storage, StorageError};

/// 全局自增计数器,用于生成唯一的临时文件路径。
static COUNTER: AtomicU32 = AtomicU32::new(0);

/// 生成一个唯一的临时 `.nodb` 测试文件路径(位于系统临时目录)。
fn temp_nodb(name: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!(
        "nodblite_test_{}_{}_{}.nodb",
        name,
        std::process::id(),
        n
    ))
}

/// 清理测试遗留文件(忽略删除失败)。
fn cleanup(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// 生成一个唯一的临时后缀路径(后缀不固定,用于非法后缀测试)。
fn temp_path(name: &str, ext: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!(
        "nodblite_test_{}_{}_{}.{}",
        name,
        std::process::id(),
        n,
        ext
    ))
}

/// 创建后可成功打开(创建 → 打开 → 关闭 往返)。
#[test]
fn create_then_open_roundtrip() {
    let path = temp_nodb("roundtrip");
    let engine = RedbEngine::create(&path).expect("创建数据库应成功");
    engine.close().expect("关闭应成功");
    // Windows 下 redb 持有文件独占锁,必须先释放第一个句柄才能再次打开
    drop(engine);
    assert!(path.exists(), "创建后文件应存在");

    let engine = RedbEngine::open(&path).expect("打开数据库应成功");
    engine.close().expect("关闭应成功");
    drop(engine);
    cleanup(&path);
}

/// 创建后再次创建同一路径应报"已存在"错误,防止误覆盖数据。
#[test]
fn create_existing_path_fails() {
    let path = temp_nodb("dup");
    let engine = RedbEngine::create(&path).expect("首次创建应成功");
    drop(engine);

    let err = RedbEngine::create(&path).expect_err("重复创建应失败");
    assert!(
        matches!(err, StorageError::Other(ref msg) if msg.contains("已存在")),
        "应提示文件已存在,实际: {}",
        err
    );
    cleanup(&path);
}

/// 拒绝非 `.nodb` 后缀:其他后缀一律不允许。
#[test]
fn create_rejects_other_suffixes() {
    let cases = ["db", "sqlite", "txt", "json", "data"];
    for ext in cases {
        let path = temp_path("bad_suffix", ext);
        let err = RedbEngine::create(&path).expect_err("非法后缀应被拒绝");
        assert!(
            matches!(err, StorageError::InvalidPath(ref msg) if msg.contains(".nodb")),
            "后缀 {} 应报 InvalidPath,实际: {}",
            ext,
            err
        );
        assert!(!path.exists(), "非法后缀不应创建文件");
    }
}

/// 拒绝无后缀路径。
#[test]
fn create_rejects_no_suffix() {
    let path = temp_path("no_suffix", "");
    let err = RedbEngine::create(&path).expect_err("无后缀应被拒绝");
    assert!(
        matches!(err, StorageError::InvalidPath(_)),
        "应报 InvalidPath,实际: {}",
        err
    );
}

/// `.nodb` 后缀不区分大小写(`.NODB` 应允许)。
#[test]
fn create_accepts_uppercase_suffix() {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "nodblite_test_upper_{}_{}.NODB",
        std::process::id(),
        n
    ));
    let engine = RedbEngine::create(&path).expect("大写 .NODB 后缀应允许");
    engine.close().expect("关闭应成功");
    cleanup(&path);
}

/// 打开不存在的文件应报"不是有效数据库"错误。
#[test]
fn open_nonexistent_fails() {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "nodblite_test_missing_{}_{}.nodb",
        std::process::id(),
        n
    ));
    let err = RedbEngine::open(&path).expect_err("打开不存在的文件应失败");
    assert!(
        matches!(err, StorageError::InvalidFormat(_)),
        "应报 InvalidFormat,实际: {}",
        err
    );
}

/// 打开一个魔数不匹配的文件(纯 redb 数据库、非 nodblite 创建)应被拒绝。
#[test]
fn open_foreign_redb_fails() {
    let path = temp_nodb("foreign");
    // 直接用 redb 创建,但不写 nodblite 元数据
    let db = redb::Database::create(&path).expect("用 redb 直接创建");
    drop(db);

    let err = RedbEngine::open(&path).expect_err("魔数不匹配应被拒绝");
    assert!(
        matches!(err, StorageError::InvalidFormat(ref msg) if msg.contains("魔数") || msg.contains("元数据")),
        "应报魔数/元数据不匹配,实际: {}",
        err
    );
    cleanup(&path);
}

/// 打开一个随机垃圾文件(非 redb 格式)应被拒绝。
#[test]
fn open_garbage_file_fails() {
    let path = temp_nodb("garbage");
    std::fs::write(&path, b"this is definitely not a nodblite database file")
        .expect("写入垃圾数据应成功");

    let err = RedbEngine::open(&path).expect_err("垃圾文件应被拒绝");
    assert!(
        matches!(err, StorageError::InvalidFormat(_)),
        "应报 InvalidFormat,实际: {}",
        err
    );
    cleanup(&path);
}

/// 打开一个非 `.nodb` 后缀的已存在文件也应先被后缀校验拦截。
#[test]
fn open_rejects_other_suffixes() {
    let path = temp_path("open_bad", "db");
    std::fs::write(&path, b"whatever").expect("写入文件应成功");

    let err = RedbEngine::open(&path).expect_err("非法后缀应被拒绝");
    assert!(
        matches!(err, StorageError::InvalidPath(_)),
        "应报 InvalidPath,实际: {}",
        err
    );
    cleanup(&path);
}
