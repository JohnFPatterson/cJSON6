---
name: port-c-library-to-rust
description: Port a C library to Rust as a safe core crate plus a thin C ABI shim. Use when the user asks to port a C library to Rust, add a C ABI / FFI shim, match C quirks, or build a parse/print parity harness against original C.
---

# Port a C library to Rust

Follow this playbook when moving a C library into a Cargo workspace that existing C callers can still link. Keep the original C sources in-tree as the behavioral spec. Do not "improve" the C API.

Worked example in this repo: `cjson-core`, `cjson-ffi`, `MIGRATION.md`, `PARITY.md`, `parity.sh`.

## When to use

- Porting a C library (parser, codec, data structure) to Rust while preserving C ABI.
- Splitting logic into a safe crate and an `unsafe` FFI crate.

Out of scope unless the user asks: rewriting the API to be more idiomatic, dropping quirks, porting optional companion libraries (silent skip is a bug — name them as out of scope), adding a fuzzer requirement.

## MUST / MUST NOT

| MUST | MUST NOT |
|------|----------|
| Cargo workspace with `{lib}-core` (all logic) and `{lib}-ffi` (shim only) | Put parse/print/tree logic in the FFI crate |
| `#![forbid(unsafe_code)]` on the core crate | Sprinkle `unsafe` through core "just for FFI" |
| FFI is the **only** crate with `unsafe`; every `unsafe` block has a `SAFETY:` comment | Export C functions without documenting pointer contracts |
| Keep original `*.c` / `*.h` in-tree as spec + oracle | Delete C sources once Rust "looks right" |
| Match C **including quirks** (lookup case folding, duplicate keys, print format, limits) | "Fix" the API or normalize output |
| Malformed input returns `Result` / `Error`; library code does not panic on untrusted data | `unwrap` / `expect` / panic on bad input |
| Port the original test suite (Unity or equivalent), same cases | Invent a new suite that happens to pass |
| Differential harness vs C: parse ok/fail, error position, byte-identical compact **and** pretty print | Compare only success/failure or pretty-printed JSON that is "close enough" |
| Write `PARITY.md` (per-input table, quirks with **C source lines**, divergences or "none") | Treat a green `cargo test` as the only evidence |
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

## Behavior

1. Read the C sources before writing Rust. Quote C file:line for quirks in `PARITY.md`.
2. Match print byte-for-byte (compact and pretty), including C `sprintf` formats, integer-vs-float branches, and non-finite → whatever C emits.
3. Match parse failure the way C does, including error cursor / `GetErrorPtr`-style offsets.
4. Preserve documented limits (nesting, circular walks on the C side).
5. Do not panic on malformed input.

## Proof

1. Port the project's existing tests onto `{lib}-core` (keep C tests runnable too).
2. Build a small C oracle (`*-oracle.c` compiling original `.c`) that prints parse status, error offset, compact blob, pretty blob.
3. Run every fixture the project already has (and C tests' golden files) through C and Rust; fail the process if anything diverges.
4. Write `PARITY.md` and `MIGRATION.md` as specified above.
5. `grep -rn "unsafe" --include=*.rs --exclude-dir=target` from the repo root: core should only mention `unsafe` in `forbid(unsafe_code)` docs; every executable `unsafe` is in FFI with `SAFETY:`.

A one-command demo (`make parity` or a root script) that builds both implementations, runs fixtures + original tests, prints a short summary, and exits nonzero on divergence is the preferred handoff — not a requirement of the original architecture, but do it when the user wants a projector/demo bar.

## Done checklist

- [ ] `{lib}-core` has `#![forbid(unsafe_code)]`; all logic lives there
- [ ] `{lib}-ffi` is a thin ABI; every `unsafe` has `SAFETY:`
- [ ] Original C still builds and is the oracle
- [ ] Original test suite ported; C suite still runs
- [ ] Differential harness: parse, error position, compact, pretty
- [ ] `PARITY.md` and `MIGRATION.md` written
- [ ] Out-of-scope modules named
- [ ] `cargo test` and `cargo clippy -- -D warnings` pass

## Target-repo rule

At the start of a port, copy [`.cursor/rules/c-to-rust-port.mdc`](../../rules/c-to-rust-port.mdc) into the **target** repository. Leave `alwaysApply: false` (or enable it only while the port is in progress). Do not leave it always-on in a finished port.
