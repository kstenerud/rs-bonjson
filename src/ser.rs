// ABOUTME: Serde Serializer implementation for BONJSON encoding.
// ABOUTME: Supports typed arrays (buffered sequences) and records (two-pass struct optimization).

use crate::encoder::{self, Encoder};
use crate::error::{Error, Result};
use crate::types::type_code;
use serde::ser::{self, Serialize};
use std::collections::HashMap;
use std::io::Write;

/// Configuration for the serde serializer.
#[derive(Debug, Clone)]
pub struct SerializerConfig {
    /// Emit typed arrays for homogeneous numeric sequences (default: true).
    /// When enabled, sequences where all elements use the same numeric serde
    /// method are encoded as typed arrays if doing so produces a smaller encoding.
    pub typed_arrays: bool,
    /// Emit record definitions/instances for repeated struct types (default: false).
    /// When enabled, requires a two-pass traversal: the first pass counts struct
    /// types, the second pass emits record definitions for structs appearing 2+ times.
    pub records: bool,
}

impl Default for SerializerConfig {
    fn default() -> Self {
        Self {
            typed_arrays: true,
            records: false,
        }
    }
}

/// A serde Serializer that writes BONJSON.
pub struct Serializer<'a, W: Write> {
    encoder: &'a mut Encoder<W>,
    config: SerializerConfig,
    /// Record definitions for the serde path: struct_name → (keys, def_index).
    /// Populated by the two-pass record detection when `config.records` is true.
    record_defs: Option<HashMap<&'static str, (Vec<&'static str>, usize)>>,
}

impl<'a, W: Write> Serializer<'a, W> {
    /// Create a new Serializer wrapping an Encoder.
    pub fn new(encoder: &'a mut Encoder<W>) -> Self {
        Self {
            encoder,
            config: SerializerConfig::default(),
            record_defs: None,
        }
    }

    /// Create a new Serializer with custom configuration.
    pub fn with_config(
        encoder: &'a mut Encoder<W>,
        config: SerializerConfig,
        record_defs: Option<HashMap<&'static str, (Vec<&'static str>, usize)>>,
    ) -> Self {
        Self {
            encoder,
            config,
            record_defs,
        }
    }
}

