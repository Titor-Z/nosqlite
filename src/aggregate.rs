//! 聚合管道执行(服务端 wire 与 C ABI 共用)。
//!
//! # 执行模型
//!
//! **逐阶段执行**:每个阶段吃上一阶段的文档集合、产出新的集合。
//! v0.6.x 的实现是「把各阶段收进局部变量最后一次性执行」,那种结构无法支持
//! `$unwind` / `$lookup` / `$facet`(它们必须拿到中间结果)。
//!
//! 唯一的优化:**首个 `$match` 下推到存储层**,这样 `{tenant: "t1"}` 这类
//! 前置过滤仍能走索引,而不是先全表读再内存过滤。
//!
//! # 已实现的阶段
//!
//! `$match` `$project` `$addFields`/`$set` `$unset` `$sort` `$skip` `$limit`
//! `$count` `$group` `$unwind` `$lookup` `$facet` `$bucket` `$replaceRoot`/`$replaceWith`
//!
//! # 已实现的 `$group` 累加器
//!
//! `$sum` `$avg` `$min` `$max` `$first` `$last` `$push` `$addToSet` `$count`
//!
//! 未实现的阶段/累加器一律返回 `40324`(与 MongoDB 的 `Location40324` 对齐),
//! **不静默忽略** —— 静默忽略会产出看起来正常却是错的结果。

use serde_json::{Map, Value, json};

use crate::database::Database;
use crate::query::expr::{as_number, eval, number_value};
use crate::query::{Filter, FindOptions, SortSpec, compare_values};

/// 聚合错误:带 MongoDB 风格错误码,供 wire 层映射。
#[derive(Debug, Clone)]
pub struct AggregateError {
    pub code: i32,
    pub message: String,
}

impl AggregateError {
    fn bad_value(message: impl Into<String>) -> Self {
        AggregateError {
            code: 2,
            message: message.into(),
        }
    }

