// ABOUTME: Unit tests for the BONJSON serializer module.
// ABOUTME: Tests serde integration including typed arrays, records, and fallback behavior.

use crate::encoder::Encoder;
use crate::ser::{Serializer, SerializerConfig};
use crate::types::type_code;
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

fn serialize_with_config<T: Serialize>(value: &T, config: SerializerConfig) -> Vec<u8> {
    crate::to_vec_with_config(value, &config).unwrap()
}

fn serialize_no_typed_arrays<T: Serialize>(value: &T) -> Vec<u8> {
    let config = SerializerConfig {
        typed_arrays: false,
        ..Default::default()
    };
    serialize_with_config(value, config)
}

#[test]
fn test_serialize_primitives() {
    assert_eq!(serialize(&true), vec![0xb5]);
    assert_eq!(serialize(&false), vec![0xb4]);
    assert_eq!(serialize(&42i32), vec![0x2a]);
    assert_eq!(
        serialize(&"hello"),
        vec![0x6a, b'h', b'e', b'l', b'l', b'o']
    );
}

#[test]
fn test_serialize_option() {
    assert_eq!(serialize(&None::<i32>), vec![0xb3]);
    assert_eq!(serialize(&Some(42i32)), vec![0x2a]);
}

#[test]
fn test_serialize_vec_small_ints_regular() {
    // [1, 2, 3] as Vec<i32>: small ints 1-byte each → regular array is smaller
    // Regular: B7 01 02 03 B6 = 5 bytes
    // Typed SINT32: F8 03 01000000 02000000 03000000 = 1+1+12 = 14 bytes
    // Size comparison picks regular
    let bytes = serialize(&vec![1i32, 2, 3]);
    assert_eq!(bytes, vec![0xb7, 0x01, 0x02, 0x03, 0xb6]);
}

#[test]
fn test_serialize_vec_typed_arrays_disabled() {
    // With typed arrays disabled, always use regular array
    let bytes = serialize_no_typed_arrays(&vec![1000i32, 2000, 3000]);
    // Regular array: B7 + elements + B6
    assert_eq!(bytes[0], type_code::ARRAY);
    assert_eq!(*bytes.last().unwrap(), type_code::CONTAINER_END);
}

#[test]
fn test_serialize_vec_f64_typed_array() {
    // Vec<f64> with values that need f64: typed array should be emitted
    // Regular: B7 + 3 * (1+8) + B6 = 1 + 27 + 1 = 29 bytes
    // Typed FLOAT64: F5 03 + 3*8 = 1 + 1 + 24 = 26 bytes → typed is smaller
    let values = vec![1.1f64, 2.2, 3.3];
    let bytes = serialize(&values);
    assert_eq!(bytes[0], type_code::TYPED_ARRAY_FLOAT64);
    // LEB128(3) = 0x03
    assert_eq!(bytes[1], 0x03);
    // 3 * 8 = 24 bytes of float data
    assert_eq!(bytes.len(), 1 + 1 + 3 * 8);
    // Verify roundtrip
    let decoded: Vec<f64> = crate::from_slice(&bytes).unwrap();
    assert_eq!(decoded, values);
}

#[test]
fn test_serialize_vec_f32_typed_array() {
    // Vec<f32>: typed array should be emitted when beneficial
    // Regular: B7 + 3 * (1+4) + B6 = 1 + 15 + 1 = 17 bytes
    // Typed FLOAT32: F6 03 + 3*4 = 1 + 1 + 12 = 14 bytes → typed is smaller
    let values = vec![1.5f32, 2.5, 3.5];
    let bytes = serialize(&values);
    assert_eq!(bytes[0], type_code::TYPED_ARRAY_FLOAT32);
    let decoded: Vec<f32> = crate::from_slice(&bytes).unwrap();
    assert_eq!(decoded, values);
}

#[test]
fn test_serialize_vec_large_i32_typed_array() {
    // Values in the sint32 range that are expensive in regular encoding:
    // i32::MAX (2147483647) → needs sint64 in regular (1+8=9 bytes), or sint32 typed (4 bytes)
    // Regular: 1 + 10*9 + 1 = 92 bytes
    // Typed SINT32: 1 + 1 + 10*4 = 42 bytes → typed wins
    let values = vec![i32::MAX; 10];
    let bytes = serialize(&values);
    assert_eq!(bytes[0], type_code::TYPED_ARRAY_SINT32);
    let decoded: Vec<i32> = crate::from_slice(&bytes).unwrap();
    assert_eq!(decoded, values);
}

