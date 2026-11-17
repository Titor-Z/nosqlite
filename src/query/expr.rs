//! MongoDB 聚合表达式求值器(v0.7.0)。
//!
//! 一套实现同时服务三处,保证语义不分叉:
//!
//! - 过滤器的 `$expr`(`{"$expr": {"$gt": ["$a", "$b"]}}`);
//! - 聚合的 `$project` / `$addFields` / `$set` 的字段表达式;
//! - `$group` / `$bucket` 的 `_id` 与累加器参数。
//!
//! 旧版的 `$group` 只认 `"$path"` 字符串,`$project` 只做字段裁剪 —— 那是退化实现,
//! 一旦 `$expr` 也要用就会写出第二份。故抽出本模块。
//!
//! # 表达式形态
//!
//! | 写法 | 含义 |
//! |---|---|
//! | `"$a.b"` | 字段引用(点号路径,遇数组自动展开为数组) |
//! | `"$$ROOT"` / `"$$CURRENT"` | 整篇文档 |
//! | `{"$literal": x}` | 字面量(避免 `"$x"` 被当字段引用) |
//! | 其它对象 | 逐值求值后组成新对象 |
//! | 数组 | 逐元素求值 |
//! | 数字/布尔/null | 常量 |
//!
//! # 已实现的操作符
//!
//! 算术 `$add/$subtract/$multiply/$divide/$mod`;比较 `$eq/$ne/$gt/$gte/$lt/$lte/$cmp`;
//! 逻辑 `$and/$or/$not`;条件 `$cond/$ifNull`;集合 `$in/$size/$arrayElemAt/$isArray/$concatArrays`;
//! 字符串 `$concat/$toUpper/$toLower`。
//!
//! 未实现的操作符**明确报错**,不静默返回 null —— 否则管道会静默产出空字段。

use serde_json::{Map, Value};

/// 表达式求值错误。
#[derive(Debug, Clone)]
pub struct ExprError(pub String);

impl std::fmt::Display for ExprError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ExprError {}

type Result<T> = std::result::Result<T, ExprError>;

fn err<T>(message: impl Into<String>) -> Result<T> {
    Err(ExprError(message.into()))
}

/// 求值一条表达式。
pub fn eval(expr: &Value, doc: &Map<String, Value>) -> Result<Value> {
    match expr {
        Value::String(s) => Ok(string_ref(s, doc)),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(eval(item, doc)?);
            }
            Ok(Value::Array(out))
        }
        Value::Object(map) => {
            // 恰好一个 `$` 前缀键 → 操作符表达式;否则是普通对象(逐值求值)
            if map.len() == 1
                && let Some(key) = map.keys().next()
                && key.starts_with('$')
            {
                return eval_operator(key, map.values().next().unwrap(), doc);
            }
            let mut out = Map::new();
            for (k, v) in map {
                out.insert(k.clone(), eval(v, doc)?);
            }
            Ok(Value::Object(out))
        }
        other => Ok(other.clone()),
    }
}

/// 字符串:`"$path"` 是字段引用,`"$$ROOT"` 是整篇文档,其余是常量字符串。
fn string_ref(s: &str, doc: &Map<String, Value>) -> Value {
    let Some(rest) = s.strip_prefix('$') else {
        return Value::String(s.to_string());
    };
    if let Some(var) = rest.strip_prefix('$') {
        let (name, path) = match var.split_once('.') {
            Some((n, p)) => (n, Some(p)),
            None => (var, None),
        };
        if name == "ROOT" || name == "CURRENT" {
            return match path {
                Some(p) => path_value(doc, p).unwrap_or(Value::Null),
                None => Value::Object(doc.clone()),
            };
        }
        // 未定义的变量(如 $$NOW)显式报 null,不 panic
        return Value::Null;
    }
    path_value(doc, rest).unwrap_or(Value::Null)
}

/// 按点号路径取值;路径中途遇数组时逐元素展开并收集为数组。
fn path_value(doc: &Map<String, Value>, path: &str) -> Option<Value> {
    let segments: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
    let mut current = Value::Object(doc.clone());
    for seg in segments {
        current = step(current, seg)?;
    }
    Some(current)
}

fn step(value: Value, seg: &str) -> Option<Value> {
    match value {
        Value::Object(map) => map.get(seg).cloned(),
        Value::Array(items) => {
            // 数组上取字段 → 展开为数组;数字键则按下标取
            if let Ok(index) = seg.parse::<usize>() {
                return items.get(index).cloned();
            }
            let mut out = Vec::new();
            for item in items {
                if let Some(v) = step(item, seg) {
                    out.push(v);
                }
            }
            Some(Value::Array(out))
        }
        _ => None,
    }
}

