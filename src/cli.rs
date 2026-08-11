//! 类 sqlite3 的命令行工具。
//!
//! 支持两种形态:
//! - **单命令模式**:`nosqlite <子命令> <file.nodb> ...`,执行完即退出;
//! - **交互式 shell**:`nosqlite shell <file.nodb>`,进入 REPL(提示符 `nosqlite> `),
//!   后续命令不再带文件路径。
//!
//! 单命令子命令:
//! - `nosqlite create <file.nodb>`:创建新数据库;
//! - `nosqlite open   <file.nodb>`:打开并校验已有数据库;
//! - `nosqlite insert <file.nodb> <collection> <json>`:向集合插入一条文档;
//! - `nosqlite find   <file.nodb> <collection> [filter] [--sort <json>] [--skip <n>] [--limit <n>]`:
//!   查询集合内文档;
//! - `nosqlite update <file.nodb> <collection> <filter> <update>`:更新匹配文档;
//! - `nosqlite delete <file.nodb> <collection> <filter>`:删除匹配文档;
//! - `nosqlite count  <file.nodb> <collection> [filter]`:统计匹配文档条数;
//! - `nosqlite collections <file.nodb>`:列出数据库内全部集合名;
//! - `nosqlite drop   <file.nodb> <collection>`:删除整个集合及其全部文档;
//! - `nosqlite shell  <file.nodb>`:进入交互式 shell。

use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use serde_json::Value;

use crate::database::Database;
use crate::document::Document;
use crate::engine::{RedbEngine, Storage};
use crate::query::{Filter, FindOptions, SortSpec, Update};

/// 命令行子命令。
enum Command {
    /// 创建新数据库(目标文件必须不存在)。
    Create(String),
    /// 打开并校验已有数据库。
    Open(String),
    /// 向集合插入一条文档(JSON 对象)。
    Insert(String, String, String),
    /// 查询集合内文档。
    Find {
        /// 数据库文件路径。
        path: String,
        /// 集合名。
        collection: String,
        /// filter(JSON 对象,null 表示匹配全部)。
        filter: String,
        /// 排序规格(JSON 对象,如 `{"age": 1}`)。
        sort: Option<String>,
        /// 跳过条数。
        skip: usize,
        /// 限制条数。
        limit: Option<usize>,
    },
    /// 更新集合内满足 filter 的文档。
    Update(String, String, String, String),
    /// 删除集合内满足 filter 的文档。
    Delete(String, String, String),
    /// 统计集合内满足 filter 的文档条数。
    Count(String, String, String),
    /// 列出数据库内全部集合名。
    Collections(String),
    /// 删除整个集合及其全部文档(不可恢复)。
    Drop(String, String),
    /// 进入交互式 shell(打开指定数据库,后续命令不再带文件路径)。
    Shell(String),
    /// 显示帮助信息。
    Help,
}

/// shell 内的一条命令(不带文件路径参数,作用于已打开的数据库)。
#[derive(Debug, PartialEq)]
pub enum ShellCommand {
    /// 向集合插入一条文档(JSON 对象)。
    Insert(String, String),
    /// 查询集合内文档。
    Find {
        /// 集合名。
        collection: String,
        /// filter(JSON 对象,null 表示匹配全部)。
        filter: String,
        /// 排序规格(JSON 对象,如 `{"age": 1}`)。
        sort: Option<String>,
        /// 跳过条数。
        skip: usize,
        /// 限制条数。
        limit: Option<usize>,
    },
    /// 更新集合内满足 filter 的文档。
    Update(String, String, String),
    /// 删除集合内满足 filter 的文档。
    Delete(String, String),
    /// 统计集合内满足 filter 的文档条数。
    Count(String, String),
    /// 列出数据库内全部集合名。
    Collections,
    /// 删除整个集合及其全部文档(不可恢复)。
    Drop(String),
    /// 显示 shell 命令用法。
    Help,
    /// 退出 shell。
    Quit,
}

