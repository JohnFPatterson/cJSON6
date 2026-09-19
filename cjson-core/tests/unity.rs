//! Port of the cJSON Unity tests (`tests/*.c`) onto `cjson-core`.

use cjson_core::{
    compare, compare_double, minify, minify_in_place, parse, parse_array_at, parse_hex4,
    parse_number, parse_object_at, parse_string, parse_string_at, parse_value_at,
    parse_with_length, parse_with_opts, print, print_double, print_number, print_string,
    print_unformatted_to_string, Number, ParseOptions, Type, Value, NESTING_LIMIT,
};
use std::fs;
use std::path::PathBuf;

fn nul_terminated(s: &str) -> Vec<u8> {
    let mut v = s.as_bytes().to_vec();
    v.push(0);
    v
}

fn inputs_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests/inputs")
}

fn read_input(name: &str) -> String {
    fs::read_to_string(inputs_dir().join(name)).expect("read input")
}

fn utf8(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).expect("utf8")
}

fn print_pretty_str(v: &Value) -> String {
    String::from_utf8(print(v).expect("print")).expect("utf8 print")
}

fn print_compact_str(v: &Value) -> String {
    print_unformatted_to_string(v).expect("print")
}

// --- parse_number.c ---

#[test]
fn parse_number_should_parse_zero() {
    let (n, _) = parse_number(&nul_terminated("0")).unwrap();
    assert_eq!(n.valueint, 0);
    assert_eq!(n.valuedouble, 0.0);
    let (n, _) = parse_number(&nul_terminated("0.0")).unwrap();
    assert_eq!(n.valueint, 0);
    assert_eq!(n.valuedouble, 0.0);
    let (n, _) = parse_number(&nul_terminated("-0")).unwrap();
    assert_eq!(n.valueint, 0);
    assert_eq!(n.valuedouble, -0.0);
}

#[test]
fn parse_number_should_parse_negative_integers() {
    let (n, _) = parse_number(&nul_terminated("-1")).unwrap();
    assert_eq!(n.valueint, -1);
    assert_eq!(n.valuedouble, -1.0);
    let (n, _) = parse_number(&nul_terminated("-32768")).unwrap();
    assert_eq!(n.valueint, -32768);
    assert_eq!(n.valuedouble, -32768.0);
    let (n, _) = parse_number(&nul_terminated("-2147483648")).unwrap();
    assert_eq!(n.valueint, (-2147483648.0_f64) as i32);
    assert_eq!(n.valuedouble, -2147483648.0);
}

#[test]
fn parse_number_should_parse_positive_integers() {
    let (n, _) = parse_number(&nul_terminated("1")).unwrap();
    assert_eq!(n.valueint, 1);
    let (n, _) = parse_number(&nul_terminated("32767")).unwrap();
    assert_eq!(n.valueint, 32767);
    let (n, _) = parse_number(&nul_terminated("2147483647")).unwrap();
    assert_eq!(n.valueint, 2147483647);
}

#[test]
fn parse_number_should_parse_positive_reals() {
    let (n, _) = parse_number(&nul_terminated("0.001")).unwrap();
    assert_eq!(n.valueint, 0);
    assert_eq!(n.valuedouble, 0.001);
    let (n, _) = parse_number(&nul_terminated("10e-10")).unwrap();
    assert_eq!(n.valueint, 0);
    assert_eq!(n.valuedouble, 10e-10);
    let (n, _) = parse_number(&nul_terminated("10E-10")).unwrap();
    assert_eq!(n.valuedouble, 10e-10);
    let (n, _) = parse_number(&nul_terminated("10e10")).unwrap();
    assert_eq!(n.valueint, i32::MAX);
    assert_eq!(n.valuedouble, 10e10);
    let (n, _) = parse_number(&nul_terminated("123e+127")).unwrap();
    assert_eq!(n.valueint, i32::MAX);
    assert_eq!(n.valuedouble, 123e127);
    let (n, _) = parse_number(&nul_terminated("123e-128")).unwrap();
    assert_eq!(n.valueint, 0);
    assert_eq!(n.valuedouble, 123e-128);
}

