# 与 MongoDB 的差异

::: warning 迁移前请读完这页
我们**尽力对齐 MongoDB 语义**,但有几处是刻意不同,也有几项确实还没做。
:::

::: info 一句话结论
**查询、更新、投影的语义按 MongoDB 实现并逐条测过**;差异集中在
**值类型系统**、**事务**、**聚合管道** 三块。
:::

---

##  已对齐的部分

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

##  刻意的不同

### 1. 富类型用 Extended JSON 表示,存储是普通 JSON {#extended-json}

nodblite 的文档值就是 JSON,富类型借 MongoDB Extended JSON 表达:

| 写法 | 含义 |
|---|---|
| `{"$date": "2026-09-16T10:00:00Z"}` | 日期,**按时间戳比较与排序**,时区偏移自动归一 |
| `{"$oid": "507f1f77bcf86cd799439011"}` | ≡ 24 位 hex 字符串(与存储层主键一致) |
| `{"$numberLong": "123"}` | ≡ 整数 |
| `{"$numberDouble": "1.5"}` | ≡ 浮点数 |

::: info 为什么这么做
**写入什么就存什么,不做 BSON 二进制转换。**

好处:文件可读,可用任何 JSON 工具处理(`jq`、编辑器、备份脚本)
代价:没有 BSON 的原生类型身份,只能靠字面量约定(见下节「类型系统差异」)
:::

### 2. `$oid` 被当作字符串 {#oid-as-string}

MongoDB 里 ObjectId 有独立类型序位。nodblite 中 `{"$oid": ...}` **等价于等长的 hex 字符串**,按字符串排序。

::: danger 可能影响你的排序结果
MongoDB 里 ObjectId 有独立的类型序位。nodblite 中 `{"$oid": ...}` **等价于等长的 hex 字符串**,按字符串排序。

若你有依赖 ObjectId 类型排序的查询,结果顺序会与 MongoDB 不同。
:::

### 3. 事务 {#transactions}

| | MongoDB | nodblite |
|---|---|---|
| 单条语句 | 原子 | 原子 |
| 多命中更新/删除 | 原子 | **原子**(单事务) |
| `insert_many` | — | **原子**(单事务) |
| 跨集合显式事务会话 | `startTransaction()` | **不支持** |

::: warning 跨集合一致性要自己做
多语句的跨集合一致性需要在应用层处理(如补偿逻辑或幂等重试)。
:::

### 4. 并发模型 {#concurrency}

单文件由**进程独占**(SQLite 同款约束)。

- 同进程内多线程读写安全(写事务串行化);
- **多进程不能同时写同一个 `.nodb`**;
- 需要多服务共享 → 用 `nodblite-server`,客户端走网络。

---

##  类型系统差异 {#type-system}

::: info 一句话
**语义层对齐**(怎么比、怎么排序),**类型层不对齐**(有哪些类型、怎么区分)。
:::

类型序逐级对齐 BSON 官方序,所以**排序、范围查询、索引结果与 MongoDB 一致**。
但内部是 JSON,只有六种原生值,富类型靠 Extended JSON 字面量模拟 ——
凡是依赖「类型本身」做区分的场景,结果会不同。

| 差异 | 我们的行为 | MongoDB | 影响 |
|---|---|---|---|
| 整数不分子类型 | `$type: "int"` 与 `"long"` 都实现为「是 `i64` 范围内的整数」 | int32 与 int64 是不同类型 | 存成 int64 的值,`$type: "int"` 在 MongoDB 不匹配,在这里匹配 |
| 数值比较走 `f64` | `9007199254740993` 与 `…992` **相等** | int64 精确比较 | 超出 2^53 的大整数做等值/范围判断会出错 |
| `$numberDecimal` 按对象比较 | 只保留原始字节,**不参与数值比较**,按 `Object` 等级排序 | Decimal128 属 `Number` 等级 | `{"$numberDecimal": "1"} > 999999` 在这里成立 |
| 24 位 hex 字符串双身份 | 同时被 `$type: "string"` 与 `"objectId"` 命中 | 两者互斥 | 靠 `$type` 做 schema 校验会多命中 |
| `$type: "regex"` 恒为 false | 存储层不保留正则字面量 | 有 regex 类型 | 只能判断「不匹配」 |
| 无 `undefined` / `symbol` / `dbPointer` / `code_w_scope` | 不支持 | 支持 | wire 协议收到这几个类型无法无损往返 |

::: danger 迁移前的判断标准
- **排序、范围查询、索引**:可以放心按 MongoDB 的预期写。
- **`$type` 精确判断、依赖 int32/int64 区别、Decimal128 运算**:不要指望一致,
  在应用层用显式字段(如 `{"n_type": "long"}`)自行区分。
:::

### 大整数:超过 2^53 一律存字符串 {#big-int}

数值比较走 `f64`,意味着 **超过 2^53(9007199254740992)的整数会丢精度**,
而且丢在**所有比较**上 —— 不只是 `$lt`/`$gt`,**等值也会误判**:

