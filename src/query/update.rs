//! 更新操作模块(M4)。
//!
//! 仿 MongoDB 的更新操作符。当前支持 `$set`(字段点号路径赋值,嵌套自动创建/覆盖),
//! 后续里程碑可扩展 `$unset/$inc/$push` 等。

use serde_json::{Map, Value};

/// 更新操作错误。
#[derive(Debug)]
pub enum UpdateError {
    /// update 顶层必须是 JSON 对象,且只能包含已知操作符。
    InvalidUpdate(String),
    /// `$set` 的值必须是 JSON 对象。
    SetNotObject,
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateError::InvalidUpdate(msg) => write!(f, "更新操作使用错误: {}", msg),
            UpdateError::SetNotObject => write!(f, "$set 的值必须是 JSON 对象"),
        }
    }
}

impl std::error::Error for UpdateError {}

/// 一组更新操作。
///
/// 当前仅支持 `$set`:将指定字段(点号路径)设置为新值。
#[derive(Debug)]
pub struct Update {
    /// `$set` 的字段赋值列表(字段路径 → 新值)。
    set_fields: Vec<(Vec<String>, Value)>,
}

impl Update {
    /// 从 JSON 值解析更新操作。
    ///
    /// 示例:`{"$set": {"name": "李四", "addr.city": "上海"}}`。
    pub fn from_value(value: &Value) -> Result<Self, UpdateError> {
        let map = value
            .as_object()
            .ok_or_else(|| UpdateError::InvalidUpdate("update 顶层必须是 JSON 对象".to_string()))?;

        // 只允许已知操作符:$set
        for key in map.keys() {
            if !["$set"].contains(&key.as_str()) {
                return Err(UpdateError::InvalidUpdate(format!(
                    "暂不支持的更新操作符: {}",
                    key
                )));
            }
        }

        let set_map = match map.get("$set") {
            Some(Value::Object(set_map)) => set_map,
            Some(_) => return Err(UpdateError::SetNotObject),
            // 无 $set:空操作(结果不影响任何文档)
            None => {
                return Ok(Update {
                    set_fields: Vec::new(),
                })
            }
        };

        // 解析 $set 的每个字段(支持点号路径)
        let mut set_fields = Vec::new();
        for (key, value) in set_map {
            let path = key.split('.').map(|s| s.to_string()).collect::<Vec<_>>();
            set_fields.push((path, value.clone()));
        }

        Ok(Update { set_fields })
    }

    /// 将更新操作应用到文档(原地修改)。
    ///
    /// - 点号路径嵌套赋值,路径上的中间对象若不存在则自动创建;
    /// - `_id` 字段不允许被修改(主键不可变)。
    pub fn apply(&self, doc: &mut Map<String, Value>) -> Result<(), UpdateError> {
        for (path, value) in &self.set_fields {
            // 主键不可修改
            if path.first().map(|s| s.as_str()) == Some("_id") {
                return Err(UpdateError::InvalidUpdate(
                    "不允许修改主键 _id".to_string(),
                ));
            }
            set_by_path(doc, path, value);
        }
        Ok(())
    }
}

/// 按点号路径赋值(嵌套对象不存在则自动创建)。
fn set_by_path(doc: &mut Map<String, Value>, path: &[String], value: &Value) {
    // 空路径:忽略
    let Some(first) = path.first() else {
        return;
    };

    // 单段路径:直接赋值
    if path.len() == 1 {
        doc.insert(first.clone(), value.clone());
        return;
    }

    // 多段路径:递归下钻(中间缺失的对象自动创建)
    let next: &[String] = &path[1..];
    let child = doc
        .entry(first.clone())
        .or_insert_with(|| Value::Object(Map::new()));
    match child {
        Value::Object(map) => set_by_path(map, next, value),
        // 中间路径碰到非对象值:用对象覆盖(与 MongoDB $set 语义一致)
        _ => {
            let mut new_map = Map::new();
            set_by_path(&mut new_map, next, value);
            *child = Value::Object(new_map);
        }
    }
}
