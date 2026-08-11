//! 文档模型模块测试(M2)。
//!
//! 覆盖 `Document` 的创建/解析/序列化、`_id` 主键规则,以及 ObjectId 生成。

use nosqlite::document::ObjectId;
use nosqlite::Document;

/// 空文档创建后为空 JSON 对象。
#[test]
fn new_document_is_empty_object() {
    let doc = Document::new();
    assert_eq!(doc.to_bytes().unwrap(), b"{}");
}

/// 从合法 JSON 对象创建文档。
#[test]
fn from_json_valid_object() {
    let doc = Document::from_json(r#"{"name":"张三","age":30,"tags":["a","b"]}"#).expect("解析应成功");
    assert_eq!(doc.get("name").unwrap().as_str(), Some("张三"));
    assert_eq!(doc.get("age").unwrap().as_i64(), Some(30));
    assert!(doc.get("tags").unwrap().is_array());
}

/// 顶层非对象(数组/标量)应报 NotAnObject 错误。
#[test]
fn from_json_rejects_non_object() {
    for bad in [r#"[1,2,3]"#, r#""hello""#, r#"42"#, "null"] {
        let err = Document::from_json(bad).expect_err("非对象顶层应报错");
        assert!(
            matches!(err, nosqlite::DocumentError::NotAnObject),
            "输入 {} 应报 NotAnObject",
            bad
        );
    }
}

/// 非法 JSON 应报 Json 错误。
#[test]
fn from_json_rejects_bad_json() {
    let err = Document::from_json("{not valid").expect_err("非法 JSON 应报错");
    assert!(
        matches!(err, nosqlite::DocumentError::Json(_)),
        "应报 Json 错误"
    );
}

/// 序列化与反序列化互逆(字节往返一致)。
#[test]
fn to_bytes_from_bytes_roundtrip() {
    let doc = Document::from_json(r#"{"_id":"abc","nested":{"x":1}}"#).unwrap();
    let bytes = doc.to_bytes().unwrap();
    let back = Document::from_bytes(&bytes).unwrap();
    assert_eq!(doc, back);
}

/// 无 `_id` 时 ensure_id 自动生成并回填。
#[test]
fn ensure_id_generates_when_missing() {
    let mut doc = Document::from_json(r#"{"name":"张三"}"#).unwrap();
    let id = doc.ensure_id().unwrap();
    assert_eq!(id.len(), 24, "自动生成的 _id 应为 24 字符");
    assert_eq!(doc.get_id().unwrap(), Some(id));
}

/// 已有 `_id`(字符串)时 ensure_id 返回原值,不覆盖。
#[test]
fn ensure_id_keeps_existing() {
    let mut doc = Document::from_json(r#"{"_id":"my_custom_id","name":"张三"}"#).unwrap();
    let id = doc.ensure_id().unwrap();
    assert_eq!(id, "my_custom_id");
    assert_eq!(doc.get_id().unwrap(), Some("my_custom_id".to_string()));
}

/// `_id` 存在但不是字符串时 ensure_id 应报 IdNotString。
#[test]
fn ensure_id_rejects_non_string_id() {
    for bad in [r#"{"_id":123}"#, r#"{"_id":{"x":1}}"#] {
        let mut doc = Document::from_json(bad).unwrap();
        let err = doc.ensure_id().expect_err("非字符串 _id 应报错");
        assert!(
            matches!(err, nosqlite::DocumentError::IdNotString),
            "输入 {} 应报 IdNotString",
            bad
        );
    }
}

/// ObjectId 生成:24 字符小写 hex。
#[test]
fn object_id_format() {
    let id = ObjectId::generate();
    assert_eq!(id.len(), 24);
    assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(id, id.to_lowercase());
}

/// ObjectId 生成:批量唯一。
#[test]
fn object_id_unique_in_batch() {
    let mut set = std::collections::HashSet::new();
    for _ in 0..1000 {
        assert!(set.insert(ObjectId::generate()));
    }
}
