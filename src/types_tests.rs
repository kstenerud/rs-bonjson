// ABOUTME: Unit tests for the BONJSON types module.
// ABOUTME: Tests type codes, BigNumber, and related utilities.

use crate::types::{type_code, BigNumber};

#[test]
fn test_small_int_codes() {
    assert!(type_code::is_small_int(0x00)); // -100
    assert!(type_code::is_small_int(0x64)); // 0
    assert!(type_code::is_small_int(0xc8)); // 100
    assert!(!type_code::is_small_int(0xc9)); // Reserved

    assert_eq!(type_code::small_int_value(0x00), -100);
    assert_eq!(type_code::small_int_value(0x63), -1);
    assert_eq!(type_code::small_int_value(0x64), 0);
    assert_eq!(type_code::small_int_value(0x65), 1);
    assert_eq!(type_code::small_int_value(0xc8), 100);

    assert_eq!(type_code::small_int_code(-100), 0x00);
    assert_eq!(type_code::small_int_code(-1), 0x63);
    assert_eq!(type_code::small_int_code(0), 0x64);
    assert_eq!(type_code::small_int_code(1), 0x65);
    assert_eq!(type_code::small_int_code(100), 0xc8);
}

#[test]
fn test_short_string_codes() {
    // Short strings: 0xd0-0xdf
    assert!(type_code::is_short_string(0xd0));
    assert!(type_code::is_short_string(0xdf));
    assert!(!type_code::is_short_string(0xcf));
    assert!(!type_code::is_short_string(0xe0));

    assert_eq!(type_code::short_string_len(0xd0), 0);
    assert_eq!(type_code::short_string_len(0xdf), 15);
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
