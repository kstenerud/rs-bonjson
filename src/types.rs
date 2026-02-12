// ABOUTME: Defines BONJSON type codes and the BigNumber type.
// ABOUTME: Type codes map directly to the BONJSON specification byte values.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]

/// Type codes for BONJSON values.
/// These match the BONJSON specification exactly.
pub mod type_code {
    // Small integers: 0x00-0x64 (values 0 to 100, value = type_code)
    pub const SMALLINT_MIN: u8 = 0x00; // 0
    pub const SMALLINT_MAX: u8 = 0x64; // 100

    // Short strings (0-66 bytes): 0x65-0xa7
    pub const STRING0: u8 = 0x65;
    pub const STRING_MAX: u8 = 0xa7;

    // Unsigned integers (CPU-native sizes): 0xa8-0xab
    pub const UINT8: u8 = 0xa8;
    pub const UINT16: u8 = 0xa9;
    pub const UINT32: u8 = 0xaa;
    pub const UINT64: u8 = 0xab;

    // Signed integers (CPU-native sizes): 0xac-0xaf
    pub const SINT8: u8 = 0xac;
    pub const SINT16: u8 = 0xad;
    pub const SINT32: u8 = 0xae;
    pub const SINT64: u8 = 0xaf;

    // Floats (little-endian IEEE 754)
    pub const FLOAT32: u8 = 0xb0;
    pub const FLOAT64: u8 = 0xb1;

    // Big number: zigzag LEB128 exponent + zigzag LEB128 signed_length + LE magnitude bytes
    pub const BIG_NUMBER: u8 = 0xb2;

    // Null and booleans
    pub const NULL: u8 = 0xb3;
    pub const FALSE: u8 = 0xb4;
    pub const TRUE: u8 = 0xb5;

    // Container end marker
    pub const CONTAINER_END: u8 = 0xb6;

    // Containers (delimiter-terminated)
    pub const ARRAY: u8 = 0xb7;
    pub const OBJECT: u8 = 0xb8;

    // Record definition and instance
    pub const RECORD_DEF: u8 = 0xb9;
    pub const RECORD_INSTANCE: u8 = 0xba;

    // Reserved: 0xbb-0xf4

    // Typed arrays: 0xf5-0xfe (length-prefixed, no end marker)
    pub const TYPED_ARRAY_FLOAT64: u8 = 0xf5;
    pub const TYPED_ARRAY_FLOAT32: u8 = 0xf6;
    pub const TYPED_ARRAY_SINT64: u8 = 0xf7;
    pub const TYPED_ARRAY_SINT32: u8 = 0xf8;
    pub const TYPED_ARRAY_SINT16: u8 = 0xf9;
    pub const TYPED_ARRAY_SINT8: u8 = 0xfa;
    pub const TYPED_ARRAY_UINT64: u8 = 0xfb;
    pub const TYPED_ARRAY_UINT32: u8 = 0xfc;
    pub const TYPED_ARRAY_UINT16: u8 = 0xfd;
    pub const TYPED_ARRAY_UINT8: u8 = 0xfe;

    // Long string delimiter (starts and terminates long strings)
    pub const STRING_LONG: u8 = 0xff;

    /// Check if a type code is a small integer (0 to 100)
    #[inline]
    #[must_use]
    pub const fn is_small_int(code: u8) -> bool {
        code <= SMALLINT_MAX
    }

    /// Decode a small integer type code to its value (same as type_code)
    #[inline]
    #[must_use]
    pub const fn small_int_value(code: u8) -> u8 {
        code
    }

    /// Encode a small integer value (0 to 100) to its type code
    #[inline]
    #[must_use]
    pub const fn small_int_code(value: u8) -> u8 {
        value
    }

    /// Check if a type code is a short string (0-66 bytes)
    #[inline]
    #[must_use]
    pub const fn is_short_string(code: u8) -> bool {
        code >= STRING0 && code <= STRING_MAX
    }

