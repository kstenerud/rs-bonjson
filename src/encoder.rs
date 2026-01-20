// ABOUTME: High-performance BONJSON binary encoder.
// ABOUTME: Uses compiler intrinsics (leading_zeros) for efficient length field encoding.


use crate::error::{Error, Result};
use crate::types::{type_code, BigNumber};
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
    // These methods skip container state tracking (ensuring keys alternate with
    // values in objects, tracking nesting depth) for better performance. They
    // are designed for the serde serialization path, where Rust's type system
    // guarantees correct structure.
    //
    // These methods still perform:
    // - NaN/Infinity rejection for floats
    // - Optimal encoding selection (small ints, bfloat16, etc.)
    //
    // For manual encoding where you want structural validation, use the public
    // checked methods (write_null, write_i64, begin_object, etc.) instead.
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
    #[allow(clippy::cast_possible_truncation)] // Intentional: f64 to i64 for whole number check
    #[allow(clippy::cast_precision_loss)] // Intentional: i64 to f64 for round-trip comparison
    pub(crate) fn write_f64_unchecked(&mut self, value: f64) -> Result<()> {
        // Check for NaN and infinity
        if value.is_nan() || value.is_infinite() {
            return Err(Error::InvalidData("NaN and Infinity are not allowed".into()));
        }

        // Check for negative zero - must be encoded as float, not integer
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
    #[allow(clippy::cast_possible_truncation)] // len <= 15 checked before cast to u8
    pub(crate) fn write_str_unchecked(&mut self, value: &str) -> Result<()> {
        let bytes = value.as_bytes();
        let len = bytes.len();

        if len <= 15 {
            self.write_byte(type_code::STRING0 + len as u8)?;
            self.write_bytes(bytes)?;
        } else {
            self.write_byte(type_code::STRING_LONG)?;
            self.write_length_field(len as u64, false)?;
            self.write_bytes(bytes)?;
        }
        Ok(())
    }

    /// Begin an array with known element count without state checks.
    /// Writes type code and chunk header in one operation.
    #[inline]
    pub(crate) fn begin_array_unchecked(&mut self, element_count: usize) -> Result<()> {
        self.write_byte(type_code::ARRAY)?;
        self.write_length_field(element_count as u64, false)
    }

    /// Begin an object with known pair count without state checks.
    /// Writes type code and chunk header in one operation.
    #[inline]
    pub(crate) fn begin_object_unchecked(&mut self, pair_count: usize) -> Result<()> {
        self.write_byte(type_code::OBJECT)?;
        self.write_length_field(pair_count as u64, false)
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
    ///
    /// # Errors
    ///
    /// Returns [`Error::ExpectedObjectKey`] if called when an object key is expected.
    pub fn write_null(&mut self) -> Result<()> {
        if self.expecting_object_key() {
            return Err(Error::ExpectedObjectKey);
        }
        self.write_byte(type_code::NULL)?;
        self.toggle_object_state();
        Ok(())
    }

    /// Encode a boolean value.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ExpectedObjectKey`] if called when an object key is expected.
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
    ///
    /// # Errors
    ///
    /// Returns [`Error::ExpectedObjectKey`] if called when an object key is expected.
    pub fn write_u64(&mut self, value: u64) -> Result<()> {
        if self.expecting_object_key() {
            return Err(Error::ExpectedObjectKey);
        }
        self.write_unsigned_int(value)?;
        self.toggle_object_state();
        Ok(())
    }

    /// Encode a signed integer.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ExpectedObjectKey`] if called when an object key is expected.
    pub fn write_i64(&mut self, value: i64) -> Result<()> {
        if self.expecting_object_key() {
            return Err(Error::ExpectedObjectKey);
        }
        self.write_signed_int(value)?;
        self.toggle_object_state();
        Ok(())
    }

    /// Encode a 64-bit float.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Called when an object key is expected
    /// - The value is NaN or infinity (not allowed in BONJSON)
    #[allow(clippy::cast_possible_truncation)] // Intentional: f64 to i64 for whole number check
    #[allow(clippy::cast_precision_loss)] // Intentional: i64 to f64 for round-trip comparison
    pub fn write_f64(&mut self, value: f64) -> Result<()> {
        if self.expecting_object_key() {
            return Err(Error::ExpectedObjectKey);
        }

        // Check for NaN and infinity
        if value.is_nan() || value.is_infinite() {
            return Err(Error::InvalidData("NaN and Infinity are not allowed".into()));
        }

        // Check for negative zero - must be encoded as float, not integer
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
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Called when an object key is expected
    /// - The value is NaN or infinity (not allowed in BONJSON)
    pub fn write_f32(&mut self, value: f32) -> Result<()> {
        self.write_f64(f64::from(value))
    }

    /// Encode a `BigNumber`.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Called when an object key is expected
    /// - The exponent is out of the valid range (-0x800000 to 0x7fffff)
    #[allow(clippy::cast_possible_truncation)] // sig_bytes and exp_bytes are <= 8, safe for u8
    pub fn write_big_number(&mut self, value: BigNumber) -> Result<()> {
        if self.expecting_object_key() {
            return Err(Error::ExpectedObjectKey);
        }

        // Validate exponent range
        if value.exponent < -0x0080_0000 || value.exponent > 0x007f_ffff {
            return Err(Error::InvalidData("BigNumber exponent out of range".into()));
        }

        self.write_byte(type_code::BIG_NUMBER)?;

        let exp_bytes = required_signed_bytes_min0(i64::from(value.exponent));
        let sig_bytes = required_unsigned_bytes_min0(value.significand);

        // Header byte: SSSSS EE N
        // S = significand length (5 bits)
        // E = exponent length (2 bits)
        // N = negative sign (1 bit)
        let header = ((sig_bytes as u8) << 3)
            | ((exp_bytes as u8) << 1)
            | u8::from(value.sign < 0);

        self.write_byte(header)?;

        // Write exponent (little-endian)
        if exp_bytes > 0 {
            let exp_le = i64::from(value.exponent).to_le_bytes();
            self.write_bytes(&exp_le[..exp_bytes])?;
        }

        // Write significand (little-endian)
        if sig_bytes > 0 {
            let sig_le = value.significand.to_le_bytes();
            self.write_bytes(&sig_le[..sig_bytes])?;
        }

        self.toggle_object_state();
        Ok(())
    }

    /// Encode a string.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if writing fails.
    #[allow(clippy::cast_possible_truncation)] // len <= 15 checked before cast to u8
    pub fn write_str(&mut self, value: &str) -> Result<()> {
        let bytes = value.as_bytes();
        let len = bytes.len();

        if len <= 15 {
            // Short string: type code encodes length
            self.write_byte(type_code::STRING0 + len as u8)?;
            self.write_bytes(bytes)?;
        } else {
            // Long string
            self.write_byte(type_code::STRING_LONG)?;
            self.write_length_field(len as u64, false)?;
            self.write_bytes(bytes)?;
        }

        self.toggle_object_state();
        Ok(())
    }

    /// Begin encoding an array with known element count.
    ///
    /// With chunked container encoding, the element count must be known upfront.
    /// All elements should be written after this call.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ExpectedObjectKey`] if called when an object key is expected.
    pub fn begin_array(&mut self, element_count: usize) -> Result<()> {
        if self.expecting_object_key() {
            return Err(Error::ExpectedObjectKey);
        }
        self.write_byte(type_code::ARRAY)?;
        self.write_length_field(element_count as u64, false)?;
        self.containers.push(ContainerState {
            is_object: false,
            expecting_key: false,
        });
        Ok(())
    }

    /// Begin encoding an object with known pair count.
    ///
    /// With chunked container encoding, the pair count must be known upfront.
    /// All key-value pairs should be written after this call.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ExpectedObjectKey`] if called when an object key is expected.
    pub fn begin_object(&mut self, pair_count: usize) -> Result<()> {
        if self.expecting_object_key() {
            return Err(Error::ExpectedObjectKey);
        }
        self.write_byte(type_code::OBJECT)?;
        self.write_length_field(pair_count as u64, false)?;
        self.containers.push(ContainerState {
            is_object: true,
            expecting_key: true,
        });
        Ok(())
    }

    /// End the current container (array or object).
    ///
    /// With chunked encoding, this just updates internal state (the chunk header
    /// already specified the container size).
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - There is no open container to close
    /// - An object is closed while expecting a value (odd number of elements)
    pub fn end_container(&mut self) -> Result<()> {
        let container = self
            .containers
            .pop()
            .ok_or(Error::UnbalancedContainers)?;

        // Can't close an object while expecting a value
        if container.is_object && !container.expecting_key {
            return Err(Error::ExpectedObjectValue);
        }

        self.toggle_object_state();
        Ok(())
    }

    /// Finish encoding and ensure all containers are closed.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnclosedContainer`] if there are unclosed containers.
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
    #[allow(clippy::cast_possible_truncation)] // value <= 100 checked; byte_count <= 8
    fn write_unsigned_int(&mut self, value: u64) -> Result<()> {
        // Small integer range: 0-100 (type codes 0x64-0xc8)
        if value <= 100 {
            // type_code = value + 100
            return self.write_byte((value as u8) + 100);
        }

        let byte_count = required_unsigned_bytes_min1(value);

        // Check if MSB is set - if not, we can use signed encoding (preferred per spec)
        let msb_set = (value >> (byte_count * 8 - 1)) & 1 != 0;
        let type_code = if msb_set {
            type_code::UINT8 + (byte_count as u8) - 1
        } else {
            type_code::SINT8 + (byte_count as u8) - 1
        };

        self.write_byte(type_code)?;
        let bytes = value.to_le_bytes();
        self.write_bytes(&bytes[..byte_count])
    }

    /// Write a signed integer using the optimal encoding.
    #[allow(clippy::cast_possible_truncation)] // -100..=100 checked; byte_count <= 8
    #[allow(clippy::cast_sign_loss)] // Intentional: i64 to u64 when positive
    fn write_signed_int(&mut self, value: i64) -> Result<()> {
        // Small integer range: -100 to 100 (type codes 0x00-0xc8)
        if (-100..=100).contains(&value) {
            // type_code = value + 100
            return self.write_byte((value + 100) as u8);
        }

        let byte_count = required_signed_bytes_min1(value);

        // Check if value is positive and fits in fewer bytes as unsigned
        if value > 0 {
            let unsigned_bytes = required_unsigned_bytes_min1(value as u64);
            if unsigned_bytes < byte_count {
                // Use unsigned encoding
                self.write_byte(type_code::UINT8 + (unsigned_bytes as u8) - 1)?;
                let bytes = (value as u64).to_le_bytes();
                return self.write_bytes(&bytes[..unsigned_bytes]);
            }
        }

        // Use signed encoding
        self.write_byte(type_code::SINT8 + (byte_count as u8) - 1)?;
        let bytes = value.to_le_bytes();
        self.write_bytes(&bytes[..byte_count])
    }

    /// Write a float using the optimal encoding (16, 32, or 64 bit).
    #[allow(clippy::cast_possible_truncation)] // Intentional: testing if f64 fits in f32
    fn write_float(&mut self, value: f64) -> Result<()> {
        let f32_val = value as f32;
        let f32_bits = f32_val.to_bits();

        // Try bfloat16: truncate f32 to upper 16 bits
        let bf16_bits = f32_bits & 0xffff_0000;
        let bf16_as_f32 = f32::from_bits(bf16_bits);
        #[allow(clippy::float_cmp)]
        if f64::from(bf16_as_f32) == value {
            // Can use bfloat16
            self.write_byte(type_code::FLOAT16)?;
            let bytes = ((f32_bits >> 16) as u16).to_le_bytes();
            return self.write_bytes(&bytes);
        }

        // Try f32
        #[allow(clippy::float_cmp)]
        if f64::from(f32_val) == value {
            self.write_byte(type_code::FLOAT32)?;
            let bytes = f32_val.to_le_bytes();
            return self.write_bytes(&bytes);
        }

        // Use f64
        self.write_byte(type_code::FLOAT64)?;
        let bytes = value.to_le_bytes();
        self.write_bytes(&bytes)
    }

    /// Write a length field with the given value and continuation bit.
    ///
    /// The length field uses a compact encoding where the header byte
    /// contains trailing 1s terminated by a 0 to indicate the field size.
    pub(crate) fn write_length_field(&mut self, length: u64, continuation: bool) -> Result<()> {
        // Payload = (length << 1) | continuation_bit
        let payload = (length << 1) | u64::from(continuation);

        // For very large payloads (> 56 bits), use the 9-byte encoding
        if payload > 0x00ff_ffff_ffff_ffff {
            self.write_byte(0xff)?;
            return self.write_bytes(&payload.to_le_bytes());
        }

        // Calculate extra bytes needed (0-7)
        let extra_bytes = calc_length_extra_bytes(payload);

        // Shift payload left to make room for count field
        // Then add trailing 1s (but not the terminating 0)
        let shifted = (payload << (1 + extra_bytes)) | ((1u64 << extra_bytes) - 1);

        let bytes = shifted.to_le_bytes();
        self.write_bytes(&bytes[..=extra_bytes])
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
    // (63 - leading_zeros) / 8 + 1
    let bits = 64 - value.leading_zeros() as usize;
    (bits + 7) / 8
}

/// Calculate the number of bytes required to store an unsigned integer (minimum 0).
#[inline]
fn required_unsigned_bytes_min0(value: u64) -> usize {
    if value == 0 {
        return 0;
    }
    required_unsigned_bytes_min1(value)
}

/// Calculate the number of bytes required to store a signed integer (minimum 1).
#[inline]
fn required_signed_bytes_min1(value: i64) -> usize {
    if value == 0 {
        return 1;
    }

    // Count leading redundant sign bits
    let redundant = if value < 0 {
        value.leading_ones() as usize
    } else {
        value.leading_zeros() as usize
    };

    // We need at least one sign bit, so subtract 1 from redundant count
    let significant_bits = 64 - redundant + 1;
    (significant_bits + 7) / 8
}

/// Calculate the number of bytes required to store a signed integer (minimum 0).
#[inline]
fn required_signed_bytes_min0(value: i64) -> usize {
    if value == 0 {
        return 0;
    }
    required_signed_bytes_min1(value)
}

/// Calculate extra bytes needed for length field encoding.
/// The overhead is 1 bit per byte, giving 7 payload bits per byte.
#[inline]
fn calc_length_extra_bytes(payload: u64) -> usize {
    // Branchless version using saturating arithmetic.
    // When payload is 0, leading_zeros() is 64, saturating_sub gives 0, and 0/7 = 0.
    // For other values, this computes (highest_bit_position) / 7.
    let highest_bit = 63usize.saturating_sub(payload.leading_zeros() as usize);
    highest_bit / 7
}

// =============================================================================
// Convenience functions
// =============================================================================

/// Encode a value to a byte vector.
///
/// # Errors
///
/// Returns an error if serialization fails (e.g., NaN/infinity floats, I/O errors).
pub fn to_vec<T: serde::Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
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
        // 0 → type_code = 0 + 100 = 0x64
        let mut buf = Vec::new();
        let mut enc = Encoder::new(&mut buf);
        enc.write_i64(0).unwrap();
        assert_eq!(buf, vec![0x64]);

        // 100 → type_code = 100 + 100 = 0xc8
        buf.clear();
        let mut enc = Encoder::new(&mut buf);
        enc.write_i64(100).unwrap();
        assert_eq!(buf, vec![0xc8]);

        // -100 → type_code = -100 + 100 = 0x00
        buf.clear();
        let mut enc = Encoder::new(&mut buf);
        enc.write_i64(-100).unwrap();
        assert_eq!(buf, vec![0x00]);

        // -1 → type_code = -1 + 100 = 0x63
        buf.clear();
        let mut enc = Encoder::new(&mut buf);
        enc.write_i64(-1).unwrap();
        assert_eq!(buf, vec![0x63]);
    }

    #[test]
    fn test_encode_larger_ints() {
        // 1000 as sint16 (0xd9)
        let mut buf = Vec::new();
        let mut enc = Encoder::new(&mut buf);
        enc.write_i64(1000).unwrap();
        assert_eq!(buf, vec![0xd9, 0xe8, 0x03]);

        // 180 as uint8 (0xd0) - can't fit in small int (0-100)
        buf.clear();
        let mut enc = Encoder::new(&mut buf);
        enc.write_u64(180).unwrap();
        assert_eq!(buf, vec![0xd0, 0xb4]);
    }

    #[test]
    fn test_encode_null_bool() {
        let mut buf = Vec::new();
        let mut enc = Encoder::new(&mut buf);
        enc.write_null().unwrap();
        assert_eq!(buf, vec![0xf5]);

        buf.clear();
        let mut enc = Encoder::new(&mut buf);
        enc.write_bool(true).unwrap();
        assert_eq!(buf, vec![0xf7]);

        buf.clear();
        let mut enc = Encoder::new(&mut buf);
        enc.write_bool(false).unwrap();
        assert_eq!(buf, vec![0xf6]);
    }

    #[test]
    fn test_encode_short_string() {
        // Empty string: 0xe0
        let mut buf = Vec::new();
        let mut enc = Encoder::new(&mut buf);
        enc.write_str("").unwrap();
        assert_eq!(buf, vec![0xe0]);

        // "A": 0xe1 + 'A'
        buf.clear();
        let mut enc = Encoder::new(&mut buf);
        enc.write_str("A").unwrap();
        assert_eq!(buf, vec![0xe1, 0x41]);

        // "x": 0xe1 + 'x'
        buf.clear();
        let mut enc = Encoder::new(&mut buf);
        enc.write_str("x").unwrap();
        assert_eq!(buf, vec![0xe1, 0x78]);
    }

    #[test]
    fn test_encode_empty_containers() {
        // Empty array: 0xf8 + chunk(count=0, cont=0) = 0xf8 0x00
        let mut buf = Vec::new();
        let mut enc = Encoder::new(&mut buf);
        enc.begin_array(0).unwrap();
        enc.end_container().unwrap();
        assert_eq!(buf, vec![0xf8, 0x00]);

        // Empty object: 0xf9 + chunk(count=0, cont=0) = 0xf9 0x00
        buf.clear();
        let mut enc = Encoder::new(&mut buf);
        enc.begin_object(0).unwrap();
        enc.end_container().unwrap();
        assert_eq!(buf, vec![0xf9, 0x00]);
    }

    #[test]
    fn test_encode_array_with_values() {
        // [1, "x", null]
        // 0xf8 + chunk(count=3, cont=0) + 0x65 + 0xe1 0x78 + 0xf5
        // chunk: payload = (3 << 1) | 0 = 6, encoded = (6 << 1) | 0 = 12 = 0x0c
        let mut buf = Vec::new();
        let mut enc = Encoder::new(&mut buf);
        enc.begin_array(3).unwrap();
        enc.write_i64(1).unwrap();
        enc.write_str("x").unwrap();
        enc.write_null().unwrap();
        enc.end_container().unwrap();
        assert_eq!(buf, vec![0xf8, 0x0c, 0x65, 0xe1, 0x78, 0xf5]);
    }

    #[test]
    fn test_encode_object() {
        // {"a": 1}
        // 0xf9 + chunk(count=1, cont=0) + 0xe1 0x61 + 0x65
        // chunk: payload = (1 << 1) | 0 = 2, encoded = (2 << 1) | 0 = 4 = 0x04
        let mut buf = Vec::new();
        let mut enc = Encoder::new(&mut buf);
        enc.begin_object(1).unwrap();
        enc.write_str("a").unwrap();
        enc.write_i64(1).unwrap();
        enc.end_container().unwrap();
        assert_eq!(buf, vec![0xf9, 0x04, 0xe1, 0x61, 0x65]);
    }

    #[test]
    fn test_encode_float16() {
        // 1.125 as bfloat16 = 0x3f90
        let mut buf = Vec::new();
        let mut enc = Encoder::new(&mut buf);
        enc.write_f64(1.125).unwrap();
        assert_eq!(buf, vec![0xf2, 0x90, 0x3f]);
    }

    #[test]
    fn test_length_field_encoding() {
        let mut buf = Vec::new();
        let mut enc = Encoder::new(&mut buf);

        // Length 0, no continuation → payload = 0, encoded = 0x00
        enc.write_length_field(0, false).unwrap();
        assert_eq!(buf, vec![0x00]);

        // Length 0, with continuation → payload = 1, encoded = 0x02
        buf.clear();
        let mut enc = Encoder::new(&mut buf);
        enc.write_length_field(0, true).unwrap();
        assert_eq!(buf, vec![0x02]);

        // Length 63, no continuation → payload = 126 = 0x7e, encoded = 0xfc
        buf.clear();
        let mut enc = Encoder::new(&mut buf);
        enc.write_length_field(63, false).unwrap();
        assert_eq!(buf, vec![0xfc]);

        // Length 64, no continuation → payload = 128, needs 2 bytes
        buf.clear();
        let mut enc = Encoder::new(&mut buf);
        enc.write_length_field(64, false).unwrap();
        assert_eq!(buf, vec![0x01, 0x02]);
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
