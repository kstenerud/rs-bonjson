// ABOUTME: serde_bonjson - A BONJSON (Binary Object Notation for JSON) encoder/decoder.
// ABOUTME: Drop-in replacement for serde_json - just prepend "bon" to "json" in your imports.

//! # serde_bonjson
//!
//! A drop-in replacement for [`serde_json`](https://docs.rs/serde_json) that's 2x faster
//! and produces smaller payloads.
//!
//! BONJSON is a binary encoding that's 1:1 compatible with JSON's data model.
//! If you're using `serde_json`, switching is a one-line change — just prepend "bon" to "json".
//!
//! ## Migrating from serde_json
//!
//! ### Zero-Change Migration
//!
//! Alias the crate and use the [`json!`] macro for seamless migration:
//!
//! ```rust
//! use serde_bonjson as serde_json;
//! use serde_json::json;
//!
//! let value = json!({ "name": "Alice", "age": 30 });
//! let bytes = serde_json::to_vec(&value).unwrap();
//! let decoded: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
//! ```
//!
//! ### Standard Migration
//!
//! Or update imports explicitly — the API mirrors `serde_json`:
//!
//! ```text
//! // Before                                  // After
//! serde_json::to_vec(&data)                  serde_bonjson::to_vec(&data)
//! serde_json::from_slice(&bytes)             serde_bonjson::from_slice(&bytes)
//! serde_json::json!({ "key": value })        serde_bonjson::bonjson!({ "key": value })
//! serde_json::Value                          serde_bonjson::Value
//! ```
//!
//! Your existing `#[derive(Serialize, Deserialize)]` types work unchanged.
//!
//! ## Quick Start
//!
//! ```rust
//! use serde_bonjson::{to_vec, from_slice};
//! use serde::{Serialize, Deserialize};
//!
//! #[derive(Serialize, Deserialize, PartialEq, Debug)]
//! struct Person {
//!     name: String,
//!     age: u32,
//! }
//!
//! let person = Person { name: "Alice".into(), age: 30 };
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
//! When you don't know the structure at compile time, use [`Value`]:
//!
//! ```rust
//! use serde_bonjson::{Value, bonjson};
//!
//! // Build values with the bonjson! macro (just like json!)
//! let value = bonjson!({
//!     "name": "test",
//!     "values": [1, 2, 3],
//!     "active": true
//! });
//!
//! // Access fields dynamically
//! assert_eq!(value.get_key("name").and_then(|v| v.as_str()), Some("test"));
//!
//! // Encode and decode Value types
//! let bytes = serde_bonjson::encode_value(&value).unwrap();
//! let decoded = serde_bonjson::decode_value(&bytes).unwrap();
//! assert_eq!(value, decoded);
//! ```
//!
//! ## Performance Benefits
//!
//! Compared to `serde_json`:
//!
//! - **Encoding**: 2-3x faster (no string formatting)
//! - **Decoding**: 1.5-2x faster (no text parsing)
//! - **Size**: 25-50% smaller (binary integers vs ASCII digits)
//!
//! ## Configuration
//!
//! For advanced use cases, configure validation and limits via [`DecoderConfig`]:
//!
//! ```rust
//! use serde_bonjson::{from_slice_with_config, DecoderConfig};
//!
//! # let bytes = serde_bonjson::to_vec(&vec![1, 2, 3]).unwrap();
//! let mut config = DecoderConfig::default();
//! config.allow_nul = true;  // Skip NUL byte validation for trusted data
//!
//! let data: Vec<i32> = from_slice_with_config(&bytes, config).unwrap();
//! ```
//!
//! ## Resource Limits
//!
//! Default limits per the BONJSON specification:
//! - Maximum document size: 2 GB
//! - Maximum nesting depth: 512
//! - Maximum container size: 1,000,000 elements
//! - Maximum string length: 10 MB
//! - Maximum chunks per string: 100
//!
//! ## Optional Features
//!
//! ### `simd-utf8`
//!
//! Enables SIMD-accelerated UTF-8 validation using the [`simdutf8`](https://docs.rs/simdutf8) crate.
//! This can improve decoding performance for workloads with large strings or Unicode-heavy content:
//!
//! - **Large strings (400+ bytes)**: ~5-10% faster
//! - **Unicode-heavy content**: ~30% faster
//! - **Small ASCII strings**: No significant change
//!
//! Enable in your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! serde_bonjson = { version = "0.1", features = ["simd-utf8"] }
//! ```

pub mod de;
pub mod decoder;
pub mod encoder;
pub mod error;
pub mod ser;
pub mod types;
pub mod value;

#[cfg(test)]
mod de_tests;
#[cfg(test)]
mod ser_tests;
#[cfg(test)]
mod lib_tests;
#[cfg(test)]
mod error_tests;
#[cfg(test)]
mod types_tests;
#[cfg(test)]
mod value_tests;

