//! 类 sqlite3 的命令行工具(交互式 TTY,对齐 sqlite3 形态)。
//!
//! 使用方法:
//! - `nodblite`:直接进入交互式 shell,未绑定任何数据库;
//! - `nodblite <file.nodb>`:打开该数据库进入 shell,文件不存在则自动创建;
//! - `nodblite -h`/`--help`/`help`:显示用法。
//!
//! shell 内命令(提示符 `nodblite> `)不带文件路径:
//! - 数据库管理:`open <file>`(不存在则创建)、`create <file>`(新建,已存在拒绝)、`databases`(显示当前绑定);
//! - 集合操作(无点):`insert/find/update/upsert/delete/count/drop <collection> ...`;
//! - 导入导出与统计:`.export <collection> [file]`、`.import <collection> <file>`、`.stats`;
//! - `.help` 显示用法,`.quit`/`.exit` 退出。

use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use serde_json::Value;

use crate::database::Database;
use crate::database::UpsertResult;
use crate::document::Document;
use crate::query::{Filter, FindOptions, Projection, SortSpec, Update};

/// shell 内的一条命令(不带文件路径参数)。
///
/// 管理命令一律点号开头(对齐 sqlite3 点命令),CRUD 一律无点。
#[derive(Debug, PartialEq)]
pub enum ShellCommand {
    /// 打开数据库:带路径则打开(不存在则自动创建),无路径则关闭当前回到未绑定。
    Open(Option<String>),
    /// 显示当前绑定的数据库文件路径。
    Databases,
    /// 列出数据库内全部集合名(替代 sqlite3 `.tables` 语义)。
    Collect,
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
        /// 字段投影(JSON 对象,如 `{"name": 1}`;None 表示返回整条文档)。
        projection: Option<String>,
    },
    /// 更新集合内满足 filter 的文档。
    Update(String, String, String),
    /// 更新或插入(upsert):命中则更新,未命中则按 filter 等值字段新建。
    Upsert(String, String, String),
    /// 删除集合内满足 filter 的文档。
    Delete(String, String),
    /// 统计集合内满足 filter 的文档条数。
    Count(String, String),
    /// 删除整个集合及其全部文档(不可恢复)。
    Drop(String),
    /// 导出集合为 JSON Lines(省略文件则输出到 stdout)。
    Export {
        /// 集合名。
        collection: String,
        /// 目标文件(省略则 stdout)。
        file: Option<String>,
        /// 追加而非覆盖(仅对文件有效)。
        append: bool,
    },
    /// 从 JSON Lines(或 JSON 数组)导入集合。
    Import {
        /// 集合名。
        collection: String,
        /// 源文件。
        file: String,
    },
    /// 显示数据库统计(文件大小、集合数、各集合文档数)。
    Stats,
    /// 显示 shell 命令用法。
    Help,
    /// 退出 shell。
    Quit,
}

/// CLI 程序入口(对齐 sqlite3):
/// - 无参数 → 直接进入 shell(未绑定数据库);
/// - 单个路径参数 → 打开(不存在则创建)后进入 shell;
/// - `-h`/`--help`/`help` → 显示用法。
pub fn run() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.as_slice() {
        // 无参数:直接进入 shell,未绑定数据库
        [] => run_shell(None),
        [arg] if arg == "-h" || arg == "--help" || arg == "help" => {
            print_help();
            ExitCode::SUCCESS
        }
        // 单个路径:打开(不存在则自动创建)后进入 shell
        [path] => match open_or_create(path) {
            Ok(db) => run_shell(Some(db)),
            Err(msg) => {
                eprintln!("错误: {}", msg);
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!("错误: 参数过多,`nodblite` 只接受可选的 <file.nodb> 路径。");
            eprintln!("输入 `nodblite help` 查看用法。");
            ExitCode::FAILURE
        }
    }
}

/// 打开数据库;文件不存在则自动创建(对齐 sqlite3"打开即建")。
fn open_or_create(path: &str) -> Result<Database, String> {
    // 先尝试打开;若文件不存在则创建空库
    match Database::open(Path::new(path)) {
        Ok(db) => Ok(db),
        Err(_) => match Database::create(Path::new(path)) {
            Ok(db) => Ok(db),
            // 创建也失败(如后缀非法)→ 报告原始错误
            Err(e) => Err(e.to_string()),
        },
    }
}

