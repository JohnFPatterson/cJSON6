# cJSON C vs Rust parity

Differential run of every file in `tests/inputs/` through the original C library (`cJSON.c`) and the Rust `cjson-core` port.

Produced by `make parity` (or `./parity.sh`).

## Method

- **C:** `cJSON_Parse` + `cJSON_PrintUnformatted` + `cJSON_Print` (oracle at `tools/cjson_oracle.c`). Failure offset is `cJSON_GetErrorPtr() - buffer` (`cJSON.c:95`).
- **Rust:** `cjson_core::parse` + `print_unformatted` + `print`. `ParseError.position` matches that offset.
- Compact and pretty printed output is compared as raw bytes.
- The original Unity suites under `tests/*.c` are compiled against `cJSON.c`; the port lives in `cjson-core/tests/unity.rs`.

## Summary

21 files, 21 byte-identical (parse success/failure, error position, compact print, pretty print).

## Unity

| Implementation | Passed | Failed | Suites |
|----------------|--------|--------|--------|
| C (`tests/*.c` + `cJSON.c`) | 153 | 0 | 18/18 |
| Rust (`cjson-core/tests/unity.rs`) | 68 | 0 | 18/18 |

C Unity counts each `TEST` function (153 across 18 files). The Rust port covers the same 18 suites in 68 `#[test]` functions; some C cases were grouped. A failure in either implementation is a parity failure.

## Per-file results

| File | C parse | C error pos | Rust parse | Rust error pos | Compact | Pretty | Match | Notes |
|------|---------|-------------|------------|----------------|---------|--------|-------|-------|
| `test1` | ok | — | ok | — | yes (360 B) | yes (474 B) | yes |  |
| `test1.expected` | ok | — | ok | — | yes (360 B) | yes (474 B) | yes |  |
| `test10` | ok | — | ok | — | yes (72 B) | yes (78 B) | yes |  |
| `test10.expected` | ok | — | ok | — | yes (72 B) | yes (78 B) | yes |  |
| `test11` | ok | — | ok | — | yes (118 B) | yes (147 B) | yes |  |
| `test11.expected` | ok | — | ok | — | yes (118 B) | yes (147 B) | yes |  |
| `test2` | ok | — | ok | — | yes (183 B) | yes (268 B) | yes |  |
| `test2.expected` | ok | — | ok | — | yes (183 B) | yes (268 B) | yes |  |
| `test3` | ok | — | ok | — | yes (389 B) | yes (505 B) | yes |  |
| `test3.expected` | ok | — | ok | — | yes (389 B) | yes (505 B) | yes |  |
| `test4` | ok | — | ok | — | yes (2710 B) | yes (3285 B) | yes |  |
| `test4.expected` | ok | — | ok | — | yes (2710 B) | yes (3285 B) | yes |  |
| `test5` | ok | — | ok | — | yes (613 B) | yes (900 B) | yes |  |
| `test5.expected` | ok | — | ok | — | yes (613 B) | yes (900 B) | yes |  |
| `test6` | fail | 0 | fail | 0 | yes | yes | yes | both failed to parse (expected for non-JSON such as test6); error pos 0 (cJSON.c:95 cJSON_GetErrorPtr) |
| `test7` | ok | — | ok | — | yes (278 B) | yes (347 B) | yes |  |
| `test7.expected` | ok | — | ok | — | yes (278 B) | yes (347 B) | yes |  |
| `test8` | ok | — | ok | — | yes (181 B) | yes (228 B) | yes |  |
| `test8.expected` | ok | — | ok | — | yes (181 B) | yes (228 B) | yes |  |
| `test9` | ok | — | ok | — | yes (26 B) | yes (34 B) | yes |  |
| `test9.expected` | ok | — | ok | — | yes (26 B) | yes (34 B) | yes |  |

## Documented C quirks (not divergences)

These behaviors were **matched** by the Rust port; they are listed so a reader can find the C source that specifies them.

| Behavior | C source |
|----------|----------|
| `cJSON_GetErrorPtr` is `global_error.json + global_error.position` | `cJSON.c:95` |
| Parse failure stores `buffer.offset`, or `buffer.length - 1` at EOF | `cJSON.c:1206–1232` |
| `cJSON_Parse` uses `strlen(value) + 1` (includes the NUL) | `cJSON.c:1141` / `1151` |
| `cJSON_GetObjectItem` is case-insensitive; `CaseSensitive` is not | `cJSON.c:137` `case_insensitive_strcmp`, `cJSON.c:2006` |
| Objects preserve insertion order and allow duplicate keys | `parse_object` appends to the child linked list; no key uniquing |
| `cJSON_Compare` looks up object members by key, so duplicate keys compare against the **first** match | `cJSON.c` `cJSON_Compare` calling `get_object_item` |
| Floats print with `sprintf("%1.15g")`, falling back to `"%1.17g"` if `compare_double` fails | `cJSON.c:605` `print_number` (`%1.15g` at 632) |
| Integers that equal `(double)valueint` print with `%d` | `cJSON.c:625` |
| NaN and Infinity print as `null` | `cJSON.c:621` (`isnan` / `isinf`) |
| Nesting deeper than `CJSON_NESTING_LIMIT` (1000) is rejected | `parse_array` / `parse_object` / `print_array` / `print_object` |
| UTF-16 surrogate pairs via `\uD800`-`\uDBFF` + `\uDC00`-`\uDFFF` | `utf16_literal_to_utf8` (`cJSON.c:712`) |
| UTF-8 BOM is skipped only when at least 5 bytes are addressable | `skip_utf8_bom` (`cJSON.c:1125`, `can_access_at_index(buffer, 4)`) |
| `parse_hex4` returns 0 for invalid hex (same as `\u0000`) | `parse_hex4` (`cJSON.c:675`) |

## Divergences

None. Every input file produced the same parse success/failure, the same error position on failure, and byte-identical compact and pretty output.
