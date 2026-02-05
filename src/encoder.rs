// ABOUTME: High-performance BONJSON binary encoder.
// ABOUTME: Encodes values using delimiter-terminated containers and FF-terminated long strings.


use crate::error::{Error, Result};
use crate::types::{type_code, BigNumber, zigzag_encode, leb128_encode, NATIVE_SIZE_INDEX};
use std::io::Write;

/// A BONJSON encoder that writes to a byte buffer.
///
/// The encoder tracks container state to ensure well-formed output.
///
/// # Performance Note
///
/// The encoder writes small chunks (often single bytes) directly to the writer.
/// For file or network I/O, wrap your writer in [`std::io::BufWriter`] to avoid
/// excessive syscall overhead. For in-memory writers like `Vec<u8>`, no buffering
/// is needed.
pub struct Encoder<W: Write> {
    writer: W,
    /// Stack of container states: true = object (expecting key/value alternation)
    containers: Vec<ContainerState>,
}

#[derive(Clone, Copy)]
struct ContainerState {
    is_object: bool,
    expecting_key: bool,
}

impl<W: Write> Encoder<W> {
    /// Create a new encoder that writes to the given writer.
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            containers: Vec::new(),
        }
    }

    /// Consume the encoder and return the underlying writer.
    pub fn into_inner(self) -> W {
        self.writer
    }

    /// Get a reference to the underlying writer.
    pub fn get_ref(&self) -> &W {
        &self.writer
    }

    /// Check if we're currently in an object and expecting a key.
    #[inline]
    fn expecting_object_key(&self) -> bool {
        self.containers
            .last()
            .is_some_and(|c| c.is_object && c.expecting_key)
    }

    /// Toggle the key/value expectation in the current object.
    #[inline]
    fn toggle_object_state(&mut self) {
        if let Some(container) = self.containers.last_mut() {
            if container.is_object {
                container.expecting_key = !container.expecting_key;
            }
        }
    }

    // =========================================================================
    // Unchecked methods for serde serializer
    //
    // These methods skip container state tracking for better performance.
    // Designed for the serde serialization path, where Rust's type system
    // guarantees correct structure.
    //
    // These methods still perform:
    // - NaN/Infinity rejection for floats
    // - Optimal encoding selection (small ints, float32, etc.)
    // =========================================================================

    /// Encode a null value without container state checks.
    #[inline]
    pub(crate) fn write_null_unchecked(&mut self) -> Result<()> {
        self.write_byte(type_code::NULL)
    }

    /// Encode a boolean value without state checks.
    #[inline]
    pub(crate) fn write_bool_unchecked(&mut self, value: bool) -> Result<()> {
        self.write_byte(if value { type_code::TRUE } else { type_code::FALSE })
    }

    /// Encode an unsigned integer without state checks.
    #[inline]
    pub(crate) fn write_u64_unchecked(&mut self, value: u64) -> Result<()> {
        self.write_unsigned_int(value)
    }

    /// Encode a signed integer without state checks.
    #[inline]
    pub(crate) fn write_i64_unchecked(&mut self, value: i64) -> Result<()> {
        self.write_signed_int(value)
    }

    /// Encode a 32-bit float without state checks.
    #[inline]
    pub(crate) fn write_f32_unchecked(&mut self, value: f32) -> Result<()> {
        self.write_f64_unchecked(f64::from(value))
    }

    /// Encode a 64-bit float without state checks.
    #[inline]
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_precision_loss)]
    pub(crate) fn write_f64_unchecked(&mut self, value: f64) -> Result<()> {
        if value.is_nan() {
            return Err(Error::NanNotAllowed);
        }
        if value.is_infinite() {
            return Err(Error::InfinityNotAllowed);
        }

        // Negative zero must be encoded as float
        if value == 0.0 && value.is_sign_negative() {
            return self.write_float(value);
        }

        // Try to encode as integer if it's a whole number
        let as_int = value as i64;
        #[allow(clippy::float_cmp)]
        if (as_int as f64) == value {
            return self.write_signed_int(as_int);
        }

        self.write_float(value)
    }

    /// Encode a string without state checks.
    #[inline]
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn write_str_unchecked(&mut self, value: &str) -> Result<()> {
        let bytes = value.as_bytes();
        let len = bytes.len();

        if len <= 15 {
            self.write_byte(type_code::STRING0 + len as u8)?;
            self.write_bytes(bytes)?;
        } else {
            // Long string: FF + data + FF
            self.write_byte(type_code::STRING_LONG)?;
            self.write_bytes(bytes)?;
            self.write_byte(type_code::STRING_LONG)?;
        }
        Ok(())
    }

    /// Begin an array without state checks.
    #[inline]
    pub(crate) fn begin_array_unchecked(&mut self) -> Result<()> {
        self.write_byte(type_code::ARRAY)
    }

    /// Begin an object without state checks.
    #[inline]
    pub(crate) fn begin_object_unchecked(&mut self) -> Result<()> {
        self.write_byte(type_code::OBJECT)
    }

    /// Write a container end marker without state checks.
    #[inline]
    pub(crate) fn end_container_unchecked(&mut self) -> Result<()> {
        self.write_byte(type_code::CONTAINER_END)
    }

    /// Write a single byte.
    #[inline]
    fn write_byte(&mut self, byte: u8) -> Result<()> {
        self.writer.write_all(&[byte])?;
        Ok(())
    }

    /// Write multiple bytes.
    #[inline]
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        self.writer.write_all(bytes)?;
        Ok(())
    }

    /// Encode a null value.
    pub fn write_null(&mut self) -> Result<()> {
        if self.expecting_object_key() {
            return Err(Error::ExpectedObjectKey);
        }
        self.write_byte(type_code::NULL)?;
        self.toggle_object_state();
        Ok(())
    }

    /// Encode a boolean value.
    pub fn write_bool(&mut self, value: bool) -> Result<()> {
        if self.expecting_object_key() {
            return Err(Error::ExpectedObjectKey);
        }
        self.write_byte(if value {
            type_code::TRUE
        } else {
            type_code::FALSE
        })?;
        self.toggle_object_state();
        Ok(())
    }

    /// Encode an unsigned integer.
    pub fn write_u64(&mut self, value: u64) -> Result<()> {
        if self.expecting_object_key() {
            return Err(Error::ExpectedObjectKey);
        }
        self.write_unsigned_int(value)?;
        self.toggle_object_state();
        Ok(())
    }

    /// Encode a signed integer.
    pub fn write_i64(&mut self, value: i64) -> Result<()> {
        if self.expecting_object_key() {
            return Err(Error::ExpectedObjectKey);
        }
        self.write_signed_int(value)?;
        self.toggle_object_state();
        Ok(())
    }

    /// Encode a 64-bit float.
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_precision_loss)]
    pub fn write_f64(&mut self, value: f64) -> Result<()> {
        if self.expecting_object_key() {
            return Err(Error::ExpectedObjectKey);
        }

        if value.is_nan() {
            return Err(Error::NanNotAllowed);
        }
        if value.is_infinite() {
            return Err(Error::InfinityNotAllowed);
        }

        // Negative zero must be encoded as float
        if value == 0.0 && value.is_sign_negative() {
            self.write_float(value)?;
            self.toggle_object_state();
            return Ok(());
        }

        // Try to encode as integer if it's a whole number
        let as_int = value as i64;
        #[allow(clippy::float_cmp)]
        if (as_int as f64) == value {
            return self.write_i64(as_int);
        }

        self.write_float(value)?;
        self.toggle_object_state();
        Ok(())
    }

    /// Encode a 32-bit float.
    pub fn write_f32(&mut self, value: f32) -> Result<()> {
        self.write_f64(f64::from(value))
    }

    /// Encode a `BigNumber` using zigzag LEB128 metadata and LE magnitude bytes.
    pub fn write_big_number(&mut self, value: BigNumber) -> Result<()> {
        if self.expecting_object_key() {
            return Err(Error::ExpectedObjectKey);
        }

        self.write_big_number_payload(value)?;

        self.toggle_object_state();
        Ok(())
    }

    /// Write the BigNumber payload (type code + exponent + signed_length + magnitude).
    /// Shared between checked and unchecked paths.
    fn write_big_number_payload(&mut self, value: BigNumber) -> Result<()> {
        self.write_byte(type_code::BIG_NUMBER)?;

        // Encode exponent as zigzag LEB128
        let mut buf = [0u8; 10];
        let n = leb128_encode(zigzag_encode(value.exponent), &mut buf);
        self.write_bytes(&buf[..n])?;

        if value.significand == 0 {
            // Zero significand: signed_length = 0, no magnitude bytes
            self.write_byte(0x00)?;
            return Ok(());
        }

        // Convert significand to LE bytes and find normalized length
        let sig_bytes = value.significand.to_le_bytes();
        let byte_count = 8 - sig_bytes.iter().rev().take_while(|&&b| b == 0).count();

        // Encode signed_length: positive byte_count for positive, negative for negative
        let signed_length: i64 = if value.sign < 0 {
            -(byte_count as i64)
        } else {
            byte_count as i64
        };
        let n = leb128_encode(zigzag_encode(signed_length), &mut buf);
        self.write_bytes(&buf[..n])?;

        // Write raw LE magnitude bytes
        self.write_bytes(&sig_bytes[..byte_count])
    }

    /// Encode a string.
    #[allow(clippy::cast_possible_truncation)]
    pub fn write_str(&mut self, value: &str) -> Result<()> {
        let bytes = value.as_bytes();
        let len = bytes.len();

        if len <= 15 {
            self.write_byte(type_code::STRING0 + len as u8)?;
            self.write_bytes(bytes)?;
        } else {
            // Long string: FF + data + FF
            self.write_byte(type_code::STRING_LONG)?;
            self.write_bytes(bytes)?;
            self.write_byte(type_code::STRING_LONG)?;
        }

        self.toggle_object_state();
        Ok(())
    }

    /// Begin encoding an array (delimiter-terminated).
    pub fn begin_array(&mut self) -> Result<()> {
        if self.expecting_object_key() {
            return Err(Error::ExpectedObjectKey);
        }
        self.write_byte(type_code::ARRAY)?;
        self.containers.push(ContainerState {
            is_object: false,
            expecting_key: false,
        });
        Ok(())
    }

    /// Begin encoding an object (delimiter-terminated).
    pub fn begin_object(&mut self) -> Result<()> {
        if self.expecting_object_key() {
            return Err(Error::ExpectedObjectKey);
        }
        self.write_byte(type_code::OBJECT)?;
        self.containers.push(ContainerState {
            is_object: true,
            expecting_key: true,
        });
        Ok(())
    }

    /// End the current container by writing the end marker (0xFE).
    pub fn end_container(&mut self) -> Result<()> {
        let container = self
            .containers
            .pop()
            .ok_or(Error::UnbalancedContainers)?;

        // Can't close an object while expecting a value
        if container.is_object && !container.expecting_key {
            return Err(Error::ExpectedObjectValue);
        }

        self.write_byte(type_code::CONTAINER_END)?;
        self.toggle_object_state();
        Ok(())
    }

    /// Finish encoding and ensure all containers are closed.
    pub fn finish(self) -> Result<W> {
        if !self.containers.is_empty() {
            return Err(Error::UnclosedContainer);
        }
        Ok(self.writer)
    }

    // -------------------------------------------------------------------------
    // Internal encoding methods
    // -------------------------------------------------------------------------

    /// Write an unsigned integer using the optimal encoding.
    #[allow(clippy::cast_possible_truncation)]
    fn write_unsigned_int(&mut self, value: u64) -> Result<()> {
        // Small integer range: 0-100
        if value <= 100 {
            return self.write_byte((value as u8) + 100);
        }

        let min_bytes = required_unsigned_bytes_min1(value);
        let native_index = NATIVE_SIZE_INDEX[min_bytes - 1];

        // If MSB is clear, prefer signed encoding (better interop)
        let native_bytes = 1usize << (native_index as usize);
        let msb_set = (value >> (native_bytes * 8 - 1)) & 1 != 0;
        let type_code = if msb_set {
            type_code::UINT8 + native_index
        } else {
            type_code::SINT8 + native_index
        };

        self.write_byte(type_code)?;
        let bytes = value.to_le_bytes();
        self.write_bytes(&bytes[..native_bytes])
    }

    /// Write a signed integer using the optimal encoding.
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    fn write_signed_int(&mut self, value: i64) -> Result<()> {
        // Small integer range: -100 to 100
        if (-100..=100).contains(&value) {
            return self.write_byte((value + 100) as u8);
        }

        let min_bytes = required_signed_bytes_min1(value);
        let native_index = NATIVE_SIZE_INDEX[min_bytes - 1];
        let native_bytes = 1usize << (native_index as usize);

        // For positive values, check if unsigned encoding needs fewer bytes
        if value > 0 {
            let unsigned_min = required_unsigned_bytes_min1(value as u64);
            let unsigned_native_index = NATIVE_SIZE_INDEX[unsigned_min - 1];
            let unsigned_native_bytes = 1usize << (unsigned_native_index as usize);
            if unsigned_native_bytes < native_bytes {
                self.write_byte(type_code::UINT8 + unsigned_native_index)?;
                let bytes = (value as u64).to_le_bytes();
                return self.write_bytes(&bytes[..unsigned_native_bytes]);
            }
        }

        self.write_byte(type_code::SINT8 + native_index)?;
        let bytes = value.to_le_bytes();
        self.write_bytes(&bytes[..native_bytes])
    }

    /// Write a float using the optimal encoding (32 or 64 bit).
    #[allow(clippy::cast_possible_truncation)]
    fn write_float(&mut self, value: f64) -> Result<()> {
        // Try f32
        let f32_val = value as f32;
        #[allow(clippy::float_cmp)]
        if f64::from(f32_val) == value {
            let mut buf = [0u8; 5];
            buf[0] = type_code::FLOAT32;
            buf[1..5].copy_from_slice(&f32_val.to_le_bytes());
            return self.write_bytes(&buf);
        }

        // Use f64
        let mut buf = [0u8; 9];
        buf[0] = type_code::FLOAT64;
        buf[1..9].copy_from_slice(&value.to_le_bytes());
        self.write_bytes(&buf)
    }
}

