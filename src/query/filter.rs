//! Filter 查询条件(M3)。
//!
//! 仿 MongoDB 文档过滤语法:
//!
//! - 顶层为 JSON 对象;每个字段的值若是普通 Value 则视为隐式 `$eq`;
//! - 值若是对象且键以 `$` 开头,则视为操作符对象,同一字段多个操作符按 AND 组合;
//! - 操作符子集:`$eq/$ne/$gt/$gte/$lt/$lte/$in/$nin/$regex`(M10 新增 `$elemMatch`);
//! - 支持点号路径(如 `a.b.c`);
//! - 顶层支持 `$and/$or/$nor/$not` 逻辑组合。

use std::cmp::Ordering;

use serde_json::{Map, Value};

use super::compare::compare_values;

/// 查询条件错误。
#[derive(Debug)]
pub enum FilterError {
    /// filter 顶层必须是 JSON 对象(或省略/空对象表示匹配全部)。
    NotObject,
    /// 操作符值不合法(如 `$in` 需要数组、`$regex` 需要字符串)。
    InvalidOperator(String),
    /// `$regex` 模式非法。
    InvalidRegex(String),
}

impl std::fmt::Display for FilterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FilterError::NotObject => write!(f, "filter 顶层必须是 JSON 对象"),
            FilterError::InvalidOperator(msg) => write!(f, "操作符使用错误: {}", msg),
            FilterError::InvalidRegex(msg) => write!(f, "正则表达式无效: {}", msg),
        }
    }
}

impl std::error::Error for FilterError {}

/// 单个字段的查询条件。
#[derive(Debug)]
pub struct FieldCondition {
    /// 字段点号路径(如 `["a","b","c"]` 表示 `a.b.c`)。
    path: Vec<String>,
    /// 该字段上需同时满足的操作符(AND 组合)。
    ops: Vec<Operator>,
}

/// 逻辑条件节点。
#[derive(Debug)]
pub enum Condition {
    /// 匹配所有文档(空 filter)。
    MatchAll,
    /// 字段条件:路径 + 操作符集合。
    Field(FieldCondition),
    /// `$and`:所有子条件都必须满足。
    And(Vec<Condition>),
    /// `$or`:任一子条件满足即可。
    Or(Vec<Condition>),
    /// `$nor`:所有子条件都不满足。
    Nor(Vec<Condition>),
    /// `$not`:子条件不满足。
    Not(Box<Condition>),
}

/// 查询操作符。
#[derive(Debug)]
pub enum Operator {
    /// 等于。
    Eq(Value),
    /// 不等于。
    Ne(Value),
    /// 大于。
    Gt(Value),
    /// 大于等于。
    Gte(Value),
    /// 小于。
    Lt(Value),
    /// 小于等于。
    Lte(Value),
    /// 在数组中。
    In(Vec<Value>),
    /// 不在数组中。
    Nin(Vec<Value>),
    /// 正则匹配字符串字段。
    Regex(regex::Regex),
    /// `$elemMatch`:字段为数组且至少一个元素满足内部条件(M10 新增)。
    ElemMatch(Box<ElemMatchSpec>),
}

/// `$elemMatch` 的两种内部条件形态。
#[derive(Debug)]
pub enum ElemMatchSpec {
    /// 操作符形态(如 `{"$gte": 10, "$lt": 20}`):对每个元素直接应用操作符,
    /// 适用于标量数组,也可用于对象数组的整体比较(如 `$eq` 整个对象)。
    Ops(Vec<Operator>),
    /// 文档形态(如 `{"name": "x", "qty": {"$gt": 5}}`):元素必须是对象,
    /// 对元素文档应用完整条件(支持点号路径与逻辑组合)。
    Doc(Box<Condition>),
}

/// 顶层查询条件。
#[derive(Debug)]
pub struct Filter {
    /// 根条件。
    root: Condition,
}

