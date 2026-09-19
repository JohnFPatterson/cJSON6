//! Error types for the cJSON Rust port.
//!
//! Malformed input never panics; parsers and printers return these errors.

use std::fmt;

/// Position of a parse failure inside the input buffer, matching `cJSON_GetErrorPtr`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParseError {
    /// Byte offset into the parse buffer (which may include a trailing NUL).
    pub position: usize,
}

impl ParseError {
    pub(crate) fn at(position: usize) -> Self {
        Self { position }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cJSON parse error at position {}", self.position)
    }
}

impl std::error::Error for ParseError {}

/// Printing failed, typically because nesting exceeded [`crate::NESTING_LIMIT`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrintError {
    /// Array/object nesting reached `CJSON_NESTING_LIMIT` (1000).
    NestingTooDeep,
    /// A raw item had no payload, or the value was `Invalid`.
    InvalidValue,
}

impl fmt::Display for PrintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrintError::NestingTooDeep => write!(f, "cJSON print error: nesting too deep"),
            PrintError::InvalidValue => write!(f, "cJSON print error: invalid value"),
        }
    }
}

impl std::error::Error for PrintError {}
