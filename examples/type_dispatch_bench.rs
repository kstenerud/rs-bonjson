// ABOUTME: Benchmark comparing different type code dispatch strategies.
// ABOUTME: Tests mask-based optimizations vs current cascading if approach.
// Run with: cargo run --release --example type_dispatch_bench

use std::time::Instant;
use std::hint::black_box;

/// Current type code module (baseline)
mod current {
    pub const SMALLINT_MAX: u8 = 0xc8;
    pub const RESERVED_C9: u8 = 0xc9;
    pub const RESERVED_CF: u8 = 0xcf;
    pub const RESERVED_FA: u8 = 0xfa;
    pub const STRING_LONG: u8 = 0xf0;
    pub const BIG_NUMBER: u8 = 0xf1;
    pub const FLOAT16: u8 = 0xf2;
    pub const FLOAT32: u8 = 0xf3;
    pub const FLOAT64: u8 = 0xf4;
    pub const NULL: u8 = 0xf5;
    pub const FALSE: u8 = 0xf6;
    pub const TRUE: u8 = 0xf7;
    pub const ARRAY: u8 = 0xf8;
    pub const OBJECT: u8 = 0xf9;

    #[inline]
    pub const fn is_small_int(code: u8) -> bool {
        code <= SMALLINT_MAX
    }

    #[inline]
    pub const fn is_reserved(code: u8) -> bool {
        (code >= RESERVED_C9 && code <= RESERVED_CF) || code >= RESERVED_FA
    }

    #[inline]
    pub const fn is_unsigned_int(code: u8) -> bool {
        (code & 0xf8) == 0xd0
    }

    #[inline]
    pub const fn is_signed_int(code: u8) -> bool {
        (code & 0xf8) == 0xd8
    }

    #[inline]
    pub const fn is_short_string(code: u8) -> bool {
        (code & 0xf0) == 0xe0
    }

    #[inline]
    pub const fn short_string_len(code: u8) -> usize {
        (code & 0x0f) as usize
    }

    #[inline]
    pub const fn unsigned_int_size(code: u8) -> usize {
        ((code & 0x07) + 1) as usize
    }

    #[inline]
    pub const fn signed_int_size(code: u8) -> usize {
        ((code & 0x07) + 1) as usize
    }
}

/// Optimized dispatch using combined integer check
mod optimized {
    pub const SMALLINT_MAX: u8 = 0xc8;
    pub const STRING_LONG: u8 = 0xf0;
    pub const BIG_NUMBER: u8 = 0xf1;
    pub const FLOAT16: u8 = 0xf2;
    pub const FLOAT32: u8 = 0xf3;
    pub const FLOAT64: u8 = 0xf4;
    pub const NULL: u8 = 0xf5;
    pub const FALSE: u8 = 0xf6;
    pub const TRUE: u8 = 0xf7;
    pub const ARRAY: u8 = 0xf8;
    pub const OBJECT: u8 = 0xf9;

    #[inline]
    pub const fn is_small_int(code: u8) -> bool {
        code <= SMALLINT_MAX
    }

    /// Check if code is any integer (signed or unsigned): 0xd0-0xdf
    #[inline]
    pub const fn is_any_int(code: u8) -> bool {
        (code & 0xf0) == 0xd0
    }

    /// Check if integer is signed (bit 3 set): 0xd8-0xdf
    #[inline]
    pub const fn is_signed(code: u8) -> bool {
        (code & 0x08) != 0
    }

    /// Get integer byte size from type code (works for both signed and unsigned)
    #[inline]
    pub const fn int_size(code: u8) -> usize {
        ((code & 0x07) + 1) as usize
    }

    #[inline]
    pub const fn is_short_string(code: u8) -> bool {
        (code & 0xf0) == 0xe0
    }

    #[inline]
    pub const fn short_string_len(code: u8) -> usize {
        (code & 0x0f) as usize
    }

    /// Check if code is in reserved range (0xc9-0xcf or 0xfa-0xff)
    #[inline]
    pub const fn is_reserved(code: u8) -> bool {
        // 0xc9-0xcf: code > 0xc8 && code < 0xd0
        // 0xfa-0xff: code > 0xf9
        (code > 0xc8 && code < 0xd0) || code > 0xf9
    }

