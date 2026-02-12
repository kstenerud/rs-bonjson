# CLAUDE.md - serde_bonjson

## Overview

This is `serde_bonjson`, a Rust implementation of BONJSON (Binary Object Notation for JSON). It's designed as a drop-in replacement for `serde_json` — users can migrate by prepending "bon" to "json" in their imports:

- `serde_json` → `serde_bonjson`
- `json!` → `bonjson!`
- `serde_json::Value` → `serde_bonjson::Value`

BONJSON is a binary format that is 1:1 compatible with JSON but more efficient to process. The codec follows the BONJSON specification at `specification/bonjson.md`.

## Architecture

The project uses a layered architecture:

```
┌─────────────────────────────────────────────┐
│               Public API (lib.rs)           │
│  to_vec, from_slice, encode_value, etc.     │
├─────────────────────────────────────────────┤
│         Serde Integration                   │
│    ser.rs (Serializer)  de.rs (Deserializer)│
├─────────────────────────────────────────────┤
│       Core Encoder/Decoder                  │
│    encoder.rs           decoder.rs          │
├─────────────────────────────────────────────┤
│         Support Types                       │
│  types.rs  error.rs  value.rs               │
└─────────────────────────────────────────────┘
```

## Source Files

### types.rs
- Type codes as defined by the BONJSON spec
- `BigNumber` struct for arbitrary precision decimals (sign × magnitude × 10^exponent)
- Helper functions for encoding/decoding type codes using mask-based dispatch
- Zigzag and LEB128 encoding/decoding helpers for BigNumber metadata
- Resource limits (max depth, max container size, etc.)

### error.rs
- `Error` enum with variants mapping to spec-defined error types
- Each variant has an `error_type()` method returning the standardized name
- Implements `serde::de::Error` and `serde::ser::Error` for serde integration

### encoder.rs
- `Encoder<W: Write>` - streaming binary encoder
- Supports all BONJSON types: small ints, sized ints (u8-u64, i8-i64), float32, float64, BigNumber
- Automatically chooses smallest encoding for integers and floats
- Validates floats (rejects NaN/Infinity by default)
- BigNumber encoding: zigzag LEB128 exponent + zigzag LEB128 signed_length + raw LE magnitude bytes
- Delimiter-terminated containers (B7/B8 start, B6 end)
- Short strings up to 66 bytes inline, FF-terminated long strings (FF + payload + FF)
- Methods: `write_record_definition()`, `begin_record_instance()`, `write_typed_array_raw()`
- Encoding-size helpers: `signed_int_encoding_size()`, `unsigned_int_encoding_size()`, `float_encoding_size()` — compute encoded size without writing, used by serde typed array size comparison

### decoder.rs
- `Decoder<'a>` - zero-copy decoder that borrows from input slice
- `DecoderConfig` for configurable limits and options
- `DuplicateKeyMode` - Error, KeepFirst, or KeepLast
- `NanInfinityMode` - Reject, Allow, or Stringify
- `OutOfRangeMode` - Error or Stringify (for BigNumber limit violations)
- `InvalidUtf8Mode` - Reject, Replace, or Delete
- `UnicodeNormalization` - None or Nfc (requires `unicode-normalization` feature)
- Optional SIMD-accelerated UTF-8 validation via `simd-utf8` feature
- `DecodedValue<'a>` enum uses `Cow<'a, str>` for strings (zero-copy in default mode)
- Returns `DecodedValue<'a>` enum for streaming access (includes `RecordInstanceStart`, `TypedArrayStart`)
- BigNumber decoding: zigzag LEB128 exponent + zigzag LEB128 signed_length + raw LE magnitude bytes with normalization validation
- Direct decode methods for serde path avoid `DecodedValue` intermediary
- Tracks `record_definitions` field for record instance expansion
- Methods: `read_record_definitions()`, `read_typed_array_element()`, `end_typed_array()`

### value.rs
- `Value` enum - dynamic value type similar to `serde_json::Value`
- Variants: Null, Bool, Int(i64), UInt(u64), Float(f64), BigNumber, String, Array, Object
- `bonjson!` macro for JSON-like value literals
- Accessor methods (as_str, as_i64, get_key, get_index, etc.)

