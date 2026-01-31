// ABOUTME: Unit tests for the BONJSON deserializer module.
// ABOUTME: Tests serde integration, attributes, enums, nested structures.

use crate::de::from_slice;
use serde::Deserialize;

#[test]
fn test_deserialize_primitives() {
    // true: 0xcf, false: 0xce
    assert!(from_slice::<bool>(&[0xcf]).unwrap());
    assert!(!from_slice::<bool>(&[0xce]).unwrap());
    // 42 as small int: 42 + 100 = 142 = 0x8e
    assert_eq!(from_slice::<i32>(&[0x8e]).unwrap(), 42);
    // "hello" (5 chars): 0xd5 + bytes
    assert_eq!(
        from_slice::<String>(&[0xd5, b'h', b'e', b'l', b'l', b'o']).unwrap(),
        "hello"
    );
}

#[test]
fn test_deserialize_option() {
    // null: 0xcd
    assert_eq!(from_slice::<Option<i32>>(&[0xcd]).unwrap(), None);
    // 42 as small int: 0x8e
    assert_eq!(from_slice::<Option<i32>>(&[0x8e]).unwrap(), Some(42));
}

/// Test null values inside containers (regression test for container state tracking).
#[test]
fn test_null_in_containers() {
    let nulls: Vec<Option<i32>> = vec![None, None];
    let bytes = crate::to_vec(&nulls).unwrap();
    assert_eq!(from_slice::<Vec<Option<i32>>>(&bytes).unwrap(), nulls);

    let mixed: Vec<Option<i32>> = vec![Some(1), None, Some(2), None];
    let bytes = crate::to_vec(&mixed).unwrap();
    assert_eq!(from_slice::<Vec<Option<i32>>>(&bytes).unwrap(), mixed);

    #[derive(Debug, serde::Serialize, Deserialize, PartialEq)]
    struct Data {
        a: String,
        b: Option<i32>,
    }

    let data = vec![
        Data { a: "x".to_string(), b: None },
        Data { a: "y".to_string(), b: None },
    ];
    let bytes = crate::to_vec(&data).unwrap();
    assert_eq!(from_slice::<Vec<Data>>(&bytes).unwrap(), data);

    let nested: Vec<Vec<Option<i32>>> = vec![vec![None, None], vec![None]];
    let bytes = crate::to_vec(&nested).unwrap();
    assert_eq!(from_slice::<Vec<Vec<Option<i32>>>>(&bytes).unwrap(), nested);
}

#[test]
fn test_deserialize_vec() {
    // [1, 2, 3]: FC + elements + FE
    assert_eq!(
        from_slice::<Vec<i32>>(&[0xfc, 0x65, 0x66, 0x67, 0xfe]).unwrap(),
        vec![1, 2, 3]
    );
}

#[test]
fn test_deserialize_struct() {
    #[derive(Debug, Deserialize, PartialEq)]
    struct Point {
        x: i32,
        y: i32,
    }

    // {"x": 1, "y": 2}: FD + "x" + 1 + "y" + 2 + FE
    let bytes = vec![0xfd, 0xd1, b'x', 0x65, 0xd1, b'y', 0x66, 0xfe];
    assert_eq!(from_slice::<Point>(&bytes).unwrap(), Point { x: 1, y: 2 });
}

#[test]
fn test_deserialize_enum() {
    #[derive(Debug, Deserialize, PartialEq)]
    enum Color {
        Red,
        Green,
        Blue,
    }

    // "Red" (3 chars): 0xd3 + bytes
    let bytes = vec![0xd3, b'R', b'e', b'd'];
    assert_eq!(from_slice::<Color>(&bytes).unwrap(), Color::Red);
}

// =========================================================================
// Serde attribute tests
// =========================================================================

#[test]
fn test_serde_rename() {
    #[derive(Debug, Deserialize, PartialEq)]
    struct Data {
        #[serde(rename = "firstName")]
        first_name: String,
    }

    let bytes = crate::to_vec(&serde_json::json!({"firstName": "Alice"})).unwrap();
    let result: Data = from_slice(&bytes).unwrap();
    assert_eq!(result.first_name, "Alice");
}

#[test]
fn test_serde_rename_all() {
    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(rename_all = "camelCase")]
    struct Person {
        first_name: String,
        last_name: String,
    }

    let bytes = crate::to_vec(&serde_json::json!({
        "firstName": "Alice",
        "lastName": "Smith"
    })).unwrap();
    let result: Person = from_slice(&bytes).unwrap();
    assert_eq!(result.first_name, "Alice");
    assert_eq!(result.last_name, "Smith");
}

#[test]
fn test_serde_default() {
    #[derive(Debug, Deserialize, PartialEq)]
    struct Config {
        name: String,
        #[serde(default)]
        count: i32,
    }

    let bytes = crate::to_vec(&serde_json::json!({"name": "test"})).unwrap();
    let result: Config = from_slice(&bytes).unwrap();
    assert_eq!(result.name, "test");
    assert_eq!(result.count, 0);
}

#[test]
fn test_serde_default_with_value() {
    fn default_count() -> i32 { 42 }

    #[derive(Debug, Deserialize, PartialEq)]
    struct Config {
        name: String,
        #[serde(default = "default_count")]
        count: i32,
    }

    let bytes = crate::to_vec(&serde_json::json!({"name": "test"})).unwrap();
    let result: Config = from_slice(&bytes).unwrap();
    assert_eq!(result.count, 42);
}

