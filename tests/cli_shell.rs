//! CLI 交互式 shell 解析测试(M6)。
//!
//! 交互式 stdin 不易自动化,故以「命令解析层」测试为主:
//! 覆盖 shell 内各命令(insert/find/update/delete/count/collections/drop/help/quit)的解析,
//! 与单命令模式的 filter/update 解析逻辑保持一致。

use nosqlite::cli::ShellCommand;

/// 辅助:按空白拆分一行命令为单词列表。
fn parse_line(line: &str) -> Result<ShellCommand, String> {
    let parts: Vec<String> = line.split_whitespace().map(|s| s.to_string()).collect();
    nosqlite::cli::parse_shell(&parts)
}

/// quit / exit / .quit / .exit 均退出 shell。
#[test]
fn shell_quit_forms() {
    for cmd in ["quit", "exit", ".quit", ".exit"] {
        assert_eq!(parse_line(cmd).unwrap(), ShellCommand::Quit, "{} 应解析为退出", cmd);
    }
}

/// help / .help / ? 显示帮助。
#[test]
fn shell_help_forms() {
    for cmd in ["help", ".help", "?"] {
        assert_eq!(parse_line(cmd).unwrap(), ShellCommand::Help, "{} 应解析为帮助", cmd);
    }
}

/// insert <collection> <json>。
#[test]
fn shell_parse_insert() {
    let cmd = parse_line(r#"insert users {"name":"张三","age":30}"#).unwrap();
    assert_eq!(
        cmd,
        ShellCommand::Insert("users".to_string(), r#"{"name":"张三","age":30}"#.to_string())
    );
}

/// find <collection> [filter]:无 filter 时缺省 null。
#[test]
fn shell_parse_find_no_filter() {
    let cmd = parse_line("find users").unwrap();
    assert_eq!(
        cmd,
        ShellCommand::Find {
            collection: "users".to_string(),
            filter: "null".to_string(),
            sort: None,
            skip: 0,
            limit: None,
        }
    );
}

/// find <collection> <filter> + --sort/--skip/--limit。
#[test]
fn shell_parse_find_with_options() {
    let cmd = parse_line(r#"find users {"age":{"$gte":18}} --sort {"age":-1} --skip 2 --limit 5"#)
        .unwrap();
    assert_eq!(
        cmd,
        ShellCommand::Find {
            collection: "users".to_string(),
            filter: r#"{"age":{"$gte":18}}"#.to_string(),
            sort: Some(r#"{"age":-1}"#.to_string()),
            skip: 2,
            limit: Some(5),
        }
    );
}

/// update <collection> <filter> <update>。
#[test]
fn shell_parse_update() {
    let cmd = parse_line(r#"update users {"_id":"u1"} {"$set":{"vip":true}}"#).unwrap();
    assert_eq!(
        cmd,
        ShellCommand::Update(
            "users".to_string(),
            r#"{"_id":"u1"}"#.to_string(),
            r#"{"$set":{"vip":true}}"#.to_string(),
        )
    );
}

/// delete <collection> <filter>。
#[test]
fn shell_parse_delete() {
    let cmd = parse_line(r#"delete users {"age":{"$lt":18}}"#).unwrap();
    assert_eq!(
        cmd,
        ShellCommand::Delete("users".to_string(), r#"{"age":{"$lt":18}}"#.to_string())
    );
}

/// count <collection> [filter]:无 filter 时缺省 null。
#[test]
fn shell_parse_count() {
    assert_eq!(
        parse_line("count users").unwrap(),
        ShellCommand::Count("users".to_string(), "null".to_string())
    );
    assert_eq!(
        parse_line(r#"count users {"age":{"$gte":18}}"#).unwrap(),
        ShellCommand::Count("users".to_string(), r#"{"age":{"$gte":18}}"#.to_string())
    );
}

/// collections(无参)。
#[test]
fn shell_parse_collections() {
    assert_eq!(parse_line("collections").unwrap(), ShellCommand::Collections);
}

/// drop <collection>。
#[test]
fn shell_parse_drop() {
    assert_eq!(parse_line("drop users").unwrap(), ShellCommand::Drop("users".to_string()));
}

/// 空行 / 缺参 / 未知命令:解析报错而非 panic。
#[test]
fn shell_parse_errors() {
    // 空参数列表
    assert!(parse_line("").is_err(), "空命令应报错");

    // 缺集合名参数
    for cmd in ["insert", "find", "update", "delete", "count", "drop"] {
        assert!(
            parse_line(cmd).is_err(),
            "{} 缺少参数应报错",
            cmd
        );
    }

    // 未知命令
    assert!(parse_line("foo bar").is_err(), "未知命令应报错");

    // find 未知参数
    assert!(parse_line("find users --bogus x").is_err(), "find 未知选项应报错");

    // --skip 非数字
    assert!(parse_line("find users --skip abc").is_err(), "--skip 非数字应报错");
}
