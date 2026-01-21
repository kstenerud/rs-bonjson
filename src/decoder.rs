// ABOUTME: High-performance BONJSON binary decoder.
// ABOUTME: Uses compiler intrinsics (trailing_zeros) for efficient length field decoding.

#![allow(clippy::missing_errors_doc)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]

use crate::error::{Error, Result};
use crate::types::{limits, type_code, BigNumber};

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
    /// Stack of container states for chunked iteration
    containers: Vec<ContainerState>,
    /// Scratch buffer for multi-chunk string concatenation (reused across calls)
    scratch: Vec<u8>,
}

/// Container state for tracking chunked containers.
#[derive(Clone, Copy)]
struct ContainerState {
    is_object: bool,
    /// Number of elements/pairs remaining in current chunk
    remaining: usize,
    /// Whether there are more chunks after the current one
    has_continuation: bool,
    /// Total elements/pairs seen (for limit checking)
    total_count: usize,
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
            scratch: Vec::new(),
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

    /// Check if at container end and consume it if so (for serde).
    /// With chunked containers, this checks if current chunk is exhausted
    /// and no continuation follows.
    #[inline]
    pub(crate) fn try_consume_container_end(&mut self) -> Result<bool> {
        if self.is_at_container_end_internal()? {
            self.containers.pop();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Expect and skip an array start marker, reading chunk header.
    #[inline]
    pub(crate) fn expect_array_start(&mut self) -> Result<()> {
        let tc = self.read_byte()?;
        if tc != type_code::ARRAY {
            return Err(Error::Custom(format!("expected array, got 0x{tc:02x}")));
        }
        // Consume element from parent container (this array is a value in parent)
        self.consume_element();
        self.begin_array_internal()
    }

    /// Expect and skip an object start marker, reading chunk header.
    #[inline]
    pub(crate) fn expect_object_start(&mut self) -> Result<()> {
        let tc = self.read_byte()?;
        if tc != type_code::OBJECT {
            return Err(Error::Custom(format!("expected object, got 0x{tc:02x}")));
        }
        // Consume element from parent container (this object is a value in parent)
        self.consume_element();
        self.begin_object_internal()
    }

    /// Decode an i64 directly.
    #[inline]
    #[allow(clippy::cast_possible_wrap)]
    pub(crate) fn decode_i64_direct(&mut self) -> Result<i64> {
        let tc = self.read_byte()?;

        // Small integers: 0x00-0xc8 (value = tc - 100)
        if type_code::is_small_int(tc) {
            self.consume_element();
            return Ok(i64::from(type_code::small_int_value(tc)));
        }

        // All integers (signed and unsigned): 0xd0-0xdf
        if type_code::is_any_int(tc) {
            let size = type_code::int_size(tc);
            let val = if type_code::int_is_signed(tc) {
                self.read_signed_int_sized(size)?
            } else {
                self.read_unsigned_int_sized(size)? as i64
            };
            self.consume_element();
            return Ok(val);
        }

        Err(Error::Custom(format!("expected integer, got 0x{tc:02x}")))
    }

    /// Decode a u64 directly.
    #[inline]
    pub(crate) fn decode_u64_direct(&mut self) -> Result<u64> {
        let tc = self.read_byte()?;

        // Small integers: 0x00-0xc8 (value = tc - 100)
        if type_code::is_small_int(tc) {
            let val = type_code::small_int_value(tc);
            self.consume_element();
            if val < 0 {
                return Err(Error::Custom("cannot decode negative int as u64".into()));
            }
            return Ok(val as u64);
        }

        // All integers (signed and unsigned): 0xd0-0xdf
        if type_code::is_any_int(tc) {
            let size = type_code::int_size(tc);
            let val = if type_code::int_is_signed(tc) {
                let signed_val = self.read_signed_int_sized(size)?;
                self.consume_element();
                if signed_val < 0 {
                    return Err(Error::ValueOutOfRange);
                }
                signed_val as u64
            } else {
                let unsigned_val = self.read_unsigned_int_sized(size)?;
                self.consume_element();
                unsigned_val
            };
            return Ok(val);
        }

        Err(Error::Custom(format!("expected unsigned integer, got 0x{tc:02x}")))
    }

    /// Decode a bool directly.
    #[inline]
    pub(crate) fn decode_bool_direct(&mut self) -> Result<bool> {
        let tc = self.read_byte()?;
        let result = match tc {
            type_code::TRUE => Ok(true),
            type_code::FALSE => Ok(false),
            _ => Err(Error::Custom(format!("expected bool, got 0x{tc:02x}"))),
        };
        if result.is_ok() {
            self.consume_element();
        }
        result
    }

    /// Decode a string directly.
    #[inline]
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn decode_str_direct(&mut self) -> Result<&'a str> {
        let tc = self.read_byte()?;

        // Short strings: 0xe0-0xef
        if type_code::is_short_string(tc) {
            let len = type_code::short_string_len(tc);
            let s = self.decode_string_content(len)?;
            self.consume_element();
            return Ok(s);
        }

        // Long string: 0xf0
        if tc == type_code::STRING_LONG {
            let s = self.decode_long_string_content()?;
            self.consume_element();
            return Ok(s);
        }

        Err(Error::ExpectedObjectKey)
    }

    /// Decode an f64 directly.
    #[inline]
    #[allow(clippy::cast_possible_wrap)]
    #[allow(clippy::cast_precision_loss)]
    pub(crate) fn decode_f64_direct(&mut self) -> Result<f64> {
        let tc = self.read_byte()?;

        // Small integers
        if type_code::is_small_int(tc) {
            self.consume_element();
            return Ok(f64::from(type_code::small_int_value(tc)));
        }

        // All integers (signed and unsigned): 0xd0-0xdf
        if type_code::is_any_int(tc) {
            let size = type_code::int_size(tc);
            let val = if type_code::int_is_signed(tc) {
                self.read_signed_int_sized(size)? as f64
            } else {
                self.read_unsigned_int_sized(size)? as f64
            };
            self.consume_element();
            return Ok(val);
        }

        let result = match tc {
            type_code::FLOAT16 => self.read_float16(),
            type_code::FLOAT32 => self.read_float32(),
            type_code::FLOAT64 => self.read_float64(),
            _ => Err(Error::Custom(format!("expected number, got 0x{tc:02x}"))),
        };

        if result.is_ok() {
            self.consume_element();
        }
        result
    }

    // =========================================================================
    // Internal methods
    // =========================================================================

    /// Check if at end of current container (chunk exhausted, no continuation).
    fn is_at_container_end_internal(&mut self) -> Result<bool> {
        // First, check if we need to read the next chunk (avoiding borrow issues)
        let (remaining, has_continuation) = {
            let Some(container) = self.containers.last() else {
                return Err(Error::UnbalancedContainers);
            };
            (container.remaining, container.has_continuation)
        };

        if remaining > 0 {
            return Ok(false);
        }

        // Current chunk exhausted, check for continuation
        if !has_continuation {
            return Ok(true);
        }

        // Read next chunk header (self is no longer borrowed)
        let (count, continuation) = self.decode_length_field()?;

        // Empty chunk with continuation is invalid (DoS vector)
        if count == 0 && continuation {
            return Err(Error::EmptyChunkContinuation);
        }

        // Now update container state
        let container = self.containers.last_mut().unwrap();
        // For objects, remaining tracks individual elements (key + value)
        container.remaining = if container.is_object {
            (count as usize) * 2
        } else {
            count as usize
        };
        container.has_continuation = continuation;
        container.total_count += count as usize;

        if container.total_count > self.config.max_container_size {
            return Err(Error::MaxContainerSizeExceeded);
        }

        // If new chunk is also empty with no continuation, we're done
        Ok(count == 0 && !continuation)
    }

    /// Consume one element from current container (decrement remaining count).
    /// Must be called after fully processing a value that was read from a container.
    pub(crate) fn consume_element(&mut self) {
        if let Some(container) = self.containers.last_mut() {
            if container.remaining > 0 {
                container.remaining -= 1;
            }
        }
    }

    /// Begin array (read chunk header).
    fn begin_array_internal(&mut self) -> Result<()> {
        if self.containers.len() >= self.config.max_depth {
            return Err(Error::MaxDepthExceeded);
        }

        let (count, continuation) = self.decode_length_field()?;

        if count == 0 && continuation {
            return Err(Error::EmptyChunkContinuation);
        }

        if count > self.config.max_container_size as u64 {
            return Err(Error::MaxContainerSizeExceeded);
        }

        self.containers.push(ContainerState {
            is_object: false,
            remaining: count as usize,
            has_continuation: continuation,
            total_count: count as usize,
        });

        Ok(())
    }

    /// Begin object (read chunk header).
    fn begin_object_internal(&mut self) -> Result<()> {
        if self.containers.len() >= self.config.max_depth {
            return Err(Error::MaxDepthExceeded);
        }

        let (count, continuation) = self.decode_length_field()?;

        if count == 0 && continuation {
            return Err(Error::EmptyChunkContinuation);
        }

        if count > self.config.max_container_size as u64 {
            return Err(Error::MaxContainerSizeExceeded);
        }

        // For objects, remaining tracks individual elements (key + value for each pair)
        // The chunk header contains pair count, so multiply by 2
        self.containers.push(ContainerState {
            is_object: true,
            remaining: (count as usize) * 2,
            has_continuation: continuation,
            total_count: count as usize, // Keep pair count for size limit checking
        });

        Ok(())
    }

    /// Decode a value given its type code.
    #[allow(clippy::cast_possible_wrap)]
    fn decode_value_with_type_code(&mut self, tc: u8) -> Result<DecodedValue<'a>> {
        // Small integers: 0x00-0xc8 (value = tc - 100)
        if type_code::is_small_int(tc) {
            self.consume_element();
            return Ok(DecodedValue::Int(i64::from(type_code::small_int_value(tc))));
        }

        // All integers (signed and unsigned): 0xd0-0xdf
        // Combined check is more efficient than separate unsigned/signed checks
        if type_code::is_any_int(tc) {
            let size = type_code::int_size(tc);
            return if type_code::int_is_signed(tc) {
                let val = self.read_signed_int_sized(size)?;
                self.consume_element();
                Ok(DecodedValue::Int(val))
            } else {
                let val = self.read_unsigned_int_sized(size)?;
                self.consume_element();
                Ok(DecodedValue::UInt(val))
            };
        }

        // Short strings: 0xe0-0xef
        if type_code::is_short_string(tc) {
            let len = type_code::short_string_len(tc);
            let s = self.decode_string_content(len)?;
            self.consume_element();
            return Ok(DecodedValue::String(s));
        }

        // High codes 0xf0-0xf9 and reserved ranges
        match tc {
            type_code::STRING_LONG => {
                let s = self.decode_long_string_content()?;
                self.consume_element();
                Ok(DecodedValue::String(s))
            }
            type_code::BIG_NUMBER => {
                let bn = self.decode_big_number()?;
                self.consume_element();
                Ok(bn)
            }
            type_code::FLOAT16 => {
                let f = self.read_float16()?;
                self.consume_element();
                Ok(DecodedValue::Float(f))
            }
            type_code::FLOAT32 => {
                let f = self.read_float32()?;
                self.consume_element();
                Ok(DecodedValue::Float(f))
            }
            type_code::FLOAT64 => {
                let f = self.read_float64()?;
                self.consume_element();
                Ok(DecodedValue::Float(f))
            }
            type_code::NULL => {
                self.consume_element();
                Ok(DecodedValue::Null)
            }
            type_code::FALSE => {
                self.consume_element();
                Ok(DecodedValue::Bool(false))
            }
            type_code::TRUE => {
                self.consume_element();
                Ok(DecodedValue::Bool(true))
            }
            type_code::ARRAY => {
                // Consume an element from parent container (this array is a value in the parent)
                self.consume_element();
                self.begin_array_internal()?;
                Ok(DecodedValue::ArrayStart)
            }
            type_code::OBJECT => {
                // Consume an element from parent container (this object is a value in the parent)
                self.consume_element();
                self.begin_object_internal()?;
                Ok(DecodedValue::ObjectStart)
            }
            _ => Err(Error::InvalidTypeCode(tc)),
        }
    }

