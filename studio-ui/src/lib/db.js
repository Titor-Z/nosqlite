/**
 * 浏览器内的数据库会话:WASM 实例 + 内存文件系统。
 *
 * 一个 `.nodb` 文件的全部字节都在 `MemoryFs` 里;导入 = 写入,
 * 导出 = 读回。引擎本身完全不知道自己在浏览器里。
 */

import { MemoryFs, createWasi } from './wasi.js';
import { openDb } from './nodblite.js';

/** WASI 预打开目录固定为 `/data`(见 wasi.js)。 */
export const DB_PATH = '/data/db.nodb';
const FILE_NAME = 'db.nodb';

export class StudioDb {
  constructor() {
    this.fs = new MemoryFs();
    this.exports = null;
    this.db = null;
    this.module = null;
  }

  /** 加载并实例化 wasm(只做一次)。 */
  async init() {
    if (this.exports) return;
    const wasi = createWasi(this.fs);
    const response = await fetch('/nodblite.wasm');
    if (!response.ok) throw new Error(`加载 wasm 失败: HTTP ${response.status}`);
    if (!this.module) this.module = await WebAssembly.compile(await response.arrayBuffer());
    const instance = await WebAssembly.instantiate(this.module, wasi.imports);
    wasi.bind(instance.exports.memory);
    this.exports = instance.exports;
  }

  /**
   * 打开数据库。`bytes` 为空表示新建。
   * 打开失败时保持关闭状态,不留下半开的句柄。
   */
  open(bytes) {
    this.close();
    this.fs.reset();
    if (bytes && bytes.byteLength > 0) this.fs.setFile(FILE_NAME, new Uint8Array(bytes));
    try {
      this.db = openDb(this.exports, DB_PATH, true);
    } catch (e) {
      this.db = null;
      throw e;
    }
  }

  /** 当前数据库文件的完整字节(用于导出)。 */
  bytes() {
    const data = this.fs.getFile(FILE_NAME);
    return data ? data.slice() : new Uint8Array(0);
  }

  /** 当前文件大小。 */
  size() {
    return this.fs.getFile(FILE_NAME)?.length ?? 0;
  }

  close() {
    if (this.db) {
      this.db.close();
      this.db = null;
    }
  }
}