// =============================================================================
// Utility functions
// =============================================================================

/// Calculate the number of bytes required to store an unsigned integer (minimum 1).
#[inline]
fn required_unsigned_bytes_min1(value: u64) -> usize {
    if value == 0 {
        return 1;
    }
    let bits = 64 - value.leading_zeros() as usize;
    (bits + 7) / 8
}

/// Calculate the number of bytes required to store a signed integer (minimum 1).
#[inline]
fn required_signed_bytes_min1(value: i64) -> usize {
    if value == 0 {
        return 1;
    }

    let redundant = if value < 0 {
        value.leading_ones() as usize
    } else {
        value.leading_zeros() as usize
    };

    // We need at least one sign bit, so subtract 1 from redundant count
    let significant_bits = 64 - redundant + 1;
    (significant_bits + 7) / 8
}

// =============================================================================
// Convenience functions
// =============================================================================

/// Encode a value to a byte vector.
pub fn to_vec<T: serde::Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(128);
    let mut encoder = Encoder::new(&mut buf);
    value.serialize(&mut crate::ser::Serializer::new(&mut encoder))?;
    encoder.finish()?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_required_unsigned_bytes() {
        assert_eq!(required_unsigned_bytes_min1(0), 1);
        assert_eq!(required_unsigned_bytes_min1(255), 1);
        assert_eq!(required_unsigned_bytes_min1(256), 2);
        assert_eq!(required_unsigned_bytes_min1(0xffff), 2);
        assert_eq!(required_unsigned_bytes_min1(0x10000), 3);
        assert_eq!(required_unsigned_bytes_min1(u64::MAX), 8);
    }

    #[test]
    fn test_required_signed_bytes() {
        assert_eq!(required_signed_bytes_min1(0), 1);
        assert_eq!(required_signed_bytes_min1(127), 1);
        assert_eq!(required_signed_bytes_min1(128), 2);
        assert_eq!(required_signed_bytes_min1(-1), 1);
        assert_eq!(required_signed_bytes_min1(-128), 1);
        assert_eq!(required_signed_bytes_min1(-129), 2);
    }

    #[test]
    fn test_encode_small_ints() {
        let mut buf = Vec::new();
        let mut enc = Encoder::new(&mut buf);
        enc.write_i64(0).unwrap();
        assert_eq!(buf, vec![0x64]);

        buf.clear();
        let mut enc = Encoder::new(&mut buf);
        enc.write_i64(100).unwrap();
        assert_eq!(buf, vec![0xc8]);

        buf.clear();
        let mut enc = Encoder::new(&mut buf);
        enc.write_i64(-100).unwrap();
        assert_eq!(buf, vec![0x00]);

        buf.clear();
        let mut enc = Encoder::new(&mut buf);
        enc.write_i64(-1).unwrap();
        assert_eq!(buf, vec![0x63]);
    }

    #[test]
    fn test_encode_larger_ints() {
        // 1000 as sint16 (0xe5): native size for 2 bytes
        let mut buf = Vec::new();
        let mut enc = Encoder::new(&mut buf);
        enc.write_i64(1000).unwrap();
        assert_eq!(buf, vec![0xe5, 0xe8, 0x03]);

        // 180 as uint8 (0xe0)
        buf.clear();
        let mut enc = Encoder::new(&mut buf);
        enc.write_u64(180).unwrap();
        assert_eq!(buf, vec![0xe0, 0xb4]);
    }

    #[test]
    fn test_encode_null_bool() {
        let mut buf = Vec::new();
        let mut enc = Encoder::new(&mut buf);
        enc.write_null().unwrap();
        assert_eq!(buf, vec![0xcd]);

        buf.clear();
        let mut enc = Encoder::new(&mut buf);
        enc.write_bool(true).unwrap();
        assert_eq!(buf, vec![0xcf]);

        buf.clear();
        let mut enc = Encoder::new(&mut buf);
        enc.write_bool(false).unwrap();
        assert_eq!(buf, vec![0xce]);
    }

    #[test]
    fn test_encode_short_string() {
        // Empty string: 0xd0
        let mut buf = Vec::new();
        let mut enc = Encoder::new(&mut buf);
        enc.write_str("").unwrap();
        assert_eq!(buf, vec![0xd0]);

        // "A": 0xd1 + 'A'
        buf.clear();
        let mut enc = Encoder::new(&mut buf);
        enc.write_str("A").unwrap();
        assert_eq!(buf, vec![0xd1, 0x41]);
    }

    #[test]
    fn test_encode_long_string() {
        // 16-byte string → FF + data + FF
        let s = "abcdefghijklmnop"; // 16 bytes
        let mut buf = Vec::new();
        let mut enc = Encoder::new(&mut buf);
        enc.write_str(s).unwrap();
        assert_eq!(buf[0], 0xff);
        assert_eq!(&buf[1..17], s.as_bytes());
        assert_eq!(buf[17], 0xff);
    }

    #[test]
    fn test_encode_empty_containers() {
        // Empty array: FC FE
        let mut buf = Vec::new();
        let mut enc = Encoder::new(&mut buf);
        enc.begin_array().unwrap();
        enc.end_container().unwrap();
        assert_eq!(buf, vec![0xfc, 0xfe]);

        // Empty object: FD FE
        buf.clear();
        let mut enc = Encoder::new(&mut buf);
        enc.begin_object().unwrap();
        enc.end_container().unwrap();
        assert_eq!(buf, vec![0xfd, 0xfe]);
    }

    #[test]
    fn test_encode_array_with_values() {
        // [1, "x", null] → FC 65 D1 78 CD FE
        let mut buf = Vec::new();
        let mut enc = Encoder::new(&mut buf);
        enc.begin_array().unwrap();
        enc.write_i64(1).unwrap();
        enc.write_str("x").unwrap();
        enc.write_null().unwrap();
        enc.end_container().unwrap();
        assert_eq!(buf, vec![0xfc, 0x65, 0xd1, 0x78, 0xcd, 0xfe]);
    }

    #[test]
    fn test_encode_object() {
        // {"a": 1} → FD D1 61 65 FE
        let mut buf = Vec::new();
        let mut enc = Encoder::new(&mut buf);
        enc.begin_object().unwrap();
        enc.write_str("a").unwrap();
        enc.write_i64(1).unwrap();
        enc.end_container().unwrap();
        assert_eq!(buf, vec![0xfd, 0xd1, 0x61, 0x65, 0xfe]);
    }

    #[test]
    fn test_nan_infinity_rejected() {
        let mut buf = Vec::new();
        let mut enc = Encoder::new(&mut buf);
        assert!(enc.write_f64(f64::NAN).is_err());

        buf.clear();
        let mut enc = Encoder::new(&mut buf);
        assert!(enc.write_f64(f64::INFINITY).is_err());

        buf.clear();
        let mut enc = Encoder::new(&mut buf);
        assert!(enc.write_f64(f64::NEG_INFINITY).is_err());
    }
}
