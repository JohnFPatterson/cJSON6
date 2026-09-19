//! Differential harness: every file in `tests/inputs/` is parsed and printed
//! by the original C library and by `cjson-core`. Prefer `make parity` for the
//! projector demo (C + Rust Unity) and for writing `PARITY.md`.

use std::process::Command;

#[test]
fn differential_harness_inputs() {
    let bin = env!("CARGO_BIN_EXE_parity");
    let md = std::env::temp_dir().join("cjson-parity-test.md");
    let output = Command::new(bin)
        .arg("--md")
        .arg(&md)
        .output()
        .expect("run parity binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "C/Rust parity divergences found; see {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        md.display()
    );
    assert!(
        stdout.contains("inputs byte-identical"),
        "parity binary should print the total line, got:\n{stdout}"
    );
}
