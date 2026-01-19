// ABOUTME: BONJSON (Binary Object Notation for JSON) encoder/decoder for Rust.
// ABOUTME: Provides serde integration and a serde_json-like API for encoding/decoding.

//! # BONJSON
//!
//! A high-performance BONJSON (Binary Object Notation for JSON) encoder and decoder for Rust.
//!
//! BONJSON is a binary format that is 1:1 compatible with JSON but faster to process
//! and more compact. It's designed to take advantage of modern CPU intrinsics.
//!
//! ## Quick Start
//!
//! ```rust
//! use bonjson::{to_vec, from_slice};
//! use serde::{Serialize, Deserialize};
//!
//! #[derive(Serialize, Deserialize, Debug, PartialEq)]
//! struct Person {
//!     name: String,
//!     age: u32,
//! }
//!
//! let person = Person {
//!     name: "Alice".to_string(),
//!     age: 30,
//! };
//!
//! // Serialize to BONJSON
//! let bytes = to_vec(&person).unwrap();
//!
//! // Deserialize from BONJSON
//! let decoded: Person = from_slice(&bytes).unwrap();
//! assert_eq!(person, decoded);
//! ```
//!
//! ## Working with Dynamic Values
//!
//! ```rust
//! use bonjson::{Value, bonjson};
//!
//! // Create values with the macro
//! let value = bonjson!({
//!     "name": "test",
//!     "values": [1, 2, 3],
//!     "active": true
//! });
//!
//! // Access fields
//! assert_eq!(value.get_key("name").and_then(|v| v.as_str()), Some("test"));
//! ```
//!
//! ## Compliance
//!
//! This implementation provides **basic compliance** per the BONJSON specification:
//! - UTF-8 validation is performed on decode
//! - Duplicate key detection uses byte-for-byte comparison
//! - Unicode normalization is NOT performed (see spec for security implications)
//!
//! ## Resource Limits
//!
//! Default limits per the BONJSON specification:
//! - Maximum document size: 2 GB
//! - Maximum nesting depth: 512
//! - Maximum container size: 1,000,000 elements
//! - Maximum string length: 10 MB
//! - Maximum chunks per string: 100

pub mod de;
pub mod decoder;
pub mod encoder;
pub mod error;
pub mod ser;
pub mod types;
pub mod value;

// Re-export commonly used items at the crate root
pub use de::{from_slice, from_slice_with_config, Deserializer};
pub use decoder::{DecodedValue, Decoder, DecoderConfig, DuplicateKeyMode};
pub use encoder::Encoder;
pub use error::{Error, Result};
pub use ser::Serializer;
pub use types::{limits, type_code, BigNumber};
pub use value::Value;

// The bonjson! macro is automatically exported at crate root via #[macro_export]

use serde::{Deserialize, Serialize};
use std::io::Write;

/// Serialize a value to a BONJSON byte vector.
///
/// # Example
///
/// ```rust
/// use bonjson::to_vec;
///
/// let bytes = to_vec(&42i32).unwrap();
/// assert_eq!(bytes, vec![0x2a]); // Small integer 42
/// ```
pub fn to_vec<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    to_writer(&mut buf, value)?;
    Ok(buf)
}

/// Serialize a value to a writer.
///
/// # Example
///
/// ```rust
/// use bonjson::to_writer;
///
/// let mut buf = Vec::new();
/// to_writer(&mut buf, &"hello").unwrap();
/// ```
pub fn to_writer<W: Write, T: Serialize>(writer: W, value: &T) -> Result<()> {
    let mut encoder = Encoder::new(writer);
    {
        let mut serializer = Serializer::new(&mut encoder);
        value.serialize(&mut serializer)?;
    }
    encoder.finish()?;
    Ok(())
}

/// Decode a BONJSON document into a `Value`.
///
/// # Example
///
/// ```rust
/// use bonjson::{decode_value, Value};
///
/// let bytes = vec![0x99, 0x01, 0x02, 0x03, 0x9b]; // [1, 2, 3]
/// let value = decode_value(&bytes).unwrap();
/// assert!(value.is_array());
/// ```
pub fn decode_value(data: &[u8]) -> Result<Value> {
    let mut decoder = Decoder::new(data);
    let value = decode_value_recursive(&mut decoder)?;
    decoder.finish()?;
    Ok(value)
}

/// Decode a BONJSON document into a `Value` with custom configuration.
pub fn decode_value_with_config(data: &[u8], config: DecoderConfig) -> Result<Value> {
    let mut decoder = Decoder::with_config(data, config);
    let value = decode_value_recursive(&mut decoder)?;
    decoder.finish()?;
    Ok(value)
}

