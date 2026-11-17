# 聚合

::: warning 部分实现
聚合管道覆盖统计与关联的常用场景,但**表达式操作符只实现了子集**。

未支持的 stage / 累加器 / 表达式会**明确报错**(`40324`),不会静默返回错误结果。
:::

## 支持的 stage

| stage | 说明 |
|---|---|
| `$match` | 过滤(与查询同一套语法);**位于首位时会下推到存储层走索引** |
| `$project` | 字段裁剪 + 表达式计算(见下) |
| `$addFields` / `$set` | 增加/覆盖字段(表达式) |
| `$unset` | 删除字段(字段名或数组) |
| `$sort` | 排序 |
| `$skip` / `$limit` | 分页 |
| `$count` | 计数,输出单篇文档;空输入产出**空结果集** |
| `$group` | 分组聚合 |
| `$unwind` | 展开数组 |
| `$lookup` | 关联另一集合(等值形式) |
| `$facet` | 同一输入跑多条子管道 |
| `$bucket` | 按区间分桶 |
| `$replaceRoot` / `$replaceWith` | 用子文档/表达式结果替换整篇文档 |

## `$group`

```js
const out = await coll.aggregate([
  { $match: { v: { $gte: 1 } } },
  { $group: { _id: '$k', total: { $sum: '$v' }, avg: { $avg: '$v' } } },
  { $sort: { _id: 1 } },
]).toArray();
```

### `_id` 的写法

`_id` 是**表达式**,因此可以是字段引用、常量或任意表达式:

| 写法 | 含义 |
|---|---|
| `_id: null` | 全部文档归为一组(常用于计数) |
| `_id: "$field"` | 按该字段分组 |
| `_id: {$gt: ["$price", 15]}` | 按表达式结果分组 |

### 累加器

| 累加器 | 说明 |
|---|---|
| `{$sum: 1}` / `{$sum: "$field"}` | 计数 / 求和(非数值被忽略) |
| `{$avg: "$field"}` | 平均值;组内无数值时为 `null` |
| `{$min: "$field"}` / `{$max: "$field"}` | 按 BSON 类型序取极值 |
| `{$first: "$field"}` / `{$last: "$field"}` | 按输入顺序取首个 / 末个 |
| `{$push: "$field"}` | 收集为数组 |
| `{$addToSet: "$field"}` | 收集为数组并去重 |
| `{$count: {}}` | 计数 |

> `$first` / `$last` 依赖**输入顺序**。需要确定性时先 `$sort`。

## `$unwind`

```js
// 简写
{ $unwind: '$items' }

// 完整形式
{ $unwind: { path: '$items', preserveNullAndEmptyArrays: true, includeArrayIndex: 'idx' } }
```

默认丢弃「非数组 / 空数组 / 缺失」的文档。

## `$lookup`

```js
{ $lookup: { from: 'authors', localField: 'author', foreignField: '_id', as: 'authorDoc' } }
```

- `localField` 是数组时会逐个元素匹配(即 multikey 关联);
- 未匹配时输出**空数组**,不是字段缺失;
- 实现是「先收集本地值 → 一次 `$in` 查询 → 分组挂载」,不是逐条 N+1。

::: warning 子管道形式未实现
`{ $lookup: { from, pipeline, let } }` 会报 `40324`。需要 `let` 变量请先在应用层拼条件。
:::

## `$facet`

```js
[
  { $facet: {
      byItem: [{ $group: { _id: '$item', total: { $sum: '$qty' } } }],
      count: [{ $count: 'n' }],
  } },
]
```

输出**单篇文档**,每个子管道的结果放在同名字段里。

## `$bucket`

```js
{ $bucket: {
    groupBy: '$age',
    boundaries: [0, 18, 65],
    default: 'other',          // 越界值的归处;不提供则越界报错
    output: { n: { $sum: 1 } }, // 省略时默认 {count: {$sum: 1}}
} }
```

## 表达式

`$project` / `$addFields` / `$expr` / `$group._id` / 累加器参数共用同一套表达式求值器。

| 类别 | 操作符 |
|---|---|
| 算术 | `$add` `$subtract` `$multiply` `$divide` `$mod` |
| 比较 | `$eq` `$ne` `$gt` `$gte` `$lt` `$lte` `$cmp` |
| 逻辑 | `$and` `$or` `$not` |
| 条件 | `$cond` `$ifNull` |
| 集合 | `$in` `$size` `$arrayElemAt` `$isArray` `$concatArrays` |
| 字符串 | `$concat` `$toUpper` `$toLower` |
| 其它 | `$literal` |

字段引用写 `"$a.b"`;`"$$ROOT"` 表示整篇文档。

```js
// 常用:$project 里算新字段
{ $project: { _id: 0, total: { $multiply: ['$price', '$qty'] } } }
```

## 未支持的 stage

`$out`、`$merge`、`$graphLookup`、`$bucketAuto`、`$sample`、`$setWindowFields`、`$search`。

`$group` 的其余累加器(`$stdDevPop`、`$mergeObjects`、`$top`/`$bottom` 等)与其余表达式操作符同样未实现。

如果你的场景重度依赖这些,nodblite 目前**不是**合适的选择 —— 见 [与 MongoDB 的差异](../reference/compatibility.md)。
