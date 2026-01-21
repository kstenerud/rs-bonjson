// ABOUTME: Defines BONJSON type codes and the BigNumber type.
// ABOUTME: Type codes map directly to the BONJSON specification byte values.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]

/// Type codes for BONJSON values.
/// These match the BONJSON specification exactly.
pub mod type_code {
    // Small integers: 0x00-0xc8 (values -100 to 100, computed as type_code - 100)
    pub const SMALLINT_MIN: u8 = 0x00; // -100 (0 - 100 = -100)
    pub const SMALLINT_MAX: u8 = 0xc8; // 100 (200 - 100 = 100)
    pub const SMALLINT_ZERO: u8 = 0x64; // 0 (100 - 100 = 0)

    // Reserved: 0xc9-0xcf
    pub const RESERVED_C9: u8 = 0xc9;
    pub const RESERVED_CF: u8 = 0xcf;

    // Unsigned integers (1-8 bytes): 0xd0-0xd7
    pub const UINT8: u8 = 0xd0;
    pub const UINT16: u8 = 0xd1;
    pub const UINT24: u8 = 0xd2;
    pub const UINT32: u8 = 0xd3;
    pub const UINT40: u8 = 0xd4;
    pub const UINT48: u8 = 0xd5;
    pub const UINT56: u8 = 0xd6;
    pub const UINT64: u8 = 0xd7;

    // Signed integers (1-8 bytes): 0xd8-0xdf
    pub const SINT8: u8 = 0xd8;
    pub const SINT16: u8 = 0xd9;
    pub const SINT24: u8 = 0xda;
    pub const SINT32: u8 = 0xdb;
    pub const SINT40: u8 = 0xdc;
    pub const SINT48: u8 = 0xdd;
    pub const SINT56: u8 = 0xde;
    pub const SINT64: u8 = 0xdf;

    // Short strings (0-15 bytes): 0xe0-0xef
    pub const STRING0: u8 = 0xe0;
    pub const STRING15: u8 = 0xef;

    // Long string
    pub const STRING_LONG: u8 = 0xf0;

    // Big number
    pub const BIG_NUMBER: u8 = 0xf1;

    // Floats
    pub const FLOAT16: u8 = 0xf2;
    pub const FLOAT32: u8 = 0xf3;
    pub const FLOAT64: u8 = 0xf4;

    // Null and booleans
    pub const NULL: u8 = 0xf5;
    pub const FALSE: u8 = 0xf6;
    pub const TRUE: u8 = 0xf7;

    // Containers (chunked encoding, no end marker)
    pub const ARRAY: u8 = 0xf8;
    pub const OBJECT: u8 = 0xf9;

    // Reserved: 0xfa-0xff
    pub const RESERVED_FA: u8 = 0xfa;
    pub const RESERVED_FF: u8 = 0xff;

    /// Check if a type code is a small integer (-100 to 100)
    #[inline]
    #[must_use]
    pub const fn is_small_int(code: u8) -> bool {
        code <= SMALLINT_MAX
    }

    /// Decode a small integer type code to its value (`type_code` - 100)
    #[inline]
    #[must_use]
    pub const fn small_int_value(code: u8) -> i8 {
        // code is 0x00-0xc8, subtract 100 to get -100 to 100
        (code as i16 - 100) as i8
    }

    /// Encode a small integer value (-100 to 100) to its type code
    #[inline]
    #[must_use]
    pub const fn small_int_code(value: i8) -> u8 {
        // value is -100 to 100, add 100 to get 0x00-0xc8
        (value as i16 + 100) as u8
    }

    /// Check if a type code is a short string (0-15 bytes)
    #[inline]
    #[must_use]
    pub const fn is_short_string(code: u8) -> bool {
        // 0xe0-0xef: (code & 0xf0) == 0xe0
        (code & 0xf0) == 0xe0
    }

    /// Get the length of a short string from its type code
    #[inline]
    #[must_use]
    pub const fn short_string_len(code: u8) -> usize {
        (code & 0x0f) as usize
    }

    /// Check if a type code is an unsigned integer (0xd0-0xd7)
    #[inline]
    #[must_use]
    pub const fn is_unsigned_int(code: u8) -> bool {
        // (code & 0xf8) == 0xd0
        (code & 0xf8) == 0xd0
    }

