# Migration: cJSON C → Rust

cJSON 1.7.19 is now a Cargo workspace with two crates. The original `cJSON.c` /
`cJSON.h` remain in the tree as the behavioral specification and as the oracle
used by the differential harness.

## Layout

| Path | Role |
|------|------|
| `cjson-core/` | All parse / print / tree logic. `#![forbid(unsafe_code)]` in `src/lib.rs`. |
| `cjson-ffi/` | Thin C ABI (`cJSON_*` symbols, `#[repr(C)]` `cJSON` struct) so existing C callers can link unchanged. **Only** crate allowed to contain `unsafe`. |
| `cjson-core/tests/unity.rs` | Port of the Unity tests under `tests/*.c` (cJSON core, not Utils). |
| `cjson-core/tests/parity.rs` | Differential harness over `tests/inputs/` (invokes the `parity` binary). |
| `cjson-core/src/bin/parity.rs` | Comparison engine: parse / error pos / compact / pretty. Writes `PARITY.md`. |
| `parity.sh` / `make parity` | Projector demo: build C + Rust, run Unity on both, compare `tests/inputs/`. |
| `tools/cjson_oracle.c` | Original-C oracle (`cJSON_Parse` + both print modes). |

Rust callers should depend on `cjson-core` (`Value`, `parse`, `print`, …).
C callers should link `cjson-ffi` (`crate-type = ["staticlib", "cdylib"]`) and
keep including `cJSON.h`.

## What changed relative to the C API

- **Tree representation (core):** owned `Value` with `Vec` children instead of
  `next`/`prev`/`child` pointers. Insertion order and duplicate object keys are
  preserved. The FFI crate reconstitutes the C linked-list layout.
- **Errors:** malformed input returns `Result<_, ParseError>` / `PrintError`.
  Library code does not `unwrap` or panic on untrusted JSON.
- **Printing:** compact and pretty output is byte-identical to cJSON, including
  `%1.15g` / `%1.17g` float formatting, `%d` for integral `valueint`, and
  `null` for NaN/Infinity. Pretty mode still uses tabs (`:\t`, `\n`, `\t`
  indent) as in `print_object` / `print_array`.
- **Lookups:** `get_object_item` is case-insensitive (ASCII `tolower`);
  `get_object_item_case_sensitive` is not.
- **cJSON_Utils** (`cJSON_Utils.c` / JSON Patch) is **not** ported.
- **`cJSON_InitHooks`:** honored in the FFI crate for C-tree and printed-string
  allocations. `cjson-core` always uses the Rust global allocator.
- **`cJSON_SetValuestring` overlap check** (C refuses overlapping buffers) is
  a C-memory concern; the Rust API replaces the owned byte string.
- **Pointer cycles:** a `Value` is an acyclic tree. FFI `cJSON_Duplicate` of a
  cyclic C list still fails at `CJSON_CIRCULAR_LIMIT` (10000) when converting.

## `grep -rn "unsafe" --include=*.rs`

From the repo root (`--exclude-dir=target`). `cjson-core` only mentions `unsafe`
in the `forbid(unsafe_code)` docs; every executable `unsafe` is in `cjson-ffi`.
Each `unsafe {` block in that crate is preceded by a `SAFETY:` comment.

