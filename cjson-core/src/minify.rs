//! In-place minify matching `cJSON_Minify`: strip whitespace and `//` / `/* */`
//! comments, leaving string contents intact.

/// Minify `json` in place and return the new length (not including a trailing
/// NUL; callers that want C semantics should write `buf[len] = 0`).
pub fn minify_bytes(buf: &mut [u8]) -> usize {
    if buf.is_empty() {
        return 0;
    }
    let mut json = 0usize;
    let mut into = 0usize;
    let len = buf.len();

    while json < len && buf[json] != 0 {
        match buf[json] {
            b' ' | b'\t' | b'\r' | b'\n' => {
                json += 1;
            }
            b'/' => {
                let next = buf.get(json + 1).copied().unwrap_or(0);
                if next == b'/' {
                    skip_oneline_comment(buf, &mut json);
                } else if next == b'*' {
                    skip_multiline_comment(buf, &mut json);
                } else {
                    json += 1;
                }
            }
            b'"' => {
                minify_string(buf, &mut json, &mut into);
            }
            _ => {
                buf[into] = buf[json];
                json += 1;
                into += 1;
            }
        }
    }
    if into < len {
        buf[into] = 0;
    }
    into
}

fn skip_oneline_comment(buf: &[u8], json: &mut usize) {
    *json += 2; // "//"
    let len = buf.len();
    while *json < len && buf[*json] != 0 {
        if buf[*json] == b'\n' {
            *json += 1;
            return;
        }
        *json += 1;
    }
}

fn skip_multiline_comment(buf: &[u8], json: &mut usize) {
    *json += 2; // "/*"
    let len = buf.len();
    while *json < len && buf[*json] != 0 {
        if buf[*json] == b'*' && buf.get(*json + 1).copied() == Some(b'/') {
            *json += 2;
            return;
        }
        *json += 1;
    }
}

fn minify_string(buf: &mut [u8], json: &mut usize, into: &mut usize) {
    buf[*into] = buf[*json];
    *json += 1;
    *into += 1;
    let len = buf.len();
    while *json < len && buf[*json] != 0 {
        buf[*into] = buf[*json];
        if buf[*json] == b'"' {
            buf[*into] = b'"';
            *json += 1;
            *into += 1;
            return;
        } else if buf[*json] == b'\\' && buf.get(*json + 1).copied() == Some(b'"') {
            if *into + 1 < len {
                buf[*into + 1] = buf[*json + 1];
            }
            *json += 1;
            *into += 1;
        }
        if *json < len {
            *json += 1;
        }
        *into += 1;
        if *into >= len {
            break;
        }
    }
}

/// Minify a UTF-8 string, returning a new compact string.
pub fn minify(json: &str) -> String {
    let mut buf = json.as_bytes().to_vec();
    if buf.is_empty() {
        return String::new();
    }
    buf.push(0);
    let n = minify_bytes(&mut buf);
    buf.truncate(n);
    String::from_utf8(buf).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

/// Minify a `String` in place (C `cJSON_Minify` analogue for owned buffers).
pub fn minify_in_place(json: &mut String) {
    let mut buf = json.as_bytes().to_vec();
    if buf.is_empty() {
        return;
    }
    buf.push(0);
    let n = minify_bytes(&mut buf);
    buf.truncate(n);
    match String::from_utf8(buf) {
        Ok(s) => *json = s,
        Err(e) => *json = String::from_utf8_lossy(e.as_bytes()).into_owned(),
    }
}
