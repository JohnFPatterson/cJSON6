---
name: port-c-library-to-rust
description: Port a C library to Rust as a safe core crate plus a thin C ABI shim. Use when the user asks to port a C library to Rust, add a C ABI / FFI shim, match C quirks, build a parse/print parity harness against original C, scan C modules with SonarQube MCP, or record security-driven parity exceptions.
---

# Port a C library to Rust

Follow this playbook when moving a C library into a Cargo workspace that existing C callers can still link. Keep the original C sources in-tree as the behavioral spec. Do not "improve" the C API except where SonarQube flags a **security** issue or hotspot — those fixes belong in Rust, logged as parity exceptions.

Worked example in this repo: `cjson-core`, `cjson-ffi`, `MIGRATION.md`, `PARITY.md`, `PARITY_EXCEPTIONS.md`, `parity.sh`.

## When to use

- Porting a C library (parser, codec, data structure) to Rust while preserving C ABI.
- Splitting logic into a safe crate and an `unsafe` FFI crate.

Out of scope unless the user asks: rewriting the API to be more idiomatic, dropping non-security quirks, porting optional companion libraries (silent skip is a bug — name them as out of scope), adding a fuzzer requirement.

## MUST / MUST NOT

| MUST | MUST NOT |
|------|----------|
| Cargo workspace with `{lib}-core` (all logic) and `{lib}-ffi` (shim only) | Put parse/print/tree logic in the FFI crate |
| `#![forbid(unsafe_code)]` on the core crate | Sprinkle `unsafe` through core "just for FFI" |
| FFI is the **only** crate with `unsafe`; every `unsafe` block has a `SAFETY:` comment | Export C functions without documenting pointer contracts |
| Keep original `*.c` / `*.h` in-tree as spec + oracle | Delete C sources once Rust "looks right" or patch C to "fix" Sonar findings |
| Match C **including quirks**, except SonarQube SECURITY issues and Security Hotspots | "Fix" non-security quirks (lookup case folding, duplicate keys, print format, limits) |
| Scan each library C module via SonarQube MCP before treating the port as done | Skip the scan if MCP is missing; invent findings; use maintainability noise as exceptions |
| Malformed input returns `Result` / `Error`; library code does not panic on untrusted data | `unwrap` / `expect` / panic on bad input |
| Port the original test suite (Unity or equivalent), same cases | Invent a new suite that happens to pass |
| Differential harness vs C: parse ok/fail, error position, byte-identical compact **and** pretty print | Compare only success/failure or pretty-printed JSON that is "close enough" |
| Unlisted C/Rust diffs fail the harness | Allowlist a mismatch that is not a `PARITY_EXCEPTIONS.md` row with a test |
| Write `PARITY.md` (per-input table, quirks with **C source lines**, divergences or "none") | Treat a green `cargo test` as the only evidence |
| Write `PARITY_EXCEPTIONS.md`; one `#[test]` per row in `{lib}-core/tests/exceptions.rs` | Change observable behavior without an exception ID and test |
| Write `MIGRATION.md` (layout, `grep -rn "unsafe" --include=*.rs`, kept quirks vs intentional diffs) | Leave unsafe sites undocumented |
| `cargo test` and `cargo clippy -- -D warnings` pass | Ship with clippy warnings or skipped tests |
| State out-of-scope APIs explicitly | Silently skip Utils / optional modules |

## Crate layout

```
{lib}-core/     # logic; forbid(unsafe_code)
{lib}-ffi/      # #[repr(C)] types, c_fn exports; crate-type staticlib + cdylib
*.c / *.h       # original sources remain
tools/          # C oracle driver that calls the original library
```

Rust callers depend on `{lib}-core`. C callers link `{lib}-ffi` and keep including the original header.

Core may use an owned tree (`Vec` children, etc.). FFI reconstitutes whatever layout the C header documents (linked lists, interior pointers, allocator hooks). Hooks and `malloc` failure injection stay in FFI; core uses the Rust allocator.

## Security scan (SonarQube MCP)

Do this **before** declaring parity complete. Only SECURITY issues and Security Hotspots may become exceptions. Leave C files unchanged.

### Connect

1. Call `GetDynamicTools` with `namespace: "Sonarqube"` (try `SonarQube` / `sonarqube` only if that exact name is missing).
2. If `namespaceStatus` is `needsAuth`, call that namespace's `mcp_auth` tool, then inspect the namespace again.
3. If the namespace is missing, in `error`, or auth fails: **stop**. Tell the user to connect SonarQube MCP. Do not invent findings or skip the scan.

Discover tool schemas with `GetDynamicTools` before `CallDynamicTool`. Names below are the usual SonarQube MCP tools; use whatever the live schema exposes that matches these jobs.

### Modules

Scan each **library** C translation unit: product `*.c` (and headers that contain logic). Skip `tests/`, Unity, fuzzers, and examples.

### Query

