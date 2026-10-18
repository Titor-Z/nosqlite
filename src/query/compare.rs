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
//! - object 按键名有序逐对比较(同键比值,键数少者在前);
//! - **date**(`{"$date": ...}`,MongoDB Extended JSON)按 UTC 毫秒比较,
//!   时区偏移与精度差异在解析阶段即归一。
//!
//! v0.1.1 起 array/object 可参与排序与 `$eq`,不再是「恒等」;
//! v0.1.3 起支持 Date 类型;v0.2.0 起支持 Extended JSON 的 `$oid`/`$numberLong`/`$numberDouble`。
//!
//! # Extended JSON 归一化
//!
//! 为了让 MongoDB driver 与 C ABI 客户端能用各自的惯用写法互通,
//! 下列字面量在**比较与匹配时**按其「真实类型」参与:
//!
//! - `{"$oid": "<24位hex>"}` ≡ 普通字符串(主键就是 24 位 hex 字符串);
//! - `{"$numberLong": "123"}` ≡ 整数;`{"$numberDouble": "1.5"}` ≡ 浮点;
//! - `{"$date": ...}` ≡ 时间戳(见 [`as_date`])。
//!
//! 存储层不做改写 —— 写入什么就存什么,只在比较时归一,保证已有数据不受影响。

use serde_json::Value;

/// 日期在类型序中的位置:介于 number 与 string 之间。
const RANK_DATE: u8 = 4;

/// 若值是 Extended JSON 的 `{"$oid": "..."}`,返回其中的字符串。
///
/// `_id` 在 nosqlite 中就是 24 位 hex 字符串,因此 `$oid` 与普通字符串等价。
pub fn as_oid(value: &Value) -> Option<&str> {
    wrap_str(value, "$oid")
}

/// 若值是 `{"$numberLong": "123"}`,返回其数值(整数优先)。
pub fn as_number_long(value: &Value) -> Option<Value> {
    scalar_wrap(value, "$numberLong")
        .and_then(|v| parse_numeric(v.as_str()?, true))
        .map(Value::from)
}

/// 若值是 `{"$numberDouble": "1.5"}`,返回其数值。
pub fn as_number_double(value: &Value) -> Option<Value> {
    scalar_wrap(value, "$numberDouble")
        .and_then(|v| parse_numeric(v.as_str()?, false))
        .map(Value::from)
}

/// 取出「恰好只有一个指定键」的包装对象内的值。
fn scalar_wrap<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    let obj = value.as_object()?;
    if obj.len() != 1 {
        return None;
    }
    obj.get(key)
}

/// 取出 `{"$key": "字符串"}` 中的字符串(非字符串值不算)。
fn wrap_str<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    scalar_wrap(value, key)?.as_str()
}

/// 解析数字字符串:整数优先,浮点按需。
fn parse_numeric(raw: &str, integer_only: bool) -> Option<serde_json::Number> {
    if let Ok(i) = raw.parse::<i64>() {
        return Some(serde_json::Number::from(i));
    }
    if integer_only {
        return None;
    }
    raw.parse::<f64>()
        .ok()
        .and_then(serde_json::Number::from_f64)
}

/// 把 Extended JSON 包装归一为基础值;普通的字符串/数字原样返回。
///
/// 返回值是「该值参与比较时的真实形态」。
fn normalize(value: &Value) -> std::borrow::Cow<'_, Value> {
    if let Some(oid) = as_oid(value) {
        return std::borrow::Cow::Owned(Value::String(oid.to_string()));
    }
    if let Some(n) = as_number_long(value) {
        return std::borrow::Cow::Owned(n);
    }
    if let Some(n) = as_number_double(value) {
        return std::borrow::Cow::Owned(n);
    }
    std::borrow::Cow::Borrowed(value)
}

/// 若值是 Date 字面量(`{"$date": ...}` 且只有一个键),解析为 UTC 毫秒。
///
/// 值无法解析时返回 `None` —— 此时该对象按**普通对象**参与比较,
/// 保证既有文档与误写格式不受影响。
pub fn as_date(value: &Value) -> Option<i64> {
    let obj = value.as_object()?;
    // 必须恰好只有一个 `$date` 键,避免与普通业务对象混淆
    if obj.len() != 1 {
        return None;
    }
    parse_date_value(obj.get("$date")?)
}

/// 解析 `$date` 的取值:毫秒时间戳(数字)或 ISO-8601 字符串。
fn parse_date_value(value: &Value) -> Option<i64> {
    match value {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => parse_iso8601(s),
        _ => None,
    }
}