    /// Check if code is in the high range (0xf0-0xf9) where we use match
    #[inline]
    pub const fn is_high_code(code: u8) -> bool {
        code >= 0xf0 && code <= 0xf9
    }
}

/// Lookup table based dispatch for 0xf0-0xff codes
mod lookup {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[repr(u8)]
    pub enum HighCode {
        StringLong = 0,
        BigNumber = 1,
        Float16 = 2,
        Float32 = 3,
        Float64 = 4,
        Null = 5,
        False = 6,
        True = 7,
        Array = 8,
        Object = 9,
        Reserved = 10,
    }

    /// Lookup table for codes 0xf0-0xff (16 entries)
    /// Index = code - 0xf0
    static HIGH_CODE_TABLE: [HighCode; 16] = [
        HighCode::StringLong, // 0xf0
        HighCode::BigNumber,  // 0xf1
        HighCode::Float16,    // 0xf2
        HighCode::Float32,    // 0xf3
        HighCode::Float64,    // 0xf4
        HighCode::Null,       // 0xf5
        HighCode::False,      // 0xf6
        HighCode::True,       // 0xf7
        HighCode::Array,      // 0xf8
        HighCode::Object,     // 0xf9
        HighCode::Reserved,   // 0xfa
        HighCode::Reserved,   // 0xfb
        HighCode::Reserved,   // 0xfc
        HighCode::Reserved,   // 0xfd
        HighCode::Reserved,   // 0xfe
        HighCode::Reserved,   // 0xff
    ];

    #[inline]
    pub fn lookup_high_code(code: u8) -> HighCode {
        HIGH_CODE_TABLE[(code & 0x0f) as usize]
    }
}

/// Simulated decoded value type (simplified)
#[derive(Debug, Clone, Copy, PartialEq)]
enum ValueType {
    SmallInt(i8),
    UnsignedInt(usize), // size in bytes
    SignedInt(usize),   // size in bytes
    ShortString(usize), // length
    LongString,
    BigNumber,
    Float16,
    Float32,
    Float64,
    Null,
    False,
    True,
    Array,
    Object,
    Reserved,
}

/// Current dispatch strategy (baseline)
#[inline(never)]
fn dispatch_current(code: u8) -> ValueType {
    if current::is_small_int(code) {
        return ValueType::SmallInt((code as i16 - 100) as i8);
    }

    if current::is_reserved(code) {
        return ValueType::Reserved;
    }

    if current::is_unsigned_int(code) {
        return ValueType::UnsignedInt(current::unsigned_int_size(code));
    }

    if current::is_signed_int(code) {
        return ValueType::SignedInt(current::signed_int_size(code));
    }

    if current::is_short_string(code) {
        return ValueType::ShortString(current::short_string_len(code));
    }

    match code {
        current::STRING_LONG => ValueType::LongString,
        current::BIG_NUMBER => ValueType::BigNumber,
        current::FLOAT16 => ValueType::Float16,
        current::FLOAT32 => ValueType::Float32,
        current::FLOAT64 => ValueType::Float64,
        current::NULL => ValueType::Null,
        current::FALSE => ValueType::False,
        current::TRUE => ValueType::True,
        current::ARRAY => ValueType::Array,
        current::OBJECT => ValueType::Object,
        _ => ValueType::Reserved,
    }
}