/// 进入交互式 shell。
///
/// `initial` 为进入时已绑定的数据库(可为空)。shell 内可通过
/// `.open` 命令切换绑定;集合命令在未绑定数据库时给予中文提示。
fn run_shell(initial: Option<Database>) -> ExitCode {
    // 持有可变绑定状态(切换绑定前需 drop 旧句柄,满足 Windows redb 独占锁约束)
    let mut current: Option<Database> = initial;

    // 进入提示
    match &current {
        Some(db) => println!("已打开数据库: {}", db.path()),
        None => println!("提示: 尚未绑定数据库,可输入 `.open <file.nodb>` 打开或创建。"),
    }
    println!("输入 `.help` 查看命令用法,输入 `.quit` 退出。");

    // 逐行读取 stdin(交互式 TTY 或管道重定向均可)
    let stdin = std::io::stdin();
    loop {
        // 打印提示符并刷新缓冲区
        print!("nodblite> ");
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

        // 整行按空白拆分得到单词列表
        let parts: Vec<String> = trimmed.split_whitespace().map(|s| s.to_string()).collect();
        match parse_shell(&parts) {
            Ok(ShellCommand::Quit) => {
                println!("再见!");
                break;
            }
            Ok(cmd) => {
                // 命令执行错误打印后继续循环,不退出
                let _ = exec_shell(&mut current, cmd);
            }
            Err(msg) => {
                eprintln!("错误: {}", msg);
            }
        }
    }

    // 释放数据库句柄(Windows 下 redb 独占锁,显式 drop)
    drop(current);
    ExitCode::SUCCESS
}

