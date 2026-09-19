//! JSON value tree. Objects preserve insertion order and allow duplicate keys.

use crate::number::{saturate_i32, Number};
use crate::string::{c_str_eq, case_insensitive_eq};
use crate::CIRCULAR_LIMIT;

/// cJSON type tag (`type & 0xFF`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Type {
    Invalid = 0,
    False = 1 << 0,
    True = 1 << 1,
    Null = 1 << 2,
    Number = 1 << 3,
    String = 1 << 4,
    Array = 1 << 5,
    Object = 1 << 6,
    Raw = 1 << 7,
}

impl Type {
    pub fn as_c_int(self) -> i32 {
        self as i32
    }
}

/// A JSON value. Array and object children live in `children`; object member
/// names are stored on the child in `name` (matching cJSON's `item->string`).
#[derive(Clone, Debug)]
pub struct Value {
    pub kind: Type,
    pub valuestring: Option<Vec<u8>>,
    pub valueint: i32,
    pub valuedouble: f64,
    pub children: Vec<Value>,
    pub name: Option<Vec<u8>>,
}

impl Default for Value {
    fn default() -> Self {
        Self::invalid()
    }
}

impl Value {
    pub fn invalid() -> Self {
        Self {
            kind: Type::Invalid,
            valuestring: None,
            valueint: 0,
            valuedouble: 0.0,
            children: Vec::new(),
            name: None,
        }
    }

    pub fn null() -> Self {
        Self {
            kind: Type::Null,
            ..Self::invalid()
        }
    }

    pub fn boolean(v: bool) -> Self {
        Self {
            kind: if v { Type::True } else { Type::False },
            valueint: i32::from(v),
            ..Self::invalid()
        }
    }

    pub fn from_number(n: Number) -> Self {
        Self {
            kind: Type::Number,
            valueint: n.valueint,
            valuedouble: n.valuedouble,
            ..Self::invalid()
        }
    }

    pub fn number(d: f64) -> Self {
        Self::from_number(Number::from_double(d))
    }

    pub fn string(s: &str) -> Self {
        Self::string_bytes(s.as_bytes().to_vec())
    }

    pub fn string_bytes(bytes: Vec<u8>) -> Self {
        Self {
            kind: Type::String,
            valuestring: Some(bytes),
            ..Self::invalid()
        }
    }

    pub fn raw(s: &str) -> Self {
        Self::raw_bytes(s.as_bytes().to_vec())
    }

    pub fn raw_bytes(bytes: Vec<u8>) -> Self {
        Self {
            kind: Type::Raw,
            valuestring: Some(bytes),
            ..Self::invalid()
        }
    }

    pub fn array() -> Self {
        Self {
            kind: Type::Array,
            ..Self::invalid()
        }
    }

    pub fn object() -> Self {
        Self {
            kind: Type::Object,
            ..Self::invalid()
        }
    }

    pub fn is_invalid(&self) -> bool {
        self.kind == Type::Invalid
    }
    pub fn is_false(&self) -> bool {
        self.kind == Type::False
    }
    pub fn is_true(&self) -> bool {
        self.kind == Type::True
    }
    pub fn is_bool(&self) -> bool {
        self.kind == Type::True || self.kind == Type::False
    }
    pub fn is_null(&self) -> bool {
        self.kind == Type::Null
    }
    pub fn is_number(&self) -> bool {
        self.kind == Type::Number
    }
    pub fn is_string(&self) -> bool {
        self.kind == Type::String
    }
    pub fn is_array(&self) -> bool {
        self.kind == Type::Array
    }
    pub fn is_object(&self) -> bool {
        self.kind == Type::Object
    }
    pub fn is_raw(&self) -> bool {
        self.kind == Type::Raw
    }

    pub fn string_value(&self) -> Option<&[u8]> {
        if self.is_string() {
            self.valuestring.as_deref()
        } else {
            None
        }
    }

    pub fn string_value_str(&self) -> Option<&str> {
        self.string_value()
            .and_then(|b| std::str::from_utf8(b).ok())
    }

    pub fn number_value(&self) -> f64 {
        if self.is_number() {
            self.valuedouble
        } else {
            f64::NAN
        }
    }

