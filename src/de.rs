// ABOUTME: Serde Deserializer implementation for BONJSON decoding.
// ABOUTME: Allows BONJSON bytes to be decoded into any serde-deserializable Rust type.

use crate::decoder::{DecodedValue, Decoder, DecoderConfig};
use crate::error::{Error, Result};
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::Deserialize;

/// A serde Deserializer that reads BONJSON.
pub struct Deserializer<'de> {
    decoder: Decoder<'de>,
    /// Peeked value for look-ahead
    peeked: Option<DecodedValue<'de>>,
}

impl<'de> Deserializer<'de> {
    /// Create a new Deserializer from a byte slice.
    pub fn from_slice(data: &'de [u8]) -> Self {
        Self {
            decoder: Decoder::new(data),
            peeked: None,
        }
    }

    /// Create a new Deserializer with custom configuration.
    pub fn from_slice_with_config(data: &'de [u8], config: DecoderConfig) -> Self {
        Self {
            decoder: Decoder::with_config(data, config),
            peeked: None,
        }
    }

    /// Get the underlying decoder (consumes self).
    pub fn into_decoder(self) -> Decoder<'de> {
        self.decoder
    }

    fn peek_value(&mut self) -> Result<&DecodedValue<'de>> {
        if self.peeked.is_none() {
            self.peeked = Some(self.decoder.decode_value_unchecked()?);
        }
        Ok(self.peeked.as_ref().unwrap())
    }

    fn next_value(&mut self) -> Result<DecodedValue<'de>> {
        match self.peeked.take() {
            Some(v) => Ok(v),
            None => self.decoder.decode_value_unchecked(),
        }
    }
}

/// Deserialize a value from a BONJSON byte slice.
pub fn from_slice<'de, T: Deserialize<'de>>(data: &'de [u8]) -> Result<T> {
    let mut de = Deserializer::from_slice(data);
    de.decoder.check_document_size()?;
    let value = T::deserialize(&mut de)?;
    de.decoder.finish()?;
    Ok(value)
}

/// Deserialize a value from a BONJSON byte slice with custom configuration.
pub fn from_slice_with_config<'de, T: Deserialize<'de>>(
    data: &'de [u8],
    config: DecoderConfig,
) -> Result<T> {
    let mut de = Deserializer::from_slice_with_config(data, config);
    de.decoder.check_document_size()?;
    let value = T::deserialize(&mut de)?;
    de.decoder.finish()?;
    Ok(value)
}

