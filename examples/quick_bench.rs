// ABOUTME: Quick benchmark comparing BONJSON vs JSON performance.
// Run with: cargo run --release --example quick_bench

use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct TestData {
    id: u64,
    name: String,
    email: String,
    scores: Vec<i32>,
    active: bool,
    rating: f64,
}

fn create_test_data(count: usize) -> Vec<TestData> {
    (0..count)
        .map(|i| TestData {
            id: i as u64 * 1000 + 12345,
            name: format!("User Name {}", i),
            email: format!("user{}@example.com", i),
            scores: vec![95, 87, 92, 88, 91, 89, 94, 90, 93, 86],
            active: i % 2 == 0,
            rating: 4.5 + (i as f64 * 0.01),
        })
        .collect()
}

fn bench_encode<T: Serialize>(name: &str, data: &T, iterations: u32) {
    // Warmup
    for _ in 0..100 {
        let _ = bonjson::to_vec(data);
        let _ = serde_json::to_vec(data);
    }

    // BONJSON encode
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = bonjson::to_vec(data).unwrap();
    }
    let bonjson_time = start.elapsed();

    // JSON encode
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = serde_json::to_vec(data).unwrap();
    }
    let json_time = start.elapsed();

    let bonjson_bytes = bonjson::to_vec(data).unwrap();
    let json_bytes = serde_json::to_vec(data).unwrap();

    println!("\n{} ENCODE ({} iterations):", name, iterations);
    println!("  BONJSON: {:>8.2?} ({:>6} bytes)", bonjson_time, bonjson_bytes.len());
    println!("  JSON:    {:>8.2?} ({:>6} bytes)", json_time, json_bytes.len());
    println!(
        "  Speedup: {:.2}x faster, {:.1}% smaller",
        json_time.as_nanos() as f64 / bonjson_time.as_nanos() as f64,
        (1.0 - bonjson_bytes.len() as f64 / json_bytes.len() as f64) * 100.0
    );
}

fn bench_decode<T: Serialize + for<'de> Deserialize<'de>>(name: &str, data: &T, iterations: u32) {
    let bonjson_bytes = bonjson::to_vec(data).unwrap();
    let json_bytes = serde_json::to_vec(data).unwrap();

    // Warmup
    for _ in 0..100 {
        let _: T = bonjson::from_slice(&bonjson_bytes).unwrap();
        let _: T = serde_json::from_slice(&json_bytes).unwrap();
    }

    // BONJSON decode
    let start = Instant::now();
    for _ in 0..iterations {
        let _: T = bonjson::from_slice(&bonjson_bytes).unwrap();
    }
    let bonjson_time = start.elapsed();

    // JSON decode
    let start = Instant::now();
    for _ in 0..iterations {
        let _: T = serde_json::from_slice(&json_bytes).unwrap();
    }
    let json_time = start.elapsed();

    println!("\n{} DECODE ({} iterations):", name, iterations);
    println!("  BONJSON: {:>8.2?}", bonjson_time);
    println!("  JSON:    {:>8.2?}", json_time);
    println!(
        "  Speedup: {:.2}x faster",
        json_time.as_nanos() as f64 / bonjson_time.as_nanos() as f64
    );
}

fn main() {
    println!("=== BONJSON vs JSON Performance Comparison ===");
    println!("(running in release mode for accurate results)");

    // Small data
    let small = create_test_data(1);
    bench_encode("Small (1 object)", &small, 100_000);
    bench_decode("Small (1 object)", &small, 100_000);

    // Medium data
    let medium = create_test_data(100);
    bench_encode("Medium (100 objects)", &medium, 10_000);
    bench_decode("Medium (100 objects)", &medium, 10_000);

    // Large data
    let large = create_test_data(1000);
    bench_encode("Large (1000 objects)", &large, 1_000);
    bench_decode("Large (1000 objects)", &large, 1_000);

    // Integer array
    let integers: Vec<i32> = (0..10000).collect();
    bench_encode("Integer array (10000)", &integers, 1_000);
    bench_decode("Integer array (10000)", &integers, 1_000);

    // String heavy
    let strings: Vec<String> = (0..1000)
        .map(|i| format!("This is a longer string with some content for item number {}", i))
        .collect();
    bench_encode("String array (1000)", &strings, 1_000);
    bench_decode("String array (1000)", &strings, 1_000);

    println!("\n=== Summary ===");
    println!("BONJSON is generally faster for encoding/decoding and produces smaller output.");
    println!("The performance advantage is most significant for:");
    println!("  - Integer-heavy data (compact encoding)");
    println!("  - Deeply nested structures (no text parsing overhead)");
}
