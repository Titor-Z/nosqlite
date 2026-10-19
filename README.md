# nodblite

**像 SQLite 一样小巧的单文件 NoSQL 数据库** —— MongoDB 风格的文档模型,单文件(`.nodb`)零配置部署。

📖 **[完整文档](https://titor-z.github.io/nosqlite/)** · [快速开始](https://titor-z.github.io/nosqlite/quickstart/rust/) · [与 MongoDB 的差异](https://titor-z.github.io/nosqlite/reference/compatibility/)

```bash
cargo add nodblite
```

## 它是什么

nodblite 把 MongoDB「顺手」的文档语义装进一个像 SQLite 一样可直接拷贝的单文件里。

**部署形态学 SQLite** —— 单文件、进程内、零配置、无后台服务。
**数据模型学 MongoDB** —— BSON 类型序、文档查询、更新操作符、二级索引、游标协议。

### 能力一览

| 能力 | 说明 |
|---|---|
| **存储** | 单文件 `.nodb`(redb B+树),格式版本 v1,复制即迁移 |
| **文档模型** | JSON 文档 + 自动 `_id`,集合隔离,无需建表 |
| **查询** | `$eq/$ne/$gt/$gte/$lt/$lte/$in/$nin/$regex/$elemMatch/$exists/$type/$all/$size` + `$and/$or/$nor/$not` |
| **更新** | `$set/$unset/$inc/$mul/$min/$max/$rename/$push/$addToSet/$currentDate/$pull/$pop` + upsert |
| **投影** | 字段取舍 + `$slice` / `$elemMatch` / 位置运算符 `{"arr.$": 1}` |
| **索引** | 单字段 / 复合 / 降序 / 唯一 / 稀疏 / multikey,查询自动选索引 + `explain` |
| **聚合** | `$match/$sort/$skip/$limit/$project/$group/$addFields` |
| **类型** | BSON 官方类型序;Date 用 `{"$date": ...}`,按时间戳比较排序 |
| **协议** | MongoDB wire 协议,Go 18 项 + Node 12 项官方驱动验收 |
| **CLI** | sqlite3 风格 REPL + `.export`/`.import`/`.stats` |

### 两个形态

| 形态 | 说明 | 状态 |
|---|---|---|
| **嵌入式(C ABI)** | 像 SQLite 一样被任何语言**进程内**调用,无需额外服务进程 | ✅ `include/nodblite.h` |
| **服务端(wire 协议)** | `nodblite-server` 监听 `mongodb://`,第三方 MongoDB driver 直连 | ✅ Go + Node 驱动验收通过 |

两者共用同一套查询语义。底层是 [redb](https://crates.io/crates/redb)(纯 Rust 单文件 B+树 KV 存储),之上提供文档语义层。

## 何时该用 / 不该用

**适合**:桌面 / 单机应用、嵌入式后端、原型与工具链、需要单文件迁移的场景。

**不适合**:

| 场景 | 原因 |
|---|---|
| 多进程并发写同一个库 | 单文件由进程独占(SQLite 同款约束);需要多服务共享请走 `nodblite-server` |
| TB 级数据 / 集群 | 定位是嵌入式单机库,不是分布式系统 |
| 需要副本集、分片、认证、change stream | wire 协议已实现,但这些能力不在范围内 |
| 重度依赖聚合管道 | 仅支持 7 个 stage |
| 跨集合显式事务 | 单语句原子 + 单事务批量,无事务会话 |

细节见 [完整文档](https://titor-z.github.io/nosqlite/)。

## 嵌入到任意语言(C ABI)

构建后得到 `libnodblite.dylib` / `.so` / `.dll`,配合 `include/nodblite.h` 即可被任何语言**进程内**调用 —— 没有服务进程,没有网络。

```c
#include "nodblite.h"

NodbliteDb *db = nodblite_open("app.nodb", 1);
NodbliteResult *r = nodblite_insert(db, "users", "{\"name\":\"张三\"}");
char id[64];
snprintf(id, sizeof(id), "%.*s", (int)r->value.len, r->value.ptr);
nodblite_result_free(r);            // 整块释放,只需一次

/* 游标:大结果集逐条取,内存 O(批大小) */
NodbliteCursor *cur = nodblite_find_cursor(db, "users", NULL, NULL, NULL, 0, 0, 1);
const char *doc;
while ((doc = nodblite_cursor_next(cur)) != NULL) {
    puts(doc);
}
nodblite_cursor_close(cur);

nodblite_close(db);
```

设计上**骨架照搬 SQLite**(不透明句柄 + `int` 错误码 + 游标),但修掉它因 2004 年 ABI 冻结而做不到的三件事:

| 现代化点 | SQLite | nodblite |
|---|---|---|
| 结果释放 | 每个返回值单独 `sqlite3_free` | **arena,一次 `nodblite_result_free`** |
| 错误信息 | 一个笼统错误码 | **结构化**:`{code, code_str, message, op}` |
| 接口演进 | 只能堆 `_v2`/`_v3` | **`struct_size` 版本化 options** |

### 获取库

两种方式,任选:

**① 预编译包**(推荐):从 [GitHub Releases](https://github.com/Titor-Z/nosqlite/releases) 下载对应平台的 `nodblite-ffi-<platform>.tar.gz`,内含动态库、静态库与 `include/nodblite.h`,并附 `.sha256` 校验。

| 平台 | 动态库 | 静态库 |
|---|---|---|
| Linux x86_64 | `libnodblite.so` | `libnodblite.a` |
| macOS aarch64 | `libnodblite.dylib` | `libnodblite.a` |
| Windows x86_64 | `nodblite.dll` | `nodblite.lib` |

**② 本地构建**:`cargo build --release`,产物在 `target/release/`。

### 各语言接入

```python
# Python(标准库 ctypes,零依赖)
import ctypes
lib = ctypes.CDLL("./libnodblite.dylib")
lib.nodblite_open.restype = ctypes.c_void_p
lib.nodblite_open.argtypes = [ctypes.c_char_p, ctypes.c_uint8]
db = lib.nodblite_open(b"app.nodb", 1)
```

```go
// Go(cgo)
/*
#cgo LDFLAGS: -L. -lnodblite
#include "nodblite.h"
*/
import "C"
// C.nodblite_open(cPath, 1)
```

```rust
// Rust:直接用 crate,无需 C ABI(编译期零开销)
let db = nodblite::Database::open(Path::new("app.nodb"))?;
```

- 完整 Python 示例(纯标准库 `ctypes`,无第三方依赖):[`examples/python/nodblite.py`](examples/python/nodblite.py)
- 错误码表见 `include/nodblite.h` 顶部宏定义;程序化判断请用 `code`,不要匹配消息文本

> ⚠️ 用 ctypes 调用时**必须声明 `argtypes`**,否则句柄的 64 位指针会被当成 `int` 截断而段错误。

## 作为 MongoDB 服务端(nodblite-server)

```bash
./nodblite-server app.nodb                      # 默认监听 127.0.0.1:27017
./nodblite-server --addr 0.0.0.0:27018 app.nodb
```

然后用**任意 MongoDB 官方驱动**直连,无需 nodblite 专属 SDK:

```go
client, _ := mongo.Connect(options.Client().ApplyURI("mongodb://127.0.0.1:27017"))
coll := client.Database("app").Collection("users")
coll.InsertOne(ctx, bson.M{"name": "张三", "age": 30})
```

已实现命令:`hello`、`ping`、`insert`、`find`、`update`、`delete`、`findAndModify`、`count`、`distinct`、`listCollections`、`listDatabases`、`createIndexes`、`drop`、`dropDatabase`、`buildInfo`、`aggregate`(`$match/$sort/$skip/$limit/$group/$project`)、`endSessions`、`killCursors`。

```js
// Node.js(官方 mongodb 驱动)
import { MongoClient } from 'mongodb';
const client = new MongoClient('mongodb://127.0.0.1:27017');
await client.connect();
const users = client.db('app').collection('users');
await users.insertOne({ name: '张三', age: 30 });
const list = await users.find({ age: { $gte: 18 } }).sort({ age: -1 }).toArray();
```

验收方式:两套官方驱动端到端跑通 ——
`tests/go_compat/`(**Go 官方驱动 `mongo-driver/v2`**,18 项)与
`tests/node_compat/`(**Node 官方驱动 `mongodb`**,12 项)。
覆盖插入/查询/游标分批/更新/删除/findAndModify/upsert/Date/主键冲突/数组操作符/过滤操作符/投影/索引/聚合。

游标:支持 `batchSize` 分批与 `getMore`/`killCursors`,大结果集不会一次性全部返回。

尚未支持:认证、副本集、分片、changeStream。

## 快速上手

```rust
use nodblite::{query::Filter, Document, Database};
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
let update = nodblite::query::Update::from_value(&json!({"$set": {"vip": true}}))?;
db.update_by_id("users", &id, &update)?;

// 计数自增 / 去重追加
let inc = nodblite::query::Update::from_value(&json!({"inc": {"views": 1}}))?;
db.update_by_id("users", &id, &inc)?;

// upsert:命中则更新,未命中按 filter 等值字段新建
let filter = Filter::from_value(&json!({"email": "a@b.c"}))?;
db.update_or_insert("users", &filter, &update)?;
```

CLI:

```text
$ nodblite app.nodb
nodblite> insert users {"name": "张三", "age": 30}
插入成功: 集合 `users`, _id = 6aaa...
nodblite> find users {"age": {"$gte": 18}} --sort {"age": -1} --limit 10
nodblite> .stats
nodblite> .export users backup.jsonl
nodblite> .import users backup.jsonl
```

## 性能参考

> ⚠️ 写入数字**强依赖平台的落盘(fsync)语义**,引用时必须标注平台。

| 场景 | macOS (Apple Silicon) | Linux (CI runner) |
|---|---|---|
| 逐条插入 | ~185 条/秒 | ~1,400 条/秒 |
| `insert_many` 批量 | ~44,000 条/秒 | ~300,000 条/秒 |
| `find` 全量扫描 | ~8 µs/条 | ~1.2 µs/条 |
| 唯一字段等值查询走索引 | **~510 ms → ~1 ms(500x)** | — |
| 范围查询走索引(命中 1%) | **~425 ms → ~8 ms(55x)** | — |
| 索引对写入的影响 | 每多一个索引 **+80%** | — |

差异根源:macOS 的 `sync_all` 映射到 `F_FULLFSYNC`,比 POSIX `fsync` 慢 **66 倍**;Linux 无此放大。**要吞吐就用 `insert_many`**。

完整基准数据、测试方法与归因分析见 **[性能文档](https://titor-z.github.io/nosqlite/reference/performance/)**。

## 与 MongoDB 的差异

查询、更新、投影的语义按 MongoDB 实现并逐条测过。差异集中在三块:

| 方面 | 差异 |
|---|---|
| **类型系统** | 文档值即 JSON,富类型用 Extended JSON(`{"$date":...}` / `{"$oid":...}`);`$oid` **按字符串处理**,不占独立类型序位;无 `Binary`/`Decimal128` |
| **事务** | 单语句原子;`insert_many` / 多命中 `update`/`delete` 也是单事务原子;**无跨集合显式事务会话** |
| **聚合** | 仅 7 个 stage,无 `$unwind`/`$lookup`/`$facet` |
| **并发** | 单文件由进程独占;**多线程并发写无加速比**(单写者模型) |

已对齐的部分(这些曾是偏差,现已修正并测过):同层条件 AND、数组包含、`$in` 展开数组、点号穿透数组、缺失 ≡ null、`$exists` 与 `$eq: null` 的区别、BSON 官方类型序。

**完整清单 → [与 MongoDB 的差异](https://titor-z.github.io/nosqlite/reference/compatibility/)**

## 内存与持久化语义

- **内存**:除旧接口 `scan`(全量物化,标注 deprecated)外,`find/count/update/delete` 均流式处理;带排序的查询用 `find_page`(两遍法:先收集「排序键 + _id」排序,再按主键回取页内文档),内存 O(n × 排序键大小),与命中文档大小无关。
- **流式扫描会话**:`Database::scan()` 返回持有读事务的游标(`next_batch(n)` 从上次位置续取,内存 O(一批)),是索引扫描与 wire 协议真流式游标的基础;会话期间数据为一致快照(MVCC)。
- **持久化**:每次写事务提交都会落盘(redb 默认 `Durability::Immediate`)。**不提供**关闭落盘的选项 —— 防止静默丢写入;需要吞吐时用 `insert_many` / 批量 `update` / 批量 `delete` 把 N 次提交合并为 1 次。
- **批量写的原子性**:`insert_many`、`update`(多命中)与 `delete`(多命中)都在单个写事务内完成 —— 要么全部生效,要么整体回滚。代价是收集阶段内存为 O(命中数)。
- **崩溃恢复**:单文件 B+树(redb),进程崩溃后文件保持一致。
- **格式稳定**:`.nodb` v1;本库 0.x 阶段 API 可能调整,但存储格式保持兼容。

## 开发

```bash
cargo test --all-targets     # 526 例测试
cargo clippy --all-targets -- -D warnings
cargo run --release -- app.nodb   # CLI
```

基准:`cargo run --release --example bench_write -- 10000 3`(写路径归因)、`--example bench -- 10000 100000`(读路径与内存)。CI 在 GitHub Actions(ubuntu + macos);基准为手动/tag 触发的工作流。

## License

Apache-2.0(见 [LICENSE](LICENSE))。
