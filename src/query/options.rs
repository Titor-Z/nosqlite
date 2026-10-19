//! 查询选项(M3):排序、跳过、限制。

use std::cmp::Ordering;

use serde_json::{Map, Value};

use super::compare::compare_values;

/// 排序方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    /// 升序(1)。
    Asc,
    /// 降序(-1)。
    Desc,
}

/// 单个排序字段。
#[derive(Debug)]
struct SortField {
    /// 字段点号路径(如 `a.b.c`)。
    path: Vec<String>,
    /// 排序方向。
    direction: Direction,
}

/// 排序规格(`{"age": 1, "name": -1}`)。
///
/// 多字段按声明顺序依次比较;支持点号路径。
#[derive(Debug)]
pub struct SortSpec {
    /// 排序字段列表(按声明顺序)。
    fields: Vec<SortField>,
}

impl SortSpec {
    /// 从 JSON 对象解析排序规格。
    ///
    /// 每个字段的值必须是 `1`(升序)或 `-1`(降序)。
    pub fn from_value(value: &Value) -> Result<Self, SortError> {
        let map = value.as_object().ok_or(SortError::NotObject)?;
        Self::from_map(map)
    }

    /// 从 JSON 对象解析排序规格(内部实现,便于复用)。
    fn from_map(map: &Map<String, Value>) -> Result<Self, SortError> {
        let mut fields = Vec::new();
        for (key, value) in map {
            let direction = match value {
                Value::Number(n) if n.as_i64() == Some(1) => Direction::Asc,
                Value::Number(n) if n.as_i64() == Some(-1) => Direction::Desc,
                _ => {
                    return Err(SortError::InvalidDirection(key.clone()));
                }
            };
            fields.push(SortField {
                path: key.split('.').map(|s| s.to_string()).collect(),
                direction,
            });
        }
        if fields.is_empty() {
            return Err(SortError::Empty);
        }
        Ok(SortSpec { fields })
    }

    /// 排序方向序列(`1` 升序 / `-1` 降序),供规划器与索引方向比对(M27)。
    pub fn direction_signs(&self) -> Vec<i8> {
        self.fields
            .iter()
            .map(|f| match f.direction {
                Direction::Asc => 1,
                Direction::Desc => -1,
            })
            .collect()
    }

    /// 全部排序字段的路径段列表(供查询规划器判断能否利用索引序)。
    pub fn field_paths(&self) -> Vec<&[String]> {
        self.fields.iter().map(|f| f.path.as_slice()).collect()
    }

    /// 若排序规格恰好只有一个字段,返回该字段的路径段(供查询规划器判断能否利用索引序)。
    pub fn single_field_path(&self) -> Option<&[String]> {
        if self.fields.len() == 1 {
            Some(&self.fields[0].path)
        } else {
            None
        }
    }

    /// 对文档列表按本规格排序(稳定排序)。
    pub fn sort(&self, docs: &mut [Map<String, Value>]) {
        docs.sort_by(|a, b| self.compare_docs(a, b));
    }

    /// 提取一条文档的排序键(M11 新增,供 `find_page` 两遍法使用)。
    ///
    /// 按字段声明顺序逐个取点号路径值,缺失字段为 `None`。
    /// 返回的向量长度恒等于字段数,便于两遍法中只保存小体量的排序键而非整份文档。
    pub fn extract_sort_key(&self, doc: &Map<String, Value>) -> Vec<Option<Value>> {
        self.fields
            .iter()
            .map(|field| resolve_path(doc, &field.path).cloned())
            .collect()
    }

    /// 比较两个排序键(M11 新增,方向由本规格决定)。
    ///
    /// 两者长度应均为字段数;若不等,按较短者逐位比较后由较长者胜出(不应发生)。
    pub fn compare_sort_keys(&self, a: &[Option<Value>], b: &[Option<Value>]) -> Ordering {
        for (i, field) in self.fields.iter().enumerate() {
            let va = a.get(i).and_then(|v| v.as_ref());
            let vb = b.get(i).and_then(|v| v.as_ref());
            let ord = compare_values(va, vb);
            let ord = match field.direction {
                Direction::Asc => ord,
                Direction::Desc => ord.reverse(),
            };
            if ord != Ordering::Equal {
                return ord;
            }
        }
        Ordering::Equal
    }