/// Optimized dispatch with combined integer check
#[inline(never)]
fn dispatch_combined_int(code: u8) -> ValueType {
    if optimized::is_small_int(code) {
        return ValueType::SmallInt((code as i16 - 100) as i8);
    }

    // Combined integer check: 0xd0-0xdf
    if optimized::is_any_int(code) {
        let size = optimized::int_size(code);
        return if optimized::is_signed(code) {
            ValueType::SignedInt(size)
        } else {
            ValueType::UnsignedInt(size)
        };
    }

    if optimized::is_short_string(code) {
        return ValueType::ShortString(optimized::short_string_len(code));
    }

    if optimized::is_high_code(code) {
        match code {
            optimized::STRING_LONG => ValueType::LongString,
            optimized::BIG_NUMBER => ValueType::BigNumber,
            optimized::FLOAT16 => ValueType::Float16,
            optimized::FLOAT32 => ValueType::Float32,
            optimized::FLOAT64 => ValueType::Float64,
            optimized::NULL => ValueType::Null,
            optimized::FALSE => ValueType::False,
            optimized::TRUE => ValueType::True,
            optimized::ARRAY => ValueType::Array,
            optimized::OBJECT => ValueType::Object,
            _ => ValueType::Reserved,
        }
    } else {
        ValueType::Reserved
    }
}

/// Optimized dispatch with lookup table for high codes
#[inline(never)]
fn dispatch_lookup_table(code: u8) -> ValueType {
    if optimized::is_small_int(code) {
        return ValueType::SmallInt((code as i16 - 100) as i8);
    }

    // Combined integer check: 0xd0-0xdf
    if optimized::is_any_int(code) {
        let size = optimized::int_size(code);
        return if optimized::is_signed(code) {
            ValueType::SignedInt(size)
        } else {
            ValueType::UnsignedInt(size)
        };
    }

    if optimized::is_short_string(code) {
        return ValueType::ShortString(optimized::short_string_len(code));
    }

    // Use lookup table for 0xf0-0xff
    if code >= 0xf0 {
        match lookup::lookup_high_code(code) {
            lookup::HighCode::StringLong => ValueType::LongString,
            lookup::HighCode::BigNumber => ValueType::BigNumber,
            lookup::HighCode::Float16 => ValueType::Float16,
            lookup::HighCode::Float32 => ValueType::Float32,
            lookup::HighCode::Float64 => ValueType::Float64,
            lookup::HighCode::Null => ValueType::Null,
            lookup::HighCode::False => ValueType::False,
            lookup::HighCode::True => ValueType::True,
            lookup::HighCode::Array => ValueType::Array,
            lookup::HighCode::Object => ValueType::Object,
            lookup::HighCode::Reserved => ValueType::Reserved,
        }
    } else {
        ValueType::Reserved
    }
}

/// Full lookup table dispatch (256-entry table)
mod full_lookup {
    use super::ValueType;

    #[derive(Debug, Clone, Copy)]
    pub struct TypeInfo {
        pub value_type: u8,  // Discriminant
        pub payload: i16,    // Size, length, or small int value
    }

    const SMALL_INT: u8 = 0;
    const UNSIGNED_INT: u8 = 1;
    const SIGNED_INT: u8 = 2;
    const SHORT_STRING: u8 = 3;
    const LONG_STRING: u8 = 4;
    const BIG_NUMBER: u8 = 5;
    const FLOAT16: u8 = 6;
    const FLOAT32: u8 = 7;
    const FLOAT64: u8 = 8;
    const NULL: u8 = 9;
    const FALSE: u8 = 10;
    const TRUE: u8 = 11;
    const ARRAY: u8 = 12;
    const OBJECT: u8 = 13;
    const RESERVED: u8 = 14;

