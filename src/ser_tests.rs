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
    assert_eq!(serialize(&true), vec![0x6f]);
    assert_eq!(serialize(&false), vec![0x6e]);
    assert_eq!(serialize(&42i32), vec![0x2a]);
    assert_eq!(serialize(&"hello"), vec![0x85, b'h', b'e', b'l', b'l', b'o']);
}

#[test]
fn test_serialize_option() {
    assert_eq!(serialize(&None::<i32>), vec![0x6d]);
    assert_eq!(serialize(&Some(42i32)), vec![0x2a]);
}

#[test]
fn test_serialize_vec() {
    assert_eq!(serialize(&vec![1, 2, 3]), vec![0x99, 0x01, 0x02, 0x03, 0x9b]);
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
    // {"x": 1, "y": 2}
    assert_eq!(
        bytes,
        vec![0x9a, 0x81, b'x', 0x01, 0x81, b'y', 0x02, 0x9b]
    );
}