impl<'de, 'a> de::Deserializer<'de> for &'a mut Deserializer<'de> {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self.next_value()? {
            DecodedValue::Null => visitor.visit_unit(),
            DecodedValue::Bool(b) => visitor.visit_bool(b),
            DecodedValue::Int(n) => visitor.visit_i64(n),
            DecodedValue::UInt(n) => visitor.visit_u64(n),
            DecodedValue::Float(f) => visitor.visit_f64(f),
            DecodedValue::BigNumber(bn) => {
                // Try to convert to a native type
                if let Some(i) = bn.to_i64() {
                    visitor.visit_i64(i)
                } else if let Some(u) = bn.to_u64() {
                    visitor.visit_u64(u)
                } else {
                    visitor.visit_f64(bn.to_f64())
                }
            }
            DecodedValue::String(s) => visitor.visit_borrowed_str(s),
            DecodedValue::ArrayStart => {
                let seq = SeqDeserializer::new(self);
                visitor.visit_seq(seq)
            }
            DecodedValue::ObjectStart => {
                let map = MapDeserializer::new(self);
                visitor.visit_map(map)
            }
            DecodedValue::ContainerEnd => Err(Error::UnbalancedContainers),
        }
    }

    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self.next_value()? {
            DecodedValue::Bool(b) => visitor.visit_bool(b),
            _ => Err(Error::Custom("expected bool".into())),
        }
    }

    fn deserialize_i8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self.next_value()? {
            DecodedValue::Int(n) => visitor.visit_i64(n),
            DecodedValue::UInt(n) if n <= i64::MAX as u64 => visitor.visit_i64(n as i64),
            DecodedValue::Float(f) if f.fract() == 0.0 => visitor.visit_i64(f as i64),
            DecodedValue::BigNumber(bn) => {
                if let Some(n) = bn.to_i64() {
                    visitor.visit_i64(n)
                } else {
                    Err(Error::ValueOutOfRange)
                }
            }
            _ => Err(Error::Custom("expected integer".into())),
        }
    }

    fn deserialize_u8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self.next_value()? {
            DecodedValue::UInt(n) => visitor.visit_u64(n),
            DecodedValue::Int(n) if n >= 0 => visitor.visit_u64(n as u64),
            DecodedValue::Float(f) if f.fract() == 0.0 && f >= 0.0 => visitor.visit_u64(f as u64),
            DecodedValue::BigNumber(bn) => {
                if let Some(n) = bn.to_u64() {
                    visitor.visit_u64(n)
                } else {
                    Err(Error::ValueOutOfRange)
                }
            }
            _ => Err(Error::Custom("expected unsigned integer".into())),
        }
    }

    fn deserialize_f32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_f64(visitor)
    }

    fn deserialize_f64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self.next_value()? {
            DecodedValue::Float(f) => visitor.visit_f64(f),
            DecodedValue::Int(n) => visitor.visit_f64(n as f64),
            DecodedValue::UInt(n) => visitor.visit_f64(n as f64),
            DecodedValue::BigNumber(bn) => visitor.visit_f64(bn.to_f64()),
            _ => Err(Error::Custom("expected float".into())),
        }
    }

    fn deserialize_char<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self.next_value()? {
            DecodedValue::String(s) => {
                let mut chars = s.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) => visitor.visit_char(c),
                    _ => Err(Error::Custom("expected single character".into())),
                }
            }
            _ => Err(Error::Custom("expected string".into())),
        }
    }

    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self.next_value()? {
            DecodedValue::String(s) => visitor.visit_borrowed_str(s),
            _ => Err(Error::Custom("expected string".into())),
        }
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_str(visitor)
    }

    fn deserialize_bytes<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        // Try to decode as an array of integers
        match self.next_value()? {
            DecodedValue::ArrayStart => {
                let mut bytes = Vec::new();
                loop {
                    match self.decoder.decode_value_unchecked()? {
                        DecodedValue::ContainerEnd => break,
                        DecodedValue::Int(n) if n >= 0 && n <= 255 => bytes.push(n as u8),
                        DecodedValue::UInt(n) if n <= 255 => bytes.push(n as u8),
                        _ => return Err(Error::Custom("expected byte array".into())),
                    }
                }
                visitor.visit_bytes(&bytes)
            }
            _ => Err(Error::Custom("expected array of bytes".into())),
        }
    }

    fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_bytes(visitor)
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self.peek_value()? {
            DecodedValue::Null => {
                self.next_value()?;
                visitor.visit_none()
            }
            _ => visitor.visit_some(self),
        }
    }

    fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self.next_value()? {
            DecodedValue::Null => visitor.visit_unit(),
            _ => Err(Error::Custom("expected null".into())),
        }
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value> {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self.next_value()? {
            DecodedValue::ArrayStart => {
                let seq = SeqDeserializer::new(self);
                visitor.visit_seq(seq)
            }
            _ => Err(Error::Custom("expected array".into())),
        }
    }

    fn deserialize_tuple<V: Visitor<'de>>(self, _len: usize, visitor: V) -> Result<V::Value> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self.next_value()? {
            DecodedValue::ObjectStart => {
                let map = MapDeserializer::new(self);
                visitor.visit_map(map)
            }
            _ => Err(Error::Custom("expected object".into())),
        }
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        self.deserialize_map(visitor)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        match self.peek_value()? {
            DecodedValue::String(_) => {
                // Unit variant: just a string
                visitor.visit_enum(UnitVariantDeserializer::new(self))
            }
            DecodedValue::ObjectStart => {
                // Other variants: object with single key
                self.next_value()?;
                visitor.visit_enum(EnumDeserializer::new(self))
            }
            _ => Err(Error::Custom("expected string or object for enum".into())),
        }
    }

    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_str(visitor)
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_any(visitor)
    }
}

struct SeqDeserializer<'a, 'de> {
    de: &'a mut Deserializer<'de>,
}

impl<'a, 'de> SeqDeserializer<'a, 'de> {
    fn new(de: &'a mut Deserializer<'de>) -> Self {
        SeqDeserializer { de }
    }
}

impl<'a, 'de> SeqAccess<'de> for SeqDeserializer<'a, 'de> {
    type Error = Error;

    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>> {
        match self.de.peek_value()? {
            DecodedValue::ContainerEnd => {
                self.de.next_value()?;
                Ok(None)
            }
            _ => seed.deserialize(&mut *self.de).map(Some),
        }
    }
}

struct MapDeserializer<'a, 'de> {
    de: &'a mut Deserializer<'de>,
}

impl<'a, 'de> MapDeserializer<'a, 'de> {
    fn new(de: &'a mut Deserializer<'de>) -> Self {
        MapDeserializer { de }
    }
}

impl<'a, 'de> MapAccess<'de> for MapDeserializer<'a, 'de> {
    type Error = Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>> {
        match self.de.peek_value()? {
            DecodedValue::ContainerEnd => {
                self.de.next_value()?;
                Ok(None)
            }
            _ => seed.deserialize(&mut *self.de).map(Some),
        }
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value> {
        seed.deserialize(&mut *self.de)
    }
}

struct UnitVariantDeserializer<'a, 'de> {
    de: &'a mut Deserializer<'de>,
}

