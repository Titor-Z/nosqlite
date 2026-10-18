//! CLI 交互式 shell 解析测试(M6/M7)。
//!
//! 交互式 stdin 不易自动化,故以「命令解析层」测试为主:
//! 覆盖 shell 内各命令(open/create/databases/insert/find/update/delete/count/
//! collections/drop/help/quit)的解析,与 filter/update 解析逻辑保持一致。

use nosqlite::cli::ShellCommand;

/// 辅助:按空白拆分一行命令为单词列表。
fn parse_line(line: &str) -> Result<ShellCommand, String> {
    let parts: Vec<String> = line.split_whitespace().map(|s| s.to_string()).collect();
    nosqlite::cli::parse_shell(&parts)
}

/// .quit / .exit 退出 shell(严格点命令,无点别名报错)。
#[test]
fn shell_quit_forms() {
    for cmd in [".quit", ".exit"] {
        assert_eq!(
            parse_line(cmd).unwrap(),
            ShellCommand::Quit,
            "{} 应解析为退出",
            cmd
        );
    }
}

/// .help 显示帮助。
#[test]
fn shell_help_forms() {
    assert_eq!(
        parse_line(".help").unwrap(),
        ShellCommand::Help,
        "应解析为帮助"
    );
}

/// .open <file.nodb>:打开数据库(不存在则自动创建);无参 = 关闭当前绑定。
#[test]
fn shell_parse_open() {
    assert_eq!(
        parse_line(".open data.nodb").unwrap(),
        ShellCommand::Open(Some("data.nodb".to_string()))
    );
    assert_eq!(parse_line(".open").unwrap(), ShellCommand::Open(None));
}

/// .databases:显示当前绑定。
#[test]
fn shell_parse_databases() {
    assert_eq!(parse_line(".databases").unwrap(), ShellCommand::Databases);
}

/// .collect:列出全部集合。
#[test]
fn shell_parse_collect() {
    assert_eq!(parse_line(".collect").unwrap(), ShellCommand::Collect);
}

/// open / create 缺路径参数应报错。
#[test]
fn shell_parse_open_create_errors() {
    assert!(parse_line("open").is_err(), "open 缺路径应报错");
    assert!(parse_line("create").is_err(), "create 缺路径应报错");
}

/// insert <collection> <json>。
#[test]
fn shell_parse_insert() {
    let cmd = parse_line(r#"insert users {"name":"张三","age":30}"#).unwrap();
    assert_eq!(
        cmd,
        ShellCommand::Insert(
            "users".to_string(),
            r#"{"name":"张三","age":30}"#.to_string()
        )
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
            projection: None,
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
            projection: None,
        }
    );
}

/// find <collection> + --fields 投影。
#[test]
fn shell_parse_find_with_projection() {
    let cmd = parse_line(r#"find users {} --fields {"name":1,"_id":0}"#).unwrap();
    assert_eq!(
        cmd,
        ShellCommand::Find {
            collection: "users".to_string(),
            filter: "{}".to_string(),
            sort: None,
            skip: 0,
            limit: None,
            projection: Some(r#"{"name":1,"_id":0}"#.to_string()),
        }
    );
}

/// `--fields` 缺参数报错。
#[test]
fn shell_parse_fields_needs_argument() {
    assert!(parse_line("find users {} --fields").is_err());
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

/// upsert <collection> <filter> <update>。
#[test]
fn shell_parse_upsert() {
    assert_eq!(
        parse_line(r#"upsert users {"email":"a@b.c"} {"$set":{"vip":true}}"#).unwrap(),
        ShellCommand::Upsert(
            "users".to_string(),
            r#"{"email":"a@b.c"}"#.to_string(),
            r#"{"$set":{"vip":true}}"#.to_string(),
        )
    );
    // 缺参数报错
    assert!(parse_line("upsert users {}").is_err());
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

/// 无点管理别名一律报错(严格点命令)。
#[test]
fn shell_reject_bare_management() {
    for cmd in [
        "open data.nodb",
        "create data.nodb",
        "databases",
        "db",
        "collections",
        "help",
        "?",
        "quit",
        "exit",
    ] {
        assert!(parse_line(cmd).is_err(), "无点管理命令 `{}` 应报错", cmd);
    }
}

/// drop <collection>。
#[test]
fn shell_parse_drop() {
    assert_eq!(
        parse_line("drop users").unwrap(),
        ShellCommand::Drop("users".to_string())
    );
}

/// 空行 / 缺参 / 未知命令:解析报错而非 panic。
#[test]
fn shell_parse_errors() {
    // 空参数列表
    assert!(parse_line("").is_err(), "空命令应报错");

    // 缺集合名参数
    for cmd in ["insert", "find", "update", "delete", "count", "drop"] {
        assert!(parse_line(cmd).is_err(), "{} 缺少参数应报错", cmd);
    }

    // 未知命令
    assert!(parse_line("foo bar").is_err(), "未知命令应报错");

    // find 未知参数
    assert!(
        parse_line("find users --bogus x").is_err(),
        "find 未知选项应报错"
    );

    // --skip 非数字
    assert!(
        parse_line("find users --skip abc").is_err(),
        "--skip 非数字应报错"
    );
}
