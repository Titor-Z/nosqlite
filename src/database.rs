//! 对外门面:`Database` —— Rust 内嵌 API 的入口。
//!
//! 对底层 [`Storage`] 引擎做文档级封装,提供与 SQLite 类似的简单 API:
//!
//! - [`Database::create`] / [`Database::open`]:创建/打开 `.nodb` 数据库。
//! - [`Database::insert`]:向集合插入一条文档(自动生成或沿用 `_id`)。
//!
//! 后续里程碑将扩展 `find/update/delete` 等操作。

use std::path::Path;

use serde_json::{Map, Value};

use crate::document::{Document, DocumentError};
use crate::engine::{RedbEngine, Storage, StorageError};
use crate::query::{
    Filter, FilterError, FindOptions, Projection, SortError, SortSpec, Update, UpdateError,
};

/// nosqlite 数据库门面。
///
/// 持有具体存储引擎,对外暴露文档级操作。
#[derive(Debug)]
pub struct Database {
    /// 底层存储引擎(当前为 redb 实现)。
    engine: RedbEngine,
    /// 数据库文件路径(用于错误信息展示)。
    path: String,
}

/// 门面层统一错误:融合引擎错误与文档模型错误。
#[derive(Debug)]
pub enum DatabaseError {
    /// 存储引擎错误(见 [`StorageError`])。
    Storage(StorageError),
    /// 文档模型错误(见 [`DocumentError`])。
    Document(DocumentError),
    /// 查询条件错误(见 [`FilterError`])。
    Filter(FilterError),
    /// 排序规格错误(见 [`SortError`])。
    Sort(SortError),
    /// 更新操作错误(见 [`UpdateError`])。
    Update(UpdateError),
}

impl std::fmt::Display for DatabaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DatabaseError::Storage(e) => write!(f, "{}", e),
            DatabaseError::Document(e) => write!(f, "{}", e),
            DatabaseError::Filter(e) => write!(f, "{}", e),
            DatabaseError::Sort(e) => write!(f, "{}", e),
            DatabaseError::Update(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for DatabaseError {}

impl From<UpdateError> for DatabaseError {
    fn from(e: UpdateError) -> Self {
        DatabaseError::Update(e)
    }
}

impl From<serde_json::Error> for DatabaseError {
    fn from(e: serde_json::Error) -> Self {
        DatabaseError::Document(DocumentError::Json(e))
    }
}

impl From<StorageError> for DatabaseError {
    fn from(e: StorageError) -> Self {
        DatabaseError::Storage(e)
    }
}

impl From<DocumentError> for DatabaseError {
    fn from(e: DocumentError) -> Self {
        DatabaseError::Document(e)
    }
}

impl From<FilterError> for DatabaseError {
    fn from(e: FilterError) -> Self {
        DatabaseError::Filter(e)
    }
}

impl From<SortError> for DatabaseError {
    fn from(e: SortError) -> Self {
        DatabaseError::Sort(e)
    }
}

/// upsert 的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpsertResult {
    /// 命中了已有文档并更新,返回实际修改条数。
    Updated(usize),
    /// 未命中,已按 filter 的等值字段新建文档,返回新文档主键。
    Inserted(String),
}

impl Database {
    /// 创建新的 `.nodb` 数据库文件(目标文件必须不存在)。
    pub fn create(path: &Path) -> Result<Self, DatabaseError> {
        let engine = RedbEngine::create(path)?;
        Ok(Database {
            engine,
            path: path.display().to_string(),
        })
    }

    /// 打开已存在的 `.nodb` 数据库文件,并校验魔数与版本。
    pub fn open(path: &Path) -> Result<Self, DatabaseError> {
        let engine = RedbEngine::open(path)?;
        Ok(Database {
            engine,
            path: path.display().to_string(),
        })
    }