#[test]
fn parse_number_should_parse_negative_reals() {
    let (n, _) = parse_number(&nul_terminated("-0.001")).unwrap();
    assert_eq!(n.valuedouble, -0.001);
    let (n, _) = parse_number(&nul_terminated("-10e20")).unwrap();
    assert_eq!(n.valueint, i32::MIN);
    assert_eq!(n.valuedouble, -10e20);
}

#[test]
fn parse_number_should_parse_big_numbers() {
    assert!(parse_number(&nul_terminated(
        "9999999999999999999999999999999999999999999999912345678901234567"
    ))
    .is_ok());
}

// --- print_number.c ---

fn assert_print_number(expected: &str, input: f64) {
    assert_eq!(print_double(input), expected);
}

#[test]
fn print_number_should_print_zero() {
    assert_print_number("0", 0.0);
}

#[test]
fn print_number_should_print_negative_integers() {
    assert_print_number("-1", -1.0);
    assert_print_number("-32768", -32768.0);
    assert_print_number("-2147483648", -2147483648.0);
}

#[test]
fn print_number_should_print_positive_integers() {
    assert_print_number("1", 1.0);
    assert_print_number("32767", 32767.0);
    assert_print_number("2147483647", 2147483647.0);
}

#[test]
fn print_number_should_print_positive_reals() {
    assert_print_number("0.123", 0.123);
    assert_print_number("1e-09", 10e-10);
    assert_print_number("1000000000000", 10e11);
    assert_print_number("1.23e+129", 123e+127);
    assert_print_number("1.23e-126", 123e-128);
    #[allow(clippy::excessive_precision, clippy::approx_constant)]
    {
        assert_print_number("3.1415926535897931", 3.1415926535897931);
    }
}

#[test]
fn print_number_should_print_negative_reals() {
    assert_print_number("-0.0123", -0.0123);
    assert_print_number("-1e-09", -10e-10);
    assert_print_number("-1e+21", -10e20);
    assert_print_number("-1.23e+129", -123e+127);
    assert_print_number("-1.23e-126", -123e-128);
}

#[test]
fn print_number_should_print_non_number() {
    assert_eq!(print_double(f64::NAN), "null");
    assert_eq!(print_double(f64::INFINITY), "null");
    assert_eq!(print_double(f64::NEG_INFINITY), "null");
}

// --- parse_string.c ---

fn assert_parse_string(json: &str, expected: &str) {
    let (got, _) = parse_string(&nul_terminated(json)).expect("parse string");
    assert_eq!(utf8(&got), expected);
}

#[test]
fn parse_string_should_parse_strings() {
    assert_parse_string("\"\"", "");
    assert_parse_string(
        "\" !\\\"#$%&'()*+,-./\\/0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\\\]^_'abcdefghijklmnopqrstuvwxyz{|}~\"",
        " !\"#$%&'()*+,-.//0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_'abcdefghijklmnopqrstuvwxyz{|}~",
    );
    assert_parse_string(
        "\"\\\"\\\\\\/\\b\\f\\n\\r\\t\\u20AC\\u732b\"",
        "\"\\/\u{8}\u{c}\n\r\t€猫",
    );
    assert_parse_string("\"\u{8}\u{c}\n\r\t\"", "\u{8}\u{c}\n\r\t");
}

#[test]
fn parse_string_should_parse_utf16_surrogate_pairs() {
    assert_parse_string("\"\\uD83D\\udc31\"", "🐱");
}

#[test]
fn parse_string_should_not_parse_non_strings() {
    assert!(parse_string(&nul_terminated("this\" is not a string\"")).is_err());
    assert!(parse_string(&nul_terminated("")).is_err());
}

#[test]
fn parse_string_should_not_parse_invalid_backslash() {
    assert!(parse_string(&nul_terminated("Abcdef\\123")).is_err());
    assert!(parse_string(&nul_terminated("Abcdef\\e23")).is_err());
}

#[test]
fn parse_string_should_not_overflow_with_closing_backslash() {
    assert!(parse_string(&nul_terminated("\"000000000000000000\\")).is_err());
}

#[test]
fn parse_string_should_parse_bug_94() {
    let string = "\"~!@\\\\#$%^&*()\\\\\\\\-\\\\+{}[]:\\\\;\\\\\\\"\\\\<\\\\>?/.,DC=ad,DC=com\"";
    assert_parse_string(
        string,
        "~!@\\#$%^&*()\\\\-\\+{}[]:\\;\\\"\\<\\>?/.,DC=ad,DC=com",
    );
}