### ser.rs
- `Serializer<'a, W>` - serde Serializer implementation wrapping the low-level `Encoder`
- `SerializerConfig` with `typed_arrays` (default: true) and `records` (default: false)
- `BufferedSeqSerializer` — probes sequences for typed array optimization:
  - Buffers elements, tracking element kind and raw LE bytes
  - At `end()`, compares typed array size vs regular array size, emits smaller one
  - Falls back to regular streaming on type mismatch or non-numeric elements
  - Uses `SeqElementSerializer` to capture individual elements without writing
  - `NoOpCompound` absorbs compound-type children during probing
- `StructSerializer` enum — `Regular` (key+value) or `Record` (value only, keys from definition)
- `CountingSerializer` — no-output first pass for record detection, counts struct name occurrences
- `serialize_bytes` emits `TYPED_ARRAY_UINT8` instead of regular array
- Tuples always use regular arrays (heterogeneous by nature)

### de.rs
- `Deserializer<'a>` - serde Deserializer implementation
- Wraps the low-level `Decoder`
- Zero-copy string deserialization when possible
- `deserialize_struct` handles both OBJECT and RECORD_INSTANCE transparently

### lib.rs
- Public API: `to_vec`, `to_writer`, `to_vec_with_config`, `to_writer_with_config`
- `to_writer_with_config` implements two-pass record detection when `config.records` is true:
  1. Run `CountingSerializer` → collect struct types appearing 2+ times
  2. Write record definitions via encoder, then serialize with record instances
- Deserialization: `from_slice`, `from_slice_with_config`
- Value-based API: `encode_value`, `decode_value`, `decode_value_with_config`
- Recursive value decoding with duplicate key detection and container size limits
- Re-exports commonly used types including `SerializerConfig`

## Key Design Decisions

### Type Code Layout
| Range | Meaning |
|-------|---------|
| 00–64 | Small integers (0 to 100), value = code |
| 65–A7 | Short string (0–66 bytes, length = code - 0x65) |
| A8–AB | Unsigned integers (uint8, uint16, uint32, uint64) |
| AC–AF | Signed integers (int8, int16, int32, int64) |
| B0 | float32 (IEEE 754, little-endian) |
| B1 | float64 (IEEE 754, little-endian) |
| B2 | BigNumber (zigzag LEB128 exponent + signed_length + LE magnitude) |
| B3 | null |
| B4 | false |
| B5 | true |
| B6 | Container end |
| B7 | Array start |
| B8 | Object start |
| B9 | Record definition (string keys + 0xB6) |
| BA | Record instance (LEB128 def_index + values + 0xB6) |
| BB–F4 | Reserved |
| F5–FE | Typed arrays (float64, float32, sint64..uint8) |
| FF | Long string start/terminator |

### Record Types
Record definitions (`0xB9`) define key-set templates before the root value. Record instances (`0xBA`) reference a definition by LEB128 index. During encoding (Value API), the encoder performs a two-pass scan: collect key sets that appear 2+ times, emit definitions, then encode objects matching those key sets as record instances. The serde path also supports records when `SerializerConfig::records` is true — it uses `CountingSerializer` for a lightweight first pass to count struct types, then emits definitions and instances for types appearing 2+ times. During decoding, record instances are transparently expanded into objects (both `deserialize_any` and `deserialize_struct` handle them).

### Typed Arrays
Typed arrays (`0xF5-0xFE`) are length-prefixed homogeneous numeric arrays. 10 element types: float64, float32, sint64/32/16/8, uint64/32/16/8. During encoding (Value API), the encoder auto-detects homogeneous numeric `Value::Array`s and emits typed arrays. The serde path also supports typed arrays by default (`SerializerConfig::typed_arrays`): `BufferedSeqSerializer` probes sequence elements, buffers raw LE bytes, and at `end()` compares typed vs regular size to emit the smaller encoding. `serialize_bytes` always emits `TYPED_ARRAY_UINT8`. During decoding, typed arrays are transparently expanded into individual values.

### Performance Optimizations

#### Encoder (ser.rs, encoder.rs)
- `to_vec()` pre-allocates 128 bytes to reduce reallocations for small-to-medium payloads
- Uses unchecked write methods for serde path (bounds already validated)
- Combined type code + payload write for floats (single `write_all` call)
- Inline hints on hot paths