    /// Get the byte count for an unsigned integer type code
    #[inline]
    #[must_use]
    pub const fn unsigned_int_size(code: u8) -> usize {
        ((code & 0x07) + 1) as usize
    }

    /// Check if a type code is a signed integer (0xd8-0xdf)
    #[inline]
    #[must_use]
    pub const fn is_signed_int(code: u8) -> bool {
        // (code & 0xf8) == 0xd8
        (code & 0xf8) == 0xd8
    }

    /// Get the byte count for a signed integer type code
    #[inline]
    #[must_use]
    pub const fn signed_int_size(code: u8) -> usize {
        ((code & 0x07) + 1) as usize
    }

    /// Check if a type code is any integer (signed or unsigned): 0xd0-0xdf
    /// This is more efficient than checking signed and unsigned separately.
    #[inline]
    #[must_use]
    pub const fn is_any_int(code: u8) -> bool {
        (code & 0xf0) == 0xd0
    }

    /// Check if an integer type code is signed (bit 3 set).
    /// Only valid when `is_any_int()` returns true.
    #[inline]
    #[must_use]
    pub const fn int_is_signed(code: u8) -> bool {
        (code & 0x08) != 0
    }

    /// Get the byte count for any integer type code (works for both signed and unsigned).
    /// Only valid when `is_any_int()` returns true.
    #[inline]
    #[must_use]
    pub const fn int_size(code: u8) -> usize {
        ((code & 0x07) + 1) as usize
    }

    /// Check if a type code is reserved
    #[inline]
    #[must_use]
    pub const fn is_reserved(code: u8) -> bool {
        (code >= RESERVED_C9 && code <= RESERVED_CF)
            || code >= RESERVED_FA
    }
}

/// A big number with arbitrary precision base-10 representation.
///
/// The value is: sign × significand × 10^exponent
///
/// This type can represent numbers with up to 75 significant digits
/// and exponents in the range ±8 million.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BigNumber {
    /// The absolute value of the significand (0 to 2^64-1)
    pub significand: u64,
    /// The base-10 exponent (-0x800000 to 0x7fffff)
    pub exponent: i32,
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
    #[must_use] pub const fn new(sign: i8, significand: u64, exponent: i32) -> Self {
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

    /// Create a `BigNumber` representing negative zero.
    #[inline]
    #[must_use] pub const fn neg_zero() -> Self {
        Self::new(-1, 0, 0)
    }

    /// Check if this `BigNumber` is zero (positive or negative).
    #[inline]
    #[must_use] pub const fn is_zero(&self) -> bool {
        self.significand == 0
    }

    /// Check if this `BigNumber` is negative.
    #[inline]
    #[must_use] pub const fn is_negative(&self) -> bool {
        self.sign < 0
    }

    /// Try to convert this `BigNumber` to an i64.
    /// Returns None if the value cannot be represented exactly.
    #[must_use]
    #[allow(clippy::cast_sign_loss)] // exponent >= 0 checked above
    #[allow(clippy::cast_possible_wrap)] // checked_mul returns None on overflow
    pub fn to_i64(&self) -> Option<i64> {
        if self.exponent < 0 {
            return None;
        }
        if self.exponent > 18 {
            return None; // Would overflow
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
    #[allow(clippy::cast_sign_loss)] // exponent >= 0 checked above
    pub fn to_u64(&self) -> Option<u64> {
        if self.sign < 0 && self.significand != 0 {
            return None;
        }
        if self.exponent < 0 {
            return None;
        }
        if self.exponent > 19 {
            return None; // Would overflow
        }

        let multiplier = 10u64.checked_pow(self.exponent as u32)?;
        self.significand.checked_mul(multiplier)
    }

    /// Try to convert this `BigNumber` to an f64.
    /// This may lose precision for very large or very precise numbers.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // Documented: may lose precision
    pub fn to_f64(&self) -> f64 {
        let sign = if self.sign < 0 { -1.0 } else { 1.0 };
        let significand = self.significand as f64;
        let exponent = 10.0f64.powi(self.exponent);
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

    /// Maximum chunks per string
    pub const MAX_CHUNKS: usize = 100;
}
