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
- Delimiter-terminated containers (FC/FD start, FE end)
- FF-terminated long strings (FF + payload + FF)

### decoder.rs
- `Decoder<'a>` - zero-copy decoder that borrows from input slice
- `DecoderConfig` for configurable limits and options
- `DuplicateKeyMode` - Error, KeepFirst, or KeepLast
- Optional SIMD-accelerated UTF-8 validation via `simd-utf8` feature
- Returns `DecodedValue<'a>` enum for streaming access
- BigNumber decoding: zigzag LEB128 exponent + zigzag LEB128 signed_length + raw LE magnitude bytes with normalization validation
- Direct decode methods for serde path avoid `DecodedValue` intermediary

### value.rs
- `Value` enum - dynamic value type similar to `serde_json::Value`
- Variants: Null, Bool, Int(i64), UInt(u64), Float(f64), BigNumber, String, Array, Object
- `bonjson!` macro for JSON-like value literals
- Accessor methods (as_str, as_i64, get_key, get_index, etc.)

### ser.rs
- `Serializer<'a, W>` - serde Serializer implementation
- Wraps the low-level `Encoder`
- Handles all serde data model types

### de.rs
- `Deserializer<'a>` - serde Deserializer implementation
- Wraps the low-level `Decoder`
- Zero-copy string deserialization when possible

### lib.rs
- Public API functions: `to_vec`, `to_writer`, `from_slice`, `from_slice_with_config`
- Value-based API: `encode_value`, `decode_value`, `decode_value_with_config`
- Recursive value decoding with duplicate key detection and container size limits
- Re-exports commonly used types

## Key Design Decisions

### Type Code Layout
| Range | Meaning |
|-------|---------|
| 00–C8 | Small integers (-100 to 100), value = code - 100 |
| C9 | Reserved |
| CA | BigNumber (zigzag LEB128 exponent + signed_length + LE magnitude) |
| CB | float32 (IEEE 754, little-endian) |
| CC | float64 (IEEE 754, little-endian) |
| CD | null |
| CE | false |
| CF | true |
| D0–DF | Short string (0–15 bytes, length = code & 0x0F) |
| E0–E3 | Unsigned integers (uint8, uint16, uint32, uint64) |
| E4–E7 | Signed integers (int8, int16, int32, int64) |
| E8–FB | Reserved |
| FC | Array start |
| FD | Object start |
| FE | Container end |
| FF | Long string start/terminator |

### Performance Optimizations

#### Encoder (ser.rs, encoder.rs)
- `to_vec()` pre-allocates 128 bytes to reduce reallocations for small-to-medium payloads
- Uses unchecked write methods for serde path (bounds already validated)
- Combined type code + payload write for floats (single `write_all` call)
- Inline hints on hot paths

#### Decoder (de.rs, decoder.rs)
- Zero-copy string decoding for single-segment strings
- Direct decode methods (`decode_i64_direct()`, `decode_str_direct()`, etc.) return primitives directly instead of going through `DecodedValue` enum
- `try_consume_container_end()` checks for 0xFE delimiter and pops container state
- Serde path uses unchecked methods that skip container state tracking
- Branchless sign extension for signed integer decoding (arithmetic right shift)
- Uses `memchr` for NUL byte detection in both short and long strings (decoder and encoder)
- Inline hints on hot paths

#### Type Code Dispatch (types.rs)
The type code layout enables efficient mask-based dispatch:
- `0x00-0xc8`: Small integers (value = code - 100)
- `0xd0-0xdf`: Short strings — `(code & 0xf0) == 0xd0`, length from `(code & 0x0f)`
- `0xe0-0xe7`: Integers — `(code & 0xf8) == 0xe0`, sign bit at `(code & 0x04)`, size index from `(code & 0x03)`

Key optimization: Combined integer check `is_any_int()` tests all integers (signed and unsigned)
with a single mask operation, then determines sign with `int_is_signed()`.

### Compliance Levels
- **Basic compliance**: UTF-8 validation, NUL character rejection, duplicate key detection (byte-for-byte comparison)
- **Secure compliance**: Same as basic plus NFC normalization for duplicate key detection (not yet implemented)
- Optional features: NaN/Infinity handling, duplicate key keep_first/keep_last modes

### Known Limitations
- BigNumber significands limited to i64 range
- NaN/Infinity stringify mode not implemented (NaN/Infinity rejection returns `invalid_data` error type per spec)
- Unicode normalization (NFC) not implemented
- Out-of-range BigNumber stringify mode not implemented
- Invalid UTF-8 replace/delete modes not implemented

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