// Re-export commonly used items at the crate root
pub use de::{from_slice, from_slice_with_config, Deserializer};
pub use decoder::{DecodedValue, Decoder, DecoderConfig, DuplicateKeyMode};
pub use encoder::Encoder;
pub use error::{Error, Result};
pub use ser::Serializer;
pub use types::{limits, type_code, BigNumber};
pub use value::Value;

// The bonjson! and json! macros are automatically exported at crate root via #[macro_export]

/// A map of String to Value, used for JSON objects.
///
/// This is a type alias for compatibility with `serde_json::Map`.
pub type Map<K, V> = std::collections::BTreeMap<K, V>;

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

/// Serialize a value to a BONJSON byte vector.
///
/// # Example
///
/// ```rust
/// use serde_bonjson::to_vec;
///
/// let bytes = to_vec(&42i32).unwrap();
/// assert_eq!(bytes, vec![0x8e]); // Small integer 42 (type_code = 42 + 100 = 142 = 0x8e)
/// ```
///
/// # Performance Note
///
/// This function pre-allocates 128 bytes, which is a reasonable default for
/// small-to-medium payloads. For large values where you can estimate the
/// serialized size, use [`to_writer`] with a pre-sized `Vec` for better performance:
///
/// ```rust
/// use serde_bonjson::to_writer;
///
/// let large_data = vec![0i32; 10000];
/// let mut buf = Vec::with_capacity(large_data.len() * 2); // Estimate ~2 bytes per element
/// to_writer(&mut buf, &large_data).unwrap();
/// ```
///
/// # Errors
///
/// Returns an error if serialization fails (e.g., NaN/infinity floats).
pub fn to_vec<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(128);
    to_writer(&mut buf, value)?;
    Ok(buf)
}

/// Serialize a value to a writer.
///
/// # Example
///
/// ```rust
/// use serde_bonjson::to_writer;
///
/// let mut buf = Vec::new();
/// to_writer(&mut buf, &"hello").unwrap();
/// ```
///
/// # Performance Note
///
/// The encoder writes small chunks (often single bytes) directly to the writer.
/// For file or network I/O, wrap your writer in [`std::io::BufWriter`] to avoid
/// excessive syscall overhead:
///
/// ```rust
/// use std::io::BufWriter;
/// use std::fs::File;
/// use serde_bonjson::to_writer;
///
/// let file = File::create("data.bonjson").unwrap();
/// let buffered = BufWriter::new(file);
/// to_writer(buffered, &42i32).unwrap();
/// ```
///
/// For in-memory writers like `Vec<u8>`, no buffering is needed.
///
/// # Errors
///
/// Returns an error if serialization fails or writing to the writer fails.
pub fn to_writer<W: Write, T: Serialize>(writer: W, value: &T) -> Result<()> {
    let mut encoder = Encoder::new(writer);
    {
        let mut serializer = Serializer::new(&mut encoder);
        value.serialize(&mut serializer)?;
    }
    encoder.finish()?;
    Ok(())
}

/// Deserialize from a reader.
///
/// # Example
///
/// ```rust
/// use serde_bonjson::from_reader;
/// use std::io::Cursor;
///
/// let data = Cursor::new(vec![0x8e]); // Small integer 42 (type_code = 42 + 100 = 142 = 0x8e)
/// let value: i32 = from_reader(data).unwrap();
/// assert_eq!(value, 42);
/// ```
///
/// # Performance Note
///
/// This function reads the entire input into memory before parsing.
/// For large files, consider memory-mapping or streaming approaches.
/// For better performance with unbuffered readers (files, network),
/// wrap them in [`std::io::BufReader`]:
///
/// ```rust
/// use serde_bonjson::from_reader;
/// use std::io::BufReader;
/// use std::fs::File;
///
/// # fn example() -> serde_bonjson::Result<()> {
/// let file = File::open("data.bonjson")?;
/// let buffered = BufReader::new(file);
/// let data: Vec<i32> = from_reader(buffered)?;
/// # Ok(())
/// # }
/// ```
///
/// # Errors
///
/// Returns an error if reading fails or deserialization fails.
pub fn from_reader<R: Read, T: for<'de> Deserialize<'de>>(mut reader: R) -> Result<T> {
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf)?;
    from_slice(&buf)
}

/// Deserialize from a reader with custom configuration.
///
/// # Errors
///
/// Returns an error if reading fails or deserialization fails.
pub fn from_reader_with_config<R: Read, T: for<'de> Deserialize<'de>>(
    mut reader: R,
    config: DecoderConfig,
) -> Result<T> {
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf)?;
    from_slice_with_config(&buf, config)
}

