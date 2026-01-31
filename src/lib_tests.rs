// ABOUTME: Integration tests for the BONJSON library.
// ABOUTME: Tests roundtrip encoding/decoding and the bonjson! macro.

use crate::{
    bonjson, decode_value, encode_value, from_reader, from_reader_with_config, from_slice,
    from_value, json, to_value, to_vec, DecoderConfig, Map, Value,
};
use serde::{Deserialize, Serialize};
use std::io::Cursor;

#[test]
fn test_roundtrip_primitives() {
    // Integer
    let bytes = to_vec(&42i32).unwrap();
    let decoded: i32 = from_slice(&bytes).unwrap();
    assert_eq!(decoded, 42);

    // String
    let bytes = to_vec(&"hello").unwrap();
    let decoded: String = from_slice(&bytes).unwrap();
    assert_eq!(decoded, "hello");

    // Bool
    let bytes = to_vec(&true).unwrap();
    let decoded: bool = from_slice(&bytes).unwrap();
    assert!(decoded);
}

#[test]
fn test_roundtrip_containers() {
    // Vec
    let original = vec![1, 2, 3, 4, 5];
    let bytes = to_vec(&original).unwrap();
    let decoded: Vec<i32> = from_slice(&bytes).unwrap();
    assert_eq!(decoded, original);

    // Nested
    let original = vec![vec![1, 2], vec![3, 4]];
    let bytes = to_vec(&original).unwrap();
    let decoded: Vec<Vec<i32>> = from_slice(&bytes).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn test_roundtrip_struct() {
    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Person {
        name: String,
        age: u32,
        active: bool,
    }

    let original = Person {
        name: "Alice".to_string(),
        age: 30,
        active: true,
    };

    let bytes = to_vec(&original).unwrap();
    let decoded: Person = from_slice(&bytes).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn test_value_roundtrip() {
    let value = bonjson!({
        "name": "test",
        "values": [1, 2, 3],
        "nested": {
            "flag": true
        }
    });

    let bytes = encode_value(&value).unwrap();
    let decoded = decode_value(&bytes).unwrap();
    assert_eq!(decoded, value);
}

#[test]
fn test_decode_spec_example() {
    // Example: {"number": 50}
    // - object: 0xfd
    // - "number" (6 chars): 0xd6 + bytes
    // - 50 as small int: 50 + 100 = 150 = 0x96
    // - end marker: 0xfe
    let bytes = vec![
        0xfd, // object
        0xd6, b'n', b'u', b'm', b'b', b'e', b'r', // "number"
        0x96, // 50
        0xfe, // end marker
    ];

    let value = decode_value(&bytes).unwrap();
    assert!(value.is_object());
    assert_eq!(value.get_key("number").and_then(|v| v.as_i64()), Some(50));
}

#[test]
fn test_from_reader() {
    // Test with Cursor (in-memory reader)
    let bytes = to_vec(&42i32).unwrap();
    let reader = Cursor::new(bytes);
    let decoded: i32 = from_reader(reader).unwrap();
    assert_eq!(decoded, 42);

    // Test with a struct
    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Data {
        name: String,
        value: i32,
    }

    let original = Data {
        name: "test".to_string(),
        value: 123,
    };
    let bytes = to_vec(&original).unwrap();
    let reader = Cursor::new(bytes);
    let decoded: Data = from_reader(reader).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn test_from_reader_with_config() {
    let bytes = to_vec(&vec![1, 2, 3]).unwrap();
    let reader = Cursor::new(bytes);

    let config = DecoderConfig::default();
    let decoded: Vec<i32> = from_reader_with_config(reader, config).unwrap();
    assert_eq!(decoded, vec![1, 2, 3]);
}

#[test]
fn test_to_value() {
    // Test with primitives
    let value = to_value(&42i32).unwrap();
    assert_eq!(value.as_i64(), Some(42));

    let value = to_value(&"hello").unwrap();
    assert_eq!(value.as_str(), Some("hello"));

    let value = to_value(&true).unwrap();
    assert_eq!(value.as_bool(), Some(true));

    // Test with a struct
    #[derive(Serialize)]
    struct Person {
        name: String,
        age: u32,
    }

    let person = Person {
        name: "Alice".to_string(),
        age: 30,
    };
    let value = to_value(&person).unwrap();
    assert!(value.is_object());
    assert_eq!(value.get_key("name").and_then(|v| v.as_str()), Some("Alice"));
    assert_eq!(value.get_key("age").and_then(|v| v.as_i64()), Some(30));

    // Test with a vec
    let value = to_value(&vec![1, 2, 3]).unwrap();
    assert!(value.is_array());
    let arr = value.as_array().unwrap();
    assert_eq!(arr.len(), 3);
}

#[test]
fn test_from_value() {
    // Test with primitives
    let value = Value::Int(42);
    let decoded: i32 = from_value(&value).unwrap();
    assert_eq!(decoded, 42);

    let value = Value::String("hello".to_string());
    let decoded: String = from_value(&value).unwrap();
    assert_eq!(decoded, "hello");

    let value = Value::Bool(true);
    let decoded: bool = from_value(&value).unwrap();
    assert!(decoded);

    // Test with a struct
    #[derive(Debug, Deserialize, PartialEq)]
    struct Person {
        name: String,
        age: u32,
    }

    let value = bonjson!({
        "name": "Bob",
        "age": 25
    });
    let person: Person = from_value(&value).unwrap();
    assert_eq!(
        person,
        Person {
            name: "Bob".to_string(),
            age: 25
        }
    );

    // Test with a vec
    let value = bonjson!([1, 2, 3]);
    let decoded: Vec<i32> = from_value(&value).unwrap();
    assert_eq!(decoded, vec![1, 2, 3]);
}

#[test]
fn test_to_value_from_value_roundtrip() {
    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Complex {
        id: u64,
        name: String,
        tags: Vec<String>,
        active: bool,
    }

    let original = Complex {
        id: 12345,
        name: "Test".to_string(),
        tags: vec!["a".to_string(), "b".to_string()],
        active: true,
    };

    let value = to_value(&original).unwrap();
    let decoded: Complex = from_value(&value).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn test_map_type_alias() {
    // Verify Map is usable as a type alias for BTreeMap
    let mut map: Map<String, Value> = Map::new();
    map.insert("key".to_string(), Value::Int(42));
    map.insert("name".to_string(), Value::String("test".to_string()));

    assert_eq!(map.len(), 2);
    assert_eq!(map.get("key"), Some(&Value::Int(42)));

    // Can be used in Value::Object
    let value = Value::Object(map);
    assert!(value.is_object());
    assert_eq!(value.get_key("key").and_then(|v| v.as_i64()), Some(42));
}

#[test]
fn test_json_macro_alias() {
    // Verify json! macro works the same as bonjson!
    let value1 = json!({
        "name": "test",
        "values": [1, 2, 3]
    });

    let value2 = bonjson!({
        "name": "test",
        "values": [1, 2, 3]
    });

    assert_eq!(value1, value2);

    // Test all forms
    assert_eq!(json!(null), bonjson!(null));
    assert_eq!(json!(true), bonjson!(true));
    assert_eq!(json!(false), bonjson!(false));
    assert_eq!(json!(42), bonjson!(42));
    assert_eq!(json!("hello"), bonjson!("hello"));
    assert_eq!(json!([1, 2, 3]), bonjson!([1, 2, 3]));
}
