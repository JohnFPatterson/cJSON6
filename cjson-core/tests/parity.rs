//! Differential harness: every file in `tests/inputs/` is parsed and printed
//! by the original C library and by `cjson-core`. Results are written to
//! `PARITY.md`.

use cjson_core::{parse, print, print_unformatted};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

struct OracleResult {
    ok: bool,
    #[allow(dead_code)]
    error_pos: Option<i64>,
    unformatted: Option<Vec<u8>>,
    formatted: Option<Vec<u8>>,
    #[allow(dead_code)]
    print_fail: bool,
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn oracle_bin() -> PathBuf {
    workspace_root().join("target/cjson_oracle")
}

fn build_oracle() {
    let root = workspace_root();
    let out = oracle_bin();
    if let Some(parent) = out.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let status = Command::new("gcc")
        .args(["-O0", "-g", "-std=c89", "-o"])
        .arg(&out)
        .arg(root.join("tools/cjson_oracle.c"))
        .arg(root.join("cJSON.c"))
        .arg("-I")
        .arg(&root)
        .arg("-lm")
        .status()
        .expect("spawn gcc");
    assert!(status.success(), "failed to compile C oracle");
}

fn parse_oracle_stdout(stdout: &[u8]) -> OracleResult {
    let mut lines = stdout.split(|&b| b == b'\n');
    let first = lines.next().unwrap_or(b"");
    if first == b"FAIL" {
        let pos_line = lines.next().unwrap_or(b"");
        let pos = std::str::from_utf8(pos_line)
            .ok()
            .and_then(|s| s.strip_prefix("POS "))
            .and_then(|s| s.trim().parse().ok());
        return OracleResult {
            ok: false,
            error_pos: pos,
            unformatted: None,
            formatted: None,
            print_fail: false,
        };
    }
    if first == b"PRINT_FAIL" {
        return OracleResult {
            ok: true,
            error_pos: None,
            unformatted: None,
            formatted: None,
            print_fail: true,
        };
    }
    let mut rest = &stdout[first.len() + 1..];
    let parse_one = |input: &[u8]| -> Option<(Vec<u8>, usize)> {
        let nl = input.iter().position(|&b| b == b'\n')?;
        let header = std::str::from_utf8(&input[..nl]).ok()?;
        let (_tag, len_s) = header.split_once(' ')?;
        let len: usize = len_s.trim().parse().ok()?;
        let body_start = nl + 1;
        let _body = input.get(body_start..body_start + len)?;
        let mut consumed = body_start + len;
        if input.get(consumed) == Some(&b'\n') {
            consumed += 1;
        }
        Some((input[body_start..body_start + len].to_vec(), consumed))
    };
    let (unformatted, n) = parse_one(rest).expect("oracle U blob");
    rest = &rest[n..];
    let (formatted, _) = parse_one(rest).expect("oracle F blob");
    OracleResult {
        ok: true,
        error_pos: None,
        unformatted: Some(unformatted),
        formatted: Some(formatted),
        print_fail: false,
    }
}

fn run_c_oracle(path: &Path) -> OracleResult {
    let output = Command::new(oracle_bin())
        .arg(path)
        .output()
        .expect("run oracle");
    assert!(
        output.status.success(),
        "oracle failed for {}: {}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    parse_oracle_stdout(&output.stdout)
}

fn run_rust(path: &Path) -> OracleResult {
    let bytes = fs::read(path).expect("read input");
    // cJSON_Parse uses strlen, so stop at first NUL like C.
    let cstr_end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let text = match std::str::from_utf8(&bytes[..cstr_end]) {
        Ok(s) => s,
        Err(_) => {
            // C still parses raw bytes via strlen; feed lossy only if we must.
            // Use parse_with_length on the raw bytes including a synthetic NUL
            // to match cJSON_Parse's strlen+1 buffer.
            let mut buf = bytes[..cstr_end].to_vec();
            buf.push(0);
            return match cjson_core::parse_with_opts(
                &buf,
                cjson_core::ParseOptions {
                    require_null_terminated: false,
                    skip_bom: true,
                },
            ) {
                Ok(success) => rust_print(&success.value),
                Err(_) => OracleResult {
                    ok: false,
                    error_pos: None,
                    unformatted: None,
                    formatted: None,
                    print_fail: false,
                },
            };
        }
    };
    match parse(text) {
        Ok(v) => rust_print(&v),
        Err(_) => OracleResult {
            ok: false,
            error_pos: None,
            unformatted: None,
            formatted: None,
            print_fail: false,
        },
    }
}

fn rust_print(v: &cjson_core::Value) -> OracleResult {
    match (print_unformatted(v), print(v)) {
        (Ok(u), Ok(f)) => OracleResult {
            ok: true,
            error_pos: None,
            unformatted: Some(u),
            formatted: Some(f),
            print_fail: false,
        },
        _ => OracleResult {
            ok: true,
            error_pos: None,
            unformatted: None,
            formatted: None,
            print_fail: true,
        },
    }
}

struct FileReport {
    name: String,
    c_ok: bool,
    rust_ok: bool,
    compact_match: bool,
    pretty_match: bool,
    notes: String,
}

#[test]
fn differential_harness_inputs() {
    build_oracle();
    let inputs = workspace_root().join("tests/inputs");
    let mut files: Vec<_> = fs::read_dir(&inputs)
        .expect("inputs dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    files.sort();

    let mut reports = Vec::new();
    let mut divergences = 0usize;

    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let c = run_c_oracle(path);
        let r = run_rust(path);
        let mut notes = String::new();
        let compact_match = match (&c.unformatted, &r.unformatted) {
            (Some(a), Some(b)) => a == b,
            (None, None) => c.ok == r.ok,
            _ => false,
        };
        let pretty_match = match (&c.formatted, &r.formatted) {
            (Some(a), Some(b)) => a == b,
            (None, None) => c.ok == r.ok,
            _ => false,
        };
        if c.ok != r.ok {
            notes.push_str(&format!(
                "parse success diverges (C ok={}, Rust ok={}); ",
                c.ok, r.ok
            ));
        }
        if c.ok && r.ok && !compact_match {
            notes.push_str("compact print mismatch; ");
        }
        if c.ok && r.ok && !pretty_match {
            notes.push_str("pretty print mismatch; ");
        }
        if !c.ok && !r.ok {
            notes.push_str("both failed to parse (expected for non-JSON such as test6)");
        }
        if !notes.is_empty() && (c.ok != r.ok || !compact_match || !pretty_match) {
            divergences += 1;
        }
        reports.push(FileReport {
            name,
            c_ok: c.ok,
            rust_ok: r.ok,
            compact_match,
            pretty_match,
            notes,
        });
    }

    let mut md = String::new();
    md.push_str("# cJSON C vs Rust parity\n\n");
    md.push_str("Differential run of every file in `tests/inputs/` through the original C library (`cJSON.c`) and the Rust `cjson-core` port.\n\n");
    md.push_str("## Method\n\n");
    md.push_str("- **C:** `cJSON_Parse` + `cJSON_PrintUnformatted` + `cJSON_Print` (oracle at `tools/cjson_oracle.c`).\n");
    md.push_str("- **Rust:** `cjson_core::parse` + `print_unformatted` + `print`.\n");
    md.push_str("- Printed output is compared as raw bytes.\n\n");
    md.push_str(&format!(
        "## Summary\n\n{} files, {} parse/print divergences.\n\n",
        reports.len(),
        divergences
    ));
    md.push_str("| File | C parse | Rust parse | Compact identical | Pretty identical | Notes |\n");
    md.push_str("|------|---------|------------|-------------------|------------------|-------|\n");
    for r in &reports {
        md.push_str(&format!(
            "| `{}` | {} | {} | {} | {} | {} |\n",
            r.name,
            if r.c_ok { "ok" } else { "fail" },
            if r.rust_ok { "ok" } else { "fail" },
            if r.compact_match { "yes" } else { "NO" },
            if r.pretty_match { "yes" } else { "NO" },
            r.notes.replace('|', "\\|")
        ));
    }
    md.push_str("\n## Documented C quirks (not divergences)\n\n");
    md.push_str("These behaviors were **matched** by the Rust port; they are listed so a reader can find the C source that specifies them.\n\n");
    md.push_str("| Behavior | C source |\n");
    md.push_str("|----------|----------|\n");
    md.push_str("| `cJSON_GetObjectItem` is case-insensitive; `CaseSensitive` is not | `cJSON.c` `get_object_item` / `case_insensitive_strcmp` |\n");
    md.push_str("| Objects preserve insertion order and allow duplicate keys | `parse_object` appends to the child linked list; no key uniquing |\n");
    md.push_str("| `cJSON_Compare` looks up object members by key, so duplicate keys compare against the **first** match | `cJSON.c` `cJSON_Compare` calling `get_object_item` |\n");
    md.push_str("| Floats print with `sprintf(\"%1.15g\")`, falling back to `\"%1.17g\"` if `compare_double` fails | `cJSON.c` `print_number` |\n");
    md.push_str(
        "| Integers that equal `(double)valueint` print with `%d` | `cJSON.c` `print_number` |\n",
    );
    md.push_str(
        "| NaN and Infinity print as `null` | `cJSON.c` `print_number` (`isnan` / `isinf`) |\n",
    );
    md.push_str("| Nesting deeper than `CJSON_NESTING_LIMIT` (1000) is rejected | `parse_array` / `parse_object` / `print_array` / `print_object` |\n");
    md.push_str("| UTF-16 surrogate pairs via `\\uD800`-`\\uDBFF` + `\\uDC00`-`\\uDFFF` | `utf16_literal_to_utf8` |\n");
    md.push_str("| UTF-8 BOM is skipped only when at least 5 bytes are addressable | `skip_utf8_bom` (`can_access_at_index(buffer, 4)`) |\n");
    md.push_str("| `parse_hex4` returns 0 for invalid hex (same as `\\u0000`) | `parse_hex4` |\n");
    md.push_str("\n## Divergences\n\n");
    if divergences == 0 {
        md.push_str("None. Every input file produced the same parse success/failure and byte-identical compact and pretty output.\n");
    } else {
        md.push_str("See the table above for files marked `NO`.\n");
    }

    let dest = workspace_root().join("PARITY.md");
    let mut f = fs::File::create(&dest).expect("write PARITY.md");
    f.write_all(md.as_bytes()).expect("write");

    assert_eq!(
        divergences, 0,
        "C/Rust parity divergences found; see PARITY.md"
    );
}