fn decode_value_recursive(decoder: &mut Decoder<'_>) -> Result<Value> {
    use decoder::DuplicateKeyMode;

    match decoder.decode_value()? {
        DecodedValue::Null => Ok(Value::Null),
        DecodedValue::Bool(b) => Ok(Value::Bool(b)),
        DecodedValue::Int(n) => Ok(Value::Int(n)),
        DecodedValue::UInt(n) => Ok(Value::UInt(n)),
        DecodedValue::Float(f) => Ok(Value::Float(f)),
        DecodedValue::BigNumber(bn) => Ok(Value::BigNumber(bn)),
        DecodedValue::String(s) => Ok(Value::String(s.to_owned())),
        DecodedValue::ArrayStart => {
            let mut arr = Vec::new();
            loop {
                // Peek at next value to check for container end
                let next = decoder.decode_value()?;
                if matches!(next, DecodedValue::ContainerEnd) {
                    break;
                }
                // We need to handle the value we just decoded
                arr.push(decode_value_from_decoded(next, decoder)?);
            }
            Ok(Value::Array(arr))
        }
        DecodedValue::ObjectStart => {
            let dup_mode = decoder.config().duplicate_key_mode;
            let mut map = std::collections::BTreeMap::new();
            loop {
                let key_value = decoder.decode_value()?;
                if matches!(key_value, DecodedValue::ContainerEnd) {
                    break;
                }
                let key = match key_value {
                    DecodedValue::String(s) => s.to_owned(),
                    _ => return Err(Error::ExpectedObjectKey),
                };
                let value = decode_value_recursive(decoder)?;
                // Check for duplicate key
                if map.contains_key(&key) {
                    match dup_mode {
                        DuplicateKeyMode::Error => return Err(Error::DuplicateKey),
                        DuplicateKeyMode::KeepFirst => {
                            // Skip this value, keep the original
                            continue;
                        }
                        DuplicateKeyMode::KeepLast => {
                            // Fall through to insert (will overwrite)
                        }
                    }
                }
                map.insert(key, value);
            }
            Ok(Value::Object(map))
        }
        DecodedValue::ContainerEnd => Err(Error::UnbalancedContainers),
    }
}

fn decode_value_from_decoded(decoded: DecodedValue<'_>, decoder: &mut Decoder<'_>) -> Result<Value> {
    use decoder::DuplicateKeyMode;

    match decoded {
        DecodedValue::Null => Ok(Value::Null),
        DecodedValue::Bool(b) => Ok(Value::Bool(b)),
        DecodedValue::Int(n) => Ok(Value::Int(n)),
        DecodedValue::UInt(n) => Ok(Value::UInt(n)),
        DecodedValue::Float(f) => Ok(Value::Float(f)),
        DecodedValue::BigNumber(bn) => Ok(Value::BigNumber(bn)),
        DecodedValue::String(s) => Ok(Value::String(s.to_owned())),
        DecodedValue::ArrayStart => {
            let mut arr = Vec::new();
            loop {
                let next = decoder.decode_value()?;
                if matches!(next, DecodedValue::ContainerEnd) {
                    break;
                }
                arr.push(decode_value_from_decoded(next, decoder)?);
            }
            Ok(Value::Array(arr))
        }
        DecodedValue::ObjectStart => {
            let dup_mode = decoder.config().duplicate_key_mode;
            let mut map = std::collections::BTreeMap::new();
            loop {
                let key_value = decoder.decode_value()?;
                if matches!(key_value, DecodedValue::ContainerEnd) {
                    break;
                }
                let key = match key_value {
                    DecodedValue::String(s) => s.to_owned(),
                    _ => return Err(Error::ExpectedObjectKey),
                };
                let value = decode_value_recursive(decoder)?;
                // Check for duplicate key
                if map.contains_key(&key) {
                    match dup_mode {
                        DuplicateKeyMode::Error => return Err(Error::DuplicateKey),
                        DuplicateKeyMode::KeepFirst => continue,
                        DuplicateKeyMode::KeepLast => {}
                    }
                }
                map.insert(key, value);
            }
            Ok(Value::Object(map))
        }
        DecodedValue::ContainerEnd => Err(Error::UnbalancedContainers),
    }
}

