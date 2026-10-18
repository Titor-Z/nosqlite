//! 值比较工具(M3)。
//!
//! 提供 MongoDB BSON 风格的类型序比较,用于 filter 操作符与 sort 排序:
//!
//! 缺失(missing) < null < bool < number < string < array < object
//!
//! - number 内部按数值比较(int/float 可互比);
//! - string 按字典序;
//! - bool 按 `false < true`;
//! - array 逐元素比较(先比公共长度部分,再比长度,短者在前);
//! - object 按键名有序逐对比较(同键比值,键数少者在前)。
//!
//! 0.2.0 起 array/object 可参与排序与 `$eq`,不再是「恒等」。

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
/// array/object 递归比较(BSON 风格子集),因此可以参与排序与相等判断。
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
            // array:逐元素比较,公共部分相等时短者在前
            (Some(Value::Array(x)), Some(Value::Array(y))) => {
                for (xi, yi) in x.iter().zip(y.iter()) {
                    let ord = compare_values(Some(xi), Some(yi));
                    if ord != std::cmp::Ordering::Equal {
                        return ord;
                    }
                }
                x.len().cmp(&y.len())
            }
            // object:按键名有序逐对比较(serde_json::Map 的迭代器已按 key 有序)
            (Some(Value::Object(x)), Some(Value::Object(y))) => {
                for ((kx, vx), (ky, vy)) in x.iter().zip(y.iter()) {
                    let ord = kx.cmp(ky);
                    if ord != std::cmp::Ordering::Equal {
                        return ord;
                    }
                    let ord = compare_values(Some(vx), Some(vy));
                    if ord != std::cmp::Ordering::Equal {
                        return ord;
                    }
                }
                x.len().cmp(&y.len())
            }
            // 同类型序但组合不可能(防御):视为相等
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