#[test]
fn test_serialize_vec_u8_typed_array() {
    // Vec<u8> with values > 100: typed array should win
    // Each 200 as uint8: 2 bytes regular. Typed UINT8: 1 byte each.
    // 10 elements: Regular = 1 + 10*2 + 1 = 22. Typed = 1 + 1 + 10 = 12.
    let values = vec![200u8; 10];
    let bytes = serialize(&values);
    assert_eq!(bytes[0], type_code::TYPED_ARRAY_UINT8);
    let decoded: Vec<u8> = crate::from_slice(&bytes).unwrap();
    assert_eq!(decoded, values);
}

#[test]
fn test_serialize_bytes_typed_uint8() {
    // serialize_bytes should always emit typed uint8 array
    #[derive(Serialize)]
    struct ByteData(#[serde(serialize_with = "serialize_as_bytes")] Vec<u8>);

    fn serialize_as_bytes<S: serde::Serializer>(
        data: &[u8],
        s: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_bytes(data)
    }

    let data = ByteData(vec![0u8, 127, 255]);
    let bytes = serialize(&data);
    assert_eq!(bytes[0], type_code::TYPED_ARRAY_UINT8);
    assert_eq!(bytes[1], 0x03); // count = 3
    assert_eq!(&bytes[2..5], &[0, 127, 255]);
}

#[test]
fn test_serialize_mixed_type_fallback() {
    // A sequence with mixed types should fall back to regular array
    #[derive(Serialize)]
    struct MixedArray {
        #[serde(serialize_with = "serialize_mixed")]
        items: (),
    }

    fn serialize_mixed<S: serde::Serializer>(_: &(), s: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let mut seq = s.serialize_seq(Some(3))?;
        seq.serialize_element(&1i32)?;
        seq.serialize_element(&"hello")?;
        seq.serialize_element(&true)?;
        seq.end()
    }

    let value = MixedArray { items: () };
    let bytes = serialize(&value);
    // The sequence should be a regular array inside an object
    // Object: B8 ... B6
    assert_eq!(bytes[0], type_code::OBJECT);
}

#[test]
fn test_serialize_empty_seq() {
    let bytes = serialize(&Vec::<i32>::new());
    // Empty array: B7 B6
    assert_eq!(bytes, vec![type_code::ARRAY, type_code::CONTAINER_END]);
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
    // {"x": 1, "y": 2}: B8 + "x" + 1 + "y" + 2 + B6
    assert_eq!(
        bytes,
        vec![0xb8, 0x66, b'x', 0x01, 0x66, b'y', 0x02, 0xb6]
    );
}

#[test]
fn test_serialize_struct_with_vec_field() {
    // Ensure structs containing vecs still work
    #[derive(Serialize, serde::Deserialize, PartialEq, Debug)]
    struct Data {
        values: Vec<f64>,
    }

    let data = Data {
        values: vec![1.1, 2.2, 3.3],
    };
    let bytes = serialize(&data);
    let decoded: Data = crate::from_slice(&bytes).unwrap();
    assert_eq!(decoded, data);
}

#[test]
fn test_serialize_nested_vecs() {
    // Vec<Vec<i32>> — outer is non-numeric (elements are arrays), falls back to regular
    let data = vec![vec![1i32, 2], vec![3, 4]];
    let bytes = serialize(&data);
    let decoded: Vec<Vec<i32>> = crate::from_slice(&bytes).unwrap();
    assert_eq!(decoded, data);
}

#[test]
fn test_serialize_option_vec() {
    // Vec<Option<i32>> — None serializes as unit (non-numeric), Some as i32
    // First element Some(1) → I32, second element None → non-numeric → fallback
    let data = vec![Some(1i32), None, Some(3)];
    let bytes = serialize(&data);
    let decoded: Vec<Option<i32>> = crate::from_slice(&bytes).unwrap();
    assert_eq!(decoded, data);
}

#[test]
fn test_records_single_struct() {
    // A single struct (appears once) should NOT emit a record definition
    #[derive(Serialize, serde::Deserialize, PartialEq, Debug)]
    struct Point {
        x: i32,
        y: i32,
    }

    let config = SerializerConfig {
        records: true,
        ..Default::default()
    };
    let p = Point { x: 1, y: 2 };
    let bytes = serialize_with_config(&p, config);
    // Should be a regular object, not a record
    assert_eq!(bytes[0], type_code::OBJECT);
    let decoded: Point = crate::from_slice(&bytes).unwrap();
    assert_eq!(decoded, p);
}

#[test]
fn test_records_repeated_struct() {
    // Two instances of the same struct → record definition + instances
    #[derive(Serialize, serde::Deserialize, PartialEq, Debug)]
    struct Point {
        x: i32,
        y: i32,
    }

    let config = SerializerConfig {
        records: true,
        ..Default::default()
    };
    let data = vec![Point { x: 1, y: 2 }, Point { x: 3, y: 4 }];
    let bytes = serialize_with_config(&data, config);

    // Should start with a record definition (B9)
    assert_eq!(bytes[0], type_code::RECORD_DEF);

    // Should roundtrip correctly
    let decoded: Vec<Point> = crate::from_slice(&bytes).unwrap();
    assert_eq!(decoded, data);
}

#[test]
fn test_records_disabled_by_default() {
    #[derive(Serialize, serde::Deserialize, PartialEq, Debug)]
    struct Point {
        x: i32,
        y: i32,
    }

    let data = vec![Point { x: 1, y: 2 }, Point { x: 3, y: 4 }];
    let bytes = serialize(&data);
    // Default config has records disabled — should NOT start with record def
    assert_ne!(bytes[0], type_code::RECORD_DEF);
    let decoded: Vec<Point> = crate::from_slice(&bytes).unwrap();
    assert_eq!(decoded, data);
}

#[test]
fn test_records_smaller_than_regular() {
    // Verify records actually produce smaller output for repeated structs
    #[derive(Serialize, serde::Deserialize, PartialEq, Debug)]
    struct Person {
        name: String,
        age: i32,
        active: bool,
    }

    let data: Vec<Person> = (0..10)
        .map(|i| Person {
            name: format!("person_{i}"),
            age: 20 + i,
            active: true,
        })
        .collect();

    let regular_bytes = serialize_with_config(
        &data,
        SerializerConfig {
            records: false,
            ..Default::default()
        },
    );
    let record_bytes = serialize_with_config(
        &data,
        SerializerConfig {
            records: true,
            ..Default::default()
        },
    );

    // Records should be smaller (saves repeating key strings)
    assert!(
        record_bytes.len() < regular_bytes.len(),
        "records ({}) should be smaller than regular ({})",
        record_bytes.len(),
        regular_bytes.len()
    );

    // Both should roundtrip correctly
    let decoded_regular: Vec<Person> = crate::from_slice(&regular_bytes).unwrap();
    let decoded_record: Vec<Person> = crate::from_slice(&record_bytes).unwrap();
    assert_eq!(decoded_regular, data);
    assert_eq!(decoded_record, data);
}

#[test]
fn test_typed_array_and_records_together() {
    // Both optimizations working together
    #[derive(Serialize, serde::Deserialize, PartialEq, Debug)]
    struct Measurement {
        timestamp: f64,
        value: f64,
    }

    let config = SerializerConfig {
        typed_arrays: true,
        records: true,
    };

    let data = vec![
        Measurement {
            timestamp: 1000.0,
            value: 42.5,
        },
        Measurement {
            timestamp: 2000.0,
            value: 43.5,
        },
        Measurement {
            timestamp: 3000.0,
            value: 44.5,
        },
    ];

    let bytes = serialize_with_config(&data, config);
    // Should have record definition
    assert_eq!(bytes[0], type_code::RECORD_DEF);

    let decoded: Vec<Measurement> = crate::from_slice(&bytes).unwrap();
    assert_eq!(decoded, data);
}

#[test]
fn test_roundtrip_vec_i64() {
    let values: Vec<i64> = vec![-1000, 0, 1000, i64::MIN, i64::MAX];
    let bytes = serialize(&values);
    let decoded: Vec<i64> = crate::from_slice(&bytes).unwrap();
    assert_eq!(decoded, values);
}

#[test]
fn test_roundtrip_vec_u64() {
    let values: Vec<u64> = vec![0, 100, 1000, u64::MAX];
    let bytes = serialize(&values);
    let decoded: Vec<u64> = crate::from_slice(&bytes).unwrap();
    assert_eq!(decoded, values);
}