/// CLI 程序入口:解析参数 → 执行子命令 → 返回退出码。
pub fn run() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match parse(&args) {
        Ok(cmd) => execute(cmd),
        Err(msg) => {
            // 参数解析失败:打印错误并提示帮助
            eprintln!("错误: {}", msg);
            eprintln!("输入 `nosqlite help` 查看用法。");
            ExitCode::FAILURE
        }
    }
}

/// 解析命令行参数为 [`Command`]。
fn parse(args: &[String]) -> Result<Command, String> {
    match args {
        [] => Ok(Command::Help),
        [flag] if flag == "-h" || flag == "--help" || flag == "help" => Ok(Command::Help),
        [sub, path] if sub == "create" => Ok(Command::Create(path.clone())),
        [sub, path] if sub == "open" => Ok(Command::Open(path.clone())),
        [sub, path] if sub == "shell" => Ok(Command::Shell(path.clone())),
        [sub, path, collection, json] if sub == "insert" => Ok(Command::Insert(
            path.clone(),
            collection.clone(),
            json.clone(),
        )),
        // update <file> <collection> <filter> <update>
        [sub, path, collection, filter, update] if sub == "update" => Ok(Command::Update(
            path.clone(),
            collection.clone(),
            filter.clone(),
            update.clone(),
        )),
        // delete <file> <collection> <filter>
        [sub, path, collection, filter] if sub == "delete" => Ok(Command::Delete(
            path.clone(),
            collection.clone(),
            filter.clone(),
        )),
        // count <file> <collection> [filter]
        [sub, path, collection] if sub == "count" => {
            Ok(Command::Count(path.clone(), collection.clone(), "null".to_string()))
        }
        [sub, path, collection, filter] if sub == "count" => Ok(Command::Count(
            path.clone(),
            collection.clone(),
            filter.clone(),
        )),
        // collections <file>
        [sub, path] if sub == "collections" => Ok(Command::Collections(path.clone())),
        // drop <file> <collection>
        [sub, path, collection] if sub == "drop" => {
            Ok(Command::Drop(path.clone(), collection.clone()))
        }
        // find <file> <collection> [filter] [--sort json] [--skip n] [--limit n]
        [sub, path, collection, rest @ ..] if sub == "find" => {
            parse_find(path, collection, rest)
        }
        [sub] => Err(format!("子命令 `{}` 需要一个 <file.nodb> 路径参数", sub)),
        [unknown, ..] => Err(format!("未知子命令或参数过多: `{}`", unknown)),
    }
}

/// 解析 find 子命令的可选参数。
fn parse_find(path: &str, collection: &str, args: &[String]) -> Result<Command, String> {
    let mut filter = String::new();
    let mut sort: Option<String> = None;
    let mut skip: usize = 0;
    let mut limit: Option<usize> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--sort" => {
                i += 1;
                if i >= args.len() {
                    return Err("`--sort` 需要一个 JSON 对象参数".to_string());
                }
                sort = Some(args[i].clone());
            }
            "--skip" => {
                i += 1;
                if i >= args.len() {
                    return Err("`--skip` 需要一个数字参数".to_string());
                }
                skip = args[i]
                    .parse()
                    .map_err(|_| format!("`--skip` 参数不是有效数字: {}", args[i]))?;
            }
            "--limit" => {
                i += 1;
                if i >= args.len() {
                    return Err("`--limit` 需要一个数字参数".to_string());
                }
                let n: usize = args[i]
                    .parse()
                    .map_err(|_| format!("`--limit` 参数不是有效数字: {}", args[i]))?;
                limit = Some(n);
            }
            // 非选项参数:filter(filter 省略则匹配全部)
            flag if filter.is_empty() && !flag.starts_with("--") => filter = flag.to_string(),
            flag => return Err(format!("find 未知参数: `{}`", flag)),
        }
        i += 1;
    }

    // filter 缺省视为空对象(null),匹配全部
    if filter.is_empty() {
        filter = "null".to_string();
    }

    Ok(Command::Find {
        path: path.to_string(),
        collection: collection.to_string(),
        filter,
        sort,
        skip,
        limit,
    })
}