    fn unsupported(message: impl Into<String>) -> Self {
        AggregateError {
            code: 40324,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for AggregateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// 执行聚合管道,返回输出文档。
pub fn run(
    db: &Database,
    collection: &str,
    pipeline: &Value,
) -> Result<Vec<Map<String, Value>>, AggregateError> {
    let stages = pipeline
        .as_array()
        .ok_or_else(|| AggregateError::bad_value("pipeline 必须是数组"))?;

    // 首阶段是 $match 时下推到存储层:保住索引能力,也少读数据
    let mut filter = Filter::match_all();
    let mut rest = stages.as_slice();
    if let Some(Value::Object(first)) = stages.first()
        && let Some(cond) = first.get("$match")
    {
        filter =
            Filter::from_value(cond).map_err(|e| AggregateError::bad_value(e.to_string()))?;
        rest = &stages[1..];
    }

    let docs = db
        .find_with_options(collection, &filter, &FindOptions::default())
        .map_err(|e| AggregateError::bad_value(e.to_string()))?;

    run_stages(db, docs, rest)
}

/// 逐阶段执行(供 `$facet` 的子管道复用)。
fn run_stages(
    db: &Database,
    mut docs: Vec<Map<String, Value>>,
    stages: &[Value],
) -> Result<Vec<Map<String, Value>>, AggregateError> {
    for stage in stages {
        let Some(obj) = stage.as_object() else {
            return Err(AggregateError::bad_value("每个 stage 必须是对象"));
        };
        let Some((name, arg)) = obj.iter().next() else {
            return Err(AggregateError::bad_value("stage 不能为空对象"));
        };
        if obj.len() != 1 {
            return Err(AggregateError::bad_value(format!(
                "每个 stage 只能有一个键,实际: {}",
                obj.len()
            )));
        }
        docs = match name.as_str() {
            "$match" => {
                let filter = Filter::from_value(arg)
                    .map_err(|e| AggregateError::bad_value(e.to_string()))?;
                docs.into_iter().filter(|d| filter.matches(d)).collect()
            }
            "$project" => {
                docs.iter().map(|d| project_doc(d, arg)).collect::<Result<_, _>>()?
            }
            "$addFields" | "$set" => {
                docs.iter().map(|d| add_fields(d, arg)).collect::<Result<_, _>>()?
            }
            "$unset" => {
                for doc in docs.iter_mut() {
                    for field in unset_fields(arg)? {
                        doc.remove(&field);
                    }
                }
                docs
            }
            "$sort" => {
                let sort = SortSpec::from_value(arg)
                    .map_err(|e| AggregateError::bad_value(e.to_string()))?;
                sort.sort(&mut docs);
                docs
            }
            "$skip" => {
                let n = arg
                    .as_u64()
                    .ok_or_else(|| AggregateError::bad_value("$skip 需要非负整数"))?
                    as usize;
                docs.into_iter().skip(n).collect()
            }
            "$limit" => {
                let n = arg
                    .as_u64()
                    .ok_or_else(|| AggregateError::bad_value("$limit 需要非负整数"))?
                    as usize;
                docs.into_iter().take(n).collect()
            }
            "$count" => count_stage(&docs, arg)?,
            "$group" => group_docs(&docs, arg)?,
            "$unwind" => unwind_stage(&docs, arg)?,
            "$lookup" => lookup_stage(db, &docs, arg)?,
            "$facet" => facet_stage(db, &docs, arg)?,
            "$bucket" => bucket_stage(&docs, arg)?,
            "$replaceRoot" => replace_root(&docs, arg, "newRoot")?,
            "$replaceWith" => replace_root(&docs, arg, "")?,
            other => {
                return Err(AggregateError::unsupported(format!(
                    "aggregate 阶段尚未实现: {}",
                    other
                )));
            }
        };
    }
    Ok(docs)
}

// ==================== $project / $addFields ====================

/// `$project`:既支持字段裁剪(`1`/`0`),也支持表达式计算。
///
/// 语义对齐 MongoDB:出现任何表达式或 `1` 即为**包含模式**,`_id` 默认保留;
/// 全部为 `0` 时为**排除模式**;两者混用报错。
fn project_doc(
    doc: &Map<String, Value>,
    spec: &Value,
) -> Result<Map<String, Value>, AggregateError> {
    let Some(obj) = spec.as_object() else {
        return Err(AggregateError::bad_value("$project 必须是对象"));
    };

    let is_expr = |v: &Value| !matches!(v, Value::Number(_) | Value::Bool(_));
    let inclusion = obj
        .iter()
        .any(|(k, v)| k != "_id" && (v.as_i64() == Some(1) || is_expr(v)));
    let exclusion = obj
        .iter()
        .any(|(k, v)| k != "_id" && (v.as_i64() == Some(0) || v == &Value::Bool(false)));
    if inclusion && exclusion {
        return Err(AggregateError::bad_value(
            "$project 不能混用包含与排除(除 _id 外)",
        ));
    }

    let mut out = Map::new();
    if inclusion {
        // `_id` 默认保留,除非显式排除
        match obj.get("_id") {
            Some(Value::Number(n)) if n.as_i64() == Some(0) => {}
            Some(v) if is_expr(v) => {
                out.insert("_id".to_string(), project_value(v, doc)?);
            }
            _ => {
                if let Some(id) = doc.get("_id") {
                    out.insert("_id".to_string(), id.clone());
                }
            }
        }
        for (key, value) in obj {
            if key == "_id" {
                continue;
            }
            if value.as_i64() == Some(0) || value == &Value::Bool(false) {
                continue;
            }
            if is_expr(value) {
                out.insert(key.clone(), project_value(value, doc)?);
            } else if let Some(found) = doc.get(key) {
                out.insert(key.clone(), found.clone());
            }
        }
    } else {
        // 排除模式:先克隆,再删掉指定字段
        out = doc.clone();
        for (key, value) in obj {
            if value.as_i64() == Some(0) || value == &Value::Bool(false) {
                out.remove(key);
            }
        }
    }
    Ok(out)
}

/// `$project` 的单个字段值:`1` 表示原样保留,其余按表达式求值。
fn project_value(value: &Value, doc: &Map<String, Value>) -> Result<Value, AggregateError> {
    if value.as_i64() == Some(1) || value == &Value::Bool(true) {
        return Ok(Value::Null);
    }
    eval(value, doc).map_err(|e| AggregateError::bad_value(e.to_string()))
}

/// `$addFields` / `$set`:在原文档上覆盖/新增字段。
fn add_fields(
    doc: &Map<String, Value>,
    spec: &Value,
) -> Result<Map<String, Value>, AggregateError> {
    let Some(obj) = spec.as_object() else {
        return Err(AggregateError::bad_value("$addFields 必须是对象"));
    };
    let mut out = doc.clone();
    for (key, value) in obj {
        let computed = eval(value, doc).map_err(|e| AggregateError::bad_value(e.to_string()))?;
        set_dotted(&mut out, key, computed);
    }
    Ok(out)
}

/// 按点号路径写入字段(中间对象自动创建)。
fn set_dotted(doc: &mut Map<String, Value>, path: &str, value: Value) {
    let mut segments = path.split('.').peekable();
    let Some(first) = segments.next() else {
        return;
    };
    if segments.peek().is_none() {
        doc.insert(first.to_string(), value);
        return;
    }
    let entry = doc
        .entry(first.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    match entry {
        Value::Object(map) => set_dotted(map, &segments.collect::<Vec<_>>().join("."), value),
        _ => *entry = value,
    }
}

/// `$unset`:字段名或字段名数组。
fn unset_fields(arg: &Value) -> Result<Vec<String>, AggregateError> {
    match arg {
        Value::String(s) => Ok(vec![s.clone()]),
        Value::Array(items) => items
            .iter()
            .map(|v| {
                v.as_str()
                    .map(|s| s.to_string())
                    .ok_or_else(|| AggregateError::bad_value("$unset 的元素必须是字段名字符串"))
            })
            .collect(),
        _ => Err(AggregateError::bad_value("$unset 需要字段名或字段名数组")),
    }
}

// ==================== $count ====================

/// `$count: "n"` → 单个文档 `{n: <条数>}`;空输入产出**空结果集**(MongoDB 语义)。
fn count_stage(
    docs: &[Map<String, Value>],
    arg: &Value,
) -> Result<Vec<Map<String, Value>>, AggregateError> {
    let name = arg
        .as_str()
        .ok_or_else(|| AggregateError::bad_value("$count 需要字段名字符串"))?;
    if docs.is_empty() {
        return Ok(Vec::new());
    }
    let mut doc = Map::new();
    doc.insert(name.to_string(), json!(docs.len()));
    Ok(vec![doc])
}

// ==================== $group ====================

/// 单个累加器的状态。
enum Acc {
    Sum(f64),
    Avg { sum: f64, count: usize },
    Min(Option<Value>),
    Max(Option<Value>),
    First(Option<Value>),
    Last(Option<Value>),
    Push(Vec<Value>),
    AddToSet(Vec<Value>),
    Count(i64),
}

/// 一条累加器定义:输出字段名 + 操作符 + 参数表达式。
struct AccSpec {
    name: String,
    op: String,
    arg: Value,
}

impl AccSpec {
    fn initial(&self) -> Acc {
        match self.op.as_str() {
            "$sum" => Acc::Sum(0.0),
            "$avg" => Acc::Avg {
                sum: 0.0,
                count: 0,
            },
            "$min" => Acc::Min(None),
            "$max" => Acc::Max(None),
            "$first" => Acc::First(None),
            "$last" => Acc::Last(None),
            "$push" => Acc::Push(Vec::new()),
            "$addToSet" => Acc::AddToSet(Vec::new()),
            _ => Acc::Count(0),
        }
    }

    /// 累加一篇文档。
    fn feed(&self, acc: &mut Acc, doc: &Map<String, Value>) -> Result<(), AggregateError> {
        // `$count` 不取参数;其余按表达式求值
        let value = if self.op == "$count" {
            Value::from(1)
        } else {
            eval(&self.arg, doc).map_err(|e| AggregateError::bad_value(e.to_string()))?
        };
        match acc {
            Acc::Sum(total) => {
                if let Some(n) = as_number(&value) {
                    *total += n;
                }
            }
            Acc::Avg { sum, count } => {
                if let Some(n) = as_number(&value) {
                    *sum += n;
                    *count += 1;
                }
            }
            Acc::Min(current) => {
                if !value.is_null()
                    && current
                        .as_ref()
                        .is_none_or(|c| compare_values(Some(&value), Some(c)).is_lt())
                {
                    *current = Some(value);
                }
            }
            Acc::Max(current) => {
                if !value.is_null()
                    && current
                        .as_ref()
                        .is_none_or(|c| compare_values(Some(&value), Some(c)).is_gt())
                {
                    *current = Some(value);
                }
            }
            Acc::First(slot) => {
                if slot.is_none() {
                    *slot = Some(value);
                }
            }
            Acc::Last(slot) => *slot = Some(value),
            Acc::Push(items) => items.push(value),
            Acc::AddToSet(items) => {
                if !items
                    .iter()
                    .any(|existing| compare_values(Some(existing), Some(&value)).is_eq())
                {
                    items.push(value);
                }
            }
            Acc::Count(n) => *n += 1,
        }
        Ok(())
    }

    /// 产出最终值。
    fn finish(&self, acc: Acc) -> Value {
        match acc {
            Acc::Sum(total) => number_value(total),
            Acc::Avg { sum, count } => {
                if count == 0 {
                    Value::Null
                } else {
                    number_value(sum / count as f64)
                }
            }
            Acc::Min(v) | Acc::Max(v) | Acc::First(v) | Acc::Last(v) => v.unwrap_or(Value::Null),
            Acc::Push(items) | Acc::AddToSet(items) => Value::Array(items),
            Acc::Count(n) => Value::from(n),
        }
    }
}

/// 解析 `$group` / `$bucket.output` 里的累加器定义。
fn parse_accumulators(spec: &Map<String, Value>) -> Result<Vec<AccSpec>, AggregateError> {
    let mut out = Vec::new();
    for (name, value) in spec {
        let obj = value.as_object().ok_or_else(|| {
            AggregateError::bad_value(format!("累加器 {} 必须是 {{$op: 参数}} 形式", name))
        })?;
        if obj.len() != 1 {
            return Err(AggregateError::bad_value(format!(
                "累加器 {} 只能有一个操作符",
                name
            )));
        }
        let (op, arg) = obj.iter().next().unwrap();
        match op.as_str() {
            "$sum" | "$avg" | "$min" | "$max" | "$first" | "$last" | "$push" | "$addToSet"
            | "$count" => out.push(AccSpec {
                name: name.clone(),
                op: op.clone(),
                arg: arg.clone(),
            }),
            other => {
                return Err(AggregateError::unsupported(format!(
                    "$group 累加器尚未实现: {}",
                    other
                )));
            }
        }
    }
    Ok(out)
}

/// `$group`:按 `_id` 表达式分组,各组按**首次出现顺序**输出。
fn group_docs(
    docs: &[Map<String, Value>],
    arg: &Value,
) -> Result<Vec<Map<String, Value>>, AggregateError> {
    let Some(obj) = arg.as_object() else {
        return Err(AggregateError::bad_value("$group 必须是对象"));
    };
    let group_id = obj
        .get("_id")
        .ok_or_else(|| AggregateError::bad_value("$group 缺少 _id"))?;
    let mut acc_map = Map::new();
    for (key, value) in obj {
        if key != "_id" {
            acc_map.insert(key.clone(), value.clone());
        }
    }
    let specs = parse_accumulators(&acc_map)?;

    // 保持首次出现顺序;key 用序列化后的字符串做分组标识
    let mut order: Vec<String> = Vec::new();
    let mut groups: std::collections::HashMap<String, (Value, Vec<Acc>)> =
        std::collections::HashMap::new();

    for doc in docs {
        let key = eval(group_id, doc).map_err(|e| AggregateError::bad_value(e.to_string()))?;
        let repr = serde_json::to_string(&key).unwrap_or_default();
        let entry = groups.entry(repr.clone()).or_insert_with(|| {
            order.push(repr.clone());
            (key.clone(), specs.iter().map(|s| s.initial()).collect())
        });
        for (spec, acc) in specs.iter().zip(entry.1.iter_mut()) {
            spec.feed(acc, doc)?;
        }
    }

    let mut out = Vec::with_capacity(order.len());
    for repr in order {
        let (key, accs) = groups.remove(&repr).unwrap();
        let mut doc = Map::new();
        doc.insert("_id".to_string(), key);
        for (spec, acc) in specs.iter().zip(accs) {
            doc.insert(spec.name.clone(), spec.finish(acc));
        }
        out.push(doc);
    }
    Ok(out)
}

// ==================== $unwind ====================

/// `$unwind`:把数组字段展开成多条文档。
///
/// 支持字符串简写(`"$tags"`)与对象形式
/// (`{path, preserveNullAndEmptyArrays, includeArrayIndex}`)。
fn unwind_stage(
    docs: &[Map<String, Value>],
    arg: &Value,
) -> Result<Vec<Map<String, Value>>, AggregateError> {
    let (path, preserve, index_field) = match arg {
        Value::String(s) => (s.clone(), false, None),
        Value::Object(obj) => {
            let path = obj
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AggregateError::bad_value("$unwind 缺少 path"))?
                .to_string();
            let preserve = obj
                .get("preserveNullAndEmptyArrays")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let index_field = obj
                .get("includeArrayIndex")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            (path, preserve, index_field)
        }
        _ => return Err(AggregateError::bad_value("$unwind 需要字符串或对象")),
    };
    let field = path
        .strip_prefix('$')
        .ok_or_else(|| AggregateError::bad_value("$unwind 路径必须以 $ 开头"))?
        .to_string();

    let mut out = Vec::new();
    for doc in docs {
        match resolve_field(doc, &field) {
            Some(Value::Array(items)) if !items.is_empty() => {
                for (index, item) in items.iter().enumerate() {
                    let mut copy = doc.clone();
                    set_dotted(&mut copy, &field, item.clone());
                    if let Some(name) = &index_field {
                        set_dotted(&mut copy, name, Value::from(index));
                    }
                    out.push(copy);
                }
            }
            // 非数组(含空数组 / 缺失 / null):按 MongoDB 语义丢弃或原样保留
            _ if preserve => {
                let mut copy = doc.clone();
                if let Some(name) = &index_field {
                    set_dotted(&mut copy, name, Value::Null);
                }
                out.push(copy);
            }
            _ => {}
        }
    }
    Ok(out)
}

/// 按点号路径取值(数组下钻不做展开,供 `$unwind`/`$lookup` 使用)。
fn resolve_field(doc: &Map<String, Value>, path: &str) -> Option<Value> {
    let mut current = Value::Object(doc.clone());
    for seg in path.split('.').filter(|s| !s.is_empty()) {
        current = match current {
            Value::Object(map) => map.get(seg)?.clone(),
            Value::Array(items) => {
                let index = seg.parse::<usize>().ok()?;
                items.get(index)?.clone()
            }
            _ => return None,
        };
    }
    Some(current)
}

// ==================== $lookup ====================

/// `$lookup`:`{from, localField, foreignField, as}`。
///
/// 实现为「先收集本地值 → 一次 `$in` 查询外部集合 → 按值分组」,
/// 比逐条查询少 N 次往返。子管道形式(`pipeline` + `let`)未实现。
fn lookup_stage(
    db: &Database,
    docs: &[Map<String, Value>],
    arg: &Value,
) -> Result<Vec<Map<String, Value>>, AggregateError> {
    let Some(obj) = arg.as_object() else {
        return Err(AggregateError::bad_value("$lookup 必须是对象"));
    };
    if obj.contains_key("pipeline") || obj.contains_key("let") {
        return Err(AggregateError::unsupported(
            "$lookup 的子管道形式(pipeline/let)尚未实现",
        ));
    }
    let from = obj
        .get("from")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AggregateError::bad_value("$lookup 缺少 from"))?;
    let local = obj
        .get("localField")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AggregateError::bad_value("$lookup 缺少 localField"))?;
    let foreign = obj
        .get("foreignField")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AggregateError::bad_value("$lookup 缺少 foreignField"))?;
    let alias = obj
        .get("as")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AggregateError::bad_value("$lookup 缺少 as"))?;

    // 1. 收集本地值(null 不参与匹配)
    let mut wanted: Vec<Value> = Vec::new();
    for doc in docs {
        collect_lookup_values(doc, local, &mut wanted);
    }
    wanted.retain(|v| !v.is_null());
    wanted.dedup_by(|a, b| compare_values(Some(a), Some(b)).is_eq());

    // 2. 一次查出外部文档(空集合也照常返回,不必特判)
    let mut matched: Vec<Map<String, Value>> = Vec::new();
    if !wanted.is_empty() {
        let mut cond = Map::new();
        cond.insert("$in".to_string(), Value::Array(wanted.clone()));
        let mut filter_json = Map::new();
        filter_json.insert(foreign.to_string(), Value::Object(cond));
        let filter = Filter::from_value(&Value::Object(filter_json))
            .map_err(|e| AggregateError::bad_value(e.to_string()))?;
        matched = db
            .find_with_options(from, &filter, &FindOptions::default())
            .map_err(|e| AggregateError::bad_value(e.to_string()))?;
    }

    // 3. 按外部值分组后挂到每篇文档上
    let mut out = Vec::with_capacity(docs.len());
    for doc in docs {
        let mut local_values: Vec<Value> = Vec::new();
        collect_lookup_values(doc, local, &mut local_values);
        let joined: Vec<Value> = matched
            .iter()
            .filter(|foreign_doc| {
                let Some(foreign_value) = resolve_field(foreign_doc, foreign) else {
                    return false;
                };
                local_values
                    .iter()
                    .any(|lv| compare_values(Some(lv), Some(&foreign_value)).is_eq())
            })
            .map(|foreign_doc| Value::Object(foreign_doc.clone()))
            .collect();
        let mut copy = doc.clone();
        set_dotted(&mut copy, alias, Value::Array(joined));
        out.push(copy);
    }
    Ok(out)
}