    pub fn set_number(&mut self, d: f64) -> f64 {
        self.valueint = saturate_i32(d);
        self.valuedouble = d;
        d
    }

    pub fn set_bool(&mut self, v: bool) -> Type {
        if self.kind == Type::True || self.kind == Type::False {
            self.kind = if v { Type::True } else { Type::False };
            self.kind
        } else {
            Type::Invalid
        }
    }

    pub fn set_valuestring(&mut self, s: &str) -> Option<&[u8]> {
        if self.kind != Type::String {
            return None;
        }
        self.valuestring = Some(s.as_bytes().to_vec());
        self.valuestring.as_deref()
    }

    pub fn array_size(&self) -> usize {
        self.children.len()
    }

    pub fn array_item(&self, index: i32) -> Option<&Value> {
        if index < 0 {
            return None;
        }
        self.children.get(index as usize)
    }

    pub fn array_item_mut(&mut self, index: i32) -> Option<&mut Value> {
        if index < 0 {
            return None;
        }
        self.children.get_mut(index as usize)
    }

    /// Case-insensitive lookup (`cJSON_GetObjectItem`). Walks `children` even
    /// if `self` is not an object, matching the C function.
    pub fn get_object_item(&self, key: &str) -> Option<&Value> {
        self.get_object_item_bytes(key.as_bytes(), false)
    }

    /// Case-sensitive lookup (`cJSON_GetObjectItemCaseSensitive`).
    pub fn get_object_item_case_sensitive(&self, key: &str) -> Option<&Value> {
        self.get_object_item_bytes(key.as_bytes(), true)
    }

    pub fn get_object_item_bytes(&self, key: &[u8], case_sensitive: bool) -> Option<&Value> {
        self.children.iter().find(|child| {
            if case_sensitive {
                c_str_eq(Some(key), child.name.as_deref())
            } else {
                case_insensitive_eq(Some(key), child.name.as_deref())
            }
        })
    }

    pub fn has_object_item(&self, key: &str) -> bool {
        self.get_object_item(key).is_some()
    }

    pub fn add_item_to_array(&mut self, item: Value) -> bool {
        self.children.push(item);
        true
    }

    pub fn add_item_to_object(&mut self, key: &str, mut item: Value) -> bool {
        item.name = Some(key.as_bytes().to_vec());
        self.children.push(item);
        true
    }

    pub fn add_null_to_object(&mut self, key: &str) -> &Value {
        self.add_item_to_object(key, Value::null());
        self.children.last().expect("just pushed")
    }

    pub fn add_true_to_object(&mut self, key: &str) -> &Value {
        self.add_item_to_object(key, Value::boolean(true));
        self.children.last().expect("just pushed")
    }

    pub fn add_false_to_object(&mut self, key: &str) -> &Value {
        self.add_item_to_object(key, Value::boolean(false));
        self.children.last().expect("just pushed")
    }

    pub fn add_bool_to_object(&mut self, key: &str, v: bool) -> &Value {
        self.add_item_to_object(key, Value::boolean(v));
        self.children.last().expect("just pushed")
    }

    pub fn add_number_to_object(&mut self, key: &str, n: f64) -> &Value {
        self.add_item_to_object(key, Value::number(n));
        self.children.last().expect("just pushed")
    }

    pub fn add_string_to_object(&mut self, key: &str, s: &str) -> &Value {
        self.add_item_to_object(key, Value::string(s));
        self.children.last().expect("just pushed")
    }

    pub fn add_raw_to_object(&mut self, key: &str, s: &str) -> &Value {
        self.add_item_to_object(key, Value::raw(s));
        self.children.last().expect("just pushed")
    }

    pub fn add_object_to_object(&mut self, key: &str) -> &mut Value {
        self.add_item_to_object(key, Value::object());
        let i = self.children.len() - 1;
        &mut self.children[i]
    }

    pub fn add_array_to_object(&mut self, key: &str) -> &mut Value {
        self.add_item_to_object(key, Value::array());
        let i = self.children.len() - 1;
        &mut self.children[i]
    }

    pub fn detach_item_from_array(&mut self, index: i32) -> Option<Value> {
        if index < 0 {
            return None;
        }
        let i = index as usize;
        if i >= self.children.len() {
            return None;
        }
        Some(self.children.remove(i))
    }

