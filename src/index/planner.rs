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
    /// 多区间**并集**(M26):每个子计划各自扫索引,候选 doc_id 取并集。
    ///
    /// 用于 `$or`:`{$or: [{a:1},{a:2}]}` → 两个区间并集。
    /// **所有分支都必须能走索引**,否则整体退回全表扫。
    Union {
        /// 各分支的索引扫描计划。
        branches: Vec<Plan>,
    },
    /// 多索引**交集**(M26):各索引各自出候选集,取 doc_id 交集。
    ///
    /// 用于 `{a:1, b:2}` 且 `a`、`b` 各有独立索引(closest 候选最少的先扫)。
    Intersect {
        /// 参与交集的索引扫描计划。
        branches: Vec<Plan>,
    },
}

impl Plan {
    /// 计划名(供 `explain` 与测试断言)。
    pub fn stage(&self) -> &'static str {
        match self {
            Plan::CollScan => "COLLSCAN",
            Plan::IndexScan { .. } => "IXSCAN",
            Plan::Union { .. } => "OR",
            Plan::Intersect { .. } => "AND",
        }
    }

    /// 使用的索引名(全表扫时为 None)。
    pub fn index_name(&self) -> Option<&str> {
        match self {
            Plan::CollScan => None,
            Plan::IndexScan { index, .. } => Some(&index.name),
            Plan::Union { branches } => branches.first().and_then(|b| b.index_name()),
            Plan::Intersect { branches } => branches.first().and_then(|b| b.index_name()),
        }
    }

    /// 是否为索引扫描。
    pub fn is_index_scan(&self) -> bool {
        !matches!(self, Plan::CollScan)
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

    // ① `$or`:拆分各分支,全部能走索引才整体走索引(并集)
    if let Some(Value::Array(branches)) = obj.get("$or") {
        let plans: Option<Vec<Plan>> = branches
            .iter()
            .map(|b| {
                let p = plan_conjunction(b, sort, indexes);
                p.is_index_scan().then_some(p)
            })
            .collect();
        if let Some(plans) = plans.filter(|v| !v.is_empty()) {
            return Plan::Union { branches: plans };
        }
        return Plan::CollScan;
    }

    // ② 顶层是 `$and`:合并各子条件的字段(交集由下面的 conjunct 逻辑处理)
    //    这里简化处理:把 `$and` 数组里的对象合并成一个平坦对象再规划。
    if let Some(Value::Array(conds)) = obj.get("$and") {
        let mut merged = Map::new();
        let mut ok = true;
        for c in conds {
            match c.as_object() {
                Some(m) => {
                    for (k, v) in m {
                        if merged.insert(k.clone(), v.clone()).is_some() {
                            ok = false; // 同字段重复,放弃合并
                        }
                    }
                }
                None => ok = false,
            }
        }
        if ok {
            return plan_conjunction(&Value::Object(merged), sort, indexes);
        }
    }

    plan_conjunction(filter_json, sort, indexes)
}

/// 为**合取条件**(顶层字段之间是 AND)生成计划。
///
/// 优先尝试「单个复合索引覆盖最长前缀」;若不划算,尝试「多个单字段索引取交集」。
fn plan_conjunction(filter_json: &Value, sort: Option<&SortSpec>, indexes: &[IndexDef]) -> Plan {
    plan_conjunction_inner(filter_json, sort, indexes, true)
}

/// `allow_intersection = false` 时不再尝试交集,避免构造交集分支时无限递归。
fn plan_conjunction_inner(
    filter_json: &Value,
    sort: Option<&SortSpec>,
    indexes: &[IndexDef],
    allow_intersection: bool,
) -> Plan {
    let Some(obj) = filter_json.as_object() else {
        return Plan::CollScan;
    };

    // 收集所有可用计划,取「前缀最长」者(等值优先于范围)
    let mut best: Option<(usize, usize, Plan)> = None; // (前缀长度, 是否全等值, plan)

    for def in indexes {
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
                all_equality = false;
                last_range_field = Some(field.clone());
                break;
            }
            segs.push(encode_key(Some(value)));
        }

        if segs.is_empty() && last_range_field.is_none() {
            continue;
        }

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
                let mut u = concat_segs(&segs);
                if let Some(h) = hi {
                    u.extend_from_slice(&h);
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
            sorted_by_index: sort.map(|s| sort_matches_prefix(s, def)).unwrap_or(true),
        };

        let score = (segs.len(), usize::from(all_equality));
        if best.as_ref().is_none_or(|(n, eq, _)| score > (*n, *eq)) {
            best = Some((score.0, score.1, plan));
        }
    }

    let primary = best.map(|(_, _, p)| p);

    // 尝试索引交集:对 filter 里每个有独立索引的字段各出一个计划
    if let Some(secondary) = allow_intersection
        .then(|| intersection_plans(obj, sort, indexes, primary.as_ref()))
        .flatten()
    {
        return Plan::Intersect {
            branches: secondary,
        };
    }

    primary.unwrap_or(Plan::CollScan)
}

