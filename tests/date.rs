//! Date 类型(v0.1.3):`{"$date": ...}` 字面量的解析、比较、查询与排序。
//!
//! 采用 MongoDB Extended JSON 的写法,存储仍是普通 JSON 对象,
//! 因此 `.nodb` 格式保持 v1 —— 旧库可直接打开。

use std::path::PathBuf;

use nosqlite::query::{Filter, SortSpec, as_date, compare_values, parse_iso8601};
use nosqlite::{Database, Document};
use serde_json::{Map, Value, json};

/// 生成本次测试独占的临时 `.nodb` 路径。
fn temp_nodb(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "nosqlite_date_{}_{}_{}.nodb",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&path);
    path
}

/// 构造 filter。
fn f(value: Value) -> Filter {
    Filter::from_value(&value).expect("filter 应解析成功")
}

/// 把 JSON 转成文档 Map。
fn d(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

/// 便捷比较两个 JSON 值。
fn cmp(a: Value, b: Value) -> std::cmp::Ordering {
    compare_values(Some(&a), Some(&b))
}

// ==================== ISO-8601 解析 ====================

/// 日期部分与完整的日期时间。
#[test]
fn parse_iso_basic() {
    // 1970-01-01T00:00:00Z = 0
    assert_eq!(parse_iso8601("1970-01-01"), Some(0));
    assert_eq!(parse_iso8601("1970-01-01T00:00:00Z"), Some(0));
    // 1970-01-02 = 86400000
    assert_eq!(parse_iso8601("1970-01-02"), Some(86_400_000));
    assert_eq!(parse_iso8601("1970-01-01T01:00:00Z"), Some(3_600_000));
}

/// 小数秒按毫秒截断/补齐。
#[test]
fn parse_iso_fractional_seconds() {
    assert_eq!(parse_iso8601("1970-01-01T00:00:00.5Z"), Some(500));
    assert_eq!(parse_iso8601("1970-01-01T00:00:00.123Z"), Some(123));
    // 超过三位按毫秒截断
    assert_eq!(parse_iso8601("1970-01-01T00:00:00.1239Z"), Some(123));
    // 一位小数补足到毫秒
    assert_eq!(parse_iso8601("1970-01-01T00:00:00.1Z"), Some(100));
}

/// 时区偏移归一:`+08:00` 的时刻早于 `Z` 表示的同一墙上时间。
#[test]
fn parse_iso_timezone_offset() {
    let utc = parse_iso8601("2026-09-16T10:00:00Z").unwrap();
    // 同一墙上时间的 +08:00 比 UTC 早 8 小时
    assert_eq!(
        parse_iso8601("2026-09-16T10:00:00+08:00"),
        Some(utc - 8 * 3_600_000)
    );
    // 无冒号写法等价
    assert_eq!(
        parse_iso8601("2026-09-16T10:00:00+0800"),
        Some(utc - 8 * 3_600_000)
    );
    // 负偏移
    assert_eq!(
        parse_iso8601("2026-09-16T10:00:00-05:00"),
        Some(utc + 5 * 3_600_000)
    );
    // 无时区按 UTC
    assert_eq!(parse_iso8601("2026-09-16T10:00:00"), Some(utc));
}

/// 闰年与月末。
#[test]
fn parse_iso_leap_year() {
    assert_eq!(
        parse_iso8601("2024-02-29"),
        parse_iso8601("2024-02-29T00:00:00Z")
    );
    assert!(parse_iso8601("2024-02-29").is_some());
    // 2024-03-01 比 2024-02-29 晚一天
    assert_eq!(
        parse_iso8601("2024-03-01").unwrap() - parse_iso8601("2024-02-29").unwrap(),
        86_400_000
    );
}

/// 非法输入返回 None(交给调用方降级处理)。
#[test]
fn parse_iso_rejects_invalid() {
    for bad in [
        "",
        "not-a-date",
        "2026-13-01",
        "2026-00-01",
        "2026-09-32",
        "2026-09-16T25:00:00Z",
        "2026-09-16T10:60:00Z",
        "2026-09-16T10:00:00X",
        "2026-9-16",
        "2026-09-16T10:00:00Zjunk",
    ] {
        assert_eq!(parse_iso8601(bad), None, "应拒绝: {}", bad);
    }
}

// ==================== Date 字面量识别 ====================

/// 字符串与毫秒时间戳两种写法都识别为 Date。
#[test]
fn as_date_recognizes_both_forms() {
    assert_eq!(as_date(&json!({"$date": "1970-01-01T00:00:00Z"})), Some(0));
    assert_eq!(as_date(&json!({"$date": 0})), Some(0));
    assert_eq!(as_date(&json!({"$date": 1000})), Some(1000));
    // 负数(1970 之前)
    assert_eq!(as_date(&json!({"$date": -86_400_000})), Some(-86_400_000));
}

/// 非 Date 的对象/值不被误判。
#[test]
fn as_date_rejects_non_date() {
    // 有别的键 → 普通对象
    assert_eq!(as_date(&json!({"$date": 0, "x": 1})), None);
    // 值类型不对
    assert_eq!(as_date(&json!({"$date": true})), None);
    assert_eq!(as_date(&json!({"$date": "bad"})), None);
    // 普通对象 / 标量
    assert_eq!(as_date(&json!({"a": 1})), None);
    assert_eq!(as_date(&json!("x")), None);
    assert_eq!(as_date(&json!(1)), None);
}

// ==================== 比较与类型序 ====================

/// Date 按时间戳比较,与时区写法无关。
#[test]
fn date_comparison_normalizes_timezone() {
    use std::cmp::Ordering::*;
    // 上海 10:00 == UTC 02:00
    assert_eq!(
        cmp(
            json!({"$date": "2026-09-16T10:00:00+08:00"}),
            json!({"$date": "2026-09-16T02:00:00Z"})
        ),
        Equal
    );
    // 早于
    assert_eq!(
        cmp(
            json!({"$date": "2026-01-01"}),
            json!({"$date": "2026-01-02"})
        ),
        Less
    );
    // 晚于
    assert_eq!(
        cmp(
            json!({"$date": "2026-01-02"}),
            json!({"$date": "2026-01-01"})
        ),
        Greater
    );
}

/// 字符串形式与数字形式可以互相比较。
#[test]
fn date_string_equals_millis() {
    assert_eq!(
        cmp(
            json!({"$date": "1970-01-01T00:00:01Z"}),
            json!({"$date": 1000})
        ),
        std::cmp::Ordering::Equal
    );
}

/// 类型序:number < date < string。
#[test]
fn date_type_rank() {
    use std::cmp::Ordering::*;
    // v0.4.1 起对齐 MongoDB 官方 BSON 序:
    // number(2) < string(3) < object(4) < array(5) < bool(8) < date(9)
    assert_eq!(
        cmp(json!(999999), json!({"$date": 0})),
        Less,
        "number < date"
    );
    assert_eq!(
        cmp(json!("1970"), json!({"$date": 0})),
        Less,
        "string < date"
    );
    assert_eq!(cmp(json!([1]), json!({"$date": 0})), Less, "array < date");
    assert_eq!(cmp(json!(true), json!({"$date": 0})), Less, "bool < date");
}

/// 无法解析的 `$date` 对象降级为普通对象,不 panic。
#[test]
fn invalid_date_falls_back_to_object() {
    // 与合法 date 比较:类型等级不同时由 object 等级决定
    let bad = json!({"$date": "bad"});
    assert_eq!(as_date(&bad), None);
    // 与另一个对象比较可正常进行(不 panic,返回确定顺序)
    let _ = cmp(bad.clone(), json!({"a": 1}));
    let _ = cmp(bad.clone(), json!({"$date": 0}));
}

// ==================== 查询 ====================

/// 范围查询:时区与精度差异不影响结果。
#[test]
fn range_query_across_timezones() {
    let filter = f(json!({"at": {"$gte": {"$date": "2026-09-16T00:00:00Z"}}}));

    // 北京时间 09:00 = UTC 01:00 → 命中
    assert!(filter.matches(&d(json!({"at": {"$date": "2026-09-16T09:00:00+08:00"}}))));
    // 北京时间 07:00 = UTC 前一天 23:00 → 不命中
    assert!(!filter.matches(&d(json!({"at": {"$date": "2026-09-16T07:00:00+08:00"}}))));
}

/// 等值查询:同一时刻的不同写法互相匹配。
#[test]
fn equality_across_representations() {
    let filter = f(json!({"at": {"$date": "2026-09-16T02:00:00Z"}}));
    assert!(filter.matches(&d(json!({"at": {"$date": "2026-09-16T10:00:00+08:00"}}))));
    assert!(filter.matches(&d(json!({"at": {"$date": 1789524000000i64}}))));
}

/// Date 字面量不会被当成操作符对象而报错。
#[test]
fn date_literal_is_value_not_operator() {
    // 若被当作操作符对象会报「不支持的字段操作符: $date」
    let filter = f(json!({"at": {"$date": "2026-09-16"}}));
    assert!(filter.matches(&d(json!({"at": {"$date": "2026-09-16T00:00:00Z"}}))));
}

/// Date 也可用于 `$in` / `$elemMatch`。
#[test]
fn date_in_and_elem_match() {
    let filter = f(json!({"at": {"$in": [
        {"$date": "2026-01-01"},
        {"$date": "2026-02-01"}
    ]}}));
    assert!(filter.matches(&d(json!({"at": {"$date": "2026-02-01T00:00:00Z"}}))));
    assert!(!filter.matches(&d(json!({"at": {"$date": "2026-03-01"}}))));

    let filter = f(json!({"events": {"$elemMatch": {"at": {"$gt": {"$date": "2026-01-01"}}}}}));
    assert!(filter.matches(&d(json!({"events": [
        {"at": {"$date": "2025-01-01"}},
        {"at": {"$date": "2026-06-01"}}
    ]}))));
}

// ==================== 排序与端到端 ====================

/// 按日期字段排序:时间正确的顺序,而非字符串字典序。
#[test]
fn sort_by_date_field() {
    let spec = SortSpec::from_value(&json!({"at": 1})).unwrap();
    let mut docs: Vec<Map<String, Value>> = vec![
        // 字典序上 "2026-09-16T10:00:00+08:00" 会排在 "2026-09-16T02:00:00Z" 之后,
        // 但时间上它更早 —— 正确的排序应体现时间顺序。
        json!({"at": {"$date": "2026-09-16T10:00:00+08:00"}}),
        json!({"at": {"$date": "2026-09-16T02:00:00Z"}}),
        json!({"at": {"$date": "2026-08-01"}}),
    ]
    .into_iter()
    .map(|v| v.as_object().unwrap().clone())
    .collect();
    spec.sort(&mut docs);

    let dates: Vec<i64> = docs
        .iter()
        .map(|d| as_date(&d["at"]).expect("应为 Date"))
        .collect();
    assert_eq!(
        dates,
        vec![
            parse_iso8601("2026-08-01").unwrap(),
            parse_iso8601("2026-09-16T02:00:00Z").unwrap(),
            parse_iso8601("2026-09-16T02:00:00Z").unwrap(),
        ]
    );
}

/// 端到端:写入 → 范围查询 → 排序 → 持久化重开。
#[test]
fn end_to_end_dates_persist() {
    let path = temp_nodb("e2e");
    let db = Database::create(&path).unwrap();

    let mut docs: Vec<Document> = vec![
        r#"{"name": "a", "at": {"$date": "2026-01-15T00:00:00Z"}}"#,
        r#"{"name": "b", "at": {"$date": "2026-06-01T00:00:00Z"}}"#,
        r#"{"name": "c", "at": {"$date": "2026-12-31T00:00:00Z"}}"#,
    ]
    .into_iter()
    .map(|s| Document::from_json(s).unwrap())
    .collect();
    db.insert_many("events", &mut docs).unwrap();

    let filter = f(json!({"at": {"$gte": {"$date": "2026-05-01T00:00:00Z"}}}));
    let found = db.find("events", &filter).unwrap();
    let mut names: Vec<&str> = found.iter().map(|d| d["name"].as_str().unwrap()).collect();
    names.sort();
    assert_eq!(names, vec!["b", "c"]);

    // 重新打开后 Date 仍按时间比较
    drop(db);
    let db = Database::open(&path).unwrap();
    let found = db
        .find(
            "events",
            &f(json!({"at": {"$lt": {"$date": "2026-02-01"}}})),
        )
        .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0]["name"], json!("a"));
    // 存的就是普通 JSON 对象(格式未变)
    assert_eq!(found[0]["at"], json!({"$date": "2026-01-15T00:00:00Z"}));

    drop(db);
    let _ = std::fs::remove_file(path);
}