/// 收集 `$lookup` 的本地值(数组字段展开为多个候选)。
fn collect_lookup_values(doc: &Map<String, Value>, path: &str, out: &mut Vec<Value>) {
    match resolve_field(doc, path) {
        Some(Value::Array(items)) => out.extend(items),
        Some(v) => out.push(v),
        None => {}
    }
}

// ==================== $facet ====================

/// `$facet`:同一份输入并发跑多条子管道,各自结果放进同名字段。
fn facet_stage(
    db: &Database,
    docs: &[Map<String, Value>],
    arg: &Value,
) -> Result<Vec<Map<String, Value>>, AggregateError> {
    let Some(obj) = arg.as_object() else {
        return Err(AggregateError::bad_value("$facet 必须是对象"));
    };
    let mut out = Map::new();
    for (name, pipeline) in obj {
        let stages = pipeline
            .as_array()
            .ok_or_else(|| AggregateError::bad_value(format!("$facet.{} 必须是数组", name)))?;
        let result = run_stages(db, docs.to_vec(), stages)?;
        out.insert(
            name.clone(),
            Value::Array(result.into_iter().map(Value::Object).collect()),
        );
    }
    Ok(vec![out])
}

// ==================== $bucket ====================

/// `$bucket`:`{groupBy, boundaries, default, output}`。
///
/// 实现为「先按区间算出分组键,再套用与 `$group` 相同的累加器」。
fn bucket_stage(
    docs: &[Map<String, Value>],
    arg: &Value,
) -> Result<Vec<Map<String, Value>>, AggregateError> {
    let Some(obj) = arg.as_object() else {
        return Err(AggregateError::bad_value("$bucket 必须是对象"));
    };
    let group_by = obj
        .get("groupBy")
        .ok_or_else(|| AggregateError::bad_value("$bucket 缺少 groupBy"))?;
    let boundaries = obj
        .get("boundaries")
        .and_then(|v| v.as_array())
        .ok_or_else(|| AggregateError::bad_value("$bucket 缺少 boundaries"))?;
    if boundaries.len() < 2 {
        return Err(AggregateError::bad_value(
            "$bucket 的 boundaries 至少需要 2 个值",
        ));
    }
    let output = obj.get("output");
    let specs = match output.and_then(|v| v.as_object()) {
        Some(m) => parse_accumulators(m)?,
        None => vec![AccSpec {
            name: "count".to_string(),
            op: "$sum".to_string(),
            arg: json!(1),
        }],
    };

    // 先算出每篇文档的桶键(边界下界),再复用分组逻辑
    let mut order: Vec<String> = Vec::new();
    let mut groups: std::collections::HashMap<String, (Value, Vec<Acc>)> =
        std::collections::HashMap::new();
    for doc in docs {
        let value = eval(group_by, doc).map_err(|e| AggregateError::bad_value(e.to_string()))?;
        let key = bucket_key(&value, boundaries, obj.get("default"))?;
        let repr = serde_json::to_string(&key).unwrap_or_default();
        let entry = groups.entry(repr.clone()).or_insert_with(|| {
            order.push(repr.clone());
            (key.clone(), specs.iter().map(|s| s.initial()).collect())
        });
        for (spec, acc) in specs.iter().zip(entry.1.iter_mut()) {
            spec.feed(acc, doc)?;
        }
    }
    // 按桶边界顺序输出(与 MongoDB 一致)
    order.sort_by(|a, b| {
        let va = &groups[a].0;
        let vb = &groups[b].0;
        compare_values(Some(va), Some(vb))
    });

    let mut out = Vec::with_capacity(order.len());
    for repr in order {
        let (key, accs) = groups.remove(&repr).unwrap();
        let mut doc = Map::new();
        doc.insert("_id".to_string(), key);
        for (spec, acc) in specs.iter().zip(accs) {
            doc.insert(spec.name.clone(), spec.finish(acc));
        }
        out.push(doc);
    }
    Ok(out)
}

