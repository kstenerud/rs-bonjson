// ABOUTME: Unit tests for the BONJSON value module.
// ABOUTME: Tests Value type, accessors, conversions, and the bonjson! macro.

use crate::{bonjson, Value};
use std::collections::BTreeMap;

#[test]
fn test_value_types() {
    assert!(Value::Null.is_null());
    assert!(Value::Bool(true).is_bool());
    assert!(Value::Int(42).is_number());
    assert!(Value::Float(2.5).is_number());
    assert!(Value::String("hello".into()).is_string());
    assert!(Value::Array(vec![]).is_array());
    assert!(Value::Object(BTreeMap::new()).is_object());
}

#[test]
fn test_value_accessors() {
    assert_eq!(Value::Bool(true).as_bool(), Some(true));
    assert_eq!(Value::Int(42).as_i64(), Some(42));
    assert_eq!(Value::UInt(100).as_u64(), Some(100));
    assert_eq!(Value::Float(2.5).as_f64(), Some(2.5));
    assert_eq!(Value::String("hello".into()).as_str(), Some("hello"));
}

#[test]
fn test_value_from() {
    let v: Value = 42i32.into();
    assert_eq!(v.as_i64(), Some(42));

    let v: Value = "hello".into();
    assert_eq!(v.as_str(), Some("hello"));

    let v: Value = vec![1, 2, 3].into();
    assert!(v.is_array());
}

#[test]
fn test_bonjson_macro() {
    let v = bonjson!(null);
    assert!(v.is_null());

    let v = bonjson!([1, 2, 3]);
    assert!(v.is_array());
    assert_eq!(v.get(0).and_then(|v| v.as_i64()), Some(1));

    let v = bonjson!({
        "name": "test",
        "value": 42
    });
    assert!(v.is_object());
    assert_eq!(v.get_key("name").and_then(|v| v.as_str()), Some("test"));
}
