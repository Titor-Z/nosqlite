//! 聚合管道执行(服务端 wire 与 C ABI 共用)。
//!
//! 只支持已实现的阶段:`$match/$sort/$skip/$limit/$group/$project/$addFields/$set`。
//! 未实现的阶段返回 `40324`(与 MongoDB 的 Location40324 对齐),不静默忽略。

use serde_json::{Map, Value, json};

use crate::database::Database;
use crate::query::{Filter, FindOptions, SortSpec};

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

/// 执行聚合管道,返回输出文档。
pub fn run(
    db: &Database,
    collection: &str,
    pipeline: &Value,
) -> Result<Vec<Map<String, Value>>, AggregateError> {
    let stages = pipeline
        .as_array()
        .ok_or_else(|| AggregateError::bad_value("pipeline 必须是数组"))?;

    let mut filter = Filter::match_all();
    let mut sort: Option<SortSpec> = None;
    let mut skip = 0usize;
    let mut limit: Option<usize> = None;
    let mut group_stage: Option<Value> = None;
    let mut project_stage: Option<Value> = None;

    for stage in stages {
        let Some(obj) = stage.as_object() else {
            return Err(AggregateError::bad_value("每个 stage 必须是对象"));
        };
        let Some((name, arg)) = obj.iter().next() else {
            return Err(AggregateError::bad_value("stage 不能为空对象"));
        };
        match name.as_str() {
            "$match" => {
                filter = Filter::from_value(arg).map_err(|e| AggregateError::bad_value(e.to_string()))?;
            }
            "$sort" => {
                sort = Some(SortSpec::from_value(arg).map_err(|e| AggregateError::bad_value(e.to_string()))?);
            }
            "$skip" => skip = arg.as_u64().unwrap_or(0) as usize,
            "$limit" => limit = arg.as_u64().map(|n| n as usize),
            "$group" => group_stage = Some(arg.clone()),
            "$project" | "$addFields" | "$set" => project_stage = Some(arg.clone()),
            other => {
                return Err(AggregateError::unsupported(format!(
                    "aggregate 阶段尚未实现: {}",
                    other
                )));
            }
        }
    }

    if let Some(group) = &group_stage {
        return group_docs(db, collection, &filter, group);
    }

    let docs = if let Some(sort) = &sort {
        db.find_page_total(collection, &filter, Some(sort), skip, limit, None)
            .map(|(_, docs)| docs)
            .map_err(|e| AggregateError::bad_value(e.to_string()))?
    } else {
        db.find_with_options(
            collection,
            &filter,
            &FindOptions {
                sort: None,
                projection: None,
                skip,
                limit,
            },
        )
        .map_err(|e| AggregateError::bad_value(e.to_string()))?
    };

    // 简单的 $project / $addFields:按字段包含裁剪
    match project_stage {
        Some(projection) => Ok(docs
            .iter()
            .map(|doc| project_doc(doc, &projection))
            .collect()),
        None => Ok(docs),
    }
}

fn project_doc(doc: &Map<String, Value>, projection: &Value) -> Map<String, Value> {
    let Some(obj) = projection.as_object() else {
        return doc.clone();
    };
    let mut out = Map::new();
    for key in obj.keys() {
        if key == "_id" {
            continue;
        }
        if let Some(value) = doc.get(key) {
            out.insert(key.clone(), value.clone());
        }
    }
    if let Some(id) = doc.get("_id") {
        out.insert("_id".to_string(), id.clone());
    }
    out
}

/// `$group`:`_id` 可以是常量或 `"$path"`;累加器仅 `$sum` / `$count`。
fn group_docs(
    db: &Database,
    collection: &str,
    filter: &Filter,
    group: &Value,
) -> Result<Vec<Map<String, Value>>, AggregateError> {
    let Some(obj) = group.as_object() else {
        return Err(AggregateError::unsupported("$group 必须是对象"));
    };
    let Some(group_id) = obj.get("_id") else {
        return Err(AggregateError::unsupported("$group 缺少 _id"));
    };

    let mut accumulators: Vec<(String, String, Value)> = Vec::new();
    for (key, value) in obj {
        if key == "_id" {
            continue;
        }
        let Some(spec) = value.as_object() else {
            return Err(AggregateError::unsupported("不支持的 $group 累加器"));
        };
        let Some((acc, arg)) = spec.iter().next() else {
            return Err(AggregateError::unsupported("空的 $group 累加器"));
        };
        match acc.as_str() {
            "$sum" | "$count" => accumulators.push((key.clone(), acc.clone(), arg.clone())),
            other => {
                return Err(AggregateError::unsupported(format!(
                    "$group 累加器尚未实现: {}",
                    other
                )));
            }
        }
    }
    if accumulators.is_empty() {
        accumulators.push(("n".to_string(), "$sum".to_string(), json!(1)));
    }

    let docs =
        db.find_with_options(collection, filter, &FindOptions::default())
            .map_err(|e| AggregateError::bad_value(e.to_string()))?;

    let mut order: Vec<String> = Vec::new();
    let mut groups: std::collections::HashMap<String, (Value, Vec<f64>)> =
        std::collections::HashMap::new();

    for doc in &docs {
        let key_value = eval_group_id(group_id, doc);
        let key_repr = serde_json::to_string(&key_value).unwrap_or_default();
        let entry = groups.entry(key_repr.clone()).or_insert_with(|| {
            order.push(key_repr.clone());
            (key_value.clone(), vec![0.0; accumulators.len()])
        });
        for (i, (_, acc, arg)) in accumulators.iter().enumerate() {
            let add = match acc.as_str() {
                "$count" => 1.0,
                "$sum" => match arg {
                    Value::Number(_) | Value::Bool(_) | Value::Null => arg.as_f64().unwrap_or(0.0),
                    Value::String(path) if path.starts_with('$') => {
                        doc.get(&path[1..]).and_then(|v| v.as_f64()).unwrap_or(0.0)
                    }
                    _ => 0.0,
                },
                _ => 0.0,
            };
            entry.1[i] += add;
        }
    }

    let mut out = Vec::new();
    for repr in &order {
        let (key_value, sums) = &groups[repr];
        let mut doc = Map::new();
        doc.insert("_id".to_string(), key_value.clone());
        for (i, (name, _, _)) in accumulators.iter().enumerate() {
            let v = sums[i];
            let num = if v.fract() == 0.0 && v.abs() < i64::MAX as f64 {
                json!(v as i64)
            } else {
                json!(v)
            };
            doc.insert(name.clone(), num);
        }
        out.push(doc);
    }
    Ok(out)
}

fn eval_group_id(group_id: &Value, doc: &Map<String, Value>) -> Value {
    match group_id {
        Value::String(path) if path.starts_with('$') => {
            doc.get(&path[1..]).cloned().unwrap_or(Value::Null)
        }
        other => other.clone(),
    }
}