    /// 向指定集合插入一条文档,返回其主键 `_id`。
    ///
    /// - 文档顶层必须是 JSON 对象;
    /// - `_id` 缺省时自动生成 24 字符 ObjectId,并回填到文档中;
    /// - `_id` 重复时返回 [`DatabaseError::Storage`](`StorageError::DuplicateKey`) 主键冲突。
    pub fn insert(
        &self,
        collection: &str,
        document: &mut Document,
    ) -> Result<String, DatabaseError> {
        // 1. 确保主键:缺省则自动生成并写入文档
        let doc_id = document.ensure_id()?;
        // 2. 序列化文档为 JSON 字节
        let bytes = document.to_bytes()?;
        // 3. 交给引擎插入(重复主键由引擎报冲突)
        self.engine.insert(collection, &doc_id, &bytes)?;
        Ok(doc_id)
    }

    /// 关闭数据库(redb 在 Drop 时自动落盘,此处保留接口便于后续扩展)。
    pub fn close(&self) -> Result<(), DatabaseError> {
        self.engine.close()?;
        Ok(())
    }

    /// 查询集合内满足 filter 的文档,返回其 JSON 对象列表。
    ///
    /// - `filter` 为空(null/省略)时匹配全部文档;
    /// - 结果按 B+树主键顺序返回,不做排序与分页;
    /// - 若要排序/分页,请使用 [`Database::find_with_options`]。
    pub fn find(
        &self,
        collection: &str,
        filter: &Filter,
    ) -> Result<Vec<Map<String, Value>>, DatabaseError> {
        self.find_with_options(collection, filter, &FindOptions::default())
    }

    /// 查询集合内满足 filter 的文档,支持排序、跳过与限制。
    ///
    /// 处理流水线:**流式遍历** → 反序列化单条 → 过滤 → 排序 → skip → limit。
    ///
    /// **内存 M11 起为 O(命中数)**:不再把整个集合载入内存,未命中的文档在回调内即被释放。
    /// 若还需要排序,命中文档仍需全量保留以供排序(内存 O(命中数));
    /// 若只需分页,请改用 [`Database::find_page`](Self::find_page),内存可进一步降到 **O(n × 排序键大小)**。
    pub fn find_with_options(
        &self,
        collection: &str,
        filter: &Filter,
        options: &FindOptions,
    ) -> Result<Vec<Map<String, Value>>, DatabaseError> {
        // 1. 流式遍历 + 过滤(不再把整个集合物化进内存)
        let mut docs: Vec<Map<String, Value>> = Vec::new();
        let mut error: Option<DatabaseError> = None;
        self.engine.for_each(collection, &mut |_doc_id, bytes| {
            if error.is_some() {
                return false;
            }
            match Document::from_bytes(bytes) {
                Ok(doc) => {
                    let map = doc.as_map().clone();
                    if filter.matches(&map) {
                        docs.push(map);
                    }
                    true
                }
                Err(e) => {
                    error = Some(e.into());
                    false
                }
            }
        })?;
        if let Some(e) = error {
            return Err(e);
        }

        // 2. 排序
        if let Some(sort) = &options.sort {
            sort.sort(&mut docs);
        }

        // 3. skip
        if options.skip > 0 {
            let skip = options.skip.min(docs.len());
            docs.drain(0..skip);
        }

        // 4. limit
        if let Some(limit) = options.limit {
            docs.truncate(limit);
        }

        // 5. 字段投影(0.2.0):在排序/分页之后裁剪,保证排序字段可用
        if let Some(projection) = &options.projection {
            docs = docs.iter().map(|doc| projection.apply(doc)).collect();
        }

        Ok(docs)
    }

