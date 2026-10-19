//! CLI `.export` / `.import` / `.stats`(v0.1.5)的解析与端到端往返。
//!
//! 导出导入的往返逻辑本身在 `src/cli.rs` 内(private),
//! 这里通过二进制入口的 stdin 注入来验证真实行为。

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// 生成本次测试独占的临时路径。
fn temp_path(tag: &str, ext: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "nodblite_cli_io_{}_{}_{}.{}",
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

/// 向 CLI 注入若干行命令,返回 stdout+stderr。
fn run_shell(lines: &[String]) -> String {
    let exe = env!("CARGO_BIN_EXE_nodblite");
    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("应能启动 CLI");
    {
        let stdin = child.stdin.as_mut().expect("应有 stdin");
        for line in lines {
            writeln!(stdin, "{}", line).expect("写入 stdin 应成功");
        }
    }
    let out = child.wait_with_output().expect("应能等待退出");
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// 建库 + 插入若干文档,返回命令序列。
fn seed_lines(db: &Path, coll: &str) -> Vec<String> {
    vec![
        format!(".open {}", db.display()),
        format!(r#"insert {coll} {{"name":"张三","age":30}}"#),
        format!(r#"insert {coll} {{"name":"李四","age":20}}"#),
        format!(r#"insert {coll} {{"name":"王五","age":40}}"#),
    ]
}

/// `.export` 导出后用 `.import` 导入到新库,数据应完全一致。
#[test]
fn export_import_roundtrip() {
    let db = temp_path("rt_src", "nodb");
    let db2 = temp_path("rt_dst", "nodb");
    let jsonl = temp_path("rt", "jsonl");

    // 1. 建库并导出
    let mut lines = seed_lines(&db, "users");
    lines.push(format!(".export users {}", jsonl.display()));
    lines.push(".quit".to_string());
    let out = run_shell(&lines);
    assert!(out.contains("已导出 3 条文档"), "实际输出: {out}");

    // JSONL 应是一行一条、共 3 行
    let content = std::fs::read_to_string(&jsonl).expect("导出文件应存在");
    let non_empty: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(non_empty.len(), 3, "应为 3 行,实际: {content}");
    for line in &non_empty {
        let v: serde_json::Value = serde_json::from_str(line).expect("每行应是合法 JSON");
        assert!(v.get("_id").is_some(), "应包含 _id: {line}");
    }

    // 2. 导入到新库
    let out = run_shell(&[
        format!(".open {}", db2.display()),
        format!(".import users {}", jsonl.display()),
        "count users".to_string(),
        r#"find users {"age":{"$gte":30}}"#.to_string(),
        ".quit".to_string(),
    ]);
    assert!(out.contains("已导入 3 条文档"), "实际输出: {out}");
    assert!(out.contains("共 3 条"), "实际输出: {out}");
    assert!(out.contains("找到 2 条"), "年龄 >= 30 应有 2 条: {out}");

    let _ = std::fs::remove_file(&db);
    let _ = std::fs::remove_file(&db2);
    let _ = std::fs::remove_file(&jsonl);
}

/// `.import` 也接受「整个文件是一个 JSON 数组」。
#[test]
fn import_accepts_json_array() {
    let db = temp_path("arr", "nodb");
    let file = temp_path("arr", "json");

    std::fs::write(&file, r#"[{"name":"a","n":1},{"name":"b","n":2}]"#).unwrap();

    let out = run_shell(&[
        format!(".open {}", db.display()),
        format!(".import items {}", file.display()),
        "count items".to_string(),
        ".quit".to_string(),
    ]);
    assert!(out.contains("已导入 2 条文档"), "实际输出: {out}");
    assert!(out.contains("共 2 条"));

    let _ = std::fs::remove_file(&db);
    let _ = std::fs::remove_file(&file);
}

/// 导入非法 JSON 时整批不写(不会留下半个集合)。
#[test]
fn import_invalid_json_writes_nothing() {
    let db = temp_path("bad", "nodb");
    let file = temp_path("bad", "jsonl");

    // 第 2 行是非法 JSON
    std::fs::write(&file, "{\"n\":1}\n这不是JSON\n{\"n\":3}\n").unwrap();

    let out = run_shell(&[
        format!(".open {}", db.display()),
        format!(".import items {}", file.display()),
        "count items".to_string(),
        ".quit".to_string(),
    ]);
    assert!(out.contains("第 2 行格式错误"), "实际输出: {out}");
    assert!(out.contains("共 0 条"), "整批不应写入: {out}");

    let _ = std::fs::remove_file(&db);
    let _ = std::fs::remove_file(&file);
}

/// 主键冲突时整批回滚。
#[test]
fn import_duplicate_id_rolls_back() {
    let db = temp_path("dup", "nodb");
    let file = temp_path("dup", "jsonl");

    std::fs::write(&file, "{\"_id\":\"x\",\"n\":1}\n{\"_id\":\"x\",\"n\":2}\n").unwrap();

    let out = run_shell(&[
        format!(".open {}", db.display()),
        format!(".import items {}", file.display()),
        "count items".to_string(),
        ".quit".to_string(),
    ]);
    assert!(out.contains("导入失败"), "实际输出: {out}");
    assert!(out.contains("共 0 条"), "冲突应整批回滚: {out}");

    let _ = std::fs::remove_file(&db);
    let _ = std::fs::remove_file(&file);
}

/// `--append` 追加而不是覆盖。
#[test]
fn export_append() {
    let db = temp_path("app", "nodb");
    let file = temp_path("app", "jsonl");

    let mut lines = seed_lines(&db, "users");
    lines.push(format!(".export users {}", file.display()));
    lines.push(format!(".export users {} --append", file.display()));
    lines.push(".quit".to_string());
    run_shell(&lines);

    let content = std::fs::read_to_string(&file).unwrap();
    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), 6, "两次导出应累积 6 行");

    let _ = std::fs::remove_file(&db);
    let _ = std::fs::remove_file(&file);
}

/// 无 `--append` 时覆盖。
#[test]
fn export_overwrites_by_default() {
    let db = temp_path("ow", "nodb");
    let file = temp_path("ow", "jsonl");

    let mut lines = seed_lines(&db, "users");
    lines.push(format!(".export users {}", file.display()));
    lines.push(format!(".export users {}", file.display()));
    lines.push(".quit".to_string());
    run_shell(&lines);

    let content = std::fs::read_to_string(&file).unwrap();
    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), 3, "覆盖后仍应只有 3 行");

    let _ = std::fs::remove_file(&db);
    let _ = std::fs::remove_file(&file);
}

/// 导出空集合/不存在的集合产出空文件,不报错。
#[test]
fn export_empty_collection() {
    let db = temp_path("empty", "nodb");
    let file = temp_path("empty", "jsonl");

    let out = run_shell(&[
        format!(".open {}", db.display()),
        format!(".export nothing {}", file.display()),
        ".quit".to_string(),
    ]);
    assert!(out.contains("已导出 0 条文档"), "实际输出: {out}");
    let content = std::fs::read_to_string(&file).unwrap();
    assert!(content.is_empty());

    let _ = std::fs::remove_file(&db);
    let _ = std::fs::remove_file(&file);
}

/// `.stats` 输出文件大小、集合数与各集合条数。
#[test]
fn stats_reports_sizes_and_counts() {
    let db = temp_path("stats", "nodb");

    let out = run_shell(&[
        format!(".open {}", db.display()),
        r#"insert users {"n":1}"#.to_string(),
        r#"insert users {"n":2}"#.to_string(),
        r#"insert logs {"n":1}"#.to_string(),
        ".stats".to_string(),
        ".quit".to_string(),
    ]);

    assert!(out.contains("文件大小:"), "实际输出: {out}");
    assert!(out.contains("集合数量: 2"), "实际输出: {out}");
    assert!(out.contains("users"), "应列出 users: {out}");
    assert!(out.contains("logs"), "应列出 logs: {out}");
    assert!(out.contains("合计"), "应有合计行: {out}");

    let _ = std::fs::remove_file(&db);
}

/// 缺少参数时报错,并给出用法。
#[test]
fn missing_arguments_report_usage() {
    let out = run_shell(&[
        ".export".to_string(),
        ".import users".to_string(),
        ".quit".to_string(),
    ]);
    assert!(out.contains(".export 缺少参数"), "实际输出: {out}");
    assert!(out.contains(".import 缺少参数"), "实际输出: {out}");
}
