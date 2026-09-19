# cJSON C vs Rust parity

Differential run of every file in `tests/inputs/` through the original C library (`cJSON.c`) and the Rust `cjson-core` port.

## Method

- **C:** `cJSON_Parse` + `cJSON_PrintUnformatted` + `cJSON_Print` (oracle at `tools/cjson_oracle.c`).
- **Rust:** `cjson_core::parse` + `print_unformatted` + `print`.
- Printed output is compared as raw bytes.

## Summary

21 files, 0 parse/print divergences.

| File | C parse | Rust parse | Compact identical | Pretty identical | Notes |
|------|---------|------------|-------------------|------------------|-------|
| `test1` | ok | ok | yes | yes |  |
| `test1.expected` | ok | ok | yes | yes |  |
| `test10` | ok | ok | yes | yes |  |
| `test10.expected` | ok | ok | yes | yes |  |
| `test11` | ok | ok | yes | yes |  |
| `test11.expected` | ok | ok | yes | yes |  |
| `test2` | ok | ok | yes | yes |  |
| `test2.expected` | ok | ok | yes | yes |  |
| `test3` | ok | ok | yes | yes |  |
| `test3.expected` | ok | ok | yes | yes |  |
| `test4` | ok | ok | yes | yes |  |
| `test4.expected` | ok | ok | yes | yes |  |
| `test5` | ok | ok | yes | yes |  |
| `test5.expected` | ok | ok | yes | yes |  |
| `test6` | fail | fail | yes | yes | both failed to parse (expected for non-JSON such as test6) |
| `test7` | ok | ok | yes | yes |  |
| `test7.expected` | ok | ok | yes | yes |  |
| `test8` | ok | ok | yes | yes |  |
| `test8.expected` | ok | ok | yes | yes |  |
| `test9` | ok | ok | yes | yes |  |
| `test9.expected` | ok | ok | yes | yes |  |

## Documented C quirks (not divergences)

These behaviors were **matched** by the Rust port; they are listed so a reader can find the C source that specifies them.

| Behavior | C source |
|----------|----------|
| `cJSON_GetObjectItem` is case-insensitive; `CaseSensitive` is not | `cJSON.c` `get_object_item` / `case_insensitive_strcmp` |
| Objects preserve insertion order and allow duplicate keys | `parse_object` appends to the child linked list; no key uniquing |
| `cJSON_Compare` looks up object members by key, so duplicate keys compare against the **first** match | `cJSON.c` `cJSON_Compare` calling `get_object_item` |
| Floats print with `sprintf("%1.15g")`, falling back to `"%1.17g"` if `compare_double` fails | `cJSON.c` `print_number` |
| Integers that equal `(double)valueint` print with `%d` | `cJSON.c` `print_number` |
| NaN and Infinity print as `null` | `cJSON.c` `print_number` (`isnan` / `isinf`) |
| Nesting deeper than `CJSON_NESTING_LIMIT` (1000) is rejected | `parse_array` / `parse_object` / `print_array` / `print_object` |
| UTF-16 surrogate pairs via `\uD800`-`\uDBFF` + `\uDC00`-`\uDFFF` | `utf16_literal_to_utf8` |
| UTF-8 BOM is skipped only when at least 5 bytes are addressable | `skip_utf8_bom` (`can_access_at_index(buffer, 4)`) |
| `parse_hex4` returns 0 for invalid hex (same as `\u0000`) | `parse_hex4` |

## Divergences

None. Every input file produced the same parse success/failure and byte-identical compact and pretty output.
