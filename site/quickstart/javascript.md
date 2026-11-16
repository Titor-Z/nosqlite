# 快速开始:JavaScript / TypeScript

Node.js 里直接用官方 npm 包 —— 同步 API、完整类型定义、预编译二进制,不需要 node-gyp、不需要 C 编译器。

```bash
npm install nodblite
```

```js
import { Database } from 'nodblite';

const db = new Database('app.nodb');       // 不存在则自动创建

const id = db.insert('users', { name: '张三', age: 30 });
const adults = db.find('users', { age: { $gte: 18 } }, { sort: { age: -1 } });

console.log(id, adults);
db.close();
```

整个数据库就是一个 `.nodb` 文件 —— 复制即迁移,备份即拷贝。

---

## 什么时候不该用这个包

| 你的环境 | 该用什么 |
|---|---|
| Node.js(Electron 主进程、CLI、服务端) | **本页的 npm 包** |
| 浏览器 / 边缘运行时 / 不想装动态库 | [wasm](/quickstart/wasm) |
| 多进程共享同一个库、多语言、已有 MongoDB 驱动 | [wire 协议](/quickstart/wire) |

> npm 包与 wasm 读写的是**同一种 `.nodb` 文件**,可以在 Node 里写入、在浏览器里打开,反之亦然。

---

## 基本操作

### 打开与关闭

```js
import { Database } from 'nodblite';

const db = new Database('app.nodb');
// 只读打开(文件必须已存在):new Database('app.nodb', { create: false })

db.close();                       // 可重复调用
// 或让 TS 5.2+ 自动关闭:
{
  using db2 = new Database('other.nodb');
}
```

路径后缀必须是 `.nodb`,否则抛 `InvalidPathError`。

### 写入

```js
const id = db.insert('users', { name: '李四', age: 22, tags: ['a'] });

// 批量插入:单事务,任一主键冲突则整批回滚
const ids = db.insertMany('users', [
  { name: '王五', age: 41 },
  { name: '赵六', age: 19 },
]);

// 更新,返回实际修改条数(内容无变化的文档不计入)
const modified = db.update('users', { name: '李四' }, { $set: { age: 23 }, $push: { tags: 'b' } });

// upsert:命中则更新,否则用 filter 的等值字段建新文档
const r = db.upsert('users', { name: '孙七' }, { $set: { age: 27 } });
// r = { updated: 0 } 或 { inserted: 'xxxxxxxxxxxxxxxxxxxxxxxx' }

// 删除,返回删除条数
db.delete('users', { age: { $lt: 20 } });
```

更新操作符与 MongoDB 一致:`$set` `$unset` `$inc` `$mul` `$min` `$max` `$rename` `$push` `$addToSet` `$pull` `$pop` `$currentDate`,详见[更新](/guide/update)。

### 查询

```js
const list = db.find('users', { age: { $gte: 18 } }, {
  sort: { age: -1 },      // 1 升序 / -1 降序
  fields: { name: 1 },    // 投影
  skip: 0,
  limit: 20,
});

const one = db.get('users', id);      // 按 _id 读单条,不存在返回 null
const n = db.count('users', { age: { $gte: 18 } });
const names = db.collections();
```

列表页用 `findPage`,**一次扫描**同时给出「匹配总数 + 当页」:

```js
const { total, items } = db.findPage('users', { age: { $gte: 18 } }, {
  sort: { age: -1 },
  skip: 40,
  limit: 20,
});
```

`find` 的 `filter` 支持全部查询操作符与逻辑组合,见[查询](/guide/query)与[投影](/guide/projection)。

### 索引

```js
db.createIndex('users', 'age_1', 'age');            // 单字段

// 复合 + 唯一:字段顺序即前缀顺序,方向 1 升序 / -1 降序
db.createIndex('users', 'tenant_status', 'tenant', {
  fields: ['tenant', 'status'],
  directions: [1, -1],
  unique: true,
});

db.listIndexes('users');
db.dropIndex('users', 'age_1');

// 看查询走了什么计划
db.explain('users', { age: { $gte: 18 } }, { age: -1 });
// { stage: 'IXSCAN', indexName: 'age_1', ... }
```

单字段索引传字符串,复合索引传字段数组;`{ unique, sparse }` 可选。索引语义与选型见[索引](/guide/indexes)。

---

## 错误处理

所有错误都是 `NodbliteError` 的子类,带稳定的 `code`(可用它做程序判断,不要匹配文案):

```js
import { DuplicateKeyError, InvalidFilterError } from 'nodblite';

try {
  db.insert('users', { _id: 'fixed', name: '张三' });
} catch (e) {
  if (e instanceof DuplicateKeyError) {
    // 主键冲突
  }
  console.error(e.code, e.message);   // 例:DUPLICATE_KEY 主键冲突:…
}
```

完整错误码表见[与 MongoDB 的差异](/reference/compatibility)。

---

## 平台与依赖

- **Node.js 18+**,纯 ESM(`import`);CommonJS 项目请用动态 `import()`。
- 动态库已预编译随包分发:`prebuilds/` 覆盖 macOS(arm64 / x64)、Linux(x64)、Windows(x64)。
- 底层通过 [koffi](https://www.npmjs.com/package/koffi) 调用 C ABI —— 这是实现细节,你不需要关心;需要绕过封装直接调 C ABI 时看 [C ABI](/quickstart/c-api)。
- Electron:主进程可用;打包时把 `node_modules/nodblite/prebuilds` 作为 `extraResources` 一起带上。

::: warning 单进程独占
`.nodb` 文件由打开它的进程独占(SQLite 同款约束)。需要多进程同时访问,请用 [wire 协议](/quickstart/wire) 起一个服务进程。
:::

---

## 常见问题

::: details Electron 里能用吗?
能,主进程就是 Node 环境。注意打包时把 `prebuilds/` 目录作为 `extraResources` 带进去 —— 否则安装包在用户机器上找不到动态库。
:::

::: details 能多进程同时写吗?
不能。单文件由进程独占。要共享请起 `nodblite-server`,用 [wire 协议](/quickstart/wire) 访问。
:::

::: details 浏览器里能用吗?
能,走 [wasm](/quickstart/wasm);npm 包本身只在 Node 里工作。
:::
