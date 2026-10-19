//! 更新操作模块(M4)。
//!
//! 仿 MongoDB 的更新操作符。当前支持 `$set`/`$unset`(M10):
//!
//! - `$set`:字段点号路径赋值,嵌套自动创建/覆盖;
//! - `$unset`:字段点号路径删除 —— 对象字段直接移除;数组元素按 MongoDB 语义置为
//!   `null`(占位保留);路径不存在时静默无操作(对齐 MongoDB);
//!
//! 点号路径的每一段既可以是对象字段名,也可以是**数组下标**(M9 修复):
//! - 路径中间是对象 → 按字段名下钻;字段不存在时自动创建对象;
//! - 路径中间是数组 → 当前段必须是合法且未越界的下标,否则报错;
//! - 路径中间是标量 → 报错,不会静默覆盖数组/标量(M9 前会用对象覆盖数组,属数据破坏缺陷)。
//!
//! v0.1.4 新增字段操作符:`$inc`/`$mul`/`$min`/`$max`/`$rename`/`$push`/`$addToSet`/`$currentDate`。
//!
//! 数组操作符(`$push`/`$addToSet`)支持 `$each` / `$position` / `$slice` / `$sort` 修饰符。

use serde_json::{Map, Value};

use super::compare::compare_values;

/// 更新操作错误。
#[derive(Debug)]
pub enum UpdateError {
    /// update 顶层必须是 JSON 对象,且只能包含已知操作符。
    InvalidUpdate(String),
    /// `$set` 的值必须是 JSON 对象。
    SetNotObject,
    /// `$unset` 的值必须是 JSON 对象。
    UnsetNotObject,
    /// 数组下标非法(该路径段不是纯数字)。
    InvalidArrayIndex(String),
    /// 数组下标越界。
    ArrayIndexOutOfBounds {
        /// 请求访问的下标。
        index: usize,
        /// 数组实际长度。
        len: usize,
    },
    /// 路径中间段是标量值(非对象也非数组),无法继续下钻创建字段。
    ScalarIntermediate {
        /// 出错的剩余路径(点号分隔)。
        path: String,
    },
    /// 操作符作用的字段类型不对(如对非数值 `$inc`、对非数组 `$push`)。
    TypeMismatch {
        /// 操作符名。
        op: String,
        /// 字段路径。
        path: String,
        /// 期望的类型描述。
        expected: String,
    },
    /// 数组操作符的修饰符不合法。
    InvalidModifier(String),
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateError::InvalidUpdate(msg) => write!(f, "更新操作使用错误: {}", msg),
            UpdateError::SetNotObject => write!(f, "$set 的值必须是 JSON 对象"),
            UpdateError::UnsetNotObject => write!(f, "$unset 的值必须是 JSON 对象"),
            UpdateError::InvalidArrayIndex(seg) => {
                write!(f, "数组下标非法: 路径段 `{}` 必须是数字", seg)
            }
            UpdateError::ArrayIndexOutOfBounds { index, len } => {
                write!(f, "数组下标越界: 索引 {} 超出数组长度 {}", index, len)
            }
            UpdateError::ScalarIntermediate { path } => write!(
                f,
                "无法在标量值上创建字段: 路径 `{}` 的中间段不是对象或数组",
                path
            ),
            UpdateError::TypeMismatch { op, path, expected } => {
                write!(f, "{} 作用于字段 `{}` 失败: 需要{}", op, path, expected)
            }
            UpdateError::InvalidModifier(msg) => write!(f, "数组操作符修饰符错误: {}", msg),
        }
    }
}

impl std::error::Error for UpdateError {}

/// `$push` 的解析结果。
#[derive(Debug)]
struct PushSpec {
    /// 字段路径。
    path: Vec<String>,
    /// 待追加的元素(单个值或 `$each` 展开后的列表)。
    values: Vec<Value>,
    /// 插入位置(`$position`)。
    position: Option<usize>,
    /// 截取(`$slice`):正数保留前 N,负数保留后 N,0 清空。
    slice: Option<i64>,
    /// 排序(`$sort`):按字段排序或整体排序。
    sort: Option<Value>,
}

/// `$pull` 的解析结果(v0.6.1)。
#[derive(Debug)]
struct PullSpec {
    /// 字段路径。
    path: Vec<String>,
    /// 原字段名(构造元素探针用,`matcher` 的条件以它为键)。
    field: String,
    /// 匹配条件:带 `$` 键的对象按查询操作符解释,否则按等值。
    matcher: crate::query::filter::Condition,
}

/// `$pop` 的解析结果(v0.6.1)。
#[derive(Debug)]
struct PopSpec {
    /// 字段路径。
    path: Vec<String>,
    /// `1` 移除最后一个,`-1` 移除第一个。
    last: bool,
}

