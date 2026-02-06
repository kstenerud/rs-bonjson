// ABOUTME: High-performance BONJSON binary decoder.
// ABOUTME: Uses delimiter-terminated containers (0xFE) and FF-terminated long strings.

#![allow(clippy::missing_errors_doc)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]

use crate::error::{Error, Result};
use crate::types::{limits, type_code, BigNumber, zigzag_decode, leb128_decode};
use std::borrow::Cow;

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

/// Delete invalid UTF-8 bytes, keeping only valid UTF-8 sequences.
fn delete_invalid_utf8(bytes: &[u8]) -> String {
    let mut result = String::new();
    let mut i = 0;
    while i < bytes.len() {
        match std::str::from_utf8(&bytes[i..]) {
            Ok(valid) => {
                result.push_str(valid);
                break;
            }
            Err(e) => {
                let valid_up_to = e.valid_up_to();
                if valid_up_to > 0 {
                    // Safety: from_utf8 confirmed these bytes are valid
                    result.push_str(unsafe { std::str::from_utf8_unchecked(&bytes[i..i + valid_up_to]) });
                }
                // Skip the invalid byte(s)
                i += valid_up_to + e.error_len().unwrap_or(1);
            }
        }
    }
    result
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

/// How to handle NaN and Infinity float values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NanInfinityMode {
    /// Reject NaN and Infinity values (default)
    Reject,
    /// Allow NaN and Infinity as float values
    Allow,
    /// Convert NaN and Infinity to string values
    Stringify,
}

impl Default for NanInfinityMode {
    fn default() -> Self {
        Self::Reject
    }
}

/// How to handle BigNumber values that exceed configured limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutOfRangeMode {
    /// Return an error (default)
    Error,
    /// Convert to string representation
    Stringify,
}

impl Default for OutOfRangeMode {
    fn default() -> Self {
        Self::Error
    }
}

/// How to handle invalid UTF-8 in strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidUtf8Mode {
    /// Reject invalid UTF-8 (default)
    Reject,
    /// Replace invalid bytes with U+FFFD replacement character
    Replace,
    /// Delete invalid bytes
    Delete,
}

impl Default for InvalidUtf8Mode {
    fn default() -> Self {
        Self::Reject
    }
}

/// Unicode normalization mode for string comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnicodeNormalization {
    /// No normalization (default)
    None,
    /// NFC normalization
    Nfc,
}

impl Default for UnicodeNormalization {
    fn default() -> Self {
        Self::None
    }
}

/// Configuration options for the decoder.
#[derive(Debug, Clone)]
pub struct DecoderConfig {
    /// Allow NUL characters in strings (default: false)
    pub allow_nul: bool,
    /// How to handle NaN and Infinity values (default: Reject)
    pub nan_infinity_mode: NanInfinityMode,
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
    /// Maximum BigNumber exponent (absolute value)
    pub max_bignumber_exponent: usize,
    /// Maximum BigNumber magnitude in bytes
    pub max_bignumber_magnitude: usize,
    /// How to handle out-of-range BigNumber values (default: Error)
    pub out_of_range_mode: OutOfRangeMode,
    /// How to handle invalid UTF-8 in strings (default: Reject)
    pub invalid_utf8_mode: InvalidUtf8Mode,
    /// Unicode normalization mode (default: None).
    /// Requires the `unicode-normalization` feature for Nfc mode.
    pub unicode_normalization: UnicodeNormalization,
}

