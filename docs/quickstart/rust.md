# 快速开始:Rust 内嵌

## 安装

```bash
cargo add nodblite
```

需要 Rust 1.85+(edition 2024)。

## 打开与创建

```rust
use nodblite::Database;

// 创建(文件已存在会失败,防误覆盖)
let db = Database::create("app.nodb")?;

// 打开已有库
let db = Database::open("app.nodb")?;
```

> 文件后缀**必须是 `.nodb`**,其他后缀一律拒绝 —— 这是刻意的,保证「单文件可识别」。

## 增删改查

```rust
use nodblite::{Database, Document};
use nodblite::query::{Filter, FindOptions, Update};
use serde_json::json;

let db = Database::open("app.nodb")?;

// 插入(未给 _id 则自动生成)
let mut doc = Document::from_json(r#"{"name": "张三", "age": 30, "tags": ["admin"]}"#)?;
let id = db.insert("users", &mut doc)?;

// 批量插入(单事务,快得多)
let mut docs: Vec<Document> = (0..1000)
    .map(|i| Document::from_json(&json!({"n": i}).to_string()))
    .collect::<Result<_, _>>()?;
db.insert_many("users", &mut docs)?;

// 查询
let filter = Filter::from_value(&json!({"age": {"$gte": 18}}))?;
let adults = db.find("users", &filter)?;

// 排序 + 分页
let opts = FindOptions {
    sort: Some(nodblite::query::SortSpec::from_value(&json!({"age": -1}))?),
    skip: 0,
    limit: Some(20),
    ..Default::default()
};
let page = db.find_with_options("users", &filter, &opts)?;

// 更新
let upd = Update::from_value(&json!({"$inc": {"age": 1}}))?;
db.update("users", &filter, &upd)?;

// 按主键直查(走 B+树,不等同于全表扫)
let one = db.get_by_id("users", &id)?;

// 删除
db.delete("users", &filter)?;
```

## 关键差异:何时用哪个 API

| 需求 | 用这个 | 说明 |
|---|---|---|
| 全量读、结果集不大 | `find` | 流式过滤,内存 O(命中数) |
| 分页列表 | `find_page_total` | **一次扫描**同时拿到总数 + 当页 |
| 只需部分字段 | 加 `Projection` | 减少序列化开销 |
| 按 `_id` 单条操作 | `get_by_id` / `update_by_id` / `delete_by_id` | 主键直达,毫秒级 |
| 大量写入 | `insert_many` | 单事务,比循环 insert 快两个数量级 |
| 需要知道是否走索引 | `explain` | 返回 `IXSCAN` / `COLLSCAN` |

## 索引

```rust
// 单字段
db.create_index("users", "age_1", "age", false, false)?;

// 复合(遵循 MongoDB 前缀原则)
db.create_index_composite("users", "status_age", &["status", "age"], false, false)?;

// 降序
db.create_index_with_directions("users", "age_-1", &["age"], &[-1], false, false)?;

// 唯一
db.create_index("users", "email_1", "email", true, false)?;

// 看查询会怎么走
let plan = db.explain("users", &json!({"age": {"$gte": 18}}), None)?;
println!("{}", plan["stage"]); // "IXSCAN" 或 "COLLSCAN"
```

## 错误处理

错误码是结构化的,可程序判断:

```rust
match db.insert("users", &mut doc) {
    Err(e) => {
        println!("code = {}", e.code().as_str()); // 如 "DUPLICATE_KEY"
    }
    _ => {}
}
```

## 完整示例

参见仓库 [`examples/`](https://github.com/Titor-Z/nosqlite/tree/main/examples),包含基准与并发示例。