/// 算出值所属桶的下界;越界时用 `default`(未提供则报错)。
fn bucket_key(
    value: &Value,
    boundaries: &[Value],
    default: Option<&Value>,
) -> Result<Value, AggregateError> {
    for pair in boundaries.windows(2) {
        let (low, high) = (&pair[0], &pair[1]);
        if compare_values(Some(value), Some(low)).is_ge()
            && compare_values(Some(value), Some(high)).is_lt()
        {
            return Ok(low.clone());
        }
    }
    default
        .cloned()
        .ok_or_else(|| AggregateError::bad_value("$bucket 的值落在 boundaries 之外且未提供 default"))
}

// ==================== $replaceRoot ====================

/// `$replaceRoot` / `$replaceWith`:用表达式结果整体替换文档。
fn replace_root(
    docs: &[Map<String, Value>],
    arg: &Value,
    key: &str,
) -> Result<Vec<Map<String, Value>>, AggregateError> {
    let expr = if key.is_empty() {
        arg
    } else {
        arg.get(key)
            .ok_or_else(|| AggregateError::bad_value(format!("$replaceRoot 缺少 {}", key)))?
    };
    let mut out = Vec::with_capacity(docs.len());
    for doc in docs {
        let value = eval(expr, doc).map_err(|e| AggregateError::bad_value(e.to_string()))?;
        match value {
            Value::Object(map) => out.push(map),
            other => {
                return Err(AggregateError::bad_value(format!(
                    "$replaceRoot 的结果必须是对象,实际: {}",
                    other
                )));
            }
        }
    }
    Ok(out)
}
