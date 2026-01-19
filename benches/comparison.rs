// ABOUTME: Benchmark comparing BONJSON codec performance against serde_json.

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct SimpleStruct {
    name: String,
    age: u32,
    active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct ComplexStruct {
    id: u64,
    name: String,
    email: String,
    scores: Vec<i32>,
    metadata: Metadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Metadata {
    created: String,
    updated: String,
    tags: Vec<String>,
    rating: f64,
}

fn create_simple_data() -> SimpleStruct {
    SimpleStruct {
        name: "Alice".to_string(),
        age: 30,
        active: true,
    }
}

fn create_complex_data() -> ComplexStruct {
    ComplexStruct {
        id: 12345678901234,
        name: "Bob Smith".to_string(),
        email: "bob.smith@example.com".to_string(),
        scores: vec![95, 87, 92, 88, 91, 89, 94, 90, 93, 86],
        metadata: Metadata {
            created: "2024-01-15T10:30:00Z".to_string(),
            updated: "2024-01-18T14:22:33Z".to_string(),
            tags: vec![
                "premium".to_string(),
                "verified".to_string(),
                "active".to_string(),
            ],
            rating: 4.7,
        },
    }
}

fn create_array_data() -> Vec<i32> {
    (0..1000).collect()
}

fn create_nested_data() -> Vec<ComplexStruct> {
    (0..100).map(|i| {
        ComplexStruct {
            id: i as u64,
            name: format!("User {}", i),
            email: format!("user{}@example.com", i),
            scores: vec![i as i32; 10],
            metadata: Metadata {
                created: "2024-01-15T10:30:00Z".to_string(),
                updated: "2024-01-18T14:22:33Z".to_string(),
                tags: vec!["tag1".to_string(), "tag2".to_string()],
                rating: (i as f64) / 10.0,
            },
        }
    }).collect()
}

fn bench_simple_struct(c: &mut Criterion) {
    let data = create_simple_data();

    let mut group = c.benchmark_group("simple_struct");

    // Encoding
    group.bench_function("bonjson_encode", |b| {
        b.iter(|| bonjson::to_vec(black_box(&data)).unwrap())
    });

    group.bench_function("json_encode", |b| {
        b.iter(|| serde_json::to_vec(black_box(&data)).unwrap())
    });

    // Decoding
    let bonjson_bytes = bonjson::to_vec(&data).unwrap();
    let json_bytes = serde_json::to_vec(&data).unwrap();

    group.bench_function("bonjson_decode", |b| {
        b.iter(|| {
            let decoded: SimpleStruct = bonjson::from_slice(black_box(&bonjson_bytes)).unwrap();
            decoded
        })
    });

    group.bench_function("json_decode", |b| {
        b.iter(|| {
            let decoded: SimpleStruct = serde_json::from_slice(black_box(&json_bytes)).unwrap();
            decoded
        })
    });

    println!("Simple struct sizes: BONJSON={} bytes, JSON={} bytes",
             bonjson_bytes.len(), json_bytes.len());

    group.finish();
}

fn bench_complex_struct(c: &mut Criterion) {
    let data = create_complex_data();

    let mut group = c.benchmark_group("complex_struct");

    // Encoding
    group.bench_function("bonjson_encode", |b| {
        b.iter(|| bonjson::to_vec(black_box(&data)).unwrap())
    });

    group.bench_function("json_encode", |b| {
        b.iter(|| serde_json::to_vec(black_box(&data)).unwrap())
    });

    // Decoding
    let bonjson_bytes = bonjson::to_vec(&data).unwrap();
    let json_bytes = serde_json::to_vec(&data).unwrap();

    group.bench_function("bonjson_decode", |b| {
        b.iter(|| {
            let decoded: ComplexStruct = bonjson::from_slice(black_box(&bonjson_bytes)).unwrap();
            decoded
        })
    });

    group.bench_function("json_decode", |b| {
        b.iter(|| {
            let decoded: ComplexStruct = serde_json::from_slice(black_box(&json_bytes)).unwrap();
            decoded
        })
    });

    println!("Complex struct sizes: BONJSON={} bytes, JSON={} bytes",
             bonjson_bytes.len(), json_bytes.len());

    group.finish();
}

fn bench_integer_array(c: &mut Criterion) {
    let data = create_array_data();

    let mut group = c.benchmark_group("integer_array_1000");

    let bonjson_bytes = bonjson::to_vec(&data).unwrap();
    let json_bytes = serde_json::to_vec(&data).unwrap();

    group.throughput(Throughput::Elements(data.len() as u64));

    // Encoding
    group.bench_function("bonjson_encode", |b| {
        b.iter(|| bonjson::to_vec(black_box(&data)).unwrap())
    });

    group.bench_function("json_encode", |b| {
        b.iter(|| serde_json::to_vec(black_box(&data)).unwrap())
    });

    // Decoding
    group.bench_function("bonjson_decode", |b| {
        b.iter(|| {
            let decoded: Vec<i32> = bonjson::from_slice(black_box(&bonjson_bytes)).unwrap();
            decoded
        })
    });

    group.bench_function("json_decode", |b| {
        b.iter(|| {
            let decoded: Vec<i32> = serde_json::from_slice(black_box(&json_bytes)).unwrap();
            decoded
        })
    });

    println!("Integer array sizes: BONJSON={} bytes, JSON={} bytes",
             bonjson_bytes.len(), json_bytes.len());

    group.finish();
}

fn bench_nested_data(c: &mut Criterion) {
    let data = create_nested_data();

    let mut group = c.benchmark_group("nested_100_objects");

    let bonjson_bytes = bonjson::to_vec(&data).unwrap();
    let json_bytes = serde_json::to_vec(&data).unwrap();

    group.throughput(Throughput::Bytes(json_bytes.len() as u64));

    // Encoding
    group.bench_function("bonjson_encode", |b| {
        b.iter(|| bonjson::to_vec(black_box(&data)).unwrap())
    });

    group.bench_function("json_encode", |b| {
        b.iter(|| serde_json::to_vec(black_box(&data)).unwrap())
    });

    // Decoding
    group.bench_function("bonjson_decode", |b| {
        b.iter(|| {
            let decoded: Vec<ComplexStruct> = bonjson::from_slice(black_box(&bonjson_bytes)).unwrap();
            decoded
        })
    });

    group.bench_function("json_decode", |b| {
        b.iter(|| {
            let decoded: Vec<ComplexStruct> = serde_json::from_slice(black_box(&json_bytes)).unwrap();
            decoded
        })
    });

    println!("Nested data sizes: BONJSON={} bytes, JSON={} bytes ({:.1}% of JSON)",
             bonjson_bytes.len(), json_bytes.len(),
             (bonjson_bytes.len() as f64 / json_bytes.len() as f64) * 100.0);

    group.finish();
}

criterion_group!(
    benches,
    bench_simple_struct,
    bench_complex_struct,
    bench_integer_array,
    bench_nested_data,
);

criterion_main!(benches);