/// 执行解析出的子命令。
fn execute(cmd: Command) -> ExitCode {
    match cmd {
        Command::Create(path) => {
            let engine = RedbEngine::create(Path::new(&path));
            report(&path, "创建", engine)
        }
        Command::Open(path) => {
            let engine = RedbEngine::open(Path::new(&path));
            report(&path, "打开", engine)
        }
        Command::Insert(path, collection, json) => run_insert(&path, &collection, &json),
        Command::Find {
            path,
            collection,
            filter,
            sort,
            skip,
            limit,
        } => run_find(&path, &collection, &filter, sort.as_deref(), skip, limit),
        Command::Update(path, collection, filter, update) => {
            run_update(&path, &collection, &filter, &update)
        }
        Command::Delete(path, collection, filter) => run_delete(&path, &collection, &filter),
        Command::Count(path, collection, filter) => run_count(&path, &collection, &filter),
        Command::Collections(path) => run_collections(&path),
        Command::Drop(path, collection) => run_drop(&path, &collection),
        Command::Shell(path) => run_shell(&path),
        Command::Help => {
            print_help();
            ExitCode::SUCCESS
        }
    }
}

/// 打开数据库,失败时打印中文错误并返回 `None`。
fn open_database(path: &str) -> Option<Database> {
    match Database::open(Path::new(path)) {
        Ok(db) => Some(db),
        Err(e) => {
            eprintln!("打开数据库失败: {}", e);
            None
        }
    }
}

/// 进入交互式 shell:打开数据库 → 循环读取 stdin 每行命令 → 分发执行。
fn run_shell(path: &str) -> ExitCode {
    let db = match open_database(path) {
        Some(db) => db,
        None => return ExitCode::FAILURE,
    };

    println!("已打开数据库: {}", path);
    println!("输入 `help` 查看命令用法,输入 `quit`/`exit` 退出 shell。");

    // 逐行读取 stdin(交互式 TTY 或管道重定向均可)
    let stdin = std::io::stdin();
    loop {
        // 打印提示符并刷新缓冲区
        print!("nosqlite> ");
        if std::io::stdout().flush().is_err() {
            break;
        }

        let mut line = String::new();
        match stdin.read_line(&mut line) {
            // 读取到 EOF(Ctrl+D/文件末尾)→ 退出
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => break,
        }

        // 空行忽略
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // 整行按空白拆分得到单词列表(与单命令模式的 argv 对齐)
        let parts: Vec<String> = trimmed.split_whitespace().map(|s| s.to_string()).collect();
        match parse_shell(&parts) {
            Ok(ShellCommand::Quit) => {
                println!("再见!");
                break;
            }
            Ok(cmd) => {
                // 命令执行错误打印后继续循环,不退出
                let _ = exec_shell(&db, cmd);
            }
            Err(msg) => {
                eprintln!("错误: {}", msg);
            }
        }
    }

    // 释放数据库句柄(Windows 下 redb 独占锁,显式 drop 以便后续再开)
    drop(db);
    ExitCode::SUCCESS
}

