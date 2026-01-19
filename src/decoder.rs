// ABOUTME: High-performance BONJSON binary decoder.
// ABOUTME: Uses compiler intrinsics (trailing_zeros) for efficient length field decoding.

// Allow intentional casts for binary format decoding - the format requires direct type casting
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

use crate::error::{Error, Result};
use crate::types::{limits, type_code, BigNumber};

/// Check if all bytes are ASCII (1-127, no NUL or high bytes).
/// For short strings (≤16 bytes), inline loop is faster than function calls.
#[inline]
fn is_short_ascii_no_nul(bytes: &[u8]) -> bool {
    for &b in bytes {
        if b == 0 || b >= 128 {
            return false;
        }
    }
    true
}

/// Validate UTF-8 and check for NUL bytes.
///
/// This uses two separate passes over the data, which may seem inefficient, but is
/// actually faster than a single-pass approach because:
/// 1. Stdlib's `contains()` and `from_utf8()` use highly optimized architecture-specific
///    SIMD implementations (SSE2/AVX2 on x86, NEON on ARM)
/// 2. Replicating these optimizations in a single-pass NUL+UTF-8 check would require
///    massive code bloat and platform-specific assembly
/// 3. Two SIMD-accelerated passes over cached data beats one scalar pass
#[inline]
fn validate_utf8_no_nul(bytes: &[u8]) -> std::result::Result<&str, Error> {
    if bytes.contains(&0) {
        return Err(Error::NulCharacter);
    }
    Ok(std::str::from_utf8(bytes)?)
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
    /// Maximum chunks per string
    pub max_chunks: usize,
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
            max_chunks: limits::MAX_CHUNKS,
            max_document_size: limits::MAX_DOCUMENT_SIZE,
        }
    }
}

/// A BONJSON decoder that reads from a byte slice.
pub struct Decoder<'a> {
    data: &'a [u8],
    pos: usize,
    config: DecoderConfig,
    /// Stack of container states
    containers: Vec<ContainerState>,
    /// Scratch buffer for multi-chunk string concatenation (reused across calls)
    scratch: Vec<u8>,
}

