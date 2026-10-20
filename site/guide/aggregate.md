# 聚合

::: warning 部分实现
聚合管道只覆盖常用统计场景。**重度依赖聚合请选别的方案。**

未支持的 stage 会**明确报错**,不会静默返回错误结果。
:::

## 支持的 stage

| stage | 说明 |
|---|---|
| `$match` | 过滤(与查询同一套语法) |
| `$sort` | 排序 |
| `$skip` | 跳过 |
| `$limit` | 限制 |
| `$project` | 字段裁剪 |
| `$group` | 分组聚合 |
| `$addFields` / `$set` | 增加/覆盖字段 |

## `$group`

```js
const out = await coll.aggregate([
  { $match: { v: { $gte: 1 } } },
  { $group: { _id: '$k', total: { $sum: '$v' } } },
  { $sort: { _id: 1 } },
]).toArray();
```

### `_id` 的写法

| 写法 | 含义 |
|---|---|
| `_id: null` | 全部文档归为一组(常用于计数) |
| `_id: "$field"` | 按该字段分组 |
| `_id: <常量>` | 字面量分组键 |

### 累加器

| 累加器 | 说明 |
|---|---|
| `{$sum: 1}` | 计数 |
| `{$sum: "$field"}` | 对字段求和 |
| `{$count: ...}` | 计数 |

> 其他累加器(`$avg` / `$min` / `$max` / `$push` 等)**尚未实现**,会明确报错,不会静默返回错误结果。

## 常见用法:计数

```js
// 驱动的 countDocuments 底层就是这条管道
await coll.aggregate([
  { $group: { _id: null, n: { $sum: 1 } } }
]);
```

## 未支持的 stage

`$unwind`、`$lookup`、`$facet`、`$bucket`、`$graphLookup`、`$merge`、`$out`。

如果你的场景重度依赖聚合管道,nodblite 目前**不是**合适的选择 —— 见 [与 MongoDB 的差异](../reference/compatibility.md)。