// --- print_string.c ---

#[test]
fn print_string_should_print_empty_strings() {
    assert_eq!(print_string(Some(b"")), b"\"\"");
    assert_eq!(print_string(None), b"\"\"");
}

#[test]
fn print_string_should_print_ascii() {
    let mut ascii = Vec::new();
    for i in 1..0x7F {
        ascii.push(i as u8);
    }
    let printed = print_string(Some(&ascii));
    let expected = "\"\\u0001\\u0002\\u0003\\u0004\\u0005\\u0006\\u0007\\b\\t\\n\\u000b\\f\\r\\u000e\\u000f\\u0010\\u0011\\u0012\\u0013\\u0014\\u0015\\u0016\\u0017\\u0018\\u0019\\u001a\\u001b\\u001c\\u001d\\u001e\\u001f !\\\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\\\]^_`abcdefghijklmnopqrstuvwxyz{|}~\"";
    assert_eq!(utf8(&printed), expected);
}

#[test]
fn print_string_should_print_utf8() {
    assert_eq!(
        print_string(Some("ü猫慕".as_bytes())),
        "\"ü猫慕\"".as_bytes()
    );
}

// --- parse_hex4.c ---

#[test]
fn parse_hex4_should_parse_all_combinations() {
    for number in 0u32..=0xFFFF {
        let lower = format!("{number:04x}");
        let upper = format!("{number:04X}");
        assert_eq!(parse_hex4(lower.as_bytes()), number);
        assert_eq!(parse_hex4(upper.as_bytes()), number);
    }
}

#[test]
fn parse_hex4_should_parse_mixed_case() {
    for s in [
        "beef", "beeF", "beEf", "beEF", "bEef", "bEeF", "bEEf", "bEEF", "Beef", "BeeF", "BeEf",
        "BeEF", "BEef", "BEeF", "BEEf", "BEEF",
    ] {
        assert_eq!(parse_hex4(s.as_bytes()), 0xBEEF);
    }
}

// --- parse_value.c ---

fn assert_parse_value(s: &str, ty: Type) {
    let (v, _) = parse_value_at(&nul_terminated(s)).unwrap();
    assert_eq!(v.kind, ty);
}

#[test]
fn parse_value_should_parse_literals() {
    assert_parse_value("null", Type::Null);
    assert_parse_value("true", Type::True);
    assert_parse_value("false", Type::False);
    assert_parse_value("1.5", Type::Number);
    assert_parse_value("\"\"", Type::String);
    assert_parse_value("\"hello\"", Type::String);
    assert_parse_value("[]", Type::Array);
    assert_parse_value("{}", Type::Object);
}

// --- parse_array.c ---

#[test]
fn parse_array_should_parse_empty_arrays() {
    let (v, _) = parse_array_at(&nul_terminated("[]")).unwrap();
    assert!(v.is_array());
    assert!(v.children.is_empty());
    let (v, _) = parse_array_at(&nul_terminated("[\n\t]")).unwrap();
    assert!(v.children.is_empty());
}

#[test]
fn parse_array_should_parse_arrays_with_one_element() {
    let (v, _) = parse_array_at(&nul_terminated("[1]")).unwrap();
    assert_eq!(v.children[0].kind, Type::Number);
    let (v, _) = parse_array_at(&nul_terminated("[\"hello!\"]")).unwrap();
    assert_eq!(v.string_value_str_of(0), Some("hello!"));
    let (v, _) = parse_array_at(&nul_terminated("[[]]")).unwrap();
    assert_eq!(v.children[0].kind, Type::Array);
    assert!(v.children[0].children.is_empty());
    let (v, _) = parse_array_at(&nul_terminated("[null]")).unwrap();
    assert_eq!(v.children[0].kind, Type::Null);
}

trait Helper {
    fn string_value_str_of(&self, i: usize) -> Option<&str>;
}
impl Helper for Value {
    fn string_value_str_of(&self, i: usize) -> Option<&str> {
        self.children.get(i).and_then(|c| c.string_value_str())
    }
}