```
cjson-ffi/src/lib.rs:4://! Every `unsafe` block has a SAFETY comment. JSON logic lives in `cjson-core`.
cjson-ffi/src/lib.rs:6://! All exported `cJSON_*` functions are `unsafe` because they dereference
cjson-ffi/src/lib.rs:47:    pub malloc_fn: Option<unsafe extern "C" fn(usize) -> *mut c_void>,
cjson-ffi/src/lib.rs:48:    pub free_fn: Option<unsafe extern "C" fn(*mut c_void)>,
cjson-ffi/src/lib.rs:52:    allocate: unsafe extern "C" fn(usize) -> *mut c_void,
cjson-ffi/src/lib.rs:53:    deallocate: unsafe extern "C" fn(*mut c_void),
cjson-ffi/src/lib.rs:56:unsafe extern "C" fn libc_malloc(sz: usize) -> *mut c_void {
cjson-ffi/src/lib.rs:58:    unsafe { libc::malloc(sz) }
cjson-ffi/src/lib.rs:61:unsafe extern "C" fn libc_free(p: *mut c_void) {
cjson-ffi/src/lib.rs:63:    unsafe { libc::free(p) }
cjson-ffi/src/lib.rs:79:unsafe impl Send for ErrorLoc {}
cjson-ffi/src/lib.rs:81:unsafe impl Sync for ErrorLoc {}
cjson-ffi/src/lib.rs:118:    unsafe { (h.allocate)(size) }
cjson-ffi/src/lib.rs:128:    unsafe { (h.deallocate)(p) }
cjson-ffi/src/lib.rs:137:    unsafe { ptr::write_bytes(p, 0, 1) };
cjson-ffi/src/lib.rs:149:    unsafe {
cjson-ffi/src/lib.rs:161:    let bytes = unsafe { CStr::from_ptr(s).to_bytes() };
cjson-ffi/src/lib.rs:179:    unsafe {
cjson-ffi/src/lib.rs:217:unsafe fn cjson_to_value(item: *const cJSON, depth: usize) -> Option<Value> {
cjson-ffi/src/lib.rs:226:    let raw = unsafe { &*item };
cjson-ffi/src/lib.rs:243:        let bytes = unsafe { CStr::from_ptr(raw.valuestring).to_bytes() };
cjson-ffi/src/lib.rs:248:        let bytes = unsafe { CStr::from_ptr(raw.string).to_bytes() };
cjson-ffi/src/lib.rs:254:        let ch = unsafe { cjson_to_value(child, depth + 1)? };
cjson-ffi/src/lib.rs:257:        child = unsafe { (*child).next };
cjson-ffi/src/lib.rs:267:    let value = match unsafe { cjson_to_value(item, 0) } {
cjson-ffi/src/lib.rs:282:unsafe fn slice_from_ptr_len<'a>(value: *const c_char, len: usize) -> Option<&'a [u8]> {
cjson-ffi/src/lib.rs:287:    Some(unsafe { std::slice::from_raw_parts(value as *const u8, len) })
cjson-ffi/src/lib.rs:291:pub unsafe extern "C" fn cJSON_Version() -> *const c_char {
cjson-ffi/src/lib.rs:296:pub unsafe extern "C" fn cJSON_InitHooks(hooks: *mut cJSON_Hooks) {
cjson-ffi/src/lib.rs:304:    let hooks = unsafe { &*hooks };
cjson-ffi/src/lib.rs:316:pub unsafe extern "C" fn cJSON_GetErrorPtr() -> *const c_char {
cjson-ffi/src/lib.rs:323:    unsafe { loc.json.add(loc.position) }
cjson-ffi/src/lib.rs:338:    let slice = match unsafe { slice_from_ptr_len(value, buffer_length) } {
cjson-ffi/src/lib.rs:353:                unsafe { *return_parse_end = value.add(success.parse_end) };
cjson-ffi/src/lib.rs:368:                unsafe { *return_parse_end = value.add(pos) };
cjson-ffi/src/lib.rs:376:pub unsafe extern "C" fn cJSON_Parse(value: *const c_char) -> *mut cJSON {
cjson-ffi/src/lib.rs:382:    let len = unsafe { libc::strlen(value) } + 1;
cjson-ffi/src/lib.rs:387:pub unsafe extern "C" fn cJSON_ParseWithLength(
cjson-ffi/src/lib.rs:395:pub unsafe extern "C" fn cJSON_ParseWithOpts(
cjson-ffi/src/lib.rs:405:    let len = unsafe { libc::strlen(value) } + 1;
cjson-ffi/src/lib.rs:410:pub unsafe extern "C" fn cJSON_ParseWithLengthOpts(
cjson-ffi/src/lib.rs:425:pub unsafe extern "C" fn cJSON_Print(item: *const cJSON) -> *mut c_char {
cjson-ffi/src/lib.rs:430:pub unsafe extern "C" fn cJSON_PrintUnformatted(item: *const cJSON) -> *mut c_char {
cjson-ffi/src/lib.rs:435:pub unsafe extern "C" fn cJSON_PrintBuffered(
cjson-ffi/src/lib.rs:444:pub unsafe extern "C" fn cJSON_PrintPreallocated(
cjson-ffi/src/lib.rs:454:    let value = match unsafe { cjson_to_value(item, 0) } {
cjson-ffi/src/lib.rs:470:    unsafe {
cjson-ffi/src/lib.rs:478:pub unsafe extern "C" fn cJSON_Delete(item: *mut cJSON) {
cjson-ffi/src/lib.rs:482:        let next = unsafe { (*cur).next };
cjson-ffi/src/lib.rs:483:        let is_ref = unsafe { (*cur).type_ & CJSON_IS_REFERENCE } != 0;
cjson-ffi/src/lib.rs:484:        let str_const = unsafe { (*cur).type_ & CJSON_STRING_IS_CONST } != 0;
cjson-ffi/src/lib.rs:487:            let child = unsafe { (*cur).child };
cjson-ffi/src/lib.rs:492:            let vs = unsafe { (*cur).valuestring };
cjson-ffi/src/lib.rs:499:            let s = unsafe { (*cur).string };
cjson-ffi/src/lib.rs:510:pub unsafe extern "C" fn cJSON_GetArraySize(array: *const cJSON) -> c_int {
cjson-ffi/src/lib.rs:516:    let mut child = unsafe { (*array).child };
cjson-ffi/src/lib.rs:519:        child = unsafe { (*child).next };
cjson-ffi/src/lib.rs:525:pub unsafe extern "C" fn cJSON_GetArrayItem(array: *const cJSON, index: c_int) -> *mut cJSON {
cjson-ffi/src/lib.rs:530:    let mut child = unsafe { (*array).child };
cjson-ffi/src/lib.rs:535:        child = unsafe { (*child).next };
cjson-ffi/src/lib.rs:545:    let key = unsafe { CStr::from_ptr(name).to_bytes() };
cjson-ffi/src/lib.rs:546:    let mut cur = unsafe { (*object).child };
cjson-ffi/src/lib.rs:548:        let kptr = unsafe { (*cur).string };
cjson-ffi/src/lib.rs:551:            let kb = unsafe { CStr::from_ptr(kptr).to_bytes() };
cjson-ffi/src/lib.rs:566:        cur = unsafe { (*cur).next };
cjson-ffi/src/lib.rs:572:pub unsafe extern "C" fn cJSON_GetObjectItem(
cjson-ffi/src/lib.rs:580:pub unsafe extern "C" fn cJSON_GetObjectItemCaseSensitive(
cjson-ffi/src/lib.rs:588:pub unsafe extern "C" fn cJSON_HasObjectItem(object: *const cJSON, string: *const c_char) -> c_int {
cjson-ffi/src/lib.rs:593:pub unsafe extern "C" fn cJSON_GetStringValue(item: *const cJSON) -> *mut c_char {
cjson-ffi/src/lib.rs:598:    if unsafe { c_type_low((*item).type_) } != CJSON_STRING {
cjson-ffi/src/lib.rs:602:    unsafe { (*item).valuestring }
cjson-ffi/src/lib.rs:606:pub unsafe extern "C" fn cJSON_GetNumberValue(item: *const cJSON) -> c_double {
cjson-ffi/src/lib.rs:611:    if unsafe { c_type_low((*item).type_) } != CJSON_NUMBER {
cjson-ffi/src/lib.rs:615:    unsafe { (*item).valuedouble }
cjson-ffi/src/lib.rs:623:    i32::from(unsafe { c_type_low((*item).type_) } == t)
cjson-ffi/src/lib.rs:627:pub unsafe extern "C" fn cJSON_IsInvalid(item: *const cJSON) -> c_int {
cjson-ffi/src/lib.rs:631:pub unsafe extern "C" fn cJSON_IsFalse(item: *const cJSON) -> c_int {
cjson-ffi/src/lib.rs:635:pub unsafe extern "C" fn cJSON_IsTrue(item: *const cJSON) -> c_int {
cjson-ffi/src/lib.rs:639:pub unsafe extern "C" fn cJSON_IsBool(item: *const cJSON) -> c_int {
cjson-ffi/src/lib.rs:644:    i32::from(unsafe { (*item).type_ & (CJSON_TRUE | CJSON_FALSE) } != 0)
cjson-ffi/src/lib.rs:647:pub unsafe extern "C" fn cJSON_IsNull(item: *const cJSON) -> c_int {
cjson-ffi/src/lib.rs:651:pub unsafe extern "C" fn cJSON_IsNumber(item: *const cJSON) -> c_int {
cjson-ffi/src/lib.rs:655:pub unsafe extern "C" fn cJSON_IsString(item: *const cJSON) -> c_int {
cjson-ffi/src/lib.rs:659:pub unsafe extern "C" fn cJSON_IsArray(item: *const cJSON) -> c_int {
cjson-ffi/src/lib.rs:663:pub unsafe extern "C" fn cJSON_IsObject(item: *const cJSON) -> c_int {
cjson-ffi/src/lib.rs:667:pub unsafe extern "C" fn cJSON_IsRaw(item: *const cJSON) -> c_int {
cjson-ffi/src/lib.rs:675:        unsafe { (*item).type_ = t };
cjson-ffi/src/lib.rs:681:pub unsafe extern "C" fn cJSON_CreateNull() -> *mut cJSON {
cjson-ffi/src/lib.rs:685:pub unsafe extern "C" fn cJSON_CreateTrue() -> *mut cJSON {
cjson-ffi/src/lib.rs:689:pub unsafe extern "C" fn cJSON_CreateFalse() -> *mut cJSON {
cjson-ffi/src/lib.rs:693:pub unsafe extern "C" fn cJSON_CreateBool(boolean: c_int) -> *mut cJSON {
cjson-ffi/src/lib.rs:702:pub unsafe extern "C" fn cJSON_CreateNumber(num: c_double) -> *mut cJSON {
cjson-ffi/src/lib.rs:708:    unsafe {
cjson-ffi/src/lib.rs:717:pub unsafe extern "C" fn cJSON_CreateString(string: *const c_char) -> *mut cJSON {
cjson-ffi/src/lib.rs:723:    unsafe {
cjson-ffi/src/lib.rs:735:pub unsafe extern "C" fn cJSON_CreateRaw(raw: *const c_char) -> *mut cJSON {
cjson-ffi/src/lib.rs:741:    unsafe {
cjson-ffi/src/lib.rs:753:pub unsafe extern "C" fn cJSON_CreateArray() -> *mut cJSON {
cjson-ffi/src/lib.rs:757:pub unsafe extern "C" fn cJSON_CreateObject() -> *mut cJSON {
cjson-ffi/src/lib.rs:762:pub unsafe extern "C" fn cJSON_CreateStringReference(string: *const c_char) -> *mut cJSON {
cjson-ffi/src/lib.rs:768:    unsafe {
cjson-ffi/src/lib.rs:776:pub unsafe extern "C" fn cJSON_CreateObjectReference(child: *const cJSON) -> *mut cJSON {
cjson-ffi/src/lib.rs:782:    unsafe {
cjson-ffi/src/lib.rs:790:pub unsafe extern "C" fn cJSON_CreateArrayReference(child: *const cJSON) -> *mut cJSON {
cjson-ffi/src/lib.rs:796:    unsafe {
cjson-ffi/src/lib.rs:808:    unsafe {
cjson-ffi/src/lib.rs:825:pub unsafe extern "C" fn cJSON_AddItemToArray(array: *mut cJSON, item: *mut cJSON) -> c_int {
cjson-ffi/src/lib.rs:839:    unsafe {
cjson-ffi/src/lib.rs:864:pub unsafe extern "C" fn cJSON_AddItemToObject(
cjson-ffi/src/lib.rs:873:pub unsafe extern "C" fn cJSON_AddItemToObjectCS(
cjson-ffi/src/lib.rs:882:pub unsafe extern "C" fn cJSON_AddItemReferenceToArray(
cjson-ffi/src/lib.rs:894:    unsafe {
cjson-ffi/src/lib.rs:905:pub unsafe extern "C" fn cJSON_AddItemReferenceToObject(
cjson-ffi/src/lib.rs:918:    unsafe {
cjson-ffi/src/lib.rs:933:        unsafe { cJSON_Delete(item) };
cjson-ffi/src/lib.rs:939:pub unsafe extern "C" fn cJSON_AddNullToObject(
cjson-ffi/src/lib.rs:946:pub unsafe extern "C" fn cJSON_AddTrueToObject(
cjson-ffi/src/lib.rs:953:pub unsafe extern "C" fn cJSON_AddFalseToObject(
cjson-ffi/src/lib.rs:960:pub unsafe extern "C" fn cJSON_AddBoolToObject(
cjson-ffi/src/lib.rs:968:pub unsafe extern "C" fn cJSON_AddNumberToObject(
cjson-ffi/src/lib.rs:976:pub unsafe extern "C" fn cJSON_AddStringToObject(
cjson-ffi/src/lib.rs:984:pub unsafe extern "C" fn cJSON_AddRawToObject(
cjson-ffi/src/lib.rs:992:pub unsafe extern "C" fn cJSON_AddObjectToObject(
cjson-ffi/src/lib.rs:999:pub unsafe extern "C" fn cJSON_AddArrayToObject(
cjson-ffi/src/lib.rs:1007:pub unsafe extern "C" fn cJSON_DetachItemViaPointer(
cjson-ffi/src/lib.rs:1015:    unsafe {
cjson-ffi/src/lib.rs:1037:pub unsafe extern "C" fn cJSON_DetachItemFromArray(array: *mut cJSON, which: c_int) -> *mut cJSON {
cjson-ffi/src/lib.rs:1042:pub unsafe extern "C" fn cJSON_DeleteItemFromArray(array: *mut cJSON, which: c_int) {
cjson-ffi/src/lib.rs:1047:pub unsafe extern "C" fn cJSON_DetachItemFromObject(
cjson-ffi/src/lib.rs:1055:pub unsafe extern "C" fn cJSON_DetachItemFromObjectCaseSensitive(
cjson-ffi/src/lib.rs:1063:pub unsafe extern "C" fn cJSON_DeleteItemFromObject(object: *mut cJSON, string: *const c_char) {
cjson-ffi/src/lib.rs:1068:pub unsafe extern "C" fn cJSON_DeleteItemFromObjectCaseSensitive(
cjson-ffi/src/lib.rs:1076:pub unsafe extern "C" fn cJSON_InsertItemInArray(
cjson-ffi/src/lib.rs:1089:    unsafe {
cjson-ffi/src/lib.rs:1106:pub unsafe extern "C" fn cJSON_ReplaceItemViaPointer(
cjson-ffi/src/lib.rs:1118:    unsafe {
cjson-ffi/src/lib.rs:1148:pub unsafe extern "C" fn cJSON_ReplaceItemInArray(
cjson-ffi/src/lib.rs:1160:pub unsafe extern "C" fn cJSON_ReplaceItemInObject(
cjson-ffi/src/lib.rs:1169:    unsafe {
cjson-ffi/src/lib.rs:1183:pub unsafe extern "C" fn cJSON_ReplaceItemInObjectCaseSensitive(
cjson-ffi/src/lib.rs:1192:    unsafe {
cjson-ffi/src/lib.rs:1210:pub unsafe extern "C" fn cJSON_Duplicate(item: *const cJSON, recurse: c_int) -> *mut cJSON {
cjson-ffi/src/lib.rs:1215:    let value = unsafe { cjson_to_value(item, 0) };
cjson-ffi/src/lib.rs:1234:pub unsafe extern "C" fn cJSON_Compare(
cjson-ffi/src/lib.rs:1243:    let av = unsafe { cjson_to_value(a, 0) };
cjson-ffi/src/lib.rs:1245:    let bv = unsafe { cjson_to_value(b, 0) };
cjson-ffi/src/lib.rs:1253:pub unsafe extern "C" fn cJSON_Minify(json: *mut c_char) {
cjson-ffi/src/lib.rs:1258:    let len = unsafe { libc::strlen(json) };
cjson-ffi/src/lib.rs:1259:    let slice = unsafe { std::slice::from_raw_parts_mut(json as *mut u8, len + 1) };
cjson-ffi/src/lib.rs:1264:pub unsafe extern "C" fn cJSON_SetNumberHelper(object: *mut cJSON, number: c_double) -> c_double {
cjson-ffi/src/lib.rs:1269:    unsafe {
cjson-ffi/src/lib.rs:1277:pub unsafe extern "C" fn cJSON_SetValuestring(
cjson-ffi/src/lib.rs:1285:    unsafe {
cjson-ffi/src/lib.rs:1303:pub unsafe extern "C" fn cJSON_malloc(size: usize) -> *mut c_void {
cjson-ffi/src/lib.rs:1308:pub unsafe extern "C" fn cJSON_free(object: *mut c_void) {
cjson-ffi/src/lib.rs:1313:pub unsafe extern "C" fn cJSON_CreateIntArray(numbers: *const c_int, count: c_int) -> *mut cJSON {
cjson-ffi/src/lib.rs:1323:        let n = unsafe { cJSON_CreateNumber(*numbers.add(i as usize) as f64) };
cjson-ffi/src/lib.rs:1338:pub unsafe extern "C" fn cJSON_CreateFloatArray(numbers: *const f32, count: c_int) -> *mut cJSON {
cjson-ffi/src/lib.rs:1348:        let n = unsafe { cJSON_CreateNumber(f64::from(*numbers.add(i as usize))) };
cjson-ffi/src/lib.rs:1363:pub unsafe extern "C" fn cJSON_CreateDoubleArray(
cjson-ffi/src/lib.rs:1376:        let n = unsafe { cJSON_CreateNumber(*numbers.add(i as usize)) };
cjson-ffi/src/lib.rs:1391:pub unsafe extern "C" fn cJSON_CreateStringArray(
cjson-ffi/src/lib.rs:1404:        let n = unsafe { cJSON_CreateString(*strings.add(i as usize)) };
cjson-core/src/lib.rs:3://! This crate contains all parse/print/tree logic and forbids `unsafe`.
cjson-core/src/lib.rs:5:#![forbid(unsafe_code)]
```

