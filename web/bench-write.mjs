/**
 * wasm 写路径计量:定位瓶颈在「WASI 边界往返」还是「JSON CPU」。
 *
 * 用法:
 *   node web/bench-write.mjs --wasm <nodblite.wasm> --dir <宿主目录> [--n 20000]
 *
 * 输出分项:
 *   - 宿主侧 JSON.stringify 耗时(生成待写入载荷)
 *   - wasm 内 insertMany 总耗时
 *   - WASI 各 I/O 调用的次数与字节数(fd_pwrite / fd_pread / fd_sync ...)
 *   - 派生指标:平均每页字节、每次提交的 pwrite 次数
 */

import fs from 'node:fs/promises';
import path from 'node:path';
import { loadWasm, openDb, wasiMetrics } from './nodblite.js';

function arg(name, fallback) {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 && process.argv[i + 1] ? process.argv[i + 1] : fallback;
}

const wasmPath = arg('wasm', 'target/wasm32-wasip1/release/nodblite.wasm');
const hostDir = arg('dir', '/tmp/nodblite-wasm-bench');
const n = Number(arg('n', '20000'));
const padLen = Number(arg('pad', '200'));

await fs.mkdir(hostDir, { recursive: true });
const dbName = `bench-${Date.now()}.nodb`;

// ---------- 1. 宿主侧准备载荷 ----------
const docs = new Array(n);
for (let i = 0; i < n; i++) {
  docs[i] = { i, name: `u${i}`, pad: 'z'.repeat(padLen) };
}
let t = process.hrtime.bigint();
const payload = JSON.stringify(docs);
const hostStringifyMs = Number(process.hrtime.bigint() - t) / 1e6;

// ---------- 2. wasm 侧写入 ----------
const metrics = wasiMetrics();
const ex = await loadWasm({ wasmPath, preopens: { '/data': hostDir }, metrics });
const db = openDb(ex, `/data/${dbName}`);

t = process.hrtime.bigint();
db.insertMany('c', docs);
const wasmMs = Number(process.hrtime.bigint() - t) / 1e6;

const count = db.count('c');
db.close();

// ---------- 3. 报告 ----------
const fileSize = (await fs.stat(path.join(hostDir, dbName))).size;
const pwrite = metrics.calls.fd_pwrite ?? 0;
const pwBytes = metrics.bytes.fd_pwrite ?? 0;
const pread = metrics.calls.fd_pread ?? 0;
const syncs = metrics.calls.fd_sync ?? 0;

const report = {
  条数: n,
  每文档填充: padLen,
  载荷字节: payload.length,
  宿主stringify毫秒: +hostStringifyMs.toFixed(1),
  wasm写入毫秒: +wasmMs.toFixed(1),
  wasm吞吐条每秒: Math.round(n / (wasmMs / 1000)),
  库文件字节: fileSize,
  wasi调用: metrics.calls,
  wasi字节: metrics.bytes,
  wasi耗时毫秒: Object.fromEntries(
    Object.entries(metrics.nanos).map(([k, v]) => [k, +(v / 1e6).toFixed(2)]),
  ),
  wasi耗时合计毫秒: +(Object.values(metrics.nanos).reduce((a, b) => a + b, 0) / 1e6).toFixed(2),
  pwrite次数: pwrite,
  pwrite平均字节: pwrite ? Math.round(pwBytes / pwrite) : 0,
  每次fsync对应pwrite次数: syncs ? +(pwrite / syncs).toFixed(1) : null,
  理论脏页数_4KB: Math.round(pwBytes / 4096),
};

console.log(JSON.stringify(report, null, 2));

// 清理:只留一份产物供人工检查
for (const f of await fs.readdir(hostDir)) {
  if (f.startsWith('bench-') && f !== dbName) await fs.rm(path.join(hostDir, f), { force: true });
}
