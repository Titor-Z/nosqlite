//! 流式扫描会话(阶段 M22)。
//!
//! 这是 SQLite `sqlite3_stmt` 的等价物:**持有底层读事务与 B+树迭代器**,
//! 可跨多次调用持续推进,内存 O(一批) 而非 O(集合)。
//!
//! # 为什么需要它
//!
//! 旧接口 [`Storage::for_each`](crate::engine::Storage::for_each) 是回调式的:
//! 回调结束,读事务即关闭。这带来两个限制:
//!
//! 1. 游标只能把**整个结果集**缓存在内存里(wire 协议的 `getMore` 无法真流式);
//! 2. **索引扫描无法实现** —— 「从索引取一批 doc_id → 回表读文档」天然需要
//!    跨多次调用的读取。
//!
//! # 设计取舍
//!
//! 对外暴露的是 [`ScanSession::next_batch`] 而不是 Rust `Iterator`:
//!
//! - C ABI 无法接收 Rust 迭代器,但「从当前位置取一批」可以直接映射成 C 游标;
//! - wire 协议的 `getMore` 语义与 `next_batch` 一一对应;
//! - 避免 redb `Range` guard 与调用方生命周期互相纠缠。

use super::redb_engine::DOCS_TABLE;
use super::storage::StorageError;

/// 一条扫描结果:`(doc_id, 文档字节)`。
type ScanItem = Result<(Vec<u8>, Vec<u8>), StorageError>;

/// 底层 B+树迭代器(自持有读事务,故为 `'static`)。
type ScanIter = Box<dyn Iterator<Item = ScanItem> + Send>;

/// 一次流式扫描的会话。
///
/// 由 [`RedbEngine::scan_session_impl`](super::RedbEngine) 创建,
/// 每次 [`next_batch`](Self::next_batch) 从**上次结束位置继续**读取,不重新定位。
pub struct RedbScanSession {
    /// 集合 key 前缀(`集合名 + 0x00`)。
    prefix: Vec<u8>,
    /// 读事务句柄:会话存活期间保持快照隔离。
    ///
    /// 持有它(而非依赖调用方的 `&Database` 生命周期)使会话**自持有**,
    /// 调用方可以在持有游标时继续使用/移动 `Database`。
    _read_tx: redb::ReadTransaction,
    /// 底层 B+树迭代器(持有读事务,保证快照隔离)。
    ///
    /// redb 的 `Range<'static, ...>` 自持读事务,因此这里无需额外的生命周期参数。
    iter: ScanIter,
    /// 是否已耗尽。
    exhausted: bool,
}

impl RedbScanSession {
    /// 从集合起点创建会话。
    pub(crate) fn new(
        prefix: Vec<u8>,
        read_tx: redb::ReadTransaction,
        iter: impl Iterator<Item = ScanItem> + Send + 'static,
    ) -> Self {
        RedbScanSession {
            prefix,
            _read_tx: read_tx,
            iter: Box::new(iter),
            exhausted: false,
        }
    }

    /// 创建一个立即耗尽的会话(空库 / 表不存在时使用)。
    pub(crate) fn empty(prefix: Vec<u8>, read_tx: redb::ReadTransaction) -> Self {
        RedbScanSession {
            prefix,
            _read_tx: read_tx,
            iter: Box::new(std::iter::empty()),
            exhausted: true,
        }
    }

    /// 推进并返回下一批文档(`doc_id`, 文档字节)。
    ///
    /// 返回空 `Vec` 表示已耗尽;之后再次调用仍返回空(幂等)。
    pub fn next_batch(&mut self, max: usize) -> Result<Vec<(String, Vec<u8>)>, StorageError> {
        if self.exhausted || max == 0 {
            return Ok(Vec::new());
        }
        let mut out = Vec::with_capacity(max.min(1024));
        while out.len() < max {
            match self.iter.next() {
                Some(Ok((key, value))) => {
                    // 不再是本集合前缀 → 耗尽
                    if !key.starts_with(&self.prefix) {
                        self.exhausted = true;
                        break;
                    }
                    let doc_id = String::from_utf8_lossy(&key[self.prefix.len()..]).into_owned();
                    out.push((doc_id, value));
                }
                Some(Err(e)) => return Err(e),
                None => {
                    self.exhausted = true;
                    break;
                }
            }
        }
        Ok(out)
    }

    /// 是否已耗尽(无需再调用)。
    pub fn is_exhausted(&self) -> bool {
        self.exhausted
    }
}

impl super::storage::ScanSession for RedbScanSession {
    fn next_batch(&mut self, max: usize) -> Result<Vec<(String, Vec<u8>)>, StorageError> {
        RedbScanSession::next_batch(self, max)
    }
}

/// 打开文档表并按集合前缀创建迭代器(供引擎实现调用)。
///
/// 返回 `None` 表示文档表尚不存在(空库)。
pub(crate) fn open_range(
    read_tx: &redb::ReadTransaction,
    prefix: &[u8],
) -> Result<Option<impl Iterator<Item = ScanItem> + Send + 'static>, StorageError> {
    let table: redb::ReadOnlyTable<&[u8], &[u8]> = match read_tx.open_table(DOCS_TABLE) {
        Ok(t) => t,
        Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
        Err(e) => return Err(StorageError::Other(format!("打开文档表失败: {}", e))),
    };
    // 需要把 prefix 拷进迭代器闭包,避免借用调用方的临时值
    let prefix_owned = prefix.to_vec();
    let range = table
        .range(prefix_owned.as_slice()..)
        .map_err(|e| StorageError::Other(format!("遍历文档表失败: {}", e)))?;

    Ok(Some(range.map(move |entry| {
        entry
            .map(|(k, v)| (k.value().to_vec(), v.value().to_vec()))
            .map_err(|e| StorageError::Other(format!("读取文档失败: {}", e)))
    })))
}