#[test]
fn parse_array_should_parse_arrays_with_multiple_elements() {
    let (v, _) = parse_array_at(&nul_terminated("[1\t,\n2, 3]")).unwrap();
    assert_eq!(v.children.len(), 3);
    let (v, _) =
        parse_array_at(&nul_terminated("[1, null, true, false, [], \"hello\", {}]")).unwrap();
    let expected = [
        Type::Number,
        Type::Null,
        Type::True,
        Type::False,
        Type::Array,
        Type::String,
        Type::Object,
    ];
    assert_eq!(v.children.len(), 7);
    for (i, t) in expected.iter().enumerate() {
        assert_eq!(v.children[i].kind, *t);
    }
}

#[test]
fn parse_array_should_not_parse_non_arrays() {
    for s in [
        "",
        "[",
        "]",
        "{\"hello\":[]}",
        "42",
        "3.14",
        "\"[]hello world!\n\"",
    ] {
        assert!(parse_array_at(&nul_terminated(s)).is_err(), "{s}");
    }
}

// --- parse_object.c ---

#[test]
fn parse_object_should_parse_empty_objects() {
    let (v, _) = parse_object_at(&nul_terminated("{}")).unwrap();
    assert!(v.is_object() && v.children.is_empty());
    let (v, _) = parse_object_at(&nul_terminated("{\n\t}")).unwrap();
    assert!(v.children.is_empty());
}

#[test]
fn parse_object_should_parse_objects_with_one_element() {
    let (v, _) = parse_object_at(&nul_terminated("{\"one\":1}")).unwrap();
    assert_eq!(v.children[0].name.as_deref(), Some(b"one".as_slice()));
    assert_eq!(v.children[0].kind, Type::Number);
}

#[test]
fn parse_object_should_parse_objects_with_multiple_elements() {
    let (v, _) = parse_object_at(&nul_terminated(
        "{\"one\":1, \"NULL\":null, \"TRUE\":true, \"FALSE\":false, \"array\":[], \"world\":\"hello\", \"object\":{}}",
    ))
    .unwrap();
    let names = ["one", "NULL", "TRUE", "FALSE", "array", "world", "object"];
    let types = [
        Type::Number,
        Type::Null,
        Type::True,
        Type::False,
        Type::Array,
        Type::String,
        Type::Object,
    ];
    assert_eq!(v.children.len(), 7);
    for i in 0..7 {
        assert_eq!(v.children[i].name.as_deref().map(utf8), Some(names[i]));
        assert_eq!(v.children[i].kind, types[i]);
    }
}

#[test]
fn parse_object_should_not_parse_non_objects() {
    for s in [
        "",
        "{",
        "}",
        "[\"hello\",{}]",
        "42",
        "3.14",
        "\"{}hello world!\n\"",
    ] {
        assert!(parse_object_at(&nul_terminated(s)).is_err(), "{s}");
    }
}

// --- print_value / array / object ---

#[test]
fn print_value_should_roundtrip_literals() {
    for s in [
        "null",
        "true",
        "false",
        "1.5",
        "\"\"",
        "\"hello\"",
        "[]",
        "{}",
    ] {
        let v = parse(s).unwrap();
        assert_eq!(print_compact_str(&v), s);
    }
}

#[test]
fn print_array_should_print_empty_and_elements() {
    let v = parse("[]").unwrap();
    assert_eq!(print_pretty_str(&v), "[]");
    let v = parse("[1,2,3]").unwrap();
    assert_eq!(print_compact_str(&v), "[1,2,3]");
    assert_eq!(print_pretty_str(&v), "[1, 2, 3]");
    let v = parse("[1,null,true,false,[],\"hello\",{}]").unwrap();
    assert_eq!(
        print_pretty_str(&v),
        "[1, null, true, false, [], \"hello\", {\n\t}]"
    );
}

#[test]
fn print_object_should_print_formatted() {
    let v = parse("{}").unwrap();
    assert_eq!(print_pretty_str(&v), "{\n}");
    let v = parse("{\"one\":1}").unwrap();
    assert_eq!(print_pretty_str(&v), "{\n\t\"one\":\t1\n}");
    let v = parse("{\"one\":1,\"two\":2,\"three\":3}").unwrap();
    assert_eq!(
        print_pretty_str(&v),
        "{\n\t\"one\":\t1,\n\t\"two\":\t2,\n\t\"three\":\t3\n}"
    );
}

// --- parse_examples.c ---

