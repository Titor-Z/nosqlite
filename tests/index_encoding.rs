//! 索引键编码正确性(M23b)。
//!
//! # 铁律
//!
//! **对任意两个值,`compare_values(a, b)` 的结果必须等于 `encode_key(a).cmp(encode_key(b))`。**
//!
//! 这是整个索引方案的根基:两者不一致,范围查询走索引就会静默返回错误结果
//! (全表扫和走索引给出不同答案,且不报错)。本文件用穷举两两组合锁死它。

use nosqlite::index::key::{encode_index_values, encode_key, is_indexable};
use nosqlite::query::compare_values;
use serde_json::{Value, json};

/// 用于两两校验的样本值(覆盖全部 BSON 类型 + 边界值)。
fn samples() -> Vec<(&'static str, Value)> {
    vec![
        ("MinKey", json!({"$minKey": 1})),
        ("Null", json!(null)),
        ("Num/-1e9", json!(-1_000_000_000)),
        ("Num/-1.5", json!(-1.5)),
        ("Num/-1", json!(-1)),
        ("Num/0", json!(0)),
        ("Num/1", json!(1)),
        ("Num/1.0", json!(1.0)),
        ("Num/1.5", json!(1.5)),
        ("Num/1e9", json!(1_000_000_000)),
        ("NumLong", json!({"$numberLong": "42"})),
        ("NumDouble", json!({"$numberDouble": "3.25"})),
        ("Str/", json!("")),
        ("Str/a", json!("a")),
        ("Str/ab", json!("ab")),
        ("Str/b", json!("b")),
        ("Obj/{}", json!({})),
        ("Obj/a", json!({"a": 1})),
        ("Obj/b", json!({"b": 1})),
        ("Arr/[]", json!([])),
        ("Arr/1", json!([1])),
        ("Arr/12", json!([1, 2])),
        ("Arr/2", json!([2])),
        (
            "Bin",
            json!({"$binary": {"base64": "AA==", "subType": "00"}}),
        ),
        ("Oid1", json!({"$oid": "000000000000000000000001"})),
        ("Oid2", json!({"$oid": "000000000000000000000002"})),
        ("Bool/false", json!(false)),
        ("Bool/true", json!(true)),
        ("Date/0", json!({"$date": 0})),
        ("Date/1", json!({"$date": 1})),
        ("Date/-1", json!({"$date": -1})),
        ("Ts/0,0", json!({"$timestamp": {"t": 0, "i": 0}})),
        ("Ts/0,1", json!({"$timestamp": {"t": 0, "i": 1}})),
        ("Ts/1,0", json!({"$timestamp": {"t": 1, "i": 0}})),
        (
            "Regex/a",
            json!({"$regularExpression": {"pattern": "a", "options": ""}}),
        ),
        (
            "Regex/b",
            json!({"$regularExpression": {"pattern": "b", "options": ""}}),
        ),
        ("MaxKey", json!({"$maxKey": 1})),
    ]
}