/// `$addToSet` 的解析结果。
#[derive(Debug)]
struct AddToSetSpec {
    /// 字段路径。
    path: Vec<String>,
    /// 待加入的元素(单个值或 `$each` 展开后的列表)。
    values: Vec<Value>,
}

/// 一组更新操作。
///
/// 支持 `$set`/`$unset`(M4/M10)与 v0.1.4 新增的
/// `$inc`/`$mul`/`$min`/`$max`/`$rename`/`$push`/`$addToSet`/`$currentDate`。
#[derive(Debug)]
pub struct Update {
    /// `$set` 的字段赋值列表(字段路径 → 新值)。
    set_fields: Vec<(Vec<String>, Value)>,
    /// `$unset` 的字段路径列表(按声明顺序依次删除)。
    unset_fields: Vec<Vec<String>>,
    /// `$inc` 的字段增减列表。
    inc_fields: Vec<(Vec<String>, f64)>,
    /// `$mul` 的字段乘法列表。
    mul_fields: Vec<(Vec<String>, f64)>,
    /// `$min` 的字段列表。
    min_fields: Vec<(Vec<String>, Value)>,
    /// `$max` 的字段列表。
    max_fields: Vec<(Vec<String>, Value)>,
    /// `$rename` 的「旧路径 → 新路径」列表。
    rename_fields: Vec<(Vec<String>, Vec<String>)>,
    /// `$push` 列表。
    push_fields: Vec<PushSpec>,
    /// `$addToSet` 列表。
    add_to_set_fields: Vec<AddToSetSpec>,
    /// `$currentDate` 的字段路径列表。
    current_date_fields: Vec<Vec<String>>,
    /// `$pull` 列表(v0.6.1)。
    pull_fields: Vec<PullSpec>,
    /// `$pop` 列表(v0.6.1)。
    pop_fields: Vec<PopSpec>,
}

/// 已知的更新操作符。
const KNOWN_OPS: [&str; 12] = [
    "$set",
    "$unset",
    "$inc",
    "$mul",
    "$min",
    "$max",
    "$rename",
    "$push",
    "$addToSet",
    "$currentDate",
    "$pull",
    "$pop",
];

/// 把点号路径字符串拆成路径段,并校验不含空段。
fn parse_path(key: &str) -> Result<Vec<String>, UpdateError> {
    let path = key.split('.').map(|s| s.to_string()).collect::<Vec<_>>();
    if path.iter().any(|seg| seg.is_empty()) {
        return Err(UpdateError::InvalidUpdate(format!(
            "字段路径非法: `{}` 含空的路径段",
            key
        )));
    }
    Ok(path)
}

/// 取出某操作符的对象值(不是对象则报错)。
fn op_object<'a>(
    map: &'a Map<String, Value>,
    op: &str,
    not_object: UpdateError,
) -> Result<Option<&'a Map<String, Value>>, UpdateError> {
    match map.get(op) {
        Some(Value::Object(m)) => Ok(Some(m)),
        Some(_) => Err(not_object),
        None => Ok(None),
    }
}