fn do_file_test(name: &str) {
    let input = read_input(name);
    let expected = read_input(&format!("{name}.expected"));
    let tree = parse(&input).expect("parse");
    let actual = print_pretty_str(&tree);
    assert_eq!(actual, expected, "mismatch for {name}");
}

#[test]
fn file_tests_should_be_parsed_and_printed() {
    for n in [
        "test1", "test2", "test3", "test4", "test5", "test7", "test8", "test9", "test10", "test11",
    ] {
        do_file_test(n);
    }
}

#[test]
fn file_test6_should_not_be_parsed() {
    let test6 = read_input("test6");
    assert!(parse(&test6).is_err());
}

#[test]
fn test12_should_not_be_parsed() {
    let test12 = "{ \"name\": ";
    assert!(parse(test12).is_err());
}

#[test]
fn test13_should_be_parsed_without_null_termination() {
    let test_13 = concat!(
        "{",
        "\"Image\":{",
        "\"Width\":800,",
        "\"Height\":600,",
        "\"Title\":\"Viewfrom15thFloor\",",
        "\"Thumbnail\":{",
        "\"Url\":\"http:/*www.example.com/image/481989943\",",
        "\"Height\":125,",
        "\"Width\":\"100\"",
        "},",
        "\"IDs\":[116,943,234,38793]",
        "}",
        "}"
    );
    let bytes = test_13.as_bytes();
    assert!(parse_with_length(bytes).is_ok());
}

#[test]
fn test14_should_not_be_parsed() {
    let test_14 = concat!(
        "{",
        "\"Image\":{",
        "\"Width\":800,",
        "\"Height\":600,",
        "\"Title\":\"Viewfrom15thFloor\",",
        "\"Thumbnail\":{",
        "\"Url\":\"http:/*www.example.com/image/481989943\",",
        "\"Height\":125,",
        "\"Width\":\"100\"",
        "},",
        "\"IDs\":[116,943,234,38793]",
        "}",
        "}"
    );
    assert!(parse_with_length(&test_14.as_bytes()[..test_14.len() - 1]).is_err());
}

#[test]
fn test15_should_not_heap_buffer_overflow() {
    for s in ["{\"1\":1,", "{\"1\":1, "] {
        let _ = parse_with_length(s.as_bytes());
    }
}

// --- parse_with_opts.c ---

#[test]
fn parse_with_opts_should_handle_null_and_empty() {
    assert!(parse_with_opts(&[], ParseOptions::default()).is_err());
    let empty = [0u8];
    assert!(parse_with_opts(&empty, ParseOptions::default()).is_err());
}

#[test]
fn parse_with_opts_should_handle_incomplete_json() {
    let json = nul_terminated("{ \"name\": ");
    assert!(parse_with_opts(&json, ParseOptions::default()).is_err());
}

#[test]
fn parse_with_opts_should_require_null_if_requested() {
    let mut opts = ParseOptions {
        require_null_terminated: true,
        skip_bom: true,
    };
    assert!(parse_with_opts(&nul_terminated("{}"), opts).is_ok());
    assert!(parse_with_opts(&nul_terminated("{} \n"), opts).is_ok());
    assert!(parse_with_opts(&nul_terminated("{}x"), opts).is_err());
    opts.require_null_terminated = false;
    let r = parse_with_opts(&nul_terminated("[] empty array XD"), opts).unwrap();
    assert_eq!(r.parse_end, 2);
}

#[test]
fn parse_with_opts_should_parse_utf8_bom() {
    let with = parse("\u{feff}{}").unwrap();
    let without = parse("{}").unwrap();
    assert!(compare(&with, &without, true));
}

// --- minify ---

#[test]
fn cjson_minify_should_not_overflow_buffer() {
    let mut s = String::from("/* bla");
    minify_in_place(&mut s);
    assert_eq!(s, "");
    let mut s = String::from("\"\\");
    minify_in_place(&mut s);
    assert_eq!(s, "\"\\");
}

#[test]
fn cjson_minify_should_remove_comments_and_spaces() {
    assert_eq!(
        minify("{// this is {} \"some kind\" of [] comment /*, don't you see\n}"),
        "{}"
    );
    assert_eq!(minify("{ \"key\":\ttrue\r\n    }"), "{\"key\":true}");
    assert_eq!(
        minify("{/* this is\n a /* multi\n //line \n {comment \"\\\" */}"),
        "{}"
    );
    let s = "\"this is a string \\\" \\t bla\"";
    assert_eq!(minify(s), s);
}

