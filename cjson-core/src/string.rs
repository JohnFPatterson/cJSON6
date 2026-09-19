//! JSON string parse/print, UTF-16 `\u` escapes, and `parse_hex4`.

use crate::error::ParseError;

/// Parse four hex digits, matching `parse_hex4` in `cJSON.c`.
/// Invalid digits return 0 (the C function cannot distinguish `\u0000` from
/// garbage); callers that have fewer than 4 bytes get 0 as well.
pub fn parse_hex4(input: &[u8]) -> u32 {
    if input.len() < 4 {
        return 0;
    }
    let mut h: u32 = 0;
    for (i, &b) in input.iter().take(4).enumerate() {
        let nibble = match b {
            b'0'..=b'9' => u32::from(b - b'0'),
            b'A'..=b'F' => 10 + u32::from(b - b'A'),
            b'a'..=b'f' => 10 + u32::from(b - b'a'),
            _ => return 0,
        };
        h += nibble;
        if i < 3 {
            h <<= 4;
        }
    }
    h
}

/// Convert one or two `\uXXXX` sequences to UTF-8 bytes.
/// Returns the input sequence length (6 or 12) on success.
fn utf16_literal_to_utf8(input: &[u8], output: &mut Vec<u8>) -> Option<usize> {
    if input.len() < 6 {
        return None;
    }
    let first_code = parse_hex4(&input[2..]);
    if (0xDC00..=0xDFFF).contains(&first_code) {
        return None;
    }
    let (codepoint, sequence_length) = if (0xD800..=0xDBFF).contains(&first_code) {
        if input.len() < 12 {
            return None;
        }
        if input[6] != b'\\' || input[7] != b'u' {
            return None;
        }
        let second_code = parse_hex4(&input[8..]);
        if !(0xDC00..=0xDFFF).contains(&second_code) {
            return None;
        }
        let cp = 0x10000 + (((first_code & 0x3FF) << 10) | (second_code & 0x3FF));
        (cp, 12usize)
    } else {
        (first_code, 6usize)
    };

    let mut buf = [0u8; 4];
    let n = match codepoint {
        0..=0x7F => {
            buf[0] = (codepoint & 0x7F) as u8;
            1
        }
        0x80..=0x7FF => {
            buf[0] = (0xC0 | ((codepoint >> 6) & 0x1F)) as u8;
            buf[1] = (0x80 | (codepoint & 0x3F)) as u8;
            2
        }
        0x800..=0xFFFF => {
            buf[0] = (0xE0 | ((codepoint >> 12) & 0x0F)) as u8;
            buf[1] = (0x80 | ((codepoint >> 6) & 0x3F)) as u8;
            buf[2] = (0x80 | (codepoint & 0x3F)) as u8;
            3
        }
        0x10000..=0x10FFFF => {
            buf[0] = (0xF0 | ((codepoint >> 18) & 0x07)) as u8;
            buf[1] = (0x80 | ((codepoint >> 12) & 0x3F)) as u8;
            buf[2] = (0x80 | ((codepoint >> 6) & 0x3F)) as u8;
            buf[3] = (0x80 | (codepoint & 0x3F)) as u8;
            4
        }
        _ => return None,
    };
    output.extend_from_slice(&buf[..n]);
    Some(sequence_length)
}