    /// Get the length of a short string from its type code
    #[inline]
    #[must_use]
    pub const fn short_string_len(code: u8) -> usize {
        (code - STRING0) as usize
    }

    /// Check if a type code is any string type (short or long)
    #[inline]
    #[must_use]
    pub const fn is_any_string(code: u8) -> bool {
        is_short_string(code) || code == STRING_LONG
    }

    /// Check if a type code is an unsigned integer (0xa8-0xab)
    #[inline]
    #[must_use]
    pub const fn is_unsigned_int(code: u8) -> bool {
        code >= UINT8 && code <= UINT64
    }

    /// Get the byte count for an unsigned integer type code.
    /// Returns 1, 2, 4, or 8.
    #[inline]
    #[must_use]
    pub const fn unsigned_int_size(code: u8) -> usize {
        1 << (code - UINT8) as usize
    }

    /// Check if a type code is a signed integer (0xac-0xaf)
    #[inline]
    #[must_use]
    pub const fn is_signed_int(code: u8) -> bool {
        code >= SINT8 && code <= SINT64
    }

    /// Get the byte count for a signed integer type code.
    /// Returns 1, 2, 4, or 8.
    #[inline]
    #[must_use]
    pub const fn signed_int_size(code: u8) -> usize {
        1 << (code - SINT8) as usize
    }

    /// Check if a type code is any integer (signed or unsigned): 0xa8-0xaf
    #[inline]
    #[must_use]
    pub const fn is_any_int(code: u8) -> bool {
        code >= UINT8 && code <= SINT64
    }

    /// Check if an integer type code is signed (0xac-0xaf).
    /// Only valid when `is_any_int()` returns true.
    #[inline]
    #[must_use]
    pub const fn int_is_signed(code: u8) -> bool {
        code >= SINT8
    }

    /// Get the byte count for any integer type code (works for both signed and unsigned).
    /// Returns 1, 2, 4, or 8.
    /// Only valid when `is_any_int()` returns true.
    #[inline]
    #[must_use]
    pub const fn int_size(code: u8) -> usize {
        // Mask off the sign bit (bit 2) to get index 0-3, then 1 << index
        1 << ((code & 0x03) as usize)
    }

    /// Check if a type code is a typed array (0xf5-0xfe)
    #[inline]
    #[must_use]
    pub const fn is_typed_array(code: u8) -> bool {
        code >= TYPED_ARRAY_FLOAT64 && code <= TYPED_ARRAY_UINT8
    }

    /// Get the element byte size for a typed array type code.
    /// Returns 1, 2, 4, or 8.
    /// Only valid when `is_typed_array()` returns true.
    #[inline]
    #[must_use]
    pub const fn typed_array_element_size(code: u8) -> usize {
        // Lookup indexed by (0xfe - code): [0]=u8:1, [1]=u16:2, [2]=u32:4, [3]=u64:8, [4]=s8:1, [5]=s16:2, [6]=s32:4, [7]=s64:8, [8]=f32:4, [9]=f64:8
        const SIZES: [usize; 10] = [1, 2, 4, 8, 1, 2, 4, 8, 4, 8];
        SIZES[(TYPED_ARRAY_UINT8 - code) as usize]
    }

    /// Check if a typed array element type is signed integer.
    /// Only valid when `is_typed_array()` returns true.
    #[inline]
    #[must_use]
    pub const fn typed_array_is_signed_int(code: u8) -> bool {
        code >= TYPED_ARRAY_SINT64 && code <= TYPED_ARRAY_SINT8
    }

    /// Check if a typed array element type is unsigned integer.
    /// Only valid when `is_typed_array()` returns true.
    #[inline]
    #[must_use]
    pub const fn typed_array_is_unsigned_int(code: u8) -> bool {
        code >= TYPED_ARRAY_UINT64 && code <= TYPED_ARRAY_UINT8
    }

    /// Check if a typed array element type is float.
    /// Only valid when `is_typed_array()` returns true.
    #[inline]
    #[must_use]
    pub const fn typed_array_is_float(code: u8) -> bool {
        code == TYPED_ARRAY_FLOAT32 || code == TYPED_ARRAY_FLOAT64
    }

