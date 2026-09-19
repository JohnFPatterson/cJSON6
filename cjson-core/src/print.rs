//! JSON printer matching cJSON compact and pretty (`\t`-indented) output.

use crate::error::PrintError;
use crate::number::print_number;
use crate::string::print_string;
use crate::value::{Type, Value};
use crate::NESTING_LIMIT;

fn print_value(
    item: &Value,
    format: bool,
    depth: usize,
    out: &mut Vec<u8>,
) -> Result<(), PrintError> {
    match item.kind {
        Type::Null => out.extend_from_slice(b"null"),
        Type::False => out.extend_from_slice(b"false"),
        Type::True => out.extend_from_slice(b"true"),
        Type::Number => {
            let n = crate::number::Number {
                valuedouble: item.valuedouble,
                valueint: item.valueint,
            };
            out.extend_from_slice(print_number(&n).as_bytes());
        }
        Type::Raw => {
            let Some(raw) = item.valuestring.as_deref() else {
                return Err(PrintError::InvalidValue);
            };
            let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
            out.extend_from_slice(&raw[..end]);
        }
        Type::String => {
            out.extend(print_string(item.valuestring.as_deref()));
        }
        Type::Array => print_array(item, format, depth, out)?,
        Type::Object => print_object(item, format, depth, out)?,
        Type::Invalid => return Err(PrintError::InvalidValue),
    }
    Ok(())
}

fn print_array(
    item: &Value,
    format: bool,
    depth: usize,
    out: &mut Vec<u8>,
) -> Result<(), PrintError> {
    if depth >= NESTING_LIMIT {
        return Err(PrintError::NestingTooDeep);
    }
    out.push(b'[');
    let depth = depth + 1;
    for (i, child) in item.children.iter().enumerate() {
        print_value(child, format, depth, out)?;
        if i + 1 < item.children.len() {
            out.push(b',');
            if format {
                out.push(b' ');
            }
        }
    }
    out.push(b']');
    Ok(())
}

fn print_object(
    item: &Value,
    format: bool,
    depth: usize,
    out: &mut Vec<u8>,
) -> Result<(), PrintError> {
    if depth >= NESTING_LIMIT {
        return Err(PrintError::NestingTooDeep);
    }
    out.push(b'{');
    let depth = depth + 1;
    if format {
        out.push(b'\n');
    }
    for (i, child) in item.children.iter().enumerate() {
        if format {
            for _ in 0..depth {
                out.push(b'\t');
            }
        }
        out.extend(print_string(child.name.as_deref()));
        out.push(b':');
        if format {
            out.push(b'\t');
        }
        print_value(child, format, depth, out)?;
        if i + 1 < item.children.len() {
            out.push(b',');
        }
        if format {
            out.push(b'\n');
        }
    }
    if format {
        for _ in 0..depth.saturating_sub(1) {
            out.push(b'\t');
        }
    }
    out.push(b'}');
    Ok(())
}

/// Pretty-print (`cJSON_Print`): objects use tabs and newlines.
pub fn print(item: &Value) -> Result<Vec<u8>, PrintError> {
    let mut out = Vec::new();
    print_value(item, true, 0, &mut out)?;
    Ok(out)
}

/// Compact print (`cJSON_PrintUnformatted`).
pub fn print_unformatted(item: &Value) -> Result<Vec<u8>, PrintError> {
    let mut out = Vec::new();
    print_value(item, false, 0, &mut out)?;
    Ok(out)
}

/// Pretty-print into a `String`. Fails if the output is not valid UTF-8
/// (cJSON can emit raw non-UTF-8 bytes from string values).
pub fn print_to_string(item: &Value) -> Result<String, PrintError> {
    let bytes = print(item)?;
    String::from_utf8(bytes).map_err(|_| PrintError::InvalidValue)
}

/// Compact print into a `String`.
pub fn print_unformatted_to_string(item: &Value) -> Result<String, PrintError> {
    let bytes = print_unformatted(item)?;
    String::from_utf8(bytes).map_err(|_| PrintError::InvalidValue)
}