    pub fn delete_item_from_array(&mut self, index: i32) {
        let _ = self.detach_item_from_array(index);
    }

    pub fn detach_item_from_object(&mut self, key: &str) -> Option<Value> {
        self.detach_item_from_object_bytes(key.as_bytes(), false)
    }

    pub fn detach_item_from_object_case_sensitive(&mut self, key: &str) -> Option<Value> {
        self.detach_item_from_object_bytes(key.as_bytes(), true)
    }

    fn detach_item_from_object_bytes(&mut self, key: &[u8], case_sensitive: bool) -> Option<Value> {
        let idx = self.children.iter().position(|child| {
            if case_sensitive {
                c_str_eq(Some(key), child.name.as_deref())
            } else {
                case_insensitive_eq(Some(key), child.name.as_deref())
            }
        })?;
        Some(self.children.remove(idx))
    }

    pub fn delete_item_from_object(&mut self, key: &str) {
        let _ = self.detach_item_from_object(key);
    }

    pub fn delete_item_from_object_case_sensitive(&mut self, key: &str) {
        let _ = self.detach_item_from_object_case_sensitive(key);
    }

    pub fn insert_item_in_array(&mut self, index: i32, item: Value) -> bool {
        if index < 0 {
            return false;
        }
        let i = index as usize;
        if i >= self.children.len() {
            self.children.push(item);
        } else {
            self.children.insert(i, item);
        }
        true
    }

    pub fn replace_item_in_array(&mut self, index: i32, item: Value) -> bool {
        if index < 0 {
            return false;
        }
        let i = index as usize;
        if i >= self.children.len() {
            return false;
        }
        self.children[i] = item;
        true
    }

    pub fn replace_item_in_object(&mut self, key: &str, mut item: Value) -> bool {
        item.name = Some(key.as_bytes().to_vec());
        let idx = self
            .children
            .iter()
            .position(|child| case_insensitive_eq(Some(key.as_bytes()), child.name.as_deref()));
        if let Some(i) = idx {
            self.children[i] = item;
            true
        } else {
            false
        }
    }

    pub fn replace_item_in_object_case_sensitive(&mut self, key: &str, mut item: Value) -> bool {
        item.name = Some(key.as_bytes().to_vec());
        let idx = self
            .children
            .iter()
            .position(|child| c_str_eq(Some(key.as_bytes()), child.name.as_deref()));
        if let Some(i) = idx {
            self.children[i] = item;
            true
        } else {
            false
        }
    }

    /// Deep copy. `depth` tracks child recursion so circular C trees converted
    /// into this type cannot explode (`CJSON_CIRCULAR_LIMIT`).
    pub fn duplicate(&self, recurse: bool) -> Option<Self> {
        self.duplicate_rec(0, recurse)
    }

    fn duplicate_rec(&self, depth: usize, recurse: bool) -> Option<Self> {
        let mut new = Value {
            kind: self.kind,
            valuestring: self.valuestring.clone(),
            valueint: self.valueint,
            valuedouble: self.valuedouble,
            children: Vec::new(),
            name: self.name.clone(),
        };
        if !recurse {
            return Some(new);
        }
        for child in &self.children {
            if depth >= CIRCULAR_LIMIT {
                return None;
            }
            new.children.push(child.duplicate_rec(depth + 1, true)?);
        }
        Some(new)
    }

    pub fn create_int_array(numbers: &[i32]) -> Self {
        let mut a = Value::array();
        for n in numbers {
            a.children.push(Value::number(f64::from(*n)));
        }
        a
    }

    pub fn create_float_array(numbers: &[f32]) -> Self {
        let mut a = Value::array();
        for n in numbers {
            a.children.push(Value::number(f64::from(*n)));
        }
        a
    }

    pub fn create_double_array(numbers: &[f64]) -> Self {
        let mut a = Value::array();
        for n in numbers {
            a.children.push(Value::number(*n));
        }
        a
    }

    pub fn create_string_array(strings: &[&str]) -> Self {
        let mut a = Value::array();
        for s in strings {
            a.children.push(Value::string(s));
        }
        a
    }
}