impl<'a, 'b, W: Write> ser::Serializer for &'a mut Serializer<'b, W> {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = BufferedSeqSerializer<'a, 'b, W>;
    type SerializeTuple = &'a mut Serializer<'b, W>;
    type SerializeTupleStruct = &'a mut Serializer<'b, W>;
    type SerializeTupleVariant = &'a mut Serializer<'b, W>;
    type SerializeMap = &'a mut Serializer<'b, W>;
    type SerializeStruct = StructSerializer<'a, 'b, W>;
    type SerializeStructVariant = &'a mut Serializer<'b, W>;

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
        // Always emit as typed uint8 array — equal or better than regular array
        self.encoder
            .write_typed_array_raw_unchecked(type_code::TYPED_ARRAY_UINT8, v.len(), v)
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

    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq> {
        if self.config.typed_arrays {
            Ok(BufferedSeqSerializer::new_probing(self, len))
        } else {
            self.encoder.begin_array_unchecked()?;
            Ok(BufferedSeqSerializer::new_regular(self))
        }
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

    fn serialize_struct(
        self,
        name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStruct> {
        // Check if this struct has a record definition
        if let Some(ref defs) = self.record_defs {
            if let Some((_, def_index)) = defs.get(name) {
                self.encoder.begin_record_instance_unchecked(*def_index)?;
                return Ok(StructSerializer::Record(self));
            }
        }
        self.encoder.begin_object_unchecked()?;
        Ok(StructSerializer::Regular(self))
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

// =============================================================================
// BufferedSeqSerializer — typed array probing for sequences
// =============================================================================

/// The element type detected during probing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ElementKind {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
}

impl ElementKind {
    /// The typed array type code for this element kind.
    fn typed_array_code(self) -> u8 {
        match self {
            ElementKind::I8 => type_code::TYPED_ARRAY_SINT8,
            ElementKind::I16 => type_code::TYPED_ARRAY_SINT16,
            ElementKind::I32 => type_code::TYPED_ARRAY_SINT32,
            ElementKind::I64 => type_code::TYPED_ARRAY_SINT64,
            ElementKind::U8 => type_code::TYPED_ARRAY_UINT8,
            ElementKind::U16 => type_code::TYPED_ARRAY_UINT16,
            ElementKind::U32 => type_code::TYPED_ARRAY_UINT32,
            ElementKind::U64 => type_code::TYPED_ARRAY_UINT64,
            ElementKind::F32 => type_code::TYPED_ARRAY_FLOAT32,
            ElementKind::F64 => type_code::TYPED_ARRAY_FLOAT64,
        }
    }

    /// Size of each element in bytes.
    fn element_size(self) -> usize {
        match self {
            ElementKind::I8 | ElementKind::U8 => 1,
            ElementKind::I16 | ElementKind::U16 => 2,
            ElementKind::I32 | ElementKind::U32 | ElementKind::F32 => 4,
            ElementKind::I64 | ElementKind::U64 | ElementKind::F64 => 8,
        }
    }
}

enum SeqMode {
    /// Buffering elements, checking if all are the same numeric type.
    Probing {
        kind: Option<ElementKind>,
        /// Raw LE bytes of buffered elements.
        data: Vec<u8>,
        count: usize,
        /// Accumulated regular encoding size (sum of each element's optimal size).
        regular_size: usize,
    },
    /// Fell back to streaming regular array.
    Regular,
}

/// Serializer for sequences that probes for typed array optimization.
pub struct BufferedSeqSerializer<'a, 'b, W: Write> {
    ser: &'a mut Serializer<'b, W>,
    mode: SeqMode,
}

impl<'a, 'b, W: Write> BufferedSeqSerializer<'a, 'b, W> {
    fn new_probing(ser: &'a mut Serializer<'b, W>, len: Option<usize>) -> Self {
        let capacity = len.unwrap_or(0);
        Self {
            ser,
            mode: SeqMode::Probing {
                kind: None,
                data: Vec::with_capacity(capacity * 4), // reasonable estimate
                count: 0,
                regular_size: 0,
            },
        }
    }

    fn new_regular(ser: &'a mut Serializer<'b, W>) -> Self {
        Self {
            ser,
            mode: SeqMode::Regular,
        }
    }

    /// Flush buffered probing data as a regular array.
    fn flush_as_regular(&mut self) -> Result<()> {
        if let SeqMode::Probing {
            kind,
            ref data,
            count,
            ..
        } = self.mode
        {
            self.ser.encoder.begin_array_unchecked()?;
            if count > 0 {
                if let Some(k) = kind {
                    write_buffered_elements(self.ser.encoder, k, data, count)?;
                }
            }
        }
        self.mode = SeqMode::Regular;
        Ok(())
    }
}

/// Write buffered element data back as regular (non-typed-array) values.
fn write_buffered_elements<W: Write>(
    encoder: &mut Encoder<W>,
    kind: ElementKind,
    data: &[u8],
    count: usize,
) -> Result<()> {
    let elem_size = kind.element_size();
    for i in 0..count {
        let offset = i * elem_size;
        let chunk = &data[offset..offset + elem_size];
        match kind {
            ElementKind::I8 => {
                encoder.write_i64_unchecked(i64::from(chunk[0] as i8))?;
            }
            ElementKind::I16 => {
                let v = i16::from_le_bytes([chunk[0], chunk[1]]);
                encoder.write_i64_unchecked(i64::from(v))?;
            }
            ElementKind::I32 => {
                let v = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                encoder.write_i64_unchecked(i64::from(v))?;
            }
            ElementKind::I64 => {
                let v = i64::from_le_bytes(chunk.try_into().unwrap());
                encoder.write_i64_unchecked(v)?;
            }
            ElementKind::U8 => {
                encoder.write_u64_unchecked(u64::from(chunk[0]))?;
            }
            ElementKind::U16 => {
                let v = u16::from_le_bytes([chunk[0], chunk[1]]);
                encoder.write_u64_unchecked(u64::from(v))?;
            }
            ElementKind::U32 => {
                let v = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                encoder.write_u64_unchecked(u64::from(v))?;
            }
            ElementKind::U64 => {
                let v = u64::from_le_bytes(chunk.try_into().unwrap());
                encoder.write_u64_unchecked(v)?;
            }
            ElementKind::F32 => {
                let v = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                encoder.write_f32_unchecked(v)?;
            }
            ElementKind::F64 => {
                let v = f64::from_le_bytes(chunk.try_into().unwrap());
                encoder.write_f64_unchecked(v)?;
            }
        }
    }
    Ok(())
}

/// Compute the LEB128 encoded size of a value.
fn leb128_size(value: u64) -> usize {
    if value == 0 {
        return 1;
    }
    let bits = 64 - value.leading_zeros() as usize;
    (bits + 6) / 7
}

impl<W: Write> ser::SerializeSeq for BufferedSeqSerializer<'_, '_, W> {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        match self.mode {
            SeqMode::Regular => {
                value.serialize(&mut *self.ser)
            }
            SeqMode::Probing { .. } => {
                // Use the element serializer to capture the element
                let mut elem_ser = SeqElementSerializer {
                    result: None,
                };
                value.serialize(&mut elem_ser)?;

                match elem_ser.result {
                    Some((new_kind, raw_bytes, regular_elem_size)) => {
                        // Temporarily take ownership of probing state
                        let (kind, count) = if let SeqMode::Probing { kind, count, .. } = &self.mode {
                            (*kind, *count)
                        } else {
                            unreachable!()
                        };

                        if count == 0 {
                            // First element establishes the type
                            if let SeqMode::Probing {
                                kind: ref mut k,
                                ref mut data,
                                count: ref mut c,
                                regular_size: ref mut rs,
                            } = self.mode
                            {
                                *k = Some(new_kind);
                                data.extend_from_slice(&raw_bytes);
                                *c = 1;
                                *rs = regular_elem_size;
                            }
                        } else if kind == Some(new_kind) {
                            // Same type, keep buffering
                            if let SeqMode::Probing {
                                ref mut data,
                                count: ref mut c,
                                regular_size: ref mut rs,
                                ..
                            } = self.mode
                            {
                                data.extend_from_slice(&raw_bytes);
                                *c += 1;
                                *rs += regular_elem_size;
                            }
                        } else {
                            // Type mismatch — flush and fall back
                            self.flush_as_regular()?;
                            // Now re-serialize this element in regular mode
                            value.serialize(&mut *self.ser)?;
                        }
                    }
                    None => {
                        // Non-numeric element — flush and fall back
                        self.flush_as_regular()?;
                        value.serialize(&mut *self.ser)?;
                    }
                }
                Ok(())
            }
        }
    }

    fn end(self) -> Result<()> {
        match self.mode {
            SeqMode::Regular => {
                self.ser.encoder.end_container_unchecked()
            }
            SeqMode::Probing {
                kind,
                ref data,
                count,
                regular_size,
            } => {
                if count == 0 || kind.is_none() {
                    // Empty sequence or no elements buffered — emit empty regular array
                    self.ser.encoder.begin_array_unchecked()?;
                    return self.ser.encoder.end_container_unchecked();
                }

                let k = kind.unwrap();

                // Compute typed array size:
                // type_code (1) + LEB128(count) + count * elem_size
                let typed_size =
                    1 + leb128_size(count as u64) + count * k.element_size();

                // Compute regular array size:
                // ARRAY marker (1) + sum of element sizes + CONTAINER_END (1)
                let regular_total = 1 + regular_size + 1;

                if typed_size < regular_total {
                    // Emit typed array
                    self.ser.encoder.write_typed_array_raw_unchecked(
                        k.typed_array_code(),
                        count,
                        data,
                    )
                } else {
                    // Regular is smaller or equal — emit regular array
                    self.ser.encoder.begin_array_unchecked()?;
                    write_buffered_elements(self.ser.encoder, k, data, count)?;
                    self.ser.encoder.end_container_unchecked()
                }
            }
        }
    }
}

/// Internal serializer used to capture individual sequence elements during probing.
/// It only accepts numeric types and records their raw LE bytes + regular encoding size.
struct SeqElementSerializer {
    /// (ElementKind, raw LE bytes, regular encoding size)
    result: Option<(ElementKind, Vec<u8>, usize)>,
}

impl ser::Serializer for &mut SeqElementSerializer {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = NoOpCompound;
    type SerializeTuple = NoOpCompound;
    type SerializeTupleStruct = NoOpCompound;
    type SerializeTupleVariant = NoOpCompound;
    type SerializeMap = NoOpCompound;
    type SerializeStruct = NoOpCompound;
    type SerializeStructVariant = NoOpCompound;

    fn serialize_i8(self, v: i8) -> Result<()> {
        self.result = Some((
            ElementKind::I8,
            vec![v as u8],
            encoder::signed_int_encoding_size(i64::from(v)),
        ));
        Ok(())
    }

    fn serialize_i16(self, v: i16) -> Result<()> {
        self.result = Some((
            ElementKind::I16,
            v.to_le_bytes().to_vec(),
            encoder::signed_int_encoding_size(i64::from(v)),
        ));
        Ok(())
    }

    fn serialize_i32(self, v: i32) -> Result<()> {
        self.result = Some((
            ElementKind::I32,
            v.to_le_bytes().to_vec(),
            encoder::signed_int_encoding_size(i64::from(v)),
        ));
        Ok(())
    }

    fn serialize_i64(self, v: i64) -> Result<()> {
        self.result = Some((
            ElementKind::I64,
            v.to_le_bytes().to_vec(),
            encoder::signed_int_encoding_size(v),
        ));
        Ok(())
    }

    fn serialize_u8(self, v: u8) -> Result<()> {
        self.result = Some((
            ElementKind::U8,
            vec![v],
            encoder::unsigned_int_encoding_size(u64::from(v)),
        ));
        Ok(())
    }

    fn serialize_u16(self, v: u16) -> Result<()> {
        self.result = Some((
            ElementKind::U16,
            v.to_le_bytes().to_vec(),
            encoder::unsigned_int_encoding_size(u64::from(v)),
        ));
        Ok(())
    }

    fn serialize_u32(self, v: u32) -> Result<()> {
        self.result = Some((
            ElementKind::U32,
            v.to_le_bytes().to_vec(),
            encoder::unsigned_int_encoding_size(u64::from(v)),
        ));
        Ok(())
    }

    fn serialize_u64(self, v: u64) -> Result<()> {
        self.result = Some((
            ElementKind::U64,
            v.to_le_bytes().to_vec(),
            encoder::unsigned_int_encoding_size(v),
        ));
        Ok(())
    }

    fn serialize_f32(self, v: f32) -> Result<()> {
        self.result = Some((
            ElementKind::F32,
            v.to_le_bytes().to_vec(),
            encoder::float_encoding_size(f64::from(v)),
        ));
        Ok(())
    }

    fn serialize_f64(self, v: f64) -> Result<()> {
        self.result = Some((
            ElementKind::F64,
            v.to_le_bytes().to_vec(),
            encoder::float_encoding_size(v),
        ));
        Ok(())
    }

    // All non-numeric types signal fallback by leaving result as None
    fn serialize_bool(self, _v: bool) -> Result<()> { Ok(()) }
    fn serialize_char(self, _v: char) -> Result<()> { Ok(()) }
    fn serialize_str(self, _v: &str) -> Result<()> { Ok(()) }
    fn serialize_bytes(self, _v: &[u8]) -> Result<()> { Ok(()) }
    fn serialize_none(self) -> Result<()> { Ok(()) }
    fn serialize_some<T: ?Sized + Serialize>(self, _value: &T) -> Result<()> { Ok(()) }
    fn serialize_unit(self) -> Result<()> { Ok(()) }
    fn serialize_unit_struct(self, _name: &'static str) -> Result<()> { Ok(()) }
    fn serialize_unit_variant(self, _name: &'static str, _variant_index: u32, _variant: &'static str) -> Result<()> { Ok(()) }
    fn serialize_newtype_struct<T: ?Sized + Serialize>(self, _name: &'static str, _value: &T) -> Result<()> { Ok(()) }
    fn serialize_newtype_variant<T: ?Sized + Serialize>(self, _name: &'static str, _variant_index: u32, _variant: &'static str, _value: &T) -> Result<()> { Ok(()) }
    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq> { Ok(NoOpCompound) }
    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple> { Ok(NoOpCompound) }
    fn serialize_tuple_struct(self, _name: &'static str, _len: usize) -> Result<Self::SerializeTupleStruct> { Ok(NoOpCompound) }
    fn serialize_tuple_variant(self, _name: &'static str, _variant_index: u32, _variant: &'static str, _len: usize) -> Result<Self::SerializeTupleVariant> { Ok(NoOpCompound) }
    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap> { Ok(NoOpCompound) }
    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self::SerializeStruct> { Ok(NoOpCompound) }
    fn serialize_struct_variant(self, _name: &'static str, _variant_index: u32, _variant: &'static str, _len: usize) -> Result<Self::SerializeStructVariant> { Ok(NoOpCompound) }
}

/// A no-op compound serializer that absorbs all calls without doing anything.
/// Used by `SeqElementSerializer` to drain non-numeric compound elements.
struct NoOpCompound;

impl ser::SerializeSeq for NoOpCompound {
    type Ok = ();
    type Error = Error;
    fn serialize_element<T: ?Sized + Serialize>(&mut self, _value: &T) -> Result<()> { Ok(()) }
    fn end(self) -> Result<()> { Ok(()) }
}

impl ser::SerializeTuple for NoOpCompound {
    type Ok = ();
    type Error = Error;
    fn serialize_element<T: ?Sized + Serialize>(&mut self, _value: &T) -> Result<()> { Ok(()) }
    fn end(self) -> Result<()> { Ok(()) }
}

impl ser::SerializeTupleStruct for NoOpCompound {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, _value: &T) -> Result<()> { Ok(()) }
    fn end(self) -> Result<()> { Ok(()) }
}

impl ser::SerializeTupleVariant for NoOpCompound {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, _value: &T) -> Result<()> { Ok(()) }
    fn end(self) -> Result<()> { Ok(()) }
}

impl ser::SerializeMap for NoOpCompound {
    type Ok = ();
    type Error = Error;
    fn serialize_key<T: ?Sized + Serialize>(&mut self, _key: &T) -> Result<()> { Ok(()) }
    fn serialize_value<T: ?Sized + Serialize>(&mut self, _value: &T) -> Result<()> { Ok(()) }
    fn end(self) -> Result<()> { Ok(()) }
}

impl ser::SerializeStruct for NoOpCompound {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, _key: &'static str, _value: &T) -> Result<()> { Ok(()) }
    fn end(self) -> Result<()> { Ok(()) }
}

impl ser::SerializeStructVariant for NoOpCompound {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, _key: &'static str, _value: &T) -> Result<()> { Ok(()) }
    fn end(self) -> Result<()> { Ok(()) }
}

// =============================================================================
// StructSerializer — supports both regular and record instance modes
// =============================================================================

/// Serializer for struct fields that can emit as a regular object or a record instance.
pub enum StructSerializer<'a, 'b, W: Write> {
    /// Regular object: emit key + value for each field.
    Regular(&'a mut Serializer<'b, W>),
    /// Record instance: only emit values (keys come from the definition).
    Record(&'a mut Serializer<'b, W>),
}

impl<W: Write> ser::SerializeStruct for StructSerializer<'_, '_, W> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<()> {
        match self {
            StructSerializer::Regular(ser) => {
                ser.encoder.write_str_unchecked(key)?;
                value.serialize(&mut **ser)
            }
            StructSerializer::Record(ser) => {
                // Skip key — definition provides it
                value.serialize(&mut **ser)
            }
        }
    }

    fn end(self) -> Result<()> {
        match self {
            StructSerializer::Regular(ser) => ser.encoder.end_container_unchecked(),
            StructSerializer::Record(ser) => ser.encoder.end_container_unchecked(),
        }
    }
}

// =============================================================================
// Tuple and Map impls — unchanged, just use named lifetimes
// =============================================================================

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

// =============================================================================
// MapKeySerializer — ensures map keys are strings
// =============================================================================

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

// =============================================================================
// CountingSerializer — first pass for record detection
// =============================================================================

/// A no-output serializer that counts struct type occurrences for record detection.
/// After serialization, `struct_counts` contains struct_name → (keys, count).
#[derive(Default)]
pub struct CountingSerializer {
    pub struct_counts: HashMap<&'static str, (Vec<&'static str>, usize)>,
}

impl CountingSerializer {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<'a> ser::Serializer for &'a mut CountingSerializer {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = CountingSeq<'a>;
    type SerializeTuple = CountingSeq<'a>;
    type SerializeTupleStruct = CountingSeq<'a>;
    type SerializeTupleVariant = CountingSeq<'a>;
    type SerializeMap = CountingSeq<'a>;
    type SerializeStruct = CountingStruct<'a>;
    type SerializeStructVariant = CountingStruct<'a>;

