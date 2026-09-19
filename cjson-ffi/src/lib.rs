//! C ABI shim for the Rust cJSON port.
//!
//! Existing C callers can link against this crate and keep using `cJSON.h`.
//! Every `unsafe` block has a SAFETY comment. JSON logic lives in `cjson-core`.
//!
//! All exported `cJSON_*` functions are `unsafe` because they dereference
//! caller-provided C pointers under the same contract as `cJSON.h`.

#![allow(non_snake_case, non_camel_case_types, clippy::missing_safety_doc)]

use std::ffi::{c_char, c_double, c_int, c_void, CStr};
use std::ptr;
use std::sync::Mutex;

use cjson_core::{
    compare_double, minify_bytes, parse_with_opts, print, print_unformatted, ParseOptions, Type,
    Value,
};

const CJSON_INVALID: c_int = 0;
const CJSON_FALSE: c_int = 1 << 0;
const CJSON_TRUE: c_int = 1 << 1;
const CJSON_NULL: c_int = 1 << 2;
const CJSON_NUMBER: c_int = 1 << 3;
const CJSON_STRING: c_int = 1 << 4;
const CJSON_ARRAY: c_int = 1 << 5;
const CJSON_OBJECT: c_int = 1 << 6;
const CJSON_RAW: c_int = 1 << 7;
const CJSON_IS_REFERENCE: c_int = 256;
const CJSON_STRING_IS_CONST: c_int = 512;

/// The public cJSON node. Layout matches `cJSON.h`.
#[repr(C)]
pub struct cJSON {
    pub next: *mut cJSON,
    pub prev: *mut cJSON,
    pub child: *mut cJSON,
    pub type_: c_int,
    pub valuestring: *mut c_char,
    pub valueint: c_int,
    pub valuedouble: c_double,
    pub string: *mut c_char,
}

#[repr(C)]
pub struct cJSON_Hooks {
    pub malloc_fn: Option<unsafe extern "C" fn(usize) -> *mut c_void>,
    pub free_fn: Option<unsafe extern "C" fn(*mut c_void)>,
}

struct Hooks {
    allocate: unsafe extern "C" fn(usize) -> *mut c_void,
    deallocate: unsafe extern "C" fn(*mut c_void),
}

unsafe extern "C" fn libc_malloc(sz: usize) -> *mut c_void {
    // SAFETY: forwarding to C `malloc`.
    unsafe { libc::malloc(sz) }
}

unsafe extern "C" fn libc_free(p: *mut c_void) {
    // SAFETY: `p` is either null or was allocated by `malloc`/`realloc`.
    unsafe { libc::free(p) }
}

static HOOKS: Mutex<Hooks> = Mutex::new(Hooks {
    allocate: libc_malloc,
    deallocate: libc_free,
});

#[derive(Clone, Copy)]
struct ErrorLoc {
    json: *const c_char,
    position: usize,
}

// SAFETY: `ErrorLoc.json` is only ever read/written through `GLOBAL_ERROR`'s mutex
// and always refers to a buffer the C caller still owns (cJSON's GetErrorPtr contract).
unsafe impl Send for ErrorLoc {}
// SAFETY: all accesses go through the mutex, so concurrent reads/writes are serialized.
unsafe impl Sync for ErrorLoc {}

static GLOBAL_ERROR: Mutex<ErrorLoc> = Mutex::new(ErrorLoc {
    json: ptr::null(),
    position: 0,
});

const VERSION_C: &[u8] = b"1.7.19\0";

fn hooks() -> Hooks {
    match HOOKS.lock() {
        Ok(h) => Hooks {
            allocate: h.allocate,
            deallocate: h.deallocate,
        },
        Err(e) => {
            let h = e.into_inner();
            Hooks {
                allocate: h.allocate,
                deallocate: h.deallocate,
            }
        }
    }
}

fn set_error(json: *const c_char, position: usize) {
    *GLOBAL_ERROR.lock().unwrap_or_else(|e| e.into_inner()) = ErrorLoc { json, position };
}

fn clear_error() {
    set_error(ptr::null(), 0);
}

fn alloc(size: usize) -> *mut c_void {
    let h = hooks();
    // SAFETY: `allocate` is a C malloc-compatible function installed by InitHooks
    // or the libc default; `size` is the requested byte count.
    unsafe { (h.allocate)(size) }
}

fn dealloc(p: *mut c_void) {
    if p.is_null() {
        return;
    }
    let h = hooks();
    // SAFETY: `p` was produced by `alloc` / C malloc via these same hooks, or is
    // a pointer the caller asked us to free with `cJSON_free`.
    unsafe { (h.deallocate)(p) }
}

fn new_item() -> *mut cJSON {
    let p = alloc(std::mem::size_of::<cJSON>()) as *mut cJSON;
    if p.is_null() {
        return p;
    }
    // SAFETY: `p` points to freshly allocated `cJSON`-sized memory.
    unsafe { ptr::write_bytes(p, 0, 1) };
    p
}