#[test]
fn test_serde_skip_deserializing() {
    #[derive(Debug, Deserialize, PartialEq, Default)]
    struct Data {
        name: String,
        #[serde(skip_deserializing)]
        skipped: i32,
    }

    let bytes = crate::to_vec(&serde_json::json!({"name": "test", "skipped": 99})).unwrap();
    let result: Data = from_slice(&bytes).unwrap();
    assert_eq!(result.name, "test");
    assert_eq!(result.skipped, 0);
}

#[test]
fn test_serde_alias() {
    #[derive(Debug, Deserialize, PartialEq)]
    struct Data {
        #[serde(alias = "nm")]
        name: String,
    }

    let bytes = crate::to_vec(&serde_json::json!({"nm": "Alice"})).unwrap();
    let result: Data = from_slice(&bytes).unwrap();
    assert_eq!(result.name, "Alice");
}

#[test]
fn test_serde_flatten() {
    #[derive(Debug, Deserialize, PartialEq)]
    struct Inner {
        x: i32,
        y: i32,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct Outer {
        name: String,
        #[serde(flatten)]
        inner: Inner,
    }

    let bytes = crate::to_vec(&serde_json::json!({
        "name": "point",
        "x": 1,
        "y": 2
    })).unwrap();
    let result: Outer = from_slice(&bytes).unwrap();
    assert_eq!(result.name, "point");
    assert_eq!(result.inner.x, 1);
    assert_eq!(result.inner.y, 2);
}

// =========================================================================
// Enum variant tests
// =========================================================================

#[test]
fn test_enum_unit_variant() {
    #[derive(Debug, Deserialize, PartialEq)]
    enum Status {
        Active,
        Inactive,
    }

    let bytes = crate::to_vec(&"Active").unwrap();
    assert_eq!(from_slice::<Status>(&bytes).unwrap(), Status::Active);
}

#[test]
fn test_enum_newtype_variant() {
    #[derive(Debug, Deserialize, PartialEq)]
    enum Value {
        Int(i32),
        Text(String),
    }

    let bytes = crate::to_vec(&serde_json::json!({"Int": 42})).unwrap();
    assert_eq!(from_slice::<Value>(&bytes).unwrap(), Value::Int(42));

    let bytes = crate::to_vec(&serde_json::json!({"Text": "hello"})).unwrap();
    assert_eq!(from_slice::<Value>(&bytes).unwrap(), Value::Text("hello".to_string()));
}

#[test]
fn test_enum_struct_variant() {
    #[derive(Debug, Deserialize, PartialEq)]
    enum Shape {
        Circle { radius: f64 },
        Rectangle { width: f64, height: f64 },
    }

    let bytes = crate::to_vec(&serde_json::json!({"Circle": {"radius": 5.0}})).unwrap();
    assert_eq!(from_slice::<Shape>(&bytes).unwrap(), Shape::Circle { radius: 5.0 });

    let bytes = crate::to_vec(&serde_json::json!({"Rectangle": {"width": 10.0, "height": 20.0}})).unwrap();
    assert_eq!(from_slice::<Shape>(&bytes).unwrap(), Shape::Rectangle { width: 10.0, height: 20.0 });
}

// =========================================================================
// Nested Option tests
// =========================================================================

#[test]
fn test_nested_option() {
    let bytes = crate::to_vec(&Some(Some(42))).unwrap();
    assert_eq!(from_slice::<Option<Option<i32>>>(&bytes).unwrap(), Some(Some(42)));

    let bytes = crate::to_vec(&None::<Option<i32>>).unwrap();
    assert_eq!(from_slice::<Option<Option<i32>>>(&bytes).unwrap(), None);
}

// =========================================================================
// Complex nested structure tests
// =========================================================================

#[test]
fn test_complex_nested_structure() {
    #[derive(Debug, Deserialize, PartialEq)]
    struct Address {
        city: String,
        zip: String,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct Person {
        name: String,
        age: u32,
        addresses: Vec<Address>,
    }

    let bytes = crate::to_vec(&serde_json::json!({
        "name": "Alice",
        "age": 30,
        "addresses": [
            {"city": "NYC", "zip": "10001"},
            {"city": "LA", "zip": "90001"}
        ]
    })).unwrap();

    let result: Person = from_slice(&bytes).unwrap();
    assert_eq!(result.name, "Alice");
    assert_eq!(result.age, 30);
    assert_eq!(result.addresses.len(), 2);
    assert_eq!(result.addresses[0].city, "NYC");
}

// =========================================================================
// DecoderConfig in deserialization tests
// =========================================================================

#[test]
fn test_from_slice_with_config_allow_nul() {
    use crate::decoder::DecoderConfig;

    // String containing NUL: "a\0b" (3 chars): 0xd3 + bytes
    let bytes = vec![0xd3, b'a', 0x00, b'b'];

    // Default config should fail
    assert!(from_slice::<String>(&bytes).is_err());

    // With allow_nul should succeed
    let config = DecoderConfig {
        allow_nul: true,
        ..Default::default()
    };
    let result: String = crate::from_slice_with_config(&bytes, config).unwrap();
    assert_eq!(result, "a\0b");
}
