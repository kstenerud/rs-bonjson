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
pub fn from_slice<'de, T: Deserialize<'de>>(data: &'de [u8]) -> Result<T> {
    let mut de = Deserializer::from_slice(data);
    de.decoder.check_document_size()?;
    de.decoder.read_record_definitions()?;
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
    de.decoder.read_record_definitions()?;
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
                if let Some(i) = bn.to_i64() {
                    visitor.visit_i64(i)
                } else if let Some(u) = bn.to_u64() {
                    visitor.visit_u64(u)
                } else {
                    visitor.visit_f64(bn.to_f64())
                }
            }
            DecodedValue::String(s) => match s {
                std::borrow::Cow::Borrowed(b) => visitor.visit_borrowed_str(b),
                std::borrow::Cow::Owned(o) => visitor.visit_string(o),
            },
            DecodedValue::ArrayStart => {
                let seq = SeqDeserializer::new(self);
                visitor.visit_seq(seq)
            }
            DecodedValue::ObjectStart => {
                let map = MapDeserializer::new(self);
                visitor.visit_map(map)
            }
            DecodedValue::TypedArrayStart { element_type_code, count } => {
                let seq = TypedArraySeqDeserializer::new(self, element_type_code, count);
                visitor.visit_seq(seq)
            }
            DecodedValue::RecordInstanceStart(def_index) => {
                let keys = self.decoder.record_definitions()[def_index].clone();
                let map = RecordMapDeserializer::new(self, keys);
                visitor.visit_map(map)
            }
            DecodedValue::ContainerEnd => Err(Error::UnbalancedContainers),
        }
    }

    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
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

        let tc = self.decoder.peek_type_code()?;

        // Support typed uint8 arrays for byte buffers
        if type_code::is_typed_array(tc) && tc == type_code::TYPED_ARRAY_UINT8 {
            self.decoder.skip_byte();
            let remaining = self.decoder.remaining();
            let (count_raw, consumed) = crate::types::leb128_decode(remaining)
                .ok_or(Error::Truncated)?;
            for _ in 0..consumed { self.decoder.skip_byte(); }
            let count = count_raw as usize;
            let mut bytes = Vec::with_capacity(count);
            for _ in 0..count {
                bytes.push(self.decoder.read_byte_unchecked());
            }
            return visitor.visit_bytes(&bytes);
        }

        self.decoder.expect_array_start()?;
        let mut bytes = Vec::new();
        loop {
            if self.decoder.try_consume_container_end()? {
                break;
            }
            let tc = self.decoder.peek_type_code()?;
            if type_code::is_small_int(tc) {
                let val = type_code::small_int_value(tc);
                self.decoder.skip_byte();
                bytes.push(val);
            } else if tc == type_code::UINT8 {
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
        if self.decoder.peek_type_code()? == crate::types::type_code::NULL {
            self.decoder.skip_byte();
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }

    fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        if self.decoder.peek_type_code()? == crate::types::type_code::NULL {
            self.decoder.skip_byte();
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
        let tc = self.decoder.peek_type_code()?;
        if crate::types::type_code::is_typed_array(tc) {
            // Consume the type code and read count
            self.decoder.skip_byte();
            let remaining = self.decoder.remaining();
            let (count_raw, consumed) = crate::types::leb128_decode(remaining)
                .ok_or(Error::Truncated)?;
            for _ in 0..consumed { self.decoder.skip_byte(); }
            let count = count_raw as usize;
            let seq = TypedArraySeqDeserializer::new_without_container(self, tc, count);
            return visitor.visit_seq(seq);
        }
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
        let tc = self.decoder.peek_type_code()?;
        if tc == crate::types::type_code::RECORD_INSTANCE {
            // Consume the type code and read the definition index
            match self.decoder.decode_value_unchecked()? {
                DecodedValue::RecordInstanceStart(def_index) => {
                    let keys = self.decoder.record_definitions()[def_index].clone();
                    let map = RecordMapDeserializer::new(self, keys);
                    visitor.visit_map(map)
                }
                _ => unreachable!(),
            }
        } else {
            self.decoder.expect_object_start()?;
            let map = MapDeserializer::new(self);
            visitor.visit_map(map)
        }
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        use crate::types::type_code;
        let tc = self.decoder.peek_type_code()?;
        if type_code::is_any_string(tc) {
            visitor.visit_enum(UnitVariantDeserializer::new(self))
        } else if tc == type_code::OBJECT {
            self.decoder.expect_object_start()?;
            visitor.visit_enum(EnumDeserializer::new(self))
        } else {
            Err(Error::Custom("expected string or object for enum".into()))
        }
    }

    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
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
        if self.de.decoder.try_consume_container_end()? {
            return Ok(None);
        }
        seed.deserialize(&mut *self.de).map(Some)
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value> {
        seed.deserialize(&mut *self.de)
    }
}

struct TypedArraySeqDeserializer<'a, 'de> {
    de: &'a mut Deserializer<'de>,
    element_type_code: u8,
    remaining: usize,
    has_container: bool,
}

impl<'a, 'de> TypedArraySeqDeserializer<'a, 'de> {
    fn new(de: &'a mut Deserializer<'de>, element_type_code: u8, count: usize) -> Self {
        TypedArraySeqDeserializer { de, element_type_code, remaining: count, has_container: true }
    }

    fn new_without_container(de: &'a mut Deserializer<'de>, element_type_code: u8, count: usize) -> Self {
        TypedArraySeqDeserializer { de, element_type_code, remaining: count, has_container: false }
    }
}

impl<'de> SeqAccess<'de> for TypedArraySeqDeserializer<'_, 'de> {
    type Error = Error;

    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>> {
        if self.remaining == 0 {
            if self.has_container {
                self.de.decoder.end_typed_array()?;
            }
            return Ok(None);
        }
        self.remaining -= 1;
        // Read the element and deserialize it inline
        let elem = self.de.decoder.read_typed_array_element(self.element_type_code)?;
        let value = match elem {
            DecodedValue::Int(n) => seed.deserialize(serde::de::value::I64Deserializer::new(n)),
            DecodedValue::UInt(n) => seed.deserialize(serde::de::value::U64Deserializer::new(n)),
            DecodedValue::Float(f) => seed.deserialize(serde::de::value::F64Deserializer::new(f)),
            _ => unreachable!(),
        }.map_err(|_: serde::de::value::Error| Error::Custom("typed array element deserialization failed".into()))?;
        Ok(Some(value))
    }
}

struct RecordMapDeserializer<'a, 'de> {
    de: &'a mut Deserializer<'de>,
    keys: Vec<String>,
    index: usize,
    serving_key: bool,
}

impl<'a, 'de> RecordMapDeserializer<'a, 'de> {
    fn new(de: &'a mut Deserializer<'de>, keys: Vec<String>) -> Self {
        RecordMapDeserializer { de, keys, index: 0, serving_key: true }
    }
}

impl<'de> MapAccess<'de> for RecordMapDeserializer<'_, 'de> {
    type Error = Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>> {
        // Check if we hit container end (fewer values than keys)
        if self.de.decoder.try_consume_container_end()? {
            return Ok(None);
        }
        if self.index >= self.keys.len() {
            // Consume remaining values + end marker
            self.de.decoder.try_consume_container_end()?;
            return Ok(None);
        }
        let key = &self.keys[self.index];
        self.serving_key = false;
        seed.deserialize(serde::de::value::StrDeserializer::new(key))
            .map(Some)
            .map_err(|_: serde::de::value::Error| Error::Custom("record key deserialization failed".into()))
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value> {
        self.index += 1;
        self.serving_key = true;
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
        // Consume the outer object's end marker
        self.de.decoder.try_consume_container_end()?;
        Ok(value)
    }

    fn tuple_variant<V: Visitor<'de>>(self, _len: usize, visitor: V) -> Result<V::Value> {
        self.de.decoder.expect_array_start()?;
        let seq = SeqDeserializer::new(self.de);
        let value = visitor.visit_seq(seq)?;
        // Consume the outer object's end marker
        self.de.decoder.try_consume_container_end()?;
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
        // Consume the outer object's end marker
        self.de.decoder.try_consume_container_end()?;
        Ok(value)
    }
}