impl Default for DecoderConfig {
    fn default() -> Self {
        Self {
            allow_nul: false,
            nan_infinity_mode: NanInfinityMode::default(),
            allow_trailing_bytes: false,
            duplicate_key_mode: DuplicateKeyMode::default(),
            max_depth: limits::MAX_DEPTH,
            max_container_size: limits::MAX_CONTAINER_SIZE,
            max_string_length: limits::MAX_STRING_LENGTH,
            max_document_size: limits::MAX_DOCUMENT_SIZE,
            max_bignumber_exponent: limits::MAX_BIGNUMBER_EXPONENT,
            max_bignumber_magnitude: limits::MAX_BIGNUMBER_MAGNITUDE,
            out_of_range_mode: OutOfRangeMode::default(),
            invalid_utf8_mode: InvalidUtf8Mode::default(),
            unicode_normalization: UnicodeNormalization::default(),
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
    String(Cow<'a, str>),
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
            let s = self.decode_string_content_cow(len)?;
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
                let s = self.decode_long_string_content_cow()?;
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
        let fill = ((bytes[size - 1] as i8) >> 7) as u8;
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
        let value = f64::from_le_bytes(bytes.try_into().unwrap());
        self.check_float(value)?;
        Ok(value)
    }

    /// Check if a float value is allowed.
    #[inline]
    fn check_float(&self, value: f64) -> Result<()> {
        if self.config.nan_infinity_mode == NanInfinityMode::Reject {
            if value.is_nan() {
                return Err(Error::NanNotAllowed);
            }
            if value.is_infinite() {
                return Err(Error::InfinityNotAllowed);
            }
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

        if !self.config.allow_nul && memchr::memchr(0, bytes).is_some() {
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

    /// Decode string content with invalid UTF-8 handling.
    /// Returns Cow::Borrowed for valid UTF-8, Cow::Owned for replaced/deleted.
    fn decode_string_content_cow(&mut self, len: usize) -> Result<Cow<'a, str>> {
        if len > self.config.max_string_length {
            return Err(Error::MaxStringLengthExceeded);
        }

        let bytes = self.read_bytes(len)?;

        let s = match validate_utf8(bytes) {
            Ok(s) => Cow::Borrowed(s),
            Err(_) => match self.config.invalid_utf8_mode {
                InvalidUtf8Mode::Reject => return Err(Error::InvalidUtf8),
                InvalidUtf8Mode::Replace => Cow::Owned(String::from_utf8_lossy(bytes).into_owned()),
                InvalidUtf8Mode::Delete => Cow::Owned(delete_invalid_utf8(bytes)),
            },
        };

        if !self.config.allow_nul && memchr::memchr(0, bytes).is_some() {
            return Err(Error::NulCharacter);
        }

        Ok(s)
    }

    /// Decode long string content with invalid UTF-8 handling.
    fn decode_long_string_content_cow(&mut self) -> Result<Cow<'a, str>> {
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

            let s = match validate_utf8(bytes) {
                Ok(s) => Cow::Borrowed(s),
                Err(_) => match self.config.invalid_utf8_mode {
                    InvalidUtf8Mode::Reject => return Err(Error::InvalidUtf8),
                    InvalidUtf8Mode::Replace => Cow::Owned(String::from_utf8_lossy(bytes).into_owned()),
                    InvalidUtf8Mode::Delete => Cow::Owned(delete_invalid_utf8(bytes)),
                },
            };

            if !self.config.allow_nul && memchr::memchr(0, bytes).is_some() {
                return Err(Error::NulCharacter);
            }

            return Ok(s);
        }

        Err(Error::Truncated)
    }

    /// Decode a BigNumber (zigzag LEB128 exponent + zigzag LEB128 signed_length + LE magnitude).
    fn decode_big_number(&mut self) -> Result<DecodedValue<'a>> {
        let remaining = &self.data[self.pos..];

        // Decode exponent
        let (exp_raw, exp_consumed) = leb128_decode(remaining)
            .ok_or(Error::Truncated)?;
        self.pos += exp_consumed;
        let exponent = zigzag_decode(exp_raw);

        // Check exponent limit
        if (exponent.unsigned_abs() as usize) > self.config.max_bignumber_exponent {
            if self.config.out_of_range_mode != OutOfRangeMode::Stringify {
                return Err(Error::MaxBignumberExponentExceeded);
            }
        }

        // Decode signed_length
        let remaining = &self.data[self.pos..];
        let (slen_raw, slen_consumed) = leb128_decode(remaining)
            .ok_or(Error::Truncated)?;
        self.pos += slen_consumed;
        let signed_length = zigzag_decode(slen_raw);

        if signed_length == 0 {
            return Ok(DecodedValue::BigNumber(BigNumber::new(1, 0, exponent)));
        }

        let sign: i8 = if signed_length < 0 { -1 } else { 1 };
        let byte_count = signed_length.unsigned_abs() as usize;

        // Check magnitude limit (also enforces u64 range since default max is 8)
        if byte_count > self.config.max_bignumber_magnitude {
            if self.config.out_of_range_mode != OutOfRangeMode::Stringify {
                return Err(Error::MaxBignumberMagnitudeExceeded);
            }
        }

        // Hard safety cap for stringify mode, and always enforce u64 range
        if byte_count > 8 {
            return Err(Error::InvalidData(
                "BigNumber magnitude exceeds u64 range".into(),
            ));
        }

        // Read raw LE magnitude bytes
        let magnitude_bytes = self.read_bytes(byte_count)?;

        // Validate normalization: last byte (most significant) must be non-zero
        if magnitude_bytes[byte_count - 1] == 0 {
            return Err(Error::InvalidData(
                "non-normalized BigNumber magnitude".into(),
            ));
        }

        let mut buf = [0u8; 8];
        buf[..byte_count].copy_from_slice(magnitude_bytes);
        let significand = u64::from_le_bytes(buf);

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
    use std::borrow::Cow;

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
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::String(Cow::Borrowed("")));

        // "x": 0xd1 + 'x'
        let mut dec = Decoder::new(&[0xd1, 0x78]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::String(Cow::Borrowed("x")));
    }

    #[test]
    fn test_decode_long_string() {
        // "abcdefghijklmnop" (16 bytes): FF + data + FF
        let mut data = vec![0xff];
        data.extend_from_slice(b"abcdefghijklmnop");
        data.push(0xff);
        let mut dec = Decoder::new(&data);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::String(Cow::Borrowed("abcdefghijklmnop")));
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
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::String(Cow::Borrowed("x")));
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
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::String(Cow::Borrowed("a")));
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
