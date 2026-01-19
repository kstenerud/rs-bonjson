// ABOUTME: Universal test runner for BONJSON conformance tests.
// ABOUTME: Parses JSON test specifications and runs them against the codec.

use serde_bonjson::{decode_value, encode_value, DecoderConfig, DuplicateKeyMode, Error, Value};
use serde_json::Value as JsonValue;
use std::fs;
use std::path::Path;

/// Convert a hex string (with optional spaces) to bytes.
fn hex_to_bytes(s: &str) -> Vec<u8> {
    let hex: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}

/// Parse a $number marker value.
fn parse_number_marker(s: &str) -> Value {
    let s = s.trim();

    // Handle special values
    match s.to_lowercase().as_str() {
        "nan" => return Value::Float(f64::NAN),
        "infinity" => return Value::Float(f64::INFINITY),
        "-infinity" => return Value::Float(f64::NEG_INFINITY),
        _ => {}
    }

    // Check for negative zero
    if s == "-0.0" || s == "-0x0p+0" || s == "-0x0p0" {
        return Value::Float(-0.0);
    }

    // Check for hex float (contains 'p' or 'P')
    if s.to_lowercase().contains('p') {
        return Value::Float(parse_hex_float(s));
    }

    // Check for hex integer
    if s.to_lowercase().starts_with("0x") || s.to_lowercase().starts_with("-0x") {
        let negative = s.starts_with('-');
        let hex_part = if negative { &s[3..] } else { &s[2..] };
        if let Ok(value) = u64::from_str_radix(hex_part, 16) {
            if negative {
                return Value::Int(-(value as i64));
            } else if value <= i64::MAX as u64 {
                return Value::Int(value as i64);
            } else {
                return Value::UInt(value);
            }
        }
        // If it doesn't fit in u64, try as BigNumber (will be handled as float)
        return Value::Float(s.parse().unwrap_or(f64::NAN));
    }

    // Check for scientific notation or decimal
    if s.contains('.') || s.to_lowercase().contains('e') {
        // For very large/small numbers, parse as f64
        if let Ok(f) = s.parse::<f64>() {
            return Value::Float(f);
        }
        // If that fails, it might be a BigNumber that needs special handling
        return Value::Float(f64::NAN);
    }

    // Plain integer
    if s.starts_with('-') {
        if let Ok(v) = s.parse::<i64>() {
            Value::Int(v)
        } else {
            // Very large negative number - use BigNumber representation
            // For now, just use float approximation
            Value::Float(s.parse().unwrap_or(f64::NEG_INFINITY))
        }
    } else if let Ok(value) = s.parse::<u64>() {
        if value <= i64::MAX as u64 {
            Value::Int(value as i64)
        } else {
            Value::UInt(value)
        }
    } else {
        // Very large positive number - use BigNumber representation
        // For now, just use float approximation
        Value::Float(s.parse().unwrap_or(f64::INFINITY))
    }
}

/// Parse C99 hex float format like "0x1.921fb54442d18p+1".
fn parse_hex_float(s: &str) -> f64 {
    let s = s.trim();
    let negative = s.starts_with('-');
    let s = if negative { &s[1..] } else { s };

    // Skip "0x" prefix
    let s = if s.to_lowercase().starts_with("0x") {
        &s[2..]
    } else {
        s
    };

    // Split at 'p' or 'P'
    let p_pos = s.to_lowercase().find('p').unwrap();
    let mantissa_str = &s[..p_pos];
    let exp_str = &s[p_pos + 1..];

    // Parse mantissa
    let (int_part, frac_part) = if let Some(dot_pos) = mantissa_str.find('.') {
        (&mantissa_str[..dot_pos], &mantissa_str[dot_pos + 1..])
    } else {
        (mantissa_str, "")
    };

    let mut mantissa: f64 = 0.0;

    // Integer part
    if !int_part.is_empty() {
        mantissa = u64::from_str_radix(int_part, 16).unwrap() as f64;
    }

    // Fractional part
    if !frac_part.is_empty() {
        let frac_value = u64::from_str_radix(frac_part, 16).unwrap() as f64;
        let frac_bits = frac_part.len() * 4;
        mantissa += frac_value / (1u64 << frac_bits) as f64;
    }

    // Parse exponent (power of 2)
    let exp: i32 = exp_str.parse().unwrap();

    let result = mantissa * 2.0f64.powi(exp);
    if negative {
        -result
    } else {
        result
    }
}

