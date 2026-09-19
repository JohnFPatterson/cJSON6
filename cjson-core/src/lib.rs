//! Safe Rust port of cJSON 1.7.19.
//!
//! This crate contains all parse/print/tree logic and forbids `unsafe`.

#![forbid(unsafe_code)]

mod compare;
mod error;
mod minify;
mod number;
mod parse;
mod print;
mod string;
mod value;

pub use compare::compare;
pub use error::{ParseError, PrintError};
pub use minify::{minify, minify_bytes, minify_in_place};
pub use number::{compare_double, parse_number, print_double, print_number, saturate_i32, Number};
pub use parse::{
    parse, parse_array_at, parse_object_at, parse_string_at, parse_value_at, parse_with_length,
    parse_with_opts, ParseOptions, ParseSuccess,
};
pub use print::{print, print_to_string, print_unformatted, print_unformatted_to_string};
pub use string::{parse_hex4, parse_string, print_string};
pub use value::{Type, Value};

/// `CJSON_VERSION_*`
pub const VERSION_MAJOR: u32 = 1;
pub const VERSION_MINOR: u32 = 7;
pub const VERSION_PATCH: u32 = 19;
pub const VERSION: &str = "1.7.19";

/// `CJSON_NESTING_LIMIT`
pub const NESTING_LIMIT: usize = 1000;
/// `CJSON_CIRCULAR_LIMIT`
pub const CIRCULAR_LIMIT: usize = 10000;