#[test]
fn cjson_minify_should_minify_json() {
    let to_minify = "{\n    \"glossary\": { // comment\n        \"title\": \"example glossary\",\n  /* multi\n line */\n\t\t\"GlossDiv\": {\n            \"title\": \"S\",\n\t\t\t\"GlossList\": {\n                \"GlossEntry\": {\n                    \"ID\": \"SGML\",\n\t\t\t\t\t\"SortAs\": \"SGML\",\n\t\t\t\t\t\"Acronym\": \"SGML\",\n\t\t\t\t\t\"Abbrev\": \"ISO 8879:1986\",\n\t\t\t\t\t\"GlossDef\": {\n\t\t\t\t\t\t\"GlossSeeAlso\": [\"GML\", \"XML\"]\n                    },\n\t\t\t\t\t\"GlossSee\": \"markup\"\n                }\n            }\n        }\n    }\n}";
    let minified = "{\"glossary\":{\"title\":\"example glossary\",\"GlossDiv\":{\"title\":\"S\",\"GlossList\":{\"GlossEntry\":{\"ID\":\"SGML\",\"SortAs\":\"SGML\",\"Acronym\":\"SGML\",\"Abbrev\":\"ISO 8879:1986\",\"GlossDef\":{\"GlossSeeAlso\":[\"GML\",\"XML\"]},\"GlossSee\":\"markup\"}}}}}";
    assert_eq!(minify(to_minify), minified);
}

#[test]
fn cjson_minify_should_not_loop_infinitely() {
    let _ = minify("8 / 5\n");
}

// --- compare ---

fn compare_from_string(a: &str, b: &str, cs: bool) -> bool {
    compare(&parse(a).unwrap(), &parse(b).unwrap(), cs)
}

#[test]
fn cjson_compare_should_compare_numbers() {
    assert!(compare_from_string("1", "1", true));
    assert!(compare_from_string("0.0001", "0.0001", false));
    assert!(compare_from_string("1E100", "10E99", false));
    assert!(!compare_from_string("0.5E-100", "0.5E-101", false));
    assert!(!compare_from_string("1", "2", true));
}

#[test]
fn cjson_compare_should_compare_booleans_null_strings() {
    assert!(compare_from_string("true", "true", true));
    assert!(!compare_from_string("true", "false", false));
    assert!(compare_from_string("null", "null", true));
    assert!(!compare_from_string("null", "true", false));
    assert!(compare_from_string("\"abcdefg\"", "\"abcdefg\"", true));
    assert!(!compare_from_string("\"ABCDEFG\"", "\"abcdefg\"", false));
}

#[test]
fn cjson_compare_should_compare_raw() {
    let mut a = parse("\"[true, false]\"").unwrap();
    let mut b = parse("\"[true, false]\"").unwrap();
    a.kind = Type::Raw;
    b.kind = Type::Raw;
    assert!(compare(&a, &b, true));
}

#[test]
fn cjson_compare_should_compare_arrays_and_objects() {
    assert!(compare_from_string("[]", "[]", true));
    assert!(compare_from_string(
        "[false,true,null,42,\"string\",[],{}]",
        "[false, true, null, 42, \"string\", [], {}]",
        true
    ));
    assert!(!compare_from_string("[1,2,3]", "[1,2]", true));
    assert!(compare_from_string("{}", "{}", true));
    assert!(compare_from_string(
        "{\"false\": false, \"true\": true, \"null\": null, \"number\": 42, \"string\": \"string\", \"array\": [], \"object\": {}}",
        "{\"true\": true, \"false\": false, \"null\": null, \"number\": 42, \"string\": \"string\", \"array\": [], \"object\": {}}",
        true
    ));
    assert!(!compare_from_string(
        "{\"False\": false, \"true\": true, \"null\": null, \"number\": 42, \"string\": \"string\", \"array\": [], \"object\": {}}",
        "{\"true\": true, \"false\": false, \"null\": null, \"number\": 42, \"string\": \"string\", \"array\": [], \"object\": {}}",
        true
    ));
    assert!(compare_from_string(
        "{\"False\": false, \"true\": true, \"null\": null, \"number\": 42, \"string\": \"string\", \"array\": [], \"object\": {}}",
        "{\"true\": true, \"false\": false, \"null\": null, \"number\": 42, \"string\": \"string\", \"array\": [], \"object\": {}}",
        false
    ));
    assert!(!compare_from_string(
        "{\"one\": 1, \"two\": 2}",
        "{\"one\": 1, \"two\": 2, \"three\": 3}",
        true
    ));
}