    /// 更新集合内所有满足 filter 的文档,返回**实际修改**条数。
    ///
    /// - 更新操作符当前支持 `$set`(字段点号路径赋值,嵌套自动创建/覆盖;路径段可为数组下标);
    /// - `_id` 主键不可被修改(修改报 [`DatabaseError::Update`]);
    /// - **内容无变化的文档不写回、不计数**(如 `$set` 写入相同值),与 MongoDB `modifiedCount` 语义一致;
    /// - 流水线:**流式遍历** → 过滤 → 应用更新 → 收集 → **单事务写回**(内存 O(命中数));
    /// - 写回是**原子**的:要么全部成功,要么整体回滚(不会留下「改了一半」的状态);
    /// - 若 filter 为 `{"_id": "xxx"}`,自动走主键直接寻址快速路径(不经全表扫描)。
    pub fn update(
        &self,
        collection: &str,
        filter: &Filter,
        update: &Update,
    ) -> Result<usize, DatabaseError> {
        if let Some(id) = filter.single_id_eq().and_then(|v| v.as_str()) {
            return Ok(self.update_by_id(collection, id, update)? as usize);
        }
        // 1. 流式遍历 + 过滤 + 应用更新,只收集实际要写的 (id, 新字节)
        let mut rows: Vec<(String, Vec<u8>)> = Vec::new();
        let mut error: Option<DatabaseError> = None;
        self.engine.for_each(collection, &mut |doc_id, bytes| {
            if error.is_some() {
                return false;
            }
            // 反序列化
            let doc = match Document::from_bytes(bytes) {
                Ok(d) => d,
                Err(e) => {
                    error = Some(e.into());
                    return false;
                }
            };
            let map = doc.as_map();
            // 判断是否满足 filter
            if !filter.matches(map) {
                return true;
            }
            // 应用更新(可能失败,如尝试修改 _id)
            let mut map_mut = map.clone();
            if let Err(e) = update.apply(&mut map_mut) {
                error = Some(e.into());
                return false;
            }
            // 内容没有任何变化:跳过写回与计数(避免无意义的写放大与虚增条数)
            if map_mut == *map {
                return true;
            }
            // 序列化(在事务外完成,减小事务持锁时间)
            match serde_json::to_vec(&map_mut) {
                Ok(b) => {
                    rows.push((doc_id.to_string(), b));
                    true
                }
                Err(e) => {
                    error = Some(e.into());
                    false
                }
            }
        })?;
        if let Some(e) = error {
            return Err(e);
        }

        // 2. 单事务写回(0.2.0):N 条修改只付一次 commit/fsync,且整体原子
        let modified = rows.len();
        self.engine.update_many(collection, &rows)?;
        Ok(modified)
    }

    /// 删除集合内所有满足 filter 的文档,返回删除条数。
    ///
    /// 流水线:**流式遍历** → 过滤 → 收集命中的主键 → **单事务删除**(内存 O(命中数)),
    /// 删除是**原子**的(要么全删要么全不删)。
    /// 若 filter 为 `{"_id": "xxx"}`,自动走主键直接寻址快速路径(不经全表扫描)。
    pub fn delete(&self, collection: &str, filter: &Filter) -> Result<usize, DatabaseError> {
        if let Some(id) = filter.single_id_eq().and_then(|v| v.as_str()) {
            return Ok(self.delete_by_id(collection, id)? as usize);
        }
        // 1. 流式遍历 + 过滤,只收集命中的主键
        let mut ids: Vec<String> = Vec::new();
        let mut error: Option<DatabaseError> = None;
        self.engine.for_each(collection, &mut |doc_id, bytes| {
            if error.is_some() {
                return false;
            }
            let doc = match Document::from_bytes(bytes) {
                Ok(d) => d,
                Err(e) => {
                    error = Some(e.into());
                    return false;
                }
            };
            if filter.matches(doc.as_map()) {
                ids.push(doc_id.to_string());
            }
            true
        })?;
        if let Some(e) = error {
            return Err(e);
        }

        // 2. 单事务删除(0.2.0):N 条删除只付一次 commit/fsync
        let deleted = ids.len();
        self.engine.delete_many(collection, &ids)?;
        Ok(deleted)
    }

