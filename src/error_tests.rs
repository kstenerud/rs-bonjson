// ABOUTME: Unit tests for the BONJSON error module.
// ABOUTME: Tests error type identification and display formatting.

use crate::error::Error;

#[test]
fn test_error_types() {
    assert_eq!(Error::Truncated.error_type(), "truncated");
    assert_eq!(Error::InvalidTypeCode(0x65).error_type(), "invalid_type_code");
    assert_eq!(Error::NulCharacter.error_type(), "nul_character");
}

#[test]
fn test_error_display() {
    let err = Error::InvalidTypeCode(0x65);
    assert_eq!(format!("{}", err), "invalid type code: 0x65");
}
