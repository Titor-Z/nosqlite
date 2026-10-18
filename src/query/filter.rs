//! Filter 查询条件(M3)。
//!
//! 仿 MongoDB 文档过滤语法:
//!
//! - 顶层为 JSON 对象;每个字段的值若是普通 Value 则视为隐式 `$eq`;
//! - 值若是对象且键以 `$` 开头,则视为操作符对象,同一字段多个操作符按 AND 组合;
//! - 操作符子集:`$eq/$ne/$gt/$gte/$lt/$lte/$in/$nin/$regex`(M10 新增 `$elemMatch`,
//!   v0.1.2 新增字段级 `$not`);
//! - 支持点号路径(如 `a.b.c`),**路径遇数组会遍历其元素的同名子字段**;
//! - 顶层支持 `$and/$or/$nor/$not` 逻辑组合,语义与 MongoDB 一致:
//!   **同层字段条件与 `$or` 是 AND 关系**(v0.1.2 修正,旧版误为 OR);
//! - 数组语义与 MongoDB 对齐:标量等值 / `$in` 可命中「数组中包含该值」的文档。

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
    /// `$not`:字段条件取反(v0.1.2 新增,用于字段级 `{"a": {"$not": {...}}}`)。
    Not(Box<Condition>),
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
                // 解析出该路径上的**全部**候选值(点号路径穿透数组时可能多个)
                let values = resolve_paths(doc, &field.path);
                // 所有操作符须同时满足(AND),每个操作符在候选值列表上求值
                field.ops.iter().all(|op| op.matches_values(&values))
            }
            Condition::And(conds) => conds.iter().all(|c| c.matches(doc)),
            Condition::Or(conds) => conds.iter().any(|c| c.matches(doc)),
            Condition::Nor(conds) => conds.iter().all(|c| !c.matches(doc)),
            Condition::Not(cond) => !cond.matches(doc),
        }
    }
}

/// 判断「实际值」与「期望值」是否相等,数组语义与 MongoDB 一致:
///
/// 先比较整体(`actual == expected`),若不等且 `actual` 是数组,
/// 则**递归展开其元素**继续比较 —— 于是
/// `{"tags": "rust"}` 能命中 `{"tags": ["rust", "db"]}`,
/// `{"tags": ["rust"]}` 也能命中 `{"tags": [["rust"], "db"]}`(嵌套数组)。
fn value_matches(actual: &Value, expected: &Value) -> bool {
    if compare_values(Some(actual), Some(expected)) == Ordering::Equal {
        return true;
    }
    match actual {
        Value::Array(items) => items.iter().any(|item| value_matches(item, expected)),
        _ => false,
    }
}

/// 判断路径解析结果(候选值列表)与期望值是否相等。
///
/// 列表空(字段缺失)时,只有期望值为 `null` 才算匹配 —— MongoDB 把
/// 「字段缺失」当作 null 参与相等比较。
fn values_match(values: &[&Value], expected: &Value) -> bool {
    if values.is_empty() {
        return expected.is_null();
    }
    values.iter().any(|v| value_matches(v, expected))
}

impl Operator {
    /// 判断一组候选值(见 [`resolve_paths`])是否满足本操作符。
    ///
    /// 「候选值」在普通路径下只有一个;点号路径穿透数组时可能有多个。
    /// MongoDB 的规则是:比较类操作符由**任意**候选值满足即可,
    /// 同一字段上的多个操作符则需**分别**满足(AND)。
    fn matches_values(&self, values: &[&Value]) -> bool {
        match self {
            // $eq:任一候选值相等
            Operator::Eq(expected) => values_match(values, expected),
            // $ne:定义为 $eq 的取反(顺带修好「缺失 vs null」)
            Operator::Ne(expected) => !values_match(values, expected),
            // 比较类:任一候选值满足
            Operator::Gt(expected) => {
                values.iter().any(|v| compare_values(Some(v), Some(expected)) == Ordering::Greater)
            }
            Operator::Gte(expected) => {
                values.iter().any(|v| compare_values(Some(v), Some(expected)) != Ordering::Less)
            }
            Operator::Lt(expected) => {
                values.iter().any(|v| compare_values(Some(v), Some(expected)) == Ordering::Less)
            }
            Operator::Lte(expected) => {
                values.iter().any(|v| compare_values(Some(v), Some(expected)) != Ordering::Greater)
            }
            // $in:任一候选值等于候选列表任一元素
            Operator::In(list) => list.iter().any(|item| values_match(values, item)),
            // $nin:定义为 $in 的取反
            Operator::Nin(list) => !list.iter().any(|item| values_match(values, item)),
            // $regex:任一候选字符串匹配
            Operator::Regex(re) => values
                .iter()
                .any(|v| matches!(v, Value::String(s) if re.is_match(s))),
            // $elemMatch:本字段自身须为数组,且至少一个元素满足内部条件
            Operator::ElemMatch(spec) => values
                .iter()
                .any(|v| matches!(v, Value::Array(arr) if arr.iter().any(|el| spec_matches(spec, el)))),
            // $not:字段级取反(对整组候选值取反)
            Operator::Not(cond) => !cond.matches_field(values),
        }
    }
}

/// 判断 `$elemMatch` 的单个数组元素是否满足内部条件。
fn spec_matches(spec: &ElemMatchSpec, element: &Value) -> bool {
    match spec {
        // 操作符形态:对元素本身应用操作符
        ElemMatchSpec::Ops(ops) => ops.iter().all(|op| op.matches_values(&[element])),
        // 文档形态:元素必须是对象,对元素文档应用条件
        ElemMatchSpec::Doc(cond) => match element {
            Value::Object(map) => cond.matches(map),
            _ => false,
        },
    }
}