/// 构造「多索引取交集」的分支列表。
///
/// 只有当**每个字段**都能找到独立的索引、且分支数 >= 2 时才返回 `Some`。
/// `primary` 会被排除,避免与已有的复合索引计划重复。
fn intersection_plans(
    obj: &Map<String, Value>,
    sort: Option<&SortSpec>,
    indexes: &[IndexDef],
    primary: Option<&Plan>,
) -> Option<Vec<Plan>> {
    // 若某个复合索引的**全部字段**都被 filter 覆盖,它优于交集(单次扫描更省)。
    if let Some(Plan::IndexScan { index, .. }) = primary
        && index.fields.len() >= 2
        && index.fields.iter().all(|f| obj.contains_key(f))
    {
        return None;
    }

    let mut branches: Vec<Plan> = Vec::new();
    // 只看顶层字段条件(逻辑组合不参与交集)
    for (field, value) in obj.iter().filter(|(k, _)| !k.starts_with('$')) {
        // 找以该字段开头的索引(单字段,或复合前缀取首字段)
        let Some(def) = indexes.iter().find(|d| d.fields.first() == Some(field)) else {
            continue; // 该字段无索引,跳过(不影响交集正确性)
        };

        let mut single = Map::new();
        single.insert(field.clone(), value.clone());
        // 分支内禁止再构造交集(否则无限递归)
        let p = plan_conjunction_inner(
            &Value::Object(single),
            sort,
            std::slice::from_ref(def),
            false,
        );
        if !p.is_index_scan() {
            return None;
        }
        branches.push(p);
    }

    (branches.len() >= 2).then_some(branches)
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

/// 排序规格能否直接由索引顺序满足(M27 起**必须比对方向**)。
///
/// 条件:
/// 1. 排序字段是索引字段的**前缀**(顺序相同);
/// 2. 索引方向必须单一(全 `1` 或全 `-1`)—— 混向索引无法用一个扫描方向
///    同时满足,保守返回 `false`(照常做内存排序,结果依然正确);
/// 3. 索引是升序时排序须全升序;索引是降序时排序须全降序。
///
/// 判定错误会直接导致**返回顺序错**,所以这里宁可保守。
fn sort_matches_prefix(sort: &SortSpec, index: &IndexDef) -> bool {
    if !index.is_uniform_direction() {
        return false;
    }
    let sort_paths = sort.field_paths();
    if sort_paths.len() > index.fields.len() {
        return false;
    }
    // ① 字段路径逐级匹配
    let fields_ok = sort_paths
        .iter()
        .zip(index.fields.iter())
        .all(|(s, f)| *s == f.split('.').collect::<Vec<&str>>().as_slice());
    if !fields_ok {
        return false;
    }
    // ② 方向必须与索引整体方向一致
    let idx_dir: i8 = if index.is_descending() { -1 } else { 1 };
    sort.direction_signs().iter().all(|d| *d == idx_dir)
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
        Plan::Union { branches } => {
            out.insert("stage".to_string(), Value::String("OR".to_string()));
            out.insert("collection".to_string(), Value::String(coll.to_string()));
            out.insert(
                "branches".to_string(),
                Value::Array(
                    branches
                        .iter()
                        .map(|b| explain_plan(b, coll).into())
                        .collect(),
                ),
            );
        }
        Plan::Intersect { branches } => {
            out.insert("stage".to_string(), Value::String("AND".to_string()));
            out.insert("collection".to_string(), Value::String(coll.to_string()));
            out.insert(
                "branches".to_string(),
                Value::Array(
                    branches
                        .iter()
                        .map(|b| explain_plan(b, coll).into())
                        .collect(),
                ),
            );
        }
    }
    out
}
