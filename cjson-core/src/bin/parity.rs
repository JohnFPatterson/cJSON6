//! Differential harness: original C cJSON vs the Rust `cjson-core` port.
//!
//! Compares every file in `tests/inputs/` for parse success/failure, the exact
//! `cJSON_GetErrorPtr` offset on failure, and byte-for-byte compact + pretty
//! print. Writes `PARITY.md`. `--demo` prints a short color summary for a
//! projector; the process exits nonzero if anything diverges.

use cjson_core::{parse, parse_with_opts, print, print_unformatted, ParseOptions};
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{self, Command};

struct OracleResult {
    ok: bool,
    error_pos: Option<i64>,
    unformatted: Option<Vec<u8>>,
    formatted: Option<Vec<u8>>,
    print_fail: bool,
}

struct FileReport {
    name: String,
    c_ok: bool,
    rust_ok: bool,
    c_pos: Option<i64>,
    rust_pos: Option<i64>,
    compact_match: bool,
    pretty_match: bool,
    identical: bool,
    c_compact_len: Option<usize>,
    rust_compact_len: Option<usize>,
    c_pretty_len: Option<usize>,
    rust_pretty_len: Option<usize>,
    notes: String,
    c_source: &'static str,
}

struct UnityStats {
    passed: u32,
    failed: u32,
    suites_ok: Option<u32>,
    suites_total: Option<u32>,
}

struct Args {
    demo: bool,
    root: PathBuf,
    oracle: PathBuf,
    md: PathBuf,
    unity_c: Option<UnityStats>,
    unity_rust: Option<UnityStats>,
    c_build_ok: bool,
    rust_build_ok: bool,
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn parse_unity(spec: &str) -> UnityStats {
    // PASSED/FAILED or PASSED/FAILED/SUITES_OK/SUITES_TOTAL
    let parts: Vec<&str> = spec.split('/').collect();
    let passed = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
    let failed = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let suites_ok = parts.get(2).and_then(|s| s.parse().ok());
    let suites_total = parts.get(3).and_then(|s| s.parse().ok());
    UnityStats {
        passed,
        failed,
        suites_ok,
        suites_total,
    }
}

fn parse_args() -> Args {
    let mut demo = false;
    let mut root = workspace_root();
    let mut oracle = root.join("target/cjson_oracle");
    let mut md = root.join("PARITY.md");
    let mut unity_c = None;
    let mut unity_rust = None;
    let mut c_build_ok = true;
    let mut rust_build_ok = true;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--demo" => demo = true,
            "--root" => {
                if let Some(v) = args.next() {
                    root = PathBuf::from(v);
                    oracle = root.join("target/cjson_oracle");
                    md = root.join("PARITY.md");
                }
            }
            "--oracle" => {
                if let Some(v) = args.next() {
                    oracle = PathBuf::from(v);
                }
            }
            "--md" => {
                if let Some(v) = args.next() {
                    md = PathBuf::from(v);
                }
            }
            "--unity-c" => {
                if let Some(v) = args.next() {
                    unity_c = Some(parse_unity(&v));
                }
            }
            "--unity-rust" => {
                if let Some(v) = args.next() {
                    unity_rust = Some(parse_unity(&v));
                }
            }
            "--c-build" => {
                c_build_ok = args.next().is_some_and(|v| v == "ok");
            }
            "--rust-build" => {
                rust_build_ok = args.next().is_some_and(|v| v == "ok");
            }
            "-h" | "--help" => {
                eprintln!(
                    "usage: parity [--demo] [--root DIR] [--oracle BIN] [--md FILE]\n\
                     \t[--unity-c P/F[/SOK/STOT]] [--unity-rust P/F]\n\
                     \t[--c-build ok|fail] [--rust-build ok|fail]"
                );
                process::exit(0);
            }
            other => {
                eprintln!("unknown argument: {other}");
                process::exit(2);
            }
        }
    }
    Args {
        demo,
        root,
        oracle,
        md,
        unity_c,
        unity_rust,
        c_build_ok,
        rust_build_ok,
    }
}