    /// 比较两个文档。
    fn compare_docs(&self, a: &Map<String, Value>, b: &Map<String, Value>) -> Ordering {
        for field in &self.fields {
            let va = resolve_path(a, &field.path);
            let vb = resolve_path(b, &field.path);
            let ord = compare_values(va, vb);
            let ord = match field.direction {
                Direction::Asc => ord,
                Direction::Desc => ord.reverse(),
            };
            if ord != Ordering::Equal {
                return ord;
            }
        }
        Ordering::Equal
    }
}

/// 查询选项:排序 + 跳过 + 限制。
#[derive(Debug, Default)]
pub struct FindOptions {
    /// 排序规格(可选)。
    pub sort: Option<SortSpec>,
    /// 字段投影(可选,`None` 表示返回整条文档)。
    pub projection: Option<Projection>,
    /// 跳过前 N 条。
    pub skip: usize,
    /// 最多返回 N 条(None 表示不限制)。
    pub limit: Option<usize>,
}

/// 字段投影(`{"name": 1, "_id": 0}`),语义对齐 MongoDB。
///
/// - 只出现 `1`/`true` → **包含模式**(未列入的字段不返回);
/// - 只出现 `0`/`false` → **排除模式**(未列入的字段全部返回);
/// - 两种模式混用报错(例外:`_id` 可以在任一模式中反向出现);
/// - `_id` 默认包含,除非显式 `"_id": 0`;
/// - 支持点号路径(`a.b`);数组下标路径不支持(整字段排除);
/// - v0.6.0 起支持**投影运算符**:`$slice`(截取数组)与 `$elemMatch`(取首个匹配元素)。
#[derive(Debug, Clone)]
pub struct Projection {
    /// 是否为包含模式。
    include: bool,
    /// 字段点号路径。
    paths: Vec<Vec<String>>,
    /// 是否保留 `_id`。
    keep_id: bool,
    /// 字段投影运算符(v0.6.0),按路径匹配。
    operators: Vec<(Vec<String>, ProjectionOp)>,
}

/// 字段投影运算符(v0.6.0)。
#[derive(Debug, Clone)]
pub enum ProjectionOp {
    /// `$slice`:`n` 取前 n(`n < 0` 取后 |n|);`[skip, limit]` 取区间。
    Slice {
        /// 跳过的元素数(`[skip, limit]` 形态);单值形态恒为 0。
        skip: i64,
        /// 取出的元素数(负数表示从尾部取)。
        limit: i64,
    },
    /// `$elemMatch`:只保留**第一个**匹配的元素(结果为单元素数组)。
    ///
    /// 内部条件复用过滤器的解析结果,语义与查询完全一致。
    ElemMatch(Box<crate::query::filter::Condition>),
}

/// 投影规格错误。
#[derive(Debug)]
pub enum ProjectionError {
    /// 顶层必须是 JSON 对象。
    NotObject,
    /// 值必须是 `1/true` 或 `0/false`。
    InvalidValue(String),
    /// 不能混用包含与排除(除 `_id` 外)。
    MixedIncludeExclude,
    /// 投影运算符的参数不合法。
    InvalidOperator(String),
    /// 路径为空(如 `""` 或 `"a..b"`)。
    EmptyPath,
}

impl std::fmt::Display for ProjectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectionError::NotObject => write!(f, "projection 顶层必须是 JSON 对象"),
            ProjectionError::InvalidValue(field) => {
                write!(
                    f,
                    "projection 字段值必须为 1/true 或 0/false,字段: {}",
                    field
                )
            }
            ProjectionError::MixedIncludeExclude => {
                write!(f, "projection 不能混用包含(1)与排除(0),仅 _id 可例外")
            }
            ProjectionError::EmptyPath => write!(f, "projection 字段路径不能为空"),
            ProjectionError::InvalidOperator(msg) => write!(f, "投影运算符错误: {}", msg),
        }
    }
}

