# CLAUDE.md - BONJSON Rust Codec

## Overview

This is a Rust implementation of BONJSON (Binary Object Notation for JSON), a binary format that is 1:1 compatible with JSON but more efficient to process. The codec follows the BONJSON specification at `../bonjson/bonjson.md`.

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
- Type codes (0x00-0xff) as defined by the BONJSON spec
- `BigNumber` struct for arbitrary precision decimals (significand × 10^exponent)
- Helper functions for encoding/decoding type codes
- Resource limits (max depth, max container size, etc.)

### error.rs
- `Error` enum with variants mapping to spec-defined error types
- Each variant has an `error_type()` method returning the standardized name
- Implements `serde::de::Error` and `serde::ser::Error` for serde integration

### encoder.rs
- `Encoder<W: Write>` - streaming binary encoder
- Uses compiler intrinsics (`trailing_zeros()`) for efficient length field encoding
- Supports all BONJSON types including bfloat16, float32, float64
- Automatically chooses smallest encoding for integers and floats
- Validates floats (rejects NaN/Infinity by default)
- Key optimization: length fields use trailing-1-bits encoding for compactness

### decoder.rs
- `Decoder<'a>` - zero-copy decoder that borrows from input slice
- `DecoderConfig` for configurable limits and options
- `DuplicateKeyMode` - Error, KeepFirst, or KeepLast
- Uses compiler intrinsics (`trailing_zeros()`) for efficient length field decoding
- Validates UTF-8 per chunk (catches split multi-byte characters)
- Returns `DecodedValue<'a>` enum for streaming access

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
- Recursive value decoding with duplicate key detection
- Re-exports commonly used types

## Key Design Decisions

### Performance Optimizations

#### Encoder (ser.rs, encoder.rs)
- Length field encoding uses trailing-1-bits pattern, encoded with `trailing_zeros()` intrinsic
- bfloat16 used when float values can be exactly represented in fewer bytes
- Pre-allocates output buffer based on input estimate
- Uses unchecked write methods for serde path (bounds already validated)
- Inline hints on hot paths

#### Decoder (de.rs, decoder.rs)
- Zero-copy string decoding for single-chunk strings
- Direct decode methods (`decode_i64_direct()`, `decode_str_direct()`, etc.) return primitives directly instead of going through `DecodedValue` enum - avoids intermediate allocation and match overhead
- `try_consume_container_end()` combines bounds check, container-end check, and position increment into one operation
- Short ASCII strings (≤32 bytes) skip UTF-8 validation since ASCII is always valid UTF-8
- Length field decoding uses `trailing_zeros()` intrinsic
- Serde path uses unchecked methods that skip container state tracking
- Inline hints on hot paths

### Compliance Levels
- **Basic compliance**: UTF-8 validation per chunk, NUL character rejection
- **Standard compliance**: Duplicate key detection with Error mode
- Optional features: NaN/Infinity handling, duplicate key keep_first/keep_last modes

### Known Limitations
- BigNumber significands limited to 8 bytes (u64)
- Multi-chunk strings use `Box::leak` to return borrowed references from owned data.
  This only affects strings from external BONJSON sources that use chunking for streaming;
  the encoder in this crate always produces single-chunk strings, so in a closed system
  this code path is never executed.
- NaN/Infinity stringify mode not implemented
- Invalid UTF-8 replace/delete modes not implemented

### Performance Considerations
- For file/network I/O, wrap writers in `BufWriter` - the encoder writes small chunks
  (often single bytes) directly to the writer
- `to_vec` pre-allocates 128 bytes; for large payloads, use `to_writer` with a pre-sized Vec

## Testing

### Unit Tests
Each module has embedded `#[cfg(test)]` tests covering basic functionality.

### Conformance Tests
`tests/conformance.rs` runs the universal BONJSON test suite from `../bonjson/tests/conformance/`.

Run all tests:
```bash
cargo test
```

Run conformance tests with output:
```bash
cargo test test_conformance -- --nocapture
```

## Commands

```bash
cargo build         # Build the library
cargo test          # Run all tests
cargo doc --open    # Generate and view documentation
cargo clippy        # Run linter
```

## Adding New Features

1. Check if the feature exists in the spec (`../bonjson/bonjson.md`)
2. Add any new error variants to `error.rs`
3. Update encoder/decoder as needed
4. Add tests to the relevant module
5. Run conformance tests to verify compatibility
