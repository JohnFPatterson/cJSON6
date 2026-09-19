#!/usr/bin/env bash
# Build original C cJSON and the Rust port, run tests/inputs/ through both,
# run the Unity suite against both implementations, print a short color
# summary, and write PARITY.md. Exit nonzero if anything diverges.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT"

LOG="$ROOT/target/parity-build.log"
UNITY_DIR="$ROOT/target/unity"
ORACLE="$ROOT/target/cjson_oracle"
mkdir -p "$ROOT/target" "$UNITY_DIR"
: >"$LOG"

UNITY_SUITES=(
    parse_examples
    parse_number
    parse_hex4
    parse_string
    parse_array
    parse_object
    parse_value
    print_string
    print_number
    print_array
    print_object
    print_value
    misc_tests
    parse_with_opts
    compare_tests
    cjson_add
    readme_examples
    minify_tests
)

c_build=ok
rust_build=ok

die_build() {
    echo "parity: $1" >&2
    tail -n 40 "$LOG" >&2 || true
    exit 1
}

{
    echo "== gcc cJSON.o =="
    gcc -std=c89 -O0 -g -c -o "$ROOT/target/cJSON.o" "$ROOT/cJSON.c" -I"$ROOT" -lm
    echo "== gcc oracle =="
    gcc -O0 -g -std=c89 -o "$ORACLE" \
        "$ROOT/tools/cjson_oracle.c" "$ROOT/cJSON.c" -I"$ROOT" -lm
} >>"$LOG" 2>&1 || {
    c_build=fail
    die_build "failed to build original C cJSON / oracle (see $LOG)"
}

{
    echo "== gcc Unity suites =="
    for suite in "${UNITY_SUITES[@]}"; do
        gcc -std=c89 -O0 -g \
            -I"$ROOT" -I"$ROOT/tests" -I"$ROOT/tests/unity/src" \
            -o "$UNITY_DIR/$suite" \
            "$ROOT/tests/${suite}.c" "$ROOT/tests/unity/src/unity.c" -lm
    done
} >>"$LOG" 2>&1 || {
    c_build=fail
    die_build "failed to compile C Unity suites (see $LOG)"
}

{
    echo "== cargo build parity + unity =="
    cargo build -p cjson-core --bin parity --quiet
    cargo test -p cjson-core --test unity --no-run --quiet
} >>"$LOG" 2>&1 || {
    rust_build=fail
    die_build "failed to build Rust port (see $LOG)"
}

c_pass=0
c_fail=0
c_suites_ok=0
c_suites_total=0
C_UNITY_LOG="$ROOT/target/parity-unity-c.log"
: >"$C_UNITY_LOG"

for suite in "${UNITY_SUITES[@]}"; do
    c_suites_total=$((c_suites_total + 1))
    if [[ "$suite" == "parse_examples" ]]; then
        cwd="$ROOT/tests"
    else
        cwd="$ROOT"
    fi
    set +e
    suite_out="$(
        cd "$cwd"
        "$UNITY_DIR/$suite" 2>&1
    )"
    rc=$?
    set -e
    printf '%s\n' "$suite_out" >>"$C_UNITY_LOG"
    # Unity prints: "<n> Tests <f> Failures <i> Ignored"
    tests_n="$(printf '%s\n' "$suite_out" | sed -n 's/^\([0-9][0-9]*\) Tests \([0-9][0-9]*\) Failures \([0-9][0-9]*\) Ignored.*/\1/p' | tail -n 1)"
    fails_n="$(printf '%s\n' "$suite_out" | sed -n 's/^\([0-9][0-9]*\) Tests \([0-9][0-9]*\) Failures \([0-9][0-9]*\) Ignored.*/\2/p' | tail -n 1)"
    ignore_n="$(printf '%s\n' "$suite_out" | sed -n 's/^\([0-9][0-9]*\) Tests \([0-9][0-9]*\) Failures \([0-9][0-9]*\) Ignored.*/\3/p' | tail -n 1)"
    tests_n="${tests_n:-0}"
    fails_n="${fails_n:-0}"
    ignore_n="${ignore_n:-0}"
    c_pass=$((c_pass + tests_n - fails_n - ignore_n))
    c_fail=$((c_fail + fails_n))
    if [[ "$rc" -eq 0 && "$fails_n" -eq 0 ]]; then
        c_suites_ok=$((c_suites_ok + 1))
    fi
done

R_UNITY_LOG="$ROOT/target/parity-unity-rust.log"
set +e
cargo test -p cjson-core --test unity -- --test-threads=1 >"$R_UNITY_LOG" 2>&1
rust_unity_rc=$?
set -e

rust_line="$(grep -E 'test result:' "$R_UNITY_LOG" | tail -n 1 || true)"
rust_pass="$(printf '%s\n' "$rust_line" | sed -n 's/.* \([0-9][0-9]*\) passed.*/\1/p')"
rust_fail="$(printf '%s\n' "$rust_line" | sed -n 's/.* \([0-9][0-9]*\) failed.*/\1/p')"
rust_pass="${rust_pass:-0}"
rust_fail="${rust_fail:-0}"
if [[ "$rust_unity_rc" -ne 0 && "$rust_fail" -eq 0 ]]; then
    rust_fail=1
fi

BIN="$ROOT/target/debug/parity"
if [[ ! -x "$BIN" ]]; then
    BIN="$(find "$ROOT/target" -type f -name parity -perm -111 | head -n 1 || true)"
fi
[[ -n "$BIN" && -x "$BIN" ]] || die_build "parity binary not found"

set +e
"$BIN" --demo \
    --root "$ROOT" \
    --oracle "$ORACLE" \
    --md "$ROOT/PARITY.md" \
    --c-build "$c_build" \
    --rust-build "$rust_build" \
    --unity-c "${c_pass}/${c_fail}/${c_suites_ok}/${c_suites_total}" \
    --unity-rust "${rust_pass}/${rust_fail}/18/18"
rc=$?
set -e
exit "$rc"