impl std::error::Error for ProjectionError {}

impl Projection {
    /// 从 JSON 对象解析投影规格。
    pub fn from_value(value: &Value) -> Result<Self, ProjectionError> {
        let map = value.as_object().ok_or(ProjectionError::NotObject)?;

        // 1. 先收集显式声明的包含/排除字段(不含 _id)
        let mut include_fields: Vec<Vec<String>> = Vec::new();
        let mut exclude_fields: Vec<Vec<String>> = Vec::new();
        let mut keep_id = true;
        let mut operators: Vec<(Vec<String>, ProjectionOp)> = Vec::new();
        for (key, value) in map {
            // 投影运算符(`$slice` / `$elemMatch`)先于普通标志解析
            if let Value::Object(op_map) = value
                && op_map.keys().any(|k| k.starts_with('$'))
            {
                let path: Vec<String> = key.split('.').map(|s| s.to_string()).collect();
                if path.iter().any(|s| s.is_empty()) {
                    return Err(ProjectionError::EmptyPath);
                }
                let op = parse_projection_op(key, op_map)?;
                operators.push((path.clone(), op));
                // 运算符属于**包含模式**,把该字段也加入包含列表
                include_fields.push(path);
                continue;
            }
            let flag = match value {
                Value::Number(n) if n.as_i64() == Some(1) => true,
                Value::Number(n) if n.as_i64() == Some(0) => false,
                Value::Bool(b) => *b,
                _ => return Err(ProjectionError::InvalidValue(key.clone())),
            };
            if key == "_id" {
                keep_id = flag;
                continue;
            }
            let path: Vec<String> = key.split('.').map(|s| s.to_string()).collect();
            if path.iter().any(|s| s.is_empty()) {
                return Err(ProjectionError::EmptyPath);
            }
            if flag {
                include_fields.push(path);
            } else {
                exclude_fields.push(path);
            }
        }

        // 2. 模式判定:两种都非空 → 混用报错
        let include = match (include_fields.is_empty(), exclude_fields.is_empty()) {
            (false, false) => return Err(ProjectionError::MixedIncludeExclude),
            // 空投影或只有 _id → 按包含处理(只保留 _id 或全部)
            (true, true) => true,
            (false, true) => true,
            (true, false) => false,
        };
        let paths = if include {
            include_fields
        } else {
            exclude_fields
        };
        Ok(Projection {
            include,
            paths,
            keep_id,
            operators,
        })
    }

    /// 对一条文档应用投影,返回新文档(不修改原文档)。
    pub fn apply(&self, doc: &Map<String, Value>) -> Map<String, Value> {
        if self.include {
            let mut out = Map::new();
            if self.keep_id
                && let Some(id) = doc.get("_id")
            {
                out.insert("_id".to_string(), id.clone());
            }
            for path in &self.paths {
                if let Some(value) = get_path(doc, path) {
                    insert_path(&mut out, path, value.clone());
                }
            }
            // 应用投影运算符(在取出的字段上做局部变换)
            for (path, op) in &self.operators {
                let Some(value) = get_path(doc, path) else {
                    continue;
                };
                if let Some(transformed) = op.apply(value) {
                    insert_path(&mut out, path, transformed);
                } else {
                    remove_path(&mut out, path);
                }
            }
            out
        } else {
            let mut out = doc.clone();
            if !self.keep_id {
                out.remove("_id");
            }
            for path in &self.paths {
                remove_path(&mut out, path);
            }
            out
        }
    }
}