    /// Read an unsigned integer of given byte size.
    #[inline]
    fn read_unsigned_int_sized(&mut self, size: usize) -> Result<u64> {
        let bytes = self.read_bytes(size)?;
        let mut buf = [0u8; 8];
        buf[..size].copy_from_slice(bytes);
        Ok(u64::from_le_bytes(buf))
    }

    /// Read a signed integer of given byte size.
    #[inline]
    fn read_signed_int_sized(&mut self, size: usize) -> Result<i64> {
        let bytes = self.read_bytes(size)?;
        let sign_bit = (bytes[size - 1] >> 7) & 1;
        let fill: u8 = if sign_bit == 1 { 0xff } else { 0x00 };
        let mut buf = [fill; 8];
        buf[..size].copy_from_slice(bytes);
        Ok(i64::from_le_bytes(buf))
    }

    /// Read a bfloat16 value.
    #[inline]
    fn read_float16(&mut self) -> Result<f64> {
        let bytes = self.read_bytes(2)?;
        let bits = u16::from_le_bytes([bytes[0], bytes[1]]);
        let f32_bits = u32::from(bits) << 16;
        let value = f64::from(f32::from_bits(f32_bits));
        self.check_float(value)?;
        Ok(value)
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

    /// Decode string content (after type code).
    fn decode_string_content(&mut self, len: usize) -> Result<&'a str> {
        if len > self.config.max_string_length {
            return Err(Error::MaxStringLengthExceeded);
        }

        let bytes = self.read_bytes(len)?;

        // Validate UTF-8
        let s = validate_utf8(bytes)?;

        // Check for NUL characters
        if !self.config.allow_nul && bytes.contains(&0) {
            return Err(Error::NulCharacter);
        }

        Ok(s)
    }