    /// Check if a type code is reserved
    #[inline]
    #[must_use]
    pub const fn is_reserved(code: u8) -> bool {
        code >= 0xbb && code <= 0xf4
    }
}

/// Encode a signed i64 as zigzag encoding: 0→0, -1→1, 1→2, -2→3, ...
#[inline]
#[must_use]
pub const fn zigzag_encode(v: i64) -> u64 {
    ((v << 1) ^ (v >> 63)) as u64
}

/// Decode a zigzag-encoded u64 back to i64: 0→0, 1→-1, 2→1, 3→-2, ...
#[inline]
#[must_use]
pub const fn zigzag_decode(v: u64) -> i64 {
    ((v >> 1) as i64) ^ (-((v & 1) as i64))
}

/// Encode a u64 as LEB128 into the provided buffer.
/// Returns the number of bytes written (1-10).
#[inline]
pub fn leb128_encode(mut value: u64, buf: &mut [u8; 10]) -> usize {
    let mut i = 0;
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            buf[i] = byte;
            return i + 1;
        }
        buf[i] = byte | 0x80;
        i += 1;
    }
}

/// Decode a LEB128-encoded value from a byte slice.
/// Returns (value, bytes_consumed) or None if truncated or overflows u64.
#[inline]
pub fn leb128_decode(data: &[u8]) -> Option<(u64, usize)> {
    let mut value: u64 = 0;
    let mut shift: u32 = 0;
    for (i, &byte) in data.iter().enumerate() {
        if shift >= 64 {
            return None; // Overflow
        }
        value |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return Some((value, i + 1));
        }
        shift += 7;
    }
    None // Truncated
}

/// Lookup table mapping minimum byte count to CPU-native size index (0-3).
/// Index 1→0, 2→1, 3→2, 4→2, 5→3, 6→3, 7→3, 8→3
/// Used as: type_code = UINT8 + NATIVE_SIZE_INDEX[byte_count - 1]
pub const NATIVE_SIZE_INDEX: [u8; 8] = [0, 1, 2, 2, 3, 3, 3, 3];

/// Lookup table mapping minimum byte count to actual CPU-native byte count.
/// Index 1→1, 2→2, 3→4, 4→4, 5→8, 6→8, 7→8, 8→8
pub const NATIVE_SIZE_BYTES: [usize; 8] = [1, 2, 4, 4, 8, 8, 8, 8];

/// A big number with arbitrary precision base-10 representation.
///
/// The value is: sign(signed_length) × magnitude × 10^exponent
///
/// Encoded as zigzag LEB128 exponent, zigzag LEB128 signed_length, then raw
/// little-endian magnitude bytes. Negative zero is NOT representable (use
/// IEEE754 float -0.0 instead).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BigNumber {
    /// The absolute value of the significand (0 to 2^64-1)
    pub significand: u64,
    /// The base-10 exponent
    pub exponent: i64,
    /// The sign: 1 for positive, -1 for negative
    pub sign: i8,
}