#[test]
fn cjson_compare_invalid_is_not_equal() {
    let inv = Value::invalid();
    assert!(!compare(&inv, &inv, true));
}

// --- misc: get_object_item, types, nesting ---

#[test]
fn cjson_get_object_item_should_get_object_items() {
    let item = parse("{\"one\":1, \"Two\":2, \"tHree\":3}").unwrap();
    assert!(item.get_object_item("one").is_some());
    assert_eq!(item.get_object_item("tWo").unwrap().valuedouble, 2.0);
    assert_eq!(item.get_object_item("three").unwrap().valuedouble, 3.0);
    assert!(item.get_object_item("four").is_none());
    assert!(item.get_object_item_case_sensitive("Two").is_some());
    assert!(item.get_object_item_case_sensitive("One").is_none());
}

#[test]
fn cjson_get_object_item_should_not_crash_with_array() {
    let array = parse("[1]").unwrap();
    assert!(array.get_object_item("name").is_none());
    assert!(array.get_object_item_case_sensitive("name").is_none());
}

#[test]
fn typecheck_functions_should_check_type() {
    assert!(!Value::boolean(false).is_invalid());
    assert!(Value::invalid().is_invalid());
    assert!(Value::boolean(false).is_false());
    assert!(Value::boolean(false).is_bool());
    assert!(Value::boolean(true).is_true());
    assert!(Value::null().is_null());
    assert!(Value::number(1.0).is_number());
    assert!(Value::string("x").is_string());
    assert!(Value::array().is_array());
    assert!(Value::object().is_object());
    assert!(Value::raw("x").is_raw());
}

#[test]
fn cjson_should_not_parse_too_deeply_nested_jsons() {
    let deep = "[".repeat(NESTING_LIMIT);
    assert!(parse(&deep).is_err());
}

#[test]
fn cjson_set_number_value_should_set_numbers() {
    let mut n = Value::number(0.0);
    n.set_number(1.5);
    assert_eq!(n.valueint, 1);
    assert_eq!(n.valuedouble, 1.5);
    n.set_number(-1.5);
    assert_eq!(n.valueint, -1);
    n.set_number(1.0e10);
    assert_eq!(n.valueint, i32::MAX);
}

#[test]
fn cjson_detach_and_replace() {
    let mut arr = parse("[1,2,3]").unwrap();
    let detached = arr.detach_item_from_array(1).unwrap();
    assert_eq!(detached.valuedouble, 2.0);
    assert_eq!(arr.children.len(), 2);
    assert!(arr.replace_item_in_array(0, Value::number(9.0)));
    assert_eq!(arr.children[0].valuedouble, 9.0);
    let mut obj = parse("{\"a\":1,\"b\":2}").unwrap();
    let d = obj.detach_item_from_object("A").unwrap();
    assert_eq!(d.valuedouble, 1.0);
    assert!(obj.replace_item_in_object("b", Value::number(3.0)));
    assert_eq!(obj.get_object_item("b").unwrap().valuedouble, 3.0);
}

#[test]
fn cjson_add_helpers() {
    let mut root = Value::object();
    root.add_null_to_object("null");
    root.add_true_to_object("true");
    root.add_false_to_object("false");
    root.add_bool_to_object("bool", true);
    root.add_number_to_object("number", 1.0);
    root.add_string_to_object("string", "hi");
    root.add_raw_to_object("raw", "[]");
    root.add_object_to_object("obj");
    root.add_array_to_object("arr");
    assert!(root
        .get_object_item_case_sensitive("null")
        .unwrap()
        .is_null());
    assert!(root
        .get_object_item_case_sensitive("true")
        .unwrap()
        .is_true());
    assert_eq!(
        root.get_object_item_case_sensitive("string")
            .unwrap()
            .string_value_str(),
        Some("hi")
    );
}

