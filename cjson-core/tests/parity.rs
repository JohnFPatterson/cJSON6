//! Differential harness: every file in `tests/inputs/` is parsed and printed
//! by the original C library and by `cjson-core`. The `parity` binary writes
//! `PARITY.md`. Prefer `make parity` for the projector demo (C + Rust Unity).

use std::process::Command;

#[test]
fn differential_harness_inputs() {
    let bin = env!("CARGO_BIN_EXE_parity");
    let output = Command::new(bin)
        .output()
        .expect("run parity binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "C/Rust parity divergences found; see PARITY.md\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