/// 解析一行 shell 命令。
///
/// 对齐 sqlite3 二元结构:
/// - **点命令**(管理):`.open`/`.databases`/`.collect`/`.help`/`.quit`/`.exit`;
/// - **无点命令**(CRUD):`insert/find/update/delete/count/drop`;
/// - 无点管理别名一律报错(严格点命令)。
pub fn parse_shell(args: &[String]) -> Result<ShellCommand, String> {
    // ──────────────── 点命令(管理) ────────────────
    match args {
        [] => Err("请输入命令,输入 `.help` 查看用法".to_string()),
        [cmd] if cmd == ".quit" || cmd == ".exit" => Ok(ShellCommand::Quit),
        [cmd] if cmd == ".help" => Ok(ShellCommand::Help),
        [cmd] if cmd == ".databases" => Ok(ShellCommand::Databases),
        [cmd] if cmd == ".collect" => Ok(ShellCommand::Collect),
        // .open 无参 = 关闭当前回到未绑定
        [cmd] if cmd == ".open" => Ok(ShellCommand::Open(None)),
        // .open <file>
        [cmd, path] if cmd == ".open" => Ok(ShellCommand::Open(Some(path.clone()))),
        // ──────────────── 无点命令(CRUD) ────────────────
        [cmd, collection, json] if cmd == "insert" => {
            Ok(ShellCommand::Insert(collection.clone(), json.clone()))
        }
        [cmd, collection] if cmd == "count" => {
            Ok(ShellCommand::Count(collection.clone(), "null".to_string()))
        }
        [cmd, collection, filter] if cmd == "count" => {
            Ok(ShellCommand::Count(collection.clone(), filter.clone()))
        }
        [cmd, collection] if cmd == "drop" => Ok(ShellCommand::Drop(collection.clone())),
        // .export <collection> [file] [--append]
        [cmd, collection, rest @ ..] if cmd == ".export" => {
            let mut file: Option<String> = None;
            let mut append = false;
            for arg in rest {
                if arg == "--append" {
                    append = true;
                } else if file.is_none() {
                    file = Some(arg.clone());
                } else {
                    return Err(format!(".export 参数过多: `{}`", arg));
                }
            }
            Ok(ShellCommand::Export {
                collection: collection.clone(),
                file,
                append,
            })
        }
        [cmd] if cmd == ".export" => {
            Err(".export 缺少参数: .export <collection> [file]".to_string())
        }
        [cmd, collection, file] if cmd == ".import" => Ok(ShellCommand::Import {
            collection: collection.clone(),
            file: file.clone(),
        }),
        [cmd, ..] if cmd == ".import" => {
            Err(".import 缺少参数: .import <collection> <file>".to_string())
        }
        [cmd] if cmd == ".stats" => Ok(ShellCommand::Stats),
        [cmd, collection, filter] if cmd == "delete" => {
            Ok(ShellCommand::Delete(collection.clone(), filter.clone()))
        }
        [cmd, collection, filter, update] if cmd == "update" => Ok(ShellCommand::Update(
            collection.clone(),
            filter.clone(),
            update.clone(),
        )),
        [cmd, collection, filter, update] if cmd == "upsert" => Ok(ShellCommand::Upsert(
            collection.clone(),
            filter.clone(),
            update.clone(),
        )),
        // find <collection> [filter] [--sort json] [--skip n] [--limit n]
        [cmd, collection, rest @ ..] if cmd == "find" => parse_shell_find(collection, rest),
        [cmd]
            if cmd == "insert"
                || cmd == "find"
                || cmd == "update"
                || cmd == "upsert"
                || cmd == "delete"
                || cmd == "count"
                || cmd == "drop" =>
        {
            Err(format!("命令 `{}` 缺少参数,输入 `.help` 查看用法", cmd))
        }
        // 无点管理别名一律报错(严格点命令)
        [cmd, ..]
            if cmd == "open"
                || cmd == "create"
                || cmd == "collections"
                || cmd == "databases"
                || cmd == "db" =>
        {
            Err(format!("管理命令必须用点号:`{}` 请写成 `.{}`", cmd, cmd))
        }
        [cmd] if cmd == "quit" || cmd == "exit" || cmd == "help" || cmd == "?" => {
            Err(format!("管理命令必须用点号:`{}` 请写成 `.{}`", cmd, cmd))
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
    let mut projection: Option<String> = None;

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
            "--fields" => {
                i += 1;
                if i >= args.len() {
                    return Err("`--fields` 需要一个 JSON 对象参数".to_string());
                }
                projection = Some(args[i].clone());
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
        projection,
    })
}

/// 分发执行一条 shell 命令(可修改当前数据库绑定)。
fn exec_shell(current: &mut Option<Database>, cmd: ShellCommand) -> ExitCode {
    match cmd {
        ShellCommand::Open(path) => exec_open(current, path.as_deref()),
        ShellCommand::Databases => exec_databases(current),
        ShellCommand::Collect => require_db(current, exec_collect),
        ShellCommand::Insert(collection, json) => {
            require_db(current, |db| exec_insert(db, &collection, &json))
        }
        ShellCommand::Find {
            collection,
            filter,
            sort,
            skip,
            limit,
            projection,
        } => require_db(current, |db| {
            exec_find(
                db,
                &collection,
                &filter,
                sort.as_deref(),
                skip,
                limit,
                projection.as_deref(),
            )
        }),
        ShellCommand::Update(collection, filter, update) => {
            require_db(current, |db| exec_update(db, &collection, &filter, &update))
        }
        ShellCommand::Upsert(collection, filter, update) => {
            require_db(current, |db| exec_upsert(db, &collection, &filter, &update))
        }
        ShellCommand::Delete(collection, filter) => {
            require_db(current, |db| exec_delete(db, &collection, &filter))
        }
        ShellCommand::Count(collection, filter) => {
            require_db(current, |db| exec_count(db, &collection, &filter))
        }
        ShellCommand::Drop(collection) => require_db(current, |db| exec_drop(db, &collection)),
        ShellCommand::Export {
            collection,
            file,
            append,
        } => require_db(current, |db| {
            exec_export(db, &collection, file.as_deref(), append)
        }),
        ShellCommand::Import { collection, file } => {
            require_db(current, |db| exec_import(db, &collection, &file))
        }
        ShellCommand::Stats => require_db(current, exec_stats),
        ShellCommand::Help => {
            print_shell_help();
            ExitCode::SUCCESS
        }
        ShellCommand::Quit => ExitCode::SUCCESS,
    }
}

/// 执行 `.open`:无参 = 关闭当前回到未绑定;带路径 = 关闭当前 → 打开(不存在则创建)并绑定。
fn exec_open(current: &mut Option<Database>, path: Option<&str>) -> ExitCode {
    match path {
        // 无参:关闭当前绑定,回到未绑定状态
        None => {
            drop(current.take());
            println!("已关闭数据库。提示:输入 `.open <file.nodb>` 重新打开或创建数据库。");
            ExitCode::SUCCESS
        }
        // 带路径:打开(不存在则创建)并绑定
        Some(path) => {
            // 关闭当前绑定(Windows redb 独占锁,须先释放旧句柄)
            drop(current.take());
            match open_or_create(path) {
                Ok(db) => {
                    println!("已打开数据库: {}", db.path());
                    *current = Some(db);
                    ExitCode::SUCCESS
                }
                Err(msg) => {
                    eprintln!("打开数据库失败: {}", msg);
                    ExitCode::FAILURE
                }
            }
        }
    }
}

/// 执行 `.databases`:显示当前绑定的数据库文件路径。
fn exec_databases(current: &mut Option<Database>) -> ExitCode {
    match current {
        Some(db) => println!("当前数据库: {}", db.path()),
        None => println!("尚未绑定数据库。"),
    }
    ExitCode::SUCCESS
}

/// 未绑定数据库时的统一提示。
fn no_database_prompt() -> ExitCode {
    eprintln!("尚未打开数据库,请先执行 `open <file.nodb>` 或 `create <file.nodb>`。");
    ExitCode::FAILURE
}

/// 集合命令执行前的门卫:有绑定数据库则执行闭包,否则给出中文提示。
fn require_db<F>(current: &mut Option<Database>, f: F) -> ExitCode
where
    F: FnOnce(&Database) -> ExitCode,
{
    match current {
        Some(db) => f(db),
        None => no_database_prompt(),
    }
}

/// 解析 filter 参数(null 或省略表示匹配全部)。
fn parse_filter(json: &str) -> Result<Filter, String> {
    let value: Value = serde_json::from_str(json).map_err(|e| format!("JSON 解析失败: {}", e))?;
    Filter::from_value(&value).map_err(|e| e.to_string())
}

/// 解析 sort 参数。
fn parse_sort(json: &str) -> Result<SortSpec, String> {
    let value: Value = serde_json::from_str(json).map_err(|e| format!("JSON 解析失败: {}", e))?;
    SortSpec::from_value(&value).map_err(|e| e.to_string())
}

/// 在已打开的数据库上执行 insert。
///
/// JSON 为对象时单条插入;为数组时走 `insert_many` 单事务批量插入
/// (任一主键冲突则整批失败,无部分写入)。
fn exec_insert(db: &Database, collection: &str, json: &str) -> ExitCode {
    let value: Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("文档格式错误: {}", e);
            return ExitCode::FAILURE;
        }
    };

    // 数组 → 单事务批量插入
    if let Value::Array(items) = value {
        let docs = items
            .into_iter()
            .map(|v| Document::from_json(&serde_json::to_string(&v).unwrap_or_default()))
            .collect::<Result<Vec<_>, _>>();
        let docs = match docs {
            Ok(d) => d,
            Err(e) => {
                eprintln!("文档格式错误: {}", e);
                return ExitCode::FAILURE;
            }
        };
        if docs.is_empty() {
            eprintln!("插入失败: 批量插入至少需要一条文档");
            return ExitCode::FAILURE;
        }
        let mut docs = docs;
        return match db.insert_many(collection, &mut docs) {
            Ok(ids) => {
                println!(
                    "批量插入成功: 集合 `{}`,共 {} 条,_id = [{}]",
                    collection,
                    ids.len(),
                    ids.join(", ")
                );
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("插入失败(整批已回滚): {}", e);
                ExitCode::FAILURE
            }
        };
    }

    // 对象 → 单条插入
    let doc = match Document::from_json(&serde_json::to_string(&value).unwrap_or_default()) {
        Ok(doc) => doc,
        Err(e) => {
            eprintln!("文档格式错误: {}", e);
            return ExitCode::FAILURE;
        }
    };
    let mut doc = doc;

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