impl ProjectionOp {
    /// 对字段值应用运算符;返回 `None` 表示该字段应被移除。
    fn apply(&self, value: &Value) -> Option<Value> {
        match self {
            ProjectionOp::Slice { skip, limit } => {
                let Value::Array(arr) = value else {
                    return Some(value.clone());
                };
                let len = arr.len() as i64;
                let (start, count) = if *limit < 0 {
                    // 负数:从尾部取 |limit| 个
                    let n = (-*limit).min(len);
                    (len - n, n)
                } else {
                    let start = (*skip).clamp(0, len);
                    (start, (*limit).min(len - start))
                };
                let start = start.clamp(0, len) as usize;
                let end = (start as i64 + count.max(0)) as usize;
                Some(Value::Array(arr[start..end.min(arr.len())].to_vec()))
            }
            ProjectionOp::ElemMatch(cond) => {
                let Value::Array(arr) = value else {
                    return Some(value.clone());
                };
                // 只保留第一个匹配的元素
                arr.iter()
                    .find(|el| match el {
                        Value::Object(m) => cond.matches(m),
                        _ => false,
                    })
                    .map(|el| Value::Array(vec![el.clone()]))
                    .or(Some(Value::Array(Vec::new())))
            }
        }
    }
}

/// 解析单个字段上的投影运算符。
fn parse_projection_op(
    field: &str,
    map: &Map<String, Value>,
) -> Result<ProjectionOp, ProjectionError> {
    let mut out: Option<ProjectionOp> = None;
    for (k, v) in map {
        let op = match k.as_str() {
            "$slice" => match v {
                // 单值:n(负数从尾部取)
                Value::Number(n) => ProjectionOp::Slice {
                    skip: 0,
                    limit: n.as_i64().ok_or_else(|| {
                        ProjectionError::InvalidOperator("$slice 需要整数".to_string())
                    })?,
                },
                // 双值:[skip, limit]
                Value::Array(arr) if arr.len() == 2 => {
                    let skip = arr[0].as_i64().ok_or_else(|| {
                        ProjectionError::InvalidOperator("$slice 的 skip 需要整数".to_string())
                    })?;
                    let limit = arr[1].as_i64().ok_or_else(|| {
                        ProjectionError::InvalidOperator("$slice 的 limit 需要整数".to_string())
                    })?;
                    if limit < 1 {
                        return Err(ProjectionError::InvalidOperator(
                            "$slice 的 limit 必须为正数".to_string(),
                        ));
                    }
                    ProjectionOp::Slice { skip, limit }
                }
                _ => {
                    return Err(ProjectionError::InvalidOperator(
                        "$slice 需要整数或 [skip, limit]".to_string(),
                    ));
                }
            },
            "$elemMatch" => {
                let obj = v.as_object().ok_or_else(|| {
                    ProjectionError::InvalidOperator("$elemMatch 需要一个对象".to_string())
                })?;
                let cond = crate::query::filter::Condition::from_field_object(obj)
                    .map_err(|e| ProjectionError::InvalidOperator(e.to_string()))?;
                ProjectionOp::ElemMatch(Box::new(cond))
            }
            other => {
                return Err(ProjectionError::InvalidOperator(format!(
                    "不支持的投影运算符: {}",
                    other
                )));
            }
        };
        out = Some(op);
        let _ = field;
    }
    out.ok_or_else(|| ProjectionError::InvalidOperator("空运算符对象".to_string()))
}

/// 按点号路径取文档值(不进入数组)。
fn get_path<'a>(doc: &'a Map<String, Value>, path: &[String]) -> Option<&'a Value> {
    let mut current = doc.get(&path[0])?;
    for seg in &path[1..] {
        current = current.as_object()?.get(seg)?;
    }
    Some(current)
}

/// 按点号路径写入新文档(中间对象自动创建)。
fn insert_path(doc: &mut Map<String, Value>, path: &[String], value: Value) {
    if path.len() == 1 {
        doc.insert(path[0].clone(), value);
        return;
    }
    let entry = doc
        .entry(path[0].clone())
        .or_insert_with(|| Value::Object(Map::new()));
    if let Value::Object(map) = entry {
        insert_path(map, &path[1..], value);
    }
}

/// 按点号路径从文档删除字段(路径不存在则无操作)。
fn remove_path(doc: &mut Map<String, Value>, path: &[String]) {
    if path.len() == 1 {
        doc.remove(&path[0]);
        return;
    }
    if let Some(Value::Object(map)) = doc.get_mut(&path[0]) {
        remove_path(map, &path[1..]);
    }
}

