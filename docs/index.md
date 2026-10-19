# nodblite

**像 SQLite 一样小巧的单文件 NoSQL 数据库。**

MongoDB 风格的文档模型,整个数据库就是一个 `.nodb` 文件 —— 可直接复制、备份、迁移,无需服务进程、无需配置。

```bash
cargo add nodblite
```

```rust
use nodblite::{Database, Document};
use nodblite::query::Filter;

let db = Database::create("app.nodb")?;

let mut doc = Document::from_json(r#"{"name": "张三", "age": 30}"#)?;
db.insert("users", &mut doc)?;

let adults = db.find("users", &Filter::from_value(
    &serde_json::json!({"age": {"$gte": 18}})
)?)?;
```

---

!!! abstract "定位"
    **部署形态学 SQLite,数据模型学 MongoDB。**

    单文件、进程内、零配置、无后台服务 —— 但查询、更新、索引的语义按 MongoDB 实现,并逐条测试过。

## 它是什么

nodblite 把 MongoDB「顺手」的文档语义装进一个像 SQLite 一样可直接拷贝的单文件里:

- **部署形态学 SQLite** —— 单文件、进程内、零配置、无后台服务;
- **数据模型学 MongoDB** —— BSON 类型序、文档查询、更新操作符、二级索引、游标协议;
- **两种使用形态** —— Rust/C 直接内嵌,或启动 `nodblite-server` 让现有 MongoDB 驱动直连。

## 何时该用

!!! success "适合"
    - **桌面 / 单机应用** —— 一个文件带走全部数据,不需要用户装数据库
    - **嵌入式后端** —— Rust/C 服务内嵌,省掉一个数据库运维对象
    - **原型与工具链** —— 想用 MongoDB 的查询手感,但不想跑 mongod
    - **需要单文件迁移** —— 复制 `.nodb` 即可完成备份/迁移(已在测试中固化)

??? danger "不适合(诚实地说)"
    下面这些场景,请直接选别的方案:

    | 场景 | 原因 |
    |---|---|
    | **多进程并发写同一个库** | 单文件由进程独占,这是 SQLite 同款约束。需要多服务共享请走 `nodblite-server` |
    | **TB 级数据 / 高写入吞吐集群** | 定位是嵌入式单机库,不是分布式系统 |
    | **需要副本集、分片、认证** | wire 协议已实现,但这些能力不在范围内 |
    | **重度依赖聚合管道** | 仅支持 7 个 stage,见 [聚合](guide/aggregate.md) |
    | **跨集合事务** | 单语句原子 + 单事务批量,但没有显式事务会话 |

## 两种形态

### 一、内嵌(Rust / C)

```rust
// Rust:crate `nodblite`
let db = Database::open("app.nodb")?;
```

```c
// C:include/nodblite.h,链接 libnodblite
NodbliteDb *db = nodblite_open("app.nodb", NODBLITE_OPEN_CREATE);
```

零依赖、零配置,库直接跑在你的进程里。

### 二、服务端(wire 协议)

```bash
nodblite-server --addr 127.0.0.1:27017 app.nodb
```

之后用**任何 MongoDB 官方驱动**直连:

```js
import { MongoClient } from 'mongodb';
const client = new MongoClient('mongodb://127.0.0.1:27017');
await client.connect();
const users = client.db('app').collection('users');
await users.insertOne({ name: '张三', age: 30 });
```

!!! success "协议兼容经过真实驱动验证"
    **Go 官方驱动 18 项、Node 官方驱动 12 项**端到端测试全部通过。

    不是「自己造客户端测自己」—— 是拉起 `nodblite-server`,再用官方驱动连接,走完整协议栈。

## 下一步

<div class="grid cards" markdown>

- **[快速开始:Rust](quickstart/rust.md)** — 内嵌 API,5 分钟跑通
- **[快速开始:C ABI](quickstart/c-api.md)** — 在 C/C++ 项目里使用
- **[快速开始:命令行](quickstart/cli.md)** — sqlite3 风格的 REPL
- **[连接 MongoDB 驱动](quickstart/wire.md)** — 启动 server,现有驱动直连

</div>

!!! warning "迁移前必读"
    我们尽力对齐 MongoDB 语义,但有几处**刻意不同**,也有一批**确实还没做**。

    请看 → **[与 MongoDB 的差异](reference/compatibility.md)**
