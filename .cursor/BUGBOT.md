# Bugbot review rules: cjson C-to-Rust migration

This repository ports the C library declared in `cJSON.h` to Rust. `cjson-core` holds all logic and forbids `unsafe`. `cjson-ffi` is the thin C ABI shim and is the only crate allowed to contain `unsafe`. The original C sources stay in-tree as the behavioral spec.

Apply all three rules below on every review. Each violation is a **blocking** Bug.

## Rule 1: `unsafe` outside `cjson-ffi`

If a changed file anywhere outside `cjson-ffi/` adds any of the following, add a **blocking** Bug titled "unsafe outside cjson-ffi", naming the file and line:

- an `unsafe` block, `unsafe fn`, `unsafe impl`, or `unsafe trait`
- an `unsafe extern` block
- `#[allow(unsafe_code)]` or `#![allow(unsafe_code)]`

This covers `cjson-core/src/`, `src/bin/`, `tests/`, `benches/`, `examples/`, `build.rs`, and any other crate in the workspace.

Also add a **blocking** Bug titled "unsafe outside cjson-ffi" if a PR removes, comments out, or weakens `#![forbid(unsafe_code)]` in `cjson-core/src/lib.rs` (for example by changing it to `deny` or `warn`).

The word `unsafe` inside comments, doc comments, or string literals doesn't count, and neither does the `forbid(unsafe_code)` attribute itself.

## Rule 2: every `PARITY_EXCEPTIONS.md` entry has a matching test

`PARITY_EXCEPTIONS.md` lists intentional C-vs-Rust behavior changes. Each table row with an ID of the form `PE-NNN` names a test in its **Test** column.

Add a **blocking** Bug titled "Parity exception without matching test", listing the affected IDs, if any of these is true:

- A `PE-NNN` row's **Test** column is empty, or names a function that isn't a `#[test]` in `cjson-core/tests/exceptions.rs`.
- The named test is marked `#[ignore]`.
- A PR adds a `PE-NNN` row without adding its test in the same PR.
- A PR deletes or renames a test in `cjson-core/tests/exceptions.rs` while a row still refers to the old name.

If the table contains only the placeholder `—` row, there are no exceptions: the rule passes and `cjson-core/tests/exceptions.rs` doesn't need to exist.

## Rule 3: FFI signatures match `cJSON.h` exactly

Check every function in `cjson-ffi` exported with `#[no_mangle]` or `#[unsafe(no_mangle)]` as `pub extern "C" fn` or `pub unsafe extern "C" fn`. Compare it with the declaration of the same name in `cJSON.h`.

Before comparing, expand `CJSON_PUBLIC(type)` to `type`, and resolve the header's typedefs to their underlying C types.

These must all match exactly:

- symbol name
- `extern "C"` calling convention
- parameter count and order (a C `(void)` parameter list means no parameters)
- each parameter type and the return type, using these mappings:

| C | Rust |
|---|------|
| `int` (and `int` typedefs) | `c_int` |
| `char` | `c_char` |
| `size_t` | `usize` |
| `double` | `f64` |
| `void` return | no return type |
| `const T *` | `*const T` |
| `T *` | `*mut T` |
| `const char **` | `*mut *const c_char` |
| struct `S *` / `const S *` | `*mut S` / `*const S`, where `S` is the `#[repr(C)]` type |

For any mismatch, add a **blocking** Bug titled "FFI signature differs from cJSON.h" that quotes the header declaration (with its line number) and the Rust signature.

Also add a **blocking** Bug with that title if either of these is true, unless `MIGRATION.md` names the function as out of scope:

- A `CJSON_PUBLIC` function in `cJSON.h` has no export in `cjson-ffi`.
- A `#[no_mangle]` export in `cjson-ffi` isn't declared in `cJSON.h`.

## Known gaps

These rules don't check:

- `#[repr(C)]` struct field order, types, or layout against the header's structs.
- Non-default calling conventions. If `cJSON.h` switches to `__stdcall` for Windows DLL builds, Rust `extern "C"` (cdecl) differs from it on 32-bit Windows only.
