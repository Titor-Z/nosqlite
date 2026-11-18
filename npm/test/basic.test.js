// nodblite npm 包的行为测试。
//
// 运行前:在仓库根目录执行 `cargo build --release --lib`,
// 或设置 NODBLITE_LIB 指向已有的动态库。

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, statSync, unlinkSync } from 'node:fs';

const statSize = (p) => statSync(p).size;
import { Database, Errors } from '../src/index.js';

// 每次运行用独立路径:否则并行跑(或上一次残留)会互相踩
const DB = `/tmp/nodblite_npm_test_${process.pid}_${Date.now()}.nodb`;

function fresh() {
  if (existsSync(DB)) unlinkSync(DB);
  return new Database(DB);
}

test('打开与关闭', () => {
  const db = fresh();
  assert.equal(db.path, DB);
  db.close();
  // 重复关闭应安全
  db.close();
  if (existsSync(DB)) unlinkSync(DB);
});

test('后缀必须是 .nodb', () => {
  assert.throws(() => new Database('/tmp/x.db'), /后缀必须是 \.nodb/);
});

test('插入与查询', () => {
  const db = fresh();
  const id = db.insert('users', { name: '张三', age: 30 });
  assert.equal(typeof id, 'string');
  assert.equal(id.length, 24);

  db.insertMany('users', [
    { name: '李四', age: 25 },
    { name: '王五', age: 42 },
  ]);

  assert.equal(db.count('users'), 3);

  const adults = db.find('users', { age: { $gte: 30 } }, { sort: { age: -1 } });
  assert.equal(adults.length, 2, 'age>=30 命中 30 与 42');
  assert.equal(adults[0].name, '王五');
  assert.equal(adults[1].name, '张三');

  db.close();
  unlinkSync(DB);
});

test('按 _id 读取', () => {
  const db = fresh();
  const id = db.insert('u', { n: 1 });

  const doc = db.get('u', id);
  assert.equal(doc.n, 1);

  assert.equal(db.get('u', '不存在的id'), null);

  db.close();
  unlinkSync(DB);
});

test('findPage 同时返回总数与当页', () => {
  const db = fresh();
  db.insertMany('u', Array.from({ length: 25 }, (_, i) => ({ n: i })));

  const page = db.findPage('u', {}, { sort: { n: 1 }, skip: 5, limit: 10 });
  assert.equal(page.total, 25, '总数不受 skip/limit 影响');
  assert.equal(page.items.length, 10);
  assert.equal(page.items[0].n, 5);
  assert.equal(page.items[9].n, 14);

  db.close();
  unlinkSync(DB);
});

test('更新与删除', () => {
  const db = fresh();
  db.insert('u', { _id: 'a', n: 1 });
  db.insert('u', { _id: 'b', n: 2 });

  assert.equal(db.update('u', { _id: 'a' }, { $inc: { n: 10 } }), 1);
  assert.equal(db.get('u', 'a').n, 11);

  assert.equal(db.delete('u', { _id: 'b' }), 1);
  assert.equal(db.count('u'), 1);

  db.close();
  unlinkSync(DB);
});

test('upsert', () => {
  const db = fresh();
  const r1 = db.upsert('u', { _id: 'x' }, { $set: { n: 1 } });
  assert.ok('inserted' in r1, '首次应插入');

  const r2 = db.upsert('u', { _id: 'x' }, { $set: { n: 2 } });
  assert.ok('updated' in r2, '再次应更新');
  assert.equal(db.get('u', 'x').n, 2);

  db.close();
  unlinkSync(DB);
});

