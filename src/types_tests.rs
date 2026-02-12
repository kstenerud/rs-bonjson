// ABOUTME: Unit tests for the BONJSON types module.
// ABOUTME: Tests type codes, BigNumber, and related utilities.

use crate::types::{type_code, BigNumber};

#[test]
fn test_small_int_codes() {
    assert!(type_code::is_small_int(0x00)); // 0
    assert!(type_code::is_small_int(0x64)); // 100
    assert!(!type_code::is_small_int(0x65)); // String0

    assert_eq!(type_code::small_int_value(0x00), 0);
    assert_eq!(type_code::small_int_value(0x01), 1);
    assert_eq!(type_code::small_int_value(0x64), 100);
    assert_eq!(type_code::small_int_value(0x2a), 42);

    assert_eq!(type_code::small_int_code(0), 0x00);
    assert_eq!(type_code::small_int_code(1), 0x01);
    assert_eq!(type_code::small_int_code(100), 0x64);
}

#[test]
fn test_short_string_codes() {
    // Short strings: 0x65-0xa7
    assert!(type_code::is_short_string(0x65));
    assert!(type_code::is_short_string(0xa7));
    assert!(!type_code::is_short_string(0x64));
    assert!(!type_code::is_short_string(0xa8));

    assert_eq!(type_code::short_string_len(0x65), 0);
    assert_eq!(type_code::short_string_len(0xa7), 66);
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