/// 解析 ISO-8601 日期时间字符串为 UTC 毫秒时间戳。
///
/// 支持:`YYYY-MM-DD`、`YYYY-MM-DDTHH:MM`、`YYYY-MM-DDTHH:MM:SS`、
/// 可选小数秒,以及可选时区 `Z` / `±HH:MM` / `±HHMM`(缺省按 UTC)。
pub fn parse_iso8601(s: &str) -> Option<i64> {
    let bytes = s.as_bytes();
    let mut pos = 0usize;

    // 逐段读取定长数字
    let read_num = |pos: &mut usize, len: usize| -> Option<i64> {
        if *pos + len > bytes.len() {
            return None;
        }
        let mut n: i64 = 0;
        for _ in 0..len {
            let b = bytes[*pos];
            if !b.is_ascii_digit() {
                return None;
            }
            n = n * 10 + (b - b'0') as i64;
            *pos += 1;
        }
        Some(n)
    };

    let year = read_num(&mut pos, 4)?;
    if bytes.get(pos) != Some(&b'-') {
        return None;
    }
    pos += 1;
    let month = read_num(&mut pos, 2)?;
    if bytes.get(pos) != Some(&b'-') {
        return None;
    }
    pos += 1;
    let day = read_num(&mut pos, 2)?;

    // 时间部分可选;日期与时间之间允许 `T` 或空格
    let (mut hour, mut minute, mut second, mut millis) = (0i64, 0i64, 0i64, 0i64);
    if matches!(bytes.get(pos), Some(b'T') | Some(b' ')) {
        pos += 1;
        hour = read_num(&mut pos, 2)?;
        if bytes.get(pos) != Some(&b':') {
            return None;
        }
        pos += 1;
        minute = read_num(&mut pos, 2)?;
        if bytes.get(pos) == Some(&b':') {
            pos += 1;
            second = read_num(&mut pos, 2)?;
            // 小数秒
            if bytes.get(pos) == Some(&b'.') {
                pos += 1;
                let start = pos;
                let mut frac: i64 = 0;
                let mut digits = 0usize;
                while pos < bytes.len() && bytes[pos].is_ascii_digit() {
                    if digits < 3 {
                        frac = frac * 10 + (bytes[pos] - b'0') as i64;
                    }
                    digits += 1;
                    pos += 1;
                }
                if digits == 0 {
                    return None;
                }
                // 补足到毫秒
                for _ in digits..3 {
                    frac *= 10;
                }
                millis = frac;
                let _ = start;
            }
        }
    }

    // 时区偏移
    let mut offset_minutes = 0i64;
    match bytes.get(pos) {
        Some(b'Z') | Some(b'z') => pos += 1,
        Some(b'+') | Some(b'-') => {
            let sign = if bytes[pos] == b'-' { -1 } else { 1 };
            pos += 1;
            let h = read_num(&mut pos, 2)?;
            // 允许 ±HH:MM 或 ±HHMM
            if bytes.get(pos) == Some(&b':') {
                pos += 1;
            }
            let m = if pos < bytes.len() {
                read_num(&mut pos, 2)?
            } else {
                0
            };
            offset_minutes = sign * (h * 60 + m);
        }
        _ => {}
    }
    // 尾部不得有多余字符
    if pos != bytes.len() {
        return None;
    }
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    if hour > 23 || minute > 59 || second > 60 {
        return None;
    }

    // 转为「自 1970-01-01 起的天数」(days-from-civil 算法),再合成毫秒
    let days = days_from_civil(year, month, day);
    let ms = ((days * 24 + hour) * 60 + minute) * 60_000 + second * 1000 + millis;
    Some(ms - offset_minutes * 60_000)
}

/// Howard Hinnant 的 days_from_civil:公历日期 → 自 1970-01-01 起的天数。
fn days_from_civil(mut y: i64, m: i64, d: i64) -> i64 {
    if m <= 2 {
        y -= 1;
    }
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = (m + 9) % 12; // March = 0
    let doy = (153 * mp + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

/// BSON 风格类型等级(用于确定类型间大小)。
///
/// 先做 Extended JSON 归一(`$oid`→字符串、`$numberLong`→数字),再判等级。
fn type_rank(value: Option<&Value>) -> u8 {
    match value {
        None => 0, // 缺失字段视为最小
        Some(v) => {
            let v = normalize(v);
            match v.as_ref() {
                Value::Null => 1,
                Value::Bool(_) => 2,
                Value::Number(_) => 3,
                _ if as_date(&v).is_some() => RANK_DATE,
                Value::String(_) => 5,
                Value::Array(_) => 6,
                Value::Object(_) => 7,
            }
        }
    }
}

/// 比较两个值(可能为缺失,即 `None`)。
///
/// 遵循 BSON 风格类型序;同类型内部按各自规则比较。
/// array/object 递归比较(BSON 风格子集),因此可以参与排序与相等判断。
pub fn compare_values(a: Option<&Value>, b: Option<&Value>) -> std::cmp::Ordering {
    // 先做 Extended JSON 归一($oid→字符串、$numberLong/$numberDouble→数字)
    let na = a.map(normalize);
    let nb = b.map(normalize);
    let a = na.as_deref();
    let b = nb.as_deref();
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
            // 其余同等级情况:date 或 object
            (Some(x), Some(y)) => {
                // date:按 UTC 毫秒比较(时区偏移已在解析阶段归一)
                if let (Some(dx), Some(dy)) = (as_date(x), as_date(y)) {
                    return dx.cmp(&dy);
                }
                compare_objects(x, y)
            }
            _ => std::cmp::Ordering::Equal,
        },
        other => other,
    }
}

/// 按 BSON 风格比较两个对象:键名有序逐对比较,键数少者在前。
///
/// 非对象(或单边不可比)一律视为相等,保持排序稳定。
fn compare_objects(x: &Value, y: &Value) -> std::cmp::Ordering {
    let (Value::Object(x), Value::Object(y)) = (x, y) else {
        return std::cmp::Ordering::Equal;
    };
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