/// Encode a `Value` to BONJSON bytes.
///
/// # Example
///
/// ```rust
/// use bonjson::{encode_value, Value};
///
/// let value = Value::Int(42);
/// let bytes = encode_value(&value).unwrap();
/// assert_eq!(bytes, vec![0x2a]);
/// ```
pub fn encode_value(value: &Value) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    encode_value_to_writer(&mut buf, value)?;
    Ok(buf)
}

/// Encode a `Value` to a writer.
pub fn encode_value_to_writer<W: Write>(writer: W, value: &Value) -> Result<()> {
    let mut encoder = Encoder::new(writer);
    encode_value_recursive(&mut encoder, value)?;
    encoder.finish()?;
    Ok(())
}

fn encode_value_recursive<W: Write>(encoder: &mut Encoder<W>, value: &Value) -> Result<()> {
    match value {
        Value::Null => encoder.write_null(),
        Value::Bool(b) => encoder.write_bool(*b),
        Value::Int(n) => encoder.write_i64(*n),
        Value::UInt(n) => encoder.write_u64(*n),
        Value::Float(f) => encoder.write_f64(*f),
        Value::BigNumber(bn) => encoder.write_big_number(*bn),
        Value::String(s) => encoder.write_str(s),
        Value::Array(arr) => {
            encoder.begin_array()?;
            for item in arr {
                encode_value_recursive(encoder, item)?;
            }
            encoder.end_container()
        }
        Value::Object(map) => {
            encoder.begin_object()?;
            for (key, val) in map {
                encoder.write_str(key)?;
                encode_value_recursive(encoder, val)?;
            }
            encoder.end_container()
        }
    }
}

// Implement Serialize for Value
impl Serialize for Value {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        match self {
            Value::Null => serializer.serialize_unit(),
            Value::Bool(b) => serializer.serialize_bool(*b),
            Value::Int(n) => serializer.serialize_i64(*n),
            Value::UInt(n) => serializer.serialize_u64(*n),
            Value::Float(f) => serializer.serialize_f64(*f),
            Value::BigNumber(bn) => {
                // Serialize BigNumber as f64 for compatibility
                serializer.serialize_f64(bn.to_f64())
            }
            Value::String(s) => serializer.serialize_str(s),
            Value::Array(arr) => {
                use serde::ser::SerializeSeq;
                let mut seq = serializer.serialize_seq(Some(arr.len()))?;
                for item in arr {
                    seq.serialize_element(item)?;
                }
                seq.end()
            }
            Value::Object(map) => {
                use serde::ser::SerializeMap;
                let mut m = serializer.serialize_map(Some(map.len()))?;
                for (key, val) in map {
                    m.serialize_entry(key, val)?;
                }
                m.end()
            }
        }
    }
}

// Implement Deserialize for Value
impl<'de> Deserialize<'de> for Value {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct ValueVisitor;

        impl<'de> serde::de::Visitor<'de> for ValueVisitor {
            type Value = Value;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "any valid BONJSON value")
            }

            fn visit_bool<E>(self, v: bool) -> std::result::Result<Value, E> {
                Ok(Value::Bool(v))
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Value, E> {
                Ok(Value::Int(v))
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Value, E> {
                if v <= i64::MAX as u64 {
                    Ok(Value::Int(v as i64))
                } else {
                    Ok(Value::UInt(v))
                }
            }

            fn visit_f64<E>(self, v: f64) -> std::result::Result<Value, E> {
                Ok(Value::Float(v))
            }

            fn visit_str<E>(self, v: &str) -> std::result::Result<Value, E> {
                Ok(Value::String(v.to_owned()))
            }

            fn visit_string<E>(self, v: String) -> std::result::Result<Value, E> {
                Ok(Value::String(v))
            }

            fn visit_unit<E>(self) -> std::result::Result<Value, E> {
                Ok(Value::Null)
            }

            fn visit_none<E>(self) -> std::result::Result<Value, E> {
                Ok(Value::Null)
            }

            fn visit_some<D: serde::Deserializer<'de>>(
                self,
                deserializer: D,
            ) -> std::result::Result<Value, D::Error> {
                Deserialize::deserialize(deserializer)
            }

            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> std::result::Result<Value, A::Error> {
                let mut arr = Vec::new();
                while let Some(elem) = seq.next_element()? {
                    arr.push(elem);
                }
                Ok(Value::Array(arr))
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<Value, A::Error> {
                let mut obj = std::collections::BTreeMap::new();
                while let Some((key, val)) = map.next_entry()? {
                    obj.insert(key, val);
                }
                Ok(Value::Object(obj))
            }
        }

        deserializer.deserialize_any(ValueVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
