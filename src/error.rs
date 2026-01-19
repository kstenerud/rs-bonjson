// ABOUTME: Error types for BONJSON encoding and decoding.
// ABOUTME: Error variants map to the standardized error types in the BONJSON test spec.

use std::fmt;

/// The result type for BONJSON operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur during BONJSON encoding or decoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Unexpected end of input data.
    /// Test spec: "truncated"
    Truncated,

    /// Unconsumed bytes after decoding a complete document.
    /// Test spec: "`trailing_bytes`"
    TrailingBytes,

    /// Unrecognized or reserved type code encountered.
    /// Test spec: "`invalid_type_code`"
    InvalidTypeCode(u8),

    /// Invalid UTF-8 byte sequence in string.
    /// Test spec: "`invalid_utf8`"
    InvalidUtf8,

    /// NUL (0x00) byte in string (rejected by default).
    /// Test spec: "`nul_character`"
    NulCharacter,

    /// Duplicate key in object.
    /// Test spec: "`duplicate_key`"
    DuplicateKey,

    /// Missing container end marker.
    /// Test spec: "`unclosed_container`"
    UnclosedContainer,

    /// Generic invalid data (e.g., NaN in float, invalid `BigNumber`).
    /// Test spec: "`invalid_data`"
    InvalidData(String),

    /// Value exceeds allowed range.
    /// Test spec: "`value_out_of_range`"
    ValueOutOfRange,

    /// Non-canonical (oversized) length encoding.
    /// Test spec: "`non_canonical_length`"
    NonCanonicalLength,

    /// String exceeds chunk count limit.
    /// Test spec: "`too_many_chunks`"
    TooManyChunks,

    /// Zero-length chunk with continuation bit set.
    /// Test spec: "`empty_chunk_continuation`"
    EmptyChunkContinuation,

    /// Container nesting too deep.
    /// Test spec: "`max_depth_exceeded`"
    MaxDepthExceeded,

    /// String exceeds length limit.
    /// Test spec: "`max_string_length_exceeded`"
    MaxStringLengthExceeded,

    /// Container has too many elements.
    /// Test spec: "`max_container_size_exceeded`"
    MaxContainerSizeExceeded,

    /// Document exceeds size limit.
    /// Test spec: "`max_document_size_exceeded`"
    MaxDocumentSizeExceeded,

    /// Tried to close more containers than were opened.
    UnbalancedContainers,

    /// Expected an object key (string) but got a different type.
    ExpectedObjectKey,

    /// Container ended while expecting an object value.
    ExpectedObjectValue,

    /// IO error during encoding.
    Io(String),

    /// Custom error message (for serde integration).
    Custom(String),
}

impl Error {
    /// Create a Truncated error. Marked cold to help branch prediction.
    #[cold]
    #[inline(never)]
    #[must_use] pub fn truncated() -> Self {
        Error::Truncated
    }

    /// Create an `InvalidTypeCode` error. Marked cold to help branch prediction.
    #[cold]
    #[inline(never)]
    #[must_use] pub fn invalid_type_code(code: u8) -> Self {
        Error::InvalidTypeCode(code)
    }

    /// Create an `InvalidData` error. Marked cold to help branch prediction.
    #[cold]
    #[inline(never)]
    pub fn invalid_data(msg: impl Into<String>) -> Self {
        Error::InvalidData(msg.into())
    }

    /// Returns the standardized error type name for test matching.
    #[must_use] pub fn error_type(&self) -> &'static str {
        match self {
            Error::Truncated => "truncated",
            Error::TrailingBytes => "trailing_bytes",
            Error::InvalidTypeCode(_) => "invalid_type_code",
            Error::InvalidUtf8 => "invalid_utf8",
            Error::NulCharacter => "nul_character",
            Error::DuplicateKey => "duplicate_key",
            Error::UnclosedContainer => "unclosed_container",
            Error::InvalidData(_) => "invalid_data",
            Error::ValueOutOfRange => "value_out_of_range",
            Error::NonCanonicalLength => "non_canonical_length",
            Error::TooManyChunks => "too_many_chunks",
            Error::EmptyChunkContinuation => "empty_chunk_continuation",
            Error::MaxDepthExceeded => "max_depth_exceeded",
            Error::MaxStringLengthExceeded => "max_string_length_exceeded",
            Error::MaxContainerSizeExceeded => "max_container_size_exceeded",
            Error::MaxDocumentSizeExceeded => "max_document_size_exceeded",
            Error::UnbalancedContainers => "unbalanced_containers",
            Error::ExpectedObjectKey => "expected_object_key",
            Error::ExpectedObjectValue => "expected_object_value",
            Error::Io(_) => "io_error",
            Error::Custom(_) => "custom",
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Truncated => write!(f, "unexpected end of input"),
            Error::TrailingBytes => write!(f, "trailing bytes after document"),
            Error::InvalidTypeCode(code) => write!(f, "invalid type code: 0x{code:02x}"),
            Error::InvalidUtf8 => write!(f, "invalid UTF-8 sequence"),
            Error::NulCharacter => write!(f, "NUL character in string"),
            Error::DuplicateKey => write!(f, "duplicate key in object"),
            Error::UnclosedContainer => write!(f, "unclosed container"),
            Error::InvalidData(msg) => write!(f, "invalid data: {msg}"),
            Error::ValueOutOfRange => write!(f, "value out of range"),
            Error::NonCanonicalLength => write!(f, "non-canonical length encoding"),
            Error::TooManyChunks => write!(f, "too many string chunks"),
            Error::EmptyChunkContinuation => {
                write!(f, "empty chunk with continuation bit")
            }
            Error::MaxDepthExceeded => write!(f, "maximum container depth exceeded"),
            Error::MaxStringLengthExceeded => write!(f, "maximum string length exceeded"),
            Error::MaxContainerSizeExceeded => write!(f, "maximum container size exceeded"),
            Error::MaxDocumentSizeExceeded => write!(f, "maximum document size exceeded"),
            Error::UnbalancedContainers => write!(f, "tried to close too many containers"),
            Error::ExpectedObjectKey => write!(f, "expected object key (string)"),
            Error::ExpectedObjectValue => write!(f, "expected object value"),
            Error::Io(msg) => write!(f, "I/O error: {msg}"),
            Error::Custom(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for Error {}

impl serde::de::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Error::Custom(msg.to_string())
    }
}

impl serde::ser::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Error::Custom(msg.to_string())
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::Io(err.to_string())
    }
}

impl From<std::str::Utf8Error> for Error {
    fn from(_: std::str::Utf8Error) -> Self {
        Error::InvalidUtf8
    }
}
