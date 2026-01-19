// ABOUTME: Unit tests for the BONJSON types module.
// ABOUTME: Tests type codes, BigNumber, and related utilities.

use crate::types::{type_code, BigNumber};

#[test]
fn test_small_int_codes() {
    assert!(type_code::is_small_int(0));
    assert!(type_code::is_small_int(100));
    assert!(type_code::is_small_int(0x9c)); // -100
    assert!(type_code::is_small_int(0xff)); // -1

    assert_eq!(type_code::small_int_value(0), 0);
    assert_eq!(type_code::small_int_value(100), 100);
    assert_eq!(type_code::small_int_value(0xff), -1);
    assert_eq!(type_code::small_int_value(0x9c), -100);
}

#[test]
fn test_short_string_codes() {
    assert!(type_code::is_short_string(0x80));
    assert!(type_code::is_short_string(0x8f));
    assert!(!type_code::is_short_string(0x79));

    assert_eq!(type_code::short_string_len(0x80), 0);
    assert_eq!(type_code::short_string_len(0x8f), 15);
}

#[test]
fn test_big_number() {
    let bn = BigNumber::new(1, 15, -1);
    assert_eq!(bn.to_f64(), 1.5);

    let bn = BigNumber::from_i64(-1000);
    assert_eq!(bn.sign, -1);
    assert_eq!(bn.significand, 1000);
    assert_eq!(bn.exponent, 0);
    assert_eq!(bn.to_i64(), Some(-1000));
}
