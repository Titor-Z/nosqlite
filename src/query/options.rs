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
    /// 跳过前 N 条。
    pub skip: usize,
    /// 最多返回 N 条(None 表示不限制)。
    pub limit: Option<usize>,
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
            SortError::InvalidDirection(field) => write!(
                f,
                "排序方向必须为 1(升序)或 -1(降序),字段: {}",
                field
            ),
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
        let names: Vec<&str> = docs
            .iter()
            .map(|d| d["name"].as_str().unwrap())
            .collect();
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
