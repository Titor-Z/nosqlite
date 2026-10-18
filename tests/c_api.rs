//! C ABI 集成测试(阶段 1)。
//!
//! 不满足于「Rust 侧能调通」,而是**真的编译一个 C 程序**、链接本库动态库、
//! 跑一遍完整生命周期 —— 这样链接符号、头文件、内存约定一旦出问题就会暴露。

use std::path::PathBuf;
use std::process::Command;

/// 生成本次测试独占的临时路径。
fn temp_path(tag: &str, ext: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "nosqlite_cabi_{}_{}_{}.{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        ext
    ));
    let _ = std::fs::remove_file(&path);
    path
}

/// 定位编译产物目录(cdylib 与 rlib 都在 target/<profile> 下)。
fn target_dir() -> PathBuf {
    // 测试二进制位于 target/<profile>/deps/,其上两级即 target/<profile>
    let exe = std::env::current_exe().expect("应能取得测试二进制路径");
    exe.parent()
        .and_then(|p| p.parent())
        .expect("应有 target/<profile> 目录")
        .to_path_buf()
}

/// 编译并运行 `tests/c_smoke.c`,断言其退出码为 0。
#[test]
fn c_abi_smoke_test() {
    // 1. 确保动态库已构建(测试编译期已构建 lib,这里只做存在性校验)
    let target = target_dir();
    let lib = if cfg!(target_os = "macos") {
        target.join("libnosqlite.dylib")
    } else if cfg!(target_os = "windows") {
        target.join("nosqlite.dll")
    } else {
        target.join("libnosqlite.so")
    };
    assert!(lib.exists(), "动态库应存在: {}", lib.display());

    // 2. 编译 C 程序
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/c_smoke.c");
    let include = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("include");
    let bin = temp_path("smoke", if cfg!(windows) { "exe" } else { "bin" });

    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
    let mut cmd = Command::new(&cc);
    cmd.arg(&src)
        .arg(format!("-I{}", include.display()))
        .arg(format!("-L{}", target.display()))
        .arg("-lnosqlite")
        .arg("-o")
        .arg(&bin);
    if cfg!(target_os = "macos") {
        // 让链接器在运行时到构建目录找 dylib
        cmd.arg(format!("-Wl,-rpath,{}", target.display()));
    }
    let compile = cmd.output().expect("应能调用 C 编译器");
    assert!(
        compile.status.success(),
        "C 编译失败: {}",
        String::from_utf8_lossy(&compile.stderr)
    );

    // 3. 运行
    let db = temp_path("smoke_db", "nodb");
    let run = Command::new(&bin)
        .arg(&db)
        .env(
            if cfg!(target_os = "macos") {
                "DYLD_LIBRARY_PATH"
            } else {
                "LD_LIBRARY_PATH"
            },
            &target,
        )
        .output()
        .expect("应能运行 C 程序");

    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        run.status.success(),
        "C 冒烟测试失败\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains("ALL PASSED"), "实际输出: {}", stdout);

    let _ = std::fs::remove_file(&bin);
    let _ = std::fs::remove_file(&db);
}

/// 结构体布局必须与 C 头文件一致(`repr(C)` + 字段顺序)。
#[test]
fn struct_layouts_match_header() {
    use nosqlite::ffi::*;

    // Slice = { ptr, len }
    assert_eq!(
        std::mem::size_of::<NosqliteSlice>(),
        std::mem::size_of::<usize>() * 2
    );

    // Error = { i32, slice, slice, slice } => 4 + padding(4) + 16*3 = 56
    // 用对齐推算,不硬编码具体字节数,避免平台差异
    assert!(std::mem::size_of::<NosqliteError>() >= 4 + 3 * 16);

    // OpenOptions 的 struct_size 字段在偏移 0
    assert_eq!(std::mem::offset_of!(NosqliteOpenOptions, struct_size), 0);

    // FindOptions 的 struct_size 字段在偏移 0
    assert_eq!(std::mem::offset_of!(NosqliteFindOptions, struct_size), 0);

    // Result 的前两个字段是 code/count(便于 C 侧直接读)
    assert_eq!(std::mem::offset_of!(NosqliteResult, code), 0);
    assert!(std::mem::offset_of!(NosqliteResult, count) > 0);
}

/// 头文件里的错误码宏必须与 Rust 枚举的 `as_i32()` 一致。
///
/// 直接解析 `include/nosqlite.h` 的 `#define`,这样头文件与实现漂移会立刻被抓住。
#[test]
fn header_macros_match_rust() {
    use nosqlite::ErrorCode;

    let header = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("include/nosqlite.h"),
    )
    .expect("应能读取头文件");

    // 从 `#define NOSQLITE_<NAME> <num>` 中取出数值
    let find = |name: &str| -> Option<i32> {
        header
            .lines()
            .find(|l| l.trim_start().starts_with(&format!("#define {name} ")))
            .and_then(|l| l.split_whitespace().last())
            .and_then(|n| n.parse::<i32>().ok())
    };

    assert_eq!(find("NOSQLITE_OK"), Some(0), "成功码必须是 0");
    let pairs = [
        ("NOSQLITE_DUPLICATE_KEY", ErrorCode::DuplicateKey),
        ("NOSQLITE_INVALID_PATH", ErrorCode::InvalidPath),
        ("NOSQLITE_INVALID_FORMAT", ErrorCode::InvalidFormat),
        ("NOSQLITE_VERSION_MISMATCH", ErrorCode::VersionMismatch),
        ("NOSQLITE_INVALID_COLLECTION", ErrorCode::InvalidCollection),
        ("NOSQLITE_INVALID_FILTER", ErrorCode::InvalidFilter),
        ("NOSQLITE_INVALID_UPDATE", ErrorCode::InvalidUpdate),
        ("NOSQLITE_INVALID_SORT", ErrorCode::InvalidSort),
        ("NOSQLITE_INVALID_PROJECTION", ErrorCode::InvalidProjection),
        ("NOSQLITE_INVALID_DOCUMENT", ErrorCode::InvalidDocument),
        ("NOSQLITE_IO_ERROR", ErrorCode::IoError),
        ("NOSQLITE_INTERNAL", ErrorCode::Internal),
    ];
    for (macro_name, code) in pairs {
        assert_eq!(
            find(macro_name),
            Some(code.as_i32()),
            "头文件宏 {} 与 Rust 侧 {} 不一致",
            macro_name,
            code.as_str()
        );
    }
}
