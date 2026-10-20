# 查询

::: tip 本节速查
比较 / 元素 / 数组 / 逻辑 / 日期 —— 五类操作符,外加点号路径与排序分页。
:::

## 基本形式

```rust
let filter = Filter::from_value(&json!({"age": {"$gte": 18}}))?;
let docs = db.find("users", &filter)?;
```

filter 顶层是 JSON 对象;字段值是普通值则视为隐式 `$eq`;是含 `$` 键的对象则按操作符解释。

## 比较操作符

| 操作符 | 示例 | 说明 |
|---|---|---|
| `$eq` | `{"a": 1}` | 隐式等于;可命中数组包含 |
| `$ne` | `{"a": {"$ne": 1}}` | 不等于(`$eq` 取反) |
| `$gt` / `$gte` | `{"a": {"$gt": 10}}` | 大于 / 大于等于 |
| `$lt` / `$lte` | `{"a": {"$lt": 10}}` | 小于 / 小于等于 |
| `$in` | `{"a": {"$in": [1, 2]}}` | 命中任一;会展开数组字段 |
| `$nin` | `{"a": {"$nin": [1, 2]}}` | 均不命中 |
| `$regex` | `{"name": {"$regex": "^张"}}` | 正则匹配字符串 |

## 元素与类型

| 操作符 | 示例 | 说明 |
|---|---|---|
| `$exists` | `{"a": {"$exists": true}}` | 字段是否存在 |
| `$type` | `{"a": {"$type": "string"}}` | 类型判断,接受类型数组 |
| `$all` | `{"tags": {"$all": ["a", "b"]}}` | 数组须**同时包含**全部 |
| `$size` | `{"tags": {"$size": 2}}` | 数组长度精确匹配 |

::: danger `$exists` 与 `$eq: null` 不等价
这是最常见的误用。
:::

### `$exists` 的坑

**`$exists` 与 `$eq: null` 不等价:**

| 文档 | `{"a": {"$exists": false}}` | `{"a": null}` |
|---|---|---|
| `{"b": 1}`(a 缺失) | ✅ 命中 | ✅ 命中 |
| `{"a": null}` | ❌ 不命中 | ✅ 命中 |

判断「字段为 null」要用 `{"a": {"$type": "null"}}`,不是 `$eq: null`。

### `$type` 支持的类型

`double` / `string` / `object` / `array` / `objectId` / `bool` / `date` / `null` / `int` / `long` / `minKey` / `maxKey` / `timestamp` / `regex`

- `"number"` 是别名,接受 int / long / double;
- 可传数组:`{"a": {"$type": ["string", "null"]}}`;
- **任何 `$type` 都不匹配字段缺失的文档**。

## 数组

### 隐式数组包含

```json
{"tags": "rust"}
```
命中 `{"tags": ["rust", "db"]}` —— 标量等值会展开数组元素比较。

### `$elemMatch`

要求**同一个元素**同时满足多个条件:

```json
{"items": {"$elemMatch": {"name": "x", "qty": {"$gt": 5}}}}
```

支持两种形态:

```json
// 操作符形态(标量数组)
{"scores": {"$elemMatch": {"$gte": 80, "$lt": 90}}}

// 文档形态(对象数组)
{"items": {"$elemMatch": {"name": "x", "qty": {"$gt": 5}}}}
```

## 点号路径

```json
{"items.name": "x"}
```

会**遍历 `items` 数组元素的 `name`** —— 任一元素匹配即命中。

## 逻辑组合

| 操作符 | 说明 |
|---|---|
| `$and` | 全部满足 |
| `$or` | 任一满足 |
| `$nor` | 全部不满足 |
| `$not` | 字段级或顶层取反 |

```json
{"$or": [{"status": "ok"}, {"retries": {"$lt": 3}}]}
```

> **注意**:同层字段条件与 `$or` 是 **AND** 关系。
> `{"a": 1, "$or": [...]}` 等价于 `a == 1 AND (...)`。

## 日期

```json
{"created_at": {"$gte": {"$date": "2026-09-16T00:00:00Z"}}}
```

按**时间戳**比较与排序,时区偏移自动归一。

## 排序与分页

```rust
let opts = FindOptions {
    sort: Some(SortSpec::from_value(&json!({"age": -1, "name": 1}))?),
    skip: 20,
    limit: Some(10),
    ..Default::default()
};
```

- 多字段按声明顺序依次比较;
- 支持点号路径;
- 类型序遵循 BSON 官方序。

## 走索引

查询会自动选索引。想知道走了没有:

```rust
let plan = db.explain("users", &json!({"age": {"$gte": 18}}), None)?;
```

- `IXSCAN` —— 走了索引
- `COLLSCAN` —— 全表扫

详见 [索引](indexes.md)。
