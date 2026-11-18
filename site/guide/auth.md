# 账号与认证 {#auth}

nodblite 用 **SCRAM-SHA-256**(与 MongoDB 同一套认证机制),因此官方驱动与工具可以直接带账号密码连接。

## 先理解边界:认证只保护网络

| 访问方式 | 是否认证 |
|---|---|
| 嵌入式(Rust API / C ABI / CLI) | **不认证** —— 能打开文件就等于拥有全部数据 |
| `nodblite-server`(wire 协议) | 库内有账号后,**新连接必须认证** |

文件本身的保护靠操作系统权限与全盘加密(FileVault / LUKS / BitLocker),不靠数据库密码。

## 默认:零账号 = 零认证

新建的库没有任何账号,任何能连上端口的客户端都能访问 —— 等同 MongoDB 的 `noauth`。

**建第一个账号的那一刻,认证就开启了**(只影响此后新建的连接,已建立的连接不会被踢)。

```text
$ nodblite app.nodb
nodblite> .user add admin
请输入账号 `admin` 的密码(注:交互输入会回显;脚本请用 NODBLITE_PASSWORD):
nodblite> 账号 `admin` 已创建(此后连接需认证)
```

脚本里用环境变量传密码(命令行参数会被 `ps` 看到):

```bash
NODBLITE_PASSWORD=xxxx nodblite <<'EOF'
.open app.nodb
.user add admin
EOF
```

第一个账号自动获得 `root` 角色,后续账号默认 `readWrite`。

## 连接

```bash
mongosh "mongodb://admin:xxxx@127.0.0.1:27017/?authSource=admin"
```

Go:

```go
client, _ := mongo.Connect(options.Client().ApplyURI(
    "mongodb://admin:xxxx@127.0.0.1:27017/?authSource=admin",
))
```

Node:

```js
const client = new MongoClient('mongodb://admin:xxxx@127.0.0.1:27017/?authSource=admin');
```

`authSource` 必须是 `admin` —— 账号统一存在 `admin` 库里(与 MongoDB 一致)。

## 角色与权限

通过认证后,能做什么由**角色**决定。用 MongoDB 的角色名,单库架构下语义如下:

| 角色 | 读数据 | 写数据 | 索引 / 集合管理 | 账号管理 |
|---|---|---|---|---|
| `read` | ✅ | ❌ | ❌ | ❌ |
| `readWrite` | ✅ | ✅ | ❌ | ❌ |
| `dbAdmin` | ❌ | ❌ | ✅ | ❌ |
| `userAdmin` | ❌ | ❌ | ❌ | ✅ |
| `dbOwner` | ✅ | ✅ | ✅ | ✅ |
| `root` | ✅ | ✅ | ✅ | ✅ |

「读数据」涵盖 `find` / `aggregate` / `count` / `distinct` / `explain` / `getMore` /
`listCollections` / `listIndexes` / `listDatabases`;「写数据」是
`insert` / `update` / `delete` / `findAndModify`;「索引 / 集合管理」是
`createIndexes` / `dropIndexes` / `drop` / `dropDatabase` / `create`。

未授予的操作返回 `13 Unauthorized`:

```js
// 用 read 角色连上来
await db.collection('items').insertOne({ n: 1 });
// MongoServerError: 当前用户没有写入权限:命令 `insert`
```

指定角色(不给则默认:第一个账号 `root`,其余 `readWrite`):

```js
db.getSiblingDB('admin').runCommand({
  createUser: 'reporter',
  pwd: 'xxxx',
  roles: [{ role: 'read', db: 'admin' }],
});
```

`roles` 也接受字符串简写:`roles: ['read']`。未知角色名会报错。

改角色不用重建账号:

```js
db.getSiblingDB('admin').runCommand({
  updateUser: 'reporter',
  roles: [{ role: 'readWrite', db: 'admin' }],
});
```

### 关于 `db` 字段

角色里的 `db` 只作记录,**不按库隔离** —— nodblite 是单库架构(一个 `.nodb` = 一个逻辑库)。
要在业务单元之间硬隔离,用多个 `.nodb` + 多个 `nodblite-server` 实例。

### 明确不做

- **行级权限**(「用户 A 看不到用户 B 的数据」)—— 需要业务语义,数据库无从判断;
  MongoDB 也一样,这永远是应用的职责。
- **字段级权限** —— 同上,且会让投影语义爆炸。

## 账号管理

| 方式 | 命令 | 谁能用 |
|---|---|---|
| 本地 CLI | `.user` / `.user add <name> [role]` / `.user passwd <name>` / `.user drop <name>` | 能打开文件的人 |
| wire 协议 | `createUser` / `updateUser` / `dropUser` / `usersInfo` | 需要 `userAdmin` / `root` |

`usersInfo` 有更细的规则:**查自己永远可以**,查其他账号需要 `userAdmin` / `root`。

**忘记密码的逃生通道是 `.user passwd`** —— 能打开文件就能重置,因为文件级权限本来就是最高权限。

## 账号存在哪

物理上是一个**真实的集合** `admin.system.users`,所以 `listCollections` 里看得见:

```js
db.getSiblingDB('admin').getCollectionNames()
// [ 'system.version', 'system.users' ]
```

它受三条保护:

- **不允许普通写路径修改**(`insert` / `update` / `delete` / `drop` 一律拒绝),
  只能经 `createUser` / `updateUser` / `dropUser`;
- **读的时候剥掉 `credentials`** —— 否则一条 `find` 就能拿到所有用户的 SCRAM 摘要做离线爆破;
- **`distinct` / `aggregate` 一律拒绝** —— 否则 `distinct credentials` 一条命令就能把
  所有密码摘要当普通字段值读走;
- **`$lookup` / `$unionWith` 不得指向它** —— 否则可以把凭据 join 进自己的结果;
- `.collect` / `.export` 不包含系统集合。

`admin.system.version` 在**建库时就写入**,所以 `admin` 库从创建那一刻就存在(和 mongod 一致)。

## 支持范围

| 项 | 状态 |
|---|---|
| `SCRAM-SHA-256` | ✅ |
| `SCRAM-SHA-1` / `MONGODB-CR` | ❌ 已过时,不支持 |
| 免认证命令 | `hello` / `isMaster` / `ping` / `buildInfo` / `saslStart` / `saslContinue` / `logout` / `connectionStatus` |
| 角色(权限隔离) | ✅ `read` / `readWrite` / `dbAdmin` / `userAdmin` / `dbOwner` / `root`(collection 粒度) |
| `local` / `config` 库 | ❌ 不存在(无复制、无分片) |

> **权限粒度**:只到**集合级**(能做/不能做),没有行级过滤。若一个账号能读某个集合,
> 它就能读该集合的**全部文档** —— 需要「按用户过滤行」时,过滤条件必须写在应用里。
> 这与 MongoDB 的行为一致。