    /// 统计集合内满足 filter 的文档条数。
    ///
    /// - filter 语义与 [`find`](Self::find) 完全一致;
    /// - filter 为 null(匹配全部)时即统计集合内文档总数。
    /// - 与 `find(...).len()` 等价,但只统计不反序列化出完整文档,
    ///   语义上更接近 MongoDB 的 `countDocuments`。
    pub fn count(&self, collection: &str, filter: &Filter) -> Result<usize, DatabaseError> {
        // 流式遍历 + 计数:内存 O(1),不再物化整个集合
        let mut total = 0usize;
        let mut error: Option<DatabaseError> = None;
        self.engine.for_each(collection, &mut |_doc_id, bytes| {
            if error.is_some() {
                return false;
            }
            let doc = match Document::from_bytes(bytes) {
                Ok(d) => d,
                Err(e) => {
                    error = Some(e.into());
                    return false;
                }
            };
            if filter.matches(doc.as_map()) {
                total += 1;
            }
            true
        })?;
        if let Some(e) = error {
            return Err(e);
        }
        Ok(total)
    }

    /// 列出数据库内全部集合名(去重,按集合名有序)。
    pub fn collections(&self) -> Result<Vec<String>, DatabaseError> {
        Ok(self.engine.list_collections()?)
    }

    /// 删除整个集合及其全部文档(不可恢复),返回删除的文档条数。
    ///
    /// 集合不存在时返回 `0`,不报错。
    pub fn drop_collection(&self, collection: &str) -> Result<usize, DatabaseError> {
        Ok(self.engine.drop_collection(collection)?)
    }

    /// 数据库文件路径。
    pub fn path(&self) -> &str {
        &self.path
    }

    // ==================== M11 新增:主键直接寻址与批量写入 ====================

    /// 按主键读取单条文档(M11 新增)。
    ///
    /// **O(log n) 直接定位,不经全表扫描**;内存 O(1)。
    /// 返回 `None` 表示该主键不存在。
    pub fn get_by_id(
        &self,
        collection: &str,
        doc_id: &str,
    ) -> Result<Option<Map<String, Value>>, DatabaseError> {
        match self.engine.get(collection, doc_id)? {
            Some(bytes) => {
                let doc = Document::from_bytes(&bytes)?;
                Ok(Some(doc.as_map().clone()))
            }
            None => Ok(None),
        }
    }

    /// 按主键更新单条文档(M11 新增),返回是否实际修改。
    ///
    /// **O(log n) 定位 + 单条改写**,不经全表扫描;内存 O(1)。
    /// 主键不存在返回 `false`;内容无变化(写入相同值)也不计数、不写回。
    /// `_id` 不可被修改(与 [`Database::update`](Self::update) 语义一致)。
    pub fn update_by_id(
        &self,
        collection: &str,
        doc_id: &str,
        update: &Update,
    ) -> Result<bool, DatabaseError> {
        // 1. 主键直接读取
        let Some(bytes) = self.engine.get(collection, doc_id)? else {
            return Ok(false);
        };

        // 2. 应用更新
        let doc = Document::from_bytes(&bytes)?;
        let map = doc.as_map();
        let mut map_mut = map.clone();
        update.apply(&mut map_mut)?;

        // 3. 内容无变化:不写回
        if map_mut == *map {
            return Ok(false);
        }

        // 4. 写回
        let new_bytes = serde_json::to_vec(&map_mut)?;
        self.engine.update(collection, doc_id, &new_bytes)?;
        Ok(true)
    }

    /// 按主键删除单条文档(M11 新增),返回是否实际删除。
    ///
    /// **O(log n) 直接定位**,不经全表扫描;主键不存在返回 `false`。
    pub fn delete_by_id(&self, collection: &str, doc_id: &str) -> Result<bool, DatabaseError> {
        self.engine.delete(collection, doc_id).map_err(Into::into)
    }