    fn serialize_bool(self, _v: bool) -> Result<()> { Ok(()) }
    fn serialize_i8(self, _v: i8) -> Result<()> { Ok(()) }
    fn serialize_i16(self, _v: i16) -> Result<()> { Ok(()) }
    fn serialize_i32(self, _v: i32) -> Result<()> { Ok(()) }
    fn serialize_i64(self, _v: i64) -> Result<()> { Ok(()) }
    fn serialize_u8(self, _v: u8) -> Result<()> { Ok(()) }
    fn serialize_u16(self, _v: u16) -> Result<()> { Ok(()) }
    fn serialize_u32(self, _v: u32) -> Result<()> { Ok(()) }
    fn serialize_u64(self, _v: u64) -> Result<()> { Ok(()) }
    fn serialize_f32(self, _v: f32) -> Result<()> { Ok(()) }
    fn serialize_f64(self, _v: f64) -> Result<()> { Ok(()) }
    fn serialize_char(self, _v: char) -> Result<()> { Ok(()) }
    fn serialize_str(self, _v: &str) -> Result<()> { Ok(()) }
    fn serialize_bytes(self, _v: &[u8]) -> Result<()> { Ok(()) }
    fn serialize_none(self) -> Result<()> { Ok(()) }
    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<()> { value.serialize(self) }
    fn serialize_unit(self) -> Result<()> { Ok(()) }
    fn serialize_unit_struct(self, _name: &'static str) -> Result<()> { Ok(()) }
    fn serialize_unit_variant(self, _name: &'static str, _variant_index: u32, _variant: &'static str) -> Result<()> { Ok(()) }
    fn serialize_newtype_struct<T: ?Sized + Serialize>(self, _name: &'static str, value: &T) -> Result<()> { value.serialize(self) }
    fn serialize_newtype_variant<T: ?Sized + Serialize>(self, _name: &'static str, _variant_index: u32, _variant: &'static str, value: &T) -> Result<()> { value.serialize(self) }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq> {
        Ok(CountingSeq(self))
    }
    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple> {
        Ok(CountingSeq(self))
    }
    fn serialize_tuple_struct(self, _name: &'static str, _len: usize) -> Result<Self::SerializeTupleStruct> {
        Ok(CountingSeq(self))
    }
    fn serialize_tuple_variant(self, _name: &'static str, _variant_index: u32, _variant: &'static str, _len: usize) -> Result<Self::SerializeTupleVariant> {
        Ok(CountingSeq(self))
    }
    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap> {
        Ok(CountingSeq(self))
    }
    fn serialize_struct(self, name: &'static str, _len: usize) -> Result<Self::SerializeStruct> {
        Ok(CountingStruct {
            counter: self,
            name,
            keys: Vec::new(),
        })
    }
    fn serialize_struct_variant(self, _name: &'static str, _variant_index: u32, _variant: &'static str, _len: usize) -> Result<Self::SerializeStructVariant> {
        // Struct variants are wrapped in an object — we don't track them as records
        // but we still need to visit children to find nested structs
        Ok(CountingStruct {
            counter: self,
            name: "",
            keys: Vec::new(),
        })
    }
}

/// Visits children of sequences/maps to find nested structs.
pub struct CountingSeq<'a>(&'a mut CountingSerializer);

impl ser::SerializeSeq for CountingSeq<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut *self.0)
    }
    fn end(self) -> Result<()> { Ok(()) }
}

