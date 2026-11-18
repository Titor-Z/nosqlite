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

  // ---------- v0.7.x 新增能力(此前 wasm 宿主漏绑,这里锁死) ----------
  check('findPage 返回总数与当页', () => {
    const page = db.findPage('users', {}, { sort: { n: 1 }, limit: 2 });
    assert.equal(page.total, 5);
    assert.equal(page.items.length, 2);
    // 按 n 升序:缺 n 的(张三)排最前,其后是 n=1
    assert.equal(typeof page.items[1].n, 'number');
  });

  check('upsert 未命中则插入', () => {
    const r = db.upsert('users', { name: '新来的' }, { $set: { n: 99 } });
    assert.ok(r.inserted, `应返回 inserted,实际 ${JSON.stringify(r)}`);
  });

  check('aggregate 聚合管道', () => {
    const rows = db.aggregate('users', [
      { $group: { _id: null, total: { $sum: '$n' } } },
    ]);
    assert.equal(rows.length, 1);
    assert.equal(typeof rows[0].total, 'number');
  });

  check('explain 返回计划', () => {
    const plan = db.explain('users', {});
    assert.ok(plan && plan.stage, `应有 stage,实际 ${JSON.stringify(plan)}`);
  });

  check('复合索引 + 列表', () => {
    db.createIndexComposite('users', 'n_name', ['n', 'name']);
    const names = db.listIndexes('users').map((i) => i.name);
    assert.ok(names.includes('n_name'), `索引应存在,实际 ${JSON.stringify(names)}`);
  });

  check('游标逐条遍历', () => {
    const cur = db.cursor('users', {}, { sort: { n: 1 } });
    let seen = 0;
    let doc;
    while ((doc = cur.next()) !== null) {
      assert.ok(doc && typeof doc._id === 'string', '每条都应有 _id');
      seen += 1;
    }
    assert.ok(seen >= 5, `应遍历至少 5 条,实际 ${seen}`);
    cur.close();
  });

  check('dropCollection 返回条数', () => {
    db.insertMany('tmp_coll', [{ a: 1 }, { a: 2 }]);
    assert.equal(db.dropCollection('tmp_coll'), 2);
    assert.ok(!db.listCollections().includes('tmp_coll'));
  });

  db.close();

  // ---------- 持久化:重开后数据仍在 ----------
  const ex2 = await loadWasm({ wasmPath, preopens: { '/data': hostDir } });
  const db2 = openDb(ex2, '/data/smoke.nodb', false);
  check('重开后数据持久化', () => assert.equal(db2.count('users'), 6));
  db2.close();

  console.log(`\nWASM smoke: ALL PASSED (${passed} checks)`);
} finally {
  await fs.rm(hostDir, { recursive: true, force: true });
}
