//! Number parsing and C-compatible printing (`%1.15g` / `%1.17g`).
//!
//! cJSON stores both `valuedouble` and a saturating `valueint`. Printing uses
//! `"%d"` when the double equals `(double)valueint`, otherwise `sprintf("%1.15g")`
//! with a fallback to `"%1.17g"` when the 15-digit form does not round-trip
//! under `compare_double` (relative `DBL_EPSILON`). NaN and Infinity print as
//! `null` (`cJSON.c` `print_number`).

use crate::error::ParseError;

/// JSON number payload matching cJSON's `valuedouble` + `valueint` pair.
#[derive(Clone, Copy, Debug)]
pub struct Number {
    pub valuedouble: f64,
    pub valueint: i32,
}

impl Number {
    /// Build a number, saturating `valueint` to `INT_MIN`/`INT_MAX` like
    /// `cJSON_CreateNumber` / `parse_number`.
    pub fn from_double(d: f64) -> Self {
        Self {
            valuedouble: d,
            valueint: saturate_i32(d),
        }
    }
}

/// Saturate a double into `i32` the way cJSON does (`cJSON.c` around the
/// `item->valueint = INT_MAX` overflow branch).
pub fn saturate_i32(d: f64) -> i32 {
    if d >= i32::MAX as f64 {
        i32::MAX
    } else if d <= i32::MIN as f64 {
        i32::MIN
    } else {
        // C casts toward zero; `as i32` does the same for in-range finite values.
        d as i32
    }
}

/// Relative comparison used by cJSON (`compare_double` in `cJSON.c`).
pub fn compare_double(a: f64, b: f64) -> bool {
    let max_val = a.abs().max(b.abs());
    (a - b).abs() <= max_val * f64::EPSILON
}

/// Characters cJSON copies into the temporary buffer before `strtod`.
fn is_number_char(c: u8) -> bool {
    matches!(c, b'0'..=b'9' | b'+' | b'-' | b'e' | b'E' | b'.')
}

/// `strtod`-like scan of a buffer that contains only the cJSON number charset
/// (and a trailing NUL, typically). Returns the parsed value and how many
/// bytes `strtod` would consume.
fn strtod_compat(s: &[u8]) -> Option<(f64, usize)> {
    let mut i = 0;
    if i < s.len() && (s[i] == b'+' || s[i] == b'-') {
        i += 1;
    }
    let mut saw_digit = false;
    while i < s.len() && s[i].is_ascii_digit() {
        saw_digit = true;
        i += 1;
    }
    if i < s.len() && s[i] == b'.' {
        i += 1;
        while i < s.len() && s[i].is_ascii_digit() {
            saw_digit = true;
            i += 1;
        }
    }
    if !saw_digit {
        return None;
    }
    let mut end = i;
    if i < s.len() && (s[i] == b'e' || s[i] == b'E') {
        let mut j = i + 1;
        if j < s.len() && (s[j] == b'+' || s[j] == b'-') {
            j += 1;
        }
        let exp_digits = j;
        while j < s.len() && s[j].is_ascii_digit() {
            j += 1;
        }
        if j > exp_digits {
            end = j;
        }
    }
    let bytes = &s[..end];
    let text = core::str::from_utf8(bytes).ok()?;
    let to_parse = text.strip_prefix('+').unwrap_or(text);
    let value: f64 = to_parse.parse().ok()?;
    Some((value, end))
}

/// Parse a cJSON number from `input`, starting at offset 0 of the slice.
/// `input` should be the remainder of the parse buffer (including a trailing
/// NUL when matching `cJSON_Parse`).
pub fn parse_number(input: &[u8]) -> Result<(Number, usize), ParseError> {
    if input.is_empty() {
        return Err(ParseError::at(0));
    }
    let mut copied = 0usize;
    while copied < input.len() && is_number_char(input[copied]) {
        copied += 1;
    }
    let slice = &input[..copied];
    match strtod_compat(slice) {
        Some((value, consumed)) if consumed > 0 => Ok((Number::from_double(value), consumed)),
        _ => Err(ParseError::at(0)),
    }
}

/// Format `d` like glibc `sprintf("%1.*g", precision, d)` for 15 or 17.
fn format_sprintf_g(d: f64, precision: usize) -> String {
    let negative = d.is_sign_negative() && d != 0.0;
    let abs = if d == 0.0 { 0.0_f64 } else { d.abs() };
    let digits_after = precision.saturating_sub(1);
    let sci = format!("{:.*e}", digits_after, abs);
    let Some(e_pos) = sci.find('e') else {
        return sci;
    };
    let mantissa = &sci[..e_pos];
    let Ok(exp) = sci[e_pos + 1..].parse::<i32>() else {
        return sci;
    };
    let mut digit_str: String = mantissa.chars().filter(|c| c.is_ascii_digit()).collect();
    if digit_str.is_empty() {
        return if negative { format!("-{sci}") } else { sci };
    }
    let x = exp;
    let p = precision as i32;
    let mut out = String::new();
    if negative {
        out.push('-');
    }
    if x < -4 || x >= p {
        let mut m = digit_str;
        while m.len() > 1 && m.ends_with('0') {
            m.pop();
        }
        if let Some(first) = m.as_bytes().first().copied() {
            out.push(first as char);
        }
        if m.len() > 1 {
            out.push('.');
            out.push_str(&m[1..]);
        }
        out.push('e');
        if x < 0 {
            out.push('-');
        } else {
            out.push('+');
        }
        let ae = x.unsigned_abs();
        if ae < 10 {
            out.push('0');
        }
        out.push_str(&ae.to_string());
    } else if x >= 0 {
        let int_len = (x as usize) + 1;
        while digit_str.len() < int_len {
            digit_str.push('0');
        }
        out.push_str(&digit_str[..int_len]);
        let mut frac = digit_str[int_len.min(digit_str.len())..].to_string();
        while frac.ends_with('0') {
            frac.pop();
        }
        if !frac.is_empty() {
            out.push('.');
            out.push_str(&frac);
        }
    } else {
        out.push('0');
        out.push('.');
        let zeros = ((-x) as usize) - 1;
        for _ in 0..zeros {
            out.push('0');
        }
        let mut m = digit_str;
        while m.ends_with('0') {
            m.pop();
        }
        out.push_str(&m);
    }
    out
}

/// Render a number the way `print_number` in `cJSON.c` does.
pub fn print_number(n: &Number) -> String {
    let d = n.valuedouble;
    if d.is_nan() || d.is_infinite() {
        return "null".to_string();
    }
    if d == n.valueint as f64 {
        return n.valueint.to_string();
    }
    let s15 = format_sprintf_g(d, 15);
    match s15.parse::<f64>() {
        Ok(test) if compare_double(test, d) => s15,
        _ => format_sprintf_g(d, 17),
    }
}

/// Convenience wrapper using saturating `valueint`.
pub fn print_double(d: f64) -> String {
    print_number(&Number::from_double(d))
}
