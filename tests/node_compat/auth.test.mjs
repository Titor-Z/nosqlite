// nodblite wire 协议的 Node.js 账号认证验收(SCRAM-SHA-256)。
//
// 与 tests/go_compat/auth_test.go 对齐:只依赖官方 `mongodb` 包,
// 连接串由 NOSQLITE_AUTH_URI 传入;未提供时由 Rust 侧决定是否运行本脚本。

import { strict as assert } from 'node:assert';
import test from 'node:test';
import { MongoClient } from 'mongodb';

const uri = process.env.NOSQLITE_AUTH_URI;
if (!uri) {
  console.error('缺少环境变量 NOSQLITE_AUTH_URI');
  process.exit(1);
}

/** 连上去并 ping(握手 + 认证都在这里发生)。 */
async function connect(target) {
  const client = new MongoClient(target, { serverSelectionTimeoutMS: 5000 });
  await client.connect();
  await client.db('admin').command({ ping: 1 });
  return client;
}

test('正确账号密码可以连接并读写', async () => {
  const client = await connect(uri);
  try {
    const coll = client.db('nodeauth').collection('items');
    await coll.deleteMany({});
    await coll.insertOne({ n: 1 });
    assert.equal(await coll.countDocuments({}), 1);
  } finally {
    await client.close();
  }
});

test('错误密码被拒绝', async () => {
  const bad = uri.replace(/:\/\/([^:]+):[^@]*@/, '://$1:wrong-password@');
  await assert.rejects(() => connect(bad));
});

test('不带账号可以探活,但不能读写', async () => {
  const anon = uri.replace(/:\/\/[^@]+@/, '://');
  const client = await connect(anon);
  try {
    const coll = client.db('nodeauth').collection('items');
    await assert.rejects(() => coll.findOne({}));
  } finally {
    await client.close();
  }
});

test('createUser / usersInfo / dropUser 走 wire', async () => {
  const client = await connect(uri);
  try {
    const admin = client.db('admin');
    await admin.command({ dropUser: 'node_temp' }).catch(() => {});
    await admin.command({ createUser: 'node_temp', pwd: 'node_pwd', roles: [] });

    const info = await admin.command({ usersInfo: 'node_temp' });
    assert.equal(info.users.length, 1);
    assert.equal(info.users[0].credentials, undefined, 'usersInfo 不应泄漏 credentials');

    await admin.command({ dropUser: 'node_temp' });
  } finally {
    await client.close();
  }
});