    const fn make_table() -> [TypeInfo; 256] {
        let mut table = [TypeInfo { value_type: RESERVED, payload: 0 }; 256];
        let mut i: usize = 0;

        // Small integers: 0x00-0xc8
        while i <= 0xc8 {
            table[i] = TypeInfo {
                value_type: SMALL_INT,
                payload: i as i16 - 100,
            };
            i += 1;
        }

        // Reserved: 0xc9-0xcf (already set to RESERVED)
        i = 0xd0;

        // Unsigned integers: 0xd0-0xd7
        while i <= 0xd7 {
            table[i] = TypeInfo {
                value_type: UNSIGNED_INT,
                payload: ((i & 0x07) + 1) as i16,
            };
            i += 1;
        }

        // Signed integers: 0xd8-0xdf
        while i <= 0xdf {
            table[i] = TypeInfo {
                value_type: SIGNED_INT,
                payload: ((i & 0x07) + 1) as i16,
            };
            i += 1;
        }

        // Short strings: 0xe0-0xef
        while i <= 0xef {
            table[i] = TypeInfo {
                value_type: SHORT_STRING,
                payload: (i & 0x0f) as i16,
            };
            i += 1;
        }

        // High codes: 0xf0-0xf9
        table[0xf0] = TypeInfo { value_type: LONG_STRING, payload: 0 };
        table[0xf1] = TypeInfo { value_type: BIG_NUMBER, payload: 0 };
        table[0xf2] = TypeInfo { value_type: FLOAT16, payload: 0 };
        table[0xf3] = TypeInfo { value_type: FLOAT32, payload: 0 };
        table[0xf4] = TypeInfo { value_type: FLOAT64, payload: 0 };
        table[0xf5] = TypeInfo { value_type: NULL, payload: 0 };
        table[0xf6] = TypeInfo { value_type: FALSE, payload: 0 };
        table[0xf7] = TypeInfo { value_type: TRUE, payload: 0 };
        table[0xf8] = TypeInfo { value_type: ARRAY, payload: 0 };
        table[0xf9] = TypeInfo { value_type: OBJECT, payload: 0 };

        // 0xfa-0xff already set to RESERVED

        table
    }

    static TYPE_TABLE: [TypeInfo; 256] = make_table();

    #[inline]
    pub fn lookup(code: u8) -> ValueType {
        let info = TYPE_TABLE[code as usize];
        match info.value_type {
            SMALL_INT => ValueType::SmallInt(info.payload as i8),
            UNSIGNED_INT => ValueType::UnsignedInt(info.payload as usize),
            SIGNED_INT => ValueType::SignedInt(info.payload as usize),
            SHORT_STRING => ValueType::ShortString(info.payload as usize),
            LONG_STRING => ValueType::LongString,
            BIG_NUMBER => ValueType::BigNumber,
            FLOAT16 => ValueType::Float16,
            FLOAT32 => ValueType::Float32,
            FLOAT64 => ValueType::Float64,
            NULL => ValueType::Null,
            FALSE => ValueType::False,
            TRUE => ValueType::True,
            ARRAY => ValueType::Array,
            OBJECT => ValueType::Object,
            _ => ValueType::Reserved,
        }
    }
}

#[inline(never)]
fn dispatch_full_lookup(code: u8) -> ValueType {
    full_lookup::lookup(code)
}

/// Generate test codes with realistic distribution
fn generate_test_codes(count: usize) -> Vec<u8> {
    let mut codes = Vec::with_capacity(count);

    // Realistic distribution based on typical JSON:
    // - ~30% small integers (field values, array indices)
    // - ~35% short strings (field names, short values)
    // - ~10% longer integers
    // - ~10% containers (array/object)
    // - ~10% booleans/null
    // - ~5% floats/other

    for i in 0..count {
        let code = match i % 100 {
            // Small integers: 30%
            0..=29 => {
                let val = (i % 201) as u8; // 0x00-0xc8
                if val <= 0xc8 { val } else { 0x64 } // default to 0
            }

            // Short strings: 35%
            30..=64 => {
                let len = i % 16;
                0xe0 + len as u8
            }

            // Signed/unsigned integers: 10%
            65..=74 => {
                let size = i % 8;
                if i % 2 == 0 {
                    0xd0 + size as u8 // unsigned
                } else {
                    0xd8 + size as u8 // signed
                }
            }

            // Containers: 10%
            75..=84 => {
                if i % 2 == 0 { 0xf8 } else { 0xf9 }
            }

            // Booleans/null: 10%
            85..=94 => {
                match i % 3 {
                    0 => 0xf5, // null
                    1 => 0xf6, // false
                    _ => 0xf7, // true
                }
            }

            // Floats/other: 5%
            _ => {
                match i % 4 {
                    0 => 0xf2, // float16
                    1 => 0xf3, // float32
                    2 => 0xf4, // float64
                    _ => 0xf0, // long string
                }
            }
        };
        codes.push(code);
    }

    codes
}

/// Generate codes weighted toward small integers (common case)
fn generate_small_int_heavy(count: usize) -> Vec<u8> {
    (0..count).map(|i| (i % 201) as u8).collect()
}