impl Condition {
    /// 在**已解析的候选值**上求值(用于字段级 `$not`)。
    ///
    /// 仅支持字段条件;逻辑组合(如 `$not: {"$and": [...]}`)在解析阶段就被拒绝。
    fn matches_field(&self, values: &[&Value]) -> bool {
        match self {
            // 路径为空 = 字段级 `$not` 内部的「当前值」,直接在该组候选值上求值
            Condition::Field(field) if field.path.is_empty() => {
                field.ops.iter().all(|op| op.matches_values(values))
            }
            Condition::Field(field) => field.ops.iter().all(|op| op.matches_values(values)),
            Condition::MatchAll => true,
            other => {
                debug_assert!(false, "字段级 $not 不应出现逻辑组合: {:?}", other);
                false
            }
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

/// 组合条件节点(v0.1.2 修正为 MongoDB 语义):
///
/// - 普通字段条件、`$and`、`$or`、`$nor`、`$not` **彼此是 AND 关系**;
///   `$or` 只在其内部的各项之间做 OR,`$nor` 则是「各项都不满足」;
/// - 旧版把「字段条件」与 `$or` 各项构成 OR,导致
///   `{"status": "ok", "$or": [{...}, {...}]}` 在两边含义不同,已修正;
/// - 全部为空(空对象)→ 匹配所有。
fn combine_conditions(
    mut fields: Vec<Condition>,
    and: Vec<Condition>,
    or: Vec<Condition>,
    nor: Vec<Condition>,
    not: Option<Box<Condition>>,
) -> Condition {
    // 1. 所有顶层条件一律按 AND 汇总(MongoDB 语义)
    let mut all: Vec<Condition> = Vec::new();
    all.append(&mut fields);
    all.extend(and);
    if !or.is_empty() {
        // $or 只在其子条件之间做 OR,整体再与外层 AND
        all.push(Condition::Or(or));
    }
    if !nor.is_empty() {
        // $nor:所有子条件都不满足
        all.push(Condition::Nor(nor));
    }
    if let Some(not_cond) = not {
        all.push(Condition::Not(not_cond));
    }

    // 2. 扁平化:只有一个条件时无需包一层
    match all.len() {
        0 => Condition::MatchAll,
        1 => all.pop().unwrap(),
        _ => Condition::And(all),
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
            "$not" => {
                // 字段级 $not:值必须是一条完整字段条件对象(可含操作符或多字段)
                let obj = value.as_object().ok_or_else(|| {
                    FilterError::InvalidOperator("$not 需要一个对象".to_string())
                })?;
                if obj.is_empty() {
                    return Err(FilterError::InvalidOperator(
                        "$not 的值不能是空对象".to_string(),
                    ));
                }
                if obj.keys().any(|k| matches!(k.as_str(), "$and" | "$or" | "$nor" | "$not")) {
                    return Err(FilterError::InvalidOperator(
                        "$not 在字段级不支持逻辑操作符,请使用顶层 $not".to_string(),
                    ));
                }
                let cond = if obj.keys().any(|k| k.starts_with('$')) {
                    Condition::Field(FieldCondition {
                        path: Vec::new(),
                        ops: parse_operator_object(obj)?,
                    })
                } else {
                    // 形如 {"a": {"$not": {"b": {"$gt": 1}}}} 属于顶层语义,此处拒绝
                    return Err(FilterError::InvalidOperator(
                        "$not 在字段级只接受操作符对象(如 {\"$gt\": 1})".to_string(),
                    ));
                };
                Operator::Not(Box::new(cond))
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

/// 按点号路径解析文档字段值,返回**全部候选值**(v0.1.2)。
///
/// 与旧 `resolve_path` 的差别:路径中遇到数组时**穿透其元素**继续解析,
/// 结果可能多个(MongoDB 行为)。例如 `a.b` 对
/// `{"a": [{"b": 1}, {"b": 2}, {"c": 3}]}` 返回 `[1, 2]`。
///
/// 字段缺失、路径不存在、或穿透数组后无任何命中 → 返回空切片。
fn resolve_paths<'a>(doc: &'a Map<String, Value>, path: &[String]) -> Vec<&'a Value> {
    let Some(first) = doc.get(&path[0]) else {
        return Vec::new();
    };
    // 路径只剩一段:直接就是该字段(数组保持整体,由操作符决定是否展开)
    if path.len() == 1 {
        return vec![first];
    }
    resolve_segments(first, &path[1..])
}

/// 递归解析剩余路径段,遇到数组时对每个元素分别继续。
fn resolve_segments<'a>(value: &'a Value, segments: &[String]) -> Vec<&'a Value> {
    let Some((head, rest)) = segments.split_first() else {
        return vec![value];
    };
    match value {
        Value::Object(map) => match map.get(head) {
            Some(next) => resolve_segments(next, rest),
            None => Vec::new(),
        },
        // 穿透数组:对每个元素用同一段路径继续
        Value::Array(items) => items
            .iter()
            .flat_map(|item| match item {
                Value::Object(map) => match map.get(head) {
                    Some(next) => resolve_segments(next, rest),
                    None => Vec::new(),
                },
                _ => Vec::new(),
            })
            .collect(),
        _ => Vec::new(),
    }
}
