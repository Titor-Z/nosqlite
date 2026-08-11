//! 值比较工具(M3)。
//!
//! 提供 MongoDB BSON 风格的类型序比较,用于 filter 操作符与 sort 排序:
//!
//! 缺失(missing) < null < bool < number < string < array < object
//!
//! - number 内部按数值比较(int/float 可互比);
//! - string 按字典序;
//! - bool 按 `false < true`;
//! - array / object 不支持跨类型排序,比较时视为相等(排序稳定即可)。

use serde_json::Value;

/// BSON 风格类型等级(用于确定类型间大小)。
fn type_rank(value: Option<&Value>) -> u8 {
    match value {
        None => 0, // 缺失字段视为最小
        Some(Value::Null) => 1,
        Some(Value::Bool(_)) => 2,
        Some(Value::Number(_)) => 3,
        Some(Value::String(_)) => 4,
        Some(Value::Array(_)) => 5,
        Some(Value::Object(_)) => 6,
    }
}

/// 比较两个值(可能为缺失,即 `None`)。
///
/// 遵循 BSON 风格类型序;同类型内部按各自规则比较。
/// 对于 array/object(以及缺失 vs 对象等复杂情况),仅保证"可比较"但不保证完全有序:
/// 返回 `Equal` 表示大小关系无法判定,排序时保持相对位置。
pub fn compare_values(a: Option<&Value>, b: Option<&Value>) -> std::cmp::Ordering {
    let ra = type_rank(a);
    let rb = type_rank(b);
    match ra.cmp(&rb) {
        std::cmp::Ordering::Equal => match (a, b) {
            // 缺失 vs 缺失:相等
            (None, None) => std::cmp::Ordering::Equal,
            // null
            (Some(Value::Null), Some(Value::Null)) => std::cmp::Ordering::Equal,
            // bool
            (Some(Value::Bool(x)), Some(Value::Bool(y))) => x.cmp(y),
            // number:统一转 f64 数值比较(忽略 int/float 表示差异)
            (Some(Value::Number(x)), Some(Value::Number(y))) => {
                let fx = x.as_f64().unwrap_or(f64::NAN);
                let fy = y.as_f64().unwrap_or(f64::NAN);
                fx.partial_cmp(&fy).unwrap_or(std::cmp::Ordering::Equal)
            }
            // string:字典序
            (Some(Value::String(x)), Some(Value::String(y))) => x.cmp(y),
            // array / object:不能精确比较,视为相等(保持稳定排序)
            _ => std::cmp::Ordering::Equal,
        },
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 类型序:缺失 < null < bool < number < string。
    #[test]
    fn type_order() {
        assert_eq!(
            compare_values(None, Some(&Value::Null)),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            compare_values(Some(&Value::Null), Some(&json!(false))),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            compare_values(Some(&json!(false)), Some(&json!(0))),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            compare_values(Some(&json!(1)), Some(&json!("a"))),
            std::cmp::Ordering::Less
        );
    }

    /// number 内部数值比较:int/float 可互比,忽略表示差异。
    #[test]
    fn number_comparison() {
        assert_eq!(
            compare_values(Some(&json!(1)), Some(&json!(1.0))),
            std::cmp::Ordering::Equal
        );
        assert_eq!(
            compare_values(Some(&json!(2.5)), Some(&json!(2))),
            std::cmp::Ordering::Greater
        );
        assert_eq!(
            compare_values(Some(&json!(-1)), Some(&json!(0))),
            std::cmp::Ordering::Less
        );
    }

    /// string 按字典序。
    #[test]
    fn string_comparison() {
        assert_eq!(
            compare_values(Some(&json!("b")), Some(&json!("a"))),
            std::cmp::Ordering::Greater
        );
        assert_eq!(
            compare_values(Some(&json!("abc")), Some(&json!("abc"))),
            std::cmp::Ordering::Equal
        );
    }

    /// bool:`false < true`。
    #[test]
    fn bool_comparison() {
        assert_eq!(
            compare_values(Some(&json!(false)), Some(&json!(true))),
            std::cmp::Ordering::Less
        );
    }
}