#### Decoder (de.rs, decoder.rs)
- Zero-copy string decoding for single-segment strings
- Direct decode methods (`decode_i64_direct()`, `decode_str_direct()`, etc.) return primitives directly instead of going through `DecodedValue` enum
- `try_consume_container_end()` checks for 0xB6 delimiter and pops container state
- Serde path uses unchecked methods that skip container state tracking
- Branchless sign extension for signed integer decoding (arithmetic right shift)
- Uses `memchr` for NUL byte detection in both short and long strings (decoder and encoder)
- Inline hints on hot paths

#### Type Code Dispatch (types.rs)
The type code layout enables efficient mask-based dispatch:
- `0x00-0x64`: Small integers (value = code)
- `0x65-0xa7`: Short strings — range-based dispatch, length from `(code - 0x65)`, up to 66 bytes
- `0xa8-0xaf`: Integers — `(code & 0xf8) == 0xa8`, sign from `code >= 0xac`, size from `code & 0x03`

Key optimization: Combined integer check `is_any_int()` tests all integers (signed and unsigned)
with a single mask operation, then determines sign with `int_is_signed()`.

### Compliance Levels
- **Basic compliance**: UTF-8 validation, NUL character rejection, duplicate key detection (byte-for-byte comparison)
- **Secure compliance**: Same as basic plus NFC normalization for duplicate key detection (requires `unicode-normalization` feature)
- Optional features: NaN/Infinity handling (allow/stringify), duplicate key keep_first/keep_last modes, invalid UTF-8 replace/delete, out-of-range BigNumber stringify

### Known Limitations
- BigNumber significands limited to i64 range

### Performance Considerations
- For file/network I/O, wrap writers in `BufWriter` - the encoder writes small chunks
  (often single bytes) directly to the writer
- `to_vec` pre-allocates 128 bytes; for large payloads, use `to_writer` with a pre-sized Vec

## Optional Features

### `simd-utf8`
Enables SIMD-accelerated UTF-8 validation using the `simdutf8` crate. Benchmarks show:
- Large strings (400+ bytes): ~5-10% faster decoding
- Unicode-heavy content: ~30% faster decoding
- Small ASCII strings: No significant change

Enable with: `cargo build --features simd-utf8`

The implementation uses `simdutf8::basic::from_utf8()` which leverages SSE2/AVX2 (x86) or
NEON (ARM) instructions when available.

### `unicode-normalization`
Enables NFC Unicode normalization for string values and duplicate key detection.
When `UnicodeNormalization::Nfc` is configured in `DecoderConfig`, all decoded strings
and object keys are NFC-normalized. This enables secure compliance-level duplicate key
detection where differently-encoded Unicode strings that render identically are treated
as duplicates.

Enable with: `cargo build --features unicode-normalization`

Zero overhead when not configured (normalization is off by default).

## Testing

### Unit Tests
Each module has embedded `#[cfg(test)]` tests covering basic functionality.

### Conformance Tests
`tests/conformance.rs` runs the universal BONJSON test suite from `specification/tests/`.

The test runner implements the BONJSON universal test specification format with:
- Version validation (semver format)
- Test name validation (pattern and uniqueness)
- Required field validation per test type
- Option and error type validation
- Config file support (`bonjson-test-config` type)
- $number marker parsing (NaN, Infinity, hex floats)
- Value comparison (NaN=NaN, -0.0≠0.0)

Test categories:
- **Conformance tests** (`specification/tests/conformance/`): Validate the codec
- **Test runner validation** (`specification/tests/test-runner-validation/`): Validate the test runner itself

Run all tests:
```bash
cargo test
```

Run conformance tests with output:
```bash
cargo test test_conformance -- --nocapture
```

Run test runner validation:
```bash
cargo test test_runner_validation -- --nocapture
```

## Commands

```bash
cargo build         # Build the library
cargo test          # Run all tests
cargo doc --open    # Generate and view documentation
cargo clippy        # Run linter
```

## Adding New Features

1. Check if the feature exists in the spec (`specification/bonjson.md`)
2. Add any new error variants to `error.rs`
3. Update encoder/decoder as needed
4. Add tests to the relevant module
5. Run conformance tests to verify compatibility