/// **核心验收**:编码字节序 == `compare_values` 结果,穷举两两组合。
#[test]
fn encoding_order_matches_comparison() {
    let samples = samples();
    let mut failures: Vec<String> = Vec::new();

    for (i, (name_a, a)) in samples.iter().enumerate() {
        for (name_b, b) in samples.iter().skip(i) {
            let cmp_result = compare_values(Some(a), Some(b));
            let enc_result = encode_key(Some(a)).cmp(&encode_key(Some(b)));
            if cmp_result != enc_result {
                failures.push(format!(
                    "{} vs {}: compare={:?} encoding={:?}",
                    name_a, name_b, cmp_result, enc_result
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "编码序与比较序不一致({} 处):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// 完整类型序:编码后的字节序应与 BSON 官方序一致。
///
/// 注:`$oid` 在 nosqlite 里按**字符串**参与比较与编码(M23a 契约,
/// 存储层主键就是 24 位 hex 字符串),因此它出现在 STRING 等级,而非 OBJECT_ID 等级。
#[test]
fn encoding_follows_bson_type_order() {
    let ordered = vec![
        json!({"$minKey": 1}),
        json!(null),
        json!(1),
        // `$oid` 与普通字符串同等级,按字符串内容排序:
        // "000000000000000000000001" < "a"
        json!({"$oid": "000000000000000000000001"}),
        json!("a"),
        json!({"k": 1}),
        json!([1]),
        json!({"$binary": {"base64": "AA==", "subType": "00"}}),
        json!(true),
        json!({"$date": 0}),
        json!({"$timestamp": {"t": 0, "i": 0}}),
        json!({"$regularExpression": {"pattern": "a", "options": ""}}),
        json!({"$maxKey": 1}),
    ];
    for pair in ordered.windows(2) {
        assert!(
            encode_key(Some(&pair[0])) < encode_key(Some(&pair[1])),
            "编码序错误: {} 应小于 {}",
            pair[0],
            pair[1]
        );
    }
}

/// 负数与浮点数的 IEEE754 位翻转必须正确(小端 BSON 做不到这点)。
#[test]
fn negative_numbers_sort_correctly() {
    let ordered = [
        json!(-1.0e300),
        json!(-1_000_000),
        json!(-1.5),
        json!(-1),
        json!(-0.5),
        json!(0),
        json!(0.5),
        json!(1),
        json!(1.5),
        json!(1_000_000),
        json!(1.0e300),
    ];
    for pair in ordered.windows(2) {
        assert!(
            encode_key(Some(&pair[0])) < encode_key(Some(&pair[1])),
            "数值编码序错误: {} 应小于 {}",
            pair[0],
            pair[1]
        );
    }
}

/// 整型与浮点等价:`1` 与 `1.0` 编码必须相同(compare_values 认为它们相等)。
#[test]
fn int_and_float_encode_identically() {
    assert_eq!(encode_key(Some(&json!(1))), encode_key(Some(&json!(1.0))));
    assert_eq!(
        encode_key(Some(&json!({"$numberLong": "1"}))),
        encode_key(Some(&json!(1)))
    );
}

/// 字符串前缀关系:短者的编码是长者的前缀(字典序正确)。
#[test]
fn string_prefix_sorts_before_extension() {
    let mut list = vec![json!("ab"), json!(""), json!("b"), json!("a")];
    list.sort_by_key(|x| encode_key(Some(x)));
    assert_eq!(list, vec![json!(""), json!("a"), json!("ab"), json!("b")]);
}

/// 数组:短数组排在长数组前(公共前缀相等时),与 compare_values 一致。
#[test]
fn short_array_sorts_before_longer() {
    let mut list = vec![json!([1, 2]), json!([1]), json!([]), json!([2])];
    list.sort_by_key(|x| encode_key(Some(x)));
    assert_eq!(list, vec![json!([]), json!([1]), json!([1, 2]), json!([2])]);
}

/// 缺失字段编码等同 null(与比较语义一致)。
#[test]
fn missing_encodes_as_null() {
    assert_eq!(encode_key(None), encode_key(Some(&json!(null))));
}

/// 编码是确定性的:同一值多次编码结果相同。
#[test]
fn encoding_is_deterministic() {
    for (_, v) in samples() {
        assert_eq!(encode_key(Some(&v)), encode_key(Some(&v)));
    }
}

/// multikey:数组字段每个元素各生成一个键,非数组返回单键。
#[test]
fn multikey_expands_arrays() {
    // 标量 → 1 条
    assert_eq!(encode_index_values(Some(&json!("a"))).len(), 1);
    // 数组 → N 条
    let keys = encode_index_values(Some(&json!(["a", "b", "c"])));
    assert_eq!(keys.len(), 3);
    // 每一条都等于对应元素的单独编码
    assert_eq!(keys[0], encode_key(Some(&json!("a"))));
    assert_eq!(keys[2], encode_key(Some(&json!("c"))));
    // 空数组 → 0 条(multikey 语义:空数组不产生索引条目)
    assert_eq!(encode_index_values(Some(&json!([]))).len(), 0);
}

/// 稀疏索引:null 与缺失不入索引。
#[test]
fn indexable_skips_null_and_missing() {
    assert!(!is_indexable(None));
    assert!(!is_indexable(Some(&json!(null))));
    assert!(is_indexable(Some(&json!(0))));
    assert!(is_indexable(Some(&json!(false))));
    assert!(is_indexable(Some(&json!(""))));
}

/// Date 与 Timestamp 的编码序正确(含负数毫秒)。
#[test]
fn date_and_timestamp_encoding() {
    let dates = [
        json!({"$date": -1000}),
        json!({"$date": 0}),
        json!({"$date": 1000}),
    ];
    for pair in dates.windows(2) {
        assert!(encode_key(Some(&pair[0])) < encode_key(Some(&pair[1])));
    }

    let ts = [
        json!({"$timestamp": {"t": 0, "i": 0}}),
        json!({"$timestamp": {"t": 0, "i": 5}}),
        json!({"$timestamp": {"t": 1, "i": 0}}),
    ];
    for pair in ts.windows(2) {
        assert!(encode_key(Some(&pair[0])) < encode_key(Some(&pair[1])));
    }
}
