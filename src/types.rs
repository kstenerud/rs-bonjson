// ABOUTME: Defines BONJSON type codes and the BigNumber type.
// ABOUTME: Type codes map directly to the BONJSON specification byte values.

/// Type codes for BONJSON values.
/// These match the BONJSON specification exactly.
pub mod type_code {
    // Small integers: 0x00-0x64 (0-100), 0x9c-0xff (-100 to -1)
    pub const SMALLINT_MIN: u8 = 0x9c; // -100 as i8
    pub const SMALLINT_MAX: u8 = 0x64; // 100

    // Reserved: 0x65-0x67
    pub const RESERVED_65: u8 = 0x65;
    pub const RESERVED_66: u8 = 0x66;
    pub const RESERVED_67: u8 = 0x67;

    // Long string
    pub const STRING_LONG: u8 = 0x68;

    // Big number
    pub const BIG_NUMBER: u8 = 0x69;

    // Floats
    pub const FLOAT16: u8 = 0x6a;
    pub const FLOAT32: u8 = 0x6b;
    pub const FLOAT64: u8 = 0x6c;

    // Null and booleans
    pub const NULL: u8 = 0x6d;
    pub const FALSE: u8 = 0x6e;
    pub const TRUE: u8 = 0x6f;

    // Unsigned integers (1-8 bytes): 0x70-0x77
    pub const UINT8: u8 = 0x70;
    pub const UINT16: u8 = 0x71;
    pub const UINT24: u8 = 0x72;
    pub const UINT32: u8 = 0x73;
    pub const UINT40: u8 = 0x74;
    pub const UINT48: u8 = 0x75;
    pub const UINT56: u8 = 0x76;
    pub const UINT64: u8 = 0x77;

    // Signed integers (1-8 bytes): 0x78-0x7f
    pub const SINT8: u8 = 0x78;
    pub const SINT16: u8 = 0x79;
    pub const SINT24: u8 = 0x7a;
    pub const SINT32: u8 = 0x7b;
    pub const SINT40: u8 = 0x7c;
    pub const SINT48: u8 = 0x7d;
    pub const SINT56: u8 = 0x7e;
    pub const SINT64: u8 = 0x7f;

    // Short strings (0-15 bytes): 0x80-0x8f
    pub const STRING0: u8 = 0x80;
    pub const STRING15: u8 = 0x8f;

    // Reserved: 0x90-0x98
    pub const RESERVED_90: u8 = 0x90;
    pub const RESERVED_98: u8 = 0x98;

    // Containers
    pub const ARRAY_START: u8 = 0x99;
    pub const OBJECT_START: u8 = 0x9a;
    pub const CONTAINER_END: u8 = 0x9b;

    /// Check if a type code is a small integer (-100 to 100)
    #[inline]
    pub const fn is_small_int(code: u8) -> bool {
        code <= SMALLINT_MAX || code >= SMALLINT_MIN
    }

    /// Decode a small integer type code to its value
    #[inline]
    pub const fn small_int_value(code: u8) -> i8 {
        code as i8
    }

    /// Check if a type code is a short string (0-15 bytes)
    #[inline]
    pub const fn is_short_string(code: u8) -> bool {
        code >= STRING0 && code <= STRING15
    }

    /// Get the length of a short string from its type code
    #[inline]
    pub const fn short_string_len(code: u8) -> usize {
        (code - STRING0) as usize
    }

    /// Check if a type code is an unsigned integer
    #[inline]
    pub const fn is_unsigned_int(code: u8) -> bool {
        code >= UINT8 && code <= UINT64
    }

    /// Get the byte count for an unsigned integer type code
    #[inline]
    pub const fn unsigned_int_size(code: u8) -> usize {
        (code - UINT8 + 1) as usize
    }

    /// Check if a type code is a signed integer
    #[inline]
    pub const fn is_signed_int(code: u8) -> bool {
        code >= SINT8 && code <= SINT64
    }

    /// Get the byte count for a signed integer type code
    #[inline]
    pub const fn signed_int_size(code: u8) -> usize {
        (code - SINT8 + 1) as usize
    }

    /// Check if a type code is reserved
    #[inline]
    pub const fn is_reserved(code: u8) -> bool {
        (code >= RESERVED_65 && code <= RESERVED_67)
            || (code >= RESERVED_90 && code <= RESERVED_98)
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
    /// Create a new BigNumber.
    ///
    /// # Arguments
    /// * `sign` - The sign: 1 for positive, -1 for negative
    /// * `significand` - The absolute value of the significand
    /// * `exponent` - The base-10 exponent
    #[inline]
    pub const fn new(sign: i8, significand: u64, exponent: i32) -> Self {
        Self {
            significand,
            exponent,
            sign,
        }
    }

    /// Create a BigNumber representing zero.
    #[inline]
    pub const fn zero() -> Self {
        Self::new(1, 0, 0)
    }

    /// Create a BigNumber representing negative zero.
    #[inline]
    pub const fn neg_zero() -> Self {
        Self::new(-1, 0, 0)
    }

    /// Check if this BigNumber is zero (positive or negative).
    #[inline]
    pub const fn is_zero(&self) -> bool {
        self.significand == 0
    }

    /// Check if this BigNumber is negative.
    #[inline]
    pub const fn is_negative(&self) -> bool {
        self.sign < 0
    }

    /// Try to convert this BigNumber to an i64.
    /// Returns None if the value cannot be represented exactly.
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

    /// Try to convert this BigNumber to a u64.
    /// Returns None if the value cannot be represented exactly.
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

    /// Try to convert this BigNumber to an f64.
    /// This may lose precision for very large or very precise numbers.
    pub fn to_f64(&self) -> f64 {
        let sign = if self.sign < 0 { -1.0 } else { 1.0 };
        let significand = self.significand as f64;
        let exponent = 10.0f64.powi(self.exponent);
        sign * significand * exponent
    }

    /// Create a BigNumber from an i64.
    pub fn from_i64(value: i64) -> Self {
        if value == 0 {
            return Self::zero();
        }

        let sign = if value < 0 { -1 } else { 1 };
        let significand = value.unsigned_abs();

        Self::new(sign, significand, 0)
    }

    /// Create a BigNumber from a u64.
    pub fn from_u64(value: u64) -> Self {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_small_int_codes() {
        assert!(type_code::is_small_int(0));
        assert!(type_code::is_small_int(100));
        assert!(type_code::is_small_int(0x9c)); // -100
        assert!(type_code::is_small_int(0xff)); // -1

        assert_eq!(type_code::small_int_value(0), 0);
        assert_eq!(type_code::small_int_value(100), 100);
        assert_eq!(type_code::small_int_value(0xff), -1);
        assert_eq!(type_code::small_int_value(0x9c), -100);
    }

    #[test]
    fn test_short_string_codes() {
        assert!(type_code::is_short_string(0x80));
        assert!(type_code::is_short_string(0x8f));
        assert!(!type_code::is_short_string(0x79));

        assert_eq!(type_code::short_string_len(0x80), 0);
        assert_eq!(type_code::short_string_len(0x8f), 15);
    }

    #[test]
    fn test_big_number() {
        let bn = BigNumber::new(1, 15, -1);
        assert_eq!(bn.to_f64(), 1.5);

        let bn = BigNumber::from_i64(-1000);
        assert_eq!(bn.sign, -1);
        assert_eq!(bn.significand, 1000);
        assert_eq!(bn.exponent, 0);
        assert_eq!(bn.to_i64(), Some(-1000));
    }
}
