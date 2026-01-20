// ABOUTME: Serde Deserializer implementation for BONJSON decoding.
// ABOUTME: Allows BONJSON bytes to be decoded into any serde-deserializable Rust type.

use crate::decoder::{DecodedValue, Decoder, DecoderConfig};
use crate::error::{Error, Result};
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::Deserialize;

/// A serde Deserializer that reads BONJSON.
pub struct Deserializer<'de> {
    decoder: Decoder<'de>,
}

impl<'de> Deserializer<'de> {
    /// Create a new Deserializer from a byte slice.
    #[must_use] pub fn from_slice(data: &'de [u8]) -> Self {
        Self {
            decoder: Decoder::new(data),
        }
    }

    /// Create a new Deserializer with custom configuration.
    #[must_use] pub fn from_slice_with_config(data: &'de [u8], config: DecoderConfig) -> Self {
        Self {
            decoder: Decoder::with_config(data, config),
        }
    }

    /// Get the underlying decoder (consumes self).
    #[must_use] pub fn into_decoder(self) -> Decoder<'de> {
        self.decoder
    }
}

/// Deserialize a value from a BONJSON byte slice.
///
/// # Errors
///
/// Returns an error if:
/// - The document exceeds size limits
/// - The data is malformed or truncated
/// - The data doesn't match the expected type `T`
/// - There are trailing bytes after the value
pub fn from_slice<'de, T: Deserialize<'de>>(data: &'de [u8]) -> Result<T> {
    let mut de = Deserializer::from_slice(data);
    de.decoder.check_document_size()?;
    let value = T::deserialize(&mut de)?;
    de.decoder.finish()?;
    Ok(value)
}

/// Deserialize a value from a BONJSON byte slice with custom configuration.
///
/// # Errors
///
/// Returns an error if:
/// - The document exceeds configured limits
/// - The data is malformed or truncated
/// - The data doesn't match the expected type `T`
/// - There are trailing bytes (unless `allow_trailing_bytes` is set)
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

impl<'de> de::Deserializer<'de> for &mut Deserializer<'de> {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self.decoder.decode_value_unchecked()? {
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
        // Use direct decode to skip DecodedValue intermediate
        visitor.visit_bool(self.decoder.decode_bool_direct()?)
    }

    fn deserialize_i8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_i64(self.decoder.decode_i64_direct()?)
    }

    fn deserialize_i16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_i64(self.decoder.decode_i64_direct()?)
    }

    fn deserialize_i32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_i64(self.decoder.decode_i64_direct()?)
    }

    fn deserialize_i64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_i64(self.decoder.decode_i64_direct()?)
    }

    fn deserialize_u8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_u64(self.decoder.decode_u64_direct()?)
    }

    fn deserialize_u16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_u64(self.decoder.decode_u64_direct()?)
    }

    fn deserialize_u32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_u64(self.decoder.decode_u64_direct()?)
    }

    fn deserialize_u64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_u64(self.decoder.decode_u64_direct()?)
    }

    fn deserialize_f32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_f64(self.decoder.decode_f64_direct()?)
    }

    fn deserialize_f64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_f64(self.decoder.decode_f64_direct()?)
    }

    fn deserialize_char<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let s = self.decoder.decode_str_direct()?;
        let mut chars = s.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) => visitor.visit_char(c),
            _ => Err(Error::Custom("expected single character".into())),
        }
    }

    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_borrowed_str(self.decoder.decode_str_direct()?)
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_borrowed_str(self.decoder.decode_str_direct()?)
    }

    fn deserialize_bytes<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        use crate::types::type_code;

        // Expect array start
        self.decoder.expect_array_start()?;
        let mut bytes = Vec::new();
        loop {
            if self.decoder.try_consume_container_end()? {
                break;
            }
            // Each element should be an integer 0-255
            let tc = self.decoder.peek_type_code()?;
            // Small ints: tc 0x64-0xc8 → values 0-100
            if type_code::is_small_int(tc) {
                let val = type_code::small_int_value(tc);
                if val < 0 {
                    return Err(Error::Custom("expected byte array".into()));
                }
                self.decoder.skip_byte();
                bytes.push(val as u8);
            } else if tc == type_code::UINT8 {
                // uint8 (values 101-255 need this encoding)
                self.decoder.skip_byte();
                let b = self.decoder.read_byte_unchecked();
                bytes.push(b);
            } else {
                return Err(Error::Custom("expected byte array".into()));
            }
        }
        visitor.visit_bytes(&bytes)
    }

    fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_bytes(visitor)
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        // Use peek_type_code to check for null without full decode
        if self.decoder.peek_type_code()? == crate::types::type_code::NULL {
            self.decoder.skip_byte(); // consume the null type code
            self.decoder.consume_element(); // update container state
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }

    fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        // Expect null type code directly
        if self.decoder.peek_type_code()? == crate::types::type_code::NULL {
            self.decoder.skip_byte(); // consume the null type code
            self.decoder.consume_element(); // update container state
            visitor.visit_unit()
        } else {
            Err(Error::Custom("expected null".into()))
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
        self.decoder.expect_array_start()?;
        let seq = SeqDeserializer::new(self);
        visitor.visit_seq(seq)
    }

    fn deserialize_tuple<V: Visitor<'de>>(self, _len: usize, visitor: V) -> Result<V::Value> {
        self.decoder.expect_array_start()?;
        let seq = SeqDeserializer::new(self);
        visitor.visit_seq(seq)
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value> {
        self.decoder.expect_array_start()?;
        let seq = SeqDeserializer::new(self);
        visitor.visit_seq(seq)
    }

    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.decoder.expect_object_start()?;
        let map = MapDeserializer::new(self);
        visitor.visit_map(map)
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        self.decoder.expect_object_start()?;
        let map = MapDeserializer::new(self);
        visitor.visit_map(map)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        use crate::types::type_code;
        let tc = self.decoder.peek_type_code()?;
        // Check if it's a string (short: 0xe0-0xef, long: 0xf0)
        if type_code::is_short_string(tc) || tc == type_code::STRING_LONG {
            // Unit variant: just a string
            visitor.visit_enum(UnitVariantDeserializer::new(self))
        } else if tc == type_code::OBJECT {
            // Other variants: object with single key
            self.decoder.expect_object_start()?;
            visitor.visit_enum(EnumDeserializer::new(self))
        } else {
            Err(Error::Custom("expected string or object for enum".into()))
        }
    }

    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        // Field names are always strings - use direct decode
        visitor.visit_borrowed_str(self.decoder.decode_str_direct()?)
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