    /// 更新或插入(**upsert**,v0.1.4 新增)。
    ///
    /// 语义对齐 MongoDB `updateOne(filter, update, {upsert: true})`:
    ///
    /// - **命中**(至少一条满足 filter)→ 与 [`Database::update`](Self::update) 完全一致,
    ///   返回 [`UpsertResult::Updated`](UpsertResult::Updated)(可能为 0,表示内容无变化);
    /// - **未命中** → 用 filter 的**顶层等值字段**构造新文档,再应用 `update`,
    ///   返回 [`UpsertResult::Inserted`](UpsertResult::Inserted)(新主键)。
    ///
    /// 说明:
    /// - filter 里含操作符(`$gt` 等)或逻辑组合(`$or` 等)时,这些条件**无法**反推成
    ///   初始值,只取顶层的隐式等值字段(如 `{"name": "x", "age": {"$gt": 1}}` →
    ///   新文档只带 `name`)。
    /// - `_id` 缺省时自动生成;filter 里给了 `_id` 则沿用。
    /// - 写操作为单事务,失败不会留下半成品。
    pub fn update_or_insert(
        &self,
        collection: &str,
        filter: &Filter,
        update: &Update,
    ) -> Result<UpsertResult, DatabaseError> {
        // 1. 先尝试更新;命中则直接返回
        let modified = self.update(collection, filter, update)?;
        if modified > 0 {
            return Ok(UpsertResult::Updated(modified));
        }
        // 「命中但无变化」也要判定为 Updated:用 count 区分「没命中」与「命中未改」
        if self.count(collection, filter)? > 0 {
            return Ok(UpsertResult::Updated(0));
        }

        // 2. 未命中:用 filter 的等值字段构造初始文档
        let eq_fields = filter.eq_fields().ok_or_else(|| {
            DatabaseError::Update(UpdateError::InvalidUpdate(
                "upsert 要求 filter 的顶层字段全部是等值条件,才能反推新文档的初始值".to_string(),
            ))
        })?;
        let mut map = Map::new();
        for (key, value) in eq_fields {
            map.insert(key, value);
        }
        let mut doc = Document::new();
        for (k, v) in map {
            doc.insert(k, v);
        }

        // 3. 应用 update(可能再次失败,如对缺失字段做非法操作)
        update.apply(doc.as_map_mut())?;

        // 4. 插入
        let id = self.insert(collection, &mut doc)?;
        Ok(UpsertResult::Inserted(id))
    }

    /// 单事务批量插入(M11 新增),返回全部文档的主键列表(与入参顺序一致)。
    ///
    /// 所有文档在**同一个写事务**内写入并一次性提交 —— 这是写入吞吐的关键:
    /// 逐条插入每条都要付一次「开启事务 + 提交 + fsync」,批量把 N 条的成本合并为一次提交。
    /// 任一主键冲突则**整批失败**(随事务一并回滚)。
    /// `_id` 缺省时自动生成并回填到文档中。
    pub fn insert_many(
        &self,
        collection: &str,
        docs: &mut [Document],
    ) -> Result<Vec<String>, DatabaseError> {
        // 1. 确保主键 + 序列化(在计时/事务外完成,减小事务持锁时间)
        let mut rows: Vec<(String, Vec<u8>)> = Vec::with_capacity(docs.len());
        for doc in docs.iter_mut() {
            let doc_id = doc.ensure_id()?;
            let bytes = doc.to_bytes()?;
            rows.push((doc_id, bytes));
        }

        // 2. 单事务批量写入
        self.engine.insert_many(collection, &rows)?;

        Ok(rows.into_iter().map(|(id, _)| id).collect())
    }