impl BigNumber {
    /// Create a new `BigNumber`.
    ///
    /// # Arguments
    /// * `sign` - The sign: 1 for positive, -1 for negative
    /// * `significand` - The absolute value of the significand
    /// * `exponent` - The base-10 exponent
    #[inline]
    #[must_use] pub const fn new(sign: i8, significand: u64, exponent: i64) -> Self {
        Self {
            significand,
            exponent,
            sign,
        }
    }

    /// Create a `BigNumber` representing zero.
    #[inline]
    #[must_use] pub const fn zero() -> Self {
        Self::new(1, 0, 0)
    }

    /// Check if this `BigNumber` is zero.
    #[inline]
    #[must_use] pub const fn is_zero(&self) -> bool {
        self.significand == 0
    }

    /// Check if this `BigNumber` is negative.
    #[inline]
    #[must_use] pub const fn is_negative(&self) -> bool {
        self.sign < 0
    }

    /// Get the signed significand as an i128 (to handle the full u64 range with sign).
    #[must_use]
    pub fn signed_significand(&self) -> i128 {
        let sig = self.significand as i128;
        if self.sign < 0 { -sig } else { sig }
    }

    /// Try to convert this `BigNumber` to an i64.
    /// Returns None if the value cannot be represented exactly.
    #[must_use]
    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_possible_wrap)]
    pub fn to_i64(&self) -> Option<i64> {
        if self.exponent < 0 {
            return None;
        }
        if self.exponent > 18 {
            return None;
        }

        let multiplier = 10i64.checked_pow(self.exponent as u32)?;
        let abs_value = (self.significand as i64).checked_mul(multiplier)?;

        if self.sign < 0 {
            abs_value.checked_neg()
        } else {
            Some(abs_value)
        }
    }

    /// Try to convert this `BigNumber` to a u64.
    /// Returns None if the value cannot be represented exactly.
    #[must_use]
    #[allow(clippy::cast_sign_loss)]
    pub fn to_u64(&self) -> Option<u64> {
        if self.sign < 0 && self.significand != 0 {
            return None;
        }
        if self.exponent < 0 {
            return None;
        }
        if self.exponent > 19 {
            return None;
        }

        let multiplier = 10u64.checked_pow(self.exponent as u32)?;
        self.significand.checked_mul(multiplier)
    }

    /// Try to convert this `BigNumber` to an f64.
    /// This may lose precision for very large or very precise numbers.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn to_f64(&self) -> f64 {
        let sign = if self.sign < 0 { -1.0 } else { 1.0 };
        let significand = self.significand as f64;
        let exponent = 10.0f64.powi(self.exponent as i32);
        sign * significand * exponent
    }

    /// Create a `BigNumber` from an i64.
    #[must_use] pub fn from_i64(value: i64) -> Self {
        if value == 0 {
            return Self::zero();
        }

        let sign = if value < 0 { -1 } else { 1 };
        let significand = value.unsigned_abs();

        Self::new(sign, significand, 0)
    }

    /// Create a `BigNumber` from a u64.
    #[must_use] pub fn from_u64(value: u64) -> Self {
        Self::new(1, value, 0)
    }

    /// Format this BigNumber as a string in exponential notation.
    /// Format: ["-"]<significand>"e"<exponent>
    /// Examples: "1e6", "-1e6", "15e5", "0e0"
    #[must_use]
    pub fn to_string_notation(&self) -> String {
        if self.significand == 0 {
            return "0e0".to_string();
        }
        let sign = if self.sign < 0 { "-" } else { "" };
        format!("{}{}e{}", sign, self.significand, self.exponent)
    }
}

impl Default for BigNumber {
    fn default() -> Self {
        Self::zero()
    }
}

/// Default resource limits per the BONJSON specification.
pub mod limits {
    /// Maximum document size in bytes (2 billion)
    pub const MAX_DOCUMENT_SIZE: usize = 2_000_000_000;

    /// Maximum container nesting depth
    pub const MAX_DEPTH: usize = 512;

    /// Maximum elements in a single container
    pub const MAX_CONTAINER_SIZE: usize = 1_000_000;

    /// Maximum string length in bytes
    pub const MAX_STRING_LENGTH: usize = 10_000_000;

    /// Maximum BigNumber exponent (absolute value)
    pub const MAX_BIGNUMBER_EXPONENT: usize = 1_000_000;

    /// Maximum BigNumber magnitude in bytes
    pub const MAX_BIGNUMBER_MAGNITUDE: usize = 8;
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::type_code::*;

    #[test]
    fn test_zigzag() {
        assert_eq!(zigzag_encode(0), 0);
        assert_eq!(zigzag_encode(-1), 1);
        assert_eq!(zigzag_encode(1), 2);
        assert_eq!(zigzag_encode(-2), 3);
        assert_eq!(zigzag_encode(2), 4);
        assert_eq!(zigzag_decode(0), 0);
        assert_eq!(zigzag_decode(1), -1);
        assert_eq!(zigzag_decode(2), 1);
        assert_eq!(zigzag_decode(3), -2);
        assert_eq!(zigzag_decode(4), 2);
    }

