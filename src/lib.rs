// ABOUTME: serde_bonjson - A BONJSON (Binary Object Notation for JSON) encoder/decoder.
// ABOUTME: Drop-in replacement for serde_json - just prepend "bon" to "json" in your imports.

//! # `serde_bonjson`
//!
//! A drop-in replacement for [`serde_json`](https://docs.rs/serde_json) that's 2x faster
//! and produces smaller payloads.
//!
//! BONJSON is a binary encoding that's 1:1 compatible with JSON's data model.
//! If you're using `serde_json`, switching is a one-line change — just prepend "bon" to "json".
//!
//! ## Migrating from `serde_json`
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
pub use decoder::{DecodedValue, Decoder, DecoderConfig, DuplicateKeyMode, InvalidUtf8Mode, NanInfinityMode, OutOfRangeMode, UnicodeNormalization};
pub use encoder::{Encoder, EncoderConfig};
pub use error::{Error, Result};
pub use ser::{Serializer, SerializerConfig};
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
/// Uses the default [`SerializerConfig`] (typed arrays enabled, records disabled).
///
/// # Example
///
/// ```rust
/// use serde_bonjson::to_vec;
///
/// let bytes = to_vec(&42i32).unwrap();
/// assert_eq!(bytes, vec![0x2a]); // Small integer 42 (type_code = 42 = 0x2a)
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

/// Serialize a value to a BONJSON byte vector with custom configuration.
///
/// # Errors
///
/// Returns an error if serialization fails (e.g., NaN/infinity floats).
pub fn to_vec_with_config<T: Serialize>(value: &T, config: &SerializerConfig) -> Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(128);
    to_writer_with_config(&mut buf, value, config)?;
    Ok(buf)
}

/// Serialize a value to a writer.
///
/// Uses the default [`SerializerConfig`] (typed arrays enabled, records disabled).
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
/// # Errors
///
/// Returns an error if serialization fails or writing to the writer fails.
pub fn to_writer<W: Write, T: Serialize>(writer: W, value: &T) -> Result<()> {
    to_writer_with_config(writer, value, &SerializerConfig::default())
}

