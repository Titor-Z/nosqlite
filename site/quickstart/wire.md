# 连接 MongoDB 驱动

启动 `nodblite-server`,用**任何 MongoDB 官方驱动**直连。协议层已被 Go 与 Node 官方驱动端到端验证。

## 启动服务端

```bash
nodblite-server --addr 127.0.0.1:27017 app.nodb
```

## Node.js

```bash
npm install mongodb
```

```js
import { MongoClient } from 'mongodb';

const client = new MongoClient('mongodb://127.0.0.1:27017');
await client.connect();

const users = client.db('app').collection('users');

await users.insertMany([
  { name: '张三', age: 30 },
  { name: '李四', age: 25 },
]);

const adults = await users
  .find({ age: { $gte: 18 } })
  .sort({ age: -1 })
  .limit(10)
  .toArray();

console.log(adults);

await client.close();
```

## Go

```go
import (
    "go.mongodb.org/mongo-driver/v2/mongo"
    "go.mongodb.org/mongo-driver/v2/mongo/options"
)

client, _ := mongo.Connect(options.Client().ApplyURI("mongodb://127.0.0.1:27017"))
coll := client.Database("app").Collection("users")

coll.InsertOne(ctx, bson.M{"name": "张三", "age": 30})

cursor, _ := coll.Find(ctx, bson.M{"age": bson.M{"$gte": 18}})
```

## Python

```python
from pymongo import MongoClient

client = MongoClient("mongodb://127.0.0.1:27017")
users = client.app.users

users.insert_many([
    {"name": "张三", "age": 30},
    {"name": "李四", "age": 25},
])

for doc in users.find({"age": {"$gte": 18}}):
    print(doc)
```

## 已实现的命令 {#commands}

```
hello  ping  insert  find  getMore  killCursors
update  delete  findAndModify  count  distinct  aggregate  explain
createIndexes  listIndexes  dropIndexes
listCollections  listDatabases  drop  dropDatabase
buildInfo  endSessions
```

## 游标

支持 `batchSize` 分批与 `getMore` / `killCursors`。大结果集**不会**一次性全部返回:

```js
const cursor = users.find({}).batchSize(100);
for await (const doc of cursor) {
  // 每批 100 条
}
```

## 未支持

认证、副本集、分片、change stream。服务端是单实例单进程模型。
