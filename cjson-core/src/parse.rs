//! Recursive-descent parser matching `parse_value` / `parse_array` / `parse_object`.

use crate::error::ParseError;
use crate::number::parse_number;
use crate::string::parse_string;
use crate::value::{Type, Value};
use crate::NESTING_LIMIT;

/// Successful parse: the value plus the offset of the first unused byte
/// (`return_parse_end` in `cJSON_ParseWithOpts`).
#[derive(Clone, Debug)]
pub struct ParseSuccess {
    pub value: Value,
    pub parse_end: usize,
}

/// Options for [`parse_with_opts`].
#[derive(Clone, Copy, Debug, Default)]
pub struct ParseOptions {
    /// Reject trailing non-whitespace after the value (and require a NUL in
    /// the buffer at that point), matching `require_null_terminated`.
    pub require_null_terminated: bool,
    /// Skip a leading UTF-8 BOM when `offset == 0`.
    pub skip_bom: bool,
}

struct Parser<'a> {
    content: &'a [u8],
    offset: usize,
    depth: usize,
}

impl<'a> Parser<'a> {
    fn new(content: &'a [u8]) -> Self {
        Self {
            content,
            offset: 0,
            depth: 0,
        }
    }

    fn can_read(&self, size: usize) -> bool {
        self.offset.saturating_add(size) <= self.content.len()
    }

    fn can_access(&self, index: usize) -> bool {
        self.offset.saturating_add(index) < self.content.len()
    }

    fn cannot_access(&self, index: usize) -> bool {
        !self.can_access(index)
    }

    fn byte0(&self) -> Option<u8> {
        self.content.get(self.offset).copied()
    }

    fn starts_with(&self, needle: &[u8]) -> bool {
        self.can_read(needle.len()) && self.content[self.offset..].starts_with(needle)
    }

    fn skip_whitespace(&mut self) {
        if self.content.is_empty() {
            return;
        }
        if self.cannot_access(0) {
            return;
        }
        while self.can_access(0) && self.byte0().is_some_and(|b| b <= 32) {
            self.offset += 1;
        }
        if self.offset == self.content.len() && self.offset > 0 {
            self.offset -= 1;
        }
    }

    fn skip_utf8_bom(&mut self) {
        if self.offset != 0 {
            return;
        }
        // cJSON requires can_access_at_index(..., 4), i.e. at least 5 bytes,
        // before skipping a 3-byte BOM (`cJSON.c` `skip_utf8_bom`).
        if self.can_access(4) && self.content.starts_with(&[0xEF, 0xBB, 0xBF]) {
            self.offset += 3;
        }
    }

    fn fail_pos(&self) -> usize {
        if self.offset < self.content.len() {
            self.offset
        } else if self.content.is_empty() {
            0
        } else {
            self.content.len() - 1
        }
    }

    fn parse_value(&mut self) -> Result<Value, ParseError> {
        if self.content.is_empty() {
            return Err(ParseError::at(self.fail_pos()));
        }
        if self.starts_with(b"null") {
            self.offset += 4;
            return Ok(Value::null());
        }
        if self.starts_with(b"false") {
            self.offset += 5;
            return Ok(Value::boolean(false));
        }
        if self.starts_with(b"true") {
            self.offset += 4;
            return Ok(Value::boolean(true));
        }
        if self.can_access(0) && self.byte0() == Some(b'"') {
            return self.parse_string_value();
        }
        if self.can_access(0)
            && self
                .byte0()
                .is_some_and(|b| b == b'-' || b.is_ascii_digit())
        {
            return self.parse_number_value();
        }
        if self.can_access(0) && self.byte0() == Some(b'[') {
            return self.parse_array();
        }
        if self.can_access(0) && self.byte0() == Some(b'{') {
            return self.parse_object();
        }
        Err(ParseError::at(self.fail_pos()))
    }

    fn parse_string_value(&mut self) -> Result<Value, ParseError> {
        let rest = &self.content[self.offset..];
        match parse_string(rest) {
            Ok((bytes, consumed)) => {
                self.offset += consumed;
                Ok(Value::string_bytes(bytes))
            }
            Err(e) => {
                self.offset += e.position;
                Err(ParseError::at(self.fail_pos()))
            }
        }
    }

    fn parse_number_value(&mut self) -> Result<Value, ParseError> {
        let rest = &self.content[self.offset..];
        match parse_number(rest) {
            Ok((n, consumed)) => {
                self.offset += consumed;
                Ok(Value::from_number(n))
            }
            Err(_) => Err(ParseError::at(self.fail_pos())),
        }
    }

    fn parse_array(&mut self) -> Result<Value, ParseError> {
        if self.depth >= NESTING_LIMIT {
            return Err(ParseError::at(self.fail_pos()));
        }
        self.depth += 1;
        if self.byte0() != Some(b'[') {
            self.depth -= 1;
            return Err(ParseError::at(self.fail_pos()));
        }
        self.offset += 1;
        self.skip_whitespace();
        if self.can_access(0) && self.byte0() == Some(b']') {
            self.depth -= 1;
            self.offset += 1;
            return Ok(Value::array());
        }
        if self.cannot_access(0) {
            if self.offset > 0 {
                self.offset -= 1;
            }
            self.depth -= 1;
            return Err(ParseError::at(self.fail_pos()));
        }
        if self.offset > 0 {
            self.offset -= 1;
        }
        let mut children = Vec::new();
        loop {
            self.offset += 1;
            self.skip_whitespace();
            match self.parse_value() {
                Ok(v) => children.push(v),
                Err(e) => {
                    self.depth -= 1;
                    return Err(e);
                }
            }
            self.skip_whitespace();
            if !(self.can_access(0) && self.byte0() == Some(b',')) {
                break;
            }
        }
        if self.cannot_access(0) || self.byte0() != Some(b']') {
            self.depth -= 1;
            return Err(ParseError::at(self.fail_pos()));
        }
        self.depth -= 1;
        self.offset += 1;
        Ok(Value {
            kind: Type::Array,
            valuestring: None,
            valueint: 0,
            valuedouble: 0.0,
            children,
            name: None,
        })
    }