fn build_oracle(root: &Path, out: &Path) {
    if let Some(parent) = out.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let status = Command::new("gcc")
        .args(["-O0", "-g", "-std=c89", "-o"])
        .arg(out)
        .arg(root.join("tools/cjson_oracle.c"))
        .arg(root.join("cJSON.c"))
        .arg("-I")
        .arg(root)
        .arg("-lm")
        .status()
        .expect("spawn gcc");
    assert!(status.success(), "failed to compile C oracle");
}

fn parse_oracle_stdout(stdout: &[u8]) -> OracleResult {
    let first_nl = stdout.iter().position(|&b| b == b'\n').unwrap_or(stdout.len());
    let first = &stdout[..first_nl];
    if first == b"FAIL" {
        let rest = stdout.get(first_nl + 1..).unwrap_or(b"");
        let pos_nl = rest.iter().position(|&b| b == b'\n').unwrap_or(rest.len());
        let pos_line = &rest[..pos_nl];
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
    let mut rest = stdout.get(first_nl + 1..).unwrap_or(b"");
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

fn run_c_oracle(oracle: &Path, path: &Path) -> OracleResult {
    let output = Command::new(oracle)
        .arg(path)
        .output()
        .unwrap_or_else(|e| panic!("run oracle {}: {e}", oracle.display()));
    assert!(
        output.status.success(),
        "oracle failed for {}: {}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    parse_oracle_stdout(&output.stdout)
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

fn run_rust(path: &Path) -> OracleResult {
    let bytes = fs::read(path).expect("read input");
    // cJSON_Parse uses strlen, so stop at the first NUL like C.
    let cstr_end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let parse_result = match std::str::from_utf8(&bytes[..cstr_end]) {
        Ok(text) => parse(text),
        Err(_) => {
            let mut buf = bytes[..cstr_end].to_vec();
            buf.push(0);
            parse_with_opts(
                &buf,
                ParseOptions {
                    require_null_terminated: false,
                    skip_bom: true,
                },
            )
            .map(|s| s.value)
        }
    };
    match parse_result {
        Ok(v) => rust_print(&v),
        Err(e) => OracleResult {
            ok: false,
            error_pos: Some(e.position as i64),
            unformatted: None,
            formatted: None,
            print_fail: false,
        },
    }
}

fn c_source_for(parse_div: bool, pos_div: bool, compact_div: bool, pretty_div: bool) -> &'static str {
    if parse_div {
        "cJSON.c:1387 parse_value  (cJSON_Parse at 1239)"
    } else if pos_div {
        "cJSON.c:95 cJSON_GetErrorPtr  (fail path 1206–1232)"
    } else if compact_div && pretty_div {
        "cJSON.c:605 print_number  (Print 1321 / PrintUnformatted 1327)"
    } else if compact_div {
        "cJSON.c:1327 cJSON_PrintUnformatted  (print_value 1442, print_number 605)"
    } else if pretty_div {
        "cJSON.c:1321 cJSON_Print  (print_value 1442, print_number 605)"
    } else {
        ""
    }
}

fn compare_file(path: &Path, c: &OracleResult, r: &OracleResult) -> FileReport {
    let name = path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let compact_match = match (&c.unformatted, &r.unformatted) {
        (Some(a), Some(b)) => a == b,
        (None, None) => c.ok == r.ok && c.print_fail == r.print_fail,
        _ => false,
    };
    let pretty_match = match (&c.formatted, &r.formatted) {
        (Some(a), Some(b)) => a == b,
        (None, None) => c.ok == r.ok && c.print_fail == r.print_fail,
        _ => false,
    };
    let pos_match = if !c.ok && !r.ok {
        c.error_pos == r.error_pos
    } else {
        true
    };
    let parse_div = c.ok != r.ok;
    let pos_div = !pos_match;
    let compact_div = !compact_match;
    let pretty_div = !pretty_match;
    let identical = !parse_div && pos_match && compact_match && pretty_match && c.print_fail == r.print_fail;

    let mut notes = String::new();
    if parse_div {
        notes.push_str(&format!(
            "parse success diverges (C ok={}, Rust ok={}); ",
            c.ok, r.ok
        ));
    }
    if pos_div {
        notes.push_str(&format!(
            "error position diverges (C {:?}, Rust {:?}); ",
            c.error_pos, r.error_pos
        ));
    }
    if c.ok && r.ok && compact_div {
        notes.push_str("compact print mismatch; ");
    }
    if c.ok && r.ok && pretty_div {
        notes.push_str("pretty print mismatch; ");
    }
    if c.print_fail != r.print_fail {
        notes.push_str("print success diverges; ");
    }
    if !c.ok && !r.ok && pos_match {
        notes.push_str("both failed to parse (expected for non-JSON such as test6); ");
        if let Some(p) = c.error_pos {
            notes.push_str(&format!("error pos {p} (cJSON.c:95 cJSON_GetErrorPtr)"));
        }
    }

    FileReport {
        name,
        c_ok: c.ok,
        rust_ok: r.ok,
        c_pos: c.error_pos,
        rust_pos: r.error_pos,
        compact_match,
        pretty_match,
        identical,
        c_compact_len: c.unformatted.as_ref().map(Vec::len),
        rust_compact_len: r.unformatted.as_ref().map(Vec::len),
        c_pretty_len: c.formatted.as_ref().map(Vec::len),
        rust_pretty_len: r.formatted.as_ref().map(Vec::len),
        notes,
        c_source: c_source_for(parse_div, pos_div, compact_div, pretty_div),
    }
}

fn write_parity_md(
    dest: &Path,
    reports: &[FileReport],
    identical: usize,
    unity_c: &Option<UnityStats>,
    unity_rust: &Option<UnityStats>,
) {
    let mut md = String::new();
    md.push_str("# cJSON C vs Rust parity\n\n");
    md.push_str("Differential run of every file in `tests/inputs/` through the original C library (`cJSON.c`) and the Rust `cjson-core` port.\n\n");
    md.push_str("Produced by `make parity` (or `./parity.sh`).\n\n");
    md.push_str("## Method\n\n");
    md.push_str("- **C:** `cJSON_Parse` + `cJSON_PrintUnformatted` + `cJSON_Print` (oracle at `tools/cjson_oracle.c`). Failure offset is `cJSON_GetErrorPtr() - buffer` (`cJSON.c:95`).\n");
    md.push_str("- **Rust:** `cjson_core::parse` + `print_unformatted` + `print`. `ParseError.position` matches that offset.\n");
    md.push_str("- Compact and pretty printed output is compared as raw bytes.\n");
    md.push_str("- The original Unity suites under `tests/*.c` are compiled against `cJSON.c`; the port lives in `cjson-core/tests/unity.rs`.\n\n");

    md.push_str("## Summary\n\n");
    md.push_str(&format!(
        "{} files, {} byte-identical (parse success/failure, error position, compact print, pretty print).\n\n",
        reports.len(),
        identical
    ));

    if let (Some(c), Some(r)) = (unity_c, unity_rust) {
        md.push_str("## Unity\n\n");
        md.push_str("| Implementation | Passed | Failed | Suites |\n");
        md.push_str("|----------------|--------|--------|--------|\n");
        let c_suites = match (c.suites_ok, c.suites_total) {
            (Some(ok), Some(tot)) => format!("{ok}/{tot}"),
            _ => "—".to_string(),
        };
        md.push_str(&format!(
            "| C (`tests/*.c` + `cJSON.c`) | {} | {} | {} |\n",
            c.passed, c.failed, c_suites
        ));
        let r_suites = match (r.suites_ok, r.suites_total) {
            (Some(ok), Some(tot)) => format!("{ok}/{tot}"),
            _ => "—".to_string(),
        };
        md.push_str(&format!(
            "| Rust (`cjson-core/tests/unity.rs`) | {} | {} | {} |\n\n",
            r.passed, r.failed, r_suites
        ));
        md.push_str("C Unity counts each `TEST` function (153 across 18 files). The Rust port covers the same 18 suites in 68 `#[test]` functions; some C cases were grouped. A failure in either implementation is a parity failure.\n\n");
    }

    md.push_str("## Per-file results\n\n");
    md.push_str("| File | C parse | C error pos | Rust parse | Rust error pos | Compact | Pretty | Match | Notes |\n");
    md.push_str("|------|---------|-------------|------------|----------------|---------|--------|-------|-------|\n");
    for r in reports {
        let c_parse = if r.c_ok { "ok" } else { "fail" };
        let rust_parse = if r.rust_ok { "ok" } else { "fail" };
        let c_pos = r
            .c_pos
            .map(|p| p.to_string())
            .unwrap_or_else(|| "—".to_string());
        let rust_pos = r
            .rust_pos
            .map(|p| p.to_string())
            .unwrap_or_else(|| "—".to_string());
        let compact = if r.compact_match {
            match (r.c_compact_len, r.rust_compact_len) {
                (Some(n), Some(_)) => format!("yes ({n} B)"),
                _ => "yes".to_string(),
            }
        } else {
            format!(
                "NO (C {:?} B / Rust {:?} B)",
                r.c_compact_len, r.rust_compact_len
            )
        };
        let pretty = if r.pretty_match {
            match (r.c_pretty_len, r.rust_pretty_len) {
                (Some(n), Some(_)) => format!("yes ({n} B)"),
                _ => "yes".to_string(),
            }
        } else {
            format!(
                "NO (C {:?} B / Rust {:?} B)",
                r.c_pretty_len, r.rust_pretty_len
            )
        };
        md.push_str(&format!(
            "| `{}` | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            r.name,
            c_parse,
            c_pos,
            rust_parse,
            rust_pos,
            compact,
            pretty,
            if r.identical { "yes" } else { "**NO**" },
            r.notes.replace('|', "\\|").trim()
        ));
    }

    md.push_str("\n## Documented C quirks (not divergences)\n\n");
    md.push_str("These behaviors were **matched** by the Rust port; they are listed so a reader can find the C source that specifies them.\n\n");
    md.push_str("| Behavior | C source |\n");
    md.push_str("|----------|----------|\n");
    md.push_str("| `cJSON_GetErrorPtr` is `global_error.json + global_error.position` | `cJSON.c:95` |\n");
    md.push_str("| Parse failure stores `buffer.offset`, or `buffer.length - 1` at EOF | `cJSON.c:1206–1232` |\n");
    md.push_str("| `cJSON_Parse` uses `strlen(value) + 1` (includes the NUL) | `cJSON.c:1141` / `1151` |\n");
    md.push_str("| `cJSON_GetObjectItem` is case-insensitive; `CaseSensitive` is not | `cJSON.c:137` `case_insensitive_strcmp`, `cJSON.c:2006` |\n");
    md.push_str("| Objects preserve insertion order and allow duplicate keys | `parse_object` appends to the child linked list; no key uniquing |\n");
    md.push_str("| `cJSON_Compare` looks up object members by key, so duplicate keys compare against the **first** match | `cJSON.c` `cJSON_Compare` calling `get_object_item` |\n");
    md.push_str("| Floats print with `sprintf(\"%1.15g\")`, falling back to `\"%1.17g\"` if `compare_double` fails | `cJSON.c:605` `print_number` (`%1.15g` at 632) |\n");
    md.push_str("| Integers that equal `(double)valueint` print with `%d` | `cJSON.c:625` |\n");
    md.push_str("| NaN and Infinity print as `null` | `cJSON.c:621` (`isnan` / `isinf`) |\n");
    md.push_str("| Nesting deeper than `CJSON_NESTING_LIMIT` (1000) is rejected | `parse_array` / `parse_object` / `print_array` / `print_object` |\n");
    md.push_str("| UTF-16 surrogate pairs via `\\uD800`-`\\uDBFF` + `\\uDC00`-`\\uDFFF` | `utf16_literal_to_utf8` (`cJSON.c:712`) |\n");
    md.push_str("| UTF-8 BOM is skipped only when at least 5 bytes are addressable | `skip_utf8_bom` (`cJSON.c:1125`, `can_access_at_index(buffer, 4)`) |\n");
    md.push_str("| `parse_hex4` returns 0 for invalid hex (same as `\\u0000`) | `parse_hex4` (`cJSON.c:675`) |\n");

    md.push_str("\n## Divergences\n\n");
    let divergences: Vec<&FileReport> = reports.iter().filter(|r| !r.identical).collect();
    if divergences.is_empty() {
        md.push_str("None. Every input file produced the same parse success/failure, the same error position on failure, and byte-identical compact and pretty output.\n");
    } else {
        for d in divergences {
            md.push_str(&format!(
                "- `{}`: {}  \n  C source: `{}`\n",
                d.name,
                d.notes.trim(),
                d.c_source
            ));
        }
    }

    let mut f = fs::File::create(dest).expect("write PARITY.md");
    f.write_all(md.as_bytes()).expect("write");
}

struct Color {
    on: bool,
}

impl Color {
    fn new(force: bool) -> Self {
        // `--demo` is meant for a projector: color unless the user asked it off.
        let on = force && env::var_os("NO_COLOR").is_none();
        Self { on }
    }
    fn paint(&self, code: &str, s: &str) -> String {
        if self.on {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }
    fn bold(&self, s: &str) -> String {
        self.paint("1", s)
    }
    fn dim(&self, s: &str) -> String {
        self.paint("2", s)
    }
    fn green(&self, s: &str) -> String {
        self.paint("32", s)
    }
    fn red(&self, s: &str) -> String {
        self.paint("31", s)
    }
    fn cyan(&self, s: &str) -> String {
        self.paint("36", s)
    }
    fn yellow(&self, s: &str) -> String {
        self.paint("33", s)
    }
}

fn fmt_unity(u: &UnityStats) -> (String, bool) {
    let ok = u.failed == 0;
    let suites = match (u.suites_ok, u.suites_total) {
        (Some(a), Some(b)) => format!("{a}/{b} suites  "),
        _ => String::new(),
    };
    (
        format!("{suites}{:>3} pass  {} fail", u.passed, u.failed),
        ok,
    )
}

fn print_demo(
    color: &Color,
    args: &Args,
    reports: &[FileReport],
    identical: usize,
    unity_ok: bool,
) {
    let ok = |s: &str| color.green(s);
    let bad = |s: &str| color.red(s);
    let rule = color.dim("────────────────────────────────────────────────");

    println!();
    println!(
        "  {}",
        color.bold(&color.cyan("cJSON 1.7.19   C  ↔  Rust"))
    );
    println!("  {rule}");

    let c_build = if args.c_build_ok {
        ok("OK")
    } else {
        bad("FAIL")
    };
    let rust_build = if args.rust_build_ok {
        ok("OK")
    } else {
        bad("FAIL")
    };
    println!("  C library + oracle ...................... {c_build}");
    println!("  Rust cjson-core ......................... {rust_build}");
    println!();

    if let Some(c) = &args.unity_c {
        let (text, passed) = fmt_unity(c);
        let mark = if passed { ok("OK") } else { bad("FAIL") };
        println!("  Unity  C      {text}   {mark}");
    }
    if let Some(r) = &args.unity_rust {
        let (text, passed) = fmt_unity(r);
        let mark = if passed { ok("OK") } else { bad("FAIL") };
        println!("  Unity  Rust   {text}   {mark}");
    }
    if args.unity_c.is_some() || args.unity_rust.is_some() {
        println!();
    }

    println!(
        "  {}",
        color.dim("tests/inputs/   parse · error pos · compact · pretty")
    );
    println!(
        "  {} {} {} {}",
        color.bold(&format!("{:<16}", "FILE")),
        color.bold(&format!("{:<14}", "C")),
        color.bold(&format!("{:<14}", "RUST")),
        color.bold("RESULT")
    );

    let mismatches: Vec<&FileReport> = reports.iter().filter(|r| !r.identical).collect();
    let passed = identical;
    if passed > 0 {
        let collapsed = format!("×{passed}");
        println!(
            "  {}",
            color.green(&format!(
                "{collapsed:<16} {:<14} {:<14} MATCH",
                "identical", "identical"
            ))
        );
    }
    const SHOW_MAX: usize = 8;
    for (i, r) in mismatches.iter().enumerate() {
        if i >= SHOW_MAX {
            println!(
                "  {}",
                color.yellow(&format!(
                    "… {} more in PARITY.md",
                    mismatches.len() - SHOW_MAX
                ))
            );
            break;
        }
        let c_lab = if r.c_ok {
            "OK".to_string()
        } else {
            r.c_pos
                .map(|p| format!("FAIL@{p}"))
                .unwrap_or_else(|| "FAIL".to_string())
        };
        let r_lab = if r.rust_ok {
            "OK".to_string()
        } else {
            r.rust_pos
                .map(|p| format!("FAIL@{p}"))
                .unwrap_or_else(|| "FAIL".to_string())
        };
        println!(
            "  {:<16} {:<14} {:<14} {}",
            r.name,
            c_lab,
            r_lab,
            bad("MISMATCH")
        );
        if !r.c_source.is_empty() {
            println!("    {}", color.dim(r.c_source));
        }
        let note = r.notes.trim().trim_end_matches(';');
        if !note.is_empty() {
            println!("    {}", color.dim(note));
        }
    }

    println!();
    let total = reports.len();
    let total_line = format!("{identical}/{total} inputs byte-identical");
    if identical == total && unity_ok {
        println!("  {}", color.bold(&ok(&total_line)));
    } else {
        println!("  {}", color.bold(&bad(&total_line)));
    }
    println!("  {}", color.dim("full table → PARITY.md"));
    println!();
}

fn unity_diverged(args: &Args) -> bool {
    // C and Rust test *counts* differ (153 vs 68) because the port groups cases;
    // only actual failures are a divergence.
    match (&args.unity_c, &args.unity_rust) {
        (Some(c), Some(r)) => c.failed != 0 || r.failed != 0,
        (Some(c), None) => c.failed != 0,
        (None, Some(r)) => r.failed != 0,
        (None, None) => false,
    }
}

fn main() {
    let args = parse_args();
    build_oracle(&args.root, &args.oracle);

    let inputs = args.root.join("tests/inputs");
    let mut files: Vec<_> = fs::read_dir(&inputs)
        .unwrap_or_else(|_| panic!("inputs dir {}", inputs.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    files.sort();

    let mut reports = Vec::new();
    for path in &files {
        let c = run_c_oracle(&args.oracle, path);
        let r = run_rust(path);
        reports.push(compare_file(path, &c, &r));
    }

    let identical = reports.iter().filter(|r| r.identical).count();
    write_parity_md(
        &args.md,
        &reports,
        identical,
        &args.unity_c,
        &args.unity_rust,
    );

    let unity_ok = !unity_diverged(&args);
    let all_ok = identical == reports.len()
        && unity_ok
        && args.c_build_ok
        && args.rust_build_ok;

    if args.demo {
        let color = Color::new(true);
        print_demo(&color, &args, &reports, identical, all_ok);
    } else {
        println!(
            "{}/{} inputs byte-identical",
            identical,
            reports.len()
        );
    }

    if !all_ok {
        process::exit(1);
    }
}