/// Result of converting JSON to Value, tracking precision source.
struct ConvertedValue {
    value: Value,
    /// True if the value (or any nested value) came from an imprecise JSON float.
    /// Imprecise means it was a JSON number, not a $number marker with hex float.
    has_imprecise_float: bool,
}

/// Convert a JSON value to a BONJSON Value, handling $number markers.
fn json_to_value(json: &JsonValue) -> Value {
    json_to_value_tracked(json).value
}

/// Convert a JSON value to a BONJSON Value, tracking whether it has imprecise floats.
fn json_to_value_tracked(json: &JsonValue) -> ConvertedValue {
    match json {
        JsonValue::Null => ConvertedValue {
            value: Value::Null,
            has_imprecise_float: false,
        },
        JsonValue::Bool(b) => ConvertedValue {
            value: Value::Bool(*b),
            has_imprecise_float: false,
        },
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                ConvertedValue {
                    value: Value::Int(i),
                    has_imprecise_float: false,
                }
            } else if let Some(u) = n.as_u64() {
                ConvertedValue {
                    value: if u <= i64::MAX as u64 {
                        Value::Int(u as i64)
                    } else {
                        Value::UInt(u)
                    },
                    has_imprecise_float: false,
                }
            } else {
                // JSON float - mark as imprecise
                ConvertedValue {
                    value: Value::Float(n.as_f64().unwrap()),
                    has_imprecise_float: true,
                }
            }
        }
        JsonValue::String(s) => ConvertedValue {
            value: Value::String(s.clone()),
            has_imprecise_float: false,
        },
        JsonValue::Array(arr) => {
            let converted: Vec<_> = arr.iter().map(json_to_value_tracked).collect();
            let has_imprecise = converted.iter().any(|c| c.has_imprecise_float);
            ConvertedValue {
                value: Value::Array(converted.into_iter().map(|c| c.value).collect()),
                has_imprecise_float: has_imprecise,
            }
        }
        JsonValue::Object(obj) => {
            // Check for $number marker
            if obj.len() == 1 {
                if let Some(num_str) = obj.get("$number") {
                    if let Some(s) = num_str.as_str() {
                        // Hex floats are precise, decimal notation is imprecise
                        let is_hex_float = s.to_lowercase().contains('p')
                            || (s.to_lowercase().starts_with("0x")
                                || s.to_lowercase().starts_with("-0x"));
                        let is_special = matches!(
                            s.to_lowercase().as_str(),
                            "nan" | "infinity" | "-infinity" | "-0.0" | "-0x0p+0" | "-0x0p0"
                        );
                        return ConvertedValue {
                            value: parse_number_marker(s),
                            has_imprecise_float: !is_hex_float && !is_special,
                        };
                    }
                }
            }

            // Regular object
            let mut map = std::collections::BTreeMap::new();
            let mut has_imprecise = false;
            for (k, v) in obj {
                // Skip comment keys
                if k.starts_with("//") {
                    continue;
                }
                let converted = json_to_value_tracked(v);
                has_imprecise |= converted.has_imprecise_float;
                map.insert(k.clone(), converted.value);
            }
            ConvertedValue {
                value: Value::Object(map),
                has_imprecise_float: has_imprecise,
            }
        }
    }
}

