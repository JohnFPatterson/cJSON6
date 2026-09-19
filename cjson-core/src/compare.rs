//! Recursive comparison matching `cJSON_Compare`.

use crate::number::compare_double;
use crate::string::c_str_eq;
use crate::value::{Type, Value};

/// Recursively compare two cJSON values. `NULL`/invalid items are unequal.
/// Object keys are matched case-sensitively when `case_sensitive` is true.
pub fn compare(a: &Value, b: &Value, case_sensitive: bool) -> bool {
    if a.kind != b.kind {
        return false;
    }
    match a.kind {
        Type::False
        | Type::True
        | Type::Null
        | Type::Number
        | Type::String
        | Type::Raw
        | Type::Array
        | Type::Object => {}
        Type::Invalid => return false,
    }

    match a.kind {
        Type::False | Type::True | Type::Null => true,
        Type::Number => compare_double(a.valuedouble, b.valuedouble),
        Type::String | Type::Raw => c_str_eq(a.valuestring.as_deref(), b.valuestring.as_deref()),
        Type::Array => {
            if a.children.len() != b.children.len() {
                return false;
            }
            a.children
                .iter()
                .zip(b.children.iter())
                .all(|(x, y)| compare(x, y, case_sensitive))
        }
        Type::Object => {
            for child in &a.children {
                let Some(other) =
                    b.get_object_item_bytes(child.name.as_deref().unwrap_or(b""), case_sensitive)
                else {
                    return false;
                };
                if !compare(child, other, case_sensitive) {
                    return false;
                }
            }
            for child in &b.children {
                let Some(other) =
                    a.get_object_item_bytes(child.name.as_deref().unwrap_or(b""), case_sensitive)
                else {
                    return false;
                };
                if !compare(child, other, case_sensitive) {
                    return false;
                }
            }
            true
        }
        Type::Invalid => false,
    }
}