/// Convert a `T` into a [`Value`].
///
/// This is useful when you have a typed struct but need a dynamic `Value`
/// for further manipulation or inspection.
///
/// # Example
///
/// ```rust
/// use serde::Serialize;
/// use serde_bonjson::{to_value, Value};
///
/// #[derive(Serialize)]
/// struct Person {
///     name: String,
///     age: u32,
/// }
///
/// let person = Person { name: "Alice".into(), age: 30 };
/// let value = to_value(&person).unwrap();
///
/// assert_eq!(value.get_key("name").and_then(|v| v.as_str()), Some("Alice"));
/// assert_eq!(value.get_key("age").and_then(|v| v.as_i64()), Some(30));
/// ```
///
/// # Errors
///
/// Returns an error if serialization fails (e.g., NaN/infinity floats).
pub fn to_value<T: Serialize>(value: &T) -> Result<Value> {
    // Serialize to bytes, then decode to Value
    let bytes = to_vec(value)?;
    decode_value(&bytes)
}

/// Convert a [`Value`] into a `T`.
///
/// This is useful when you have a dynamic `Value` and want to convert it
/// into a typed struct.
///
/// # Example
///
/// ```rust
/// use serde::Deserialize;
/// use serde_bonjson::{from_value, bonjson};
///
/// #[derive(Deserialize, Debug, PartialEq)]
/// struct Person {
///     name: String,
///     age: u32,
/// }
///
/// let value = bonjson!({
///     "name": "Alice",
///     "age": 30
/// });
///
/// let person: Person = from_value(&value).unwrap();
/// assert_eq!(person, Person { name: "Alice".into(), age: 30 });
/// ```
///
/// # Errors
///
/// Returns an error if the `Value` structure doesn't match the target type.
pub fn from_value<T: for<'de> Deserialize<'de>>(value: &Value) -> Result<T> {
    // Encode to bytes, then deserialize to T
    let bytes = encode_value(value)?;
    from_slice(&bytes)
}

/// Decode a BONJSON document into a `Value`.
///
/// # Example
///
/// ```rust
/// use serde_bonjson::{decode_value, Value};
///
/// // [1, 2, 3]: array type (0xf8) + chunk header (count=3, cont=0 → 0x0c) + elements
/// let bytes = vec![0xf8, 0x0c, 0x65, 0x66, 0x67];
/// let value = decode_value(&bytes).unwrap();
/// assert!(value.is_array());
/// ```
///
/// # Errors
///
/// Returns an error if:
/// - The document exceeds size limits
/// - The data is malformed or truncated
/// - There are trailing bytes after the value
pub fn decode_value(data: &[u8]) -> Result<Value> {
    let mut decoder = Decoder::new(data);
    decoder.check_document_size()?;
    let value = decode_value_recursive(&mut decoder)?;
    decoder.finish()?;
    Ok(value)
}

/// Decode a BONJSON document into a `Value` with custom configuration.
///
/// # Errors
///
/// Returns an error if:
/// - The document exceeds configured limits
/// - The data is malformed or truncated
/// - There are trailing bytes (unless `allow_trailing_bytes` is set)
pub fn decode_value_with_config(data: &[u8], config: DecoderConfig) -> Result<Value> {
    let mut decoder = Decoder::with_config(data, config);
    decoder.check_document_size()?;
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
            while !decoder.is_at_container_end()? {
                arr.push(decode_value_recursive(decoder)?);
            }
            decoder.end_container()?;
            Ok(Value::Array(arr))
        }
        DecodedValue::ObjectStart => {
            let dup_mode = decoder.config().duplicate_key_mode;
            let mut map = std::collections::BTreeMap::new();
            while !decoder.is_at_container_end()? {
                let key_value = decoder.decode_value()?;
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
            decoder.end_container()?;
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
/// use serde_bonjson::{encode_value, Value};
///
/// let value = Value::Int(42);
/// let bytes = encode_value(&value).unwrap();
/// assert_eq!(bytes, vec![0x8e]); // 42 + 100 = 142 = 0x8e
/// ```
///
/// # Errors
///
/// Returns an error if encoding fails (e.g., NaN/infinity floats in the value).
pub fn encode_value(value: &Value) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    encode_value_to_writer(&mut buf, value)?;
    Ok(buf)
}

/// Encode a `Value` to a writer.
///
/// # Errors
///
/// Returns an error if encoding fails or writing to the writer fails.
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
            encoder.begin_array(arr.len())?;
            for item in arr {
                encode_value_recursive(encoder, item)?;
            }
            encoder.end_container()
        }
        Value::Object(map) => {
            encoder.begin_object(map.len())?;
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
                if i64::try_from(v).is_ok() {
                    #[allow(clippy::cast_possible_wrap)]
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
