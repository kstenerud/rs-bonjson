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
    // true: 0xcf, false: 0xce
    assert_eq!(serialize(&true), vec![0xcf]);
    assert_eq!(serialize(&false), vec![0xce]);
    // 42 as small int: 42 + 100 = 142 = 0x8e
    assert_eq!(serialize(&42i32), vec![0x8e]);
    // "hello" (5 chars): 0xd5 + bytes
    assert_eq!(serialize(&"hello"), vec![0xd5, b'h', b'e', b'l', b'l', b'o']);
}

#[test]
fn test_serialize_option() {
    // null: 0xcd
    assert_eq!(serialize(&None::<i32>), vec![0xcd]);
    // 42 as small int: 0x8e
    assert_eq!(serialize(&Some(42i32)), vec![0x8e]);
}

#[test]
fn test_serialize_vec() {
    // [1, 2, 3]: FC + elements + FE
    assert_eq!(serialize(&vec![1, 2, 3]), vec![0xfc, 0x65, 0x66, 0x67, 0xfe]);
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
    // {"x": 1, "y": 2}: FD + "x" + 1 + "y" + 2 + FE
    // "x": 0xd1 0x78, "y": 0xd1 0x79
    assert_eq!(
        bytes,
        vec![0xfd, 0xd1, b'x', 0x65, 0xd1, b'y', 0x66, 0xfe]
    );
}
