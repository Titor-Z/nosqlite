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

## 账号管理

| 方式 | 命令 | 谁能用 |
|---|---|---|
| 本地 CLI | `.user` / `.user add <name>` / `.user passwd <name>` / `.user drop <name>` | 能打开文件的人 |
| wire 协议 | `createUser` / `updateUser` / `dropUser` / `usersInfo` | 已认证的连接 |

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
- `.collect` / `.export` 不包含系统集合。

`admin.system.version` 在**建库时就写入**,所以 `admin` 库从创建那一刻就存在(和 mongod 一致)。

## 支持范围

| 项 | 状态 |
|---|---|
| `SCRAM-SHA-256` | ✅ |
| `SCRAM-SHA-1` / `MONGODB-CR` | ❌ 已过时,不支持 |
| 免认证命令 | `hello` / `isMaster` / `ping` / `buildInfo` / `saslStart` / `saslContinue` / `logout` / `connectionStatus` |
| 角色(权限隔离) | ⚠️ 账号里已有 `roles` 字段,但**暂不检查** —— 通过认证即拥有全部权限 |
| `local` / `config` 库 | ❌ 不存在(无复制、无分片) |

> **关于权限**:nodblite 目前只做**认证**,不做**授权**。`readWrite` 账号实际能做一切。
> 需要按业务单元隔离时,用多个 `.nodb` + 多个 `nodblite-server` 实例。
> 行级权限(「用户 A 看不到用户 B 的数据」)永远是应用的职责 —— MongoDB 也一样。