/// 解析一行 shell 命令(不带文件路径)。
///
/// `args` 为按空白拆分后的单词列表(与单命令模式的 argv 对齐)。
pub fn parse_shell(args: &[String]) -> Result<ShellCommand, String> {
    match args {
        [] => Err("请输入命令,输入 `help` 查看用法".to_string()),
        [cmd] if cmd == "quit" || cmd == "exit" || cmd == ".quit" || cmd == ".exit" => {
            Ok(ShellCommand::Quit)
        }
        [cmd] if cmd == "help" || cmd == ".help" || cmd == "?" => Ok(ShellCommand::Help),
        [cmd, collection, json] if cmd == "insert" => {
            Ok(ShellCommand::Insert(collection.clone(), json.clone()))
        }
        [cmd, collection] if cmd == "count" => Ok(ShellCommand::Count(
            collection.clone(),
            "null".to_string(),
        )),
        [cmd, collection, filter] if cmd == "count" => {
            Ok(ShellCommand::Count(collection.clone(), filter.clone()))
        }
        [cmd] if cmd == "collections" => Ok(ShellCommand::Collections),
        [cmd, collection] if cmd == "drop" => Ok(ShellCommand::Drop(collection.clone())),
        [cmd, collection, filter] if cmd == "delete" => {
            Ok(ShellCommand::Delete(collection.clone(), filter.clone()))
        }
        [cmd, collection, filter, update] if cmd == "update" => Ok(ShellCommand::Update(
            collection.clone(),
            filter.clone(),
            update.clone(),
        )),
        // find <collection> [filter] [--sort json] [--skip n] [--limit n]
        [cmd, collection, rest @ ..] if cmd == "find" => parse_shell_find(collection, rest),
        [cmd] if cmd == "insert" || cmd == "find" || cmd == "update" || cmd == "delete"
            || cmd == "count" || cmd == "drop" =>
        {
            Err(format!("命令 `{}` 缺少参数,输入 `help` 查看用法", cmd))
        }
        [unknown, ..] => Err(format!("未知命令或参数过多: `{}`", unknown)),
    }
}

/// 解析 shell 内 find 命令的可选参数。
fn parse_shell_find(collection: &str, args: &[String]) -> Result<ShellCommand, String> {
    let mut filter = String::new();
    let mut sort: Option<String> = None;
    let mut skip: usize = 0;
    let mut limit: Option<usize> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--sort" => {
                i += 1;
                if i >= args.len() {
                    return Err("`--sort` 需要一个 JSON 对象参数".to_string());
                }
                sort = Some(args[i].clone());
            }
            "--skip" => {
                i += 1;
                if i >= args.len() {
                    return Err("`--skip` 需要一个数字参数".to_string());
                }
                skip = args[i]
                    .parse()
                    .map_err(|_| format!("`--skip` 参数不是有效数字: {}", args[i]))?;
            }
            "--limit" => {
                i += 1;
                if i >= args.len() {
                    return Err("`--limit` 需要一个数字参数".to_string());
                }
                let n: usize = args[i]
                    .parse()
                    .map_err(|_| format!("`--limit` 参数不是有效数字: {}", args[i]))?;
                limit = Some(n);
            }
            // 非选项参数:filter(filter 省略则匹配全部)
            flag if filter.is_empty() && !flag.starts_with("--") => filter = flag.to_string(),
            flag => return Err(format!("find 未知参数: `{}`", flag)),
        }
        i += 1;
    }

    // filter 缺省视为空对象(null),匹配全部
    if filter.is_empty() {
        filter = "null".to_string();
    }

    Ok(ShellCommand::Find {
        collection: collection.to_string(),
        filter,
        sort,
        skip,
        limit,
    })
}

/// 分发执行一条 shell 命令(作用于已打开的数据库)。
fn exec_shell(db: &Database, cmd: ShellCommand) -> ExitCode {
    match cmd {
        ShellCommand::Insert(collection, json) => exec_insert(db, &collection, &json),
        ShellCommand::Find {
            collection,
            filter,
            sort,
            skip,
            limit,
        } => exec_find(db, &collection, &filter, sort.as_deref(), skip, limit),
        ShellCommand::Update(collection, filter, update) => {
            exec_update(db, &collection, &filter, &update)
        }
        ShellCommand::Delete(collection, filter) => exec_delete(db, &collection, &filter),
        ShellCommand::Count(collection, filter) => exec_count(db, &collection, &filter),
        ShellCommand::Collections => exec_collections(db),
        ShellCommand::Drop(collection) => exec_drop(db, &collection),
        ShellCommand::Help => {
            print_shell_help();
            ExitCode::SUCCESS
        }
        ShellCommand::Quit => ExitCode::SUCCESS,
    }
}

