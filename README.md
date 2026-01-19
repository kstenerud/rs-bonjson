# bonjson

A Rust implementation of [BONJSON](https://github.com/kstenerud/bonjson) (Binary Object Notation for JSON), a binary format that is 1:1 compatible with JSON but more compact and faster to process.

## Features

- Full serde integration for seamless serialization/deserialization
- Zero-copy deserialization for strings
- Compact encoding (typically 25-80% smaller than JSON)
- Faster than serde_json for most workloads
- Configurable validation levels for performance tuning

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
bonjson = "0.1"
```

### Basic Usage

```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
struct Person {
    name: String,
    age: u32,
}

fn main() {
    let person = Person {
        name: "Alice".to_string(),
        age: 30,
    };

    // Encode to BONJSON
    let bytes = bonjson::to_vec(&person).unwrap();

    // Decode from BONJSON
    let decoded: Person = bonjson::from_slice(&bytes).unwrap();

    println!("{:?}", decoded);
}
```

### Working with Dynamic Values

```rust
use bonjson::{Value, bonjson};

// Create values with the bonjson! macro
let value = bonjson!({
    "name": "Bob",
    "scores": [95, 87, 92]
});

// Encode/decode dynamic values
let bytes = bonjson::encode_value(&value).unwrap();
let decoded = bonjson::decode_value(&bytes).unwrap();
```

## Performance

BONJSON typically outperforms JSON for both encoding and decoding:

| Operation | Speedup vs JSON |
|-----------|-----------------|
| Encoding structured data | 2-3x faster |
| Encoding integers | 2-3x faster |
| Encoding long strings | 5-9x faster |
| Decoding structured data | 1.4-1.7x faster |
| Decoding integers | 2-3x faster |
| Decoding booleans | 7x faster |

Run the benchmark yourself:

```bash
cargo run --release --example quick_bench
```

### Performance Tuning for Trusted Data

By default, BONJSON validates that strings don't contain NUL (0x00) bytes, as required by the specification. JSON parsers don't perform this check, which gives them a slight advantage on string-heavy workloads in the default configuration.

For trusted data sources where you know NUL bytes won't be present, you can skip this validation. This makes BONJSON faster than JSON even for string-heavy data:

```rust
use bonjson::decoder::DecoderConfig;

let mut config = DecoderConfig::default();
config.allow_nul = true;

let data: MyStruct = bonjson::from_slice_with_config(&bytes, config)?;
```

**Performance impact of `allow_nul: true`:**

| String Length | Speedup Improvement |
|---------------|---------------------|
| Short (<16 bytes) | ~0% (negligible) |
| Medium (~50 bytes) | ~20% faster |
| Long (~500 bytes) | ~30% faster |
| Very long (~10KB) | ~60% faster |

This is particularly beneficial for workloads with many medium-to-large strings.

## Configuration Options

The `DecoderConfig` struct provides several options:

```rust
use bonjson::decoder::{DecoderConfig, DuplicateKeyMode};

let config = DecoderConfig {
    allow_nul: false,              // Allow NUL bytes in strings
    allow_nan_infinity: false,     // Allow NaN/Infinity float values
    allow_trailing_bytes: false,   // Allow extra bytes after document
    duplicate_key_mode: DuplicateKeyMode::Error,  // How to handle duplicate keys
    max_depth: 1000,               // Maximum nesting depth
    max_container_size: 1_000_000, // Maximum elements per container
    max_string_length: 100_000_000, // Maximum string length
    max_chunks: 1000,              // Maximum chunks per chunked string
    max_document_size: 1_000_000_000, // Maximum document size
};
```

## License

See LICENSE file for details.