    /// 排序 + 分页查询(M11 新增,两遍法,内存 O(n × 排序键大小))。
    ///
    /// 与 [`Database::find_with_options`](Self::find_with_options) 的差异:
    ///
    /// - **第一遍**:流式遍历,只提取 `(排序键, _id)` 小体量元组并排序(50 万条约 16 MB);
    /// - **第二遍**:按主键逐条回取页内文档(仅 `limit` 条,默认 20)。
    ///
    /// 因此**不再需要把命中文档的完整 DOM 全部留在内存** —— 50 万档从 819 MiB 降到几十 MiB,
    /// 且无需索引下推、不改变存储格式。
    /// 若无需排序(纯分页),第一遍甚至只记 `_id`,内存更低。
    ///
    /// # 参数
    /// - `filter` 过滤条件(与 `find` 语义一致)
    /// - `sort`   排序规格(可选;`None` 时按主键序取页,零额外内存)
    /// - `skip`   跳过前 N 条
    /// - `limit`  最多返回 N 条(`None` 表示返回 skip 之后全部)
    pub fn find_page(
        &self,
        collection: &str,
        filter: &Filter,
        sort: Option<&SortSpec>,
        skip: usize,
        limit: Option<usize>,
    ) -> Result<Vec<Map<String, Value>>, DatabaseError> {
        Ok(self
            .find_page_total(collection, filter, sort, skip, limit, None)?
            .1)
    }

    /// 排序 + 分页 + **一次扫描同时给出匹配总数**(0.2.0 新增,列表页专用)。
    ///
    /// 与 [`find_page`](Self::find_page) 的唯一差别:额外返回第一遍扫描中统计到的
    /// **匹配文档总数**(不是页内条数)。列表页需要「总数 + 当页」时,用本方法可省掉
    /// 一次独立的 [`count`](Self::count) 全表扫描 —— 500k 档可省约 2.4 s。
    ///
    /// `projection` 在排序/分页之后应用(保证排序字段与 `_id` 可被正常读取)。
    ///
    /// 返回 `(匹配总数, 页内文档)`。
    pub fn find_page_total(
        &self,
        collection: &str,
        filter: &Filter,
        sort: Option<&SortSpec>,
        skip: usize,
        limit: Option<usize>,
        projection: Option<&Projection>,
    ) -> Result<(usize, Vec<Map<String, Value>>), DatabaseError> {
        // ---------- 第一遍:流式收集 (排序键, _id) ----------
        let mut index: Vec<(Vec<Option<Value>>, String)> = Vec::new();
        let mut error: Option<DatabaseError> = None;
        self.engine.for_each(collection, &mut |_doc_id, bytes| {
            if error.is_some() {
                return false;
            }
            let doc = match Document::from_bytes(bytes) {
                Ok(d) => d,
                Err(e) => {
                    error = Some(e.into());
                    return false;
                }
            };
            let map = doc.as_map();
            if !filter.matches(map) {
                return true;
            }
            match sort {
                // 有排序:只保留排序键 + 主键(小体量)
                Some(spec) => {
                    let key = spec.extract_sort_key(map);
                    index.push((
                        key,
                        map.get("_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                    ));
                }
                // 无排序:主键即天然有序,直接记主键
                None => {
                    index.push((
                        Vec::new(),
                        map.get("_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                    ));
                }
            }
            true
        })?;
        if let Some(e) = error {
            return Err(e);
        }

        // ---------- 排序索引(小体量) ----------
        if let Some(spec) = sort {
            index.sort_by(|a, b| spec.compare_sort_keys(&a.0, &b.0));
        }

        // 匹配总数:排序之前统计(索引里就是全部命中项)
        let total = index.len();

        // ---------- 截取页内主键 ----------
        let start = skip.min(index.len());
        let page_ids: Vec<String> = match limit {
            Some(l) => index[start..]
                .iter()
                .take(l)
                .map(|(_, id)| id.clone())
                .collect(),
            None => index[start..].iter().map(|(_, id)| id.clone()).collect(),
        };
        // 索引立即释放,只留页内主键
        drop(index);

        // ---------- 第二遍:按主键回取页内文档(O(log n)/条) ----------
        let mut out = Vec::with_capacity(page_ids.len());
        for id in &page_ids {
            if let Some(doc) = self.get_by_id(collection, id)? {
                out.push(doc);
            }
        }

        // ---------- 字段投影(排序/分页之后应用) ----------
        if let Some(projection) = projection {
            out = out.iter().map(|doc| projection.apply(doc)).collect();
        }
        Ok((total, out))
    }
}
