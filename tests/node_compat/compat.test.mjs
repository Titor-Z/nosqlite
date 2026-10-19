// nodblite wire 协议的 Node.js 官方驱动验收。
//
// 与 tests/go_compat 对齐:不自己造客户端,用官方 `mongodb` 包走完整协议栈。
// 连接串由 NOSQLITE_URI 环境变量传入(由 Rust 集成测试传给 node 进程)。

import { strict as assert } from 'node:assert';
import test from 'node:test';
import { MongoClient } from 'mongodb';

const uri = process.env.NOSQLITE_URI;
if (!uri) {
  console.error('缺少环境变量 NOSQLITE_URI');
  process.exit(1);
}

const client = new MongoClient(uri);
let db;

/** 全部用例共用一个连接。 */
test.before(async () => {
  await client.connect();
  db = client.db('nodecompat');
});

test.after(async () => {
  await client.close();
});

/** 每个用例前清空集合。 */
async function freshCollection(name) {
  const coll = db.collection(name);
  await coll.drop().catch(() => {});
  return coll;
}

// ==================== 握手与基础 CRUD ====================

test('连接与 ping', async () => {
  const res = await db.admin().command({ ping: 1 });
  assert.equal(res.ok, 1, 'ping 应返回 ok:1');
});

test('insertMany 与 find', async () => {
  const coll = await freshCollection('crud');

  const docs = Array.from({ length: 10 }, (_, i) => ({ n: i, tag: `t${i}` }));
  const ins = await coll.insertMany(docs);
  assert.equal(ins.insertedCount, 10, '应插入 10 条');

  const found = await coll.find({ n: { $gte: 5 } }).sort({ n: 1 }).toArray();
  assert.equal(found.length, 5, 'n>=5 应有 5 条');
  assert.equal(found[0].n, 5);
  assert.equal(found[4].n, 9);
});

test('显式 _id 与主键冲突', async () => {
  const coll = await freshCollection('ids');

  await coll.insertOne({ _id: 'fixed', v: 1 });
  const got = await coll.findOne({ _id: 'fixed' });
  assert.equal(got.v, 1);

  // 重复主键应报错(驱动抛 MongoServerError)
  await assert.rejects(
    async () => coll.insertOne({ _id: 'fixed', v: 2 }),
    (err) => err.code === 11000 || /duplicate|冲突/i.test(err.message),
    '重复主键应被拒绝',
  );
});

// ==================== 游标分批 ====================

test('find 游标分批(getMore)', async () => {
  const coll = await freshCollection('cursor');

  const docs = Array.from({ length: 250 }, (_, i) => ({ n: i }));
  await coll.insertMany(docs);

  // 小批次强制走 getMore
  const seen = [];
  const cursor = coll.find({}).sort({ n: 1 }).batchSize(20);
  for await (const doc of cursor) {
    seen.push(doc.n);
  }
  assert.equal(seen.length, 250, '应取回全部 250 条');
  assert.equal(seen[0], 0);
  assert.equal(seen[249], 249);
  // 顺序必须严格递增(验证排序 + 分批不串序)
  for (let i = 1; i < seen.length; i++) {
    assert.ok(seen[i] > seen[i - 1], `第 ${i} 条顺序错: ${seen[i - 1]} -> ${seen[i]}`);
  }
});

// ==================== 更新与删除 ====================

test('updateOne / updateMany / $inc', async () => {
  const coll = await freshCollection('update');
  await coll.insertMany([
    { _id: 'a', n: 1 },
    { _id: 'b', n: 1 },
  ]);

  await coll.updateOne({ _id: 'a' }, { $inc: { n: 10 } });
  assert.equal((await coll.findOne({ _id: 'a' })).n, 11);

  const many = await coll.updateMany({}, { $set: { flag: true } });
  assert.equal(many.matchedCount, 2, '应匹配 2 条');

  const all = await coll.find({ flag: true }).toArray();
  assert.equal(all.length, 2);
});

test('deleteOne / deleteMany', async () => {
  const coll = await freshCollection('delete');
  await coll.insertMany([
    { _id: '1', k: 1 },
    { _id: '2', k: 1 },
    { _id: '3', k: 2 },
  ]);

  await coll.deleteOne({ _id: '1' });
  assert.equal(await coll.countDocuments({}), 2);

  const many = await coll.deleteMany({ k: 1 });
  assert.equal(many.deletedCount, 1);
  assert.equal(await coll.countDocuments({}), 1);
});

// ==================== 数组操作符(M29 新增) ====================