/// 在已打开的数据库上执行 find。
fn exec_find(
    db: &Database,
    collection: &str,
    filter_json: &str,
    sort_json: Option<&str>,
    skip: usize,
    limit: Option<usize>,
    projection_json: Option<&str>,
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

    // 解析 projection(可选)
    let projection = match projection_json {
        Some(json) => match serde_json::from_str::<Value>(json)
            .map_err(|e| e.to_string())
            .and_then(|v| Projection::from_value(&v).map_err(|e| e.to_string()))
        {
            Ok(p) => Some(p),
            Err(e) => {
                eprintln!("projection 错误: {}", e);
                return ExitCode::FAILURE;
            }
        },
        None => None,
    };

    // 带排序/投影时走 find_page_total 两遍法:一次扫描同时得到「匹配总数 + 当页」,
    // 只收集排序键排序,再按主键回取,内存 O(n × 排序键大小);
    // 无排序且无投影保持流式过滤。
    let result = if sort.is_some() || projection.is_some() {
        db.find_page_total(
            collection,
            &filter,
            sort.as_ref(),
            skip,
            limit,
            projection.as_ref(),
        )
    } else {
        db.find_with_options(
            collection,
            &filter,
            &FindOptions {
                sort: None,
                projection: None,
                skip,
                limit,
            },
        )
        .map(|docs| (docs.len(), docs.to_vec()))
    };
    match result {
        Ok((total, docs)) => {
            println!(
                "找到 {} 条文档(集合 `{}`,本页 {} 条):",
                total,
                collection,
                docs.len()
            );
            for doc in &docs {
                println!(
                    "  {}",
                    serde_json::to_string_pretty(doc).unwrap_or_default()
                );
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("查询失败: {}", e);
            ExitCode::FAILURE
        }
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

    // 执行更新
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

/// 在已打开的数据库上执行 upsert。
fn exec_upsert(db: &Database, collection: &str, filter_json: &str, update_json: &str) -> ExitCode {
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

    // 执行 upsert
    match db.update_or_insert(collection, &filter, &update) {
        Ok(UpsertResult::Updated(n)) => {
            println!("更新成功: 集合 `{}` 共修改 {} 条文档", collection, n);
            ExitCode::SUCCESS
        }
        Ok(UpsertResult::Inserted(id)) => {
            println!("插入成功: 集合 `{}`, _id = {}", collection, id);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("upsert 失败: {}", e);
            ExitCode::FAILURE
        }
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

/// 在已打开的数据库上执行 .collect(列出全部集合)。
fn exec_collect(db: &Database) -> ExitCode {
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

/// 在已打开的数据库上执行 drop。
fn exec_drop(db: &Database, collection: &str) -> ExitCode {
    match db.drop_collection(collection) {
        Ok(n) => {
            println!(
                "已删除集合 `{}` 及其全部 {} 条文档(不可恢复)",
                collection, n
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("删除集合失败: {}", e);
            ExitCode::FAILURE
        }
    }
}

/// 导出集合为 JSON Lines。
///
/// 一行一条文档(含 `_id`),便于 `wc -l`/`head`/增量追加与流式处理。
/// 为避免大集合一次性入内存,这里**逐条写出**(仅命中该集合的全部文档)。
fn exec_export(db: &Database, collection: &str, file: Option<&str>, append: bool) -> ExitCode {
    // 目标:stdout 或文件
    let mut writer: Box<dyn Write> = match file {
        Some(path) => match std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(append)
            .truncate(!append)
            .open(path)
        {
            Ok(f) => Box::new(std::io::BufWriter::new(f)),
            Err(e) => {
                eprintln!("无法写入 {}: {}", path, e);
                return ExitCode::FAILURE;
            }
        },
        None => Box::new(std::io::stdout()),
    };

    // 逐条遍历并写出(collection 不存在时自然导出 0 条)
    let filter = match Filter::from_value(&Value::Null) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("内部错误: {}", e);
            return ExitCode::FAILURE;
        }
    };
    // 按批取页写出,避免大集合一次性把全部文档读进内存
    const BATCH: usize = 1000;
    let mut written = 0usize;
    let mut skip = 0usize;
    loop {
        let page = match db.find_page(collection, &filter, None, skip, Some(BATCH)) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("导出失败: {}", e);
                return ExitCode::FAILURE;
            }
        };
        if page.is_empty() {
            break;
        }
        for doc in &page {
            let line = serde_json::to_string(doc).unwrap_or_default();
            if let Err(e) = writeln!(writer, "{}", line) {
                eprintln!("写入失败: {}", e);
                return ExitCode::FAILURE;
            }
            written += 1;
        }
        if page.len() < BATCH {
            break;
        }
        skip += page.len();
    }

    if let Err(e) = writer.flush() {
        eprintln!("写入失败: {}", e);
        return ExitCode::FAILURE;
    }
    match file {
        Some(path) => println!("已导出 {} 条文档到 {}", written, path),
        None => eprintln!("已导出 {} 条文档", written),
    }
    ExitCode::SUCCESS
}

/// 从 JSON Lines(或整个文件是一个 JSON 数组)导入集合。
///
/// 先全量解析校验,再用 `insert_many` **单事务**写入 ——
/// 任一条格式错误则整批不写,不会留下半个集合。
fn exec_import(db: &Database, collection: &str, path: &str) -> ExitCode {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("无法读取 {}: {}", path, e);
            return ExitCode::FAILURE;
        }
    };

    let mut docs: Vec<Document> = Vec::new();
    let trimmed = content.trim_start();
    if trimmed.starts_with('[') {
        // 整个文件是一个 JSON 数组
        match serde_json::from_str::<Vec<Value>>(trimmed) {
            Ok(items) => {
                for (i, item) in items.into_iter().enumerate() {
                    match Document::from_json(&item.to_string()) {
                        Ok(d) => docs.push(d),
                        Err(e) => {
                            eprintln!("第 {} 条文档格式错误: {}", i + 1, e);
                            return ExitCode::FAILURE;
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("JSON 数组解析失败: {}", e);
                return ExitCode::FAILURE;
            }
        }
    } else {
        // JSON Lines:逐行解析,忽略空行
        for (i, line) in content.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match Document::from_json(line) {
                Ok(d) => docs.push(d),
                Err(e) => {
                    eprintln!("第 {} 行格式错误: {}", i + 1, e);
                    return ExitCode::FAILURE;
                }
            }
        }
    }

    if docs.is_empty() {
        println!("文件为空,未导入任何文档");
        return ExitCode::SUCCESS;
    }

    // 单事务批量写入
    match db.insert_many(collection, &mut docs) {
        Ok(_) => {
            println!("已导入 {} 条文档到集合 `{}`", docs.len(), collection);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("导入失败(整批未写入): {}", e);
            ExitCode::FAILURE
        }
    }
}

/// 显示数据库统计信息。
fn exec_stats(db: &Database) -> ExitCode {
    let path = db.path();
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let collections = match db.collections() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("读取集合列表失败: {}", e);
            return ExitCode::FAILURE;
        }
    };

    println!("数据库: {}", path);
    println!("文件大小: {}", format_size(size));
    println!("集合数量: {}", collections.len());
    if collections.is_empty() {
        return ExitCode::SUCCESS;
    }
    println!();
    let filter = match Filter::from_value(&Value::Null) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("内部错误: {}", e);
            return ExitCode::FAILURE;
        }
    };
    let mut total = 0usize;
    for name in &collections {
        match db.count(name, &filter) {
            Ok(n) => {
                total += n;
                println!("  {:<24} {:>10} 条", name, n);
            }
            Err(e) => {
                eprintln!("统计集合 `{}` 失败: {}", name, e);
                return ExitCode::FAILURE;
            }
        }
    }
    println!("  {:<24} {:>10} 条", "合计", total);
    ExitCode::SUCCESS
}

