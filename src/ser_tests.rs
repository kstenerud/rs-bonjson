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
    // true: 0xb5, false: 0xb4
    assert_eq!(serialize(&true), vec![0xb5]);
    assert_eq!(serialize(&false), vec![0xb4]);
    // 42 as small int: 42 = 0x2a
    assert_eq!(serialize(&42i32), vec![0x2a]);
    // "hello" (5 chars): 0x6a + bytes
    assert_eq!(serialize(&"hello"), vec![0x6a, b'h', b'e', b'l', b'l', b'o']);
}

#[test]
fn test_serialize_option() {
    // null: 0xb3
    assert_eq!(serialize(&None::<i32>), vec![0xb3]);
    // 42 as small int: 0x2a
    assert_eq!(serialize(&Some(42i32)), vec![0x2a]);
}

#[test]
fn test_serialize_vec() {
    // [1, 2, 3]: 0xb7 + elements + 0xb6
    assert_eq!(serialize(&vec![1, 2, 3]), vec![0xb7, 0x01, 0x02, 0x03, 0xb6]);
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
    // {"x": 1, "y": 2}: 0xb8 + "x" + 1 + "y" + 2 + 0xb6
    // "x": 0x66 0x78, "y": 0x66 0x79
    assert_eq!(
        bytes,
        vec![0xb8, 0x66, b'x', 0x01, 0x66, b'y', 0x02, 0xb6]
    );
}
