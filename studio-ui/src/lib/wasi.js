/**
 * 浏览器端 WASI 最小实现:只有一个内存文件系统。
 *
 * nodblite 的 WASM 产物只导入 19 个 `wasi_snapshot_preview1` 函数
 * (clock / random / fd_read / fd_write / fd_pread / fd_pwrite / fd_sync /
 *  fd_filestat_* / path_open / path_filestat_get / prestat / proc_exit / environ)。
 * 这里逐一实现它们,不引入任何第三方 WASI 运行时。
 *
 * 关键语义:
 * - 预打开目录固定为 `/data`(fd 3),数据库文件放在它的根下;
 * - `fd_sync` 是无操作 —— 数据本来就在内存里,导出时才落成文件;
 * - 所有文件内容都是 `Uint8Array`,可整体取出用于下载。
 */

const ERRNO = {
  SUCCESS: 0,
  BADF: 8,
  EXIST: 20,
  INVAL: 28,
  IO: 29,
  NOENT: 44,
  NOTSUP: 58,
};

/** 文件类型:WASI 里 directory = 3,regular_file = 4。 */
const FILETYPE = { DIRECTORY: 3, REGULAR: 4 };

const PREOPEN_PATH = '/data';
const PREOPEN_FD = 3;

/** 内存文件系统。键是相对于 `/data` 的路径(如 `db.nodb`)。 */
export class MemoryFs {
  constructor() {
    this.files = new Map();
  }

  /** 写入/覆盖一个文件。 */
  setFile(name, bytes) {
    const data = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
    this.files.set(normalizeName(name), { data, mtime: Date.now() });
  }

  /** 读取一个文件(返回内部引用,调用方不要原地改)。 */
  getFile(name) {
    return this.files.get(normalizeName(name))?.data ?? null;
  }

  has(name) {
    return this.files.has(normalizeName(name));
  }

  reset() {
    this.files.clear();
  }
}

/** 去掉前导 `/`、`/data/` 前缀,避免同一文件出现两种键。 */
function normalizeName(name) {
  let n = String(name);
  if (n.startsWith(PREOPEN_PATH + '/')) n = n.slice(PREOPEN_PATH.length + 1);
  else if (n === PREOPEN_PATH) n = '';
  while (n.startsWith('/')) n = n.slice(1);
  return n;
}

/**
 * 创建 WASI 导入对象。
 *
 * @param {MemoryFs} fs
 * @param {{onStdout?: (s:string)=>void, onStderr?: (s:string)=>void}} [opts]
 * @returns {{imports: object, bind: (memory: WebAssembly.Memory) => void}}
 */