impl<'a, 'de> UnitVariantDeserializer<'a, 'de> {
    fn new(de: &'a mut Deserializer<'de>) -> Self {
        UnitVariantDeserializer { de }
    }
}

impl<'a, 'de> de::EnumAccess<'de> for UnitVariantDeserializer<'a, 'de> {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V: DeserializeSeed<'de>>(self, seed: V) -> Result<(V::Value, Self::Variant)> {
        let variant = seed.deserialize(&mut *self.de)?;
        Ok((variant, self))
    }
}

impl<'a, 'de> de::VariantAccess<'de> for UnitVariantDeserializer<'a, 'de> {
    type Error = Error;

    fn unit_variant(self) -> Result<()> {
        Ok(())
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, _seed: T) -> Result<T::Value> {
        Err(Error::Custom("expected unit variant".into()))
    }

    fn tuple_variant<V: Visitor<'de>>(self, _len: usize, _visitor: V) -> Result<V::Value> {
        Err(Error::Custom("expected unit variant".into()))
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        _fields: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value> {
        Err(Error::Custom("expected unit variant".into()))
    }
}

struct EnumDeserializer<'a, 'de> {
    de: &'a mut Deserializer<'de>,
}

impl<'a, 'de> EnumDeserializer<'a, 'de> {
    fn new(de: &'a mut Deserializer<'de>) -> Self {
        EnumDeserializer { de }
    }
}

impl<'a, 'de> de::EnumAccess<'de> for EnumDeserializer<'a, 'de> {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V: DeserializeSeed<'de>>(self, seed: V) -> Result<(V::Value, Self::Variant)> {
        let variant = seed.deserialize(&mut *self.de)?;
        Ok((variant, self))
    }
}

impl<'a, 'de> de::VariantAccess<'de> for EnumDeserializer<'a, 'de> {
    type Error = Error;

    fn unit_variant(self) -> Result<()> {
        Err(Error::Custom("expected newtype, tuple, or struct variant".into()))
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value> {
        let value = seed.deserialize(&mut *self.de)?;
        // Consume the closing container end
        match self.de.next_value()? {
            DecodedValue::ContainerEnd => Ok(value),
            _ => Err(Error::Custom("expected container end".into())),
        }
    }

    fn tuple_variant<V: Visitor<'de>>(self, _len: usize, visitor: V) -> Result<V::Value> {
        match self.de.next_value()? {
            DecodedValue::ArrayStart => {
                let seq = SeqDeserializer::new(self.de);
                let value = visitor.visit_seq(seq)?;
                // Consume the closing container end
                match self.de.next_value()? {
                    DecodedValue::ContainerEnd => Ok(value),
                    _ => Err(Error::Custom("expected container end".into())),
                }
            }
            _ => Err(Error::Custom("expected array for tuple variant".into())),
        }
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        match self.de.next_value()? {
            DecodedValue::ObjectStart => {
                let map = MapDeserializer::new(self.de);
                let value = visitor.visit_map(map)?;
                // Consume the closing container end
                match self.de.next_value()? {
                    DecodedValue::ContainerEnd => Ok(value),
                    _ => Err(Error::Custom("expected container end".into())),
                }
            }
            _ => Err(Error::Custom("expected object for struct variant".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[test]
    fn test_deserialize_primitives() {
        assert_eq!(from_slice::<bool>(&[0x6f]).unwrap(), true);
        assert_eq!(from_slice::<bool>(&[0x6e]).unwrap(), false);
        assert_eq!(from_slice::<i32>(&[0x2a]).unwrap(), 42);
        assert_eq!(
            from_slice::<String>(&[0x85, b'h', b'e', b'l', b'l', b'o']).unwrap(),
            "hello"
        );
    }

    #[test]
    fn test_deserialize_option() {
        assert_eq!(from_slice::<Option<i32>>(&[0x6d]).unwrap(), None);
        assert_eq!(from_slice::<Option<i32>>(&[0x2a]).unwrap(), Some(42));
    }

    #[test]
    fn test_deserialize_vec() {
        assert_eq!(
            from_slice::<Vec<i32>>(&[0x99, 0x01, 0x02, 0x03, 0x9b]).unwrap(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn test_deserialize_struct() {
        #[derive(Debug, Deserialize, PartialEq)]
        struct Point {
            x: i32,
            y: i32,
        }

        // {"x": 1, "y": 2}
        let bytes = vec![0x9a, 0x81, b'x', 0x01, 0x81, b'y', 0x02, 0x9b];
        assert_eq!(from_slice::<Point>(&bytes).unwrap(), Point { x: 1, y: 2 });
    }

    #[test]
    fn test_deserialize_enum() {
        #[derive(Debug, Deserialize, PartialEq)]
        enum Color {
            Red,
            Green,
            Blue,
        }

        // "Red"
        let bytes = vec![0x83, b'R', b'e', b'd'];
        assert_eq!(from_slice::<Color>(&bytes).unwrap(), Color::Red);
    }
}