impl ser::SerializeTuple for CountingSeq<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut *self.0)
    }
    fn end(self) -> Result<()> { Ok(()) }
}

impl ser::SerializeTupleStruct for CountingSeq<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut *self.0)
    }
    fn end(self) -> Result<()> { Ok(()) }
}

impl ser::SerializeTupleVariant for CountingSeq<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut *self.0)
    }
    fn end(self) -> Result<()> { Ok(()) }
}

impl ser::SerializeMap for CountingSeq<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_key<T: ?Sized + Serialize>(&mut self, _key: &T) -> Result<()> { Ok(()) }
    fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut *self.0)
    }
    fn end(self) -> Result<()> { Ok(()) }
}

/// Visits struct fields, collecting keys and counting occurrences.
pub struct CountingStruct<'a> {
    counter: &'a mut CountingSerializer,
    name: &'static str,
    keys: Vec<&'static str>,
}

impl ser::SerializeStruct for CountingStruct<'_> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<()> {
        self.keys.push(key);
        value.serialize(&mut *self.counter)
    }

    fn end(self) -> Result<()> {
        if !self.name.is_empty() {
            let entry = self.counter.struct_counts.entry(self.name);
            entry
                .and_modify(|(_, count)| *count += 1)
                .or_insert((self.keys, 1));
        }
        Ok(())
    }
}

impl ser::SerializeStructVariant for CountingStruct<'_> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        _key: &'static str,
        value: &T,
    ) -> Result<()> {
        value.serialize(&mut *self.counter)
    }

    fn end(self) -> Result<()> { Ok(()) }
}