test('$pull 三种形态', async () => {
  const coll = await freshCollection('arrayops');
  await coll.insertOne({
    _id: 'a',
    tags: ['x', 'y', 'x', 'z'],
    nums: [1, 5, 10, 15],
    items: [{ score: 8 }, { score: 9 }, { score: 8 }],
  });

  await coll.updateOne({ _id: 'a' }, { $pull: { tags: 'x' } });
  await coll.updateOne({ _id: 'a' }, { $pull: { nums: { $gte: 10 } } });
  await coll.updateOne({ _id: 'a' }, { $pull: { items: { score: 8 } } });

  const got = await coll.findOne({ _id: 'a' });
  assert.deepEqual(got.tags, ['y', 'z'], '$pull 标量');
  assert.deepEqual(got.nums, [1, 5], '$pull 范围');
  assert.equal(got.items.length, 1, '$pull 对象子集');
  assert.equal(got.items[0].score, 9);
});

test('$pop 首尾', async () => {
  const coll = await freshCollection('pop');
  await coll.insertOne({ _id: 'a', arr: [1, 2, 3] });

  await coll.updateOne({ _id: 'a' }, { $pop: { arr: -1 } });
  assert.deepEqual((await coll.findOne({ _id: 'a' })).arr, [2, 3]);

  await coll.updateOne({ _id: 'a' }, { $pop: { arr: 1 } });
  assert.deepEqual((await coll.findOne({ _id: 'a' })).arr, [2]);
});

// ==================== 过滤操作符(M28 新增) ====================

test('$exists / $type / $all / $size', async () => {
  const coll = await freshCollection('filterops');
  await coll.insertMany([
    { _id: 'missing' },
    { _id: 'null', a: null },
    { _id: 'arr2', a: ['p', 'q'] },
    { _id: 'arr1', a: ['p'] },
  ]);

  assert.equal(await coll.countDocuments({ a: { $exists: true } }), 3, '$exists:true');
  assert.equal(await coll.countDocuments({ a: { $exists: false } }), 1, '$exists:false');
  assert.equal(await coll.countDocuments({ a: { $type: 'array' } }), 2, '$type:array');
  assert.equal(await coll.countDocuments({ a: { $all: ['p', 'q'] } }), 1, '$all');
  assert.equal(await coll.countDocuments({ a: { $size: 1 } }), 1, '$size');
});

// ==================== 投影 ====================

test('投影 $slice 与位置运算符', async () => {
  const coll = await freshCollection('proj');
  await coll.insertOne({
    _id: 'a',
    arr: [1, 2, 3, 4, 5],
    items: [{ qty: 5 }, { qty: 15 }],
  });

  const sliced = await coll.findOne({ _id: 'a' }, { projection: { arr: { $slice: 2 } } });
  assert.deepEqual(sliced.arr, [1, 2], '$slice');

  const positional = await coll.findOne(
    { _id: 'a', 'items.qty': { $gt: 10 } },
    { projection: { 'items.$': 1 } },
  );
  assert.deepEqual(positional.items, [{ qty: 15 }], '位置运算符');
});

// ==================== 索引 ====================

test('createIndexes / listIndexes / dropIndexes', async () => {
  const coll = await freshCollection('idx');
  await coll.insertMany(Array.from({ length: 20 }, (_, i) => ({ n: i, k: i % 3 })));

  const name = await coll.createIndex({ n: 1 });
  assert.ok(name, '应返回索引名');

  // 复合索引
  await coll.createIndex({ k: 1, n: -1 }, { name: 'k_n' });

  const indexes = await coll.indexes();
  assert.ok(indexes.length >= 3, `应至少有 _id_/n_1/k_n 三个索引,实际 ${indexes.length}`);

  // explain 应命中索引
  const plan = await db.command({
    explain: { find: 'idx', filter: { k: 1 } },
  });
  assert.ok(plan.queryPlanner, 'explain 应返回 queryPlanner');

  await coll.dropIndex(name);
  const after = await coll.indexes();
  assert.ok(!after.some((i) => i.name === name), '索引应已删除');
});

// ==================== 聚合 ====================

test('aggregate 基础管道', async () => {
  const coll = await freshCollection('agg');
  await coll.insertMany([
    { k: 'a', v: 1 },
    { k: 'a', v: 2 },
    { k: 'b', v: 3 },
  ]);

  const out = await coll
    .aggregate([
      { $match: { v: { $gte: 1 } } },
      { $group: { _id: '$k', total: { $sum: '$v' } } },
      { $sort: { _id: 1 } },
    ])
    .toArray();

  assert.equal(out.length, 2, '应有两组');
  const a = out.find((x) => x._id === 'a');
  assert.equal(a.total, 3, 'a 组总和应为 3');
});
