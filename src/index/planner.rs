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
///
/// # 复合索引的前缀原则
///
/// 索引 `{a, b}` 可服务 `{a: 1}` 与 `{a: 1, b: 2}`,但**不能**服务 `{b: 2}`。
/// 本函数按此规则判断:只有索引字段的**前缀**全部出现在 filter 里才算可用,
/// 且可用的字段数越多、越靠前,优先级越高。
pub fn plan_with_json(filter_json: &Value, sort: Option<&SortSpec>, indexes: &[IndexDef]) -> Plan {
    let Some(obj) = filter_json.as_object() else {
        return Plan::CollScan;
    };

    // 收集所有可用计划,取「前缀最长」者(等值优先于范围)
    let mut best: Option<(usize, usize, Plan)> = None; // (前缀长度, 是否全等值, plan)

    for def in indexes {
        // 按字段顺序,尽可能多地消费 filter 里的条件
        let mut segs: Vec<Vec<u8>> = Vec::new();
        let mut all_equality = true;
        let mut last_range_field: Option<String> = None;

        for field in &def.fields {
            let Some(value) = obj.get(field) else {
                break;
            };
            let is_op_object = value
                .as_object()
                .is_some_and(|m| m.keys().any(|k| k.starts_with('$')));

            if is_op_object {
                // 范围条件:只能作为**最后一个**前缀字段(MongoDB 前缀原则)
                if segs.len() < def.fields.len() - 1 {
                    // 后面还有字段,但当前是范围 → 只能用到当前字段为止
                    all_equality = false;
                    last_range_field = Some(field.clone());
                    // 范围字段本身不进入精确前缀(区间由 bounds 表达)
                    break;
                }
                all_equality = false;
                last_range_field = Some(field.clone());
                break;
            } else {
                // 等值条件:可以进入精确前缀
                segs.push(encode_key(Some(value)));
            }
        }

        // 前缀至少要覆盖第一个字段,否则这个索引用不上
        if segs.is_empty() && last_range_field.is_none() {
            continue;
        }
        if segs.is_empty() && last_range_field.is_some() {
            // 第一个字段就是范围条件:可用,但前缀为空
        }

        // 构造区间:等值前缀 + (可选)范围字段的上下界
        let (lower, upper, desc) = match &last_range_field {
            Some(rf) => {
                let ops = obj
                    .get(rf)
                    .and_then(|v| v.as_object())
                    .cloned()
                    .unwrap_or_default();
                let (lo, hi, d) = range_component(&ops, rf);
                let mut l = concat_segs(&segs);
                l.extend_from_slice(&lo.unwrap_or_default());
                let mut u = if lo_is_set(&ops) {
                    // 有下界时上界需在等值前缀之后放开
                    concat_segs(&segs)
                } else {
                    concat_segs(&segs)
                };
                if let Some(h) = hi {
                    u.extend_from_slice(&h);
                } else {
                    // 无上界:前缀即足够,后续由 filter 二次过滤保证正确性
                }
                let lower_v = if lo_is_set(&ops) || !segs.is_empty() {
                    Some(l)
                } else {
                    None
                };
                let upper_v = if hi_is_set(&ops) && !u.is_empty() {
                    Some(u)
                } else {
                    None
                };
                (lower_v, upper_v, d)
            }
            None => {
                // 全等值:front 是精确前缀,用 0xFF 作上界
                let prefix = concat_segs(&segs);
                let mut upper = prefix.clone();
                upper.push(0xFF);
                (Some(prefix), Some(upper), "eq".to_string())
            }
        };

        let plan = Plan::IndexScan {
            index: def.clone(),
            bounds: IndexBounds {
                lower,
                upper,
                description: format!("{} ({})", def.field_signature(), desc),
            },
            sorted_by_index: sort
                .map(|s| sort_matches_prefix(s, &def.fields))
                .unwrap_or(true),
        };

        // 优先:前缀字段多 > 全等值
        let score = (segs.len(), usize::from(all_equality));
        if best.as_ref().is_none_or(|(n, eq, _)| score > (*n, *eq)) {
            best = Some((score.0, score.1, plan));
        }
    }

    best.map(|(_, _, p)| p).unwrap_or(Plan::CollScan)
}

/// 拼接等值前缀段。
fn concat_segs(segs: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    for s in segs {
        out.extend_from_slice(s);
    }
    out
}

/// 范围字段的上下界组件(不含等值前缀)。
fn range_component(
    ops: &Map<String, Value>,
    field: &str,
) -> (Option<Vec<u8>>, Option<Vec<u8>>, String) {
    let mut lo: Option<Vec<u8>> = None;
    let mut hi: Option<Vec<u8>> = None;
    let mut desc: Vec<String> = Vec::new();

    if let Some(v) = ops.get("$gte") {
        lo = Some(encode_key(Some(v)));
        desc.push(">=".to_string());
    }
    if let Some(v) = ops.get("$gt") {
        let mut k = encode_key(Some(v));
        k.push(0x00);
        lo = Some(k);
        desc.push(">".to_string());
    }
    if let Some(v) = ops.get("$lte") {
        let mut k = encode_key(Some(v));
        k.push(0xFF);
        hi = Some(k);
        desc.push("<=".to_string());
    }
    if let Some(v) = ops.get("$lt") {
        hi = Some(encode_key(Some(v)));
        desc.push("<".to_string());
    }
    (lo, hi, format!("{} {}", field, desc.join(",")))
}

/// `$gte`/`$gt` 是否出现。
fn lo_is_set(ops: &Map<String, Value>) -> bool {
    ops.contains_key("$gte") || ops.contains_key("$gt")
}

/// `$lte`/`$lt` 是否出现。
fn hi_is_set(ops: &Map<String, Value>) -> bool {
    ops.contains_key("$lte") || ops.contains_key("$lt")
}

/// 排序字段序列是否匹配索引前缀(顺序相同、方向一致)。
///
/// 复合索引 `{a,b}` 可满足 `sort {a:1,b:1}` 或 `sort {a:1}`。
fn sort_matches_prefix(sort: &SortSpec, fields: &[String]) -> bool {
    sort.field_paths()
        .iter()
        .zip(fields.iter())
        .all(|(s, f)| s == &f.split('.').map(|x| x.to_string()).collect::<Vec<_>>())
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
            out.insert("field".to_string(), Value::String(index.field_signature()));
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