`cjson-core` contains **no** `unsafe` (enforced by `forbid(unsafe_code)`).

## Behavior differences found

Parse and print of every file in `tests/inputs/` match the original C library
byte-for-byte (see `PARITY.md`). The following are C quirks the port **keeps**,
not accidental drift:

| Topic | C source | Notes |
|-------|----------|--------|
| Case-insensitive object lookup | `get_object_item` / `case_insensitive_strcmp` | Default `cJSON_GetObjectItem` ignores ASCII case. |
| Duplicate keys | `parse_object` | Keys are appended; first match wins on lookup. |
| `cJSON_Compare` + duplicate keys | `cJSON_Compare` → `get_object_item` | Each member is compared to the *first* same-named member of the other object, so `{"a":1,"a":2}` is not equal to itself. |
| Float print | `print_number` | `"%1.15g"` then `"%1.17g"` if `compare_double` (relative `DBL_EPSILON`) fails; integers use `"%d"` when `d == (double)valueint`. |
| NaN / Inf | `print_number` | Printed as `null`. |
| Nesting cap | `CJSON_NESTING_LIMIT` (1000) | Parse and print reject deeper arrays/objects. |
| Surrogate pairs | `utf16_literal_to_utf8` | High `\uD800–\uDBFF` must be followed by `\uDC00–\uDFFF`. |
| BOM skip | `skip_utf8_bom` | BOM skipped only if `can_access_at_index(..., 4)` (at least 5 bytes). |
| Invalid `\u` hex | `parse_hex4` | Returns `0`, same as `\u0000`. |

Intentional Rust-side differences (documented, not input-file divergences):

- Utils / JSON Patch API is absent.
- Custom malloc hooks do not affect `cjson-core`.
- Allocation-failure unit tests that inject `failing_malloc` via hooks are C-only.
- `Value` stores strings as `Vec<u8>` (cJSON copies raw bytes, including invalid UTF-8). Printing still stops at an interior NUL, matching C `strlen` in `print_string_ptr`.