    #[test]
    fn test_leb128() {
        let mut buf = [0u8; 10];
        assert_eq!(leb128_encode(0, &mut buf), 1);
        assert_eq!(buf[0], 0);
        assert_eq!(leb128_encode(127, &mut buf), 1);
        assert_eq!(buf[0], 127);
        assert_eq!(leb128_encode(128, &mut buf), 2);
        assert_eq!(buf[0..2], [0x80, 0x01]);

        assert_eq!(leb128_decode(&[0]), Some((0, 1)));
        assert_eq!(leb128_decode(&[127]), Some((127, 1)));
        assert_eq!(leb128_decode(&[0x80, 0x01]), Some((128, 2)));
    }

    #[test]
    fn test_int_sizes() {
        assert_eq!(int_size(UINT8), 1);
        assert_eq!(int_size(UINT16), 2);
        assert_eq!(int_size(UINT32), 4);
        assert_eq!(int_size(UINT64), 8);
        assert_eq!(int_size(SINT8), 1);
        assert_eq!(int_size(SINT16), 2);
        assert_eq!(int_size(SINT32), 4);
        assert_eq!(int_size(SINT64), 8);
    }

    #[test]
    fn test_native_size_lookup() {
        assert_eq!(NATIVE_SIZE_INDEX[0], 0); // 1 byte → index 0
        assert_eq!(NATIVE_SIZE_INDEX[1], 1); // 2 bytes → index 1
        assert_eq!(NATIVE_SIZE_INDEX[2], 2); // 3 bytes → index 2 (round to 4)
        assert_eq!(NATIVE_SIZE_INDEX[3], 2); // 4 bytes → index 2
        assert_eq!(NATIVE_SIZE_INDEX[4], 3); // 5 bytes → index 3 (round to 8)
    }

    #[test]
    fn test_short_string_range() {
        assert!(is_short_string(STRING0));
        assert!(is_short_string(STRING_MAX));
        assert!(!is_short_string(0x64)); // small int 100
        assert!(!is_short_string(0xa8)); // uint8
    }

    #[test]
    fn test_small_int_range() {
        assert!(is_small_int(0x00)); // 0
        assert!(is_small_int(0x64)); // 100
        assert!(!is_small_int(0x65)); // string0
        assert_eq!(small_int_value(0x00), 0);
        assert_eq!(small_int_value(0x64), 100);
        assert_eq!(small_int_value(0x2a), 42);
        assert_eq!(small_int_code(0), 0x00);
        assert_eq!(small_int_code(100), 0x64);
    }

    #[test]
    fn test_reserved() {
        assert!(is_reserved(0xbb));
        assert!(is_reserved(0xf4));
        assert!(!is_reserved(0xba)); // record instance
        assert!(!is_reserved(0xf5)); // typed array
    }

    #[test]
    fn test_typed_array() {
        assert!(is_typed_array(TYPED_ARRAY_FLOAT64));
        assert!(is_typed_array(TYPED_ARRAY_UINT8));
        assert!(!is_typed_array(0xf4)); // reserved
        assert!(!is_typed_array(0xff)); // string_long
        assert_eq!(typed_array_element_size(TYPED_ARRAY_FLOAT64), 8);
        assert_eq!(typed_array_element_size(TYPED_ARRAY_FLOAT32), 4);
        assert_eq!(typed_array_element_size(TYPED_ARRAY_SINT64), 8);
        assert_eq!(typed_array_element_size(TYPED_ARRAY_SINT8), 1);
        assert_eq!(typed_array_element_size(TYPED_ARRAY_UINT8), 1);
        assert_eq!(typed_array_element_size(TYPED_ARRAY_UINT16), 2);
    }
}
