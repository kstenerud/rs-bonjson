// ABOUTME: Dynamic JSON value type for BONJSON.
// ABOUTME: Similar to serde_json::Value but includes BigNumber for lossless representation.


use crate::types::BigNumber;
use std::collections::BTreeMap;
use std::fmt;

/// A BONJSON value that can hold any JSON-compatible type.
///
/// This is similar to `serde_json::Value` but includes support for
/// `BigNumber` to enable lossless round-tripping of arbitrary-precision numbers.
#[derive(Clone, PartialEq, Default)]
pub enum Value {
    /// JSON null
    #[default]
    Null,
    /// JSON boolean
    Bool(bool),
    /// A signed 64-bit integer
    Int(i64),
    /// An unsigned 64-bit integer
    UInt(u64),
    /// A 64-bit floating point number
    Float(f64),
    /// An arbitrary-precision decimal number
    BigNumber(BigNumber),
    /// A UTF-8 string
    String(String),
    /// A JSON array
    Array(Vec<Value>),
    /// A JSON object (using `BTreeMap` for deterministic ordering)
    Object(BTreeMap<String, Value>),
}

impl Value {
    /// Returns true if this value is null.
    #[must_use] pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// Returns true if this value is a boolean.
    #[must_use] pub fn is_bool(&self) -> bool {
        matches!(self, Value::Bool(_))
    }

    /// Returns true if this value is any numeric type.
    #[must_use] pub fn is_number(&self) -> bool {
        matches!(
            self,
            Value::Int(_) | Value::UInt(_) | Value::Float(_) | Value::BigNumber(_)
        )
    }

    /// Returns true if this value is a string.
    #[must_use] pub fn is_string(&self) -> bool {
        matches!(self, Value::String(_))
    }

    /// Returns true if this value is an array.
    #[must_use] pub fn is_array(&self) -> bool {
        matches!(self, Value::Array(_))
    }

    /// Returns true if this value is an object.
    #[must_use] pub fn is_object(&self) -> bool {
        matches!(self, Value::Object(_))
    }

    /// If this is a boolean, returns the value.
    #[must_use] pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// If this is an integer, returns the value as i64.
    #[must_use]
    #[allow(clippy::cast_possible_wrap)] // try_from check ensures no wrap
    #[allow(clippy::cast_precision_loss)] // Intentional: range check uses f64
    #[allow(clippy::cast_possible_truncation)] // Range checked before cast
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(n) => Some(*n),
            Value::UInt(n) if i64::try_from(*n).is_ok() => Some(*n as i64),
            Value::Float(f) if f.fract() == 0.0 && *f >= i64::MIN as f64 && *f <= i64::MAX as f64 => {
                Some(*f as i64)
            }
            Value::BigNumber(bn) => bn.to_i64(),
            _ => None,
        }
    }

    /// If this is an integer, returns the value as u64.
    #[must_use]
    #[allow(clippy::cast_sign_loss)] // >= 0 checked before cast
    #[allow(clippy::cast_precision_loss)] // Intentional: range check uses f64
    #[allow(clippy::cast_possible_truncation)] // Range checked before cast
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Value::UInt(n) => Some(*n),
            Value::Int(n) if *n >= 0 => Some(*n as u64),
            Value::Float(f) if f.fract() == 0.0 && *f >= 0.0 && *f <= u64::MAX as f64 => {
                Some(*f as u64)
            }
            Value::BigNumber(bn) => bn.to_u64(),
            _ => None,
        }
    }

    /// If this is a number, returns the value as f64.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // Intentional: int-to-float conversion may lose precision
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Float(f) => Some(*f),
            Value::Int(n) => Some(*n as f64),
            Value::UInt(n) => Some(*n as f64),
            Value::BigNumber(bn) => Some(bn.to_f64()),
            _ => None,
        }
    }

    /// If this is a string, returns a reference to it.
    #[must_use] pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// If this is an array, returns a reference to it.
    #[must_use] pub fn as_array(&self) -> Option<&Vec<Value>> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    /// If this is an array, returns a mutable reference to it.
    pub fn as_array_mut(&mut self) -> Option<&mut Vec<Value>> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    /// If this is an object, returns a reference to it.
    #[must_use] pub fn as_object(&self) -> Option<&BTreeMap<String, Value>> {
        match self {
            Value::Object(o) => Some(o),
            _ => None,
        }
    }

    /// If this is an object, returns a mutable reference to it.
    pub fn as_object_mut(&mut self) -> Option<&mut BTreeMap<String, Value>> {
        match self {
            Value::Object(o) => Some(o),
            _ => None,
        }
    }

    /// Index into an array. Returns None if not an array or index out of bounds.
    #[must_use] pub fn get(&self, index: usize) -> Option<&Value> {
        self.as_array().and_then(|a| a.get(index))
    }

    /// Index into an object by key. Returns None if not an object or key not found.
    #[must_use] pub fn get_key(&self, key: &str) -> Option<&Value> {
        self.as_object().and_then(|o| o.get(key))
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Null => write!(f, "Null"),
            Value::Bool(b) => write!(f, "Bool({b})"),
            Value::Int(n) => write!(f, "Int({n})"),
            Value::UInt(n) => write!(f, "UInt({n})"),
            Value::Float(n) => write!(f, "Float({n})"),
            Value::BigNumber(bn) => write!(f, "BigNumber({bn:?})"),
            Value::String(s) => write!(f, "String({s:?})"),
            Value::Array(a) => f.debug_tuple("Array").field(a).finish(),
            Value::Object(o) => f.debug_tuple("Object").field(o).finish(),
        }
    }
}