test('索引:创建、列表、explain、删除', () => {
  const db = fresh();
  db.insertMany('u', Array.from({ length: 100 }, (_, i) => ({ n: i, k: i % 5 })));

  db.createIndex('u', 'n_1', 'n');
  db.createIndex('u', 'k_n', 'k', { fields: ['k', 'n'], directions: [1, -1] });

  const idx = db.listIndexes('u');
  assert.equal(idx.length, 2);
  const composite = idx.find((i) => i.name === 'k_n');
  assert.deepEqual(composite.fields, ['k', 'n']);
  assert.deepEqual(composite.directions, [1, -1]);

  // 应该命中索引
  const plan = db.explain('u', { n: 50 });
  assert.equal(plan.stage, 'IXSCAN');
  assert.equal(plan.indexName, 'n_1');

  // 非索引字段 → 全表扫
  assert.equal(db.explain('u', { nope: 1 }).stage, 'COLLSCAN');

  assert.equal(db.dropIndex('u', 'k_n'), true);
  assert.equal(db.listIndexes('u').length, 1);
  assert.equal(db.dropIndex('u', 'k_n'), false, '重复删除返回 false');

  db.close();
  unlinkSync(DB);
});

test('唯一索引冲突抛 DuplicateKeyError', () => {
  const db = fresh();
  db.createIndex('u', 'email_1', 'email', { unique: true });
  db.insert('u', { email: 'a@b.c' });

  assert.throws(
    () => db.insert('u', { email: 'a@b.c' }),
    (err) => err instanceof Errors.DuplicateKeyError && err.code === 1,
  );

  db.close();
  unlinkSync(DB);
});

test('非法过滤条件抛 InvalidFilterError', () => {
  const db = fresh();
  assert.throws(
    () => db.find('u', { a: { $in: 'not-an-array' } }),
    (err) => err.code === 6,
  );
  db.close();
  unlinkSync(DB);
});

test('集合列表', () => {
  const db = fresh();
  db.insert('a', { x: 1 });
  db.insert('b', { x: 1 });

  const names = db.collections().sort();
  assert.deepEqual(names, ['a', 'b']);

  db.close();
  unlinkSync(DB);
});

test('关闭后调用报错', () => {
  const db = fresh();
  db.close();
  assert.throws(() => db.find('u'), /数据库已关闭/);
  unlinkSync(DB);
});

test('聚合:aggregate 走 C ABI', () => {
  const db = fresh();
  db.insertMany('orders', [
    { item: 'a', qty: 2, price: 10 },
    { item: 'b', qty: 1, price: 20 },
    { item: 'a', qty: 5, price: 10 },
  ]);

  // $group 汇总
  const grouped = db.aggregate('orders', [
    { $group: { _id: '$item', total: { $sum: '$qty' } } },
    { $sort: { _id: 1 } },
  ]);
  assert.equal(grouped.length, 2);
  assert.deepEqual(
    grouped.map((d) => [d._id, d.total]),
    [['a', 7], ['b', 1]],
  );

  // $unwind + $match
  db.insert('tags', { _id: '1', tags: ['x', 'y'] });
  const unwound = db.aggregate('tags', [
    { $unwind: '$tags' },
    { $match: { tags: 'y' } },
  ]);
  assert.equal(unwound.length, 1);
  assert.equal(unwound[0].tags, 'y');

  // 未实现的阶段必须报错,不能静默返回空
  assert.throws(() => db.aggregate('orders', [{ $out: 'x' }]));
  db.close();
  if (existsSync(DB)) unlinkSync(DB);
});

test('dropCollection 返回删除条数', () => {
  const db = fresh();
  db.insertMany('tmp', [{ a: 1 }, { a: 2 }]);
  assert.equal(db.collections().includes('tmp'), true);
  assert.equal(db.dropCollection('tmp'), 2);
  assert.equal(db.collections().includes('tmp'), false);
  // 不存在的集合删 0 条,不报错
  assert.equal(db.dropCollection('nope'), 0);
  db.close();
  if (existsSync(DB)) unlinkSync(DB);
});

