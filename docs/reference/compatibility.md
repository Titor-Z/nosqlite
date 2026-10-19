# 与 MongoDB 的差异

!!! warning "迁移前请读完这页"
    我们**尽力对齐 MongoDB 语义**,但有几处是刻意不同,也有几项确实还没做。

!!! abstract "一句话结论"
    **查询、更新、投影的语义按 MongoDB 实现并逐条测过**;差异集中在
    **值类型系统**、**事务**、**聚合管道** 三块。

---

## :material-check-circle: 已对齐的部分

这些曾经踩过坑,现已与 MongoDB 一致,并有测试锁死:

| 语义 | 说明 |
|---|---|
| 同层条件 AND | `{"status": "ok", "$or": [...]}` 中字段条件与 `$or` 是 **AND** 关系 |
| 数组包含 | `{"tags": "rust"}` 可命中 `{"tags": ["rust", "db"]}` |
| `$in` 展开数组 | `{"tags": {"$in": ["rust"]}}` 命中任何含 `rust` 的数组字段 |
| 点号穿透数组 | `{"items.name": "x"}` 会遍历 `items` 数组元素的 `name` |
| 缺失 ≡ null | `{"deleted_at": null}` **匹配字段缺失**的文档;`{"$ne": null}` 不匹配 |
| `$exists` vs `$eq: null` | 两者**不等价**:字段缺失时 `$exists: false` 与 `$eq: null` 都成立;字段显式为 `null` 时 `$exists: true` 成立 |
| 类型序 | 完整对齐 BSON 官方序:`MinKey < Null < Number < String < Object < Array < Binary < ObjectId < Bool < Date < Timestamp < Regex < MaxKey` |
| `$type` 与缺失字段 | 任何 `$type` **都不匹配**字段缺失的文档 |
| `$all: []` | **不匹配任何文档**(符合 MongoDB) |
| `$size` 与非数组 | **不匹配**(不是「长度相等」) |
| `modifiedCount` | 内容无变化时不写回、不计数 |
| `$pull` 对象 | 按**字段子集匹配**,不要求对象只有这些字段 |

---

## :material-swap-horizontal: 刻意的不同

### 1. 富类型用 Extended JSON 表示,存储是普通 JSON {#extended-json}

nodblite 的文档值就是 JSON,富类型借 MongoDB Extended JSON 表达:

| 写法 | 含义 |
|---|---|
| `{"$date": "2026-09-16T10:00:00Z"}` | 日期,**按时间戳比较与排序**,时区偏移自动归一 |
| `{"$oid": "507f1f77bcf86cd799439011"}` | ≡ 24 位 hex 字符串(与存储层主键一致) |
| `{"$numberLong": "123"}` | ≡ 整数 |
| `{"$numberDouble": "1.5"}` | ≡ 浮点数 |

!!! info "为什么这么做"
    **写入什么就存什么,不做 BSON 二进制转换。**

    :material-check: 好处:文件可读,可用任何 JSON 工具处理(`jq`、编辑器、备份脚本)
    :material-close: 代价:**没有** `Binary` / `Decimal128` 等剩余 BSON 类型

### 2. `$oid` 被当作字符串 {#oid-as-string}

MongoDB 里 ObjectId 有独立类型序位。nodblite 中 `{"$oid": ...}` **等价于等长的 hex 字符串**,按字符串排序。

!!! danger "可能影响你的排序结果"
    MongoDB 里 ObjectId 有独立的类型序位。nodblite 中 `{"$oid": ...}` **等价于等长的 hex 字符串**,按字符串排序。

    若你有依赖 ObjectId 类型排序的查询,结果顺序会与 MongoDB 不同。

### 3. 事务 {#transactions}

| | MongoDB | nodblite |
|---|---|---|
| 单条语句 | 原子 | 原子 |
| 多命中更新/删除 | 原子 | **原子**(单事务) |
| `insert_many` | — | **原子**(单事务) |
| 跨集合显式事务会话 | `startTransaction()` | **不支持** |

!!! warning "跨集合一致性要自己做"
    多语句的跨集合一致性需要在应用层处理(如补偿逻辑或幂等重试)。

### 4. 并发模型 {#concurrency}

单文件由**进程独占**(SQLite 同款约束)。

- 同进程内多线程读写安全(写事务串行化);
- **多进程不能同时写同一个 `.nodb`**;
- 需要多服务共享 → 用 `nodblite-server`,客户端走网络。

---

## :material-close-circle: 尚未支持

### 更新操作符
- 数组过滤器 `$[]` / `$[identifier]`

### 聚合管道

已支持:`$match`、`$sort`、`$skip`、`$limit`、`$project`、`$group`(含 `$sum`/`$count` 累加器)、`$addFields`/`$set`

未支持:`$unwind`、`$lookup`、`$facet`、`$bucket`、`$graphLookup`、`$merge`、`$out` 等

### 索引
- 无 TTL 索引
- 规划器是**纯规则匹配**,没有区分度估算(小集合或低选择率时可能不选索引)

### 服务端能力
- 无认证、无副本集、无分片、无 change stream
- 无 schema validation

### upsert 的限制

`update_or_insert` 的初始值**只能由 filter 的顶层等值字段反推**。filter 含 `$gt` 等操作符时直接报错 —— **不猜**。

---

## 协议层面

wire 协议已实现并被官方驱动验证:

| 驱动 | 验收用例 |
|---|---|
| Go `mongo-driver/v2` | 18 项 |
| Node `mongodb` | 12 项 |

覆盖:握手、CRUD、游标分批(`getMore`/`killCursors`)、数组操作符、投影、索引命令、`explain`、聚合。

已实现命令:`hello`/`ping`/`insert`/`find`/`update`/`delete`/`findAndModify`/`count`/`distinct`/`aggregate`/`explain`/`createIndexes`/`listIndexes`/`dropIndexes`/`listCollections`/`listDatabases`/`drop`/`dropDatabase`/`buildInfo`/`endSessions`/`getMore`/`killCursors`

---

## 升级提示

**v0.1.1 及更早**的固定行为与本页描述不同,若你在旧版上依赖过:

- `$or` 与字段条件是 OR 关系(现为 AND)
- 数组字段的标量等值不命中(现为命中)
- 点号路径不穿透数组(现为穿透)

升级后查询结果会变化。
