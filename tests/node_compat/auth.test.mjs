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

// ---------- B 阶段:角色授权 ----------

/** 用 root 建指定角色的账号,返回带账号密码的连接串。 */
async function createUserWithRole(uri, user, pwd, role) {
  const root = await connect(uri);
  try {
    const admin = root.db('admin');
    await admin.command({ dropUser: user }).catch(() => {});
    await admin.command({ createUser: user, pwd, roles: [{ role, db: 'admin' }] });
  } finally {
    await root.close();
  }
  return uri.replace(/:\/\/([^:]+):[^@]*@/, `://${user}:${pwd}@`);
}

test('只读账号:能读,不能写/建索引', async () => {
  const viewer = await createUserWithRole(uri, 'node_viewer', 'v_pwd', 'read');
  const client = await connect(viewer);
  try {
    const coll = client.db('nodeauth').collection('items');
    await assert.rejects(() => coll.insertOne({ n: 1 }));
    await assert.rejects(() => coll.updateOne({}, { $set: { n: 2 } }));
    await assert.rejects(() => coll.deleteOne({}));
    await assert.rejects(() => client.db('nodeauth').command({
      createIndexes: 'items', indexes: [{ key: { n: 1 }, name: 'n_1' }],
    }));
    assert.equal(await coll.countDocuments({}) >= 0, true, '只读账号应能 count');
  } finally {
    await client.close();
  }
});

test('readWrite 不能建账号;userAdmin 可以', async () => {
  const rw = await createUserWithRole(uri, 'node_rw', 'rw_pwd', 'readWrite');
  const c1 = await connect(rw);
  try {
    await assert.rejects(() => c1.db('admin').command({
      createUser: 'nope', pwd: 'x', roles: [],
    }));
  } finally {
    await c1.close();
  }

  const ua = await createUserWithRole(uri, 'node_ua', 'ua_pwd', 'userAdmin');
  const c2 = await connect(ua);
  try {
    await c2.db('admin').command({ createUser: 'node_temp2', pwd: 't', roles: [] });
    const info = await c2.db('admin').command({ usersInfo: 'node_viewer' });
    assert.equal(info.users.length, 1);
    assert.equal(info.users[0].credentials, undefined, 'usersInfo 不应泄漏 credentials');
    await c2.db('admin').command({ dropUser: 'node_temp2' });
  } finally {
    await c2.close();
  }
});

test('$lookup 不得 join 出 admin.system.users 的凭据', async () => {
  const root = await connect(uri);
  try {
    const src = root.db('nodeauth').collection('src');
    await src.drop().catch(() => {});
    await src.insertOne({ x: 'a' });
  } finally {
    await root.close();
  }
  const viewer = await createUserWithRole(uri, 'node_viewer2', 'v2_pwd', 'read');
  const c = await connect(viewer);
  try {
    await assert.rejects(() => c.db('nodeauth').command({
      aggregate: 'src',
      pipeline: [{ $lookup: { from: 'admin.system.users', localField: 'x', foreignField: '_id', as: 'u' } }],
      cursor: {},
    }));
  } finally {
    await c.close();
  }
});

test('distinct / aggregate 不得在 admin.system.users 上读凭据;find 已剥掉', async () => {
  const viewer = await createUserWithRole(uri, 'node_viewer3', 'v3_pwd', 'read');
  const c = await connect(viewer);
  try {
    await assert.rejects(() => c.db('admin').command({ distinct: 'system.users', key: 'credentials' }));
    await assert.rejects(() => c.db('admin').command({ aggregate: 'system.users', pipeline: [], cursor: {} }));
    const docs = await c.db('admin').collection('system.users').find({}).toArray();
    assert.ok(docs.length > 0, 'system.users 应有账号');
    for (const d of docs) {
      assert.equal(d.credentials, undefined, 'find 返回不应含 credentials');
    }
  } finally {
    await c.close();
  }
});