/// 统一处理引擎操作结果:成功打印中文提示,失败打印错误。
fn report(path: &str, action: &str, result: Result<RedbEngine, crate::engine::StorageError>) -> ExitCode {
    match result {
        Ok(engine) => {
            // 输出成功信息(中文,符合项目规范)
            println!("已{}数据库: {}", action, path);
            println!("格式版本: 1,文件后缀: .nodb,单文件可直接复制粘贴迁移。");
            let _ = engine.close();
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{}失败: {}", action, e);
            ExitCode::FAILURE
        }
    }
}

/// 执行 insert 子命令:打开数据库 → 插入。
fn run_insert(path: &str, collection: &str, json: &str) -> ExitCode {
    match open_database(path) {
        Some(db) => exec_insert(&db, collection, json),
        None => ExitCode::FAILURE,
    }
}

/// 在已打开的数据库上执行 insert。
fn exec_insert(db: &Database, collection: &str, json: &str) -> ExitCode {
    // 解析 JSON 为文档
    let mut doc = match Document::from_json(json) {
        Ok(doc) => doc,
        Err(e) => {
            eprintln!("文档格式错误: {}", e);
            return ExitCode::FAILURE;
        }
    };

    // 插入文档
    match db.insert(collection, &mut doc) {
        Ok(id) => {
            println!("插入成功: 集合 `{}`, _id = {}", collection, id);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("插入失败: {}", e);
            ExitCode::FAILURE
        }
    }
}

/// 执行 find 子命令:打开数据库 → 查询。
fn run_find(
    path: &str,
    collection: &str,
    filter_json: &str,
    sort_json: Option<&str>,
    skip: usize,
    limit: Option<usize>,
) -> ExitCode {
    match open_database(path) {
        Some(db) => exec_find(&db, collection, filter_json, sort_json, skip, limit),
        None => ExitCode::FAILURE,
    }
}

/// 在已打开的数据库上执行 find。
fn exec_find(
    db: &Database,
    collection: &str,
    filter_json: &str,
    sort_json: Option<&str>,
    skip: usize,
    limit: Option<usize>,
) -> ExitCode {
    // 解析 filter
    let filter = match parse_filter(filter_json) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("filter 错误: {}", e);
            return ExitCode::FAILURE;
        }
    };

    // 解析 sort(可选)
    let sort = match sort_json {
        Some(json) => match parse_sort(json) {
            Ok(s) => Some(s),
            Err(e) => {
                eprintln!("sort 错误: {}", e);
                return ExitCode::FAILURE;
            }
        },
        None => None,
    };

    // 组装查询选项并执行
    let options = FindOptions { sort, skip, limit };
    match db.find_with_options(collection, &filter, &options) {
        Ok(docs) => {
            println!("找到 {} 条文档(集合 `{}`):", docs.len(), collection);
            for doc in docs {
                println!("  {}", serde_json::to_string_pretty(&doc).unwrap_or_default());
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("查询失败: {}", e);
            ExitCode::FAILURE
        }
    }
}

/// 解析 filter 参数(null 或省略表示匹配全部)。
fn parse_filter(json: &str) -> Result<Filter, String> {
    let value: Value =
        serde_json::from_str(json).map_err(|e| format!("JSON 解析失败: {}", e))?;
    Filter::from_value(&value).map_err(|e| e.to_string())
}

/// 解析 sort 参数。
fn parse_sort(json: &str) -> Result<SortSpec, String> {
    let value: Value =
        serde_json::from_str(json).map_err(|e| format!("JSON 解析失败: {}", e))?;
    SortSpec::from_value(&value).map_err(|e| e.to_string())
}

/// 执行 update 子命令:打开数据库 → 更新。
fn run_update(path: &str, collection: &str, filter_json: &str, update_json: &str) -> ExitCode {
    match open_database(path) {
        Some(db) => exec_update(&db, collection, filter_json, update_json),
        None => ExitCode::FAILURE,
    }
}

