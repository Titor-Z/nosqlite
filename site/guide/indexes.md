# 索引

::: danger 先看代价再建索引
每多一个索引约 **+80% 写入成本**(3 个索引 ≈ 2.4 倍)。

**只对查询实际用到的字段建索引。**
:::

## 建索引

```rust
// 单字段
db.create_index("users", "age_1", "age", false, false)?;

// 唯一
db.create_index("users", "email_1", "email", true, false)?;

// 稀疏(缺失/null 不入索引)
db.create_index("users", "phone_1", "phone", true, true)?;

// 复合(遵循 MongoDB 前缀原则)
db.create_index_composite("users", "status_age", &["status", "age"], false, false)?;

// 降序
db.create_index_with_directions("users", "age_-1", &["age"], &[-1], false, false)?;
```

## 前缀原则

索引 `{a, b}` 可以服务:

- `{a: 1}` ✅
- `{a: 1, b: 2}` ✅
- `{b: 2}` ❌(不能)

**字段顺序很重要**。查询常用作过滤的字段应放在前面。

## 何时走索引

| 条件形态 | 走索引 |
|---|---|
| 等值 | ✅ |
| 范围(`$gt` / `$lt` 等) | ✅ |
| 排序 | ✅(方向需匹配) |
| `$or` 各分支都能走索引 | ✅(并集) |
| 多个字段各有独立索引 | ✅(交集) |
| 仅后缀字段有索引 | ❌ |

范围条件只能出现在**最后一个等值字段之后**。`{a: 1, b: {$gte: 2}}` 可以;`{a: {$gte: 1}, b: 2}` 只能用到 `a`。

## 验证

```rust
let plan = db.explain("users", &json!({"age": {"$gte": 18}}), None)?;
println!("stage      = {}", plan["stage"]);       // IXSCAN / COLLSCAN
println!("indexName  = {}", plan["indexName"]);
println!("sortedByIndex = {}", plan["sortedByIndex"]);
```

wire 层同样可用:

```js
const plan = await db.command({
  explain: { find: 'users', filter: { age: { $gte: 18 } } }
});
```

## 实测效果(10 万文档,macOS) {#benchmark}

| 查询场景 | 全表扫 | 走索引 | 加速比 |
|---|---|---|---|
| 唯一字段等值(命中 1 条) | ~510 ms | **~1 ms** | **300–4000x** |
| 范围(命中 1%) | ~425 ms | **~8 ms** | **~55x** |
| 复合(前缀等值 + 后缀范围) | ~410 ms | **~22 ms** | **~19x** |
| 低选择率等值(命中 10%) | ~430 ms | ~76 ms | ~5.7x |

::: warning 注意最后一行
命中率高时索引收益有限,因为**回表成本**占主导。规划器目前是纯规则匹配,没有区分度估算。
:::

## 代价

| 项 | 数值 |
|---|---|
| 建索引回填 10 万条 | 1.2–2.5 s |
| 每多一个索引 | **+80% 写入成本**(3 个索引 ≈ 2.4x) |

索引在**同一个写事务**内维护,保证数据与索引始终一致 —— 代价就是写入变慢。

**建议**:只对**查询实际用到**的字段建索引。随便加索引会让写入明显变慢。

## multikey(数组字段)

字段是数组时,每个元素各生成一条索引条目。

复合索引下多个数组字段会做**笛卡尔积展开**,上限 128 组;超过则整体不入索引(避免爆炸)。

## 管理

```rust
// 列出
let indexes = db.list_indexes("users")?;

// 删除
db.drop_index("users", "age_1")?;
```

删除集合时,该集合的索引定义与条目会**一并清理**。