impl Update {
    /// 从 JSON 值解析更新操作。
    ///
    /// 示例:`{"$set": {"name": "李四"}, "$inc": {"views": 1}}`。
    ///
    /// 空更新(`{}` 或无操作符的 `{"$set": {}}`)一律报错,不静默当作无操作。
    pub fn from_value(value: &Value) -> Result<Self, UpdateError> {
        let map = value
            .as_object()
            .ok_or_else(|| UpdateError::InvalidUpdate("update 顶层必须是 JSON 对象".to_string()))?;

        // 空对象:没有任何可执行的更新操作
        if map.is_empty() {
            return Err(UpdateError::InvalidUpdate(
                "update 不能为空对象,至少需要包含一个操作符".to_string(),
            ));
        }

        // 只允许已知操作符
        for key in map.keys() {
            if !KNOWN_OPS.contains(&key.as_str()) {
                return Err(UpdateError::InvalidUpdate(format!(
                    "暂不支持的更新操作符: {}",
                    key
                )));
            }
        }

        let set_map = op_object(map, "$set", UpdateError::SetNotObject)?;
        let unset_map = op_object(map, "$unset", UpdateError::UnsetNotObject)?;
        let inc_map = op_object(
            map,
            "$inc",
            UpdateError::InvalidUpdate("$inc 的值必须是 JSON 对象".to_string()),
        )?;
        let mul_map = op_object(
            map,
            "$mul",
            UpdateError::InvalidUpdate("$mul 的值必须是 JSON 对象".to_string()),
        )?;
        let min_map = op_object(
            map,
            "$min",
            UpdateError::InvalidUpdate("$min 的值必须是 JSON 对象".to_string()),
        )?;
        let max_map = op_object(
            map,
            "$max",
            UpdateError::InvalidUpdate("$max 的值必须是 JSON 对象".to_string()),
        )?;
        let rename_map = op_object(
            map,
            "$rename",
            UpdateError::InvalidUpdate("$rename 的值必须是 JSON 对象".to_string()),
        )?;
        let push_map = op_object(
            map,
            "$push",
            UpdateError::InvalidUpdate("$push 的值必须是 JSON 对象".to_string()),
        )?;
        let add_to_set_map = op_object(
            map,
            "$addToSet",
            UpdateError::InvalidUpdate("$addToSet 的值必须是 JSON 对象".to_string()),
        )?;
        let current_date_map = op_object(
            map,
            "$currentDate",
            UpdateError::InvalidUpdate("$currentDate 的值必须是 JSON 对象".to_string()),
        )?;
        let pull_map = op_object(
            map,
            "$pull",
            UpdateError::InvalidUpdate("$pull 的值必须是 JSON 对象".to_string()),
        )?;
        let pop_map = op_object(
            map,
            "$pop",
            UpdateError::InvalidUpdate("$pop 的值必须是 JSON 对象".to_string()),
        )?;

        // 至少要有一个非空操作符
        let any_non_empty = [
            set_map,
            unset_map,
            inc_map,
            mul_map,
            min_map,
            max_map,
            rename_map,
            push_map,
            add_to_set_map,
            current_date_map,
            pull_map,
            pop_map,
        ]
        .into_iter()
        .flatten()
        .any(|m| !m.is_empty());
        if !any_non_empty {
            return Err(UpdateError::InvalidUpdate(
                "更新操作不能为空对象,至少需要指定一个字段".to_string(),
            ));
        }

        // ---------- $set ----------
        let mut set_fields = Vec::new();
        for (key, value) in set_map.into_iter().flatten() {
            set_fields.push((parse_path(key)?, value.clone()));
        }

        // ---------- $unset ----------
        let mut unset_fields = Vec::new();
        for key in unset_map.into_iter().flatten().map(|(k, _)| k) {
            let path = parse_path(key)?;
            if path.first().map(|s| s.as_str()) == Some("_id") {
                return Err(UpdateError::InvalidUpdate("不允许删除主键 _id".to_string()));
            }
            unset_fields.push(path);
        }

        // ---------- 数值类:$inc / $mul ----------
        let number_map = |m: Option<&Map<String, Value>>,
                          op: &str|
         -> Result<Vec<(Vec<String>, f64)>, UpdateError> {
            let mut out = Vec::new();
            for (key, value) in m.into_iter().flatten() {
                let n = value.as_f64().ok_or_else(|| {
                    UpdateError::InvalidUpdate(format!("{} 的值必须是数字,字段: {}", op, key))
                })?;
                let path = parse_path(key)?;
                if path.first().map(|s| s.as_str()) == Some("_id") {
                    return Err(UpdateError::InvalidUpdate("不允许修改主键 _id".to_string()));
                }
                out.push((path, n));
            }
            Ok(out)
        };
        let inc_fields = number_map(inc_map, "$inc")?;
        let mul_fields = number_map(mul_map, "$mul")?;

        // ---------- 比较类:$min / $max ----------
        let value_map = |m: Option<&Map<String, Value>>,
                         op: &str|
         -> Result<Vec<(Vec<String>, Value)>, UpdateError> {
            let mut out = Vec::new();
            for (key, value) in m.into_iter().flatten() {
                let path = parse_path(key)?;
                if path.first().map(|s| s.as_str()) == Some("_id") {
                    return Err(UpdateError::InvalidUpdate("不允许修改主键 _id".to_string()));
                }
                let _ = op;
                out.push((path, value.clone()));
            }
            Ok(out)
        };
        let min_fields = value_map(min_map, "$min")?;
        let max_fields = value_map(max_map, "$max")?;

        // ---------- $rename ----------
        let mut rename_fields = Vec::new();
        for (from, to) in rename_map.into_iter().flatten() {
            let from_path = parse_path(from)?;
            let to_str = to.as_str().ok_or_else(|| {
                UpdateError::InvalidUpdate(format!("$rename 的目标路径必须是字符串,字段: {}", from))
            })?;
            let to_path = parse_path(to_str)?;
            if from_path.first().map(|s| s.as_str()) == Some("_id")
                || to_path.first().map(|s| s.as_str()) == Some("_id")
            {
                return Err(UpdateError::InvalidUpdate(
                    "不允许通过 $rename 修改主键 _id".to_string(),
                ));
            }
            rename_fields.push((from_path, to_path));
        }

        // ---------- $push ----------
        let mut push_fields = Vec::new();
        for (key, value) in push_map.into_iter().flatten() {
            push_fields.push(parse_push(key, value)?);
        }

        // ---------- $addToSet ----------
        let mut add_to_set_fields = Vec::new();
        for (key, value) in add_to_set_map.into_iter().flatten() {
            add_to_set_fields.push(parse_add_to_set(key, value)?);
        }

        // ---------- $currentDate ----------
        let mut current_date_fields = Vec::new();
        for (key, _) in current_date_map.into_iter().flatten() {
            let path = parse_path(key)?;
            if path.first().map(|s| s.as_str()) == Some("_id") {
                return Err(UpdateError::InvalidUpdate("不允许修改主键 _id".to_string()));
            }
            current_date_fields.push(path);
        }

        // ---------- $pull ----------
        let mut pull_fields = Vec::new();
        for (key, value) in pull_map.into_iter().flatten() {
            let path = parse_path(key)?;
            if path.first().map(|s| s.as_str()) == Some("_id") {
                return Err(UpdateError::InvalidUpdate("不允许修改主键 _id".to_string()));
            }
            pull_fields.push(PullSpec {
                path,
                field: key.to_string(),
                matcher: parse_pull_matcher(key, value)?,
            });
        }

        // ---------- $pop ----------
        let mut pop_fields = Vec::new();
        for (key, value) in pop_map.into_iter().flatten() {
            let path = parse_path(key)?;
            if path.first().map(|s| s.as_str()) == Some("_id") {
                return Err(UpdateError::InvalidUpdate("不允许修改主键 _id".to_string()));
            }
            let last = match value.as_i64() {
                Some(1) => true,
                Some(-1) => false,
                _ => {
                    return Err(UpdateError::InvalidModifier(
                        "$pop 的值只能是 1(移除最后一个)或 -1(移除第一个)".to_string(),
                    ));
                }
            };
            pop_fields.push(PopSpec { path, last });
        }

        Ok(Update {
            set_fields,
            unset_fields,
            inc_fields,
            mul_fields,
            min_fields,
            max_fields,
            rename_fields,
            push_fields,
            add_to_set_fields,
            current_date_fields,
            pull_fields,
            pop_fields,
        })
    }