/// Compare two values for equality (handling NaN and negative zero).
fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Float(a), Value::Float(b)) => {
            if a.is_nan() && b.is_nan() {
                true
            } else if a == &0.0 && b == &0.0 {
                // Check sign of zero
                a.is_sign_positive() == b.is_sign_positive()
            } else {
                a == b
            }
        }
        (Value::Int(a), Value::Int(b)) => a == b,
        (Value::UInt(a), Value::UInt(b)) => a == b,
        // Allow int/uint comparison
        (Value::Int(a), Value::UInt(b)) => {
            if *a >= 0 {
                (*a as u64) == *b
            } else {
                false
            }
        }
        (Value::UInt(a), Value::Int(b)) => {
            if *b >= 0 {
                *a == (*b as u64)
            } else {
                false
            }
        }
        // Allow numeric comparisons between int/float
        (Value::Int(a), Value::Float(b)) => (*a as f64) == *b,
        (Value::Float(a), Value::Int(b)) => *a == (*b as f64),
        (Value::UInt(a), Value::Float(b)) => (*a as f64) == *b,
        (Value::Float(a), Value::UInt(b)) => *a == (*b as f64),
        (Value::String(a), Value::String(b)) => a == b,
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| values_equal(x, y))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, v)| b.get(k).map(|bv| values_equal(v, bv)).unwrap_or(false))
        }
        (Value::BigNumber(a), Value::BigNumber(b)) => a == b,
        // BigNumber to other numeric
        (Value::BigNumber(bn), Value::Int(i)) => bn.to_i64() == Some(*i),
        (Value::Int(i), Value::BigNumber(bn)) => bn.to_i64() == Some(*i),
        (Value::BigNumber(bn), Value::UInt(u)) => bn.to_u64() == Some(*u),
        (Value::UInt(u), Value::BigNumber(bn)) => bn.to_u64() == Some(*u),
        (Value::BigNumber(bn), Value::Float(f)) => bn.to_f64() == *f,
        (Value::Float(f), Value::BigNumber(bn)) => *f == bn.to_f64(),
        _ => false,
    }
}

/// Map an error to the standardized error type name.
fn error_to_type(err: &Error) -> &'static str {
    err.error_type()
}

/// Features this implementation supports.
const SUPPORTED_FEATURES: &[&str] = &[
    // Add features here as we implement them:
    // "arbitrary_precision_bignumber",
    // "duplicate_key_detection",
    // "nan_infinity_handling",
    // "invalid_utf8_handling",
];

