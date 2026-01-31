// ABOUTME: High-performance BONJSON binary decoder.
// ABOUTME: Uses delimiter-terminated containers (0xFE) and FF-terminated long strings.

#![allow(clippy::missing_errors_doc)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]

use crate::error::{Error, Result};
use crate::types::{limits, type_code, BigNumber, zigzag_decode, leb128_decode};

/// Validate and convert bytes to a UTF-8 string.
/// Uses simdutf8 for SIMD-accelerated validation when the feature is enabled.
#[cfg(feature = "simd-utf8")]
#[inline]
fn validate_utf8(bytes: &[u8]) -> Result<&str> {
    simdutf8::basic::from_utf8(bytes).map_err(|_| Error::InvalidUtf8)
}

#[cfg(not(feature = "simd-utf8"))]
#[inline]
fn validate_utf8(bytes: &[u8]) -> Result<&str> {
    std::str::from_utf8(bytes).map_err(|_| Error::InvalidUtf8)
}

/// How to handle duplicate keys in objects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuplicateKeyMode {
    /// Raise an error on duplicate keys (default per spec)
    Error,
    /// Keep the first value, ignore subsequent duplicates
    KeepFirst,
    /// Keep the last value, overwrite earlier values
    KeepLast,
}

impl Default for DuplicateKeyMode {
    fn default() -> Self {
        Self::Error
    }
}

/// Configuration options for the decoder.
#[derive(Debug, Clone)]
pub struct DecoderConfig {
    /// Allow NUL characters in strings (default: false)
    pub allow_nul: bool,
    /// Allow NaN and Infinity values (default: false)
    pub allow_nan_infinity: bool,
    /// Allow trailing bytes after the document (default: false)
    pub allow_trailing_bytes: bool,
    /// How to handle duplicate keys (default: Error)
    pub duplicate_key_mode: DuplicateKeyMode,
    /// Maximum container nesting depth
    pub max_depth: usize,
    /// Maximum elements in a container
    pub max_container_size: usize,
    /// Maximum string length in bytes
    pub max_string_length: usize,
    /// Maximum document size in bytes
    pub max_document_size: usize,
}

impl Default for DecoderConfig {
    fn default() -> Self {
        Self {
            allow_nul: false,
            allow_nan_infinity: false,
            allow_trailing_bytes: false,
            duplicate_key_mode: DuplicateKeyMode::default(),
            max_depth: limits::MAX_DEPTH,
            max_container_size: limits::MAX_CONTAINER_SIZE,
            max_string_length: limits::MAX_STRING_LENGTH,
            max_document_size: limits::MAX_DOCUMENT_SIZE,
        }
    }
}

/// A BONJSON decoder that reads from a byte slice.
pub struct Decoder<'a> {
    data: &'a [u8],
    pos: usize,
    config: DecoderConfig,
    /// Stack tracking container depth (true = object)
    containers: Vec<bool>,
}

/// The type of value that was decoded.
#[derive(Debug, Clone, PartialEq)]
pub enum DecodedValue<'a> {
    Null,
    Bool(bool),
    Int(i64),
    UInt(u64),
    Float(f64),
    BigNumber(BigNumber),
    String(&'a str),
    ArrayStart,
    ObjectStart,
    ContainerEnd,
}

impl<'a> Decoder<'a> {
    /// Create a new decoder for the given data.
    #[must_use]
    pub fn new(data: &'a [u8]) -> Self {
        Self::with_config(data, DecoderConfig::default())
    }

    /// Create a new decoder with custom configuration.
    #[must_use]
    pub fn with_config(data: &'a [u8], config: DecoderConfig) -> Self {
        Self {
            data,
            pos: 0,
            config,
            containers: Vec::new(),
        }
    }

    /// Check document size limit (called once at start of decoding).
    #[inline]
    pub fn check_document_size(&self) -> Result<()> {
        if self.data.len() > self.config.max_document_size {
            return Err(Error::MaxDocumentSizeExceeded);
        }
        Ok(())
    }