```js
// 雪花 ID / 19 位 ID 这类值,不要用数字
await db.collection('orders').insertOne({ snowflake: '1234567890123456789' })

// 查询也用字符串,两端一致
await db.collection('orders').find({ snowflake: '1234567890123456789' })
```

| 存法 | 驱动看到 | 比较精度 |
|---|---|---|
| 字符串 | BSON `string` | 精确(字典序,定长 ID 下与数值序一致) |
| 数字 | BSON `int64`/`int32` | **2^53 以上丢失** |
| `{"$numberLong": "..."}` | BSON `int64` | 同上(字面量只是编码约束,不提升比较精度) |

::: tip 什么时候可以放心用数字
毫秒时间戳(约 1.7 × 10^12)、自增主键、以「分」为单位的金额 —— 都在 2^53 以内,不受影响。
只在 ID 生成器会产生 19 位以上数值时才需要改存字符串。
:::

---

##  尚未支持

### 查询

已支持:`$eq/$ne/$gt/$gte/$lt/$lte/$in/$nin/$regex`(含 `$options`)、`$elemMatch`、`$not`、
`$exists`、`$type`、`$all`、`$size`、`$mod`、`$expr`、`$and/$or/$nor`

未支持:`$text`(需文本索引)、`$where`(需 JS 引擎)、地理查询(`$geoWithin` 等)、
`$jsonSchema`、`$bitsAllSet` 系列

### 更新操作符

未支持:`$setOnInsert` 已补,**仍缺** 数组过滤器嵌套标识符以外的选项(`collation`)、
`$push` 的 `$sort` 与 `$slice` 组合之外的修饰符

### 聚合管道

已支持:`$match`、`$project`、`$addFields`/`$set`、`$unset`、`$sort`、`$skip`、`$limit`、
`$count`、`$group`、`$unwind`、`$lookup`、`$facet`、`$bucket`、`$replaceRoot`/`$replaceWith`

`$group` 累加器:`$sum`、`$avg`、`$min`、`$max`、`$first`、`$last`、`$push`、`$addToSet`、`$count`

未支持:`$out`、`$merge`、`$graphLookup`、`$bucketAuto`、`$sample`、`$setWindowFields`、
`$lookup` 的子管道形式(`pipeline` + `let` )、其余累加器与表达式操作符

### 索引
- 无 TTL 索引
- 规划器是**纯规则匹配**,没有区分度估算(小集合或低选择率时可能不选索引)

### 服务端能力
已支持:SCRAM-SHA-256 认证、6 个内置角色(`read`/`readWrite`/`dbAdmin`/`userAdmin`/
`dbOwner`/`root`,集合级授权)、`createUser`/`updateUser`/`dropUser`/`usersInfo`。

未支持:SCRAM-SHA-1 / MONGODB-CR、行级与字段级权限、副本集、分片、change stream、
`local` / `config` 库、schema validation、TLS(请置于受信网络或反向代理之后)

::: tip 认证只保护网络
嵌入式(Rust / C ABI / CLI)打开文件时**不做认证** —— 能读文件字节就等于拥有全部数据。
文件本身的保护靠 OS 权限与全盘加密。详见 [账号与认证](../guide/auth.md)。
:::

### upsert 的限制

`update_or_insert` 的初始值**只能由 filter 的顶层等值字段反推**。filter 含 `$gt` 等操作符时直接报错 —— **不猜**。

---

## 协议层面

wire 协议已实现并被官方驱动验证:

| 驱动 | 验收用例 |
|---|---|
| Go `mongo-driver/v2` | 18 项 |
| Node `mongodb` | 12 项 |
| Node `mongoose`(ODM) | 9 项 |

覆盖:握手、CRUD、游标分批(`getMore`/`killCursors`)、数组操作符、投影、索引命令、`explain`、聚合。
Mongoose 额外验证了 ODM 的隐性调用:建模建索引、`timestamps` 的 Date 往返、
`ref` + `populate` 的 ObjectId 翻译、`countDocuments` 背后的 `aggregate/$group`。

::: info Prisma 为什么不在列
Prisma 的可插入面只有 **SQL 方言**与 **wire 协议**两处,`provider` 是固定枚举、无自定义 provider 机制 ——
嵌入式模式接不进去。若要使用,走 `provider = "mongodb"` 指向 `nodblite-server`,与上述驱动同一条路。
文档模型的 ORM 首选是 Mongoose。
:::

已实现命令:`hello`/`ping`/`insert`/`find`/`update`/`delete`/`findAndModify`/`count`/`distinct`/`aggregate`/`explain`/`createIndexes`/`listIndexes`/`dropIndexes`/`listCollections`/`listDatabases`/`drop`/`dropDatabase`/`buildInfo`/`endSessions`/`getMore`/`killCursors`,
以及认证与账号管理:`saslStart`/`saslContinue`/`logout`/`connectionStatus`/`createUser`/`updateUser`/`dropUser`/`usersInfo`

---

## 升级提示

**v0.1.1 及更早**的固定行为与本页描述不同,若你在旧版上依赖过:

- `$or` 与字段条件是 OR 关系(现为 AND)
- 数组字段的标量等值不命中(现为命中)
- 点号路径不穿透数组(现为穿透)

升级后查询结果会变化。
