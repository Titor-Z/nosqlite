# nodblite

**像 SQLite 一样小巧的单文件 NoSQL 文档数据库** —— MongoDB 风格查询,零配置,无需服务进程。

```bash
npm install nodblite
```

```js
import { Database } from 'nodblite';

const db = new Database('app.nodb');

const id = db.insert('users', { name: '张三', age: 30 });
const adults = db.find('users', { age: { $gte: 18 } }, { sort: { age: -1 } });

db.close();
```

整个数据库就是一个 `.nodb` 文件 —— 复制即迁移,备份即拷贝。

📖 **[完整文档](https://titor-z.github.io/nosqlite/)** · [与 MongoDB 的差异](https://titor-z.github.io/nosqlite/reference/compatibility/)

---

## 为什么用它

| | |
|---|---|
| **零配置** | 没有服务进程,没有端口,没有配置文件 |
| **单文件** | 一个 `.nodb` 文件,`cp` 就是备份 |
| **MongoDB 语义** | 查询/更新/索引语法与 MongoDB 一致,迁移成本低 |
| **同步 API** | 没有 `await` 地狱,手感像 `better-sqlite3` |
| **TypeScript** | 完整类型定义,`find<T>()` 泛型推断 |

## 安装

```bash
npm install nodblite
```

需要 Node.js 18+。底层通过 [koffi](https://www.npmjs.com/package/koffi) 调用预编译动态库,**不需要 node-gyp、不需要 Python、不需要 C++ 编译器**。

支持平台:macOS(arm64/x64)、Linux(x64)、Windows(x64)。

## 快速上手

```js
import { Database } from 'nodblite';

const db = new Database('app.nodb');   // 不存在则自动创建

// 插入
const id = db.insert('users', { name: '张三', age: 30 });
db.insertMany('users', [
  { name: '李四', age: 25 },
  { name: '王五', age: 42 },
]);

// 查询
const adults = db.find('users', { age: { $gte: 18 } }, { sort: { age: -1 } });
const one = db.get('users', id);

// 分页:一次拿到总数与当页,不用查两次
const { total, items } = db.findPage('users', {}, { skip: 0, limit: 20 });

// 更新
db.update('users', { name: '张三' }, { $inc: { age: 1 } });
db.upsert('users', { email: 'a@b.c' }, { $set: { name: '新人' } });

// 删除
db.delete('users', { age: { $lt: 18 } });

db.close();
```

## 索引

```js
// 单字段
db.createIndex('users', 'age_1', 'age');

// 唯一
db.createIndex('users', 'email_1', 'email', { unique: true });

// 复合(遵循 MongoDB 前缀原则)
db.createIndex('users', 'status_age', 'status', { fields: ['status', 'age'] });

// 降序
db.createIndex('users', 'age_-1', 'age', { fields: ['age'], directions: [-1] });

// 看看查询会不会走索引
const plan = db.explain('users', { age: { $gte: 18 } });
console.log(plan.stage);      // 'IXSCAN' 或 'COLLSCAN'
console.log(plan.indexName);  // 'age_1'
```

## TypeScript

```ts
import { Database } from 'nodblite';

interface User {
  _id?: string;
  name: string;
  age: number;
  tags?: string[];
}

const db = new Database('app.nodb');

const adults: User[] = db.find<User>('users', { age: { $gte: 18 } });
const page = db.findPage<User>('users', { tags: { $all: ['admin'] } });
```

类型会拦截常见错误:

```ts
db.find<User>('users', { age: { $gte: '字符串' } });  // ❌ 类型错误
db.update<User>('users', {}, { $inc: { age: 'x' } }); // ❌ 类型错误
```

## 支持的查询操作符

`$eq` `$ne` `$gt` `$gte` `$lt` `$lte` `$in` `$nin` `$regex`
`$exists` `$type` `$all` `$size` `$elemMatch`
`$and` `$or` `$nor` `$not`

## 支持的更新操作符

`$set` `$unset` `$inc` `$mul` `$min` `$max` `$rename`
`$push` `$addToSet` `$pull` `$pop` `$currentDate`

## 资源管理

```js
// 手动关闭
const db = new Database('app.nodb');
db.close();

// 或用 using 自动关闭(TypeScript 5.2+ / Node 22+)
using db = new Database('app.nodb');
```

## 错误处理

所有错误都是 `NodbliteError` 的子类,带稳定的 `code`:

```js
import { Errors } from 'nodblite';

try {
  db.insert('users', { email: 'a@b.c' });
} catch (e) {
  if (e instanceof Errors.DuplicateKeyError) {
    console.log('邮箱已存在');
  }
}
```

| 错误类 | code | 场景 |
|---|---|---|
| `DuplicateKeyError` | 1 | 主键或唯一索引冲突 |
| `InvalidPathError` | 2 | 后缀不是 `.nodb` |
| `InvalidFilterError` | 6 | 过滤条件语法错误 |
| `InvalidUpdateError` | 7 | 更新操作符错误 |
| `InvalidDocumentError` | 10 | 文档不是合法 JSON 对象 |

## 常见问题

**能多进程同时写吗?**
不能。`.nodb` 由进程独占(SQLite 同款约束)。需要多服务共享,请用 `nodblite-server` + MongoDB 驱动。

**和 MongoDB driver 有什么区别?**
这个包是**进程内直连**,没有网络开销,没有服务进程 —— 适合桌面应用、CLI、单机服务。需要跨进程/跨语言共享时用 wire 协议。

**Deno / Bun 能用吗?**
这个包依赖 koffi(Node 原生模块),目前只在 Node.js 上验证过。Deno/Bun 推荐用 wire 协议。

**性能如何?**
查询走索引时是高选择率快 300–4000 倍(10 万文档实测)。批量写入用 `insertMany`,比逐条快两个数量级。详见[性能文档](https://titor-z.github.io/nosqlite/reference/performance/)。

## License

Apache-2.0