/// 排序规格错误。
#[derive(Debug)]
pub enum SortError {
    /// sort 顶层必须是 JSON 对象。
    NotObject,
    /// 排序方向必须是 1 或 -1。
    InvalidDirection(String),
    /// 排序字段不能为空。
    Empty,
}

impl std::fmt::Display for SortError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SortError::NotObject => write!(f, "sort 顶层必须是 JSON 对象"),
            SortError::InvalidDirection(field) => {
                write!(f, "排序方向必须为 1(升序)或 -1(降序),字段: {}", field)
            }
            SortError::Empty => write!(f, "排序字段不能为空"),
        }
    }
}

impl std::error::Error for SortError {}

/// 按点号路径解析文档字段值(缺失返回 `None`)。
fn resolve_path<'a>(doc: &'a Map<String, Value>, path: &[String]) -> Option<&'a Value> {
    let mut current = doc.get(&path[0])?;
    for seg in &path[1..] {
        match current {
            Value::Object(map) => current = map.get(seg)?,
            _ => return None,
        }
    }
    Some(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 解析 `{"age": 1, "name": -1}` 成功。
    #[test]
    fn parse_valid_sort() {
        let spec = SortSpec::from_value(&json!({"age": 1, "name": -1})).expect("解析应成功");
        assert_eq!(spec.fields.len(), 2);
        assert!(matches!(spec.fields[0].direction, Direction::Asc));
        assert!(matches!(spec.fields[1].direction, Direction::Desc));
    }

    /// 非法方向应报错。
    #[test]
    fn reject_invalid_direction() {
        for bad in [json!({"age": 2}), json!({"age": 0}), json!({"age": "asc"})] {
            let err = SortSpec::from_value(&bad).expect_err("非法方向应报错");
            assert!(
                matches!(err, SortError::InvalidDirection(_)),
                "实际: {:?}",
                err
            );
        }
    }

    /// 空对象应报错。
    #[test]
    fn reject_empty() {
        let err = SortSpec::from_value(&json!({})).expect_err("空排序应报错");
        assert!(matches!(err, SortError::Empty));
    }

    /// 单字段升序排序。
    #[test]
    fn sort_asc() {
        let spec = SortSpec::from_value(&json!({"age": 1})).unwrap();
        let mut docs = vec![
            json!({"age": 30, "name": "b"}).as_object().unwrap().clone(),
            json!({"age": 20, "name": "a"}).as_object().unwrap().clone(),
            json!({"age": 25, "name": "c"}).as_object().unwrap().clone(),
        ];
        spec.sort(&mut docs);
        let ages: Vec<i64> = docs.iter().map(|d| d["age"].as_i64().unwrap()).collect();
        assert_eq!(ages, vec![20, 25, 30]);
    }

    /// 多字段排序:先按 age 升序,再按 name 降序。
    #[test]
    fn sort_multi_field() {
        let spec = SortSpec::from_value(&json!({"age": 1, "name": -1})).unwrap();
        let mut docs = vec![
            json!({"age": 20, "name": "b"}).as_object().unwrap().clone(),
            json!({"age": 20, "name": "a"}).as_object().unwrap().clone(),
            json!({"age": 30, "name": "c"}).as_object().unwrap().clone(),
        ];
        spec.sort(&mut docs);
        // age 相同(20)时按 name 降序:b > a
        let names: Vec<&str> = docs.iter().map(|d| d["name"].as_str().unwrap()).collect();
        assert_eq!(names, vec!["b", "a", "c"]);
    }

    /// 缺失字段排最前(类型序最小)。
    #[test]
    fn sort_missing_first() {
        let spec = SortSpec::from_value(&json!({"age": 1})).unwrap();
        let mut docs = vec![
            json!({"age": 30}).as_object().unwrap().clone(),
            json!({"name": "no_age"}).as_object().unwrap().clone(),
        ];
        spec.sort(&mut docs);
        assert!(docs[0].get("age").is_none(), "缺失 age 应排最前");
    }
}