/// Generate codes weighted toward strings
fn generate_string_heavy(count: usize) -> Vec<u8> {
    (0..count).map(|i| {
        if i % 10 < 8 {
            0xe0 + (i % 16) as u8 // short string
        } else {
            0xf0 // long string
        }
    }).collect()
}

/// Generate codes weighted toward integers
fn generate_int_heavy(count: usize) -> Vec<u8> {
    (0..count).map(|i| {
        if i % 3 == 0 {
            (i % 201) as u8 // small int
        } else if i % 2 == 0 {
            0xd0 + (i % 8) as u8 // unsigned
        } else {
            0xd8 + (i % 8) as u8 // signed
        }
    }).collect()
}

fn bench_dispatch<F>(name: &str, codes: &[u8], iterations: u32, f: F)
where
    F: Fn(u8) -> ValueType,
{
    // Warmup
    for _ in 0..1000 {
        for &code in codes.iter().take(100) {
            black_box(f(code));
        }
    }

    let start = Instant::now();
    for _ in 0..iterations {
        for &code in codes {
            black_box(f(code));
        }
    }
    let elapsed = start.elapsed();

    let total_ops = codes.len() as u64 * iterations as u64;
    let ns_per_op = elapsed.as_nanos() as f64 / total_ops as f64;

    println!("{:<30} {:>10.2?}  {:>6.2} ns/op", name, elapsed, ns_per_op);
}

fn run_benchmark_suite(suite_name: &str, codes: &[u8], iterations: u32) {
    println!("\n{}", "=".repeat(60));
    println!("{} ({} codes, {} iterations)", suite_name, codes.len(), iterations);
    println!("{}", "=".repeat(60));

    bench_dispatch("Current (cascading ifs)", codes, iterations, dispatch_current);
    bench_dispatch("Combined int check", codes, iterations, dispatch_combined_int);
    bench_dispatch("Combined + lookup table", codes, iterations, dispatch_lookup_table);
    bench_dispatch("Full 256-entry lookup", codes, iterations, dispatch_full_lookup);
}

fn main() {
    println!("Type Code Dispatch Benchmark");
    println!("============================");
    println!();
    println!("Comparing dispatch strategies:");
    println!("1. Current: cascading if checks");
    println!("2. Combined int: single mask for all integers");
    println!("3. Combined + lookup: lookup table for 0xf0-0xff");
    println!("4. Full lookup: 256-entry lookup table");

    // Verify correctness first
    println!("\nVerifying correctness...");
    for code in 0..=255u8 {
        let current = dispatch_current(code);
        let combined = dispatch_combined_int(code);
        let lookup = dispatch_lookup_table(code);
        let full = dispatch_full_lookup(code);

        assert_eq!(current, combined, "Mismatch for code 0x{:02x}: current={:?}, combined={:?}", code, current, combined);
        assert_eq!(current, lookup, "Mismatch for code 0x{:02x}: current={:?}, lookup={:?}", code, current, lookup);
        assert_eq!(current, full, "Mismatch for code 0x{:02x}: current={:?}, full={:?}", code, current, full);
    }
    println!("All dispatch strategies produce identical results!");

    // Run benchmarks
    let realistic = generate_test_codes(10000);
    run_benchmark_suite("Realistic distribution", &realistic, 1000);

    let small_int_heavy = generate_small_int_heavy(10000);
    run_benchmark_suite("Small integer heavy", &small_int_heavy, 1000);

    let string_heavy = generate_string_heavy(10000);
    run_benchmark_suite("String heavy", &string_heavy, 1000);

    let int_heavy = generate_int_heavy(10000);
    run_benchmark_suite("Integer heavy (all sizes)", &int_heavy, 1000);

    // All codes
    let all_codes: Vec<u8> = (0..=255).collect();
    run_benchmark_suite("All 256 codes (uniform)", &all_codes, 50000);

    println!("\n{}", "=".repeat(60));
    println!("Summary");
    println!("{}", "=".repeat(60));
    println!("Lower ns/op is better.");
    println!();
}