#[test]
fn create_arrays() {
    let a = Value::create_int_array(&[1, 2, 3]);
    assert_eq!(a.children.len(), 3);
    let a = Value::create_double_array(&[1.0, 2.0]);
    assert_eq!(a.children.len(), 2);
    let a = Value::create_string_array(&["a", "b"]);
    assert_eq!(a.string_value_str_of(0), Some("a"));
}

#[test]
fn cjson_string_and_number_value() {
    let s = Value::string("hello");
    assert_eq!(s.string_value_str(), Some("hello"));
    assert!(Value::number(1.0).string_value().is_none());
    assert_eq!(Value::number(1.5).number_value(), 1.5);
    assert!(Value::null().number_value().is_nan());
}

#[test]
fn cjson_set_bool_value() {
    let mut o = Value::object();
    assert_eq!(o.set_bool(true), Type::Invalid);
    let mut b = Value::boolean(false);
    assert_eq!(b.set_bool(true), Type::True);
    assert!(b.is_true());
}

#[test]
fn cjson_insert_item_in_array() {
    let mut a = parse("[1,3]").unwrap();
    a.insert_item_in_array(1, Value::number(2.0));
    assert_eq!(a.children[1].valuedouble, 2.0);
    a.insert_item_in_array(10, Value::number(4.0));
    assert_eq!(a.children.last().unwrap().valuedouble, 4.0);
}

#[test]
fn readme_create_monitor() {
    let expected = "{\n\t\"name\":\t\"Awesome 4K\",\n\t\"resolutions\":\t[{\n\t\t\t\"width\":\t1280,\n\t\t\t\"height\":\t720\n\t\t}, {\n\t\t\t\"width\":\t1920,\n\t\t\t\"height\":\t1080\n\t\t}, {\n\t\t\t\"width\":\t3840,\n\t\t\t\"height\":\t2160\n\t\t}]\n}";
    let mut monitor = Value::object();
    monitor.add_string_to_object("name", "Awesome 4K");
    {
        let res = monitor.add_array_to_object("resolutions");
        for (w, h) in [(1280.0, 720.0), (1920.0, 1080.0), (3840.0, 2160.0)] {
            let mut r = Value::object();
            r.add_number_to_object("width", w);
            r.add_number_to_object("height", h);
            res.add_item_to_array(r);
        }
    }
    assert_eq!(print_pretty_str(&monitor), expected);
}

#[test]
fn supports_full_hd() {
    let json = "{\n\t\"name\":\t\"Awesome 4K\",\n\t\"resolutions\":\t[{\n\t\t\t\"width\":\t1280,\n\t\t\t\"height\":\t720\n\t\t}, {\n\t\t\t\"width\":\t1920,\n\t\t\t\"height\":\t1080\n\t\t}]\n}";
    let monitor = parse(json).unwrap();
    let resolutions = monitor
        .get_object_item_case_sensitive("resolutions")
        .unwrap();
    let mut found = false;
    for r in &resolutions.children {
        let w = r.get_object_item_case_sensitive("width").unwrap();
        let h = r.get_object_item_case_sensitive("height").unwrap();
        if compare_double(w.valuedouble, 1920.0) && compare_double(h.valuedouble, 1080.0) {
            found = true;
        }
    }
    assert!(found);
}

#[test]
fn duplicate_and_objects_preserve_order_and_duplicates() {
    let v = parse("{\"a\":1,\"a\":2,\"b\":3}").unwrap();
    assert_eq!(v.children.len(), 3);
    assert_eq!(v.get_object_item("a").unwrap().valuedouble, 1.0);
    let d = v.duplicate(true).unwrap();
    // cJSON_Compare looks up object members by key, so duplicate keys are
    // compared against the first match (`cJSON.c` get_object_item) and this
    // tree is not equal to itself. Printing still preserves both members.
    assert_eq!(print_compact_str(&v), "{\"a\":1,\"a\":2,\"b\":3}");
    assert_eq!(print_compact_str(&d), "{\"a\":1,\"a\":2,\"b\":3}");
}

#[allow(dead_code)]
fn _use_print_number(n: &Number) -> String {
    print_number(n)
}

#[allow(dead_code)]
fn _use_parse_string_at(b: &[u8]) {
    let _ = parse_string_at(b);
}