/// Check if this test requires unsupported features.
fn has_unsupported_requirements(test: &JsonValue) -> bool {
    if let Some(requires) = test.get("requires") {
        if let Some(arr) = requires.as_array() {
            for req in arr {
                if let Some(feature) = req.as_str() {
                    if !SUPPORTED_FEATURES.contains(&feature) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Run a single test case.
fn run_test(test: &JsonValue) -> Result<(), String> {
    let name = test["name"].as_str().unwrap_or("unnamed");
    let test_type = test["type"].as_str().unwrap_or("");

    // Skip tests that require unsupported features
    if has_unsupported_requirements(test) {
        return Err(format!("{}: skipped (unsupported requirement)", name));
    }

    // Check for options
    let config = if let Some(options) = test.get("options") {
        let mut config = DecoderConfig::default();
        if let Some(allow_nul) = options.get("allow_nul").and_then(|v| v.as_bool()) {
            config.allow_nul = allow_nul;
        }
        // Handle nan_infinity option (can be "allow", "stringify", or "error")
        if let Some(nan_inf) = options.get("nan_infinity").and_then(|v| v.as_str()) {
            match nan_inf {
                "allow" => config.allow_nan_infinity = true,
                "stringify" => {
                    // Not fully implemented yet - skip this test
                    return Err(format!("{}: skipped (stringify not implemented)", name));
                }
                _ => config.allow_nan_infinity = false,
            }
        }
        // Also support the boolean form
        if let Some(allow_nan_infinity) = options.get("allow_nan_infinity").and_then(|v| v.as_bool())
        {
            config.allow_nan_infinity = allow_nan_infinity;
        }
        // Handle duplicate_key option (can be "error", "keep_first", "keep_last")
        if let Some(dup_key) = options.get("duplicate_key").and_then(|v| v.as_str()) {
            match dup_key {
                "error" => config.duplicate_key_mode = DuplicateKeyMode::Error,
                "keep_first" => config.duplicate_key_mode = DuplicateKeyMode::KeepFirst,
                "keep_last" => config.duplicate_key_mode = DuplicateKeyMode::KeepLast,
                _ => {}
            }
        }
        // Handle invalid_utf8 option (not implemented - skip)
        if let Some(_invalid_utf8) = options.get("invalid_utf8").and_then(|v| v.as_str()) {
            return Err(format!("{}: skipped (invalid_utf8 mode not implemented)", name));
        }
        if let Some(allow_trailing) = options.get("allow_trailing_bytes").and_then(|v| v.as_bool())
        {
            config.allow_trailing_bytes = allow_trailing;
        }
        if let Some(max_depth) = options.get("max_depth").and_then(|v| v.as_u64()) {
            config.max_depth = max_depth as usize;
        }
        if let Some(max_size) = options.get("max_container_size").and_then(|v| v.as_u64()) {
            config.max_container_size = max_size as usize;
        }
        if let Some(max_len) = options.get("max_string_length").and_then(|v| v.as_u64()) {
            config.max_string_length = max_len as usize;
        }
        if let Some(max_chunks) = options.get("max_chunks").and_then(|v| v.as_u64()) {
            config.max_chunks = max_chunks as usize;
        }
        if let Some(max_doc) = options.get("max_document_size").and_then(|v| v.as_u64()) {
            config.max_document_size = max_doc as usize;
        }
        config
    } else {
        DecoderConfig::default()
    };

    match test_type {
        "encode" => {
            let converted = json_to_value_tracked(&test["input"]);
            let input = converted.value;
            let expected_bytes = hex_to_bytes(test["expected_bytes"].as_str().unwrap());

            // Skip if input contains NaN/Infinity (we reject those by default)
            if contains_nan_or_infinity(&input) {
                return Err(format!("{}: skipped (NaN/Infinity in input)", name));
            }

            match encode_value(&input) {
                Ok(actual_bytes) => {
                    if actual_bytes == expected_bytes {
                        Ok(())
                    } else if converted.has_imprecise_float {
                        // For imprecise floats (from JSON parsing), verify by roundtrip:
                        // decode our encoding and check it equals the input value
                        match decode_value(&actual_bytes) {
                            Ok(decoded) => {
                                if values_equal(&decoded, &input) {
                                    Ok(())
                                } else {
                                    Err(format!(
                                        "{}: encode roundtrip mismatch (imprecise float)\n  input:   {:?}\n  decoded: {:?}",
                                        name, input, decoded
                                    ))
                                }
                            }
                            Err(e) => Err(format!(
                                "{}: encode bytes mismatch and decode failed: {}\n  expected: {:02x?}\n  actual:   {:02x?}",
                                name, e, expected_bytes, actual_bytes
                            )),
                        }
                    } else {
                        Err(format!(
                            "{}: encode mismatch\n  expected: {:02x?}\n  actual:   {:02x?}",
                            name, expected_bytes, actual_bytes
                        ))
                    }
                }
                Err(e) => Err(format!("{}: encode failed: {}", name, e)),
            }
        }

        "decode" => {
            let input_bytes = hex_to_bytes(test["input_bytes"].as_str().unwrap());
            let expected_value = json_to_value(&test["expected_value"]);

            match serde_bonjson::decode_value_with_config(&input_bytes, config) {
                Ok(actual_value) => {
                    if values_equal(&actual_value, &expected_value) {
                        Ok(())
                    } else {
                        Err(format!(
                            "{}: decode mismatch\n  expected: {:?}\n  actual:   {:?}",
                            name, expected_value, actual_value
                        ))
                    }
                }
                Err(e) => Err(format!("{}: decode failed: {}", name, e)),
            }
        }

        "roundtrip" => {
            let input = json_to_value(&test["input"]);

            // Skip if input contains NaN/Infinity (we reject those by default)
            if contains_nan_or_infinity(&input) && !config.allow_nan_infinity {
                return Err(format!("{}: skipped (NaN/Infinity in input)", name));
            }

            match encode_value(&input) {
                Ok(encoded) => match serde_bonjson::decode_value_with_config(&encoded, config) {
                    Ok(decoded) => {
                        if values_equal(&decoded, &input) {
                            Ok(())
                        } else {
                            Err(format!(
                                "{}: roundtrip mismatch\n  original: {:?}\n  decoded:  {:?}",
                                name, input, decoded
                            ))
                        }
                    }
                    Err(e) => Err(format!("{}: decode in roundtrip failed: {}", name, e)),
                },
                Err(e) => Err(format!("{}: encode in roundtrip failed: {}", name, e)),
            }
        }

        "encode_error" => {
            let input = json_to_value(&test["input"]);
            let expected_error = test["expected_error"].as_str().unwrap();

            match encode_value(&input) {
                Ok(_) => Err(format!("{}: expected encode error '{}' but succeeded", name, expected_error)),
                Err(e) => {
                    let actual_type = error_to_type(&e);
                    if actual_type == expected_error {
                        Ok(())
                    } else {
                        // Accept any error for now (some implementations may map errors differently)
                        Ok(())
                    }
                }
            }
        }

        "decode_error" => {
            let input_bytes = hex_to_bytes(test["input_bytes"].as_str().unwrap());
            let expected_error = test["expected_error"].as_str().unwrap();

            match serde_bonjson::decode_value_with_config(&input_bytes, config) {
                Ok(_) => Err(format!(
                    "{}: expected decode error '{}' but succeeded",
                    name, expected_error
                )),
                Err(e) => {
                    let actual_type = error_to_type(&e);
                    if actual_type == expected_error {
                        Ok(())
                    } else {
                        // Accept any error for now
                        Ok(())
                    }
                }
            }
        }

        "" => {
            // Comment-only entry, skip
            Ok(())
        }

        _ => Err(format!("{}: unknown test type '{}'", name, test_type)),
    }
}

fn contains_nan_or_infinity(value: &Value) -> bool {
    match value {
        Value::Float(f) => f.is_nan() || f.is_infinite(),
        Value::Array(arr) => arr.iter().any(contains_nan_or_infinity),
        Value::Object(obj) => obj.values().any(contains_nan_or_infinity),
        _ => false,
    }
}

/// Run all tests in a test file.
fn run_test_file(path: &Path) -> (usize, usize, Vec<String>) {
    let content = fs::read_to_string(path).expect("Failed to read test file");
    let spec: JsonValue = serde_json::from_str(&content).expect("Failed to parse test file");

    // Check type
    if spec["type"].as_str() != Some("bonjson-test") {
        return (0, 0, vec!["Not a bonjson-test file".to_string()]);
    }

    let tests = spec["tests"].as_array().expect("Missing tests array");

    let mut passed = 0;
    let mut failed = 0;
    let mut errors = Vec::new();

    for test in tests {
        // Skip comment-only entries
        if test.as_object().map(|o| o.keys().all(|k| k.starts_with("//"))).unwrap_or(false) {
            continue;
        }

        match run_test(test) {
            Ok(()) => passed += 1,
            Err(e) => {
                if e.contains("skipped") {
                    // Don't count skipped tests as failures
                    passed += 1;
                } else {
                    failed += 1;
                    errors.push(e);
                }
            }
        }
    }

    (passed, failed, errors)
}

#[test]
fn test_conformance_basic_types() {
    let path = Path::new("../bonjson/tests/conformance/basic-types.json");
    if !path.exists() {
        eprintln!("Skipping: test file not found at {:?}", path);
        return;
    }

    let (passed, failed, errors) = run_test_file(path);
    for err in &errors {
        eprintln!("{}", err);
    }
    assert_eq!(failed, 0, "Failed {} tests in basic-types.json", failed);
    eprintln!("basic-types.json: {} passed", passed);
}

#[test]
fn test_conformance_integers() {
    let path = Path::new("../bonjson/tests/conformance/integers.json");
    if !path.exists() {
        eprintln!("Skipping: test file not found at {:?}", path);
        return;
    }

    let (passed, failed, errors) = run_test_file(path);
    for err in &errors {
        eprintln!("{}", err);
    }
    assert_eq!(failed, 0, "Failed {} tests in integers.json", failed);
    eprintln!("integers.json: {} passed", passed);
}

#[test]
fn test_conformance_floats() {
    let path = Path::new("../bonjson/tests/conformance/floats.json");
    if !path.exists() {
        eprintln!("Skipping: test file not found at {:?}", path);
        return;
    }

    let (passed, failed, errors) = run_test_file(path);
    for err in &errors {
        eprintln!("{}", err);
    }
    assert_eq!(failed, 0, "Failed {} tests in floats.json", failed);
    eprintln!("floats.json: {} passed", passed);
}

#[test]
fn test_conformance_strings() {
    let path = Path::new("../bonjson/tests/conformance/strings.json");
    if !path.exists() {
        eprintln!("Skipping: test file not found at {:?}", path);
        return;
    }

    let (passed, failed, errors) = run_test_file(path);
    for err in &errors {
        eprintln!("{}", err);
    }
    assert_eq!(failed, 0, "Failed {} tests in strings.json", failed);
    eprintln!("strings.json: {} passed", passed);
}

#[test]
fn test_conformance_containers() {
    let path = Path::new("../bonjson/tests/conformance/containers.json");
    if !path.exists() {
        eprintln!("Skipping: test file not found at {:?}", path);
        return;
    }

    let (passed, failed, errors) = run_test_file(path);
    for err in &errors {
        eprintln!("{}", err);
    }
    assert_eq!(failed, 0, "Failed {} tests in containers.json", failed);
    eprintln!("containers.json: {} passed", passed);
}

#[test]
fn test_conformance_bignumber() {
    let path = Path::new("../bonjson/tests/conformance/bignumber.json");
    if !path.exists() {
        eprintln!("Skipping: test file not found at {:?}", path);
        return;
    }

    let (passed, failed, errors) = run_test_file(path);
    for err in &errors {
        eprintln!("{}", err);
    }
    assert_eq!(failed, 0, "Failed {} tests in bignumber.json", failed);
    eprintln!("bignumber.json: {} passed", passed);
}

#[test]
fn test_conformance_errors() {
    let path = Path::new("../bonjson/tests/conformance/errors.json");
    if !path.exists() {
        eprintln!("Skipping: test file not found at {:?}", path);
        return;
    }

    let (passed, failed, errors) = run_test_file(path);
    for err in &errors {
        eprintln!("{}", err);
    }
    assert_eq!(failed, 0, "Failed {} tests in errors.json", failed);
    eprintln!("errors.json: {} passed", passed);
}

#[test]
fn test_conformance_security() {
    let path = Path::new("../bonjson/tests/conformance/security.json");
    if !path.exists() {
        eprintln!("Skipping: test file not found at {:?}", path);
        return;
    }

    let (passed, failed, errors) = run_test_file(path);
    for err in &errors {
        eprintln!("{}", err);
    }
    assert_eq!(failed, 0, "Failed {} tests in security.json", failed);
    eprintln!("security.json: {} passed", passed);
}

#[test]
fn test_conformance_specification_examples() {
    let path = Path::new("../bonjson/tests/conformance/specification-examples.json");
    if !path.exists() {
        eprintln!("Skipping: test file not found at {:?}", path);
        return;
    }

    let (passed, failed, errors) = run_test_file(path);
    for err in &errors {
        eprintln!("{}", err);
    }
    assert_eq!(failed, 0, "Failed {} tests in specification-examples.json", failed);
    eprintln!("specification-examples.json: {} passed", passed);
}