/// 在已打开的数据库上执行 update。
fn exec_update(db: &Database, collection: &str, filter_json: &str, update_json: &str) -> ExitCode {
    // 解析 filter
    let filter = match parse_filter(filter_json) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("filter 错误: {}", e);
            return ExitCode::FAILURE;
        }
    };

    // 解析 update
    let update_value: Value = match serde_json::from_str(update_json) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("update JSON 解析失败: {}", e);
            return ExitCode::FAILURE;
        }
    };
    let update = match Update::from_value(&update_value) {
        Ok(u) => u,
        Err(e) => {
            eprintln!("update 错误: {}", e);
            return ExitCode::FAILURE;
        }
    };

    // 4. 执行更新
    match db.update(collection, &filter, &update) {
        Ok(n) => {
            println!("更新成功: 集合 `{}` 共修改 {} 条文档", collection, n);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("更新失败: {}", e);
            ExitCode::FAILURE
        }
    }
}

/// 执行 delete 子命令:打开数据库 → 删除。
fn run_delete(path: &str, collection: &str, filter_json: &str) -> ExitCode {
    match open_database(path) {
        Some(db) => exec_delete(&db, collection, filter_json),
        None => ExitCode::FAILURE,
    }
}

/// 在已打开的数据库上执行 delete。
fn exec_delete(db: &Database, collection: &str, filter_json: &str) -> ExitCode {
    // 解析 filter
    let filter = match parse_filter(filter_json) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("filter 错误: {}", e);
            return ExitCode::FAILURE;
        }
    };

    // 执行删除
    match db.delete(collection, &filter) {
        Ok(n) => {
            println!("删除成功: 集合 `{}` 共删除 {} 条文档", collection, n);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("删除失败: {}", e);
            ExitCode::FAILURE
        }
    }
}

/// 执行 count 子命令:打开数据库 → 统计。
fn run_count(path: &str, collection: &str, filter_json: &str) -> ExitCode {
    match open_database(path) {
        Some(db) => exec_count(&db, collection, filter_json),
        None => ExitCode::FAILURE,
    }
}

/// 在已打开的数据库上执行 count。
fn exec_count(db: &Database, collection: &str, filter_json: &str) -> ExitCode {
    // 解析 filter
    let filter = match parse_filter(filter_json) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("filter 错误: {}", e);
            return ExitCode::FAILURE;
        }
    };

    // 统计
    match db.count(collection, &filter) {
        Ok(n) => {
            println!("集合 `{}` 满足条件的文档共 {} 条", collection, n);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("统计失败: {}", e);
            ExitCode::FAILURE
        }
    }
}

/// 执行 collections 子命令:打开数据库 → 列出集合名。
fn run_collections(path: &str) -> ExitCode {
    match open_database(path) {
        Some(db) => exec_collections(&db),
        None => ExitCode::FAILURE,
    }
}

