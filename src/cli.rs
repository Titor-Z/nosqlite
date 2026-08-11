//! 类 sqlite3 的命令行工具。
//!
//! 支持子命令:
//! - `nosqlite create <file.nodb>`:创建新数据库;
//! - `nosqlite open   <file.nodb>`:打开并校验已有数据库;
//! - `nosqlite insert <file.nodb> <collection> <json>`:向集合插入一条文档;
//! - `nosqlite find   <file.nodb> <collection> [filter] [--sort <json>] [--skip <n>] [--limit <n>]`:
//!   查询集合内文档。
//!
//! 后续里程碑会扩展 update/delete 等完整命令。

use std::path::Path;
use std::process::ExitCode;

use serde_json::Value;

use crate::database::Database;
use crate::document::Document;
use crate::engine::{RedbEngine, Storage};
use crate::query::{Filter, FindOptions, SortSpec};

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
    /// 显示帮助信息。
    Help,
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
        [sub, path, collection, json] if sub == "insert" => Ok(Command::Insert(
            path.clone(),
            collection.clone(),
            json.clone(),
        )),
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
        Command::Help => {
            print_help();
            ExitCode::SUCCESS
        }
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

/// 执行 insert 子命令:打开数据库 → 解析文档 → 插入 → 打印结果。
fn run_insert(path: &str, collection: &str, json: &str) -> ExitCode {
    // 1. 打开数据库
    let db = match Database::open(Path::new(path)) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("打开数据库失败: {}", e);
            return ExitCode::FAILURE;
        }
    };

    // 2. 解析 JSON 为文档
    let mut doc = match Document::from_json(json) {
        Ok(doc) => doc,
        Err(e) => {
            eprintln!("文档格式错误: {}", e);
            return ExitCode::FAILURE;
        }
    };

    // 3. 插入文档
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

/// 执行 find 子命令:打开数据库 → 解析 filter/sort → 查询 → 打印结果。
fn run_find(
    path: &str,
    collection: &str,
    filter_json: &str,
    sort_json: Option<&str>,
    skip: usize,
    limit: Option<usize>,
) -> ExitCode {
    // 1. 打开数据库
    let db = match Database::open(Path::new(path)) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("打开数据库失败: {}", e);
            return ExitCode::FAILURE;
        }
    };

    // 2. 解析 filter
    let filter = match parse_filter(filter_json) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("filter 错误: {}", e);
            return ExitCode::FAILURE;
        }
    };

    // 3. 解析 sort(可选)
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

    // 4. 组装查询选项并执行
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

/// 打印帮助信息。
fn print_help() {
    println!("nosqlite —— 像 SQLite 一样小巧的单文件 NoSQL 数据库");
    println!();
    println!("用法:");
    println!("  nosqlite create <file.nodb>                       创建新数据库");
    println!("  nosqlite open   <file.nodb>                       打开并校验已有数据库");
    println!("  nosqlite insert <file.nodb> <collection> <json>   向集合插入一条文档");
    println!("  nosqlite find   <file.nodb> <collection> [filter] 查询文档");
    println!("                 [--sort <json>] [--skip <n>] [--limit <n>]");
    println!("  nosqlite help                                     显示本帮助");
    println!();
    println!("示例:");
    println!("  nosqlite insert data.nodb users '{{\"name\":\"张三\",\"age\":30}}'");
    println!("  nosqlite find   data.nodb users '{{\"age\": {{\"$gte\": 18}}}}'");
    println!("  nosqlite find   data.nodb users '{{\"name\": {{\"$regex\": \"^张\"}}}}'");
    println!("  nosqlite find   data.nodb users null --sort '{{\"age\": -1}}' --limit 10");
    println!();
    println!("说明:");
    println!("  - 文件后缀必须为 .nodb,不支持其他后缀或无后缀");
    println!("  - 数据库为单文件,可直接复制粘贴迁移");
    println!("  - 文档顶层必须是 JSON 对象;_id 缺省时自动生成");
    println!("  - filter 操作符:$eq/$ne/$gt/$gte/$lt/$lte/$in/$nin/$regex");
    println!("  - 顶层逻辑:$and/$or/$nor/$not;字段支持点号路径(a.b.c)");
}
