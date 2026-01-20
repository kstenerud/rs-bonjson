// ABOUTME: Unit tests for the BONJSON serializer module.
// ABOUTME: Tests serde integration for serializing Rust types to BONJSON.

use crate::encoder::Encoder;
use crate::ser::Serializer;
use serde::Serialize;

fn serialize<T: Serialize>(value: &T) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut encoder = Encoder::new(&mut buf);
    {
        let mut serializer = Serializer::new(&mut encoder);
        value.serialize(&mut serializer).unwrap();
    }
    encoder.finish().unwrap();
    buf
}

#[test]
fn test_serialize_primitives() {
    // true: 0xf7, false: 0xf6
    assert_eq!(serialize(&true), vec![0xf7]);
    assert_eq!(serialize(&false), vec![0xf6]);
    // 42 as small int: 42 + 100 = 142 = 0x8e
    assert_eq!(serialize(&42i32), vec![0x8e]);
    // "hello" (5 chars): 0xe5 + bytes
    assert_eq!(serialize(&"hello"), vec![0xe5, b'h', b'e', b'l', b'l', b'o']);
}

#[test]
fn test_serialize_option() {
    // null: 0xf5
    assert_eq!(serialize(&None::<i32>), vec![0xf5]);
    // 42 as small int: 0x8e
    assert_eq!(serialize(&Some(42i32)), vec![0x8e]);
}

#[test]
fn test_serialize_vec() {
    // [1, 2, 3]:
    // - array: 0xf8
    // - chunk header (count=3, cont=0): payload=6, encoded=0x0c
    // - elements: 0x65 (1), 0x66 (2), 0x67 (3)
    assert_eq!(serialize(&vec![1, 2, 3]), vec![0xf8, 0x0c, 0x65, 0x66, 0x67]);
}

#[test]
fn test_serialize_struct() {
    #[derive(Serialize)]
    struct Point {
        x: i32,
        y: i32,
    }

    let p = Point { x: 1, y: 2 };
    let bytes = serialize(&p);
    // {"x": 1, "y": 2}:
    // - object: 0xf9
    // - chunk header (count=2, cont=0): payload=4, encoded=0x08
    // - "x": 0xe1 0x78
    // - 1: 0x65
    // - "y": 0xe1 0x79
    // - 2: 0x66
    assert_eq!(
        bytes,
        vec![0xf9, 0x08, 0xe1, b'x', 0x65, 0xe1, b'y', 0x66]
    );
}
