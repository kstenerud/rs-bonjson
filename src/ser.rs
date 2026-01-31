// ABOUTME: Serde Serializer implementation for BONJSON encoding.
// ABOUTME: Allows any serde-serializable Rust type to be encoded to BONJSON bytes.

use crate::encoder::Encoder;
use crate::error::{Error, Result};
use serde::ser::{self, Serialize};
use std::io::Write;

/// A serde Serializer that writes BONJSON.
pub struct Serializer<'a, W: Write> {
    encoder: &'a mut Encoder<W>,
}

impl<'a, W: Write> Serializer<'a, W> {
    /// Create a new Serializer wrapping an Encoder.
    pub fn new(encoder: &'a mut Encoder<W>) -> Self {
        Self { encoder }
    }
}

impl<W: Write> ser::Serializer for &mut Serializer<'_, W> {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;

    fn serialize_bool(self, v: bool) -> Result<()> {
        self.encoder.write_bool_unchecked(v)
    }

    fn serialize_i8(self, v: i8) -> Result<()> {
        self.encoder.write_i64_unchecked(i64::from(v))
    }

    fn serialize_i16(self, v: i16) -> Result<()> {
        self.encoder.write_i64_unchecked(i64::from(v))
    }

    fn serialize_i32(self, v: i32) -> Result<()> {
        self.encoder.write_i64_unchecked(i64::from(v))
    }

    fn serialize_i64(self, v: i64) -> Result<()> {
        self.encoder.write_i64_unchecked(v)
    }

    fn serialize_u8(self, v: u8) -> Result<()> {
        self.encoder.write_u64_unchecked(u64::from(v))
    }

    fn serialize_u16(self, v: u16) -> Result<()> {
        self.encoder.write_u64_unchecked(u64::from(v))
    }

    fn serialize_u32(self, v: u32) -> Result<()> {
        self.encoder.write_u64_unchecked(u64::from(v))
    }

    fn serialize_u64(self, v: u64) -> Result<()> {
        self.encoder.write_u64_unchecked(v)
    }

    fn serialize_f32(self, v: f32) -> Result<()> {
        self.encoder.write_f32_unchecked(v)
    }

    fn serialize_f64(self, v: f64) -> Result<()> {
        self.encoder.write_f64_unchecked(v)
    }

    fn serialize_char(self, v: char) -> Result<()> {
        let mut buf = [0u8; 4];
        let s = v.encode_utf8(&mut buf);
        self.encoder.write_str_unchecked(s)
    }

    fn serialize_str(self, v: &str) -> Result<()> {
        self.encoder.write_str_unchecked(v)
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<()> {
        self.encoder.begin_array_unchecked()?;
        for &byte in v {
            self.encoder.write_u64_unchecked(u64::from(byte))?;
        }
        self.encoder.end_container_unchecked()
    }

    fn serialize_none(self) -> Result<()> {
        self.encoder.write_null_unchecked()
    }

    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<()> {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<()> {
        self.encoder.write_null_unchecked()
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<()> {
        self.encoder.write_null_unchecked()
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<()> {
        self.encoder.write_str_unchecked(variant)
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<()> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<()> {
        self.encoder.begin_object_unchecked()?;
        self.encoder.write_str_unchecked(variant)?;
        value.serialize(&mut *self)?;
        self.encoder.end_container_unchecked()
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq> {
        self.encoder.begin_array_unchecked()?;
        Ok(self)
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple> {
        self.encoder.begin_array_unchecked()?;
        Ok(self)
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        self.encoder.begin_array_unchecked()?;
        Ok(self)
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        self.encoder.begin_object_unchecked()?;
        self.encoder.write_str_unchecked(variant)?;
        self.encoder.begin_array_unchecked()?;
        Ok(self)
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap> {
        self.encoder.begin_object_unchecked()?;
        Ok(self)
    }

    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self::SerializeStruct> {
        self.encoder.begin_object_unchecked()?;
        Ok(self)
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant> {
        self.encoder.begin_object_unchecked()?;
        self.encoder.write_str_unchecked(variant)?;
        self.encoder.begin_object_unchecked()?;
        Ok(self)
    }
}

impl<W: Write> ser::SerializeSeq for &mut Serializer<'_, W> {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        self.encoder.end_container_unchecked()
    }
}

impl<W: Write> ser::SerializeTuple for &mut Serializer<'_, W> {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        self.encoder.end_container_unchecked()
    }
}

impl<W: Write> ser::SerializeTupleStruct for &mut Serializer<'_, W> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        self.encoder.end_container_unchecked()
    }
}

impl<W: Write> ser::SerializeTupleVariant for &mut Serializer<'_, W> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        // Close the inner array and the outer object
        self.encoder.end_container_unchecked()?;
        self.encoder.end_container_unchecked()
    }
}