    /// Decode long string content (potentially multi-chunk).
    #[allow(clippy::cast_possible_truncation)]
    fn decode_long_string_content(&mut self) -> Result<&'a str> {
        let (first_len, first_continuation) = self.decode_length_field()?;

        if first_len > self.config.max_string_length as u64 {
            return Err(Error::MaxStringLengthExceeded);
        }

        let first_bytes = self.read_bytes(first_len as usize)?;

        // Single-chunk case (most common): validate and return directly
        if !first_continuation {
            let s = validate_utf8(first_bytes)?;
            if !self.config.allow_nul && first_bytes.contains(&0) {
                return Err(Error::NulCharacter);
            }
            return Ok(s);
        }

        // Multi-chunk case: concatenate all chunks, then validate UTF-8
        self.scratch.clear();
        self.scratch.extend_from_slice(first_bytes);

        let mut has_continuation = first_continuation;
        let mut chunk_count = 1usize;

        while has_continuation {
            if chunk_count >= self.config.max_chunks {
                return Err(Error::TooManyChunks);
            }

            let (chunk_len, continuation) = self.decode_length_field()?;

            // Empty chunk with continuation is invalid
            if chunk_len == 0 && continuation {
                return Err(Error::EmptyChunkContinuation);
            }

            if self.scratch.len() + chunk_len as usize > self.config.max_string_length {
                return Err(Error::MaxStringLengthExceeded);
            }

            let chunk_bytes = self.read_bytes(chunk_len as usize)?;
            self.scratch.extend_from_slice(chunk_bytes);
            has_continuation = continuation;
            chunk_count += 1;
        }