    /// Get the current position in the input.
    #[must_use]
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Get the remaining bytes.
    #[must_use]
    pub fn remaining(&self) -> &'a [u8] {
        &self.data[self.pos..]
    }

    /// Check if we've reached the end of input.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pos >= self.data.len()
    }

    /// Get the decoder configuration.
    #[must_use]
    pub fn config(&self) -> &DecoderConfig {
        &self.config
    }

    /// Skip a single byte (for internal use after peeking).
    #[inline]
    pub(crate) fn skip_byte(&mut self) {
        self.pos += 1;
    }

    /// Read a byte without bounds checking (caller must ensure there's data).
    #[inline]
    pub(crate) fn read_byte_unchecked(&mut self) -> u8 {
        let b = self.data[self.pos];
        self.pos += 1;
        b
    }

    /// Read a single byte, advancing position.
    #[inline]
    fn read_byte(&mut self) -> Result<u8> {
        if self.pos >= self.data.len() {
            return Err(Error::Truncated);
        }
        let byte = self.data[self.pos];
        self.pos += 1;
        Ok(byte)
    }

    /// Read exactly n bytes.
    #[inline]
    fn read_bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.pos + n > self.data.len() {
            return Err(Error::Truncated);
        }
        let bytes = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(bytes)
    }

    // =========================================================================
    // Unchecked methods for serde deserializer
    // =========================================================================

    /// Decode the next value without container state tracking.
    #[inline]
    pub(crate) fn decode_value_unchecked(&mut self) -> Result<DecodedValue<'a>> {
        let tc = self.read_byte()?;
        self.decode_value_with_type_code(tc)
    }

    /// Peek at the next type code without consuming it.
    #[inline]
    pub(crate) fn peek_type_code(&self) -> Result<u8> {
        if self.pos >= self.data.len() {
            return Err(Error::Truncated);
        }
        Ok(self.data[self.pos])
    }

    /// Check if at container end (next byte is 0xFE) and consume it if so.
    #[inline]
    pub(crate) fn try_consume_container_end(&mut self) -> Result<bool> {
        if self.pos < self.data.len() && self.data[self.pos] == type_code::CONTAINER_END {
            self.pos += 1;
            self.containers.pop();
            Ok(true)
        } else if self.pos >= self.data.len() {
            Err(Error::Truncated)
        } else {
            Ok(false)
        }
    }

    /// Expect and skip an array start marker.
    #[inline]
    pub(crate) fn expect_array_start(&mut self) -> Result<()> {
        let tc = self.read_byte()?;
        if tc != type_code::ARRAY {
            return Err(Error::Custom(format!("expected array, got 0x{tc:02x}")));
        }
        self.begin_container(false)
    }

    /// Expect and skip an object start marker.
    #[inline]
    pub(crate) fn expect_object_start(&mut self) -> Result<()> {
        let tc = self.read_byte()?;
        if tc != type_code::OBJECT {
            return Err(Error::Custom(format!("expected object, got 0x{tc:02x}")));
        }
        self.begin_container(true)
    }

    /// Decode an i64 directly.
    #[inline]
    #[allow(clippy::cast_possible_wrap)]
    pub(crate) fn decode_i64_direct(&mut self) -> Result<i64> {
        let tc = self.read_byte()?;

        if type_code::is_small_int(tc) {
            return Ok(i64::from(type_code::small_int_value(tc)));
        }

        if type_code::is_any_int(tc) {
            let size = type_code::int_size(tc);
            return if type_code::int_is_signed(tc) {
                self.read_signed_int_sized(size)
            } else {
                Ok(self.read_unsigned_int_sized(size)? as i64)
            };
        }

        Err(Error::Custom(format!("expected integer, got 0x{tc:02x}")))
    }

    /// Decode a u64 directly.
    #[inline]
    pub(crate) fn decode_u64_direct(&mut self) -> Result<u64> {
        let tc = self.read_byte()?;

        if type_code::is_small_int(tc) {
            let val = type_code::small_int_value(tc);
            if val < 0 {
                return Err(Error::Custom("cannot decode negative int as u64".into()));
            }
            return Ok(val as u64);
        }

        if type_code::is_any_int(tc) {
            let size = type_code::int_size(tc);
            return if type_code::int_is_signed(tc) {
                let signed_val = self.read_signed_int_sized(size)?;
                if signed_val < 0 {
                    return Err(Error::ValueOutOfRange);
                }
                Ok(signed_val as u64)
            } else {
                self.read_unsigned_int_sized(size)
            };
        }

        Err(Error::Custom(format!("expected unsigned integer, got 0x{tc:02x}")))
    }

    /// Decode a bool directly.
    #[inline]
    pub(crate) fn decode_bool_direct(&mut self) -> Result<bool> {
        let tc = self.read_byte()?;
        match tc {
            type_code::TRUE => Ok(true),
            type_code::FALSE => Ok(false),
            _ => Err(Error::Custom(format!("expected bool, got 0x{tc:02x}"))),
        }
    }

    /// Decode a string directly.
    #[inline]
    pub(crate) fn decode_str_direct(&mut self) -> Result<&'a str> {
        let tc = self.read_byte()?;

        if type_code::is_short_string(tc) {
            let len = type_code::short_string_len(tc);
            return self.decode_string_content(len);
        }

        if tc == type_code::STRING_LONG {
            return self.decode_long_string_content();
        }

        Err(Error::ExpectedObjectKey)
    }

    /// Decode an f64 directly.
    #[inline]
    #[allow(clippy::cast_possible_wrap)]
    #[allow(clippy::cast_precision_loss)]
    pub(crate) fn decode_f64_direct(&mut self) -> Result<f64> {
        let tc = self.read_byte()?;

        if type_code::is_small_int(tc) {
            return Ok(f64::from(type_code::small_int_value(tc)));
        }

        if type_code::is_any_int(tc) {
            let size = type_code::int_size(tc);
            return if type_code::int_is_signed(tc) {
                Ok(self.read_signed_int_sized(size)? as f64)
            } else {
                Ok(self.read_unsigned_int_sized(size)? as f64)
            };
        }

        match tc {
            type_code::FLOAT32 => self.read_float32(),
            type_code::FLOAT64 => self.read_float64(),
            _ => Err(Error::Custom(format!("expected number, got 0x{tc:02x}"))),
        }
    }

    // =========================================================================
    // Internal methods
    // =========================================================================

    /// Begin a container (push onto stack, check depth).
    fn begin_container(&mut self, is_object: bool) -> Result<()> {
        if self.containers.len() >= self.config.max_depth {
            return Err(Error::MaxDepthExceeded);
        }
        self.containers.push(is_object);
        Ok(())
    }

    /// Decode a value given its type code.
    #[allow(clippy::cast_possible_wrap)]
    fn decode_value_with_type_code(&mut self, tc: u8) -> Result<DecodedValue<'a>> {
        // Small integers: 0x00-0xc8
        if type_code::is_small_int(tc) {
            return Ok(DecodedValue::Int(i64::from(type_code::small_int_value(tc))));
        }

        // Short strings: 0xd0-0xdf
        if type_code::is_short_string(tc) {
            let len = type_code::short_string_len(tc);
            let s = self.decode_string_content(len)?;
            return Ok(DecodedValue::String(s));
        }

        // Integers: 0xe0-0xe7
        if type_code::is_any_int(tc) {
            let size = type_code::int_size(tc);
            return if type_code::int_is_signed(tc) {
                let val = self.read_signed_int_sized(size)?;
                Ok(DecodedValue::Int(val))
            } else {
                let val = self.read_unsigned_int_sized(size)?;
                Ok(DecodedValue::UInt(val))
            };
        }

        match tc {
            type_code::BIG_NUMBER => self.decode_big_number(),
            type_code::FLOAT32 => {
                let f = self.read_float32()?;
                Ok(DecodedValue::Float(f))
            }
            type_code::FLOAT64 => {
                let f = self.read_float64()?;
                Ok(DecodedValue::Float(f))
            }
            type_code::NULL => Ok(DecodedValue::Null),
            type_code::FALSE => Ok(DecodedValue::Bool(false)),
            type_code::TRUE => Ok(DecodedValue::Bool(true)),
            type_code::STRING_LONG => {
                let s = self.decode_long_string_content()?;
                Ok(DecodedValue::String(s))
            }
            type_code::ARRAY => {
                self.begin_container(false)?;
                Ok(DecodedValue::ArrayStart)
            }
            type_code::OBJECT => {
                self.begin_container(true)?;
                Ok(DecodedValue::ObjectStart)
            }
            type_code::CONTAINER_END => {
                self.containers.pop().ok_or(Error::UnbalancedContainers)?;
                Ok(DecodedValue::ContainerEnd)
            }
            _ => Err(Error::InvalidTypeCode(tc)),
        }
    }

    /// Read an unsigned integer of given byte size (1, 2, 4, or 8).
    #[inline]
    fn read_unsigned_int_sized(&mut self, size: usize) -> Result<u64> {
        let bytes = self.read_bytes(size)?;
        let mut buf = [0u8; 8];
        buf[..size].copy_from_slice(bytes);
        Ok(u64::from_le_bytes(buf))
    }

    /// Read a signed integer of given byte size (1, 2, 4, or 8).
    #[inline]
    fn read_signed_int_sized(&mut self, size: usize) -> Result<i64> {
        let bytes = self.read_bytes(size)?;
        let sign_bit = (bytes[size - 1] >> 7) & 1;
        let fill: u8 = if sign_bit == 1 { 0xff } else { 0x00 };
        let mut buf = [fill; 8];
        buf[..size].copy_from_slice(bytes);
        Ok(i64::from_le_bytes(buf))
    }

    /// Read a float32 value.
    #[inline]
    fn read_float32(&mut self) -> Result<f64> {
        let bytes = self.read_bytes(4)?;
        let value = f64::from(f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
        self.check_float(value)?;
        Ok(value)
    }

    /// Read a float64 value.
    #[inline]
    fn read_float64(&mut self) -> Result<f64> {
        let bytes = self.read_bytes(8)?;
        let mut buf = [0u8; 8];
        buf.copy_from_slice(bytes);
        let value = f64::from_le_bytes(buf);
        self.check_float(value)?;
        Ok(value)
    }

    /// Check if a float value is allowed.
    #[inline]
    fn check_float(&self, value: f64) -> Result<()> {
        if !self.config.allow_nan_infinity && (value.is_nan() || value.is_infinite()) {
            return Err(Error::InvalidData("NaN or Infinity not allowed".into()));
        }
        Ok(())
    }

    /// Decode string content (short string: after type code, known length).
    fn decode_string_content(&mut self, len: usize) -> Result<&'a str> {
        if len > self.config.max_string_length {
            return Err(Error::MaxStringLengthExceeded);
        }

        let bytes = self.read_bytes(len)?;
        let s = validate_utf8(bytes)?;

        if !self.config.allow_nul && bytes.contains(&0) {
            return Err(Error::NulCharacter);
        }

        Ok(s)
    }

    /// Decode long string content (FF-terminated: read until next 0xFF).
    /// Uses memchr for SIMD-accelerated scanning of the terminator byte.
    fn decode_long_string_content(&mut self) -> Result<&'a str> {
        let start = self.pos;
        let remaining = &self.data[start..];

        if let Some(offset) = memchr::memchr(0xFF, remaining) {
            let end = start + offset;
            self.pos = end + 1; // consume the terminator

            let len = offset;
            if len > self.config.max_string_length {
                return Err(Error::MaxStringLengthExceeded);
            }

            let bytes = &self.data[start..end];
            let s = validate_utf8(bytes)?;

            if !self.config.allow_nul && memchr::memchr(0, bytes).is_some() {
                return Err(Error::NulCharacter);
            }

            return Ok(s);
        }

        Err(Error::Truncated)
    }

    /// Decode a BigNumber (zigzag LEB128 exponent + zigzag LEB128 signed significand).
    fn decode_big_number(&mut self) -> Result<DecodedValue<'a>> {
        let remaining = &self.data[self.pos..];

        // Decode exponent
        let (exp_raw, exp_consumed) = leb128_decode(remaining)
            .ok_or(Error::Truncated)?;
        self.pos += exp_consumed;
        let exponent = zigzag_decode(exp_raw);

        // Decode signed significand
        let remaining = &self.data[self.pos..];
        let (sig_raw, sig_consumed) = leb128_decode(remaining)
            .ok_or(Error::Truncated)?;
        self.pos += sig_consumed;
        let signed_sig = zigzag_decode(sig_raw);

        let sign: i8 = if signed_sig < 0 { -1 } else { 1 };
        let significand = signed_sig.unsigned_abs();

        Ok(DecodedValue::BigNumber(BigNumber::new(sign, significand, exponent)))
    }

    /// Decode the next value from the input.
    pub fn decode_value(&mut self) -> Result<DecodedValue<'a>> {
        let tc = self.read_byte()?;
        self.decode_value_with_type_code(tc)
    }

    /// Check if we're at the end of the current container (next byte is 0xFE).
    pub fn is_at_container_end(&self) -> Result<bool> {
        if self.pos >= self.data.len() {
            return Err(Error::Truncated);
        }
        Ok(self.data[self.pos] == type_code::CONTAINER_END)
    }

    /// End the current container (pop from stack and consume the 0xFE marker).
    pub fn end_container(&mut self) -> Result<()> {
        let tc = self.read_byte()?;
        if tc != type_code::CONTAINER_END {
            return Err(Error::Custom(format!("expected container end, got 0x{tc:02x}")));
        }
        self.containers.pop().ok_or(Error::UnbalancedContainers)?;
        Ok(())
    }

    /// Finish decoding and check for errors.
    pub fn finish(&self) -> Result<()> {
        if !self.containers.is_empty() {
            return Err(Error::UnclosedContainer);
        }
        if !self.config.allow_trailing_bytes && self.pos < self.data.len() {
            return Err(Error::TrailingBytes);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_small_ints() {
        let mut dec = Decoder::new(&[0x64]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Int(0));

        let mut dec = Decoder::new(&[0xc8]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Int(100));

        let mut dec = Decoder::new(&[0x00]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Int(-100));

        let mut dec = Decoder::new(&[0x63]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Int(-1));
    }

    #[test]
    fn test_decode_larger_ints() {
        // sint16 (0xe5) 1000
        let mut dec = Decoder::new(&[0xe5, 0xe8, 0x03]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Int(1000));

        // uint8 (0xe0) 180
        let mut dec = Decoder::new(&[0xe0, 0xb4]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::UInt(180));
    }

    #[test]
    fn test_decode_null_bool() {
        let mut dec = Decoder::new(&[0xcd]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Null);

        let mut dec = Decoder::new(&[0xcf]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Bool(true));

        let mut dec = Decoder::new(&[0xce]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Bool(false));
    }

    #[test]
    fn test_decode_short_string() {
        // Empty string: 0xd0
        let mut dec = Decoder::new(&[0xd0]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::String(""));

        // "x": 0xd1 + 'x'
        let mut dec = Decoder::new(&[0xd1, 0x78]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::String("x"));
    }

    #[test]
    fn test_decode_long_string() {
        // "abcdefghijklmnop" (16 bytes): FF + data + FF
        let mut data = vec![0xff];
        data.extend_from_slice(b"abcdefghijklmnop");
        data.push(0xff);
        let mut dec = Decoder::new(&data);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::String("abcdefghijklmnop"));
    }

    #[test]
    fn test_decode_empty_array() {
        // FC FE
        let mut dec = Decoder::new(&[0xfc, 0xfe]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::ArrayStart);
        assert!(dec.is_at_container_end().unwrap());
        dec.end_container().unwrap();
        dec.finish().unwrap();
    }

    #[test]
    fn test_decode_empty_object() {
        // FD FE
        let mut dec = Decoder::new(&[0xfd, 0xfe]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::ObjectStart);
        assert!(dec.is_at_container_end().unwrap());
        dec.end_container().unwrap();
        dec.finish().unwrap();
    }

    #[test]
    fn test_decode_array_with_values() {
        // [1, "x", null] → FC 65 D1 78 CD FE
        let data = [0xfc, 0x65, 0xd1, 0x78, 0xcd, 0xfe];
        let mut dec = Decoder::new(&data);

        assert_eq!(dec.decode_value().unwrap(), DecodedValue::ArrayStart);
        assert!(!dec.is_at_container_end().unwrap());
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Int(1));
        assert!(!dec.is_at_container_end().unwrap());
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::String("x"));
        assert!(!dec.is_at_container_end().unwrap());
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Null);
        assert!(dec.is_at_container_end().unwrap());
        dec.end_container().unwrap();
        dec.finish().unwrap();
    }

    #[test]
    fn test_decode_object() {
        // {"a": 1} → FD D1 61 65 FE
        let data = [0xfd, 0xd1, 0x61, 0x65, 0xfe];
        let mut dec = Decoder::new(&data);

        assert_eq!(dec.decode_value().unwrap(), DecodedValue::ObjectStart);
        assert!(!dec.is_at_container_end().unwrap());
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::String("a"));
        assert!(!dec.is_at_container_end().unwrap());
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Int(1));
        assert!(dec.is_at_container_end().unwrap());
        dec.end_container().unwrap();
        dec.finish().unwrap();
    }

    #[test]
    fn test_reserved_type_codes() {
        let mut dec = Decoder::new(&[0xc9]);
        assert!(matches!(dec.decode_value(), Err(Error::InvalidTypeCode(0xc9))));

        let mut dec = Decoder::new(&[0xe8]);
        assert!(matches!(dec.decode_value(), Err(Error::InvalidTypeCode(0xe8))));
    }

    #[test]
    fn test_truncated() {
        let mut dec = Decoder::new(&[0xe5, 0xe8]); // Missing second byte of int16
        assert!(matches!(dec.decode_value(), Err(Error::Truncated)));
    }

    #[test]
    fn test_trailing_bytes() {
        let mut dec = Decoder::new(&[0x64, 0x64]); // int 0 + extra byte
        dec.decode_value().unwrap();
        assert!(matches!(dec.finish(), Err(Error::TrailingBytes)));
    }
}
