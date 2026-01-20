// ABOUTME: Unit tests for the BONJSON deserializer module.
// ABOUTME: Tests serde integration, attributes, enums, nested structures.

use crate::de::from_slice;
use serde::Deserialize;

#[test]
fn test_deserialize_primitives() {
    // true: 0xf7, false: 0xf6
    assert!(from_slice::<bool>(&[0xf7]).unwrap());
    assert!(!from_slice::<bool>(&[0xf6]).unwrap());
    // 42 as small int: 42 + 100 = 142 = 0x8e
    assert_eq!(from_slice::<i32>(&[0x8e]).unwrap(), 42);
    // "hello" (5 chars): 0xe5 + bytes
    assert_eq!(
        from_slice::<String>(&[0xe5, b'h', b'e', b'l', b'l', b'o']).unwrap(),
        "hello"
    );
}

#[test]
fn test_deserialize_option() {
    // null: 0xf5
    assert_eq!(from_slice::<Option<i32>>(&[0xf5]).unwrap(), None);
    // 42 as small int: 0x8e
    assert_eq!(from_slice::<Option<i32>>(&[0x8e]).unwrap(), Some(42));
}

#[test]
fn test_deserialize_vec() {
    // [1, 2, 3]: array 0xf8 + chunk(count=3) 0x0c + elements 0x65 0x66 0x67
    assert_eq!(
        from_slice::<Vec<i32>>(&[0xf8, 0x0c, 0x65, 0x66, 0x67]).unwrap(),
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

    // {"x": 1, "y": 2}: object 0xf9 + chunk(count=2) 0x08 + "x" + 1 + "y" + 2
    let bytes = vec![0xf9, 0x08, 0xe1, b'x', 0x65, 0xe1, b'y', 0x66];
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

    // "Red" (3 chars): 0xe3 + bytes
    let bytes = vec![0xe3, b'R', b'e', b'd'];
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

    // {"firstName": "Alice"}
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

    // {"name": "test"} - missing count field
    let bytes = crate::to_vec(&serde_json::json!({"name": "test"})).unwrap();
    let result: Config = from_slice(&bytes).unwrap();
    assert_eq!(result.name, "test");
    assert_eq!(result.count, 0); // default value
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

    // Even if skipped is in the data, it should use default
    let bytes = crate::to_vec(&serde_json::json!({"name": "test", "skipped": 99})).unwrap();
    let result: Data = from_slice(&bytes).unwrap();
    assert_eq!(result.name, "test");
    assert_eq!(result.skipped, 0); // default, not 99
}

#[test]
fn test_serde_alias() {
    #[derive(Debug, Deserialize, PartialEq)]
    struct Data {
        #[serde(alias = "nm")]
        name: String,
    }

    // Using alias "nm" instead of "name"
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

    // Flattened: {"name": "point", "x": 1, "y": 2}
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

    // {"Int": 42}
    let bytes = crate::to_vec(&serde_json::json!({"Int": 42})).unwrap();
    assert_eq!(from_slice::<Value>(&bytes).unwrap(), Value::Int(42));

    // {"Text": "hello"}
    let bytes = crate::to_vec(&serde_json::json!({"Text": "hello"})).unwrap();
    assert_eq!(from_slice::<Value>(&bytes).unwrap(), Value::Text("hello".to_string()));
}

// Note: Tuple variants have a known issue with roundtrip deserialization.
// The encoding is correct ({variant: [values...]}) but deserialization
// has a trailing bytes error. This needs further investigation.

#[test]
fn test_enum_struct_variant() {
    #[derive(Debug, Deserialize, PartialEq)]
    enum Shape {
        Circle { radius: f64 },
        Rectangle { width: f64, height: f64 },
    }

    // {"Circle": {"radius": 5.0}}
    let bytes = crate::to_vec(&serde_json::json!({"Circle": {"radius": 5.0}})).unwrap();
    assert_eq!(from_slice::<Shape>(&bytes).unwrap(), Shape::Circle { radius: 5.0 });

    // {"Rectangle": {"width": 10.0, "height": 20.0}}
    let bytes = crate::to_vec(&serde_json::json!({"Rectangle": {"width": 10.0, "height": 20.0}})).unwrap();
    assert_eq!(from_slice::<Shape>(&bytes).unwrap(), Shape::Rectangle { width: 10.0, height: 20.0 });
}

// =========================================================================
// Nested Option tests
// =========================================================================

#[test]
fn test_nested_option() {
    // Option<Option<i32>>
    // Note: serde serializes Some(None) as null, which deserializes to None.
    // This is expected serde behavior - Some(None) and None both become null.

    let bytes = crate::to_vec(&Some(Some(42))).unwrap();
    assert_eq!(from_slice::<Option<Option<i32>>>(&bytes).unwrap(), Some(Some(42)));

    // Both Some(None) and None serialize to null and deserialize to None
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

    // String containing NUL: "a\0b" (3 chars): 0xe3 + bytes
    let bytes = vec![0xe3, b'a', 0x00, b'b'];

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