impl Filter {
    /// 若本 filter 恰好是「`_id` 等值」单一条件,返回该主键值。
    ///
    /// 用于门面层的**主键直接寻址快速路径**(M11):`{"_id":"xxx"}` 可直接
    /// 走 B+树主键定位,不经全表扫描(50 万档下单条 update/delete 从 2.6 s 降到毫秒级)。
    /// 其余形态(多字段、逻辑组合、非等值)均返回 `None`。
    pub(crate) fn single_id_eq(&self) -> Option<&Value> {
        match &self.root {
            Condition::Field(field)
                if field.path.len() == 1 && field.path[0] == "_id" && field.ops.len() == 1 =>
            {
                match &field.ops[0] {
                    Operator::Eq(v) => Some(v),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// 从 JSON 值解析 filter。
    ///
    /// 顶层必须为对象或空(`Null` 视作匹配全部);省略时由调用方传入 `Filter::match_all()`。
    pub fn from_value(value: &Value) -> Result<Self, FilterError> {
        let root = match value {
            Value::Null => Condition::MatchAll,
            Value::Object(map) => parse_condition_object(map)?,
            _ => return Err(FilterError::NotObject),
        };
        Ok(Filter { root })
    }

    /// 匹配所有文档的空 filter。
    pub fn match_all() -> Self {
        Filter {
            root: Condition::MatchAll,
        }
    }

    /// 判断文档是否满足该 filter。
    pub fn matches(&self, doc: &Map<String, Value>) -> bool {
        self.root.matches(doc)
    }
}

impl Condition {
    /// 判断文档是否满足本条件节点。
    fn matches(&self, doc: &Map<String, Value>) -> bool {
        match self {
            Condition::MatchAll => true,
            Condition::Field(field) => {
                // 取路径值(缺失则 None)
                let value = resolve_path(doc, &field.path);
                // 所有操作符须同时满足(AND)
                field.ops.iter().all(|op| op.matches(value))
            }
            Condition::And(conds) => conds.iter().all(|c| c.matches(doc)),
            Condition::Or(conds) => conds.iter().any(|c| c.matches(doc)),
            Condition::Nor(conds) => conds.iter().all(|c| !c.matches(doc)),
            Condition::Not(cond) => !cond.matches(doc),
        }
    }
}

impl Operator {
    /// 判断字段值(可能缺失)是否满足本操作符。
    fn matches(&self, value: Option<&Value>) -> bool {
        match self {
            // $eq:存在且相等(缺失则否)
            Operator::Eq(expected) => match value {
                Some(v) => compare_values(Some(v), Some(expected)) == Ordering::Equal,
                None => false,
            },
            // $ne:缺失也满足(MongoDB 语义:$ne 匹配字段不存在或值不相等)
            Operator::Ne(expected) => match value {
                Some(v) => compare_values(Some(v), Some(expected)) != Ordering::Equal,
                None => true,
            },
            Operator::Gt(expected) => {
                matches!(value, Some(v) if compare_values(Some(v), Some(expected)) == Ordering::Greater)
            }
            Operator::Gte(expected) => {
                matches!(value, Some(v) if compare_values(Some(v), Some(expected)) != Ordering::Less)
            }
            Operator::Lt(expected) => {
                matches!(value, Some(v) if compare_values(Some(v), Some(expected)) == Ordering::Less)
            }
            Operator::Lte(expected) => {
                matches!(value, Some(v) if compare_values(Some(v), Some(expected)) != Ordering::Greater)
            }
            // $in:值等于数组任一元素
            Operator::In(list) => match value {
                Some(v) => list
                    .iter()
                    .any(|item| compare_values(Some(v), Some(item)) == Ordering::Equal),
                None => false,
            },
            // $nin:值不存在或不等数组任一元素
            Operator::Nin(list) => match value {
                Some(v) => list
                    .iter()
                    .all(|item| compare_values(Some(v), Some(item)) != Ordering::Equal),
                None => true,
            },
            // $regex:仅匹配字符串字段
            Operator::Regex(re) => match value {
                Some(Value::String(s)) => re.is_match(s),
                _ => false,
            },
            // $elemMatch:字段必须是数组,且至少一个元素满足内部条件
            Operator::ElemMatch(spec) => match value {
                Some(Value::Array(arr)) => arr.iter().any(|el| match spec.as_ref() {
                    // 操作符形态:对元素本身应用操作符(标量数组)
                    ElemMatchSpec::Ops(ops) => ops.iter().all(|op| op.matches(Some(el))),
                    // 文档形态:元素必须是对象,对元素文档应用条件
                    ElemMatchSpec::Doc(cond) => match el {
                        Value::Object(map) => cond.matches(map),
                        _ => false,
                    },
                }),
                _ => false,
            },
        }
    }
}

/// 解析顶层对象:区分逻辑操作符($and/$or/$nor/$not)与字段条件。
fn parse_condition_object(map: &Map<String, Value>) -> Result<Condition, FilterError> {
    // 统计逻辑操作符
    let mut and: Vec<Condition> = Vec::new();
    let mut or: Vec<Condition> = Vec::new();
    let mut nor: Vec<Condition> = Vec::new();
    let mut not: Option<Box<Condition>> = None;
    let mut fields: Vec<Condition> = Vec::new();

    for (key, value) in map {
        match key.as_str() {
            "$and" => {
                let arr = value
                    .as_array()
                    .ok_or_else(|| FilterError::InvalidOperator("$and 需要一个数组".to_string()))?;
                for item in arr {
                    let obj = item.as_object().ok_or_else(|| {
                        FilterError::InvalidOperator("$and 数组元素必须是对象".to_string())
                    })?;
                    and.push(parse_condition_object(obj)?);
                }
            }
            "$or" => {
                let arr = value
                    .as_array()
                    .ok_or_else(|| FilterError::InvalidOperator("$or 需要一个数组".to_string()))?;
                for item in arr {
                    let obj = item.as_object().ok_or_else(|| {
                        FilterError::InvalidOperator("$or 数组元素必须是对象".to_string())
                    })?;
                    or.push(parse_condition_object(obj)?);
                }
            }
            "$nor" => {
                let arr = value
                    .as_array()
                    .ok_or_else(|| FilterError::InvalidOperator("$nor 需要一个数组".to_string()))?;
                for item in arr {
                    let obj = item.as_object().ok_or_else(|| {
                        FilterError::InvalidOperator("$nor 数组元素必须是对象".to_string())
                    })?;
                    nor.push(parse_condition_object(obj)?);
                }
            }
            "$not" => {
                let obj = value
                    .as_object()
                    .ok_or_else(|| FilterError::InvalidOperator("$not 需要一个对象".to_string()))?;
                not = Some(Box::new(parse_condition_object(obj)?));
            }
            // 普通字段:解析字段条件
            field_path => fields.push(parse_field_condition(field_path, value)?),
        }
    }

    // 组合逻辑:字段条件 + $and + $not 按 AND 组合;外层按 $or/$nor 组合。
    // 语义:MongoDB 顶层 = AND(字段条件, $and, $not) 再与 ($or / $nor) 的关系
    let base = combine_conditions(fields, and, or, nor, not);
    Ok(base)
}

/// 组合条件节点:
/// - 将普通字段条件与 `$and`、`$not` 合并为 AND;
/// - 若存在 `$or`,则整体为 `OR(AND层, $or各项)`;
/// - 若存在 `$nor`,则整体为 `AND(上一步结果, NOR($nor各项))`;
/// - 全部为空(空对象)→ 匹配所有。
fn combine_conditions(
    mut fields: Vec<Condition>,
    and: Vec<Condition>,
    or: Vec<Condition>,
    nor: Vec<Condition>,
    not: Option<Box<Condition>>,
) -> Condition {
    // 1. 收集 AND 层:字段条件 + $and 子条件 + $not
    let mut and_conds = Vec::new();
    and_conds.append(&mut fields);
    and_conds.extend(and);
    if let Some(not_cond) = not {
        and_conds.push(Condition::Not(not_cond));
    }

    // 2. 构造 AND 层节点(为空则 None,不参与后续 OR 分支)
    let and_node = match and_conds.len() {
        0 => None,
        1 => Some(and_conds.pop().unwrap()),
        _ => Some(Condition::And(and_conds)),
    };

    // 3. 无任何条件 → 匹配所有
    if and_node.is_none() && or.is_empty() && nor.is_empty() {
        return Condition::MatchAll;
    }

    // 4. 处理 $or:OR(AND层?, $or各项)
    let or_node = if or.is_empty() {
        match and_node {
            Some(node) => node,
            None => Condition::MatchAll,
        }
    } else {
        let mut branches = Vec::new();
        if let Some(node) = and_node {
            branches.push(node);
        }
        branches.extend(or);
        Condition::Or(branches)
    };

    // 5. 处理 $nor:AND(上一步, NOR($nor各项))
    if nor.is_empty() {
        or_node
    } else {
        Condition::And(vec![or_node, Condition::Nor(nor)])
    }
}

/// 解析单个字段条件。
fn parse_field_condition(path: &str, value: &Value) -> Result<Condition, FilterError> {
    // 字段点号路径拆分
    let path = path.split('.').map(|s| s.to_string()).collect::<Vec<_>>();

    // 若值是对象且含 $ 前缀键 → 操作符对象
    let ops = match value {
        Value::Object(map) if map.keys().any(|k| k.starts_with('$')) => parse_operator_object(map)?,
        // 否则隐式 $eq
        other => vec![Operator::Eq(other.clone())],
    };

    Ok(Condition::Field(FieldCondition { path, ops }))
}

/// 解析操作符对象(如 `{"$gt": 18, "$lt": 60}`)。
fn parse_operator_object(map: &Map<String, Value>) -> Result<Vec<Operator>, FilterError> {
    let mut ops = Vec::new();
    for (key, value) in map {
        let op = match key.as_str() {
            "$eq" => Operator::Eq(value.clone()),
            "$ne" => Operator::Ne(value.clone()),
            "$gt" => Operator::Gt(value.clone()),
            "$gte" => Operator::Gte(value.clone()),
            "$lt" => Operator::Lt(value.clone()),
            "$lte" => Operator::Lte(value.clone()),
            "$in" => {
                let arr = value
                    .as_array()
                    .ok_or_else(|| FilterError::InvalidOperator("$in 需要一个数组".to_string()))?;
                Operator::In(arr.clone())
            }
            "$nin" => {
                let arr = value
                    .as_array()
                    .ok_or_else(|| FilterError::InvalidOperator("$nin 需要一个数组".to_string()))?;
                Operator::Nin(arr.clone())
            }
            "$regex" => {
                let pattern = value.as_str().ok_or_else(|| {
                    FilterError::InvalidOperator("$regex 需要一个字符串模式".to_string())
                })?;
                let re = regex::Regex::new(pattern)
                    .map_err(|e| FilterError::InvalidRegex(e.to_string()))?;
                Operator::Regex(re)
            }
            "$elemMatch" => {
                let obj = value.as_object().ok_or_else(|| {
                    FilterError::InvalidOperator("$elemMatch 需要一个对象".to_string())
                })?;
                // 对象含逻辑操作符 → 文档条件形态;含字段操作符 → 操作符形态;
                // 纯字段名 → 文档条件形态(隐式 $eq)
                let spec = if obj
                    .keys()
                    .any(|k| matches!(k.as_str(), "$and" | "$or" | "$nor" | "$not"))
                {
                    ElemMatchSpec::Doc(Box::new(parse_condition_object(obj)?))
                } else if obj.keys().any(|k| k.starts_with('$')) {
                    ElemMatchSpec::Ops(parse_operator_object(obj)?)
                } else {
                    ElemMatchSpec::Doc(Box::new(parse_condition_object(obj)?))
                };
                Operator::ElemMatch(Box::new(spec))
            }
            unknown => {
                return Err(FilterError::InvalidOperator(format!(
                    "不支持的字段操作符: {}",
                    unknown
                )));
            }
        };
        ops.push(op);
    }
    Ok(ops)
}

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