/// 操作符表达式分派。
fn eval_operator(op: &str, arg: &Value, doc: &Map<String, Value>) -> Result<Value> {
    match op {
        "$literal" => Ok(arg.clone()),
        // ---------- 算术 ----------
        "$add" | "$multiply" => {
            let nums = eval_args(arg, doc)?;
            let mut acc = if op == "$add" { 0.0 } else { 1.0 };
            for v in &nums {
                let n = as_number(v).ok_or_else(|| {
                    ExprError(format!("{} 的参数必须是数字,实际: {}", op, v))
                })?;
                if op == "$add" {
                    acc += n;
                } else {
                    acc *= n;
                }
            }
            Ok(number_value(acc))
        }
        "$subtract" | "$divide" | "$mod" => {
            let nums = eval_args(arg, doc)?;
            if nums.len() != 2 {
                return err(format!("{} 需要恰好 2 个参数", op));
            }
            let a = as_number(&nums[0]).ok_or_else(|| ExprError(format!("{} 参数非数字", op)))?;
            let b = as_number(&nums[1]).ok_or_else(|| ExprError(format!("{} 参数非数字", op)))?;
            if b == 0.0 && op != "$subtract" {
                return err(format!("{} 的除数为 0", op));
            }
            let result = match op {
                "$subtract" => a - b,
                "$divide" => a / b,
                _ => a % b,
            };
            Ok(number_value(result))
        }
        // ---------- 比较 ----------
        "$eq" | "$ne" | "$gt" | "$gte" | "$lt" | "$lte" | "$cmp" => {
            let vals = eval_args(arg, doc)?;
            if vals.len() != 2 {
                return err(format!("{} 需要恰好 2 个参数", op));
            }
            let ord = crate::query::compare_values(Some(&vals[0]), Some(&vals[1]));
            Ok(match op {
                "$eq" => Value::Bool(ord.is_eq()),
                "$ne" => Value::Bool(!ord.is_eq()),
                "$gt" => Value::Bool(ord.is_gt()),
                "$gte" => Value::Bool(ord.is_ge()),
                "$lt" => Value::Bool(ord.is_lt()),
                "$lte" => Value::Bool(ord.is_le()),
                _ => Value::from(match ord {
                    std::cmp::Ordering::Less => -1,
                    std::cmp::Ordering::Equal => 0,
                    std::cmp::Ordering::Greater => 1,
                }),
            })
        }
        // ---------- 逻辑 ----------
        "$and" => {
            for v in eval_args(arg, doc)? {
                if !truthy(&v) {
                    return Ok(Value::Bool(false));
                }
            }
            Ok(Value::Bool(true))
        }
        "$or" => {
            for v in eval_args(arg, doc)? {
                if truthy(&v) {
                    return Ok(Value::Bool(true));
                }
            }
            Ok(Value::Bool(false))
        }
        "$not" => {
            let vals = eval_args(arg, doc)?;
            let first = vals.first().cloned().unwrap_or(Value::Null);
            Ok(Value::Bool(!truthy(&first)))
        }
        // ---------- 条件 ----------
        "$cond" => {
            let (cond, yes, no) = match arg {
                Value::Array(items) if items.len() == 3 => {
                    (eval(&items[0], doc)?, &items[1], &items[2])
                }
                Value::Object(map) => {
                    let get = |k: &str| {
                        map.get(k)
                            .ok_or_else(|| ExprError(format!("$cond 缺少 {}", k)))
                    };
                    (eval(get("if")?, doc)?, get("then")?, get("else")?)
                }
                _ => return err("$cond 需要 3 元素数组或 {if,then,else} 对象"),
            };
            eval(if truthy(&cond) { yes } else { no }, doc)
        }
        "$ifNull" => {
            let vals = eval_args(arg, doc)?;
            for (i, v) in vals.iter().enumerate() {
                if i + 1 == vals.len() || !v.is_null() {
                    return Ok(v.clone());
                }
            }
            Ok(Value::Null)
        }
        // ---------- 集合 ----------
        "$in" => {
            let vals = eval_args(arg, doc)?;
            if vals.len() != 2 {
                return err("$in 需要 [值, 数组] 两个参数");
            }
            let arr = vals[1]
                .as_array()
                .ok_or_else(|| ExprError("$in 的第二个参数必须是数组".to_string()))?;
            Ok(Value::Bool(arr.iter().any(|el| {
                crate::query::compare_values(Some(el), Some(&vals[0])).is_eq()
            })))
        }
        "$size" => {
            let vals = eval_args(arg, doc)?;
            let arr = vals
                .first()
                .and_then(|v| v.as_array())
                .ok_or_else(|| ExprError("$size 的参数必须是数组".to_string()))?;
            Ok(Value::from(arr.len()))
        }
        "$arrayElemAt" => {
            let vals = eval_args(arg, doc)?;
            if vals.len() != 2 {
                return err("$arrayElemAt 需要 [数组, 下标] 两个参数");
            }
            let arr = vals[0]
                .as_array()
                .ok_or_else(|| ExprError("$arrayElemAt 的第一个参数必须是数组".to_string()))?;
            let idx = as_number(&vals[1])
                .ok_or_else(|| ExprError("$arrayElemAt 的下标必须是数字".to_string()))?;
            let idx = if idx < 0.0 {
                arr.len() as i64 + idx as i64
            } else {
                idx as i64
            };
            Ok(usize::try_from(idx)
                .ok()
                .and_then(|i| arr.get(i).cloned())
                .unwrap_or(Value::Null))
        }
        "$isArray" => {
            let vals = eval_args(arg, doc)?;
            Ok(Value::Bool(vals.first().is_some_and(|v| v.is_array())))
        }
        "$concatArrays" => {
            let vals = eval_args(arg, doc)?;
            let mut out = Vec::new();
            for v in &vals {
                let arr = v.as_array().ok_or_else(|| {
                    ExprError("$concatArrays 的参数必须都是数组".to_string())
                })?;
                out.extend(arr.iter().cloned());
            }
            Ok(Value::Array(out))
        }
        // ---------- 字符串 ----------
        "$concat" => {
            let mut out = String::new();
            for v in eval_args(arg, doc)? {
                match v {
                    Value::String(s) => out.push_str(&s),
                    Value::Null => {}
                    other => out.push_str(&other.to_string()),
                }
            }
            Ok(Value::String(out))
        }
        "$toUpper" | "$toLower" => {
            let vals = eval_args(arg, doc)?;
            let s = vals
                .first()
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Ok(Value::String(if op == "$toUpper" {
                s.to_uppercase()
            } else {
                s.to_lowercase()
            }))
        }
        other => err(format!("聚合表达式尚未实现: {}", other)),
    }
}

