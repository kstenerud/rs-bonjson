# serde_bonjson

A Rust implementation of [BONJSON](https://github.com/kstenerud/bonjson) (Binary Object Notation for JSON) — a binary encoding that's **1:1 compatible with JSON's data model** but faster and more compact.

**If you're using `serde_json`, switching to `serde_bonjson` is a one-line change.**

## Why Switch to BONJSON?

| Benefit                     | Description                                                     |
|-----------------------------|-----------------------------------------------------------------|
| **2-3x faster encoding**    | Binary format avoids string formatting overhead                 |
| **1.5-2x faster decoding**  | No text parsing, direct binary reads                            |
| **25-50% smaller payloads** | Integers encode in 1-9 bytes instead of ASCII digits            |
| **Drop-in replacement**     | Same API as `serde_json` — just change the import               |
| **Same data types**         | BONJSON supports the same data types as JSON. No more, no less. |
| **Full serde support**      | Works with any type that implements `Serialize`/`Deserialize`   |

## Migrating from serde_json

### Zero-Change Migration

For the smoothest migration, alias the crate and use the `json!` macro:

```rust
use serde_bonjson as serde_json;
use serde_json::json;  // json! is an alias for bonjson!

// Your existing code works unchanged!
let value = json!({ "name": "Alice", "age": 30 });
let bytes = serde_json::to_vec(&value)?;
let decoded: serde_json::Value = serde_json::from_slice(&bytes)?;
```

### Standard Migration

Or update your imports explicitly — the API mirrors `serde_json`:

```rust
// Before (serde_json)                          // After (serde_bonjson)
use serde_json;                                 use serde_bonjson;

let bytes = serde_json::to_vec(&data)?;         let bytes = serde_bonjson::to_vec(&data)?;
let data = serde_json::from_slice(&b)?;         let data = serde_bonjson::from_slice(&b)?;

serde_json::json!({ "key": value })             serde_bonjson::bonjson!({ "key": value })
serde_json::Value                               serde_bonjson::Value
```

Either way, your existing serde derives work unchanged.

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
serde_bonjson = "0.1"
```

### Serializing and Deserializing Structs

```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
struct Person {
    name: String,
    age: u32,
}

// Serialize to BONJSON bytes
let person = Person { name: "Alice".into(), age: 30 };
let bytes = serde_bonjson::to_vec(&person).unwrap();

// Deserialize from BONJSON bytes
let decoded: Person = serde_bonjson::from_slice(&bytes).unwrap();
```

### Working with Dynamic Values

When you don't know the structure at compile time, use `Value`:

```rust
use serde_bonjson::{Value, bonjson};

// Build values with the bonjson! macro (just like json!)
let value = bonjson!({
    "name": "Bob",
    "scores": [95, 87, 92],
    "active": true
});

// Access fields dynamically
if let Some(name) = value.get_key("name").and_then(|v| v.as_str()) {
    println!("Name: {}", name);
}

// Encode/decode Value types
let bytes = serde_bonjson::encode_value(&value).unwrap();
let decoded = serde_bonjson::decode_value(&bytes).unwrap();
```

### Writing to Files or Streams

```rust
use std::fs::File;
use std::io::BufWriter;

let file = File::create("data.bonjson")?;
// Use BufWriter for better performance with files/network
let writer = BufWriter::new(file);
serde_bonjson::to_writer(writer, &data)?;
```

## Performance

Benchmarks comparing BONJSON vs JSON (using `serde_json`):

| Workload | Encode Speedup | Decode Speedup | Size Reduction |
|----------|---------------|----------------|----------------|
| Simple struct | 2.0x | 1.9x | ~40% |
| Complex nested data | 2.0x | 1.5x | ~35% |
| Integer arrays | 2.9x | 2.5x | ~50% |
| String-heavy data | 1.3x* | 1.2x* | ~10% |

*With default settings. See "Performance Tuning" below for string-heavy workloads.

Run the benchmarks yourself:

```bash
cargo bench
```

### Performance Tuning for Trusted Data

By default, BONJSON validates that strings don't contain NUL bytes (per the spec). For trusted data where this check isn't needed:

```rust
use serde_bonjson::DecoderConfig;

let mut config = DecoderConfig::default();
config.allow_nul = true;

let data: MyStruct = serde_bonjson::from_slice_with_config(&bytes, config)?;
```

This improves string decoding by 20-60% depending on string length.

## API Reference

### Core Functions

| Function | Description |
|----------|-------------|
| `to_vec(&T)` | Serialize to a new `Vec<u8>` |
| `to_writer(W, &T)` | Serialize to any `Write` implementation |
| `from_slice(&[u8])` | Deserialize from bytes |
| `from_reader(R)` | Deserialize from any `Read` implementation |
| `from_slice_with_config(&[u8], config)` | Deserialize with custom limits |
| `from_reader_with_config(R, config)` | Deserialize from reader with custom limits |

### Value Functions

| Function | Description |
|----------|-------------|
| `to_value(&T)` | Convert any serializable type to `Value` |
| `from_value(&Value)` | Convert `Value` to any deserializable type |
| `encode_value(&Value)` | Encode a `Value` to bytes |
| `decode_value(&[u8])` | Decode bytes to a `Value` |
| `bonjson!({ ... })` | Macro to construct `Value` literals |
| `json!({ ... })` | Alias for `bonjson!` (for serde_json compatibility) |

### Types

| Type | Description |
|------|-------------|
| `Value` | Dynamic value type (like `serde_json::Value`) |
| `Map<K, V>` | Type alias for object maps (like `serde_json::Map`) |
| `Error` | Error type for all operations |
| `Result<T>` | Result type alias |

### Configuration

```rust
use serde_bonjson::{DecoderConfig, DuplicateKeyMode};

let config = DecoderConfig {
    // Validation options
    allow_nul: false,              // Allow NUL (0x00) in strings
    allow_nan_infinity: false,     // Allow NaN/Infinity floats
    allow_trailing_bytes: false,   // Allow extra bytes after document
    duplicate_key_mode: DuplicateKeyMode::Error,

    // Resource limits (defaults per BONJSON spec)
    max_depth: 512,
    max_container_size: 1_000_000,
    max_string_length: 10_000_000,
    max_document_size: 2_000_000_000,
    max_chunks: 100,
};
```

## When to Use BONJSON vs JSON

**Use BONJSON when:**
- Performance matters (APIs, databases, caching)
- Bandwidth is limited (mobile, IoT, real-time systems)
- You control both endpoints (internal services)

**Stick with JSON when:**
- Human readability is required (config files, logs)
- Interoperating with systems that only support JSON
- Debugging is more important than performance

## License

MIT License - see [LICENSE](LICENSE) for details.
