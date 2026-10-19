# 更新

!!! info "执行顺序固定"
    为避免歧义,同一更新文档里的操作按固定顺序执行:

    `$rename` → `$inc` → `$mul` → `$min` → `$max` → `$set` → `$unset` → `$push` → `$addToSet` → `$pull` → `$pop` → `$currentDate`

## 基本形式

```rust
let upd = Update::from_value(&json!({"$set": {"name": "李四"}}))?;
let modified = db.update("users", &filter, &upd)?;
```

## 字段操作

| 操作符 | 示例 | 说明 |
|---|---|---|
| `$set` | `{"$set": {"a.b": 1}}` | 赋值,支持点号路径,嵌套自动创建 |
| `$unset` | `{"$unset": {"a": ""}}` | 删除字段;数组元素置 `null` |
| `$inc` | `{"$inc": {"views": 1}}` | 数值增减 |
| `$mul` | `{"$mul": {"price": 1.1}}` | 数值乘法 |
| `$min` / `$max` | `{"$min": {"low": 5}}` | 取更小 / 更大 |
| `$rename` | `{"$rename": {"a": "b"}}` | 字段改名 |
| `$currentDate` | `{"$currentDate": {"updated_at": true}}` | 写入当前时间 |

## 数组操作

| 操作符 | 示例 | 说明 |
|---|---|---|
| `$push` | `{"$push": {"tags": "x"}}` | 追加元素 |
| `$addToSet` | `{"$addToSet": {"tags": "x"}}` | 追加(已存在则跳过) |
| `$pull` | `{"$pull": {"tags": "x"}}` | 移除**所有**匹配元素 |
| `$pop` | `{"$pop": {"tags": 1}}` | `1` 移除末尾,`-1` 移除开头 |

### `$push` 的修饰符

```json
{
  "$push": {
    "tags": {
      "$each": ["a", "b"],
      "$position": 0,
      "$slice": 10,
      "$sort": 1
    }
  }
}
```

### `$pull` 的三种形态 {#pull-forms}

```json
// 1. 标量等值
{"$pull": {"tags": "x"}}

// 2. 查询操作符
{"$pull": {"scores": {"$gte": 60}}}

// 3. 对象子集匹配
{"$pull": {"items": {"score": 8}}}
```

第三种是**子集匹配**:移除所有 `score == 8` 的对象,**不要求对象只有这一个字段**。

## `_id` 不可修改

任何更新操作尝试改 `_id` 都会报错。主键不变是存储层的前提。

## 空更新的行为

- 空对象、空 `$set`、空路径段(如 `a..b`)一律报错,不静默通过;
- 更新后内容**无变化**的文档不写回、不计入 `modifiedCount`。

## 批量语义

`update` 匹配多条时:

- **合并为一次提交**(不是逐条提交);
- **整体原子** —— 要么全生效,要么全回滚;
- 代价:收集阶段内存为 O(命中数)。

## 按主键更新

已知 `_id` 时走快速路径:

```rust
db.update_by_id("users", &id, &upd)?;
```

绕过全表扫描,直接 B+树定位。50 万文档下从秒级降到毫秒级。

## upsert

```rust
let result = db.update_or_insert("users", &filter, &upd)?;
match result {
    UpsertResult::Updated(n) => println!("更新了 {} 条", n),
    UpsertResult::Inserted(id) => println!("新建 {}", id),
}
```

> **限制**:新建文档的初始值只能由 filter 的**顶层等值字段**反推。filter 含 `$gt` 等操作符时会报错 —— 不猜。
