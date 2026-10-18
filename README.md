# nosqlite

**像 SQLite 一样小巧的单文件 NoSQL 数据库** —— MongoDB 风格的文档模型,单文件(`.nodb`)零配置部署。

```bash
cargo add nosqlite
```

## 它是什么

nosqlite 把 MongoDB「顺手」的文档语义装进一个像 SQLite 一样可直接拷贝的单文件里:

- **单文件存储**:整个数据库就是 一个 `.nodb` 文件,复制即迁移、备份即拷贝;
- **文档模型**:JSON 文档 + 自动主键 `_id`,集合隔离,无需建表;
- **MongoDB 风格查询**:`$eq/$ne/$gt/$gte/$lt/$lte/$in/$nin/$regex/$elemMatch` + 字段级/顶层 `$not`,顶层 `$and/$or/$nor`;数组包含与点号路径穿透数组(语义见下方「与 MongoDB 的差异」);
- **更新操作符**:`$set`(点号路径赋值,嵌套自动创建)、`$unset`(删除字段);
- **内存友好**:流式扫描与过滤,内存 O(命中数) 而非 O(集合大小);排序分页走两遍法(仅底层 `scan` 会全量物化,文档已警示仅限小集合)。
- **主键直查**:按 `_id` 的读/改/删走 B+树直达,不经全表扫描;
- **批量写入**:`insert_many` 单事务批量,一次提交摊薄落盘成本;
- **Date 类型**:`{"$date": "2026-09-16T10:00:00Z"}`,按时间戳比较排序,时区偏移自动归一;
- **批量更新/删除**:`update`/`delete` 匹配多条时**合并为一次提交**,且整体原子(不会留下改了一半的状态);
- **列表页一次扫描**:`find_page_total` 同时返回「匹配总数 + 当页文档」,省掉一次全表 `count`;
- **字段投影**:`Projection` / CLI `--fields`,列表页只取需要的字段;
- **复合值可比较**:数组/对象按 BSON 风格递归比较,可排序,`$eq`/`$in` 为精确深度匹配;
- **附带 CLI**:`nosqlite <file.nodb>` 进入 sqlite3 风格 REPL(点命令管理 + 无点 CRUD)。