    /// 将更新操作应用到文档(原地修改)。
    ///
    /// 执行顺序固定(避免歧义):`$rename` → `$inc` → `$mul` → `$min` → `$max`
    /// → `$set` → `$unset` → `$push` → `$addToSet` → `$currentDate`。
    /// 同一路径同时出现在 `$set`/`$unset` 时,以 `$unset` 生效(沿用 M10 语义)。
    ///
    /// `_id` 字段不允许被修改或删除(主键不可变)。
    pub fn apply(&self, doc: &mut Map<String, Value>) -> Result<(), UpdateError> {
        // $rename:先取旧值再删旧键,最后按 $set 逻辑写新路径
        for (from, to) in &self.rename_fields {
            let taken = take_in_map(doc, from);
            if let Some(value) = taken {
                set_in_map(doc, to, &value)?;
            }
        }

        // $inc:字段缺失视为 0;存在但非数值 → 报错(不静默覆盖)
        for (path, delta) in &self.inc_fields {
            check_not_id(path)?;
            let current = number_at(doc, path, "$inc")?.unwrap_or(0.0);
            set_in_map(doc, path, &number_value(current + delta))?;
        }

        // $mul:字段缺失视为 0(MongoDB 行为);存在但非数值 → 报错
        for (path, factor) in &self.mul_fields {
            check_not_id(path)?;
            let current = number_at(doc, path, "$mul")?.unwrap_or(0.0);
            set_in_map(doc, path, &number_value(current * factor))?;
        }

        // $min / $max:字段缺失直接写入,否则按比较结果决定
        for (path, value) in &self.min_fields {
            check_not_id(path)?;
            let current = resolve_owned(doc, path);
            // 仅当「字段缺失」或「新值更小」时才写入
            let keep = matches!(
                &current,
                Some(cur) if compare_values(Some(cur), Some(value)) != std::cmp::Ordering::Greater
            );
            if !keep {
                set_in_map(doc, path, value)?;
            }
        }
        for (path, value) in &self.max_fields {
            check_not_id(path)?;
            let current = resolve_owned(doc, path);
            // 仅当「字段缺失」或「新值更大」时才写入
            let keep = matches!(
                &current,
                Some(cur) if compare_values(Some(cur), Some(value)) != std::cmp::Ordering::Less
            );
            if !keep {
                set_in_map(doc, path, value)?;
            }
        }

        // $set
        for (path, value) in &self.set_fields {
            check_not_id(path)?;
            set_in_map(doc, path, value)?;
        }

        // $unset
        for path in &self.unset_fields {
            unset_in_map(doc, path)?;
        }

        // $push
        for spec in &self.push_fields {
            check_not_id(&spec.path)?;
            let mut arr = match resolve_owned(doc, &spec.path) {
                Some(Value::Array(a)) => a,
                Some(_) => {
                    return Err(UpdateError::TypeMismatch {
                        op: "$push".to_string(),
                        path: spec.path.join("."),
                        expected: "数组".to_string(),
                    });
                }
                None => Vec::new(),
            };
            apply_push(&mut arr, spec)?;
            set_in_map(doc, &spec.path, &Value::Array(arr))?;
        }

        // $addToSet
        for spec in &self.add_to_set_fields {
            check_not_id(&spec.path)?;
            let mut arr = match resolve_owned(doc, &spec.path) {
                Some(Value::Array(a)) => a,
                Some(_) => {
                    return Err(UpdateError::TypeMismatch {
                        op: "$addToSet".to_string(),
                        path: spec.path.join("."),
                        expected: "数组".to_string(),
                    });
                }
                None => Vec::new(),
            };
            for value in &spec.values {
                if !arr
                    .iter()
                    .any(|el| compare_values(Some(el), Some(value)) == std::cmp::Ordering::Equal)
                {
                    arr.push(value.clone());
                }
            }
            set_in_map(doc, &spec.path, &Value::Array(arr))?;
        }

        // $pull:移除所有满足条件的元素
        for spec in &self.pull_fields {
            check_not_id(&spec.path)?;
            let Some(current) = resolve_owned(doc, &spec.path) else {
                continue; // 字段缺失 → 无操作
            };
            let Value::Array(arr) = current else {
                return Err(UpdateError::TypeMismatch {
                    op: "$pull".to_string(),
                    path: spec.path.join("."),
                    expected: "数组".to_string(),
                });
            };
            let kept: Vec<Value> = arr
                .into_iter()
                .filter(|el| !pull_matches(&spec.matcher, &spec.field, el))
                .collect();
            set_in_map(doc, &spec.path, &Value::Array(kept))?;
        }

        // $pop:移除首/尾元素
        for spec in &self.pop_fields {
            check_not_id(&spec.path)?;
            let Some(current) = resolve_owned(doc, &spec.path) else {
                continue; // 字段缺失 → 无操作
            };
            let Value::Array(mut arr) = current else {
                return Err(UpdateError::TypeMismatch {
                    op: "$pop".to_string(),
                    path: spec.path.join("."),
                    expected: "数组".to_string(),
                });
            };
            if arr.is_empty() {
                continue; // 空数组 → 无操作
            }
            if spec.last {
                arr.pop();
            } else {
                arr.remove(0);
            }
            set_in_map(doc, &spec.path, &Value::Array(arr))?;
        }

        // $currentDate:需实时取时间,放最后
        if !self.current_date_fields.is_empty() {
            let now = current_millis();
            for path in &self.current_date_fields {
                check_not_id(path)?;
                set_in_map(doc, path, &date_literal(now))?;
            }
        }

        Ok(())
    }