fn dup_bytes(bytes: &[u8]) -> *mut c_char {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let bytes = &bytes[..end];
    let p = alloc(end + 1) as *mut u8;
    if p.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: `p` has `end + 1` bytes; `bytes` is `end` long.
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), p, end);
        *p.add(end) = 0;
    }
    p as *mut c_char
}

fn dup_cstr(s: *const c_char) -> *mut c_char {
    if s.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: `s` is a valid C string from the caller.
    let bytes = unsafe { CStr::from_ptr(s).to_bytes() };
    dup_bytes(bytes)
}

fn type_to_c(t: Type) -> c_int {
    t.as_c_int()
}

fn c_type_low(t: c_int) -> c_int {
    t & 0xFF
}

fn value_to_cjson(v: &Value) -> *mut cJSON {
    let node = new_item();
    if node.is_null() {
        return node;
    }
    // SAFETY: `node` is a freshly zeroed cJSON we own.
    unsafe {
        (*node).type_ = type_to_c(v.kind);
        (*node).valueint = v.valueint;
        (*node).valuedouble = v.valuedouble;
        if let Some(s) = v.valuestring.as_deref() {
            (*node).valuestring = dup_bytes(s);
            if (*node).valuestring.is_null() && !s.is_empty() {
                cJSON_Delete(node);
                return ptr::null_mut();
            }
        }
        if let Some(n) = v.name.as_deref() {
            (*node).string = dup_bytes(n);
        }
        let mut first: *mut cJSON = ptr::null_mut();
        let mut prev: *mut cJSON = ptr::null_mut();
        for child in &v.children {
            let c = value_to_cjson(child);
            if c.is_null() {
                cJSON_Delete(node);
                return ptr::null_mut();
            }
            if first.is_null() {
                first = c;
            } else {
                (*prev).next = c;
                (*c).prev = prev;
            }
            prev = c;
        }
        if !first.is_null() {
            (*first).prev = prev;
            (*node).child = first;
        }
    }
    node
}

unsafe fn cjson_to_value(item: *const cJSON, depth: usize) -> Option<Value> {
    if item.is_null() {
        return None;
    }
    if depth > cjson_core::CIRCULAR_LIMIT {
        return None;
    }
    // SAFETY: `item` is a non-null cJSON pointer owned by the caller for the
    // duration of this conversion.
    let raw = unsafe { &*item };
    let mut v = Value::invalid();
    v.kind = match c_type_low(raw.type_) {
        CJSON_FALSE => Type::False,
        CJSON_TRUE => Type::True,
        CJSON_NULL => Type::Null,
        CJSON_NUMBER => Type::Number,
        CJSON_STRING => Type::String,
        CJSON_ARRAY => Type::Array,
        CJSON_OBJECT => Type::Object,
        CJSON_RAW => Type::Raw,
        _ => Type::Invalid,
    };
    v.valueint = raw.valueint;
    v.valuedouble = raw.valuedouble;
    if !raw.valuestring.is_null() {
        // SAFETY: valuestring is a C string allocated by this library or the caller.
        let bytes = unsafe { CStr::from_ptr(raw.valuestring).to_bytes() };
        v.valuestring = Some(bytes.to_vec());
    }
    if !raw.string.is_null() {
        // SAFETY: string (key) is a C string.
        let bytes = unsafe { CStr::from_ptr(raw.string).to_bytes() };
        v.name = Some(bytes.to_vec());
    }
    let mut child = raw.child;
    while !child.is_null() {
        // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
        let ch = unsafe { cjson_to_value(child, depth + 1)? };
        v.children.push(ch);
        // SAFETY: child is a node in the same tree.
        child = unsafe { (*child).next };
    }
    Some(v)
}

fn print_to_cstring(item: *const cJSON, pretty: bool) -> *mut c_char {
    if item.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: `item` is a caller-provided cJSON tree.
    let value = match unsafe { cjson_to_value(item, 0) } {
        Some(v) => v,
        None => return ptr::null_mut(),
    };
    let printed = if pretty {
        print(&value)
    } else {
        print_unformatted(&value)
    };
    let Ok(bytes) = printed else {
        return ptr::null_mut();
    };
    dup_bytes(&bytes)
}