底层是 [redb](https://crates.io/crates/redb)(纯 Rust 单文件 B+树 KV 存储),之上提供文档语义层。`.nodb` 格式版本 v1。

## 何时**不要**用

诚实地说,nosqlite 不适合以下场景:

- **需要跨进程/跨机器并发写**:单写事务模型,写入被串行化(多线程写无加速比);
- **需要聚合管道、事务(跨文档)、索引下推**:未实现;大数据量排序分页是全量抽排序键的两遍法;
- **需要 MongoDB 兼容协议 / 副本集 / 分片**:不支持;
- **超大数据量**(单集合 亿 级):未验证;
- **需要丰富的类型系统**:文档值即 JSON 值(比较遵循 BSON 风格类型序)。

适合的:小工具、CLI、原型、桌面应用、单机 CMS/后台 —— 一个文件解决存储,不想要数据库服务器。

## 与 MongoDB 的差异

查询语义与更新操作符**以 MongoDB 为准**。v0.1.2 修正了 6 处曾经的语义偏差;仍存在下列差异,迁移前请扫一眼:

### 已与 MongoDB 对齐(v0.1.2)

| 语义 | 说明 |
|---|---|
| 同层条件 AND | `{"status":"ok","$or":[...]}` 中字段条件与 `$or` 是 **AND**(旧版误为 OR) |
| 数组包含 | `{"tags":"rust"}` 可命中 `{"tags":["rust","db"]}` |
| `$in` 展开 | `{"tags":{"$in":["rust"]}}` 命中任何含 `rust` 的数组字段 |
| 点号穿透数组 | `{"items.name":"x"}` 会遍历 `items` 数组元素的 `name` |
| 缺失 = null | `{"deleted_at":null}` 匹配**字段缺失**的文档;`{"$ne":null}` 不匹配 |
| 字段级 `$not` | `{"age":{"$not":{"$gt":18}}}` |
| Date 按时间比较 | `{"$date":"2026-09-16T10:00:00Z"}`,时区偏移归一(v0.1.3) |

> ⚠️ **升级提示**:前四条是破坏性变更 —— v0.1.1 及更早版本上,`$or` 与字段条件为 OR 关系、数组字段的标量等值不命中、点号路径不穿透数组。若你在旧版上依赖这些行为,升级后查询结果会变化。

### 仍未支持

- **检测/元操作符**:`$exists`、`$type`、`$size`、`$all`
- **更新操作符**:仅 `$set`/`$unset`;无 `$inc`/`$push`/`$pull`/`$addToSet`/`$rename`/`$currentDate`
- **upsert**:`update` 只更新已存在的文档
- **聚合管道**:`aggregate` 整体未实现(`$match`/`$group`/`$lookup`/`$unwind`/`$facet`…)
- **索引**:仅 `_id` 主键索引;无二级/复合/唯一/TTL 索引,所有非 `_id` 查询都是全表扫描
- **事务**:仅单集合内的写事务;无跨集合/跨文档显式事务会话
- **类型系统**:文档值即 JSON。**Date 用 Extended JSON 的 `{"$date": ...}` 表示**(按时间戳比较与排序,存储仍是普通 JSON);无 `ObjectId`/`Binary`/`Decimal128` 类型,`_id` 是 24 位十六进制字符串
- **Server / 协议**:无 MongoDB wire 协议,官方 driver 与 `mongosh` 无法直连
- **其他**:`distinct`、`explain`、change stream、schema validation、`$regex` 的 `$options`

## 快速上手

```rust
use nosqlite::{query::Filter, Document, Database};
use serde_json::json;

let db = Database::create("app.nodb")?;

// 插入(自动生成 _id)
let mut doc = Document::from_json(r#"{"name": "张三", "age": 30}"#)?;
let id = db.insert("users", &mut doc)?;

// 查询
let filter = Filter::from_value(&json!({"age": {"$gte": 18}}))?;
for doc in db.find("users", &filter)? {
    println!("{}", doc);
}

// 按 _id 更新(主键直达,不经扫描)
let update = nosqlite::query::Update::from_value(&json!({"$set": {"vip": true}}))?;
db.update_by_id("users", &id, &update)?;
```

CLI:

```text
$ nosqlite app.nodb
nosqlite> insert users {"name": "张三", "age": 30}
插入成功: 集合 `users`, _id = 6aaa...
nosqlite> find users {"age": {"$gte": 18}} --sort {"age": -1} --limit 10
```

## 性能参考

> ⚠️ **写入数字强依赖平台的落盘(fsync)语义**,引用时必须标注平台。
> 数据为 10k 文档、3 轮取中位数;Linux 数据来自 GitHub Actions 共享 runner,为指示性参考。

| 场景(10k 文档) | macOS(Apple Silicon) | Linux(CI runner) |
|---|---|---|
| 逐条插入(每条一个事务) | ~185 条/秒 | ~1,400 条/秒 |
| `insert_many` 批量 | ~44,000 条/秒 | ~300,000 条/秒 |
| 按 `_id` 读 | 亚毫秒 | 亚毫秒 |
| `find` 全量扫描 | ~8 µs/条 | ~1.2 µs/条 |
| `find_page` 排序+分页(内存增量) | 12 MiB @100k | ≈ 0 MiB @100k |
| `update` 批量 1000 条(单事务) | ~25 ms | — |

差异根源:Rust std 在 macOS 把 `sync_data/sync_all` 映射到 `F_FULLFSYNC`(强制磁盘写缓存落盘),实测比 POSIX `fsync` 慢 **66 倍**;Linux 上映射到 `fdatasync`,无放大。逐条写入的耗时 ≈ 一次提交的落盘成本 —— 要吞吐就用 `insert_many` 批量(见「持久化语义」)。

## 内存与持久化语义

- **内存**:除旧接口 `scan`(全量物化,标注 deprecated)外,`find/count/update/delete` 均流式处理;带排序的查询用 `find_page`(两遍法:先收集「排序键 + _id」排序,再按主键回取页内文档),内存 O(n × 排序键大小),与命中文档大小无关。
- **持久化**:每次写事务提交都会落盘(redb 默认 `Durability::Immediate`)。**不提供**关闭落盘的选项 —— 防止静默丢写入;需要吞吐时用 `insert_many` / 批量 `update` / 批量 `delete` 把 N 次提交合并为 1 次。
- **批量写的原子性**:`insert_many`、`update`(多命中)与 `delete`(多命中)都在单个写事务内完成 —— 要么全部生效,要么整体回滚。代价是收集阶段内存为 O(命中数)。
- **崩溃恢复**:单文件 B+树(redb),进程崩溃后文件保持一致。
- **格式稳定**:`.nodb` v1;本库 0.x 阶段 API 可能调整,但存储格式保持兼容。

## 开发

```bash
cargo test --all-targets     # 275 例测试
cargo clippy --all-targets -- -D warnings
cargo run --release -- app.nodb   # CLI
```

基准:`cargo run --release --example bench_write -- 10000 3`(写路径归因)、`--example bench -- 10000 100000`(读路径与内存)。CI 在 GitHub Actions(ubuntu + macos);基准为手动/tag 触发的工作流。

## License

Apache-2.0(见 [LICENSE](LICENSE))。