    /// 本更新是否包含会「写入新文档」的操作(upsert 时用于判断)。
    pub fn is_empty(&self) -> bool {
        self.set_fields.is_empty()
            && self.unset_fields.is_empty()
            && self.inc_fields.is_empty()
            && self.mul_fields.is_empty()
            && self.min_fields.is_empty()
            && self.max_fields.is_empty()
            && self.rename_fields.is_empty()
            && self.push_fields.is_empty()
            && self.add_to_set_fields.is_empty()
            && self.current_date_fields.is_empty()
    }
}

/// 主键不可被修改。
fn check_not_id(path: &[String]) -> Result<(), UpdateError> {
    if path.first().map(|s| s.as_str()) == Some("_id") {
        return Err(UpdateError::InvalidUpdate("不允许修改主键 _id".to_string()));
    }
    Ok(())
}

/// 把 f64 转回 JSON 数字(整数优先,避免 `1` 变成 `1.0`)。
fn number_value(n: f64) -> Value {
    if n.fract() == 0.0 && n.abs() < 9e15 {
        Value::from(n as i64)
    } else {
        Value::from(n)
    }
}

/// 当前 UTC 毫秒时间戳。
fn current_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 构造 Date 字面量(复用 M14 的 `{"$date": 毫秒}` 表示)。
fn date_literal(millis: i64) -> Value {
    let mut m = Map::new();
    m.insert("$date".to_string(), Value::from(millis));
    Value::Object(m)
}

