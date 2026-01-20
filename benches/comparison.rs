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
        b.iter(|| black_box(serde_bonjson::to_vec(black_box(&data)).unwrap()))
    });

    group.bench_function("json_encode", |b| {
        b.iter(|| black_box(serde_json::to_vec(black_box(&data)).unwrap()))
    });

    // Decoding
    let bonjson_bytes = serde_bonjson::to_vec(&data).unwrap();
    let json_bytes = serde_json::to_vec(&data).unwrap();

    group.bench_function("bonjson_decode", |b| {
        b.iter(|| {
            black_box(serde_bonjson::from_slice::<SimpleStruct>(black_box(&bonjson_bytes)).unwrap())
        })
    });

    group.bench_function("json_decode", |b| {
        b.iter(|| {
            black_box(serde_json::from_slice::<SimpleStruct>(black_box(&json_bytes)).unwrap())
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
        b.iter(|| black_box(serde_bonjson::to_vec(black_box(&data)).unwrap()))
    });

    group.bench_function("json_encode", |b| {
        b.iter(|| black_box(serde_json::to_vec(black_box(&data)).unwrap()))
    });

    // Decoding
    let bonjson_bytes = serde_bonjson::to_vec(&data).unwrap();
    let json_bytes = serde_json::to_vec(&data).unwrap();

    group.bench_function("bonjson_decode", |b| {
        b.iter(|| {
            black_box(serde_bonjson::from_slice::<ComplexStruct>(black_box(&bonjson_bytes)).unwrap())
        })
    });

    group.bench_function("json_decode", |b| {
        b.iter(|| {
            black_box(serde_json::from_slice::<ComplexStruct>(black_box(&json_bytes)).unwrap())
        })
    });

    println!("Complex struct sizes: BONJSON={} bytes, JSON={} bytes",
             bonjson_bytes.len(), json_bytes.len());

    group.finish();
}

fn bench_integer_array(c: &mut Criterion) {
    let data = create_array_data();

    let mut group = c.benchmark_group("integer_array_1000");

    let bonjson_bytes = serde_bonjson::to_vec(&data).unwrap();
    let json_bytes = serde_json::to_vec(&data).unwrap();

    group.throughput(Throughput::Elements(data.len() as u64));

    // Encoding
    group.bench_function("bonjson_encode", |b| {
        b.iter(|| black_box(serde_bonjson::to_vec(black_box(&data)).unwrap()))
    });

    group.bench_function("json_encode", |b| {
        b.iter(|| black_box(serde_json::to_vec(black_box(&data)).unwrap()))
    });

    // Decoding
    group.bench_function("bonjson_decode", |b| {
        b.iter(|| {
            black_box(serde_bonjson::from_slice::<Vec<i32>>(black_box(&bonjson_bytes)).unwrap())
        })
    });

    group.bench_function("json_decode", |b| {
        b.iter(|| {
            black_box(serde_json::from_slice::<Vec<i32>>(black_box(&json_bytes)).unwrap())
        })
    });

    println!("Integer array sizes: BONJSON={} bytes, JSON={} bytes",
             bonjson_bytes.len(), json_bytes.len());

    group.finish();
}

fn bench_nested_data(c: &mut Criterion) {
    let data = create_nested_data();

    let mut group = c.benchmark_group("nested_100_objects");

    let bonjson_bytes = serde_bonjson::to_vec(&data).unwrap();
    let json_bytes = serde_json::to_vec(&data).unwrap();

    group.throughput(Throughput::Bytes(json_bytes.len() as u64));

    // Encoding
    group.bench_function("bonjson_encode", |b| {
        b.iter(|| black_box(serde_bonjson::to_vec(black_box(&data)).unwrap()))
    });

    group.bench_function("json_encode", |b| {
        b.iter(|| black_box(serde_json::to_vec(black_box(&data)).unwrap()))
    });

    // Decoding
    group.bench_function("bonjson_decode", |b| {
        b.iter(|| {
            black_box(serde_bonjson::from_slice::<Vec<ComplexStruct>>(black_box(&bonjson_bytes)).unwrap())
        })
    });

    group.bench_function("json_decode", |b| {
        b.iter(|| {
            black_box(serde_json::from_slice::<Vec<ComplexStruct>>(black_box(&json_bytes)).unwrap())
        })
    });

    println!("Nested data sizes: BONJSON={} bytes, JSON={} bytes ({:.1}% of JSON)",
             bonjson_bytes.len(), json_bytes.len(),
             (bonjson_bytes.len() as f64 / json_bytes.len() as f64) * 100.0);

    group.finish();
}

/// Create data with many small strings (typical JSON field names/short values)
fn create_many_small_strings() -> Vec<String> {
    (0..1000)
        .map(|i| format!("field_{}", i))
        .collect()
}

/// Create data with large strings (paragraphs)
fn create_large_strings() -> Vec<String> {
    let paragraph = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. \
        Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. \
        Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris \
        nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in \
        reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla \
        pariatur. Excepteur sint occaecat cupidatat non proident, sunt in \
        culpa qui officia deserunt mollit anim id est laborum.";
    (0..100).map(|_| paragraph.to_string()).collect()
}

/// Create data with Unicode strings (non-ASCII)
fn create_unicode_strings() -> Vec<String> {
    vec![
        "日本語テキスト".to_string(),
        "Привет мир".to_string(),
        "مرحبا بالعالم".to_string(),
        "שלום עולם".to_string(),
        "🎉🎊🎁🎈🎂".to_string(),
        "Ελληνικά".to_string(),
        "中文文本示例".to_string(),
        "한국어 텍스트".to_string(),
        "ภาษาไทย".to_string(),
        "हिन्दी पाठ".to_string(),
    ].into_iter()
        .cycle()
        .take(500)
        .collect()
}

fn bench_many_small_strings(c: &mut Criterion) {
    let data = create_many_small_strings();

    let mut group = c.benchmark_group("many_small_strings_1000");

    let bonjson_bytes = serde_bonjson::to_vec(&data).unwrap();
    let json_bytes = serde_json::to_vec(&data).unwrap();

    group.throughput(Throughput::Elements(data.len() as u64));

    group.bench_function("bonjson_decode", |b| {
        b.iter(|| {
            black_box(serde_bonjson::from_slice::<Vec<String>>(black_box(&bonjson_bytes)).unwrap())
        })
    });

    group.bench_function("json_decode", |b| {
        b.iter(|| {
            black_box(serde_json::from_slice::<Vec<String>>(black_box(&json_bytes)).unwrap())
        })
    });

    println!("Many small strings: BONJSON={} bytes, JSON={} bytes",
             bonjson_bytes.len(), json_bytes.len());

    group.finish();
}

fn bench_large_strings(c: &mut Criterion) {
    let data = create_large_strings();

    let mut group = c.benchmark_group("large_strings_100");

    let bonjson_bytes = serde_bonjson::to_vec(&data).unwrap();
    let json_bytes = serde_json::to_vec(&data).unwrap();

    group.throughput(Throughput::Bytes(bonjson_bytes.len() as u64));

    group.bench_function("bonjson_decode", |b| {
        b.iter(|| {
            black_box(serde_bonjson::from_slice::<Vec<String>>(black_box(&bonjson_bytes)).unwrap())
        })
    });

    group.bench_function("json_decode", |b| {
        b.iter(|| {
            black_box(serde_json::from_slice::<Vec<String>>(black_box(&json_bytes)).unwrap())
        })
    });

    println!("Large strings: BONJSON={} bytes, JSON={} bytes",
             bonjson_bytes.len(), json_bytes.len());

    group.finish();
}

fn bench_unicode_strings(c: &mut Criterion) {
    let data = create_unicode_strings();

    let mut group = c.benchmark_group("unicode_strings_500");

    let bonjson_bytes = serde_bonjson::to_vec(&data).unwrap();
    let json_bytes = serde_json::to_vec(&data).unwrap();

    group.throughput(Throughput::Elements(data.len() as u64));

    group.bench_function("bonjson_decode", |b| {
        b.iter(|| {
            black_box(serde_bonjson::from_slice::<Vec<String>>(black_box(&bonjson_bytes)).unwrap())
        })
    });

    group.bench_function("json_decode", |b| {
        b.iter(|| {
            black_box(serde_json::from_slice::<Vec<String>>(black_box(&json_bytes)).unwrap())
        })
    });

    println!("Unicode strings: BONJSON={} bytes, JSON={} bytes",
             bonjson_bytes.len(), json_bytes.len());

    group.finish();
}

criterion_group!(
    benches,
    bench_simple_struct,
    bench_complex_struct,
    bench_integer_array,
    bench_nested_data,
    bench_many_small_strings,
    bench_large_strings,
    bench_unicode_strings,
);

criterion_main!(benches);