impl<'de> SeqAccess<'de> for SeqDeserializer<'_, 'de> {
    type Error = Error;

    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>> {
        // Single check+consume operation
        if self.de.decoder.try_consume_container_end()? {
            return Ok(None);
        }
        seed.deserialize(&mut *self.de).map(Some)
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

impl<'de> MapAccess<'de> for MapDeserializer<'_, 'de> {
    type Error = Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>> {
        // Single check+consume operation
        if self.de.decoder.try_consume_container_end()? {
            return Ok(None);
        }
        seed.deserialize(&mut *self.de).map(Some)
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

impl<'de> de::EnumAccess<'de> for UnitVariantDeserializer<'_, 'de> {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V: DeserializeSeed<'de>>(self, seed: V) -> Result<(V::Value, Self::Variant)> {
        let variant = seed.deserialize(&mut *self.de)?;
        Ok((variant, self))
    }
}

impl<'de> de::VariantAccess<'de> for UnitVariantDeserializer<'_, 'de> {
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

impl<'de> de::EnumAccess<'de> for EnumDeserializer<'_, 'de> {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V: DeserializeSeed<'de>>(self, seed: V) -> Result<(V::Value, Self::Variant)> {
        let variant = seed.deserialize(&mut *self.de)?;
        Ok((variant, self))
    }
}

impl<'de> de::VariantAccess<'de> for EnumDeserializer<'_, 'de> {
    type Error = Error;

    fn unit_variant(self) -> Result<()> {
        Err(Error::Custom("expected newtype, tuple, or struct variant".into()))
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value> {
        let value = seed.deserialize(&mut *self.de)?;
        // With chunked containers, the outer object ends naturally after the single key-value pair
        // Pop the container state
        self.de.decoder.end_container()?;
        Ok(value)
    }

    fn tuple_variant<V: Visitor<'de>>(self, _len: usize, visitor: V) -> Result<V::Value> {
        self.de.decoder.expect_array_start()?;
        let seq = SeqDeserializer::new(self.de);
        let value = visitor.visit_seq(seq)?;
        // With chunked containers, the outer object ends naturally after the single key-value pair
        // Pop the container state
        self.de.decoder.end_container()?;
        Ok(value)
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        self.de.decoder.expect_object_start()?;
        let map = MapDeserializer::new(self.de);
        let value = visitor.visit_map(map)?;
        // With chunked containers, the outer object ends naturally after the single key-value pair
        // Pop the container state
        self.de.decoder.end_container()?;
        Ok(value)
    }
}