/// 读取路径上的数字。
///
/// - 字段不存在 → `Ok(None)`(调用方按 0 处理);
/// - 存在但非数值 → 报 [`UpdateError::TypeMismatch`](不得静默覆盖原值)。
fn number_at(
    doc: &Map<String, Value>,
    path: &[String],
    op: &str,
) -> Result<Option<f64>, UpdateError> {
    match resolve_ref(doc, path) {
        None => Ok(None),
        Some(Value::Number(n)) => Ok(Some(n.as_f64().unwrap_or(0.0))),
        Some(_) => Err(UpdateError::TypeMismatch {
            op: op.to_string(),
            path: path.join("."),
            expected: "数值字段".to_string(),
        }),
    }
}

/// 按路径取引用(不穿透数组,仅对象下钻;数组下标按数字寻址)。
fn resolve_ref<'a>(doc: &'a Map<String, Value>, path: &[String]) -> Option<&'a Value> {
    let (first, rest) = path.split_first()?;
    let mut current = doc.get(first)?;
    for seg in rest {
        current = match current {
            Value::Object(map) => map.get(seg)?,
            Value::Array(arr) => arr.get(seg.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(current)
}

/// 按路径取值的克隆(不穿透数组)。
fn resolve_owned(doc: &Map<String, Value>, path: &[String]) -> Option<Value> {
    resolve_ref(doc, path).cloned()
}

/// 按路径**取出并移除**值(供 `$rename` 使用);路径不存在返回 None。
fn take_in_map(doc: &mut Map<String, Value>, path: &[String]) -> Option<Value> {
    let (last, parents) = path.split_last()?;
    let mut current = doc;
    for seg in parents {
        current = match current.get_mut(seg) {
            Some(Value::Object(map)) => map,
            _ => return None,
        };
    }
    current.remove(last)
}

/// 解析 `$push` 的值:单个值或含 `$each` 的修饰符对象。
fn parse_push(key: &str, value: &Value) -> Result<PushSpec, UpdateError> {
    let path = parse_path(key)?;
    // 非对象、或对象但不含修饰符键 → 当作单个元素
    let Some(obj) = value.as_object() else {
        return Ok(PushSpec {
            path,
            values: vec![value.clone()],
            position: None,
            slice: None,
            sort: None,
        });
    };
    if !obj.keys().any(|k| k.starts_with('$')) {
        return Ok(PushSpec {
            path,
            values: vec![value.clone()],
            position: None,
            slice: None,
            sort: None,
        });
    }
    // 修饰符形式:必须带 $each
    let each = obj.get("$each").ok_or_else(|| {
        UpdateError::InvalidModifier(
            "$push 使用修饰符时必须包含 $each(若想追加对象本身,请用 $each: [对象])".to_string(),
        )
    })?;
    let values = each
        .as_array()
        .ok_or_else(|| UpdateError::InvalidModifier("$push 的 $each 必须是数组".to_string()))?
        .clone();
    let position = match obj.get("$position") {
        Some(v) => Some(v.as_u64().ok_or_else(|| {
            UpdateError::InvalidModifier("$push 的 $position 必须是非负整数".to_string())
        })? as usize),
        None => None,
    };
    let slice = match obj.get("$slice") {
        Some(v) => Some(v.as_i64().ok_or_else(|| {
            UpdateError::InvalidModifier("$push 的 $slice 必须是整数".to_string())
        })?),
        None => None,
    };
    let sort = obj.get("$sort").cloned();
    // 未知修饰符一律报错,避免拼写错误静默失效
    for k in obj.keys() {
        if !["$each", "$position", "$slice", "$sort"].contains(&k.as_str()) {
            return Err(UpdateError::InvalidModifier(format!(
                "$push 不支持的修饰符: {}",
                k
            )));
        }
    }
    Ok(PushSpec {
        path,
        values,
        position,
        slice,
        sort,
    })
}

/// 解析 `$addToSet` 的值:单个值或 `{"$each": [...]}`。
fn parse_add_to_set(key: &str, value: &Value) -> Result<AddToSetSpec, UpdateError> {
    let path = parse_path(key)?;
    let Some(obj) = value.as_object() else {
        return Ok(AddToSetSpec {
            path,
            values: vec![value.clone()],
        });
    };
    if !obj.keys().any(|k| k.starts_with('$')) {
        return Ok(AddToSetSpec {
            path,
            values: vec![value.clone()],
        });
    }
    let each = obj.get("$each").ok_or_else(|| {
        UpdateError::InvalidModifier(
            "$addToSet 使用修饰符时必须包含 $each(若想加入对象本身,请用 $each: [对象])".to_string(),
        )
    })?;
    let values = each
        .as_array()
        .ok_or_else(|| UpdateError::InvalidModifier("$addToSet 的 $each 必须是数组".to_string()))?
        .clone();
    for k in obj.keys() {
        if k != "$each" {
            return Err(UpdateError::InvalidModifier(format!(
                "$addToSet 不支持的修饰符: {}",
                k
            )));
        }
    }
    Ok(AddToSetSpec { path, values })
}

/// 解析 `$pull` 的匹配条件(v0.6.1)。
///
/// - 值是含 `$` 键的对象 → 按**查询操作符**解释(复用 `filter::Condition`,
///   如 `{"$gt": 5}` / `{"$in": ["a","b"]}`);
/// - 否则按**等值/子集**匹配:标量走整体相等,对象走「字段子集匹配」
///   (MongoDB 语义:`$pull: {r: {score: 8}}` 移除所有 `score == 8` 的对象,
///   不要求对象只有这一个字段)。
fn parse_pull_matcher(
    key: &str,
    value: &Value,
) -> Result<crate::query::filter::Condition, UpdateError> {
    use crate::query::filter::Condition;
    let err = |e: crate::query::filter::FilterError| {
        UpdateError::InvalidModifier(format!("$pull 条件无效: {}", e))
    };
    if let Value::Object(obj) = value
        && obj.keys().any(|k| k.starts_with('$'))
    {
        // 操作符形态:构造 `{字段名: 操作符对象}` 交给查询侧解析
        let mut wrapper = serde_json::Map::new();
        wrapper.insert(key.to_string(), value.clone());
        return Condition::from_field_object(&wrapper).map_err(err);
    }
    // 对象形态 → **子集匹配**:把对象的字段展开成字段条件
    // (MongoDB 语义:`$pull: {r: {score: 8}}` 移除所有 score==8 的对象,
    //  不要求对象只有这一个字段)。这依赖 `Condition::matches_value` 在
    //  元素是对象时按对象自身的字段求值。
    if let Value::Object(obj) = value {
        return Condition::from_field_object(obj).map_err(err);
    }
    // 标量/数组等 → 等值
    let mut wrapper = serde_json::Map::new();
    wrapper.insert(key.to_string(), value.clone());
    Condition::from_field_object(&wrapper).map_err(err)
}

/// 判断单个数组元素是否满足 `$pull` 的条件。
///
/// `matcher` 是按「字段名 → 条件」构造的,路径段就是原字段名;这里把元素
/// 包成 `{字段名: 元素}` 再求值,于是操作符作用于元素本身、对象则按字段求值。
fn pull_matches(matcher: &crate::query::filter::Condition, field: &str, element: &Value) -> bool {
    match element {
        // 子集匹配:条件直接作用在对象自身的字段上
        Value::Object(map) => matcher.matches(map),
        // 操作符形态:把标量包成 `{字段: 值}` 让其作用于元素本身
        other => {
            let mut probe = serde_json::Map::new();
            probe.insert(field.to_string(), other.clone());
            matcher.matches(&probe)
        }
    }
}

/// 对数组应用 `$push`(含 `$position`/`$sort`/`$slice` 修饰符)。
fn apply_push(arr: &mut Vec<Value>, spec: &PushSpec) -> Result<(), UpdateError> {
    // 1. 按位置插入
    match spec.position {
        Some(pos) => {
            let pos = pos.min(arr.len());
            for (i, v) in spec.values.iter().enumerate() {
                arr.insert(pos + i, v.clone());
            }
        }
        None => arr.extend(spec.values.iter().cloned()),
    }

    // 2. 排序
    if let Some(sort) = &spec.sort {
        match sort {
            // 整体排序:1 升序 / -1 降序
            Value::Number(n) => {
                let desc = n.as_i64() == Some(-1);
                arr.sort_by(|a, b| {
                    let ord = compare_values(Some(a), Some(b));
                    if desc { ord.reverse() } else { ord }
                });
            }
            // 按字段排序:{"age": 1, "name": -1}
            Value::Object(map) => {
                let spec = crate::query::SortSpec::from_value(&Value::Object(map.clone()))
                    .map_err(|e| UpdateError::InvalidModifier(format!("$sort 无效: {}", e)))?;
                arr.sort_by(|a, b| match (a.as_object(), b.as_object()) {
                    (Some(am), Some(bm)) => {
                        let ka = spec.extract_sort_key(am);
                        let kb = spec.extract_sort_key(bm);
                        spec.compare_sort_keys(&ka, &kb)
                    }
                    _ => std::cmp::Ordering::Equal,
                });
            }
            _ => {
                return Err(UpdateError::InvalidModifier(
                    "$sort 必须是 1/-1 或字段排序对象".to_string(),
                ));
            }
        }
    }

    // 3. 截取
    if let Some(slice) = spec.slice {
        if slice == 0 {
            arr.clear();
        } else if slice > 0 {
            arr.truncate(slice as usize);
        } else {
            let keep = (-slice) as usize;
            if arr.len() > keep {
                arr.drain(0..arr.len() - keep);
            }
        }
    }
    Ok(())
}

/// 在顶层对象上按点号路径删除字段($unset)。
///
/// 语义(对齐 MongoDB):
/// - 目标字段不存在 → 静默无操作;
/// - 中间路径是标量或不存在 → 无操作(不报错);
/// - 目标是数组元素 → 置为 `null`(占位保留)。
fn unset_in_map(doc: &mut Map<String, Value>, path: &[String]) -> Result<(), UpdateError> {
    let Some((first, rest)) = path.split_first() else {
        return Ok(());
    };
    if rest.is_empty() {
        doc.remove(first);
        return Ok(());
    }
    if let Some(child) = doc.get_mut(first) {
        unset_in_value(child, rest)?;
    }
    Ok(())
}

/// 在任意 JSON 值上按点号路径删除字段,明确区分对象与数组。
fn unset_in_value(root: &mut Value, path: &[String]) -> Result<(), UpdateError> {
    let Some((first, rest)) = path.split_first() else {
        return Ok(());
    };
    match root {
        Value::Object(map) => {
            if rest.is_empty() {
                map.remove(first);
            } else if let Some(child) = map.get_mut(first) {
                unset_in_value(child, rest)?;
            }
        }
        Value::Array(arr) => {
            // 下标格式非法:与 $set 同样报错(防拼写错误静默无操作)
            let index = parse_array_index(first)?;
            if index < arr.len() {
                if rest.is_empty() {
                    // MongoDB 语义:数组元素 unset 后置 null,占位保留
                    arr[index] = Value::Null;
                } else {
                    unset_in_value(&mut arr[index], rest)?;
                }
            }
            // 越界:目标不存在,无操作
        }
        // 标量中间值:目标不存在,无操作
        _ => {}
    }
    Ok(())
}

/// 在顶层对象上按点号路径赋值。
///
/// 单段路径直接写入顶层;多段路径取出/创建子容器后交给 [`set_in_value`]。
fn set_in_map(
    doc: &mut Map<String, Value>,
    path: &[String],
    value: &Value,
) -> Result<(), UpdateError> {
    // 空路径:忽略
    let Some((first, rest)) = path.split_first() else {
        return Ok(());
    };

    // 末段路径:直接赋值
    if rest.is_empty() {
        doc.insert(first.clone(), value.clone());
        return Ok(());
    }

    // 非末段:取出或创建子容器(已存在的值原样保留,不做覆盖),
    // 真正的类型区分(对象/数组/标量)交给 set_in_value 处理。
    let child = doc
        .entry(first.clone())
        .or_insert_with(|| Value::Object(Map::new()));
    set_in_value(child, rest, value)
}

/// 在任意 JSON 值上按点号路径赋值,明确区分对象与数组。
///
/// - `Value::Object`:按字段名下钻,缺失时自动创建对象;
/// - `Value::Array`:当前路径段必须是纯数字下标且不越界,按下标寻址;
/// - 其他(标量):报 [`UpdateError::ScalarIntermediate`],**绝不覆盖原值**。
fn set_in_value(root: &mut Value, path: &[String], value: &Value) -> Result<(), UpdateError> {
    // 空路径:忽略
    let Some((first, rest)) = path.split_first() else {
        return Ok(());
    };

    match root {
        Value::Object(map) => {
            if rest.is_empty() {
                map.insert(first.clone(), value.clone());
                Ok(())
            } else {
                let child = map
                    .entry(first.clone())
                    .or_insert_with(|| Value::Object(Map::new()));
                set_in_value(child, rest, value)
            }
        }
        Value::Array(arr) => {
            // 数组必须按下标寻址:非数字段、越界段一律报错
            let index = parse_array_index(first)?;
            if index >= arr.len() {
                return Err(UpdateError::ArrayIndexOutOfBounds {
                    index,
                    len: arr.len(),
                });
            }
            if rest.is_empty() {
                arr[index] = value.clone();
                Ok(())
            } else {
                set_in_value(&mut arr[index], rest, value)
            }
        }
        // 标量中间值:不下钻也不覆盖,明确报错(避免静默破坏既有数据)
        _ => Err(UpdateError::ScalarIntermediate {
            path: path.join("."),
        }),
    }
}

/// 解析数组下标:必须是纯数字(不含符号/空格/小数)。
fn parse_array_index(seg: &str) -> Result<usize, UpdateError> {
    if seg.is_empty() || !seg.bytes().all(|b| b.is_ascii_digit()) {
        return Err(UpdateError::InvalidArrayIndex(seg.to_string()));
    }
    seg.parse::<usize>()
        .map_err(|_| UpdateError::InvalidArrayIndex(seg.to_string()))
}