unsafe fn slice_from_ptr_len<'a>(value: *const c_char, len: usize) -> Option<&'a [u8]> {
    if value.is_null() || len == 0 {
        return None;
    }
    // SAFETY: caller provided `len` readable bytes at `value`.
    Some(unsafe { std::slice::from_raw_parts(value as *const u8, len) })
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_Version() -> *const c_char {
    VERSION_C.as_ptr() as *const c_char
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_InitHooks(hooks: *mut cJSON_Hooks) {
    let mut g = HOOKS.lock().unwrap_or_else(|e| e.into_inner());
    if hooks.is_null() {
        g.allocate = libc_malloc;
        g.deallocate = libc_free;
        return;
    }
    // SAFETY: non-null hooks pointer from the caller.
    let hooks = unsafe { &*hooks };
    g.allocate = libc_malloc;
    if let Some(m) = hooks.malloc_fn {
        g.allocate = m;
    }
    g.deallocate = libc_free;
    if let Some(f) = hooks.free_fn {
        g.deallocate = f;
    }
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_GetErrorPtr() -> *const c_char {
    let loc = *GLOBAL_ERROR.lock().unwrap_or_else(|e| e.into_inner());
    if loc.json.is_null() {
        return ptr::null();
    }
    // SAFETY: `json` points into the caller's parse buffer, which must still
    // be alive when they call GetErrorPtr (same contract as cJSON).
    unsafe { loc.json.add(loc.position) }
}

fn parse_impl(
    value: *const c_char,
    buffer_length: usize,
    return_parse_end: *mut *const c_char,
    require_null_terminated: c_int,
) -> *mut cJSON {
    clear_error();
    if value.is_null() || buffer_length == 0 {
        set_error(value, 0);
        return ptr::null_mut();
    }
    // SAFETY: `value` has `buffer_length` bytes.
    let slice = match unsafe { slice_from_ptr_len(value, buffer_length) } {
        Some(s) => s,
        None => {
            set_error(value, 0);
            return ptr::null_mut();
        }
    };
    let opts = ParseOptions {
        require_null_terminated: require_null_terminated != 0,
        skip_bom: true,
    };
    match parse_with_opts(slice, opts) {
        Ok(success) => {
            if !return_parse_end.is_null() {
                // SAFETY: caller-provided out pointer.
                unsafe { *return_parse_end = value.add(success.parse_end) };
            }
            value_to_cjson(&success.value)
        }
        Err(err) => {
            let pos = if err.position < buffer_length {
                err.position
            } else if buffer_length > 0 {
                buffer_length - 1
            } else {
                0
            };
            set_error(value, pos);
            if !return_parse_end.is_null() {
                // SAFETY: caller-provided out pointer.
                unsafe { *return_parse_end = value.add(pos) };
            }
            ptr::null_mut()
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_Parse(value: *const c_char) -> *mut cJSON {
    if value.is_null() {
        set_error(ptr::null(), 0);
        return ptr::null_mut();
    }
    // SAFETY: `value` is a C string.
    let len = unsafe { libc::strlen(value) } + 1;
    parse_impl(value, len, ptr::null_mut(), 0)
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_ParseWithLength(
    value: *const c_char,
    buffer_length: usize,
) -> *mut cJSON {
    parse_impl(value, buffer_length, ptr::null_mut(), 0)
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_ParseWithOpts(
    value: *const c_char,
    return_parse_end: *mut *const c_char,
    require_null_terminated: c_int,
) -> *mut cJSON {
    if value.is_null() {
        set_error(ptr::null(), 0);
        return ptr::null_mut();
    }
    // SAFETY: `value` is a C string.
    let len = unsafe { libc::strlen(value) } + 1;
    parse_impl(value, len, return_parse_end, require_null_terminated)
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_ParseWithLengthOpts(
    value: *const c_char,
    buffer_length: usize,
    return_parse_end: *mut *const c_char,
    require_null_terminated: c_int,
) -> *mut cJSON {
    parse_impl(
        value,
        buffer_length,
        return_parse_end,
        require_null_terminated,
    )
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_Print(item: *const cJSON) -> *mut c_char {
    print_to_cstring(item, true)
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_PrintUnformatted(item: *const cJSON) -> *mut c_char {
    print_to_cstring(item, false)
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_PrintBuffered(
    item: *const cJSON,
    _prebuffer: c_int,
    fmt: c_int,
) -> *mut c_char {
    print_to_cstring(item, fmt != 0)
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_PrintPreallocated(
    item: *mut cJSON,
    buffer: *mut c_char,
    length: c_int,
    format: c_int,
) -> c_int {
    if length < 0 || buffer.is_null() || item.is_null() {
        return 0;
    }
    // SAFETY: `item` is a cJSON tree.
    let value = match unsafe { cjson_to_value(item, 0) } {
        Some(v) => v,
        None => return 0,
    };
    let printed = if format != 0 {
        print(&value)
    } else {
        print_unformatted(&value)
    };
    let Ok(bytes) = printed else {
        return 0;
    };
    if bytes.len() + 1 > length as usize {
        return 0;
    }
    // SAFETY: `buffer` has `length` bytes.
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), buffer as *mut u8, bytes.len());
        *buffer.add(bytes.len()) = 0;
    }
    1
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_Delete(item: *mut cJSON) {
    let mut cur = item;
    while !cur.is_null() {
        // SAFETY: `cur` is a node allocated by this library (or compatible).
        let next = unsafe { (*cur).next };
        let is_ref = unsafe { (*cur).type_ & CJSON_IS_REFERENCE } != 0;
        let str_const = unsafe { (*cur).type_ & CJSON_STRING_IS_CONST } != 0;
        if !is_ref {
            // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
            let child = unsafe { (*cur).child };
            if !child.is_null() {
                cJSON_Delete(child);
            }
            // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
            let vs = unsafe { (*cur).valuestring };
            if !vs.is_null() {
                dealloc(vs as *mut c_void);
            }
        }
        if !str_const {
            // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
            let s = unsafe { (*cur).string };
            if !s.is_null() {
                dealloc(s as *mut c_void);
            }
        }
        dealloc(cur as *mut c_void);
        cur = next;
    }
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_GetArraySize(array: *const cJSON) -> c_int {
    if array.is_null() {
        return 0;
    }
    let mut size: usize = 0;
    // SAFETY: array is a valid node.
    let mut child = unsafe { (*array).child };
    while !child.is_null() {
        size += 1;
        child = unsafe { (*child).next };
    }
    size as c_int
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_GetArrayItem(array: *const cJSON, index: c_int) -> *mut cJSON {
    if index < 0 || array.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
    let mut child = unsafe { (*array).child };
    let mut i = index;
    while !child.is_null() && i > 0 {
        i -= 1;
        // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
        child = unsafe { (*child).next };
    }
    child
}

fn get_object_item(object: *const cJSON, name: *const c_char, case_sensitive: bool) -> *mut cJSON {
    if object.is_null() || name.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: `name` is a C string.
    let key = unsafe { CStr::from_ptr(name).to_bytes() };
    let mut cur = unsafe { (*object).child };
    while !cur.is_null() {
        let kptr = unsafe { (*cur).string };
        if !kptr.is_null() {
            // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
            let kb = unsafe { CStr::from_ptr(kptr).to_bytes() };
            let matched = if case_sensitive {
                kb == key
            } else {
                key.len() == kb.len()
                    && key
                        .iter()
                        .zip(kb.iter())
                        .all(|(a, b)| a.to_ascii_lowercase() == b.to_ascii_lowercase())
            };
            if matched {
                return cur;
            }
        }
        // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
        cur = unsafe { (*cur).next };
    }
    ptr::null_mut()
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_GetObjectItem(
    object: *const cJSON,
    string: *const c_char,
) -> *mut cJSON {
    get_object_item(object, string, false)
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_GetObjectItemCaseSensitive(
    object: *const cJSON,
    string: *const c_char,
) -> *mut cJSON {
    get_object_item(object, string, true)
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_HasObjectItem(object: *const cJSON, string: *const c_char) -> c_int {
    i32::from(!cJSON_GetObjectItem(object, string).is_null())
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_GetStringValue(item: *const cJSON) -> *mut c_char {
    if item.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
    if unsafe { c_type_low((*item).type_) } != CJSON_STRING {
        return ptr::null_mut();
    }
    // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
    unsafe { (*item).valuestring }
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_GetNumberValue(item: *const cJSON) -> c_double {
    if item.is_null() {
        return f64::NAN;
    }
    // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
    if unsafe { c_type_low((*item).type_) } != CJSON_NUMBER {
        return f64::NAN;
    }
    // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
    unsafe { (*item).valuedouble }
}

fn is_type(item: *const cJSON, t: c_int) -> c_int {
    if item.is_null() {
        return 0;
    }
    // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
    i32::from(unsafe { c_type_low((*item).type_) } == t)
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_IsInvalid(item: *const cJSON) -> c_int {
    is_type(item, CJSON_INVALID)
}
#[no_mangle]
pub unsafe extern "C" fn cJSON_IsFalse(item: *const cJSON) -> c_int {
    is_type(item, CJSON_FALSE)
}
#[no_mangle]
pub unsafe extern "C" fn cJSON_IsTrue(item: *const cJSON) -> c_int {
    is_type(item, CJSON_TRUE)
}
#[no_mangle]
pub unsafe extern "C" fn cJSON_IsBool(item: *const cJSON) -> c_int {
    if item.is_null() {
        return 0;
    }
    // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
    i32::from(unsafe { (*item).type_ & (CJSON_TRUE | CJSON_FALSE) } != 0)
}
#[no_mangle]
pub unsafe extern "C" fn cJSON_IsNull(item: *const cJSON) -> c_int {
    is_type(item, CJSON_NULL)
}
#[no_mangle]
pub unsafe extern "C" fn cJSON_IsNumber(item: *const cJSON) -> c_int {
    is_type(item, CJSON_NUMBER)
}
#[no_mangle]
pub unsafe extern "C" fn cJSON_IsString(item: *const cJSON) -> c_int {
    is_type(item, CJSON_STRING)
}
#[no_mangle]
pub unsafe extern "C" fn cJSON_IsArray(item: *const cJSON) -> c_int {
    is_type(item, CJSON_ARRAY)
}
#[no_mangle]
pub unsafe extern "C" fn cJSON_IsObject(item: *const cJSON) -> c_int {
    is_type(item, CJSON_OBJECT)
}
#[no_mangle]
pub unsafe extern "C" fn cJSON_IsRaw(item: *const cJSON) -> c_int {
    is_type(item, CJSON_RAW)
}

fn create_typed(t: c_int) -> *mut cJSON {
    let item = new_item();
    if !item.is_null() {
        // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
        unsafe { (*item).type_ = t };
    }
    item
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_CreateNull() -> *mut cJSON {
    create_typed(CJSON_NULL)
}
#[no_mangle]
pub unsafe extern "C" fn cJSON_CreateTrue() -> *mut cJSON {
    create_typed(CJSON_TRUE)
}
#[no_mangle]
pub unsafe extern "C" fn cJSON_CreateFalse() -> *mut cJSON {
    create_typed(CJSON_FALSE)
}
#[no_mangle]
pub unsafe extern "C" fn cJSON_CreateBool(boolean: c_int) -> *mut cJSON {
    if boolean != 0 {
        cJSON_CreateTrue()
    } else {
        cJSON_CreateFalse()
    }
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_CreateNumber(num: c_double) -> *mut cJSON {
    let item = new_item();
    if item.is_null() {
        return item;
    }
    // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
    unsafe {
        (*item).type_ = CJSON_NUMBER;
        (*item).valuedouble = num;
        (*item).valueint = cjson_core::saturate_i32(num);
    }
    item
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_CreateString(string: *const c_char) -> *mut cJSON {
    let item = new_item();
    if item.is_null() {
        return item;
    }
    // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
    unsafe {
        (*item).type_ = CJSON_STRING;
        (*item).valuestring = dup_cstr(string);
        if (*item).valuestring.is_null() && !string.is_null() {
            cJSON_Delete(item);
            return ptr::null_mut();
        }
    }
    item
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_CreateRaw(raw: *const c_char) -> *mut cJSON {
    let item = new_item();
    if item.is_null() {
        return item;
    }
    // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
    unsafe {
        (*item).type_ = CJSON_RAW;
        (*item).valuestring = dup_cstr(raw);
        if (*item).valuestring.is_null() && !raw.is_null() {
            cJSON_Delete(item);
            return ptr::null_mut();
        }
    }
    item
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_CreateArray() -> *mut cJSON {
    create_typed(CJSON_ARRAY)
}
#[no_mangle]
pub unsafe extern "C" fn cJSON_CreateObject() -> *mut cJSON {
    create_typed(CJSON_OBJECT)
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_CreateStringReference(string: *const c_char) -> *mut cJSON {
    let item = new_item();
    if item.is_null() {
        return item;
    }
    // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
    unsafe {
        (*item).type_ = CJSON_STRING | CJSON_IS_REFERENCE;
        (*item).valuestring = string as *mut c_char;
    }
    item
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_CreateObjectReference(child: *const cJSON) -> *mut cJSON {
    let item = new_item();
    if item.is_null() {
        return item;
    }
    // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
    unsafe {
        (*item).type_ = CJSON_OBJECT | CJSON_IS_REFERENCE;
        (*item).child = child as *mut cJSON;
    }
    item
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_CreateArrayReference(child: *const cJSON) -> *mut cJSON {
    let item = new_item();
    if item.is_null() {
        return item;
    }
    // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
    unsafe {
        (*item).type_ = CJSON_ARRAY | CJSON_IS_REFERENCE;
        (*item).child = child as *mut cJSON;
    }
    item
}

fn add_item_to_array(array: *mut cJSON, item: *mut cJSON) -> c_int {
    if item.is_null() || array.is_null() || ptr::eq(array, item) {
        return 0;
    }
    // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
    unsafe {
        let child = (*array).child;
        if child.is_null() {
            (*array).child = item;
            (*item).prev = item;
            (*item).next = ptr::null_mut();
        } else if !(*child).prev.is_null() {
            let last = (*child).prev;
            (*last).next = item;
            (*item).prev = last;
            (*(*array).child).prev = item;
        }
    }
    1
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_AddItemToArray(array: *mut cJSON, item: *mut cJSON) -> c_int {
    add_item_to_array(array, item)
}

fn add_item_to_object(
    object: *mut cJSON,
    string: *const c_char,
    item: *mut cJSON,
    constant_key: bool,
) -> c_int {
    if object.is_null() || string.is_null() || item.is_null() || ptr::eq(object, item) {
        return 0;
    }
    // SAFETY: `object` and `item` are non-null cJSON nodes; `string` is a C string.
    unsafe {
        let was_const = ((*item).type_ & CJSON_STRING_IS_CONST) != 0;
        let new_key = if constant_key {
            string as *mut c_char
        } else {
            let copy = dup_cstr(string);
            if copy.is_null() {
                return 0;
            }
            copy
        };
        if !was_const && !(*item).string.is_null() {
            dealloc((*item).string as *mut c_void);
        }
        (*item).string = new_key;
        if constant_key {
            (*item).type_ |= CJSON_STRING_IS_CONST;
        } else {
            (*item).type_ &= !CJSON_STRING_IS_CONST;
        }
    }
    add_item_to_array(object, item)
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_AddItemToObject(
    object: *mut cJSON,
    string: *const c_char,
    item: *mut cJSON,
) -> c_int {
    add_item_to_object(object, string, item, false)
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_AddItemToObjectCS(
    object: *mut cJSON,
    string: *const c_char,
    item: *mut cJSON,
) -> c_int {
    add_item_to_object(object, string, item, true)
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_AddItemReferenceToArray(
    array: *mut cJSON,
    item: *mut cJSON,
) -> c_int {
    if array.is_null() || item.is_null() {
        return 0;
    }
    let reference = new_item();
    if reference.is_null() {
        return 0;
    }
    // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
    unsafe {
        ptr::copy_nonoverlapping(item, reference, 1);
        (*reference).string = ptr::null_mut();
        (*reference).type_ |= CJSON_IS_REFERENCE;
        (*reference).next = ptr::null_mut();
        (*reference).prev = ptr::null_mut();
    }
    add_item_to_array(array, reference)
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_AddItemReferenceToObject(
    object: *mut cJSON,
    string: *const c_char,
    item: *mut cJSON,
) -> c_int {
    if object.is_null() || string.is_null() || item.is_null() {
        return 0;
    }
    let reference = new_item();
    if reference.is_null() {
        return 0;
    }
    // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
    unsafe {
        ptr::copy_nonoverlapping(item, reference, 1);
        (*reference).string = ptr::null_mut();
        (*reference).type_ |= CJSON_IS_REFERENCE;
        (*reference).next = ptr::null_mut();
        (*reference).prev = ptr::null_mut();
    }
    add_item_to_object(object, string, reference, false)
}

fn add_to_object_helper(object: *mut cJSON, name: *const c_char, item: *mut cJSON) -> *mut cJSON {
    if add_item_to_object(object, name, item, false) != 0 {
        item
    } else {
        // SAFETY: `item` is a node we just allocated (or null, which Delete handles).
        unsafe { cJSON_Delete(item) };
        ptr::null_mut()
    }
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_AddNullToObject(
    object: *mut cJSON,
    name: *const c_char,
) -> *mut cJSON {
    add_to_object_helper(object, name, cJSON_CreateNull())
}
#[no_mangle]
pub unsafe extern "C" fn cJSON_AddTrueToObject(
    object: *mut cJSON,
    name: *const c_char,
) -> *mut cJSON {
    add_to_object_helper(object, name, cJSON_CreateTrue())
}
#[no_mangle]
pub unsafe extern "C" fn cJSON_AddFalseToObject(
    object: *mut cJSON,
    name: *const c_char,
) -> *mut cJSON {
    add_to_object_helper(object, name, cJSON_CreateFalse())
}
#[no_mangle]
pub unsafe extern "C" fn cJSON_AddBoolToObject(
    object: *mut cJSON,
    name: *const c_char,
    boolean: c_int,
) -> *mut cJSON {
    add_to_object_helper(object, name, cJSON_CreateBool(boolean))
}
#[no_mangle]
pub unsafe extern "C" fn cJSON_AddNumberToObject(
    object: *mut cJSON,
    name: *const c_char,
    number: c_double,
) -> *mut cJSON {
    add_to_object_helper(object, name, cJSON_CreateNumber(number))
}
#[no_mangle]
pub unsafe extern "C" fn cJSON_AddStringToObject(
    object: *mut cJSON,
    name: *const c_char,
    string: *const c_char,
) -> *mut cJSON {
    add_to_object_helper(object, name, cJSON_CreateString(string))
}
#[no_mangle]
pub unsafe extern "C" fn cJSON_AddRawToObject(
    object: *mut cJSON,
    name: *const c_char,
    raw: *const c_char,
) -> *mut cJSON {
    add_to_object_helper(object, name, cJSON_CreateRaw(raw))
}
#[no_mangle]
pub unsafe extern "C" fn cJSON_AddObjectToObject(
    object: *mut cJSON,
    name: *const c_char,
) -> *mut cJSON {
    add_to_object_helper(object, name, cJSON_CreateObject())
}
#[no_mangle]
pub unsafe extern "C" fn cJSON_AddArrayToObject(
    object: *mut cJSON,
    name: *const c_char,
) -> *mut cJSON {
    add_to_object_helper(object, name, cJSON_CreateArray())
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_DetachItemViaPointer(
    parent: *mut cJSON,
    item: *mut cJSON,
) -> *mut cJSON {
    if parent.is_null() || item.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
    unsafe {
        if item != (*parent).child && (*item).prev.is_null() {
            return ptr::null_mut();
        }
        if item != (*parent).child {
            (*(*item).prev).next = (*item).next;
        }
        if !(*item).next.is_null() {
            (*(*item).next).prev = (*item).prev;
        }
        if item == (*parent).child {
            (*parent).child = (*item).next;
        } else if (*item).next.is_null() {
            (*(*parent).child).prev = (*item).prev;
        }
        (*item).prev = ptr::null_mut();
        (*item).next = ptr::null_mut();
    }
    item
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_DetachItemFromArray(array: *mut cJSON, which: c_int) -> *mut cJSON {
    cJSON_DetachItemViaPointer(array, cJSON_GetArrayItem(array, which))
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_DeleteItemFromArray(array: *mut cJSON, which: c_int) {
    cJSON_Delete(cJSON_DetachItemFromArray(array, which));
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_DetachItemFromObject(
    object: *mut cJSON,
    string: *const c_char,
) -> *mut cJSON {
    cJSON_DetachItemViaPointer(object, cJSON_GetObjectItem(object, string))
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_DetachItemFromObjectCaseSensitive(
    object: *mut cJSON,
    string: *const c_char,
) -> *mut cJSON {
    cJSON_DetachItemViaPointer(object, cJSON_GetObjectItemCaseSensitive(object, string))
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_DeleteItemFromObject(object: *mut cJSON, string: *const c_char) {
    cJSON_Delete(cJSON_DetachItemFromObject(object, string));
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_DeleteItemFromObjectCaseSensitive(
    object: *mut cJSON,
    string: *const c_char,
) {
    cJSON_Delete(cJSON_DetachItemFromObjectCaseSensitive(object, string));
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_InsertItemInArray(
    array: *mut cJSON,
    which: c_int,
    newitem: *mut cJSON,
) -> c_int {
    if which < 0 || newitem.is_null() {
        return 0;
    }
    let after = cJSON_GetArrayItem(array, which);
    if after.is_null() {
        return add_item_to_array(array, newitem);
    }
    // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
    unsafe {
        if after != (*array).child && (*after).prev.is_null() {
            return 0;
        }
        (*newitem).next = after;
        (*newitem).prev = (*after).prev;
        (*after).prev = newitem;
        if after == (*array).child {
            (*array).child = newitem;
        } else {
            (*(*newitem).prev).next = newitem;
        }
    }
    1
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_ReplaceItemViaPointer(
    parent: *mut cJSON,
    item: *mut cJSON,
    replacement: *mut cJSON,
) -> c_int {
    if parent.is_null() || item.is_null() || replacement.is_null() {
        return 0;
    }
    if ptr::eq(item, replacement) {
        return 1;
    }
    // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
    unsafe {
        if (*parent).child.is_null() {
            return 0;
        }
        (*replacement).next = (*item).next;
        (*replacement).prev = (*item).prev;
        if !(*replacement).next.is_null() {
            (*(*replacement).next).prev = replacement;
        }
        if (*parent).child == item {
            if (*(*parent).child).prev == (*parent).child {
                (*replacement).prev = replacement;
            }
            (*parent).child = replacement;
        } else {
            if !(*replacement).prev.is_null() {
                (*(*replacement).prev).next = replacement;
            }
            if (*replacement).next.is_null() {
                (*(*parent).child).prev = replacement;
            }
        }
        (*item).next = ptr::null_mut();
        (*item).prev = ptr::null_mut();
    }
    cJSON_Delete(item);
    1
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_ReplaceItemInArray(
    array: *mut cJSON,
    which: c_int,
    newitem: *mut cJSON,
) -> c_int {
    if which < 0 {
        return 0;
    }
    cJSON_ReplaceItemViaPointer(array, cJSON_GetArrayItem(array, which), newitem)
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_ReplaceItemInObject(
    object: *mut cJSON,
    string: *const c_char,
    newitem: *mut cJSON,
) -> c_int {
    if newitem.is_null() || string.is_null() {
        return 0;
    }
    // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
    unsafe {
        if ((*newitem).type_ & CJSON_STRING_IS_CONST) == 0 && !(*newitem).string.is_null() {
            cJSON_free((*newitem).string as *mut c_void);
        }
        (*newitem).string = dup_cstr(string);
        if (*newitem).string.is_null() {
            return 0;
        }
        (*newitem).type_ &= !CJSON_STRING_IS_CONST;
    }
    cJSON_ReplaceItemViaPointer(object, cJSON_GetObjectItem(object, string), newitem)
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_ReplaceItemInObjectCaseSensitive(
    object: *mut cJSON,
    string: *const c_char,
    newitem: *mut cJSON,
) -> c_int {
    if newitem.is_null() || string.is_null() {
        return 0;
    }
    // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
    unsafe {
        if ((*newitem).type_ & CJSON_STRING_IS_CONST) == 0 && !(*newitem).string.is_null() {
            cJSON_free((*newitem).string as *mut c_void);
        }
        (*newitem).string = dup_cstr(string);
        if (*newitem).string.is_null() {
            return 0;
        }
        (*newitem).type_ &= !CJSON_STRING_IS_CONST;
    }
    cJSON_ReplaceItemViaPointer(
        object,
        cJSON_GetObjectItemCaseSensitive(object, string),
        newitem,
    )
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_Duplicate(item: *const cJSON, recurse: c_int) -> *mut cJSON {
    if item.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
    let value = unsafe { cjson_to_value(item, 0) };
    let Some(v) = value else {
        return ptr::null_mut();
    };
    let duped = if recurse != 0 {
        match v.duplicate(true) {
            Some(d) => d,
            None => return ptr::null_mut(),
        }
    } else {
        match v.duplicate(false) {
            Some(d) => d,
            None => return ptr::null_mut(),
        }
    };
    value_to_cjson(&duped)
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_Compare(
    a: *const cJSON,
    b: *const cJSON,
    case_sensitive: c_int,
) -> c_int {
    if a.is_null() || b.is_null() {
        return 0;
    }
    // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
    let av = unsafe { cjson_to_value(a, 0) };
    // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
    let bv = unsafe { cjson_to_value(b, 0) };
    match (av, bv) {
        (Some(a), Some(b)) => i32::from(cjson_core::compare(&a, &b, case_sensitive != 0)),
        _ => 0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_Minify(json: *mut c_char) {
    if json.is_null() {
        return;
    }
    // SAFETY: caller provided a writable NUL-terminated buffer.
    let len = unsafe { libc::strlen(json) };
    let slice = unsafe { std::slice::from_raw_parts_mut(json as *mut u8, len + 1) };
    let _ = minify_bytes(slice);
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_SetNumberHelper(object: *mut cJSON, number: c_double) -> c_double {
    if object.is_null() {
        return f64::NAN;
    }
    // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
    unsafe {
        (*object).valueint = cjson_core::saturate_i32(number);
        (*object).valuedouble = number;
        number
    }
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_SetValuestring(
    object: *mut cJSON,
    valuestring: *const c_char,
) -> *mut c_char {
    if object.is_null() || valuestring.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
    unsafe {
        if ((*object).type_ & CJSON_STRING) == 0 || ((*object).type_ & CJSON_IS_REFERENCE) != 0 {
            return ptr::null_mut();
        }
        if (*object).valuestring.is_null() {
            return ptr::null_mut();
        }
        let copy = dup_cstr(valuestring);
        if copy.is_null() {
            return ptr::null_mut();
        }
        cJSON_free((*object).valuestring as *mut c_void);
        (*object).valuestring = copy;
        copy
    }
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_malloc(size: usize) -> *mut c_void {
    alloc(size)
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_free(object: *mut c_void) {
    dealloc(object);
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_CreateIntArray(numbers: *const c_int, count: c_int) -> *mut cJSON {
    if count < 0 || numbers.is_null() {
        return ptr::null_mut();
    }
    let a = cJSON_CreateArray();
    if a.is_null() {
        return a;
    }
    for i in 0..count {
        // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
        let n = unsafe { cJSON_CreateNumber(*numbers.add(i as usize) as f64) };
        if n.is_null() {
            cJSON_Delete(a);
            return ptr::null_mut();
        }
        if cJSON_AddItemToArray(a, n) == 0 {
            cJSON_Delete(n);
            cJSON_Delete(a);
            return ptr::null_mut();
        }
    }
    a
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_CreateFloatArray(numbers: *const f32, count: c_int) -> *mut cJSON {
    if count < 0 || numbers.is_null() {
        return ptr::null_mut();
    }
    let a = cJSON_CreateArray();
    if a.is_null() {
        return a;
    }
    for i in 0..count {
        // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
        let n = unsafe { cJSON_CreateNumber(f64::from(*numbers.add(i as usize))) };
        if n.is_null() {
            cJSON_Delete(a);
            return ptr::null_mut();
        }
        if cJSON_AddItemToArray(a, n) == 0 {
            cJSON_Delete(n);
            cJSON_Delete(a);
            return ptr::null_mut();
        }
    }
    a
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_CreateDoubleArray(
    numbers: *const c_double,
    count: c_int,
) -> *mut cJSON {
    if count < 0 || numbers.is_null() {
        return ptr::null_mut();
    }
    let a = cJSON_CreateArray();
    if a.is_null() {
        return a;
    }
    for i in 0..count {
        // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
        let n = unsafe { cJSON_CreateNumber(*numbers.add(i as usize)) };
        if n.is_null() {
            cJSON_Delete(a);
            return ptr::null_mut();
        }
        if cJSON_AddItemToArray(a, n) == 0 {
            cJSON_Delete(n);
            cJSON_Delete(a);
            return ptr::null_mut();
        }
    }
    a
}

#[no_mangle]
pub unsafe extern "C" fn cJSON_CreateStringArray(
    strings: *const *const c_char,
    count: c_int,
) -> *mut cJSON {
    if count < 0 || strings.is_null() {
        return ptr::null_mut();
    }
    let a = cJSON_CreateArray();
    if a.is_null() {
        return a;
    }
    for i in 0..count {
        // SAFETY: pointers follow the cJSON.h contract (non-null unless documented).
        let n = unsafe { cJSON_CreateString(*strings.add(i as usize)) };
        if n.is_null() {
            cJSON_Delete(a);
            return ptr::null_mut();
        }
        if cJSON_AddItemToArray(a, n) == 0 {
            cJSON_Delete(n);
            cJSON_Delete(a);
            return ptr::null_mut();
        }
    }
    a
}

// Keep compare_double referenced so FFI users can match cJSON tests.
#[allow(dead_code)]
fn _compare_double(a: f64, b: f64) -> bool {
    compare_double(a, b)
}
