# nosqlite

**像 SQLite 一样小巧的单文件 NoSQL 数据库** —— MongoDB 风格的文档模型,单文件(`.nodb`)零配置部署。

```bash
cargo add nosqlite
```

## 它是什么

nosqlite 把 MongoDB「顺手」的文档语义装进一个像 SQLite 一样可直接拷贝的单文件里:

- **单文件存储**:整个数据库就是 一个 `.nodb` 文件,复制即迁移、备份即拷贝;
- **文档模型**:JSON 文档 + 自动主键 `_id`,集合隔离,无需建表;
- **MongoDB 风格查询**:`$eq/$ne/$gt/$gte/$lt/$lte/$in/$nin/$regex/$elemMatch/$exists/$type/$all/$size` + 字段级/顶层 `$not`,顶层 `$and/$or/$nor`;数组包含与点号路径穿透数组(语义见下方「与 MongoDB 的差异」);
- **更新操作符**:`$set`/`$unset`/`$inc`/`$mul`/`$min`/`$max`/`$rename`/`$push`/`$addToSet`/`$currentDate`(`$push`/`$addToSet` 支持 `$each`,另有 `$position`/`$slice`/`$sort`);
- **upsert**:`update_or_insert` —— 命中则更新,未命中按 filter 等值字段新建;
- **二级索引**:`create_index(collection, name, field, unique, sparse)` —— 唯一/稀疏/multikey/嵌套路径,写入同事务维护;索引键用 **order-preserving 编码**,字节序即 BSON 值序;
- **内存友好**:流式扫描与过滤,内存 O(命中数) 而非 O(集合大小);排序分页走两遍法(仅底层 `scan` 会全量物化,文档已警示仅限小集合)。
- **主键直查**:按 `_id` 的读/改/删走 B+树直达,不经全表扫描;
- **批量写入**:`insert_many` 单事务批量,一次提交摊薄落盘成本;
- **Date 类型**:`{"$date": "2026-09-16T10:00:00Z"}`,按时间戳比较排序,时区偏移自动归一;
- **批量更新/删除**:`update`/`delete` 匹配多条时**合并为一次提交**,且整体原子(不会留下改了一半的状态);
- **列表页一次扫描**:`find_page_total` 同时返回「匹配总数 + 当页文档」,省掉一次全表 `count`;
- **字段投影**:`Projection` / CLI `--fields`,列表页只取需要的字段;
- **复合值可比较**:数组/对象按 BSON 风格递归比较,可排序,`$eq`/`$in` 为精确深度匹配;
- **附带 CLI**:`nosqlite <file.nodb>` 进入 sqlite3 风格 REPL(点命令管理 + 无点 CRUD);
- **导入导出与统计**:`.export`(JSON Lines)/`.import`(JSONL 或 JSON 数组,单事务)/`.stats`。