/// 把字节数格式化为人类可读形式。
fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else {
        format!("{:.1} {}", value, UNITS[unit])
    }
}

/// 打印 shell 内命令用法。
fn print_shell_help() {
    println!("nodblite shell 内命令(不带文件路径):");
    println!();
    println!("数据库管理(点命令):");
    println!("  .open <file.nodb>       打开数据库(不存在则自动创建);无参 = 关闭当前");
    println!("  .databases              显示当前绑定的数据库文件");
    println!();
    println!("集合操作:");
    println!(
        "  insert <collection> <json>            插入文档(json 为对象单条;为数组则单事务批量)"
    );
    println!("  find <collection> [filter]            查询文档(输出匹配总数与本页条数)");
    println!("       [--sort <json>] [--skip <n>] [--limit <n>] [--fields <json>]");
    println!("       --fields 字段投影,如 '{{\"name\": 1, \"_id\": 0}}'");
    println!(
        "  update <collection> <filter> <update> 更新匹配文档(单事务原子写回;{{\"_id\":…}} 走主键快速路径)"
    );
    println!("  upsert <collection> <filter> <update> 更新或插入(未命中则按 filter 等值字段新建)");
    println!(
        "  delete <collection> <filter>          删除匹配文档(单事务原子删除;{{\"_id\":…}} 走主键快速路径)"
    );
    println!("  count <collection> [filter]           统计匹配条数");
    println!("  .collect                              列出全部集合");
    println!("  drop <collection>                     删除整个集合(不可恢复)");
    println!();
    println!("导入导出与统计:");
    println!(
        "  .export <collection> [file] [--append]  导出为 JSON Lines(省略文件则输出到 stdout)"
    );
    println!("  .import <collection> <file>             从 JSON Lines(或 JSON 数组)导入(单事务)");
    println!();
    println!("其他:");
    println!("  .stats                                显示文件大小与各集合文档数");
    println!("  .help                                 显示本帮助");
    println!("  .quit / .exit                         退出 shell");
    println!();
    println!("示例:");
    println!("  .open data.nodb");
    println!("  insert users '{{\"name\":\"张三\",\"age\":30}}'");
    println!("  insert users '[{{\"name\":\"张三\"}},{{\"name\":\"李四\"}}]'   批量插入(单事务)");
    println!("  find users '{{\"age\": {{\"$gte\": 18}}}}' --sort '{{\"age\": -1}}' --limit 10");
    println!("  find users '{{}}' --fields '{{\"name\": 1}}'   列表页只取需要的字段");
    println!("  update users '{{\"_id\":\"u1\"}}' '{{\"$set\":{{\"vip\":true}}}}'");
    println!("  upsert users '{{\"email\":\"a@b.c\"}}' '{{\"$set\":{{\"vip\":true}}}}'");
    println!("  update users '{{\"_id\":\"u1\"}}' '{{\"$inc\":{{\"views\":1}}}}'");
    println!("  delete users '{{\"age\": {{\"$lt\": 18}}}}'");
    println!("  .export users /tmp/users.jsonl     导出为 JSON Lines");
    println!("  .import users /tmp/users.jsonl     单事务导入");
    println!("  .stats                             看文件大小与各集合条数");
}

