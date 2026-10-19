// nodblite npm 包的行为测试。
//
// 运行前:在仓库根目录执行 `cargo build --release --lib`,
// 或设置 NODBLITE_LIB 指向已有的动态库。

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, unlinkSync } from 'node:fs';
import { Database, Errors } from '../src/index.js';

const DB = '/tmp/nodblite_npm_test.nodb';

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