/// Serialize a value to a writer with custom configuration.
///
/// When `config.records` is true, this performs a two-pass traversal:
/// 1. Count struct types (lightweight, no I/O)
/// 2. Emit record definitions for types appearing 2+ times, then serialize
///
/// # Errors
///
/// Returns an error if serialization fails or writing to the writer fails.
pub fn to_writer_with_config<W: Write, T: Serialize>(
    writer: W,
    value: &T,
    config: &SerializerConfig,
) -> Result<()> {
    use ser::CountingSerializer;
    use std::collections::HashMap;

    let mut encoder = Encoder::new(writer);

    // If records are enabled, run the counting pass first
    let record_defs = if config.records {
        let mut counter = CountingSerializer::new();
        value.serialize(&mut counter)?;

        // Filter to structs appearing 2+ times
        let qualifying: Vec<(&'static str, Vec<&'static str>)> = counter
            .struct_counts
            .into_iter()
            .filter(|(_, (_, count))| *count >= 2)
            .map(|(name, (keys, _))| (name, keys))
            .collect();

        if qualifying.is_empty() {
            None
        } else {
            // Sort for deterministic output
            let mut sorted = qualifying;
            sorted.sort_by_key(|(name, _)| *name);

            // Write record definitions and build the lookup map
            let mut defs = HashMap::new();
            for (def_index, (name, keys)) in sorted.iter().enumerate() {
                let key_refs: Vec<&str> = keys.to_vec();
                encoder.write_record_definition_unchecked(&key_refs)?;
                defs.insert(*name, (keys.clone(), def_index));
            }
            Some(defs)
        }
    } else {
        None
    };

    {
        let mut serializer = Serializer::with_config(&mut encoder, config.clone(), record_defs);
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
/// let data = Cursor::new(vec![0x2a]); // Small integer 42 (type_code = 42 = 0x2a)
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
/// // [1, 2, 3]: array start (0xb7) + elements + end marker (0xb6)
/// let bytes = vec![0xb7, 0x01, 0x02, 0x03, 0xb6];
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
    decoder.read_record_definitions()?;
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
    decoder.read_record_definitions()?;
    let value = decode_value_recursive(&mut decoder)?;
    decoder.finish()?;
    Ok(value)
}

/// Apply NFC normalization if configured and the feature is enabled.
#[cfg(feature = "unicode-normalization")]
fn maybe_nfc_normalize(mode: decoder::UnicodeNormalization, s: String) -> String {
    if mode == decoder::UnicodeNormalization::Nfc {
        use unicode_normalization::UnicodeNormalization;
        let normalized: String = s.nfc().collect();
        normalized
    } else {
        s
    }
}

/// No-op when unicode-normalization feature is not enabled.
#[cfg(not(feature = "unicode-normalization"))]
fn maybe_nfc_normalize(_mode: decoder::UnicodeNormalization, s: String) -> String {
    s
}

/// Check if a BigNumber's value exceeds the representable range of f64.
fn bignumber_exceeds_f64_range(bn: &BigNumber) -> bool {
    if bn.significand == 0 {
        return false;
    }
    let exp = bn.exponent;
    if exp > 308 {
        return true;
    }
    if exp < -343 {
        return false;
    }
    let value = bn.significand as f64 * 10_f64.powi(exp as i32);
    value.is_infinite()
}

fn decode_value_recursive(decoder: &mut Decoder<'_>) -> Result<Value> {
    use decoder::DuplicateKeyMode;
    use decoder::NanInfinityMode;
    use decoder::OutOfRangeMode;

    match decoder.decode_value()? {
        DecodedValue::Null => Ok(Value::Null),
        DecodedValue::Bool(b) => Ok(Value::Bool(b)),
        DecodedValue::Int(n) => Ok(Value::Int(n)),
        DecodedValue::UInt(n) => Ok(Value::UInt(n)),
        DecodedValue::Float(f) => {
            if decoder.config().nan_infinity_mode == NanInfinityMode::Stringify {
                if f.is_nan() {
                    return Ok(Value::String("NaN".into()));
                }
                if f == f64::INFINITY {
                    return Ok(Value::String("Infinity".into()));
                }
                if f == f64::NEG_INFINITY {
                    return Ok(Value::String("-Infinity".into()));
                }
            }
            Ok(Value::Float(f))
        }
        DecodedValue::BigNumber(bn) => {
            let exceeds_f64 = bignumber_exceeds_f64_range(&bn);
            if decoder.config().out_of_range_mode == OutOfRangeMode::Stringify {
                let exp_exceeded = (bn.exponent.unsigned_abs() as usize) > decoder.config().max_bignumber_exponent;
                // Check magnitude byte count
                let mag_bytes = if bn.significand == 0 { 0 } else { ((64 - bn.significand.leading_zeros()) as usize + 7) / 8 };
                let mag_exceeded = mag_bytes > decoder.config().max_bignumber_magnitude;
                if exp_exceeded || mag_exceeded || exceeds_f64 {
                    return Ok(Value::String(bn.to_string_notation()));
                }
            } else if exceeds_f64 {
                return Err(Error::ValueOutOfRange);
            }
            Ok(Value::BigNumber(bn))
        }
        DecodedValue::String(s) => {
            let owned = s.into_owned();
            Ok(Value::String(maybe_nfc_normalize(decoder.config().unicode_normalization, owned)))
        }
        DecodedValue::ArrayStart => {
            let max_size = decoder.config().max_container_size;
            let mut arr = Vec::new();
            while !decoder.is_at_container_end()? {
                if arr.len() >= max_size {
                    return Err(Error::MaxContainerSizeExceeded);
                }
                arr.push(decode_value_recursive(decoder)?);
            }
            decoder.end_container()?;
            Ok(Value::Array(arr))
        }
        DecodedValue::ObjectStart => {
            let dup_mode = decoder.config().duplicate_key_mode;
            let max_size = decoder.config().max_container_size;
            let mut map = std::collections::BTreeMap::new();
            let mut pair_count: usize = 0;
            while !decoder.is_at_container_end()? {
                if pair_count >= max_size {
                    return Err(Error::MaxContainerSizeExceeded);
                }
                let key_value = decoder.decode_value()?;
                let key = match key_value {
                    DecodedValue::String(s) => {
                        maybe_nfc_normalize(decoder.config().unicode_normalization, s.into_owned())
                    }
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
                pair_count += 1;
            }
            decoder.end_container()?;
            Ok(Value::Object(map))
        }
        DecodedValue::RecordInstanceStart(def_index) => {
            let keys = decoder.record_definitions()[def_index].clone();
            let dup_mode = decoder.config().duplicate_key_mode;
            let max_size = decoder.config().max_container_size;
            let mut map = std::collections::BTreeMap::new();
            let mut value_count: usize = 0;

            while !decoder.is_at_container_end()? {
                if value_count >= keys.len() {
                    return Err(Error::InvalidData(
                        "record instance has more values than keys".into(),
                    ));
                }
                if value_count >= max_size {
                    return Err(Error::MaxContainerSizeExceeded);
                }
                let key = maybe_nfc_normalize(
                    decoder.config().unicode_normalization,
                    keys[value_count].clone(),
                );
                let value = decode_value_recursive(decoder)?;
                if map.contains_key(&key) {
                    match dup_mode {
                        DuplicateKeyMode::Error => return Err(Error::DuplicateKey),
                        DuplicateKeyMode::KeepFirst => {
                            value_count += 1;
                            continue;
                        }
                        DuplicateKeyMode::KeepLast => {}
                    }
                }
                map.insert(key, value);
                value_count += 1;
            }
            decoder.end_container()?;
            // Remaining keys get Value::Null
            for key in keys.iter().skip(value_count) {
                let key = maybe_nfc_normalize(
                    decoder.config().unicode_normalization,
                    key.clone(),
                );
                map.entry(key).or_insert(Value::Null);
            }
            Ok(Value::Object(map))
        }
        DecodedValue::TypedArrayStart { element_type_code, count } => {
            let mut arr = Vec::with_capacity(count);
            for _ in 0..count {
                let elem = decoder.read_typed_array_element(element_type_code)?;
                let value = match elem {
                    DecodedValue::Int(n) => Value::Int(n),
                    DecodedValue::UInt(n) => Value::UInt(n),
                    DecodedValue::Float(f) => {
                        if decoder.config().nan_infinity_mode == NanInfinityMode::Stringify {
                            if f.is_nan() {
                                Value::String("NaN".into())
                            } else if f == f64::INFINITY {
                                Value::String("Infinity".into())
                            } else if f == f64::NEG_INFINITY {
                                Value::String("-Infinity".into())
                            } else {
                                Value::Float(f)
                            }
                        } else {
                            Value::Float(f)
                        }
                    }
                    _ => unreachable!("typed array element must be numeric"),
                };
                arr.push(value);
            }
            decoder.end_typed_array()?;
            Ok(Value::Array(arr))
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
/// assert_eq!(bytes, vec![0x2a]); // 42 = 0x2a
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
    encode_value_with_records(&mut encoder, value)?;
    encoder.finish()?;
    Ok(())
}

/// Encode a `Value` to BONJSON bytes with the given configuration.
///
/// # Errors
///
/// Returns an error if encoding fails (e.g., NaN/infinity floats in the value).
pub fn encode_value_with_config(value: &Value, config: EncoderConfig) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    encode_value_to_writer_with_config(&mut buf, value, config)?;
    Ok(buf)
}

/// Encode a `Value` to a writer with the given configuration.
///
/// # Errors
///
/// Returns an error if encoding fails or writing to the writer fails.
pub fn encode_value_to_writer_with_config<W: Write>(writer: W, value: &Value, config: EncoderConfig) -> Result<()> {
    let mut encoder = Encoder::with_config(writer, config);
    encode_value_with_records(&mut encoder, value)?;
    encoder.finish()?;
    Ok(())
}

/// Encode a value with automatic record definition detection.
fn encode_value_with_records<W: Write>(encoder: &mut Encoder<W>, value: &Value) -> Result<()> {
    // Collect record definitions (key sets appearing 2+ times)
    let defs = collect_record_definitions(value);

    if defs.is_empty() {
        // No records to emit, use simple path
        return encode_value_recursive(encoder, value);
    }

    // Build index map
    let def_index_map: std::collections::HashMap<Vec<String>, usize> = defs
        .iter()
        .enumerate()
        .map(|(i, keys)| (keys.clone(), i))
        .collect();

    // Write record definitions
    for def in &defs {
        let key_refs: Vec<&str> = def.iter().map(|s| s.as_str()).collect();
        encoder.write_record_definition(&key_refs)?;
    }

    // Encode values using record instances where applicable
    encode_value_recursive_inner(encoder, value, &defs, &def_index_map)
}

/// Detect if an array can be encoded as a typed array and return the type code if so.
fn detect_typed_array(arr: &[Value]) -> Option<u8> {
    use crate::types::type_code as tc;
    if arr.is_empty() {
        return None;
    }

    // Check what numeric types are present
    let mut all_int = true;
    let mut all_uint = true;
    let mut all_float = true;
    let mut max_unsigned: u64 = 0;
    let mut min_signed: i64 = 0;
    let mut max_signed: i64 = 0;
    let mut needs_f64 = false;

    for v in arr {
        match v {
            Value::Int(n) => {
                all_float = false;
                if *n < 0 {
                    all_uint = false;
                    if *n < min_signed {
                        min_signed = *n;
                    }
                }
                if *n > max_signed {
                    max_signed = *n;
                }
                if *n >= 0 && (*n as u64) > max_unsigned {
                    #[allow(clippy::cast_sign_loss)]
                    {
                        max_unsigned = *n as u64;
                    }
                }
            }
            Value::UInt(n) => {
                all_float = false;
                all_int = false;
                if *n > max_unsigned {
                    max_unsigned = *n;
                }
            }
            Value::Float(f) => {
                all_int = false;
                all_uint = false;
                if !f.is_finite() {
                    return None; // Can't use typed arrays for NaN/Infinity
                }
                // Check if f32 suffices
                #[allow(clippy::cast_possible_truncation)]
                let as_f32 = *f as f32;
                #[allow(clippy::float_cmp)]
                if f64::from(as_f32) != *f {
                    needs_f64 = true;
                }
            }
            _ => return None, // Non-numeric element
        }
    }

    if all_float {
        return Some(if needs_f64 { tc::TYPED_ARRAY_FLOAT64 } else { tc::TYPED_ARRAY_FLOAT32 });
    }

    if all_int {
        // All Value::Int — use signed types to preserve round-trip fidelity
        if min_signed >= i8::MIN as i64 && max_signed <= i8::MAX as i64 {
            return Some(tc::TYPED_ARRAY_SINT8);
        }
        if min_signed >= i16::MIN as i64 && max_signed <= i16::MAX as i64 {
            return Some(tc::TYPED_ARRAY_SINT16);
        }
        if min_signed >= i32::MIN as i64 && max_signed <= i32::MAX as i64 {
            return Some(tc::TYPED_ARRAY_SINT32);
        }
        return Some(tc::TYPED_ARRAY_SINT64);
    }

    if all_uint {
        // All Value::UInt — use unsigned types
        let effective_max = max_unsigned;
        if effective_max <= u8::MAX as u64 {
            return Some(tc::TYPED_ARRAY_UINT8);
        }
        if effective_max <= u16::MAX as u64 {
            return Some(tc::TYPED_ARRAY_UINT16);
        }
        if effective_max <= u32::MAX as u64 {
            return Some(tc::TYPED_ARRAY_UINT32);
        }
        return Some(tc::TYPED_ARRAY_UINT64);
    }

    None
}

/// Encode values into a typed array byte buffer.
fn encode_typed_array_data(arr: &[Value], element_type_code: u8) -> Vec<u8> {
    use crate::types::type_code as tc;
    let elem_size = tc::typed_array_element_size(element_type_code);
    let mut data = Vec::with_capacity(arr.len() * elem_size);

    for v in arr {
        match element_type_code {
            tc::TYPED_ARRAY_FLOAT32 => {
                #[allow(clippy::cast_possible_truncation)]
                let f = match v {
                    Value::Float(f) => *f as f32,
                    Value::Int(n) => *n as f32,
                    Value::UInt(n) => *n as f32,
                    _ => unreachable!(),
                };
                data.extend_from_slice(&f.to_le_bytes());
            }
            tc::TYPED_ARRAY_FLOAT64 => {
                let f = match v {
                    Value::Float(f) => *f,
                    Value::Int(n) => *n as f64,
                    Value::UInt(n) => *n as f64,
                    _ => unreachable!(),
                };
                data.extend_from_slice(&f.to_le_bytes());
            }
            tc::TYPED_ARRAY_SINT8 => {
                let n = match v { Value::Int(n) => *n, _ => unreachable!() };
                data.push(n as u8);
            }
            tc::TYPED_ARRAY_SINT16 => {
                let n = match v { Value::Int(n) => *n, _ => unreachable!() };
                data.extend_from_slice(&(n as i16).to_le_bytes());
            }
            tc::TYPED_ARRAY_SINT32 => {
                let n = match v { Value::Int(n) => *n, _ => unreachable!() };
                data.extend_from_slice(&(n as i32).to_le_bytes());
            }
            tc::TYPED_ARRAY_SINT64 => {
                let n = match v { Value::Int(n) => *n, _ => unreachable!() };
                data.extend_from_slice(&n.to_le_bytes());
            }
            tc::TYPED_ARRAY_UINT8 => {
                let n = value_as_u64(v);
                data.push(n as u8);
            }
            tc::TYPED_ARRAY_UINT16 => {
                let n = value_as_u64(v);
                data.extend_from_slice(&(n as u16).to_le_bytes());
            }
            tc::TYPED_ARRAY_UINT32 => {
                let n = value_as_u64(v);
                data.extend_from_slice(&(n as u32).to_le_bytes());
            }
            tc::TYPED_ARRAY_UINT64 => {
                let n = value_as_u64(v);
                data.extend_from_slice(&n.to_le_bytes());
            }
            _ => unreachable!(),
        }
    }
    data
}

/// Extract u64 from a Value that is known to be a non-negative integer.
#[allow(clippy::cast_sign_loss)]
fn value_as_u64(v: &Value) -> u64 {
    match v {
        Value::Int(n) => *n as u64,
        Value::UInt(n) => *n,
        _ => unreachable!(),
    }
}

/// Collect record definitions from a Value tree.
/// Returns a list of key sets that appear 2+ times among objects.
fn collect_record_definitions(value: &Value) -> Vec<Vec<String>> {
    let mut key_set_counts: std::collections::HashMap<Vec<String>, usize> = std::collections::HashMap::new();
    count_key_sets(value, &mut key_set_counts);

    let mut defs: Vec<Vec<String>> = key_set_counts
        .into_iter()
        .filter(|(_, count)| *count >= 2)
        .map(|(keys, _)| keys)
        .collect();
    // Sort for deterministic output
    defs.sort();
    defs
}

fn count_key_sets(value: &Value, counts: &mut std::collections::HashMap<Vec<String>, usize>) {
    match value {
        Value::Object(map) => {
            if !map.is_empty() {
                let keys: Vec<String> = map.keys().cloned().collect();
                *counts.entry(keys).or_insert(0) += 1;
            }
            for v in map.values() {
                count_key_sets(v, counts);
            }
        }
        Value::Array(arr) => {
            for item in arr {
                count_key_sets(item, counts);
            }
        }
        _ => {}
    }
}

fn encode_value_recursive<W: Write>(encoder: &mut Encoder<W>, value: &Value) -> Result<()> {
    encode_value_recursive_inner(encoder, value, &[], &std::collections::HashMap::new())
}

#[allow(clippy::only_used_in_recursion)]
fn encode_value_recursive_inner<W: Write>(
    encoder: &mut Encoder<W>,
    value: &Value,
    record_defs: &[Vec<String>],
    def_index_map: &std::collections::HashMap<Vec<String>, usize>,
) -> Result<()> {
    match value {
        Value::Null => encoder.write_null(),
        Value::Bool(b) => encoder.write_bool(*b),
        Value::Int(n) => encoder.write_i64(*n),
        Value::UInt(n) => encoder.write_u64(*n),
        Value::Float(f) => encoder.write_f64(*f),
        Value::BigNumber(bn) => encoder.write_big_number(*bn),
        Value::String(s) => encoder.write_str(s),
        Value::Array(arr) => {
            // Try typed array encoding
            if let Some(element_tc) = detect_typed_array(arr) {
                let data = encode_typed_array_data(arr, element_tc);
                return encoder.write_typed_array_raw(element_tc, arr.len(), &data);
            }
            encoder.begin_array()?;
            for item in arr {
                encode_value_recursive_inner(encoder, item, record_defs, def_index_map)?;
            }
            encoder.end_container()
        }
        Value::Object(map) => {
            // Check if this object matches a record definition
            if !map.is_empty() {
                let keys: Vec<String> = map.keys().cloned().collect();
                if let Some(&idx) = def_index_map.get(&keys) {
                    encoder.begin_record_instance(idx)?;
                    // Write values in key order (BTreeMap iterates in sorted order)
                    for val in map.values() {
                        encode_value_recursive_inner(encoder, val, record_defs, def_index_map)?;
                    }
                    return encoder.end_container();
                }
            }
            encoder.begin_object()?;
            for (key, val) in map {
                encoder.write_str(key)?;
                encode_value_recursive_inner(encoder, val, record_defs, def_index_map)?;
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