        // Validate UTF-8 on the complete assembled string
        let s = validate_utf8(&self.scratch)?;

        // Check for NUL characters
        if !self.config.allow_nul && self.scratch.contains(&0) {
            return Err(Error::NulCharacter);
        }

        // We need to return &'a str but have an owned Vec.
        // Leak the allocation - this only happens for multi-chunk strings.
        let owned: Box<str> = s.to_owned().into_boxed_str();
        Ok(Box::leak(owned))
    }

    /// Decode a `BigNumber`.
    fn decode_big_number(&mut self) -> Result<DecodedValue<'a>> {
        let header = self.read_byte()?;

        let sign = if header & 1 == 1 { -1i8 } else { 1i8 };
        let exp_len = ((header >> 1) & 3) as usize;
        let sig_len = (header >> 3) as usize;

        // Special encodings when significand length is 0
        if sig_len == 0 {
            match exp_len {
                0 => return Ok(DecodedValue::BigNumber(BigNumber::new(sign, 0, 0))),
                1 => {
                    if !self.config.allow_nan_infinity {
                        return Err(Error::InvalidData("Infinity not allowed".into()));
                    }
                    return Ok(DecodedValue::Float(if sign < 0 {
                        f64::NEG_INFINITY
                    } else {
                        f64::INFINITY
                    }));
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

    /// Decode a length field.
    /// Returns (length, `continuation_bit`).
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

        // Read additional bytes
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

        let length = payload >> 1;
        let continuation = (payload & 1) != 0;

        Ok((length, continuation))
    }

    /// Decode the next value from the input.
    pub fn decode_value(&mut self) -> Result<DecodedValue<'a>> {
        let tc = self.read_byte()?;
        self.decode_value_with_type_code(tc)
    }

    /// Check if we're at the end of the current container.
    pub fn is_at_container_end(&mut self) -> Result<bool> {
        self.is_at_container_end_internal()
    }

    /// End the current container (pop from stack).
    pub fn end_container(&mut self) -> Result<()> {
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
        // 0x64 = 100, value = 100 - 100 = 0
        let mut dec = Decoder::new(&[0x64]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Int(0));

        // 0xc8 = 200, value = 200 - 100 = 100
        let mut dec = Decoder::new(&[0xc8]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Int(100));

        // 0x00 = 0, value = 0 - 100 = -100
        let mut dec = Decoder::new(&[0x00]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Int(-100));

        // 0x63 = 99, value = 99 - 100 = -1
        let mut dec = Decoder::new(&[0x63]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Int(-1));
    }

    #[test]
    fn test_decode_larger_ints() {
        // sint16: 0xd9 followed by little-endian value
        let mut dec = Decoder::new(&[0xd9, 0xe8, 0x03]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Int(1000));

        // uint8: 0xd0 followed by value
        let mut dec = Decoder::new(&[0xd0, 0xb4]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::UInt(180));
    }

    #[test]
    fn test_decode_null_bool() {
        let mut dec = Decoder::new(&[0xf5]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Null);

        let mut dec = Decoder::new(&[0xf7]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Bool(true));

        let mut dec = Decoder::new(&[0xf6]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Bool(false));
    }

    #[test]
    fn test_decode_short_string() {
        // Empty string: 0xe0
        let mut dec = Decoder::new(&[0xe0]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::String(""));

        // "x": 0xe1 + 'x'
        let mut dec = Decoder::new(&[0xe1, 0x78]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::String("x"));
    }

    #[test]
    fn test_decode_empty_array() {
        // 0xf8 + chunk(count=0, cont=0) = 0xf8 0x00
        let mut dec = Decoder::new(&[0xf8, 0x00]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::ArrayStart);
        assert!(dec.is_at_container_end().unwrap());
        dec.end_container().unwrap();
        dec.finish().unwrap();
    }

    #[test]
    fn test_decode_empty_object() {
        // 0xf9 + chunk(count=0, cont=0) = 0xf9 0x00
        let mut dec = Decoder::new(&[0xf9, 0x00]);
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::ObjectStart);
        assert!(dec.is_at_container_end().unwrap());
        dec.end_container().unwrap();
        dec.finish().unwrap();
    }

    #[test]
    fn test_decode_array_with_values() {
        // [1, "x", null]
        // 0xf8 + chunk(count=3, cont=0) = 0xf8 0x0c + elements
        let data = [0xf8, 0x0c, 0x65, 0xe1, 0x78, 0xf5];
        let mut dec = Decoder::new(&data);

        assert_eq!(dec.decode_value().unwrap(), DecodedValue::ArrayStart);
        assert!(!dec.is_at_container_end().unwrap());

        // decode_value() now automatically consumes elements
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
        // {"a": 1}
        // 0xf9 + chunk(count=1, cont=0) = 0xf9 0x04 + key + value
        // For objects, pair_count=1 means remaining=2 (key + value)
        let data = [0xf9, 0x04, 0xe1, 0x61, 0x65];
        let mut dec = Decoder::new(&data);

        assert_eq!(dec.decode_value().unwrap(), DecodedValue::ObjectStart);
        assert!(!dec.is_at_container_end().unwrap());

        // decode_value() automatically consumes elements
        // Key
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::String("a"));
        assert!(!dec.is_at_container_end().unwrap());
        // Value
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Int(1));
        assert!(dec.is_at_container_end().unwrap());

        dec.end_container().unwrap();
        dec.finish().unwrap();
    }

    #[test]
    fn test_decode_multichunk_array() {
        // Multi-chunk array: chunk1 has 2 elements (cont=1), chunk2 has 1 element (cont=0)
        // Chunk1: count=2, cont=1 → payload=5 → encoded=0x0a
        // Chunk2: count=1, cont=0 → payload=2 → encoded=0x04
        let data = [0xf8, 0x0a, 0x65, 0x66, 0x04, 0x67];
        let mut dec = Decoder::new(&data);

        assert_eq!(dec.decode_value().unwrap(), DecodedValue::ArrayStart);

        // decode_value() automatically consumes elements
        // Element 1
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Int(1));

        // Element 2 - after this, chunk1 is exhausted but has continuation
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Int(2));

        // is_at_container_end reads chunk2 header
        assert!(!dec.is_at_container_end().unwrap());

        // Element 3
        assert_eq!(dec.decode_value().unwrap(), DecodedValue::Int(3));

        assert!(dec.is_at_container_end().unwrap());
        dec.end_container().unwrap();
    }

    #[test]
    fn test_reserved_type_codes() {
        let mut dec = Decoder::new(&[0xc9]);
        assert!(matches!(dec.decode_value(), Err(Error::InvalidTypeCode(0xc9))));

        let mut dec = Decoder::new(&[0xfa]);
        assert!(matches!(dec.decode_value(), Err(Error::InvalidTypeCode(0xfa))));
    }

    #[test]
    fn test_empty_chunk_continuation_rejected() {
        // Array with empty chunk but continuation bit set
        // count=0, cont=1 → payload=1 → encoded=0x02
        let data = [0xf8, 0x02, 0x00];
        let mut dec = Decoder::new(&data);

        match dec.decode_value() {
            Err(Error::EmptyChunkContinuation) => {}
            other => panic!("expected EmptyChunkContinuation, got {:?}", other),
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
    fn test_truncated() {
        let mut dec = Decoder::new(&[0xd9, 0xe8]); // Missing second byte of int16
        assert!(matches!(dec.decode_value(), Err(Error::Truncated)));
    }

    #[test]
    fn test_trailing_bytes() {
        let mut dec = Decoder::new(&[0x64, 0x64]); // int 0 + extra byte
        dec.decode_value().unwrap();
        assert!(matches!(dec.finish(), Err(Error::TrailingBytes)));
    }

    #[test]
    fn test_decode_float16() {
        // 1.125 as bfloat16 = 0x3f90
        let mut dec = Decoder::new(&[0xf2, 0x90, 0x3f]);
        if let DecodedValue::Float(v) = dec.decode_value().unwrap() {
            assert!((v - 1.125).abs() < 0.001);
        } else {
            panic!("Expected float");
        }
    }

    #[test]
    fn test_non_canonical_length_accepted() {
        // 2-byte length field encoding payload 0 (canonically 1 byte: 0x00)
        // Per updated spec, decoders MUST accept non-canonical lengths
        let data = [0x01, 0x00];
        let mut dec = Decoder::new(&data);
        assert_eq!(dec.decode_length_field().unwrap(), (0, false));

        // 2-byte length field encoding payload 2 (length=1, cont=0)
        // Canonically would be 0x04
        let data = [0x09, 0x00];
        let mut dec = Decoder::new(&data);
        assert_eq!(dec.decode_length_field().unwrap(), (1, false));

        // 3-byte length field encoding payload 0
        let data = [0x03, 0x00, 0x00];
        let mut dec = Decoder::new(&data);
        assert_eq!(dec.decode_length_field().unwrap(), (0, false));
    }
}