/// Parse a JSON string literal at the start of `input`. On success returns
/// the unescaped bytes and the number of input bytes consumed (including quotes).
pub fn parse_string(input: &[u8]) -> Result<(Vec<u8>, usize), ParseError> {
    if input.is_empty() || input[0] != b'"' {
        return Err(ParseError::at(0));
    }
    let mut input_end = 1usize;
    while input_end < input.len() && input[input_end] != b'"' {
        if input[input_end] == b'\\' {
            if input_end + 1 >= input.len() {
                return Err(ParseError::at(input_end));
            }
            input_end += 1;
        }
        input_end += 1;
    }
    if input_end >= input.len() || input[input_end] != b'"' {
        return Err(ParseError::at(input_end.min(input.len().saturating_sub(1))));
    }

    let mut output = Vec::new();
    let mut input_pointer = 1usize;
    while input_pointer < input_end {
        let b = input[input_pointer];
        if b != b'\\' {
            output.push(b);
            input_pointer += 1;
            continue;
        }
        if input_end.saturating_sub(input_pointer) < 1 {
            return Err(ParseError::at(input_pointer));
        }
        if input_pointer + 1 >= input_end {
            return Err(ParseError::at(input_pointer));
        }
        let mut sequence_length = 2usize;
        match input[input_pointer + 1] {
            b'b' => output.push(b'\x08'),
            b'f' => output.push(b'\x0c'),
            b'n' => output.push(b'\n'),
            b'r' => output.push(b'\r'),
            b't' => output.push(b'\t'),
            b'"' | b'\\' | b'/' => output.push(input[input_pointer + 1]),
            b'u' => match utf16_literal_to_utf8(&input[input_pointer..input_end], &mut output) {
                Some(n) => sequence_length = n,
                None => return Err(ParseError::at(input_pointer)),
            },
            _ => return Err(ParseError::at(input_pointer)),
        }
        input_pointer += sequence_length;
    }
    Ok((output, input_end + 1))
}

/// Escape `input` as a JSON string literal, matching `print_string_ptr`.
/// A `None` input prints as `""` (cJSON treats a NULL pointer as empty).
/// Printing stops at the first interior NUL, matching C `strlen` semantics.
pub fn print_string(input: Option<&[u8]>) -> Vec<u8> {
    let Some(input) = input else {
        return b"\"\"".to_vec();
    };
    // C walks until '\0'.
    let end = input.iter().position(|&b| b == 0).unwrap_or(input.len());
    let input = &input[..end];

    let mut escape_characters = 0usize;
    for &c in input {
        match c {
            b'"' | b'\\' | b'\x08' | b'\x0c' | b'\n' | b'\r' | b'\t' => {
                escape_characters += 1;
            }
            c if c < 32 => {
                escape_characters += 5;
            }
            _ => {}
        }
    }
    let mut output = Vec::with_capacity(input.len() + escape_characters + 2);
    output.push(b'"');
    if escape_characters == 0 {
        output.extend_from_slice(input);
        output.push(b'"');
        return output;
    }
    for &c in input {
        if c > 31 && c != b'"' && c != b'\\' {
            output.push(c);
        } else {
            output.push(b'\\');
            match c {
                b'\\' => output.push(b'\\'),
                b'"' => output.push(b'"'),
                b'\x08' => output.push(b'b'),
                b'\x0c' => output.push(b'f'),
                b'\n' => output.push(b'n'),
                b'\r' => output.push(b'r'),
                b'\t' => output.push(b't'),
                _ => {
                    output.push(b'u');
                    let hex = format!("{c:04x}");
                    output.extend_from_slice(hex.as_bytes());
                }
            }
        }
    }
    output.push(b'"');
    output
}

/// ASCII-only `tolower` used by `cJSON_GetObjectItem` (C locale).
pub fn c_tolower(c: u8) -> u8 {
    if c.is_ascii_uppercase() {
        c + 32
    } else {
        c
    }
}

/// Case-insensitive comparison matching `case_insensitive_strcmp`.
/// Two empty slices are equal; a missing key (represented as `None`) is never
/// equal, matching C's NULL-pointer handling.
pub fn case_insensitive_eq(a: Option<&[u8]>, b: Option<&[u8]>) -> bool {
    let (Some(a), Some(b)) = (a, b) else {
        return false;
    };
    if std::ptr::eq(a, b) {
        return true;
    }
    let a_end = a.iter().position(|&c| c == 0).unwrap_or(a.len());
    let b_end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    let a = &a[..a_end];
    let b = &b[..b_end];
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .all(|(x, y)| c_tolower(*x) == c_tolower(*y))
}

/// Case-sensitive equality that stops at interior NUL, like `strcmp`.
pub fn c_str_eq(a: Option<&[u8]>, b: Option<&[u8]>) -> bool {
    let (Some(a), Some(b)) = (a, b) else {
        return false;
    };
    let a_end = a.iter().position(|&c| c == 0).unwrap_or(a.len());
    let b_end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    a[..a_end] == b[..b_end]
}