    fn parse_object(&mut self) -> Result<Value, ParseError> {
        if self.depth >= NESTING_LIMIT {
            return Err(ParseError::at(self.fail_pos()));
        }
        self.depth += 1;
        if self.cannot_access(0) || self.byte0() != Some(b'{') {
            self.depth -= 1;
            return Err(ParseError::at(self.fail_pos()));
        }
        self.offset += 1;
        self.skip_whitespace();
        if self.can_access(0) && self.byte0() == Some(b'}') {
            self.depth -= 1;
            self.offset += 1;
            return Ok(Value::object());
        }
        if self.cannot_access(0) {
            if self.offset > 0 {
                self.offset -= 1;
            }
            self.depth -= 1;
            return Err(ParseError::at(self.fail_pos()));
        }
        if self.offset > 0 {
            self.offset -= 1;
        }
        let mut children = Vec::new();
        loop {
            if self.cannot_access(1) {
                self.depth -= 1;
                return Err(ParseError::at(self.fail_pos()));
            }
            self.offset += 1;
            self.skip_whitespace();
            let key = match self.parse_string_value() {
                Ok(v) => v.valuestring.unwrap_or_default(),
                Err(e) => {
                    self.depth -= 1;
                    return Err(e);
                }
            };
            self.skip_whitespace();
            if self.cannot_access(0) || self.byte0() != Some(b':') {
                self.depth -= 1;
                return Err(ParseError::at(self.fail_pos()));
            }
            self.offset += 1;
            self.skip_whitespace();
            let mut value = match self.parse_value() {
                Ok(v) => v,
                Err(e) => {
                    self.depth -= 1;
                    return Err(e);
                }
            };
            value.name = Some(key);
            children.push(value);
            self.skip_whitespace();
            if !(self.can_access(0) && self.byte0() == Some(b',')) {
                break;
            }
        }
        if self.cannot_access(0) || self.byte0() != Some(b'}') {
            self.depth -= 1;
            return Err(ParseError::at(self.fail_pos()));
        }
        self.depth -= 1;
        self.offset += 1;
        Ok(Value {
            kind: Type::Object,
            valuestring: None,
            valueint: 0,
            valuedouble: 0.0,
            children,
            name: None,
        })
    }
}

/// Parse a NUL-terminated JSON document, matching `cJSON_Parse`.
pub fn parse(text: &str) -> Result<Value, ParseError> {
    let mut buf = Vec::with_capacity(text.len() + 1);
    buf.extend_from_slice(text.as_bytes());
    buf.push(0);
    parse_with_opts(&buf, ParseOptions::default()).map(|s| s.value)
}

/// Parse a buffer of known length, matching `cJSON_ParseWithLength`.
pub fn parse_with_length(buffer: &[u8]) -> Result<Value, ParseError> {
    parse_with_opts(buffer, ParseOptions::default()).map(|s| s.value)
}

/// Parse with explicit options, matching `cJSON_ParseWithLengthOpts`.
pub fn parse_with_opts(buffer: &[u8], opts: ParseOptions) -> Result<ParseSuccess, ParseError> {
    if buffer.is_empty() {
        return Err(ParseError::at(0));
    }
    let mut parser = Parser::new(buffer);
    // cJSON always tries to skip a BOM at offset 0 (`skip_utf8_bom`).
    let _ = opts.skip_bom;
    parser.skip_utf8_bom();
    parser.skip_whitespace();
    let value = parser.parse_value()?;
    if opts.require_null_terminated {
        parser.skip_whitespace();
        if parser.offset >= parser.content.len() || parser.byte0() != Some(0) {
            return Err(ParseError::at(parser.fail_pos()));
        }
    }
    Ok(ParseSuccess {
        value,
        parse_end: parser.offset,
    })
}

/// Parse a single JSON value from a buffer that already includes a trailing
/// NUL (as the Unity tests do when they call `parse_value` directly).
pub fn parse_value_at(buffer: &[u8]) -> Result<(Value, usize), ParseError> {
    if buffer.is_empty() {
        return Err(ParseError::at(0));
    }
    let mut parser = Parser::new(buffer);
    let value = parser.parse_value()?;
    Ok((value, parser.offset))
}

/// Direct array parse used by the Unity `parse_array` tests.
pub fn parse_array_at(buffer: &[u8]) -> Result<(Value, usize), ParseError> {
    let mut parser = Parser::new(buffer);
    let value = parser.parse_array()?;
    Ok((value, parser.offset))
}

/// Direct object parse used by the Unity `parse_object` tests.
pub fn parse_object_at(buffer: &[u8]) -> Result<(Value, usize), ParseError> {
    let mut parser = Parser::new(buffer);
    let value = parser.parse_object()?;
    Ok((value, parser.offset))
}

/// Direct string parse used by the Unity `parse_string` tests.
pub fn parse_string_at(buffer: &[u8]) -> Result<(Vec<u8>, usize), ParseError> {
    parse_string(buffer)
}