test('compact 压缩文件', () => {
  const db = fresh();
  const bulk = Array.from({ length: 2000 }, (_, i) => ({ i, payload: 'x'.repeat(200) }));
  db.insertMany('blob', bulk);
  const before = statSize(DB);
  db.delete('blob', {});
  db.compact();
  const after = statSize(DB);
  assert.ok(after < before / 2, `压缩后应显著变小: ${before} → ${after}`);
  assert.equal(db.count('blob'), 0);
  db.close();
  if (existsSync(DB)) unlinkSync(DB);
});

test('$lookup 跨集合 join', () => {
  const db = fresh();
  db.insertMany('authors', [
    { _id: 'a1', name: '张三', country: 'CN' },
    { _id: 'a2', name: 'Bob', country: 'US' },
  ]);
  db.insertMany('books', [
    { _id: 'b1', title: '书一', authorId: 'a1' },
    { _id: 'b2', title: '书二', authorId: 'a1' },
    { _id: 'b3', title: '书三', authorId: 'a2' },
  ]);

  const rows = db.aggregate('books', [
    {
      $lookup: {
        from: 'authors',
        localField: 'authorId',
        foreignField: '_id',
        as: 'author',
      },
    },
    { $unwind: '$author' },
    { $sort: { _id: 1 } },
  ]);
  assert.equal(rows.length, 3);
  assert.equal(rows[0].author.name, '张三');
  assert.equal(rows[2].author.country, 'US');

  // $facet 里也能跑(子管道递归)
  const [facet] = db.aggregate('books', [
    {
      $facet: {
        left: [{ $match: { authorId: 'a1' } }, { $count: 'n' }],
        right: [{ $match: { authorId: 'a2' } }, { $count: 'n' }],
      },
    },
  ]);
  assert.equal(facet.left[0].n, 2);
  assert.equal(facet.right[0].n, 1);

  db.close();
  if (existsSync(DB)) unlinkSync(DB);
});

test('$expr / $mod / $regex $options', () => {
  const db = fresh();
  db.insertMany('products', [
    { _id: 'p1', price: 10, cost: 4, name: 'Apple' },
    { _id: 'p2', price: 20, cost: 15, name: 'banana' },
    { _id: 'p3', price: 30, cost: 10, name: 'Cherry' },
  ]);

  // 字段间比较
  const profitable = db.find('products', {
    $expr: { $gt: ['$price', { $multiply: ['$cost', 2] }] },
  });
  assert.deepEqual(profitable.map((d) => d._id), ['p1', 'p3']);

  // 取模
  const even = db.find('products', { price: { $mod: [20, 0] } });
  assert.deepEqual(even.map((d) => d._id), ['p2']);

  // 正则 + 大小写不敏感
  const matched = db.find('products', { name: { $regex: '^a', $options: 'i' } });
  assert.deepEqual(matched.map((d) => d._id), ['p1']);

  db.close();
  if (existsSync(DB)) unlinkSync(DB);
});

test('$bit 与 $[id] 位置运算符(arrayFilters)', () => {
  const db = fresh();
  db.insert('flags', { _id: 'f1', mask: 6 });

  db.update('flags', { _id: 'f1' }, { $bit: { mask: { and: 3 } } });
  assert.equal(db.get('flags', 'f1').mask, 2);

  db.insert('cart', {
    _id: 'c1',
    items: [
      { sku: 'a', qty: 1 },
      { sku: 'b', qty: 5 },
      { sku: 'a', qty: 3 },
    ],
  });
  // 只加 qty < 4 的元素
  const n = db.update(
    'cart',
    { _id: 'c1' },
    { $inc: { 'items.$[e].qty': 1 } },
    { arrayFilters: [{ 'e.qty': { $lt: 4 } }] },
  );
  assert.equal(n, 1);
  const doc = db.get('cart', 'c1');
  assert.deepEqual(doc.items.map((i) => i.qty), [2, 5, 4]);

  // 不存在的占位符必须报错,不能静默无操作
  assert.throws(() => db.update('cart', {}, { $inc: { 'items.$[ghost].qty': 1 } }));

  db.close();
  if (existsSync(DB)) unlinkSync(DB);
});