底层是 [redb](https://crates.io/crates/redb)(纯 Rust 单文件 B+树 KV 存储),之上提供文档语义层。`.nodb` 格式版本 v1。

## 路线:双形态

nosqlite 有两个形态,共用同一套查询语义:

| 形态 | 形态说明 | 状态 |
|---|---|---|
| **嵌入式(C ABI)** | 像 SQLite 一样被任何语言**进程内**调用,无需额外服务进程;适合本地 GUI/App、CLI、桌面软件 | ✅ **已实现**(`include/nosqlite.h`) |
| **服务端(wire 协议)** | `nosqlite-server` 监听 `mongodb://`,第三方 MongoDB driver 直连,可替换 mongodb-server | ✅ **已实现**(Go 驱动验收通过) |

当前已完成端侧全部能力(Rust 内嵌 API + CLI)。

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
| 类型序 | 完整对齐 MongoDB 官方 BSON 序 `MinKey < Null < Number < String < Object < Array < Binary < ObjectId < Bool < Date < Timestamp < Regex < MaxKey`(v0.4.1) |

> ⚠️ **升级提示**:前四条是破坏性变更 —— v0.1.1 及更早版本上,`$or` 与字段条件为 OR 关系、数组字段的标量等值不命中、点号路径不穿透数组。若你在旧版上依赖这些行为,升级后查询结果会变化。

### 仍未支持

- **更新操作符**:无数组过滤器 `$[]`/`$[id]`
- **投影**:`$slice`/`$elemMatch`/位置运算符 `{"arr.$": 1}` 均已支持
- **upsert 的初始值**:仅能由 filter 的**顶层等值字段**反推;filter 含 `$gt` 等操作符时报错(不猜)
- **聚合管道**:仅 `$match`/`$sort`/`$limit`/`$skip`/`$project`/`$group`/`$addFields`;无 `$unwind`/`$lookup`/`$facet`
- **索引**:无 TTL 索引;规划器是纯规则的,无区分度估算
- **事务**:仅单集合内的写事务;无跨集合/跨文档显式事务会话
- **类型系统**:文档值即 JSON,**富类型用 MongoDB Extended JSON 表示** —— `{"$date": ...}`(按时间戳比较排序)、`{"$oid": ...}` ≡ 24 位 hex 字符串、`{"$numberLong": ...}`/`{"$numberDouble": ...}` ≡ 数字;存储仍是普通 JSON。无 `Binary`/`Decimal128`/`Timestamp`/`MinKey` 等剩余 BSON 类型
- **Server / 协议**:无 MongoDB wire 协议,官方 driver 与 `mongosh` 无法直连
- **其他**:`distinct`、`explain`、change stream、schema validation、`$regex` 的 `$options`

### 跨语言契约

为 C ABI(`nosqlite-ffi`)与 MongoDB wire 协议共用,以下三项已冻结:

| 契约 | 说明 |
|---|---|
| **错误码** | `nosqlite::ErrorCode`,`as_str()` 如 `DUPLICATE_KEY`/`INVALID_FILTER`,`as_i32()` 供 C 侧返回(0 保留给成功) |
| **字段序** | 文档字段顺序**保持写入顺序**(已开启 `serde_json/preserve_order`),投影后 `_id` 恒在最前 |
| **Extended JSON** | `$date`/`$oid`/`$numberLong`/`$numberDouble` 在比较与匹配时按真实类型归一,存储层不改写 |

> 错误的中文文案(`Display`)仅供 CLI 展示;**程序化判断请一律用 `code()`**。

### 数据导入导出

- 格式是 **JSON Lines**(一行一条文档),便于 `wc -l`、`head`、增量追加与流式处理;
- `.import` 同时兼容「整个文件是一个 JSON 数组」;
- 导入**先全量校验再用单事务写入** —— 任一条格式错误或主键冲突,整批不写;
- `.export` 默认覆盖,`--append` 追加;省略文件名则输出到 stdout(可管道)。

## 嵌入到任意语言(C ABI)

构建后得到 `libnosqlite.dylib` / `.so` / `.dll`,配合 `include/nosqlite.h` 即可被任何语言**进程内**调用 —— 没有服务进程,没有网络。

```c
#include "nosqlite.h"

NosqliteDb *db = nosqlite_open("app.nodb", 1);
NosqliteResult *r = nosqlite_insert(db, "users", "{\"name\":\"张三\"}");
char id[64];
snprintf(id, sizeof(id), "%.*s", (int)r->value.len, r->value.ptr);
nosqlite_result_free(r);            // 整块释放,只需一次

/* 游标:大结果集逐条取,内存 O(批大小) */
NosqliteCursor *cur = nosqlite_find_cursor(db, "users", NULL, NULL, NULL, 0, 0, 1);
const char *doc;
while ((doc = nosqlite_cursor_next(cur)) != NULL) {
    puts(doc);
}
nosqlite_cursor_close(cur);

nosqlite_close(db);
```

设计上**骨架照搬 SQLite**(不透明句柄 + `int` 错误码 + 游标),但修掉它因 2004 年 ABI 冻结而做不到的三件事:

| 现代化点 | SQLite | nosqlite |
|---|---|---|
| 结果释放 | 每个返回值单独 `sqlite3_free` | **arena,一次 `nosqlite_result_free`** |
| 错误信息 | 一个笼统错误码 | **结构化**:`{code, code_str, message, op}` |
| 接口演进 | 只能堆 `_v2`/`_v3` | **`struct_size` 版本化 options** |

### 获取库

两种方式,任选:

**① 预编译包**(推荐):从 [GitHub Releases](https://github.com/Titor-Z/nosqlite/releases) 下载对应平台的 `nosqlite-ffi-<platform>.tar.gz`,内含动态库、静态库与 `include/nosqlite.h`,并附 `.sha256` 校验。

| 平台 | 动态库 | 静态库 |
|---|---|---|
| Linux x86_64 | `libnosqlite.so` | `libnosqlite.a` |
| macOS aarch64 | `libnosqlite.dylib` | `libnosqlite.a` |
| Windows x86_64 | `nosqlite.dll` | `nosqlite.lib` |

**② 本地构建**:`cargo build --release`,产物在 `target/release/`。

### 各语言接入

```python
# Python(标准库 ctypes,零依赖)
import ctypes
lib = ctypes.CDLL("./libnosqlite.dylib")
lib.nosqlite_open.restype = ctypes.c_void_p
lib.nosqlite_open.argtypes = [ctypes.c_char_p, ctypes.c_uint8]
db = lib.nosqlite_open(b"app.nodb", 1)
```

```go
// Go(cgo)
/*
#cgo LDFLAGS: -L. -lnosqlite
#include "nosqlite.h"
*/
import "C"
// C.nosqlite_open(cPath, 1)
```

```rust
// Rust:直接用 crate,无需 C ABI(编译期零开销)
let db = nosqlite::Database::open(Path::new("app.nodb"))?;
```

- 完整 Python 示例(纯标准库 `ctypes`,无第三方依赖):[`examples/python/nosqlite.py`](examples/python/nosqlite.py)
- 错误码表见 `include/nosqlite.h` 顶部宏定义;程序化判断请用 `code`,不要匹配消息文本

> ⚠️ 用 ctypes 调用时**必须声明 `argtypes`**,否则句柄的 64 位指针会被当成 `int` 截断而段错误。

## 作为 MongoDB 服务端(nosqlite-server)

```bash
./nosqlite-server app.nodb                      # 默认监听 127.0.0.1:27017
./nosqlite-server --addr 0.0.0.0:27018 app.nodb
```

然后用**任意 MongoDB 官方驱动**直连,无需 nosqlite 专属 SDK:

```go
client, _ := mongo.Connect(options.Client().ApplyURI("mongodb://127.0.0.1:27017"))
coll := client.Database("app").Collection("users")
coll.InsertOne(ctx, bson.M{"name": "张三", "age": 30})
```

已实现命令:`hello`、`ping`、`insert`、`find`、`update`、`delete`、`findAndModify`、`count`、`distinct`、`listCollections`、`listDatabases`、`createIndexes`、`drop`、`dropDatabase`、`buildInfo`、`aggregate`(`$match/$sort/$skip/$limit/$group/$project`)、`endSessions`、`killCursors`。

验收方式:`tests/go_compat/` 用 **Go 官方驱动 `mongo-driver/v2`** 跑 9 项端到端测试(插入/查询/游标/更新/删除/findAndModify/upsert/Date/主键冲突)。

游标:支持 `batchSize` 分批与 `getMore`/`killCursors`,大结果集不会一次性全部返回。

尚未支持:认证、副本集、分片、changeStream。

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

// 计数自增 / 去重追加
let inc = nosqlite::query::Update::from_value(&json!({"inc": {"views": 1}}))?;
db.update_by_id("users", &id, &inc)?;

// upsert:命中则更新,未命中按 filter 等值字段新建
let filter = Filter::from_value(&json!({"email": "a@b.c"}))?;
db.update_or_insert("users", &filter, &update)?;
```

CLI:

```text
$ nosqlite app.nodb
nosqlite> insert users {"name": "张三", "age": 30}
插入成功: 集合 `users`, _id = 6aaa...
nosqlite> find users {"age": {"$gte": 18}} --sort {"age": -1} --limit 10
nosqlite> .stats
nosqlite> .export users backup.jsonl
nosqlite> .import users backup.jsonl
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
- **流式扫描会话**:`Database::scan()` 返回持有读事务的游标(`next_batch(n)` 从上次位置续取,内存 O(一批)),是索引扫描与 wire 协议真流式游标的基础;会话期间数据为一致快照(MVCC)。
- **持久化**:每次写事务提交都会落盘(redb 默认 `Durability::Immediate`)。**不提供**关闭落盘的选项 —— 防止静默丢写入;需要吞吐时用 `insert_many` / 批量 `update` / 批量 `delete` 把 N 次提交合并为 1 次。
- **批量写的原子性**:`insert_many`、`update`(多命中)与 `delete`(多命中)都在单个写事务内完成 —— 要么全部生效,要么整体回滚。代价是收集阶段内存为 O(命中数)。
- **崩溃恢复**:单文件 B+树(redb),进程崩溃后文件保持一致。
- **格式稳定**:`.nodb` v1;本库 0.x 阶段 API 可能调整,但存储格式保持兼容。

## 开发

```bash
cargo test --all-targets     # 517 例测试
cargo clippy --all-targets -- -D warnings
cargo run --release -- app.nodb   # CLI
```

基准:`cargo run --release --example bench_write -- 10000 3`(写路径归因)、`--example bench -- 10000 100000`(读路径与内存)。CI 在 GitHub Actions(ubuntu + macos);基准为手动/tag 触发的工作流。

## License

Apache-2.0(见 [LICENSE](LICENSE))。
