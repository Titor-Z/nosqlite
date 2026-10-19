//! BSON 官方类型序(v0.4.1 修正)。
//!
//! MongoDB 的 `bson-type-order`:
//!
//! ```text
//! MinKey < Null/缺失 < Number < Symbol/String < Object < Array
//!        < BinData < ObjectId < Boolean < Date < Timestamp < Regex < MaxKey
//! ```
//!
//! 这个序是**查询比较与索引编码共用的单一真相源**。一旦两者不一致,
//! 范围查询在走索引时就会静默返回错误结果 —— 所以本文件把顺序完整锁死。

use std::cmp::Ordering::Less;

use nosqlite::query::{Filter, SortSpec, as_timestamp, compare_values, type_rank};
use serde_json::{Value, json};

/// 便捷比较。
fn cmp(a: Value, b: Value) -> std::cmp::Ordering {
    compare_values(Some(&a), Some(&b))
}

/// 完整类型序:逐级递增,每一对相邻类型都必须满足「前者 < 后者」。
#[test]
fn full_bson_type_order() {
    // 按 BSON 官方序排列的样本值
    let samples: Vec<(&str, Value)> = vec![
        ("MinKey", json!({"$minKey": 1})),
        ("Null", json!(null)),
        ("Number", json!(1)),
        ("String", json!("a")),
        ("Object", json!({"k": 1})),
        ("Array", json!([1])),
        (
            "Binary",
            json!({"$binary": {"base64": "AA==", "subType": "00"}}),
        ),
        ("Boolean", json!(true)),
        ("Date", json!({"$date": 0})),
        ("Timestamp", json!({"$timestamp": {"t": 0, "i": 0}})),
        (
            "Regex",
            json!({"$regularExpression": {"pattern": "a", "options": ""}}),
        ),
        ("MaxKey", json!({"$maxKey": 1})),
    ];

    for pair in samples.windows(2) {
        let (a_name, a) = &pair[0];
        let (b_name, b) = &pair[1];
        assert_eq!(
            cmp(a.clone(), b.clone()),
            Less,
            "类型序错误:{} 应小于 {}",
            a_name,
            b_name
        );
        // 反向也必须成立
        assert_eq!(
            cmp(b.clone(), a.clone()),
            std::cmp::Ordering::Greater,
            "类型序错误:{} 应大于 {}",
            b_name,
            a_name
        );
    }
}

/// 缺失字段按 null 处理(与 MongoDB 一致)。
#[test]
fn missing_equals_null_rank() {
    assert_eq!(type_rank(None), type_rank(Some(&json!(null))));
    assert_eq!(
        compare_values(None, Some(&json!(null))),
        std::cmp::Ordering::Equal
    );
    // 缺失与 null 都小于数值
    assert_eq!(
        compare_values(None, Some(&json!(1))),
        std::cmp::Ordering::Less
    );
}

/// 同类型内部比较不受影响(回调兼容性)。
#[test]
fn same_type_comparisons_unchanged() {
    use std::cmp::Ordering::{Equal, Greater};
    // number
    assert_eq!(cmp(json!(1), json!(1.0)), Equal);
    assert_eq!(cmp(json!(2), json!(1)), Greater);
    // string
    assert_eq!(cmp(json!("b"), json!("a")), Greater);
    // bool:false < true
    assert_eq!(cmp(json!(false), json!(true)), Less);
    // date
    assert_eq!(cmp(json!({"$date": 1}), json!({"$date": 2})), Less);
}

/// Timestamp 按 (t, i) 元组比较。
#[test]
fn timestamp_ordering() {
    use std::cmp::Ordering::{Equal, Greater};
    let a = json!({"$timestamp": {"t": 100, "i": 1}});
    let b = json!({"$timestamp": {"t": 100, "i": 2}});
    let c = json!({"$timestamp": {"t": 101, "i": 0}});

    assert_eq!(cmp(a.clone(), b.clone()), Less, "同秒比自增序号");
    assert_eq!(cmp(b.clone(), c.clone()), Less, "秒数优先");
    assert_eq!(cmp(a.clone(), a), Equal);
    assert_eq!(cmp(c, b), Greater);

    // as_timestamp 解析
    assert_eq!(
        as_timestamp(&json!({"$timestamp": {"t": 5, "i": 7}})),
        Some((5, 7))
    );
}