/// 打印顶层用法。
fn print_help() {
    println!("nodblite —— 像 SQLite 一样小巧的单文件 NoSQL 数据库");
    println!();
    println!("用法:");
    println!("  nodblite                  直接进入交互式 shell(未绑定数据库)");
    println!("  nodblite <file.nodb>      打开数据库进入 shell,文件不存在则自动创建");
    println!("  nodblite -h|--help|help   显示本帮助");
    println!();
    println!("进入 shell 后,输入 `help` 查看 shell 内命令(提示符 `nodblite> `)。");
    println!();
    println!("说明:");
    println!("  - 文件后缀必须为 .nodb,不支持其他后缀或无后缀");
    println!("  - 数据库为单文件,可直接复制粘贴迁移");
    println!("  - 文档顶层必须是 JSON 对象;_id 缺省时自动生成");
    println!("  - filter 操作符:$eq/$ne/$gt/$gte/$lt/$lte/$in/$nin/$regex/$elemMatch");
    println!(
        "    $elemMatch:数组字段至少一个元素满足条件,如 {{\"tags\": {{\"$elemMatch\": {{\"$gte\": 10}}}}}}"
    );
    println!("  - 顶层逻辑:$and/$or/$nor/$not;字段支持点号路径(a.b.c)");
    println!(
        "  - update 操作符:$set/$unset/$inc/$mul/$min/$max/$rename/$push/$addToSet/$currentDate;主键 _id 不可修改"
    );
    println!("    $push/$addToSet 支持 $each(以及 $push 的 $position/$slice/$sort)");
    println!("  - 日期:写成 {{\"$date\": \"2026-09-16T10:00:00Z\"}} 或 {{\"$date\": 毫秒时间戳}},");
    println!("    按时间戳比较与排序(时区偏移自动归一),存储仍是普通 JSON");
}
