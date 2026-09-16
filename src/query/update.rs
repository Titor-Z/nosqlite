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
//! 后续里程碑可扩展 `$inc/$push` 等。

use serde_json::{Map, Value};

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
        }
    }
}

impl std::error::Error for UpdateError {}

/// 一组更新操作。
///
/// 支持 `$set`(字段点号路径赋值)与 `$unset`(字段点号路径删除,M10)。
#[derive(Debug)]
pub struct Update {
    /// `$set` 的字段赋值列表(字段路径 → 新值)。
    set_fields: Vec<(Vec<String>, Value)>,
    /// `$unset` 的字段路径列表(按声明顺序依次删除)。
    unset_fields: Vec<Vec<String>>,
}

impl Update {
    /// 从 JSON 值解析更新操作。
    ///
    /// 示例:`{"$set": {"name": "李四", "addr.city": "上海"}}`。
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

        // 只允许已知操作符:$set / $unset(M10)
        for key in map.keys() {
            if !["$set", "$unset"].contains(&key.as_str()) {
                return Err(UpdateError::InvalidUpdate(format!(
                    "暂不支持的更新操作符: {}",
                    key
                )));
            }
        }

        let set_map = match map.get("$set") {
            Some(Value::Object(set_map)) => Some(set_map),
            Some(_) => return Err(UpdateError::SetNotObject),
            None => None,
        };
        let unset_map = match map.get("$unset") {
            Some(Value::Object(unset_map)) => Some(unset_map),
            Some(_) => return Err(UpdateError::UnsetNotObject),
            None => None,
        };

        // 两个操作符都缺席或均为空:没有任何实际更新
        let set_empty = set_map.map(|m| m.is_empty()).unwrap_or(true);
        let unset_empty = unset_map.map(|m| m.is_empty()).unwrap_or(true);
        if set_map.is_none() && unset_map.is_none() {
            return Err(UpdateError::InvalidUpdate(
                "update 缺少可用的更新操作符".to_string(),
            ));
        }
        if set_empty && unset_empty {
            return Err(UpdateError::InvalidUpdate(
                "更新操作不能为空对象,至少需要指定一个字段".to_string(),
            ));
        }

        // 解析 $set 的每个字段(支持点号路径)
        let mut set_fields = Vec::new();
        for (key, value) in set_map.into_iter().flatten() {
            let path = key.split('.').map(|s| s.to_string()).collect::<Vec<_>>();
            // 空字段名或空路径段(`""`、`"a..b"`、`".a"`)无意义,直接拒绝
            if path.iter().any(|seg| seg.is_empty()) {
                return Err(UpdateError::InvalidUpdate(format!(
                    "字段路径非法: `{}` 含空的路径段",
                    key
                )));
            }
            set_fields.push((path, value.clone()));
        }

        // 解析 $unset 的每个字段(支持点号路径;值被忽略,对齐 MongoDB)
        let mut unset_fields = Vec::new();
        for key in unset_map.into_iter().flatten().map(|(k, _)| k) {
            let path = key.split('.').map(|s| s.to_string()).collect::<Vec<_>>();
            if path.iter().any(|seg| seg.is_empty()) {
                return Err(UpdateError::InvalidUpdate(format!(
                    "字段路径非法: `{}` 含空的路径段",
                    key
                )));
            }
            // 主键不可被删除
            if path.first().map(|s| s.as_str()) == Some("_id") {
                return Err(UpdateError::InvalidUpdate(
                    "不允许删除主键 _id".to_string(),
                ));
            }
            unset_fields.push(path);
        }

        Ok(Update {
            set_fields,
            unset_fields,
        })
    }

    /// 将更新操作应用到文档(原地修改)。
    ///
    /// - 点号路径嵌套赋值,路径上的中间对象若不存在则自动创建;
    /// - `_id` 字段不允许被修改(主键不可变)。
    pub fn apply(&self, doc: &mut Map<String, Value>) -> Result<(), UpdateError> {
        // 先赋值后删除(同一路径同时出现在 $set/$unset 时,以 $unset 生效)
        for (path, value) in &self.set_fields {
            // 主键不可修改
            if path.first().map(|s| s.as_str()) == Some("_id") {
                return Err(UpdateError::InvalidUpdate(
                    "不允许修改主键 _id".to_string(),
                ));
            }
            set_in_map(doc, path, value)?;
        }
        for path in &self.unset_fields {
            unset_in_map(doc, path)?;
        }
        Ok(())
    }
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
fn set_in_map(doc: &mut Map<String, Value>, path: &[String], value: &Value) -> Result<(), UpdateError> {
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