/// MinKey/MaxKey 恒为端点(可用于 `{$gt: MinKey}` 这类全量查询)。
#[test]
fn min_max_key_are_extremes() {
    let min = json!({"$minKey": 1});
    let max = json!({"$maxKey": 1});
    for v in [json!(null), json!(1), json!("s"), json!([1]), json!(true)] {
        assert_eq!(cmp(min.clone(), v.clone()), Less, "MinKey 应最小");
        assert_eq!(
            cmp(max.clone(), v),
            std::cmp::Ordering::Greater,
            "MaxKey 应最大"
        );
    }
    // 两个 MinKey 相等
    assert_eq!(cmp(min.clone(), min), std::cmp::Ordering::Equal);
}

/// 跨类型范围查询:`{$gt: true}` 只应命中 bool 之后(date/timestamp/... )。
#[test]
fn range_query_across_types_uses_bson_order() {
    // $gt true:命中 date(9)及之后,不应命中 number/string/object/array
    let filter = Filter::from_value(&json!({"v": {"$gt": true}})).unwrap();
    let hit = |v: Value| filter.matches(v.as_object().unwrap());

    assert!(hit(json!({"v": {"$date": 0}})), "date 应命中(9 > 8)");
    assert!(!hit(json!({"v": 100})), "number 不应命中(2 < 8)");
    assert!(!hit(json!({"v": "z"})), "string 不应命中(3 < 8)");
    assert!(!hit(json!({"v": [1, 2, 3]})), "array 不应命中(5 < 8)");
    assert!(!hit(json!({"v": {"k": 1}})), "object 不应命中(4 < 8)");
    assert!(!hit(json!({"v": false})), "false 不应命中(同类型但更小)");
}

/// 排序按 BSON 序:混合类型值排序结果确定且符合规范。
#[test]
fn sort_mixed_types_follows_bson_order() {
    let spec = SortSpec::from_value(&json!({"v": 1})).unwrap();
    let mut docs: Vec<_> = vec![
        json!({"v": {"$date": 0}}),
        json!({"v": 1}),
        json!({"v": "a"}),
        json!({"v": true}),
        json!({"v": [1]}),
        json!({"v": {"k": 1}}),
        json!({"v": null}),
    ]
    .into_iter()
    .map(|v| v.as_object().unwrap().clone())
    .collect();
    spec.sort(&mut docs);

    let got: Vec<Value> = docs.iter().map(|d| d["v"].clone()).collect();
    let want = vec![
        json!(null),         // NULL(1)
        json!(1),            // NUMBER(2)
        json!("a"),          // STRING(3)
        json!({"k": 1}),     // OBJECT(4)
        json!([1]),          // ARRAY(5)
        json!(true),         // BOOL(8)
        json!({"$date": 0}), // DATE(9)
    ];
    assert_eq!(got, want);
}

/// 等级常量与 test 期望对应(防止常量表被误改)。
#[test]
fn rank_constants_are_stable() {
    use nosqlite::query::compare::rank;
    assert_eq!(rank::MIN_KEY, 0);
    assert_eq!(rank::NULL, 1);
    assert_eq!(rank::NUMBER, 2);
    assert_eq!(rank::STRING, 3);
    assert_eq!(rank::OBJECT, 4);
    assert_eq!(rank::ARRAY, 5);
    assert_eq!(rank::BINARY, 6);
    assert_eq!(rank::OBJECT_ID, 7);
    assert_eq!(rank::BOOL, 8);
    assert_eq!(rank::DATE, 9);
    assert_eq!(rank::TIMESTAMP, 10);
    assert_eq!(rank::REGEX, 11);
    assert_eq!(rank::MAX_KEY, 12);
}

/// `$oid` 归一为字符串,等级应等同 STRING。
#[test]
fn oid_ranks_as_string() {
    use nosqlite::query::compare::rank;
    assert_eq!(
        type_rank(Some(&json!({"$oid": "6ad54cd9bcd4e32cb134e111"}))),
        rank::STRING
    );
}

/// `$numberLong` 归一为数字,等级应等同 NUMBER。
#[test]
fn number_long_ranks_as_number() {
    use nosqlite::query::compare::rank;
    assert_eq!(type_rank(Some(&json!({"$numberLong": "42"}))), rank::NUMBER);
    assert_eq!(
        type_rank(Some(&json!({"$numberDouble": "1.5"}))),
        rank::NUMBER
    );
}