/// 把「单参数」或「参数数组」统一求值为参数列表。
fn eval_args(arg: &Value, doc: &Map<String, Value>) -> Result<Vec<Value>> {
    match arg {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(eval(item, doc)?);
            }
            Ok(out)
        }
        single => Ok(vec![eval(single, doc)?]),
    }
}

/// MongoDB 的真值语义:只有 false / null / 0 为假(空数组、空字符串都为真)。
pub fn truthy(value: &Value) -> bool {
    match value {
        Value::Bool(b) => *b,
        Value::Null => false,
        Value::Number(n) => n.as_f64().unwrap_or(0.0) != 0.0,
        _ => true,
    }
}

/// 取数值(整数优先),非数值返回 `None`。
pub fn as_number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        // `$numberLong` 等字面量按数字参与运算
        Value::Object(_) => crate::query::as_number_long(value)
            .or_else(|| crate::query::as_number_double(value))
            .and_then(|n| n.as_f64()),
        _ => None,
    }
}

/// 浮点转值:整数值保持整数(与 `$inc` 等更新操作符一致)。
pub fn number_value(n: f64) -> Value {
    if n.fract() == 0.0 && n.abs() < 9.0e15 {
        Value::from(n as i64)
    } else {
        serde_json::Number::from_f64(n)
            .map(Value::Number)
            .unwrap_or(Value::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn doc() -> Map<String, Value> {
        json!({"a": 10, "b": 3, "name": "Ada", "tags": ["x", "y"], "sub": {"x": 1}})
            .as_object()
            .unwrap()
            .clone()
    }

    #[test]
    fn field_ref_and_literal() {
        assert_eq!(eval(&json!("$a"), &doc()).unwrap(), json!(10));
        assert_eq!(eval(&json!("$sub.x"), &doc()).unwrap(), json!(1));
        // `$literal` 防止常量被当字段引用
        assert_eq!(eval(&json!({"$literal": "$a"}), &doc()).unwrap(), json!("$a"));
    }

    #[test]
    fn array_path_expands() {
        assert_eq!(eval(&json!("$tags"), &doc()).unwrap(), json!(["x", "y"]));
        assert_eq!(eval(&json!("$tags.0"), &doc()).unwrap(), json!("x"));
    }

    #[test]
    fn arithmetic_keeps_integers() {
        assert_eq!(eval(&json!({"$add": ["$a", "$b"]}), &doc()).unwrap(), json!(13));
        assert_eq!(
            eval(&json!({"$divide": ["$a", "$b"]}), &doc()).unwrap(),
            json!(10.0 / 3.0)
        );
        assert!(eval(&json!({"$divide": ["$a", 0]}), &doc()).is_err());
    }

    #[test]
    fn comparison_uses_bson_order() {
        assert_eq!(eval(&json!({"$gt": ["$a", "$b"]}), &doc()).unwrap(), json!(true));
        // 跨类型:数字(2) < 字符串(3)
        assert_eq!(eval(&json!({"$lt": ["$a", "z"]}), &doc()).unwrap(), json!(true));
    }

    #[test]
    fn cond_and_ifnull() {
        assert_eq!(
            eval(&json!({"$cond": [{"$gt": ["$a", 5]}, "big", "small"]}), &doc()).unwrap(),
            json!("big")
        );
        assert_eq!(
            eval(&json!({"$ifNull": ["$missing", "fallback"]}), &doc()).unwrap(),
            json!("fallback")
        );
    }

    #[test]
    fn truthiness_matches_mongo() {
        // 空数组、空字符串在 MongoDB 里都为真
        assert!(truthy(&json!([])));
        assert!(truthy(&json!("")));
        assert!(!truthy(&json!(0)));
        assert!(!truthy(&Value::Null));
    }

    #[test]
    fn unknown_operator_errors() {
        assert!(eval(&json!({"$where": "1"}), &doc()).is_err());
    }
}
