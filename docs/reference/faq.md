# 常见问题

## 为什么文件后缀必须是 `.nodb`?

这是**刻意**的:文件后缀是「这是个 nodblite 数据库」的唯一外部标识,复制、备份、迁移时不容易和普通 JSON/SQLite 文件混淆。

其他后缀或无后缀会直接被拒绝。

## 我的进程崩溃了,数据会丢吗?

**已提交的事务不会丢。**

- 每次写事务提交都落盘;
- 崩溃后文件保持一致(单调事务原子性);
- 未提交的写入不会留下半写状态。

这部分有专门的崩溃恢复测试:子进程写入后立即 `abort()`,父进程重开校验「要么全部写入、要么一条没有」。

## 为什么逐条插入这么慢?

因为每条都是一次独立事务,每次提交都要 fsync。**用 `insert_many`** —— 单事务批量,快两个数量级。

详见 [性能](performance.md#why-slow-write)。

## `{"a": null}` 为什么连字段缺失的文档也匹配?

这是 MongoDB 的语义:字段缺失被当作 `null` 参与相等比较。

要精确判断:
- 字段**为 null** → `{"a": {"$type": "null"}}`
- 字段**存在**(含 null) → `{"a": {"$exists": true}}`
- 字段**缺失** → `{"a": {"$exists": false}}`

## `$elemMatch` 和数组包含有什么区别?

```json
// 隐式包含:数组里有任意一个 "rust"
{"tags": "rust"}

// $elemMatch:同一个元素同时满足多个条件
{"items": {"$elemMatch": {"name": "x", "qty": {"$gt": 5}}}}
```

多条件约束同一个元素时**必须**用 `$elemMatch`,否则条件可以分散在不同元素上。

## 索引为什么没被用上?

检查三点:

1. 查询字段是否是索引的**前缀**(`{a,b}` 索引不能服务 `{b}` 查询);
2. 范围条件是否在最后一个等值字段**之后**;
3. 排序方向是否与索引一致(降序索引配降序排序)。

用 `explain` 确认:

```rust
let plan = db.explain("users", &json!({...}), None)?;
println!("{}", plan["stage"]);
```

## 多个进程能同时打开同一个 `.nodb` 吗?

**不能同时写。** 单文件由进程独占 —— 这是 SQLite 同款约束。

需要多个服务共享数据 → 启动 `nodblite-server`,客户端走网络。

## 支持事务吗?

| 场景 | 支持 |
|---|---|
| 单条语句 | ✅ 原子 |
| 多命中 update / delete | ✅ 原子(单事务) |
| `insert_many` | ✅ 原子(单事务) |
| 跨集合显式事务 | ❌ |

跨集合一致性需要应用层处理。

## 能用 MongoDB 驱动直连吗?

可以。启动 `nodblite-server` 后,任何 MongoDB 官方驱动都能连:

```bash
nodblite-server --addr 127.0.0.1:27017 app.nodb
```

Go 与 Node 官方驱动的端到端验收已通过。详见 [连接 MongoDB 驱动](../quickstart/wire.md)。

## 数据能迁移到 MongoDB 吗?

能。都支持 Extended JSON,用 `.export` 导出 JSONL,再用 `mongoimport` 导入即可。

注意 [与 MongoDB 的差异](../reference/compatibility.md) 里提到的类型系统差异(ObjectId 按字符串处理等)。

## 项目叫什么?

crate 名 **`nodblite`**,GitHub 仓库名 `nosqlite`,文件后缀 `.nodb`。

三者不一致是历史原因(crate 原名 `nosqlite` 在 crates.io 上已被占用),但 `.nodb` 后缀与存储格式始终未变。
