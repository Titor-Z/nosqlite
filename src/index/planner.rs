//! 查询规划器(M24):把 filter 条件翻译成索引扫描计划。
//!
//! # 设计原则
//!
//! **索引只负责缩小候选集,正确性由完整 filter 保证。**
//!
//! 即使区间构造有偏差,最坏结果也只是「多读了文档」,绝不会返回错误数据。
//! 调用方必须在拿到候选 doc_id 后,对回表的文档再做一次 `filter.matches()`。
//!
//! # 匹配规则
//!
//! 本期只处理**单字段条件**(复合索引留待后续):
//!
//! | filter 形态 | 索引区间 |
//! |---|---|
//! | `{f: v}` / `{f: {$eq: v}}` | `[enc(v), enc(v) + 0xFF)` |
//! | `{f: {$gte: v}}` | `[enc(v), ∞)` |
//! | `{f: {$gt: v}}` | `[enc(v) + 0x00, ∞)` |
//! | `{f: {$lte: v}}` | `(-∞, enc(v) + 0xFF)` |
//! | `{f: {$lt: v}}` | `(-∞, enc(v))` |
//!
//! 等值优先于范围(等值区间更窄)。

use serde_json::{Map, Value};

use crate::index::def::IndexDef;
use crate::index::key::encode_key;
use crate::query::SortSpec;

/// 索引区间的下界/上界(均为编码后的字节,语义为 `[lower, upper)`)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexBounds {
    /// 下界(含)。`None` 表示 -∞。
    pub lower: Option<Vec<u8>>,
    /// 上界(不含)。`None` 表示 +∞。
    pub upper: Option<Vec<u8>>,
    /// 区间描述(供 `explain` 展示)。
    pub description: String,
}

/// 一个查询计划。
#[derive(Debug, Clone)]
pub enum Plan {
    /// 全表扫描。
    CollScan,
    /// 走索引扫描。
    IndexScan {
        /// 使用的索引定义。
        index: IndexDef,
        /// 区间。
        bounds: IndexBounds,
        /// 排序是否可直接由索引顺序满足。
        sorted_by_index: bool,
    },
}

impl Plan {
    /// 计划名(供 `explain` 与测试断言)。
    pub fn stage(&self) -> &'static str {
        match self {
            Plan::CollScan => "COLLSCAN",
            Plan::IndexScan { .. } => "IXSCAN",
        }
    }

    /// 使用的索引名(全表扫时为 None)。
    pub fn index_name(&self) -> Option<&str> {
        match self {
            Plan::CollScan => None,
            Plan::IndexScan { index, .. } => Some(&index.name),
        }
    }

    /// 是否为索引扫描。
    pub fn is_index_scan(&self) -> bool {
        matches!(self, Plan::IndexScan { .. })
    }
}

/// 用**原始 filter JSON** 生成计划。
///
/// 之所以需要原始 JSON:范围操作符(`$gte` 等)在 `Filter` 内部解析后不再保留
/// 原始取值,而构造索引区间必须拿到它们。
pub fn plan_with_json(filter_json: &Value, sort: Option<&SortSpec>, indexes: &[IndexDef]) -> Plan {
    let Some(obj) = filter_json.as_object() else {
        return Plan::CollScan;
    };

    for def in indexes {
        let Some(value) = obj.get(&def.field) else {
            continue;
        };

        // 是否为「操作符对象」(含 $ 前缀键)
        let is_op_object = value
            .as_object()
            .is_some_and(|m| m.keys().any(|k| k.starts_with('$')));

        let bounds = if is_op_object {
            bounds_from_ops(value.as_object().unwrap(), &def.field)
        } else {
            // 隐式等值
            Some(equality_bounds(&encode_key(Some(value)), &def.field))
        };

        if let Some(bounds) = bounds {
            return Plan::IndexScan {
                index: def.clone(),
                bounds,
                sorted_by_index: sort
                    .map(|s| sort_single_field_matches(s, &def.field))
                    .unwrap_or(true),
            };
        }
    }
    Plan::CollScan
}

/// 等值区间:`[enc, enc + 0xFF)`。
///
/// 上界取 `enc` 后接 `0xFF`(不是 `enc` 递增),避免进位问题。
/// 调用方**必须**再做一次完整 `filter.matches()` —— 索引只缩小候选集。
fn equality_bounds(enc: &[u8], field: &str) -> IndexBounds {
    let mut upper = enc.to_vec();
    upper.push(0xFF);
    IndexBounds {
        lower: Some(enc.to_vec()),
        upper: Some(upper),
        description: format!("{} = <eq>", field),
    }
}

/// 从操作符对象构造区间(仅处理单字段比较操作符)。
fn bounds_from_ops(ops: &Map<String, Value>, field: &str) -> Option<IndexBounds> {
    // $eq 优先
    if let Some(v) = ops.get("$eq") {
        return Some(equality_bounds(&encode_key(Some(v)), field));
    }

    let mut lower: Option<Vec<u8>> = None;
    let mut upper: Option<Vec<u8>> = None;
    let mut desc: Vec<String> = Vec::new();

    if let Some(v) = ops.get("$gte") {
        lower = Some(encode_key(Some(v)));
        desc.push(">=".to_string());
    }
    if let Some(v) = ops.get("$gt") {
        let mut k = encode_key(Some(v));
        // 追加 0x00 排除等值(编码值用长度前缀自定界,追加 0x00 安全)
        k.push(0x00);
        lower = Some(k);
        desc.push(">".to_string());
    }
    if let Some(v) = ops.get("$lte") {
        let mut k = encode_key(Some(v));
        k.push(0xFF);
        upper = Some(k);
        desc.push("<=".to_string());
    }
    if let Some(v) = ops.get("$lt") {
        upper = Some(encode_key(Some(v)));
        desc.push("<".to_string());
    }

    if desc.is_empty() {
        return None;
    }
    Some(IndexBounds {
        lower,
        upper,
        description: format!("{} {}", field, desc.join(",")),
    })
}

/// 排序规格是否恰好只有该字段(单字段排序)。
fn sort_single_field_matches(sort: &SortSpec, field: &str) -> bool {
    sort.single_field_path()
        .map(|segs| segs.join(".") == field)
        .unwrap_or(false)
}

/// 供 `explain` 使用的计划描述。
pub fn explain_plan(plan: &Plan, coll: &str) -> Map<String, Value> {
    let mut out = Map::new();
    match plan {
        Plan::CollScan => {
            out.insert("stage".to_string(), Value::String("COLLSCAN".to_string()));
            out.insert("collection".to_string(), Value::String(coll.to_string()));
        }
        Plan::IndexScan {
            index,
            bounds,
            sorted_by_index,
        } => {
            out.insert("stage".to_string(), Value::String("IXSCAN".to_string()));
            out.insert("collection".to_string(), Value::String(coll.to_string()));
            out.insert("indexName".to_string(), Value::String(index.name.clone()));
            out.insert("field".to_string(), Value::String(index.field.clone()));
            out.insert(
                "bounds".to_string(),
                Value::String(bounds.description.clone()),
            );
            out.insert("sortedByIndex".to_string(), Value::Bool(*sorted_by_index));
            // 回表后仍需完整 filter 二次过滤 —— 明确写进 explain,避免误解
            out.insert("filterApplied".to_string(), Value::Bool(true));
        }
    }
    out
}