impl<W: Write> ser::SerializeMap for &mut Serializer<'_, W> {
    type Ok = ();
    type Error = Error;

    fn serialize_key<T: ?Sized + Serialize>(&mut self, key: &T) -> Result<()> {
        key.serialize(MapKeySerializer { ser: &mut **self })
    }

    fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        self.encoder.end_container_unchecked()
    }
}

impl<W: Write> ser::SerializeStruct for &mut Serializer<'_, W> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<()> {
        self.encoder.write_str_unchecked(key)?;
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        self.encoder.end_container_unchecked()
    }
}

impl<W: Write> ser::SerializeStructVariant for &mut Serializer<'_, W> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<()> {
        self.encoder.write_str_unchecked(key)?;
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        // Close the inner object and the outer object
        self.encoder.end_container_unchecked()?;
        self.encoder.end_container_unchecked()
    }
}

/// A helper serializer for map keys that ensures they are strings.
struct MapKeySerializer<'a, 'b, W: Write> {
    ser: &'a mut Serializer<'b, W>,
}

impl<W: Write> ser::Serializer for MapKeySerializer<'_, '_, W> {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = ser::Impossible<(), Error>;
    type SerializeTuple = ser::Impossible<(), Error>;
    type SerializeTupleStruct = ser::Impossible<(), Error>;
    type SerializeTupleVariant = ser::Impossible<(), Error>;
    type SerializeMap = ser::Impossible<(), Error>;
    type SerializeStruct = ser::Impossible<(), Error>;
    type SerializeStructVariant = ser::Impossible<(), Error>;

    fn serialize_str(self, v: &str) -> Result<()> {
        self.ser.encoder.write_str_unchecked(v)
    }

    fn serialize_i8(self, v: i8) -> Result<()> {
        self.serialize_str(&v.to_string())
    }
    fn serialize_i16(self, v: i16) -> Result<()> {
        self.serialize_str(&v.to_string())
    }
    fn serialize_i32(self, v: i32) -> Result<()> {
        self.serialize_str(&v.to_string())
    }
    fn serialize_i64(self, v: i64) -> Result<()> {
        self.serialize_str(&v.to_string())
    }
    fn serialize_u8(self, v: u8) -> Result<()> {
        self.serialize_str(&v.to_string())
    }
    fn serialize_u16(self, v: u16) -> Result<()> {
        self.serialize_str(&v.to_string())
    }
    fn serialize_u32(self, v: u32) -> Result<()> {
        self.serialize_str(&v.to_string())
    }
    fn serialize_u64(self, v: u64) -> Result<()> {
        self.serialize_str(&v.to_string())
    }

    fn serialize_bool(self, _v: bool) -> Result<()> {
        Err(Error::ExpectedObjectKey)
    }
    fn serialize_f32(self, _v: f32) -> Result<()> {
        Err(Error::ExpectedObjectKey)
    }
    fn serialize_f64(self, _v: f64) -> Result<()> {
        Err(Error::ExpectedObjectKey)
    }
    fn serialize_char(self, v: char) -> Result<()> {
        let mut buf = [0u8; 4];
        let s = v.encode_utf8(&mut buf);
        self.serialize_str(s)
    }
    fn serialize_bytes(self, _v: &[u8]) -> Result<()> {
        Err(Error::ExpectedObjectKey)
    }
    fn serialize_none(self) -> Result<()> {
        Err(Error::ExpectedObjectKey)
    }
    fn serialize_some<T: ?Sized + Serialize>(self, _value: &T) -> Result<()> {
        Err(Error::ExpectedObjectKey)
    }
    fn serialize_unit(self) -> Result<()> {
        Err(Error::ExpectedObjectKey)
    }
    fn serialize_unit_struct(self, _name: &'static str) -> Result<()> {
        Err(Error::ExpectedObjectKey)
    }
    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<()> {
        self.serialize_str(variant)
    }
    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<()> {
        value.serialize(self)
    }
    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _value: &T,
    ) -> Result<()> {
        Err(Error::ExpectedObjectKey)
    }
    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq> {
        Err(Error::ExpectedObjectKey)
    }
    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple> {
        Err(Error::ExpectedObjectKey)
    }
    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        Err(Error::ExpectedObjectKey)
    }
    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        Err(Error::ExpectedObjectKey)
    }
    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap> {
        Err(Error::ExpectedObjectKey)
    }
    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self::SerializeStruct> {
        Err(Error::ExpectedObjectKey)
    }
    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant> {
        Err(Error::ExpectedObjectKey)
    }
}