When the library is already on SonarQube / SonarCloud:

- `search_sonar_issues_in_projects` with `impactSoftwareQualities: ["SECURITY"]` and statuses OPEN / CONFIRMED. Group hits by file.
- `search_security_hotspots` with `files: ["path/to/module.c"]` and status `TO_REVIEW`.
- `show_rule` / `show_security_hotspot` for C `file:line` and the rule key.

Otherwise local analysis:

- `analyze_file_list` (SonarQube for IDE) with **absolute** paths of those `.c` files.
- Do **not** rely on `analyze_code_snippet` for C/C++ (its documented languages omit C).

Ignore maintainability, style, and reliability-only findings unless the user asks.

### Classify each finding

| Keep matching C | Fix in Rust (parity exception) |
|-----------------|--------------------------------|
| Not SECURITY / not a hotspot | Buffer overflow, OOB, integer overflow used unsafely, command/path injection, unbounded copy |
| Quirk that is specified behavior (case folding, `%g` print, duplicate keys) | C accepts input that should be rejected for safety |

Fix means: implement the **safe** behavior in `{lib}-core`. Typical mappings: OOB / overflow-as-UB → `Error`; integer wrap used as a weapon → saturate or `Error`; tainted command/path → reject. Use-after-free often disappears in an owned tree: **no exception row** if observable parse/print still matches C.

If the C ABI must stay crash-compatible, FFI may still return the C error channel (`NULL` + error pointer) while core returns `Result`. Document that in `MIGRATION.md`.

If fixing would not change parse/print/error-pos vs C, there is no exception — just a safer implementation.

## Behavior

1. Read the C sources before writing Rust. Quote C file:line for quirks in `PARITY.md`.
2. Match print byte-for-byte (compact and pretty), including C `sprintf` formats, integer-vs-float branches, and non-finite → whatever C emits — unless a listed exception applies.
3. Match parse failure the way C does, including error cursor / `GetErrorPtr`-style offsets — unless a listed exception applies.
4. Preserve documented limits (nesting, circular walks on the C side).
5. Do not panic on malformed input.

## Parity exceptions

File: `PARITY_EXCEPTIONS.md` (required on every port; empty table if none).

See this repo's `PARITY_EXCEPTIONS.md` for the table schema.

Each row needs:

- `ID` (`PE-001`, …)
- C `file:line`
- Sonar rule or hotspot key
- One-line finding
- C behavior vs Rust behavior
- Test name in `{lib}-core/tests/exceptions.rs`
- Fixture path if a project fixture is the exception input

Unlisted C/Rust diffs still fail `make parity` / the oracle harness. Do not weaken byte-identical fixtures except when that fixture is named on the row.

Each exception `#[test]` must:

1. Feed an input that triggers the C bug or unsafe path.
2. Assert Rust's safe result.
3. If practical, run the C oracle and assert C still shows the old behavior (keeps the exception honest).

## Proof

1. Complete the SonarQube scan and classify findings (keep vs exception).
2. Port the project's existing tests onto `{lib}-core` (keep C tests runnable too).
3. Build a small C oracle (`*-oracle.c` compiling original `.c`) that prints parse status, error offset, compact blob, pretty blob.
4. Run every fixture the project already has (and C tests' golden files) through C and Rust; fail the process if anything diverges **except** `PARITY_EXCEPTIONS.md` rows.
5. Write `PARITY.md`, `PARITY_EXCEPTIONS.md`, and `MIGRATION.md`.
6. `grep -rn "unsafe" --include=*.rs --exclude-dir=target` from the repo root: core should only mention `unsafe` in `forbid(unsafe_code)` docs; every executable `unsafe` is in FFI with `SAFETY:`.

A one-command demo (`make parity` or a root script) that builds both implementations, runs fixtures + original tests, prints a short summary, and exits nonzero on **unlisted** divergence is the preferred handoff.

## Done checklist

- [ ] `{lib}-core` has `#![forbid(unsafe_code)]`; all logic lives there
- [ ] `{lib}-ffi` is a thin ABI; every `unsafe` has `SAFETY:`
- [ ] Original C still builds and is the oracle (unpatched)
- [ ] SonarQube MCP scan of each library `.c` module (SECURITY + hotspots)
- [ ] Original test suite ported; C suite still runs
- [ ] Differential harness: parse, error position, compact, pretty
- [ ] `PARITY.md`, `PARITY_EXCEPTIONS.md`, and `MIGRATION.md` written
- [ ] One `#[test]` per exception row (or empty table)
- [ ] Out-of-scope modules named
- [ ] `cargo test` and `cargo clippy -- -D warnings` pass

## Target-repo rule

At the start of a port, copy [`.cursor/rules/c-to-rust-port.mdc`](../../rules/c-to-rust-port.mdc) into the **target** repository. Leave `alwaysApply: false` (or enable it only while the port is in progress). Do not leave it always-on in a finished port.