// Implement Display for human-readable output (JSON-like)
impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Null => write!(f, "null"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Int(n) => write!(f, "{n}"),
            Value::UInt(n) => write!(f, "{n}"),
            Value::Float(n) => {
                if n.is_finite() {
                    write!(f, "{n}")
                } else if n.is_nan() {
                    write!(f, "NaN")
                } else if n.is_sign_positive() {
                    write!(f, "Infinity")
                } else {
                    write!(f, "-Infinity")
                }
            }
            Value::BigNumber(bn) => {
                let val = bn.to_f64();
                if bn.sign < 0 {
                    write!(f, "-")?;
                }
                write!(f, "{}e{}", bn.significand, bn.exponent)?;
                write!(f, " (≈{val})")
            }
            Value::String(s) => write!(f, "\"{}\"", s.escape_default()),
            Value::Array(a) => {
                write!(f, "[")?;
                for (i, v) in a.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{v}")?;
                }
                write!(f, "]")
            }
            Value::Object(o) => {
                write!(f, "{{")?;
                for (i, (k, v)) in o.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "\"{}\": {}", k.escape_default(), v)?;
                }
                write!(f, "}}")
            }
        }
    }
}

// Convenient From implementations
impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}

impl From<i8> for Value {
    fn from(n: i8) -> Self {
        Value::Int(i64::from(n))
    }
}

impl From<i16> for Value {
    fn from(n: i16) -> Self {
        Value::Int(i64::from(n))
    }
}

impl From<i32> for Value {
    fn from(n: i32) -> Self {
        Value::Int(i64::from(n))
    }
}

impl From<i64> for Value {
    fn from(n: i64) -> Self {
        Value::Int(n)
    }
}

impl From<u8> for Value {
    fn from(n: u8) -> Self {
        Value::Int(i64::from(n))
    }
}

impl From<u16> for Value {
    fn from(n: u16) -> Self {
        Value::Int(i64::from(n))
    }
}

impl From<u32> for Value {
    fn from(n: u32) -> Self {
        Value::Int(i64::from(n))
    }
}

impl From<u64> for Value {
    #[allow(clippy::cast_possible_wrap)] // try_from check ensures no wrap
    fn from(n: u64) -> Self {
        if i64::try_from(n).is_ok() {
            Value::Int(n as i64)
        } else {
            Value::UInt(n)
        }
    }
}

impl From<f32> for Value {
    fn from(n: f32) -> Self {
        Value::Float(f64::from(n))
    }
}

impl From<f64> for Value {
    fn from(n: f64) -> Self {
        Value::Float(n)
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::String(s)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::String(s.to_owned())
    }
}

impl<T: Into<Value>> From<Vec<T>> for Value {
    fn from(v: Vec<T>) -> Self {
        Value::Array(v.into_iter().map(Into::into).collect())
    }
}

impl From<BigNumber> for Value {
    fn from(bn: BigNumber) -> Self {
        Value::BigNumber(bn)
    }
}

impl<T: Into<Value>> FromIterator<T> for Value {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Value::Array(iter.into_iter().map(Into::into).collect())
    }
}

/// Macro for creating BONJSON values easily.
///
/// This is the BONJSON-specific name. For drop-in `serde_json` compatibility,
/// you can also use [`json!`] which is an alias for this macro.
///
/// # Examples
///
/// ```rust
/// use serde_bonjson::bonjson;
///
/// let value = bonjson!({
///     "name": "test",
///     "values": [1, 2, 3],
///     "active": true
/// });
/// ```
#[macro_export]
macro_rules! bonjson {
    // null
    (null) => {
        $crate::Value::Null
    };

    // bool
    (true) => {
        $crate::Value::Bool(true)
    };
    (false) => {
        $crate::Value::Bool(false)
    };

    // array
    ([ $($elem:tt),* $(,)? ]) => {
        $crate::Value::Array(vec![ $( $crate::bonjson!($elem) ),* ])
    };

    // object
    ({ $($key:tt : $value:tt),* $(,)? }) => {
        {
            let mut map = std::collections::BTreeMap::new();
            $(
                map.insert(String::from($key), $crate::bonjson!($value));
            )*
            $crate::Value::Object(map)
        }
    };

    // other expressions (numbers, strings, etc.)
    ($other:expr) => {
        $crate::Value::from($other)
    };
}

/// Alias for [`bonjson!`] for drop-in `serde_json` compatibility.
///
/// This allows users to migrate from `serde_json` with minimal code changes:
///
/// ```rust
/// use serde_bonjson as serde_json;
/// use serde_json::json;
///
/// let value = json!({
///     "name": "test",
///     "values": [1, 2, 3],
///     "active": true
/// });
/// ```
#[macro_export]
macro_rules! json {
    ($($tt:tt)*) => {
        $crate::bonjson!($($tt)*)
    };
}
