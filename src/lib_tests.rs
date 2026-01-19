// ABOUTME: Integration tests for the BONJSON library.
// ABOUTME: Tests roundtrip encoding/decoding and the bonjson! macro.

use crate::{bonjson, decode_value, encode_value, from_slice, to_vec};
use serde::{Deserialize, Serialize};

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
    // Example from the spec: {"number": 50, ...}
    // Just test the "number": 50 part
    // 9a 86 6e 75 6d 62 65 72 32 9b
    // object_start, "number" (6 chars), 50, object_end
    let bytes = vec![
        0x9a, // object start
        0x86, b'n', b'u', b'm', b'b', b'e', b'r', // "number"
        0x32, // 50
        0x9b, // container end
    ];

    let value = decode_value(&bytes).unwrap();
    assert!(value.is_object());
    assert_eq!(value.get_key("number").and_then(|v| v.as_i64()), Some(50));
}