/// 在已打开的数据库上执行 collections。
fn exec_collections(db: &Database) -> ExitCode {
    match db.collections() {
        Ok(names) => {
            if names.is_empty() {
                println!("数据库为空,暂无集合");
            } else {
                println!("数据库共 {} 个集合:", names.len());
                for (i, name) in names.iter().enumerate() {
                    println!("  {}. {}", i + 1, name);
                }
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("列出集合失败: {}", e);
            ExitCode::FAILURE
        }
    }
}

/// 执行 drop 子命令:打开数据库 → 删除整个集合。
fn run_drop(path: &str, collection: &str) -> ExitCode {
    match open_database(path) {
        Some(db) => exec_drop(&db, collection),
        None => ExitCode::FAILURE,
    }
}

/// 在已打开的数据库上执行 drop。
fn exec_drop(db: &Database, collection: &str) -> ExitCode {
    match db.drop_collection(collection) {
        Ok(n) => {
            println!("已删除集合 `{}` 及其全部 {} 条文档(不可恢复)", collection, n);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("删除集合失败: {}", e);
            ExitCode::FAILURE
        }
    }
}

/// 打印 shell 内命令用法。
fn print_shell_help() {
    println!("nosqlite shell 内命令(不带文件路径,作用于当前数据库):");
    println!();
    println!("  insert <collection> <json>            向集合插入一条文档");
    println!("  find <collection> [filter]            查询文档");
    println!("       [--sort <json>] [--skip <n>] [--limit <n>]");
    println!("  update <collection> <filter> <update> 更新匹配文档");
    println!("  delete <collection> <filter>          删除匹配文档");
    println!("  count <collection> [filter]           统计匹配条数");
    println!("  collections                           列出全部集合");
    println!("  drop <collection>                     删除整个集合(不可恢复)");
    println!("  help / ?                              显示本帮助");
    println!("  quit / exit                           退出 shell");
    println!();
    println!("示例:");
    println!("  insert users '{{\"name\":\"张三\",\"age\":30}}'");
    println!("  find users '{{\"age\": {{\"$gte\": 18}}}}' --sort '{{\"age\": -1}}' --limit 10");
    println!("  update users '{{\"_id\":\"u1\"}}' '{{\"$set\":{{\"vip\":true}}}}'");
    println!("  delete users '{{\"age\": {{\"$lt\": 18}}}}'");
}

/// 打印帮助信息。
fn print_help() {
    println!("nosqlite —— 像 SQLite 一样小巧的单文件 NoSQL 数据库");
    println!();
    println!("用法:");
    println!("  nosqlite create <file.nodb>                       创建新数据库");
    println!("  nosqlite open   <file.nodb>                       打开并校验已有数据库");
    println!("  nosqlite shell  <file.nodb>                       进入交互式 shell");
    println!("  nosqlite insert <file.nodb> <collection> <json>   向集合插入一条文档");
    println!("  nosqlite find   <file.nodb> <collection> [filter] 查询文档");
    println!("                 [--sort <json>] [--skip <n>] [--limit <n>]");
    println!("  nosqlite update <file.nodb> <collection> <filter> <update>  更新文档");
    println!("  nosqlite delete <file.nodb> <collection> <filter>  删除文档");
    println!("  nosqlite count  <file.nodb> <collection> [filter]  统计匹配条数");
    println!("  nosqlite collections <file.nodb>                   列出全部集合");
    println!("  nosqlite drop   <file.nodb> <collection>           删除整个集合");
    println!("  nosqlite help                                     显示本帮助");
    println!();
    println!("示例:");
    println!("  nosqlite shell  data.nodb");
    println!("  nosqlite insert data.nodb users '{{\"name\":\"张三\",\"age\":30}}'");
    println!("  nosqlite find   data.nodb users '{{\"age\": {{\"$gte\": 18}}}}'");
    println!("  nosqlite find   data.nodb users '{{\"name\": {{\"$regex\": \"^张\"}}}}'");
    println!("  nosqlite find   data.nodb users null --sort '{{\"age\": -1}}' --limit 10");
    println!("  nosqlite update data.nodb users '{{\"age\": {{\"$gte\": 18}}}}' '{{\"$set\": {{\"vip\": true}}}}'");
    println!("  nosqlite delete data.nodb users '{{\"age\": {{\"$lt\": 18}}}}'");
    println!("  nosqlite count  data.nodb users '{{\"age\": {{\"$gte\": 18}}}}'");
    println!("  nosqlite collections data.nodb");
    println!("  nosqlite drop   data.nodb users");
    println!();
    println!("说明:");
    println!("  - 文件后缀必须为 .nodb,不支持其他后缀或无后缀");
    println!("  - 数据库为单文件,可直接复制粘贴迁移");
    println!("  - 文档顶层必须是 JSON 对象;_id 缺省时自动生成");
    println!("  - filter 操作符:$eq/$ne/$gt/$gte/$lt/$lte/$in/$nin/$regex");
    println!("  - 顶层逻辑:$and/$or/$nor/$not;字段支持点号路径(a.b.c)");
    println!("  - update 操作符:$set(点号路径赋值,嵌套自动创建;主键 _id 不可修改)");
}
