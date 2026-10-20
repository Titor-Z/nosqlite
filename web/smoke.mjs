/**
 * WASM 冒烟验证:真实加载 wasm,跑一轮完整 CRUD。
 *
 * 这是 CI 的守门测试。只「能编译」不足以证明产物可用 ——
 * `NodbliteResult` / `NodbliteError` 的 wasm32 布局常量一旦写错,
 * 表现是静默乱码或越界访问,只有真跑才暴露。
 *
 * 用法:
 *   node web/smoke.mjs --wasm <path/to/nodblite.wasm>
 *   node web/smoke.mjs                    # 默认 target/wasm32-wasip1/release/nodblite.wasm
 */

import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';

import { loadWasm, openDb } from './nodblite.js';

function arg(name, fallback) {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 && process.argv[i + 1] ? process.argv[i + 1] : fallback;
}

const wasmPath = arg('wasm', 'target/wasm32-wasip1/release/nodblite.wasm');
await fs.access(wasmPath).catch(() => {
  throw new Error(`找不到 wasm 产物: ${wasmPath}`);
});

// 每次用独立临时目录,避免 CI 上残留文件互相干扰
const hostDir = await fs.mkdtemp(path.join(os.tmpdir(), 'nodblite-wasm-smoke-'));
let passed = 0;

function check(name, fn) {
  try {
    fn();
    passed++;
    console.log(`  ✓ ${name}`);
  } catch (e) {
    console.error(`  ✗ ${name}`);
    throw e;
  }
}

try {
  const ex = await loadWasm({ wasmPath, preopens: { '/data': hostDir } });
  const db = openDb(ex, '/data/smoke.nodb');

  // ---------- 基础写入与读取 ----------
  const id = db.insert('users', { name: '张三', age: 30 });
  check('insert 返回非空 _id', () => assert.ok(id && id.length === 24));

  check('get 按主键读回', () => {
    const doc = db.get('users', id);
    assert.equal(doc.name, '张三');
    assert.equal(doc.age, 30);
  });

  check('get 不存在的 _id 返回 null', () => assert.equal(db.get('users', 'nope'), null));

  // ---------- 批量与流式批量 ----------
  check('insertMany 写入 3 条', () => {
    const ids = db.insertMany('users', [
      { name: 'a', n: 1 },
      { name: 'b', n: 2 },
      { name: 'c', n: 3 },
    ]);
    assert.equal(ids.length, 3);
  });

  check('bulkInsert 写入 2 条', () => {
    const ids = db.bulkInsert('users', [{ name: 'd', n: 4 }, { name: 'e', n: 5 }]);
    assert.equal(ids.length, 2);
  });

  check('总数 6 条', () => assert.equal(db.count('users'), 6));

  // ---------- 查询语义 ----------
  check('find 带范围条件', () => {
    const docs = db.find('users', { n: { $gte: 4 } });
    assert.equal(docs.length, 2);
    assert.deepEqual(docs.map((d) => d.n).sort(), [4, 5]);
  });

  check('find 带排序与投影', () => {
    const docs = db.find('users', { n: { $exists: true } }, { sort: { n: -1 }, limit: 2 });
    assert.equal(docs.length, 2);
    assert.deepEqual(docs.map((d) => d.n), [5, 4]);
  });

  // ---------- 更新与删除 ----------
  check('update 返回影响条数', () => {
    assert.equal(db.update('users', { name: 'a' }, { $set: { n: 100 } }), 1);
    assert.equal(db.get('users', db.find('users', { name: 'a' })[0]._id).n, 100);
  });

  check('delete 返回影响条数', () => {
    assert.equal(db.delete('users', { name: 'e' }), 1);
    assert.equal(db.count('users'), 5);
  });

  // ---------- 集合与索引 ----------
  check('listCollections', () => {
    db.insert('posts', { title: 'hello' });
    assert.deepEqual(db.listCollections().sort(), ['posts', 'users']);
  });

  check('createIndex 后查询仍正确', () => {
    db.createIndex('users', 'by_n', 'n');
    const docs = db.find('users', { n: 100 });
    assert.equal(docs.length, 1);
    assert.equal(docs[0].name, 'a');
  });

  // ---------- 错误路径(验证 NodbliteError 布局) ----------
  check('主键冲突抛错且消息可读', () => {
    assert.throws(
      () => db.insert('users', { _id: id, name: '重复' }),
      (e) => {
        // 布局写错时 message 会是乱码 —— 这里要求它是可读中文
        assert.ok(e.message && e.message.length > 0, '错误消息不应为空');
        assert.ok(/主键|重复|已存在/.test(e.message), `错误消息应可读,实际: ${e.message}`);
        return true;
      },
    );
  });

  check('非法 filter 抛错', () => {
    assert.throws(() => db.find('users', { n: { $badOp: 1 } }));
  });

  db.close();

  // ---------- 持久化:重开后数据仍在 ----------
  const ex2 = await loadWasm({ wasmPath, preopens: { '/data': hostDir } });
  const db2 = openDb(ex2, '/data/smoke.nodb', false);
  check('重开后数据持久化', () => assert.equal(db2.count('users'), 5));
  db2.close();

  console.log(`\nWASM smoke: ALL PASSED (${passed} checks)`);
} finally {
  await fs.rm(hostDir, { recursive: true, force: true });
}