export function createWasi(fs, opts = {}) {
  let memory = null;
  /** fd → { kind: 'dir'|'file'|'stdout'|'stderr', name?, offset?, fdflags? } */
  const fds = new Map([
    [0, { kind: 'stdin' }],
    [1, { kind: 'stdout' }],
    [2, { kind: 'stderr' }],
    [PREOPEN_FD, { kind: 'dir' }],
  ]);
  let nextFd = 4;
  const startNs = BigInt(Math.round(performance.now() * 1e6));

  const view = () => new DataView(memory.buffer);
  const bytes = () => new Uint8Array(memory.buffer);

  const u32 = (ptr) => view().getUint32(ptr, true);
  const u64 = (ptr) => Number(view().getBigUint64(ptr, true));
  const setU32 = (ptr, v) => view().setUint32(ptr, v >>> 0, true);
  const setU64 = (ptr, v) => view().setBigUint64(ptr, BigInt(v), true);
  const putU32 = (ptr, v) => view().setUint32(ptr, v >>> 0, true);

  function readStr(ptr, len) {
    return new TextDecoder().decode(bytes().slice(ptr, ptr + len));
  }

  function writeStr(ptr, text, maxLen) {
    const b = new TextEncoder().encode(text);
    const n = Math.min(b.length, maxLen);
    bytes().set(b.subarray(0, n), ptr);
    return n;
  }

  function fileEntry(name) {
    return fs.files.get(normalizeName(name)) ?? null;
  }

  function ensureSize(entry, size) {
    if (entry.data.length === size) return;
    const next = new Uint8Array(size);
    next.set(entry.data.subarray(0, Math.min(entry.data.length, size)));
    entry.data = next;
  }

  function statInto(ptr, filetype, size, mtime) {
    setU64(ptr, 0); // dev
    setU64(ptr + 8, size + 1); // ino(用大小近似,仅用于区分)
    view().setUint8(ptr + 16, filetype);
    setU64(ptr + 24, 1); // nlink
    setU64(ptr + 32, size);
    setU64(ptr + 40, mtime);
    setU64(ptr + 48, mtime);
    setU64(ptr + 56, mtime);
  }

  /** 遍历 iovec 列表,逐个调用回调写入字节。 */
  function eachIov(iovsPtr, iovsLen, fn) {
    let written = 0;
    for (let i = 0; i < iovsLen; i++) {
      const base = iovsPtr + i * 8;
      const ptr = u32(base);
      const len = u32(base + 4);
      if (len === 0) continue;
      fn(bytes().subarray(ptr, ptr + len));
      written += len;
    }
    return written;
  }

  const wasi = {
    args_get: () => ERRNO.SUCCESS,
    args_sizes_get: (argcPtr, sizePtr) => {
      putU32(argcPtr, 0);
      putU32(sizePtr, 0);
      return ERRNO.SUCCESS;
    },
    environ_get: () => ERRNO.SUCCESS,
    environ_sizes_get: (countPtr, sizePtr) => {
      putU32(countPtr, 0);
      putU32(sizePtr, 0);
      return ERRNO.SUCCESS;
    },

    clock_time_get: (id, _precision, timePtr) => {
      // 0 实时 / 1 单调;其余按单调处理(redb 只需要不倒退的时间)
      const ns = id === 0
        ? BigInt(Date.now()) * 1000000n
        : BigInt(Math.round(performance.now() * 1e6));
      view().setBigUint64(timePtr, ns, true);
      return ERRNO.SUCCESS;
    },

    random_get: (ptr, len) => {
      const arr = bytes().subarray(ptr, ptr + len);
      if (globalThis.crypto?.getRandomValues) {
        globalThis.crypto.getRandomValues(arr);
      } else {
        for (let i = 0; i < arr.length; i++) arr[i] = Math.floor(Math.random() * 256);
      }
      return ERRNO.SUCCESS;
    },

    proc_exit: (code) => {
      throw new Error(`WASI proc_exit(${code})`);
    },

    fd_prestat_get: (fd, ptr) => {
      if (fd !== PREOPEN_FD) return ERRNO.BADF;
      view().setUint8(ptr, 0); // preopentype::dir
      view().setUint8(ptr + 1, 0);
      view().setUint8(ptr + 2, 0);
      view().setUint8(ptr + 3, 0);
      putU32(ptr + 4, PREOPEN_PATH.length);
      return ERRNO.SUCCESS;
    },

    fd_prestat_dir_name: (fd, ptr, len) => {
      if (fd !== PREOPEN_FD) return ERRNO.BADF;
      writeStr(ptr, PREOPEN_PATH, len);
      return ERRNO.SUCCESS;
    },

    fd_fdstat_get: (fd, ptr) => {
      const entry = fds.get(fd);
      if (!entry) return ERRNO.BADF;
      const filetype = entry.kind === 'dir' ? FILETYPE.DIRECTORY : FILETYPE.REGULAR;
      view().setUint8(ptr, filetype);
      view().setUint8(ptr + 1, 0);
      view().setUint16(ptr + 2, entry.fdflags ?? 0, true);
      view().setBigUint64(ptr + 8, 0xffffffffffffffffn, true);
      view().setBigUint64(ptr + 16, 0xffffffffffffffffn, true);
      return ERRNO.SUCCESS;
    },

    fd_fdstat_set_flags: (fd, flags) => {
      const entry = fds.get(fd);
      if (!entry) return ERRNO.BADF;
      entry.fdflags = flags;
      return ERRNO.SUCCESS;
    },

    fd_filestat_get: (fd, ptr) => {
      const entry = fds.get(fd);
      if (!entry) return ERRNO.BADF;
      if (entry.kind === 'dir') {
        statInto(ptr, FILETYPE.DIRECTORY, 0, Date.now());
        return ERRNO.SUCCESS;
      }
      if (entry.kind !== 'file') {
        statInto(ptr, FILETYPE.REGULAR, 0, Date.now());
        return ERRNO.SUCCESS;
      }
      const file = fileEntry(entry.name);
      if (!file) return ERRNO.BADF;
      statInto(ptr, FILETYPE.REGULAR, file.data.length, file.mtime);
      return ERRNO.SUCCESS;
    },

    fd_filestat_set_size: (fd, size) => {
      const entry = fds.get(fd);
      if (!entry || entry.kind !== 'file') return ERRNO.BADF;
      const file = fileEntry(entry.name);
      if (!file) return ERRNO.BADF;
      ensureSize(file, Number(size));
      file.mtime = Date.now();
      return ERRNO.SUCCESS;
    },

    path_filestat_get: (_dirfd, _flags, pathPtr, pathLen, ptr) => {
      const name = normalizeName(readStr(pathPtr, pathLen));
      if (name === '') {
        statInto(ptr, FILETYPE.DIRECTORY, 0, Date.now());
        return ERRNO.SUCCESS;
      }
      const file = fileEntry(name);
      if (!file) return ERRNO.NOENT;
      statInto(ptr, FILETYPE.REGULAR, file.data.length, file.mtime);
      return ERRNO.SUCCESS;
    },

    path_open: (
      _dirfd,
      _dirflags,
      pathPtr,
      pathLen,
      oflags,
      _rightsBase,
      _rightsInheriting,
      fdflags,
      outFdPtr,
    ) => {
      const name = normalizeName(readStr(pathPtr, pathLen));
      if (!name) return ERRNO.NOENT;
      const CREAT = 1;
      const TRUNC = 8;
      let file = fileEntry(name);
      if (!file) {
        if (!(oflags & CREAT)) return ERRNO.NOENT;
        file = { data: new Uint8Array(0), mtime: Date.now() };
        fs.files.set(name, file);
      } else if (oflags & TRUNC) {
        file.data = new Uint8Array(0);
        file.mtime = Date.now();
      }
      const fd = nextFd++;
      fds.set(fd, { kind: 'file', name, offset: 0, fdflags });
      putU32(outFdPtr, fd);
      return ERRNO.SUCCESS;
    },

    fd_read: (fd, iovsPtr, iovsLen, nreadPtr) => {
      const entry = fds.get(fd);
      if (!entry) return ERRNO.BADF;
      if (entry.kind === 'stdin') {
        putU32(nreadPtr, 0);
        return ERRNO.SUCCESS;
      }
      if (entry.kind !== 'file') return ERRNO.BADF;
      const file = fileEntry(entry.name);
      if (!file) return ERRNO.BADF;
      let total = 0;
      for (let i = 0; i < iovsLen; i++) {
        const base = iovsPtr + i * 8;
        const ptr = u32(base);
        const len = u32(base + 4);
        const chunk = file.data.subarray(entry.offset, entry.offset + len);
        bytes().set(chunk, ptr);
        entry.offset += chunk.length;
        total += chunk.length;
        if (chunk.length < len) break;
      }
      putU32(nreadPtr, total);
      return ERRNO.SUCCESS;
    },

    fd_pread: (fd, iovsPtr, iovsLen, offset, nreadPtr) => {
      const entry = fds.get(fd);
      if (!entry || entry.kind !== 'file') return ERRNO.BADF;
      const file = fileEntry(entry.name);
      if (!file) return ERRNO.BADF;
      let pos = Number(offset);
      let total = 0;
      for (let i = 0; i < iovsLen; i++) {
        const base = iovsPtr + i * 8;
        const ptr = u32(base);
        const len = u32(base + 4);
        const chunk = file.data.subarray(pos, pos + len);
        bytes().set(chunk, ptr);
        pos += chunk.length;
        total += chunk.length;
        if (chunk.length < len) break;
      }
      putU32(nreadPtr, total);
      return ERRNO.SUCCESS;
    },

    fd_write: (fd, iovsPtr, iovsLen, nwrittenPtr) => {
      const entry = fds.get(fd);
      if (!entry) return ERRNO.BADF;
      if (entry.kind === 'stdout' || entry.kind === 'stderr') {
        const text = [];
        for (let i = 0; i < iovsLen; i++) {
          const base = iovsPtr + i * 8;
          text.push(readStr(u32(base), u32(base + 4)));
        }
        const out = text.join('');
        if (entry.kind === 'stdout') opts.onStdout?.(out);
        else opts.onStderr?.(out);
        putU32(nwrittenPtr, out.length);
        return ERRNO.SUCCESS;
      }
      if (entry.kind !== 'file') return ERRNO.BADF;
      const file = fileEntry(entry.name);
      if (!file) return ERRNO.BADF;
      const append = (entry.fdflags ?? 0) & 1;
      let pos = append ? file.data.length : entry.offset;
      let total = 0;
      for (let i = 0; i < iovsLen; i++) {
        const base = iovsPtr + i * 8;
        const ptr = u32(base);
        const len = u32(base + 4);
        if (len === 0) continue;
        if (pos + len > file.data.length) ensureSize(file, pos + len);
        file.data.set(bytes().subarray(ptr, ptr + len), pos);
        pos += len;
        total += len;
      }
      if (!append) entry.offset = pos;
      file.mtime = Date.now();
      putU32(nwrittenPtr, total);
      return ERRNO.SUCCESS;
    },

    fd_pwrite: (fd, iovsPtr, iovsLen, offset, nwrittenPtr) => {
      const entry = fds.get(fd);
      if (!entry || entry.kind !== 'file') return ERRNO.BADF;
      const file = fileEntry(entry.name);
      if (!file) return ERRNO.BADF;
      let pos = Number(offset);
      const total = eachIov(iovsPtr, iovsLen, (chunk) => {
        if (pos + chunk.length > file.data.length) ensureSize(file, pos + chunk.length);
        file.data.set(chunk, pos);
        pos += chunk.length;
      });
      file.mtime = Date.now();
      putU32(nwrittenPtr, total);
      return ERRNO.SUCCESS;
    },

    fd_sync: () => ERRNO.SUCCESS,
    fd_datasync: () => ERRNO.SUCCESS,

    fd_close: (fd) => {
      if (!fds.has(fd)) return ERRNO.BADF;
      fds.delete(fd);
      return ERRNO.SUCCESS;
    },
  };

  return {
    imports: { wasi_snapshot_preview1: wasi },
    bind(mem) {
      memory = mem;
    },
  };
}
