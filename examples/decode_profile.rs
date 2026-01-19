// Quick decode profiling benchmark
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

fn main() {
    let data = vec![TestData {
        id: 12345,
        name: "User Name 0".to_string(),
        email: "user0@example.com".to_string(),
        scores: vec![95, 87, 92, 88, 91, 89, 94, 90, 93, 86],
        active: true,
        rating: 4.5,
    }];

    let bonjson_bytes = bonjson::to_vec(&data).unwrap();
    let json_bytes = serde_json::to_vec(&data).unwrap();

    println!("BONJSON size: {} bytes", bonjson_bytes.len());
    println!("JSON size: {} bytes", json_bytes.len());

    // Warmup
    for _ in 0..10000 {
        let _: Vec<TestData> = bonjson::from_slice(&bonjson_bytes).unwrap();
        let _: Vec<TestData> = serde_json::from_slice(&json_bytes).unwrap();
    }

    // BONJSON decode
    let start = Instant::now();
    for _ in 0..500_000 {
        let _: Vec<TestData> = bonjson::from_slice(&bonjson_bytes).unwrap();
    }
    let bonjson_time = start.elapsed();

    // JSON decode
    let start = Instant::now();
    for _ in 0..500_000 {
        let _: Vec<TestData> = serde_json::from_slice(&json_bytes).unwrap();
    }
    let json_time = start.elapsed();

    println!("BONJSON decode: {:?} ({:.2} ns/op)", bonjson_time, bonjson_time.as_nanos() as f64 / 500_000.0);
    println!("JSON decode:    {:?} ({:.2} ns/op)", json_time, json_time.as_nanos() as f64 / 500_000.0);
    println!("Speedup: {:.2}x", json_time.as_nanos() as f64 / bonjson_time.as_nanos() as f64);
}