#[derive(Clone, Copy)]
struct ContainerState {
    is_object: bool,
    expecting_key: bool,
    element_count: usize,
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
    #[must_use] pub fn new(data: &'a [u8]) -> Self {
        Self::with_config(data, DecoderConfig::default())
    }

    /// Create a new decoder with custom configuration.
    #[must_use] pub fn with_config(data: &'a [u8], config: DecoderConfig) -> Self {
        Self {
            data,
            pos: 0,
            config,
            containers: Vec::new(),
            scratch: Vec::new(),
        }
    }

    /// Check document size limit (called once at start of decoding).
    ///
    /// # Errors
    ///
    /// Returns [`Error::MaxDocumentSizeExceeded`] if the input exceeds the configured limit.
    #[inline]
    pub fn check_document_size(&self) -> Result<()> {
        if self.data.len() > self.config.max_document_size {
            return Err(Error::MaxDocumentSizeExceeded);
        }
        Ok(())
    }

    /// Get the current position in the input.
    #[must_use] pub fn position(&self) -> usize {
        self.pos
    }

    /// Get the remaining bytes.
    #[must_use] pub fn remaining(&self) -> &'a [u8] {
        &self.data[self.pos..]
    }

    /// Check if we've reached the end of input.
    #[must_use] pub fn is_empty(&self) -> bool {
        self.pos >= self.data.len()
    }

    /// Get the decoder configuration.
    #[must_use] pub fn config(&self) -> &DecoderConfig {
        &self.config
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

    /// Increment element count in current container.
    /// For objects, only count after decoding values, not keys.
    #[inline]
    fn increment_element_count(&mut self) -> Result<()> {
        if let Some(container) = self.containers.last_mut() {
            // For objects, we toggle expecting_key BEFORE this call.
            // After toggling from key decode: expecting_key = false (skip count)
            // After toggling from value decode: expecting_key = true (count)
            // For arrays, always count.
            if !container.is_object || container.expecting_key {
                container.element_count += 1;
                if container.element_count > self.config.max_container_size {
                    return Err(Error::MaxContainerSizeExceeded);
                }
            }
        }
        Ok(())
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
    // Shared read methods for numeric types
    //
    // These methods read and decode raw values without state tracking.
    // Both checked and unchecked decode methods use these to avoid duplication.
    // Marked #[inline(always)] to ensure zero overhead.
    // =========================================================================

    /// Read an unsigned integer of the given type code.
    #[inline(always)]
    fn read_unsigned_int(&mut self, tc: u8) -> Result<u64> {
        let size = type_code::unsigned_int_size(tc);
        let bytes = self.read_bytes(size)?;
        let mut buf = [0u8; 8];
        buf[..size].copy_from_slice(bytes);
        Ok(u64::from_le_bytes(buf))
    }

    /// Read a signed integer of the given type code.
    #[inline(always)]
    fn read_signed_int(&mut self, tc: u8) -> Result<i64> {
        let size = type_code::signed_int_size(tc);
        let bytes = self.read_bytes(size)?;
        let sign_bit = (bytes[size - 1] >> 7) & 1;
        let fill: u8 = if sign_bit == 1 { 0xff } else { 0x00 };
        let mut buf = [fill; 8];
        buf[..size].copy_from_slice(bytes);
        Ok(i64::from_le_bytes(buf))
    }

    /// Read a bfloat16 value, checking for NaN/Infinity if configured.
    #[inline(always)]
    fn read_float16(&mut self) -> Result<f64> {
        let bytes = self.read_bytes(2)?;
        let bits = u16::from_le_bytes([bytes[0], bytes[1]]);
        let f32_bits = u32::from(bits) << 16;
        let value = f64::from(f32::from_bits(f32_bits));
        self.check_float(value)?;
        Ok(value)
    }

    /// Read a float32 value, checking for NaN/Infinity if configured.
    #[inline(always)]
    fn read_float32(&mut self) -> Result<f64> {
        let bytes = self.read_bytes(4)?;
        let value = f64::from(f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
        self.check_float(value)?;
        Ok(value)
    }

    /// Read a float64 value, checking for NaN/Infinity if configured.
    #[inline(always)]
    fn read_float64(&mut self) -> Result<f64> {
        let bytes = self.read_bytes(8)?;
        let mut buf = [0u8; 8];
        buf.copy_from_slice(bytes);
        let value = f64::from_le_bytes(buf);
        self.check_float(value)?;
        Ok(value)
    }

    /// Check if a float value is allowed (NaN/Infinity check).
    #[inline(always)]
    fn check_float(&self, value: f64) -> Result<()> {
        if !self.config.allow_nan_infinity && (value.is_nan() || value.is_infinite()) {
            return Err(Error::InvalidData("NaN or Infinity not allowed".into()));
        }
        Ok(())
    }

    // =========================================================================
    // Unchecked methods for serde deserializer
    //
    // These methods skip container state tracking (depth counting, key/value
    // alternation) for better performance. They are designed for:
    //
    // 1. The serde deserialization path, where Rust's type system guarantees
    //    correct structure (you can't deserialize an object key as an integer)
    //
    // 2. Trusted data sources where the BONJSON is known to be well-formed
    //
    // These methods still perform:
    // - Bounds checking on reads
    // - UTF-8 validation on strings
    // - NaN/Infinity rejection (unless configured otherwise)
    // - Resource limit checks (max string length, etc.)
    //
    // For untrusted data where you need full structural validation, use the
    // public checked methods (decode_value, etc.) instead.
    // =========================================================================

    /// Decode the next value without container state tracking.
    /// For use by serde deserializer which guarantees correct structure.
    #[inline]
    pub(crate) fn decode_value_unchecked(&mut self) -> Result<DecodedValue<'a>> {
        let tc = self.read_byte()?;
        self.decode_value_unchecked_with_type(tc)
    }

    /// Peek at the next type code without consuming it.
    #[inline]
    pub(crate) fn peek_type_code(&self) -> Result<u8> {
        if self.pos >= self.data.len() {
            return Err(Error::Truncated);
        }
        Ok(self.data[self.pos])
    }

    /// Check if next value is container end (without consuming).
    #[inline]
    pub(crate) fn is_at_container_end(&self) -> Result<bool> {
        if self.pos >= self.data.len() {
            return Err(Error::Truncated);
        }
        Ok(self.data[self.pos] == type_code::CONTAINER_END)
    }

    /// Skip the container end marker (assumes caller verified it's container end).
    #[inline]
    pub(crate) fn skip_container_end(&mut self) -> Result<()> {
        if self.pos >= self.data.len() {
            return Err(Error::Truncated);
        }
        if self.data[self.pos] != type_code::CONTAINER_END {
            return Err(Error::Custom("expected container end".into()));
        }
        self.pos += 1;
        Ok(())
    }

    /// Check and consume container end in one operation - returns true if consumed.
    #[inline]
    pub(crate) fn try_consume_container_end(&mut self) -> Result<bool> {
        if self.pos >= self.data.len() {
            return Err(Error::Truncated);
        }
        if self.data[self.pos] == type_code::CONTAINER_END {
            self.pos += 1;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Expect and skip an array start marker.
    #[inline]
    pub(crate) fn expect_array_start(&mut self) -> Result<()> {
        let tc = self.read_byte()?;
        if tc != type_code::ARRAY_START {
            return Err(Error::Custom("expected array".into()));
        }
        Ok(())
    }

    /// Expect and skip an object start marker.
    #[inline]
    pub(crate) fn expect_object_start(&mut self) -> Result<()> {
        let tc = self.read_byte()?;
        if tc != type_code::OBJECT_START {
            return Err(Error::Custom("expected object".into()));
        }
        Ok(())
    }

    /// Decode an i64 directly, without going through `DecodedValue`.
    #[inline]
    pub(crate) fn decode_i64_direct(&mut self) -> Result<i64> {
        let tc = self.read_byte()?;
        match tc {
            // Small integers: 0-100
            0x00..=0x64 => Ok(i64::from(tc)),
            // Small negative integers: -100 to -1
            0x9c..=0xff => Ok(i64::from(tc as i8)),
            // Signed integers
            0x78..=0x7f => {
                let size = type_code::signed_int_size(tc);
                let bytes = self.read_bytes(size)?;
                let sign_bit = (bytes[size - 1] >> 7) & 1;
                let fill: u8 = if sign_bit == 1 { 0xff } else { 0x00 };
                let mut buf = [fill; 8];
                buf[..size].copy_from_slice(bytes);
                Ok(i64::from_le_bytes(buf))
            }
            // Unsigned integers (if they fit in i64)
            0x70..=0x77 => {
                let size = type_code::unsigned_int_size(tc);
                let bytes = self.read_bytes(size)?;
                let mut buf = [0u8; 8];
                buf[..size].copy_from_slice(bytes);
                let value = u64::from_le_bytes(buf);
                if i64::try_from(value).is_ok() {
                    Ok(value as i64)
                } else {
                    Err(Error::ValueOutOfRange)
                }
            }
            _ => Err(Error::Custom("expected integer".into())),
        }
    }

    /// Decode a u64 directly, without going through `DecodedValue`.
    #[inline]
    pub(crate) fn decode_u64_direct(&mut self) -> Result<u64> {
        let tc = self.read_byte()?;
        match tc {
            // Small integers: 0-100
            0x00..=0x64 => Ok(u64::from(tc)),
            // Unsigned integers
            0x70..=0x77 => {
                let size = type_code::unsigned_int_size(tc);
                let bytes = self.read_bytes(size)?;
                let mut buf = [0u8; 8];
                buf[..size].copy_from_slice(bytes);
                Ok(u64::from_le_bytes(buf))
            }
            // Signed integers (if non-negative)
            0x78..=0x7f => {
                let size = type_code::signed_int_size(tc);
                let bytes = self.read_bytes(size)?;
                let sign_bit = (bytes[size - 1] >> 7) & 1;
                if sign_bit == 1 {
                    return Err(Error::ValueOutOfRange);
                }
                let mut buf = [0u8; 8];
                buf[..size].copy_from_slice(bytes);
                Ok(u64::from_le_bytes(buf))
            }
            _ => Err(Error::Custom("expected unsigned integer".into())),
        }
    }

    /// Decode a bool directly, without going through `DecodedValue`.
    #[inline]
    pub(crate) fn decode_bool_direct(&mut self) -> Result<bool> {
        let tc = self.read_byte()?;
        match tc {
            type_code::TRUE => Ok(true),
            type_code::FALSE => Ok(false),
            _ => Err(Error::Custom("expected bool".into())),
        }
    }

    /// Decode a string directly, without going through `DecodedValue`.
    #[inline]
    pub(crate) fn decode_str_direct(&mut self) -> Result<&'a str> {
        let tc = self.read_byte()?;
        match tc {
            0x80..=0x8f => {
                let len = type_code::short_string_len(tc);
                let bytes = self.read_bytes(len)?;
                // Fast path: short ASCII strings don't need UTF-8 validation
                // Short strings (up to 15 bytes) are common for field names
                if !self.config.allow_nul && is_short_ascii_no_nul(bytes) {
                    // SAFETY: ASCII bytes are always valid UTF-8
                    return Ok(unsafe { std::str::from_utf8_unchecked(bytes) });
                }
                if self.config.allow_nul {
                    Ok(std::str::from_utf8(bytes)?)
                } else {
                    validate_utf8_no_nul(bytes)
                }
            }
            type_code::STRING_LONG => {
                let pos_before_length = self.pos;
                let (length, continuation) = self.decode_length_field()?;
                if length > self.config.max_string_length as u64 {
                    return Err(Error::MaxStringLengthExceeded);
                }
                let bytes = self.read_bytes(length as usize)?;
                if !continuation {
                    // Use SIMD-accelerated validation for all string sizes
                    return if self.config.allow_nul {
                        Ok(std::str::from_utf8(bytes)?)
                    } else {
                        validate_utf8_no_nul(bytes)
                    };
                }
                // Multi-chunk string: fall back to full decode.
                // Back up to the start of the length field so decode_long_string
                // can re-read from the beginning.
                self.pos = pos_before_length;
                match self.decode_long_string()? {
                    DecodedValue::String(s) => Ok(s),
                    _ => unreachable!(),
                }
            }
            _ => Err(Error::Custom("expected string".into())),
        }
    }

    /// Decode an f64 directly, without going through `DecodedValue`.
    #[inline]
    pub(crate) fn decode_f64_direct(&mut self) -> Result<f64> {
        let tc = self.read_byte()?;
        match tc {
            // Small integers: 0-100
            0x00..=0x64 => Ok(f64::from(tc)),
            // Small negative integers: -100 to -1
            0x9c..=0xff => Ok(f64::from(tc as i8)),
            // Signed integers
            0x78..=0x7f => {
                let size = type_code::signed_int_size(tc);
                let bytes = self.read_bytes(size)?;
                let sign_bit = (bytes[size - 1] >> 7) & 1;
                let fill: u8 = if sign_bit == 1 { 0xff } else { 0x00 };
                let mut buf = [fill; 8];
                buf[..size].copy_from_slice(bytes);
                Ok(i64::from_le_bytes(buf) as f64)
            }
            // Unsigned integers
            0x70..=0x77 => {
                let size = type_code::unsigned_int_size(tc);
                let bytes = self.read_bytes(size)?;
                let mut buf = [0u8; 8];
                buf[..size].copy_from_slice(bytes);
                Ok(u64::from_le_bytes(buf) as f64)
            }
            // Float16
            type_code::FLOAT16 => {
                let bytes = self.read_bytes(2)?;
                let bits = u16::from_le_bytes([bytes[0], bytes[1]]);
                let f32_bits = u32::from(bits) << 16;
                let value = f64::from(f32::from_bits(f32_bits));
                self.check_float(value)?;
                Ok(value)
            }
            // Float32
            type_code::FLOAT32 => {
                let bytes = self.read_bytes(4)?;
                let value = f64::from(f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
                self.check_float(value)?;
                Ok(value)
            }
            // Float64
            type_code::FLOAT64 => {
                let bytes = self.read_bytes(8)?;
                let mut buf = [0u8; 8];
                buf.copy_from_slice(bytes);
                let value = f64::from_le_bytes(buf);
                self.check_float(value)?;
                Ok(value)
            }
            _ => Err(Error::Custom("expected number".into())),
        }
    }

    /// Decode a value given its type code, without state tracking.
    #[inline]
    fn decode_value_unchecked_with_type(&mut self, tc: u8) -> Result<DecodedValue<'a>> {
        match tc {
            // Small integers: 0-100
            0x00..=0x64 => Ok(DecodedValue::Int(i64::from(tc))),

            // Reserved
            0x65..=0x67 => Err(Error::InvalidTypeCode(tc)),

            // Long string
            type_code::STRING_LONG => self.decode_long_string_unchecked(),

            // BigNumber
            type_code::BIG_NUMBER => self.decode_big_number_unchecked(),

            // Float16 (bfloat16)
            type_code::FLOAT16 => self.decode_float16_unchecked(),

            // Float32
            type_code::FLOAT32 => self.decode_float32_unchecked(),

            // Float64
            type_code::FLOAT64 => self.decode_float64_unchecked(),

            // Null
            type_code::NULL => Ok(DecodedValue::Null),

            // False
            type_code::FALSE => Ok(DecodedValue::Bool(false)),

            // True
            type_code::TRUE => Ok(DecodedValue::Bool(true)),

            // Unsigned integers
            0x70..=0x77 => self.decode_unsigned_int_unchecked(tc),

            // Signed integers
            0x78..=0x7f => self.decode_signed_int_unchecked(tc),

            // Short strings
            0x80..=0x8f => self.decode_short_string_unchecked(tc),

            // Reserved
            0x90..=0x98 => Err(Error::InvalidTypeCode(tc)),

            // Array start
            type_code::ARRAY_START => Ok(DecodedValue::ArrayStart),

            // Object start
            type_code::OBJECT_START => Ok(DecodedValue::ObjectStart),

            // Container end
            type_code::CONTAINER_END => Ok(DecodedValue::ContainerEnd),

            // Small negative integers: -100 to -1
            0x9c..=0xff => Ok(DecodedValue::Int(i64::from(tc as i8))),
        }
    }

    #[inline]
    #[inline]
    fn decode_unsigned_int_unchecked(&mut self, tc: u8) -> Result<DecodedValue<'a>> {
        Ok(DecodedValue::UInt(self.read_unsigned_int(tc)?))
    }

    #[inline]
    fn decode_signed_int_unchecked(&mut self, tc: u8) -> Result<DecodedValue<'a>> {
        Ok(DecodedValue::Int(self.read_signed_int(tc)?))
    }

    #[inline]
    fn decode_float16_unchecked(&mut self) -> Result<DecodedValue<'a>> {
        Ok(DecodedValue::Float(self.read_float16()?))
    }

    #[inline]
    fn decode_float32_unchecked(&mut self) -> Result<DecodedValue<'a>> {
        Ok(DecodedValue::Float(self.read_float32()?))
    }

    #[inline]
    fn decode_float64_unchecked(&mut self) -> Result<DecodedValue<'a>> {
        Ok(DecodedValue::Float(self.read_float64()?))
    }

    #[inline]
    fn decode_short_string_unchecked(&mut self, tc: u8) -> Result<DecodedValue<'a>> {
        let len = type_code::short_string_len(tc);
        let bytes = self.read_bytes(len)?;
        // Fast path: short ASCII strings don't need UTF-8 validation
        if !self.config.allow_nul && is_short_ascii_no_nul(bytes) {
            // SAFETY: ASCII bytes are always valid UTF-8
            return Ok(DecodedValue::String(unsafe { std::str::from_utf8_unchecked(bytes) }));
        }
        let s = if self.config.allow_nul {
            std::str::from_utf8(bytes)?
        } else {
            validate_utf8_no_nul(bytes)?
        };
        Ok(DecodedValue::String(s))
    }

    #[inline]
    fn decode_long_string_unchecked(&mut self) -> Result<DecodedValue<'a>> {
        let pos_before_length = self.pos;
        let (length, continuation) = self.decode_length_field()?;
        if length > self.config.max_string_length as u64 {
            return Err(Error::MaxStringLengthExceeded);
        }
        let bytes = self.read_bytes(length as usize)?;
        if !continuation {
            // Fast path only for short strings (≤32 bytes)
            if !self.config.allow_nul && bytes.len() <= 32 && is_short_ascii_no_nul(bytes) {
                // SAFETY: ASCII bytes are always valid UTF-8
                return Ok(DecodedValue::String(unsafe { std::str::from_utf8_unchecked(bytes) }));
            }
            let s = if self.config.allow_nul {
                std::str::from_utf8(bytes)?
            } else {
                validate_utf8_no_nul(bytes)?
            };
            return Ok(DecodedValue::String(s));
        }
        // Multi-chunk string: delegate to the full checked implementation.
        // This is rare in practice - the encoder in this crate never produces chunked
        // strings, so this only occurs with external BONJSON sources using streaming.
        // Back up to the start of the length field so decode_long_string can re-read.
        self.pos = pos_before_length;
        self.decode_long_string()
    }

    fn decode_big_number_unchecked(&mut self) -> Result<DecodedValue<'a>> {
        // BigNumber decoding is complex enough that we don't duplicate it
        // Just call the regular version (state tracking is minimal overhead for big numbers)
        let header = self.read_byte()?;
        let sign = if header & 1 == 1 { -1i8 } else { 1i8 };
        let exp_len = ((header >> 1) & 3) as usize;
        let sig_len = (header >> 3) as usize;

        if sig_len == 0 {
            match exp_len {
                0 => return Ok(DecodedValue::BigNumber(BigNumber::new(sign, 0, 0))),
                1 => {
                    if !self.config.allow_nan_infinity {
                        return Err(Error::InvalidData("Infinity not allowed".into()));
                    }
                    return Ok(DecodedValue::Float(if sign < 0 { f64::NEG_INFINITY } else { f64::INFINITY }));
                }
                2 | 3 => {
                    if !self.config.allow_nan_infinity {
                        return Err(Error::InvalidData("NaN not allowed".into()));
                    }
                    return Ok(DecodedValue::Float(f64::NAN));
                }
                _ => unreachable!(),
            }
        }

        if sig_len > 8 {
            return Err(Error::ValueOutOfRange);
        }

        let exponent = if exp_len > 0 {
            let bytes = self.read_bytes(exp_len)?;
            let sign_bit = (bytes[exp_len - 1] >> 7) & 1;
            let fill: u8 = if sign_bit == 1 { 0xff } else { 0x00 };
            let mut buf = [fill; 4];
            buf[..exp_len].copy_from_slice(bytes);
            i32::from_le_bytes(buf)
        } else {
            0
        };

        let significand = if sig_len > 0 {
            let bytes = self.read_bytes(sig_len)?;
            let mut buf = [0u8; 8];
            buf[..sig_len].copy_from_slice(bytes);
            u64::from_le_bytes(buf)
        } else {
            0
        };

        Ok(DecodedValue::BigNumber(BigNumber::new(sign, significand, exponent)))
    }

    /// Decode the next value from the input.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The input is truncated
    /// - An invalid type code is encountered
    /// - String data contains invalid UTF-8
    /// - Configured limits (depth, container size, string length) are exceeded
    /// - A non-string value appears where an object key is expected
    #[inline]
    pub fn decode_value(&mut self) -> Result<DecodedValue<'a>> {
        let type_code = self.read_byte()?;

        // Handle object key expectation
        if self.expecting_object_key() {
            return self.decode_object_key(type_code);
        }

        self.decode_value_with_type(type_code)
    }

    /// Decode an object key (must be a string or container end).
    fn decode_object_key(&mut self, type_code: u8) -> Result<DecodedValue<'a>> {
        match type_code {
            type_code::CONTAINER_END => self.decode_container_end(),
            type_code::STRING_LONG => self.decode_long_string(),
            tc if type_code::is_short_string(tc) => self.decode_short_string(tc),
            _ => Err(Error::ExpectedObjectKey),
        }
    }

    /// Decode a value given its type code.
    fn decode_value_with_type(&mut self, tc: u8) -> Result<DecodedValue<'a>> {
        match tc {
            // Small integers: 0-100
            0x00..=0x64 => {
                self.toggle_object_state();
                self.increment_element_count()?;
                Ok(DecodedValue::Int(i64::from(tc)))
            }

            // Reserved
            0x65..=0x67 => Err(Error::InvalidTypeCode(tc)),

            // Long string
            type_code::STRING_LONG => self.decode_long_string(),

            // BigNumber
            type_code::BIG_NUMBER => self.decode_big_number(),

            // Float16 (bfloat16)
            type_code::FLOAT16 => self.decode_float16(),

            // Float32
            type_code::FLOAT32 => self.decode_float32(),

            // Float64
            type_code::FLOAT64 => self.decode_float64(),

            // Null
            type_code::NULL => {
                self.toggle_object_state();
                self.increment_element_count()?;
                Ok(DecodedValue::Null)
            }

            // False
            type_code::FALSE => {
                self.toggle_object_state();
                self.increment_element_count()?;
                Ok(DecodedValue::Bool(false))
            }

            // True
            type_code::TRUE => {
                self.toggle_object_state();
                self.increment_element_count()?;
                Ok(DecodedValue::Bool(true))
            }

            // Unsigned integers
            0x70..=0x77 => self.decode_unsigned_int(tc),

            // Signed integers
            0x78..=0x7f => self.decode_signed_int(tc),

            // Short strings
            0x80..=0x8f => self.decode_short_string(tc),

            // Reserved
            0x90..=0x98 => Err(Error::InvalidTypeCode(tc)),

            // Array start
            type_code::ARRAY_START => self.decode_array_start(),

            // Object start
            type_code::OBJECT_START => self.decode_object_start(),

            // Container end
            type_code::CONTAINER_END => self.decode_container_end(),

            // Small negative integers: -100 to -1
            0x9c..=0xff => {
                self.toggle_object_state();
                self.increment_element_count()?;
                Ok(DecodedValue::Int(i64::from(tc as i8)))
            }
        }
    }

    fn decode_unsigned_int(&mut self, tc: u8) -> Result<DecodedValue<'a>> {
        let value = self.read_unsigned_int(tc)?;
        self.toggle_object_state();
        self.increment_element_count()?;
        Ok(DecodedValue::UInt(value))
    }

    fn decode_signed_int(&mut self, tc: u8) -> Result<DecodedValue<'a>> {
        let value = self.read_signed_int(tc)?;
        self.toggle_object_state();
        self.increment_element_count()?;
        Ok(DecodedValue::Int(value))
    }

    fn decode_float16(&mut self) -> Result<DecodedValue<'a>> {
        let value = self.read_float16()?;
        self.toggle_object_state();
        self.increment_element_count()?;
        Ok(DecodedValue::Float(value))
    }

    fn decode_float32(&mut self) -> Result<DecodedValue<'a>> {
        let value = self.read_float32()?;
        self.toggle_object_state();
        self.increment_element_count()?;
        Ok(DecodedValue::Float(value))
    }

    fn decode_float64(&mut self) -> Result<DecodedValue<'a>> {
        let value = self.read_float64()?;
        self.toggle_object_state();
        self.increment_element_count()?;
        Ok(DecodedValue::Float(value))
    }

    fn decode_big_number(&mut self) -> Result<DecodedValue<'a>> {
        let header = self.read_byte()?;

        // Header: SSSSS EE N
        let sign = if header & 1 == 1 { -1i8 } else { 1i8 };
        let exp_len = ((header >> 1) & 3) as usize;
        let sig_len = (header >> 3) as usize;

        // Special encodings when significand length is 0
        if sig_len == 0 {
            match exp_len {
                0 => {
                    // Zero
                    self.toggle_object_state();
                    self.increment_element_count()?;
                    return Ok(DecodedValue::BigNumber(BigNumber::new(sign, 0, 0)));
                }
                1 => {
                    // Infinity
                    if !self.config.allow_nan_infinity {
                        return Err(Error::InvalidData("Infinity not allowed".into()));
                    }
                    // Return as f64 infinity (sign is -1 for negative, 1 for positive)
                    self.toggle_object_state();
                    self.increment_element_count()?;
                    return Ok(DecodedValue::Float(if sign < 0 {
                        f64::NEG_INFINITY
                    } else {
                        f64::INFINITY
                    }));
                }
                2 | 3 => {
                    // NaN (2 = quiet NaN, 3 = signaling NaN - we don't distinguish)
                    if !self.config.allow_nan_infinity {
                        return Err(Error::InvalidData("NaN not allowed".into()));
                    }
                    // Return as f64 NaN
                    self.toggle_object_state();
                    self.increment_element_count()?;
                    return Ok(DecodedValue::Float(f64::NAN));
                }
                _ => unreachable!(),
            }
        }

        // Check significand length limit (8 bytes max for u64)
        if sig_len > 8 {
            return Err(Error::ValueOutOfRange);
        }

        // Read exponent (signed, little-endian)
        let exponent = if exp_len > 0 {
            let bytes = self.read_bytes(exp_len)?;
            let sign_bit = (bytes[exp_len - 1] >> 7) & 1;
            let fill: u8 = if sign_bit == 1 { 0xff } else { 0x00 };
            let mut buf = [fill; 4];
            buf[..exp_len].copy_from_slice(bytes);
            i32::from_le_bytes(buf)
        } else {
            0
        };

        // Read significand (unsigned, little-endian)
        let significand = if sig_len > 0 {
            let bytes = self.read_bytes(sig_len)?;
            let mut buf = [0u8; 8];
            buf[..sig_len].copy_from_slice(bytes);
            u64::from_le_bytes(buf)
        } else {
            0
        };

        self.toggle_object_state();
        self.increment_element_count()?;
        Ok(DecodedValue::BigNumber(BigNumber::new(
            sign,
            significand,
            exponent,
        )))
    }

    fn decode_short_string(&mut self, tc: u8) -> Result<DecodedValue<'a>> {
        let len = type_code::short_string_len(tc);
        let bytes = self.read_bytes(len)?;

        // Fused UTF-8 validation and NUL check in one pass
        let s = if self.config.allow_nul {
            std::str::from_utf8(bytes)?
        } else {
            validate_utf8_no_nul(bytes)?
        };

        self.toggle_object_state();
        self.increment_element_count()?;
        Ok(DecodedValue::String(s))
    }

    fn decode_long_string(&mut self) -> Result<DecodedValue<'a>> {
        let (length, continuation) = self.decode_length_field()?;

        if length > self.config.max_string_length as u64 {
            return Err(Error::MaxStringLengthExceeded);
        }

        let bytes = self.read_bytes(length as usize)?;

        if !continuation {
            // Single-chunk string - fused UTF-8 validation and NUL check
            let s = if self.config.allow_nul {
                std::str::from_utf8(bytes)?
            } else {
                validate_utf8_no_nul(bytes)?
            };
            self.toggle_object_state();
            self.increment_element_count()?;
            return Ok(DecodedValue::String(s));
        }

        // Multi-chunk: validate first chunk
        self.validate_string_bytes(bytes)?;

        // Multi-chunk string - use scratch buffer for concatenation
        // Clear and reuse the scratch buffer (avoids repeated allocations)
        self.scratch.clear();
        self.scratch.extend_from_slice(bytes);

        let mut total_length = length as usize;
        let mut chunk_count = 1usize;

        loop {
            if chunk_count >= self.config.max_chunks {
                return Err(Error::TooManyChunks);
            }

            let (chunk_len, more) = self.decode_length_field()?;

            // Check for empty chunk with continuation bit
            if chunk_len == 0 && more {
                return Err(Error::EmptyChunkContinuation);
            }

            total_length = total_length
                .checked_add(chunk_len as usize)
                .ok_or(Error::MaxStringLengthExceeded)?;

            if total_length > self.config.max_string_length {
                return Err(Error::MaxStringLengthExceeded);
            }

            let chunk_bytes = self.read_bytes(chunk_len as usize)?;
            self.validate_string_bytes(chunk_bytes)?;
            self.scratch.extend_from_slice(chunk_bytes);
            chunk_count += 1;

            if !more {
                break;
            }
        }

        // Each chunk was already validated as UTF-8, so the concatenation is valid.
        // Use unsafe to skip redundant validation.
        // We need to copy out of the scratch buffer before mutating self.
        let s = unsafe { std::str::from_utf8_unchecked(&self.scratch) };
        let owned: Box<str> = s.to_owned().into_boxed_str();

        self.toggle_object_state();
        self.increment_element_count()?;

        // We need to return an owned string, but DecodedValue uses borrowed strings.
        // Leak the allocation so we can return a &'a str.
        //
        // Why this is acceptable:
        // 1. This code path only triggers for MULTI-CHUNK strings (continuation bit set)
        // 2. The encoder in this crate always produces SINGLE-CHUNK strings
        // 3. Multi-chunk strings only come from external BONJSON sources that use chunking
        //    for streaming large strings
        // 4. In a closed system using only this codec, this path is never executed
        // 5. For systems that do receive external chunked strings, the leak is a known
        //    trade-off to avoid a more complex lifetime design in DecodedValue
        Ok(DecodedValue::String(Box::leak(owned)))
    }

    fn validate_string_bytes(&self, bytes: &[u8]) -> Result<()> {
        if !self.config.allow_nul && bytes.contains(&0) {
            return Err(Error::NulCharacter);
        }
        // Each chunk must be valid UTF-8 on its own.
        // This catches multi-byte characters split across chunks.
        std::str::from_utf8(bytes)?;
        Ok(())
    }

    fn decode_array_start(&mut self) -> Result<DecodedValue<'a>> {
        // Check depth
        if self.containers.len() >= self.config.max_depth {
            return Err(Error::MaxDepthExceeded);
        }

        self.containers.push(ContainerState {
            is_object: false,
            expecting_key: false,
            element_count: 0,
        });

        // Don't toggle object state for container start, but do increment count
        if let Some(parent) = self.containers.get(self.containers.len().wrapping_sub(2)) {
            if !parent.is_object || !self.containers.last().unwrap().expecting_key {
                // Only count if we're in an array or as an object value
            }
        }

        Ok(DecodedValue::ArrayStart)
    }

    fn decode_object_start(&mut self) -> Result<DecodedValue<'a>> {
        // Check depth
        if self.containers.len() >= self.config.max_depth {
            return Err(Error::MaxDepthExceeded);
        }

        self.containers.push(ContainerState {
            is_object: true,
            expecting_key: true,
            element_count: 0,
        });

        Ok(DecodedValue::ObjectStart)
    }

    fn decode_container_end(&mut self) -> Result<DecodedValue<'a>> {
        let container = self.containers.pop().ok_or(Error::UnbalancedContainers)?;

        // Can't end object while expecting a value
        if container.is_object && !container.expecting_key {
            return Err(Error::ExpectedObjectValue);
        }

        self.toggle_object_state();
        // Increment parent's element count
        self.increment_element_count()?;

        Ok(DecodedValue::ContainerEnd)
    }

    /// Decode a length field payload.
    ///
    /// Returns (length, `continuation_bit`).
    ///
    /// Uses the `trailing_zeros` intrinsic for efficient decoding.
    fn decode_length_field(&mut self) -> Result<(u64, bool)> {
        let header = self.read_byte()?;

        // Special case: 0xff means 9-byte encoding
        if header == 0xff {
            let bytes = self.read_bytes(8)?;
            let mut buf = [0u8; 8];
            buf.copy_from_slice(bytes);
            let payload = u64::from_le_bytes(buf);
            let length = payload >> 1;
            let continuation = (payload & 1) != 0;
            return Ok((length, continuation));
        }

        // Count trailing 1s (which is trailing 0s of inverted header) + 1
        let inverted = !header;
        let count = (inverted.trailing_zeros() + 1) as usize;

        // Validate canonical encoding
        // Read count bytes (including the header we already read)
        let extra_bytes = count - 1;
        let mut buf = [0u8; 8];
        buf[0] = header;
        if extra_bytes > 0 {
            let bytes = self.read_bytes(extra_bytes)?;
            buf[1..=extra_bytes].copy_from_slice(bytes);
        }

        // Convert to u64 and shift right by count to remove the count field
        let raw = u64::from_le_bytes(buf);
        let payload = raw >> count;

        // Check for non-canonical encoding
        // The payload should require at least 'count' bytes to encode
        if count > 1 {
            let min_payload_for_count = 1u64 << (7 * (count - 1));
            if payload < min_payload_for_count {
                return Err(Error::NonCanonicalLength);
            }
        }

        let length = payload >> 1;
        let continuation = (payload & 1) != 0;

        Ok((length, continuation))
    }

    /// Finish decoding and check for errors.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - There are unclosed containers
    /// - There are trailing bytes (unless `allow_trailing_bytes` is configured)
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
    use crate::Encoder;

    #[test]
    fn test_decode_small_ints() {
        let mut dec = Decoder::new(&[0x00]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Int(0));

        let mut dec = Decoder::new(&[0x64]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Int(100));

        let mut dec = Decoder::new(&[0x9c]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Int(-100));

        let mut dec = Decoder::new(&[0xff]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Int(-1));
    }

    #[test]
    fn test_decode_larger_ints() {
        // sint16 1000
        let mut dec = Decoder::new(&[0x79, 0xe8, 0x03]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Int(1000));

        // uint8 180
        let mut dec = Decoder::new(&[0x70, 0xb4]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::UInt(180));
    }

    #[test]
    fn test_decode_null_bool() {
        let mut dec = Decoder::new(&[0x6d]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Null);

        let mut dec = Decoder::new(&[0x6f]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Bool(true));

        let mut dec = Decoder::new(&[0x6e]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Bool(false));
    }

    #[test]
    fn test_decode_short_string() {
        let mut dec = Decoder::new(&[0x80]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::String(""));

        let mut dec = Decoder::new(&[0x81, 0x41]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::String("A"));
    }

    #[test]
    fn test_decode_empty_containers() {
        let mut dec = Decoder::new(&[0x99, 0x9b]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::ArrayStart);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::ContainerEnd);
        dec.finish().unwrap();

        let mut dec = Decoder::new(&[0x9a, 0x9b]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::ObjectStart);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::ContainerEnd);
        dec.finish().unwrap();
    }

    #[test]
    fn test_decode_float16() {
        // 1.125 as bfloat16
        let mut dec = Decoder::new(&[0x6a, 0x90, 0x3f]);
        if let DecodedValue::Float(v) = dec.decode_value().unwrap() {
            assert!((v - 1.125).abs() < 0.001);
        } else {
            panic!("Expected float");
        }
    }

    #[test]
    fn test_decode_length_field() {
        // Length 0, no continuation
        let mut dec = Decoder::new(&[0x00]);
        assert_eq!(dec.decode_length_field().unwrap(), (0, false));

        // Length 0, with continuation
        let mut dec = Decoder::new(&[0x02]);
        assert_eq!(dec.decode_length_field().unwrap(), (0, true));

        // Length 63, no continuation
        let mut dec = Decoder::new(&[0xfc]);
        assert_eq!(dec.decode_length_field().unwrap(), (63, false));

        // Length 64, no continuation
        let mut dec = Decoder::new(&[0x01, 0x02]);
        assert_eq!(dec.decode_length_field().unwrap(), (64, false));
    }

    #[test]
    fn test_reserved_type_codes() {
        let mut dec = Decoder::new(&[0x65]);
        assert!(matches!(dec.decode_value(), Err(Error::InvalidTypeCode(0x65))));

        let mut dec = Decoder::new(&[0x90]);
        assert!(matches!(dec.decode_value(), Err(Error::InvalidTypeCode(0x90))));
    }

    #[test]
    fn test_truncated() {
        let mut dec = Decoder::new(&[0x79, 0xe8]); // Missing second byte of int16
        assert!(matches!(dec.decode_value(), Err(Error::Truncated)));
    }

    #[test]
    fn test_trailing_bytes() {
        let mut dec = Decoder::new(&[0x00, 0x00]); // Extra byte
        dec.decode_value().unwrap();
        assert!(matches!(dec.finish(), Err(Error::TrailingBytes)));
    }

    // =========================================================================
    // DecoderConfig option tests
    // =========================================================================

    #[test]
    fn test_allow_nul_false_rejects_nul() {
        // String with NUL byte: "a\0b"
        let data = [0x83, b'a', 0x00, b'b'];
        let mut dec = Decoder::new(&data);
        assert!(matches!(dec.decode_value(), Err(Error::NulCharacter)));
    }

    #[test]
    fn test_allow_nul_true_accepts_nul() {
        // String with NUL byte: "a\0b"
        let data = [0x83, b'a', 0x00, b'b'];
        let config = DecoderConfig {
            allow_nul: true,
            ..Default::default()
        };
        let mut dec = Decoder::with_config(&data, config);
        let result = dec.decode_value().unwrap();
        assert_eq!(result, DecodedValue::String("a\0b"));
    }

    #[test]
    fn test_allow_nul_in_long_string() {
        // Long string with NUL: length=4, "a\0bc"
        // Length field: payload = (4 << 1) | 0 = 8, shifted = 8 << 1 = 16 = 0x10
        let data = [0x68, 0x10, b'a', 0x00, b'b', b'c'];

        // Default config should reject
        let mut dec = Decoder::new(&data);
        assert!(matches!(dec.decode_value(), Err(Error::NulCharacter)));

        // allow_nul should accept
        let config = DecoderConfig {
            allow_nul: true,
            ..Default::default()
        };
        let mut dec = Decoder::with_config(&data, config);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::String("a\0bc"));
    }

    #[test]
    fn test_allow_trailing_bytes_false_rejects() {
        let data = [0x00, 0x00]; // int 0 + extra byte
        let mut dec = Decoder::new(&data);
        dec.decode_value().unwrap();
        assert!(matches!(dec.finish(), Err(Error::TrailingBytes)));
    }

    #[test]
    fn test_allow_trailing_bytes_true_accepts() {
        let data = [0x00, 0x00, 0x00]; // int 0 + extra bytes
        let config = DecoderConfig {
            allow_trailing_bytes: true,
            ..Default::default()
        };
        let mut dec = Decoder::with_config(&data, config);
        dec.decode_value().unwrap();
        dec.finish().unwrap(); // Should not error
    }

    #[test]
    fn test_max_depth_exceeded() {
        // Create deeply nested arrays: [[[[...]]]]
        let mut data = vec![0x99; 10]; // 10 array starts
        data.extend(vec![0x9b; 10]); // 10 container ends

        let config = DecoderConfig {
            max_depth: 5,
            ..Default::default()
        };
        let mut dec = Decoder::with_config(&data, config);

        // Should succeed for first 5 levels
        for _ in 0..5 {
            assert!(matches!(dec.decode_value().unwrap(), DecodedValue::ArrayStart));
        }
        // 6th level should fail
        assert!(matches!(dec.decode_value(), Err(Error::MaxDepthExceeded)));
    }

    #[test]
    fn test_max_depth_at_limit() {
        // Create nested arrays at exactly max_depth
        let mut data = vec![0x99; 3]; // 3 array starts
        data.extend(vec![0x9b; 3]); // 3 container ends

        let config = DecoderConfig {
            max_depth: 3,
            ..Default::default()
        };
        let mut dec = Decoder::with_config(&data, config);

        // Should succeed for exactly 3 levels
        for _ in 0..3 {
            assert!(matches!(dec.decode_value().unwrap(), DecodedValue::ArrayStart));
        }
        for _ in 0..3 {
            assert!(matches!(dec.decode_value().unwrap(), DecodedValue::ContainerEnd));
        }
        dec.finish().unwrap();
    }

    #[test]
    fn test_max_container_size_exceeded() {
        // Array with 5 elements
        let data = [0x99, 0x00, 0x01, 0x02, 0x03, 0x04, 0x9b];

        let config = DecoderConfig {
            max_container_size: 3,
            ..Default::default()
        };
        let mut dec = Decoder::with_config(&data, config);

        assert!(matches!(dec.decode_value().unwrap(), DecodedValue::ArrayStart));
        dec.decode_value().unwrap(); // 0
        dec.decode_value().unwrap(); // 1
        dec.decode_value().unwrap(); // 2
        // 4th element should fail
        assert!(matches!(dec.decode_value(), Err(Error::MaxContainerSizeExceeded)));
    }

    #[test]
    fn test_max_string_length_exceeded() {
        // Long string with length > limit
        // Type 0x68 (STRING_LONG), length=10
        // Length field: payload = (10 << 1) | 0 = 20, shifted = 20 << 1 = 40 = 0x28
        let data = [0x68, 0x28, b'a', b'b', b'c', b'd', b'e', b'f', b'g', b'h', b'i', b'j'];

        let config = DecoderConfig {
            max_string_length: 5,
            ..Default::default()
        };
        let mut dec = Decoder::with_config(&data, config);

        assert!(matches!(dec.decode_value(), Err(Error::MaxStringLengthExceeded)));
    }

    #[test]
    fn test_max_document_size_exceeded() {
        let data = [0x00; 100]; // 100 bytes

        let config = DecoderConfig {
            max_document_size: 50,
            ..Default::default()
        };
        let dec = Decoder::with_config(&data, config);

        assert!(matches!(dec.check_document_size(), Err(Error::MaxDocumentSizeExceeded)));
    }

    #[test]
    fn test_max_document_size_at_limit() {
        let data = [0x00; 50]; // exactly 50 bytes

        let config = DecoderConfig {
            max_document_size: 50,
            ..Default::default()
        };
        let dec = Decoder::with_config(&data, config);

        dec.check_document_size().unwrap(); // Should succeed
    }

    // =========================================================================
    // Direct decode method tests
    // =========================================================================

    #[test]
    fn test_decode_i64_direct() {
        // Small positive
        let mut dec = Decoder::new(&[0x2a]); // 42
        assert_eq!(dec.decode_i64_direct().unwrap(), 42);

        // Small negative
        let mut dec = Decoder::new(&[0xff]); // -1
        assert_eq!(dec.decode_i64_direct().unwrap(), -1);

        // Larger signed
        let mut dec = Decoder::new(&[0x79, 0x18, 0xfc]); // -1000 as sint16
        assert_eq!(dec.decode_i64_direct().unwrap(), -1000);
    }

    #[test]
    fn test_decode_u64_direct() {
        // Small positive
        let mut dec = Decoder::new(&[0x2a]); // 42
        assert_eq!(dec.decode_u64_direct().unwrap(), 42);

        // Larger unsigned
        let mut dec = Decoder::new(&[0x70, 0xc8]); // 200 as uint8
        assert_eq!(dec.decode_u64_direct().unwrap(), 200);
    }

    #[test]
    fn test_decode_bool_direct() {
        let mut dec = Decoder::new(&[0x6f]); // true
        assert!(dec.decode_bool_direct().unwrap());

        let mut dec = Decoder::new(&[0x6e]); // false
        assert!(!dec.decode_bool_direct().unwrap());
    }

    #[test]
    fn test_decode_str_direct() {
        // Short string
        let mut dec = Decoder::new(&[0x85, b'h', b'e', b'l', b'l', b'o']);
        assert_eq!(dec.decode_str_direct().unwrap(), "hello");

        // Empty string
        let mut dec = Decoder::new(&[0x80]);
        assert_eq!(dec.decode_str_direct().unwrap(), "");
    }

    #[test]
    fn test_decode_f64_direct() {
        // Small int as float
        let mut dec = Decoder::new(&[0x2a]); // 42
        assert_eq!(dec.decode_f64_direct().unwrap(), 42.0);

        // Float64
        let mut dec = Decoder::new(&[0x6c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0x3f]); // 1.0
        assert!((dec.decode_f64_direct().unwrap() - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_peek_type_code() {
        let dec = Decoder::new(&[0x2a, 0x6f]);
        assert_eq!(dec.peek_type_code().unwrap(), 0x2a);
        assert_eq!(dec.peek_type_code().unwrap(), 0x2a); // Doesn't consume
    }

    #[test]
    fn test_try_consume_container_end() {
        let mut dec = Decoder::new(&[0x9b, 0x00]); // container end, then int 0
        assert!(dec.try_consume_container_end().unwrap());
        assert_eq!(dec.peek_type_code().unwrap(), 0x00); // Now at int 0

        let mut dec = Decoder::new(&[0x00, 0x9b]); // int 0, then container end
        assert!(!dec.try_consume_container_end().unwrap());
        assert_eq!(dec.peek_type_code().unwrap(), 0x00); // Still at int 0
    }

    #[test]
    fn test_expect_array_start() {
        let mut dec = Decoder::new(&[0x99]); // array start
        dec.expect_array_start().unwrap();

        let mut dec = Decoder::new(&[0x9a]); // object start
        assert!(dec.expect_array_start().is_err());
    }

    #[test]
    fn test_expect_object_start() {
        let mut dec = Decoder::new(&[0x9a]); // object start
        dec.expect_object_start().unwrap();

        let mut dec = Decoder::new(&[0x99]); // array start
        assert!(dec.expect_object_start().is_err());
    }

    #[test]
    fn test_is_at_container_end() {
        let dec = Decoder::new(&[0x9b]); // container end
        assert!(dec.is_at_container_end().unwrap());

        let dec = Decoder::new(&[0x00]); // int 0
        assert!(!dec.is_at_container_end().unwrap());
    }

    // =========================================================================
    // Integer boundary tests
    // =========================================================================

    #[test]
    fn test_integer_boundaries() {
        // Boundary between small positive and uint8: 100 vs 101
        let mut dec = Decoder::new(&[0x64]); // 100 (small int)
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Int(100));

        let mut dec = Decoder::new(&[0x70, 0x65]); // 101 (uint8)
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::UInt(101));

        // Boundary between small negative and sint8: -100 vs -101
        let mut dec = Decoder::new(&[0x9c]); // -100 (small int)
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Int(-100));

        let mut dec = Decoder::new(&[0x78, 0x9b]); // -101 (sint8)
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Int(-101));
    }

    #[test]
    fn test_i64_extremes() {
        // i64::MAX = 9223372036854775807
        let mut data = vec![0x7f]; // sint64
        data.extend_from_slice(&i64::MAX.to_le_bytes());
        let mut dec = Decoder::new(&data);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Int(i64::MAX));

        // i64::MIN = -9223372036854775808
        let mut data = vec![0x7f]; // sint64
        data.extend_from_slice(&i64::MIN.to_le_bytes());
        let mut dec = Decoder::new(&data);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Int(i64::MIN));
    }

    #[test]
    fn test_u64_max() {
        // u64::MAX = 18446744073709551615
        let mut data = vec![0x77]; // uint64
        data.extend_from_slice(&u64::MAX.to_le_bytes());
        let mut dec = Decoder::new(&data);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::UInt(u64::MAX));
    }

    // =========================================================================
    // String boundary tests
    // =========================================================================

    #[test]
    fn test_string_length_boundaries() {
        // 15-byte string (max short string)
        let short_15 = "123456789012345";
        assert_eq!(short_15.len(), 15);
        let mut data = vec![0x8f]; // short string length 15
        data.extend_from_slice(short_15.as_bytes());
        let mut dec = Decoder::new(&data);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::String(short_15));

        // 16-byte string (requires long encoding)
        // Length field: payload = (16 << 1) | 0 = 32, shifted = 32 << 1 = 64 = 0x40
        let long_16 = "1234567890123456";
        assert_eq!(long_16.len(), 16);
        let mut data = vec![0x68, 0x40];
        data.extend_from_slice(long_16.as_bytes());
        let mut dec = Decoder::new(&data);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::String(long_16));
    }

    // =========================================================================
    // Error condition tests
    // =========================================================================

    #[test]
    fn test_error_truncated_int() {
        // sint16 but only 1 byte of data
        let data = [0x79, 0x01];
        let mut dec = Decoder::new(&data);
        assert!(matches!(dec.decode_value(), Err(Error::Truncated)));
    }

    #[test]
    fn test_error_truncated_string() {
        // Short string length 5 but only 3 bytes
        let data = [0x85, b'a', b'b', b'c'];
        let mut dec = Decoder::new(&data);
        assert!(matches!(dec.decode_value(), Err(Error::Truncated)));
    }

    #[test]
    fn test_error_truncated_float() {
        // float64 but only 4 bytes
        let data = [0x6c, 0x00, 0x00, 0x00, 0x00];
        let mut dec = Decoder::new(&data);
        assert!(matches!(dec.decode_value(), Err(Error::Truncated)));
    }

    #[test]
    fn test_error_invalid_utf8() {
        // Invalid UTF-8: 0xFF is never valid
        let data = [0x82, 0xff, 0xfe];
        let mut dec = Decoder::new(&data);
        assert!(matches!(dec.decode_value(), Err(Error::InvalidUtf8)));
    }

    #[test]
    fn test_error_invalid_utf8_continuation() {
        // Invalid: continuation byte (0x80-0xBF) at start
        let data = [0x81, 0x80];
        let mut dec = Decoder::new(&data);
        assert!(matches!(dec.decode_value(), Err(Error::InvalidUtf8)));
    }

    #[test]
    fn test_error_invalid_utf8_overlong() {
        // Overlong encoding of ASCII 'A' (0x41)
        // Should be [0x41] but encoded as [0xC1, 0x81]
        let data = [0x82, 0xc1, 0x81];
        let mut dec = Decoder::new(&data);
        assert!(matches!(dec.decode_value(), Err(Error::InvalidUtf8)));
    }

    #[test]
    fn test_error_unclosed_container() {
        // Array start without end
        let data = [0x99, 0x01, 0x02];
        let mut dec = Decoder::new(&data);
        dec.decode_value().unwrap(); // ArrayStart
        dec.decode_value().unwrap(); // 1
        dec.decode_value().unwrap(); // 2
        assert!(matches!(dec.finish(), Err(Error::UnclosedContainer)));
    }

    #[test]
    fn test_error_non_canonical_length() {
        // 2-byte length field encoding payload 0 (should be 1 byte: 0x00)
        // Format: [xxxxx01] [yyyyyyyy] where xxxxxyyyyyyyy is the payload
        // For payload 0: shifted = 0 << 2 | 1 = 1, so bytes = [0x01, 0x00]
        let data = [0x01, 0x00];
        let mut dec = Decoder::new(&data);
        assert!(matches!(dec.decode_length_field(), Err(Error::NonCanonicalLength)));
    }

    #[test]
    fn test_error_value_out_of_range() {
        // Try to decode a negative signed integer as u64
        let data = [0xff]; // -1 as small int
        let mut dec = Decoder::new(&data);
        assert!(matches!(dec.decode_u64_direct(), Err(Error::Custom(_))));
    }

    #[test]
    fn test_error_expected_type_mismatch() {
        // Try to decode an integer as a string
        let data = [0x2a]; // 42
        let mut dec = Decoder::new(&data);
        assert!(matches!(dec.decode_str_direct(), Err(Error::Custom(_))));
    }

    #[test]
    fn test_error_expected_array_got_object() {
        // Try to expect array but get object
        let data = [0x9a]; // object start
        let mut dec = Decoder::new(&data);
        assert!(dec.expect_array_start().is_err());
    }

    #[test]
    fn test_error_expected_object_got_array() {
        // Try to expect object but get array
        let data = [0x99]; // array start
        let mut dec = Decoder::new(&data);
        assert!(dec.expect_object_start().is_err());
    }

    #[test]
    fn test_error_all_reserved_type_codes() {
        // Test all reserved type codes
        let reserved_codes: Vec<u8> = (0x65..=0x67).chain(0x90..=0x98).collect();
        for code in reserved_codes {
            let data = [code];
            let mut dec = Decoder::new(&data);
            assert!(
                matches!(dec.decode_value(), Err(Error::InvalidTypeCode(c)) if c == code),
                "Expected InvalidTypeCode for 0x{:02x}", code
            );
        }
    }

    #[test]
    fn test_error_empty_input() {
        let data = [];
        let mut dec = Decoder::new(&data);
        assert!(matches!(dec.decode_value(), Err(Error::Truncated)));
    }

    #[test]
    fn test_error_peek_empty_input() {
        let data = [];
        let dec = Decoder::new(&data);
        assert!(matches!(dec.peek_type_code(), Err(Error::Truncated)));
    }

    #[test]
    fn test_error_container_end_mismatch() {
        // Try to consume container end when there isn't one
        let data = [0x01]; // int 1
        let mut dec = Decoder::new(&data);
        assert!(!dec.try_consume_container_end().unwrap());
    }

    // =========================================================================
    // Float edge case tests
    // =========================================================================

    #[test]
    fn test_float_negative_zero() {
        // Encode -0.0
        let mut buf = Vec::new();
        let mut enc = Encoder::new(&mut buf);
        enc.write_f64(-0.0).unwrap();

        // Decode and verify it's negative zero
        let mut dec = Decoder::new(&buf);
        if let DecodedValue::Float(v) = dec.decode_value().unwrap() {
            assert!(v.is_sign_negative());
            assert_eq!(v, 0.0);
        } else {
            panic!("Expected float");
        }
    }

    #[test]
    fn test_float_precision_boundaries() {
        // Integer floats (0-100) use small int encoding, not float
        let mut buf = Vec::new();
        let mut enc = Encoder::new(&mut buf);
        enc.write_f64(42.0).unwrap();
        assert_eq!(buf[0], 0x2a); // small int 42

        // Non-integer values that fit in bfloat16 use bfloat16 encoding
        buf.clear();
        let mut enc = Encoder::new(&mut buf);
        enc.write_f64(1.5).unwrap(); // 1.5 can be exactly represented in bfloat16
        // bfloat16 is type code 0x6a
        assert_eq!(buf[0], 0x6a);

        // Values NOT representable in bfloat16 use larger encoding
        buf.clear();
        let mut enc = Encoder::new(&mut buf);
        enc.write_f64(1.0000001).unwrap(); // Can't be exactly represented in bfloat16
        // Should use float32 (0x6b) or float64 (0x6c)
        assert!(buf[0] == 0x6b || buf[0] == 0x6c);
    }
}
