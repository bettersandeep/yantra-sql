//! The lekha engine as a WebAssembly module for the playground page.
//!
//! The engine crate forbids `unsafe` and has no dependency.
//! The page and the module share memory through raw pointers, and that needs `unsafe`.
//! All of that `unsafe` is in this file.
//!
//! A panic of the engine is a trap on this target, and Rust code cannot catch it.
//! `site.js` catches the trap and makes a new instance of the module.

use lekha::{Database, Value};

/// The longest SQL that `run` accepts, in bytes. `site.js` has the same limit.
const MAX_SQL: usize = 2000;

/// Gives the page `len` bytes of memory, for the SQL.
#[unsafe(no_mangle)]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    Box::into_raw(vec![0u8; len].into_boxed_slice()).cast()
}

/// Frees memory that the page got from `alloc` or from `run`.
///
/// # Safety
///
/// `ptr` and `len` are those of one buffer from `alloc` or `run`, and the buffer is not freed yet.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)) });
}

/// Runs the SQL in the `len` bytes at `ptr` and returns the answer as JSON.
/// The return value is `ptr << 32 | len` of a new buffer, which the page frees with `dealloc`.
///
/// # Safety
///
/// `ptr` points to `len` bytes that can be read.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn run(ptr: *const u8, len: usize) -> u64 {
    let sql = unsafe { std::slice::from_raw_parts(ptr, len) };
    let json = answer(sql).into_bytes().into_boxed_slice();
    let len = json.len() as u64;
    ((Box::into_raw(json).cast::<u8>() as u64) << 32) | len
}

/// The answer to one SQL statement: `{"ok":true,"rows":[[...]]}` or `{"ok":false,"error":"..."}`.
/// A value is a JSON string, so a large integer keeps each digit, and NULL is `null`.
fn answer(sql: &[u8]) -> String {
    if sql.len() > MAX_SQL {
        return error(&format!("The SQL is longer than {MAX_SQL} bytes."));
    }
    let Ok(sql) = std::str::from_utf8(sql) else {
        return error("The SQL is not UTF-8.");
    };
    let rows = match Database::new().execute(sql) {
        Ok(rows) => rows,
        Err(e) => return error(&e.to_string()),
    };
    let mut json = String::from("{\"ok\":true,\"rows\":[");
    for (i, row) in rows.iter().enumerate() {
        json.push_str(if i == 0 { "[" } else { ",[" });
        for (j, value) in row.iter().enumerate() {
            if j > 0 {
                json.push(',');
            }
            match value {
                Value::Null => json.push_str("null"),
                Value::Integer(i) => quote(&mut json, &i.to_string()),
                Value::Text(s) => quote(&mut json, s),
                // The engine returns no real today. The page does not guess how SQLite prints one.
                Value::Real(_) => return error("The page cannot show a real value yet."),
            }
        }
        json.push(']');
    }
    json.push_str("]}");
    json
}

fn error(message: &str) -> String {
    let mut json = String::from("{\"ok\":false,\"error\":");
    quote(&mut json, message);
    json.push('}');
    json
}

/// Appends `text` as a JSON string.
fn quote(json: &mut String, text: &str) {
    json.push('"');
    for c in text.chars() {
        match c {
            '"' | '\\' => json.extend(['\\', c]),
            c if c < ' ' => json.push_str(&format!("\\u{:04x}", c as u32)),
            c => json.push(c),
        }
    }
    json.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answer_is_json() {
        assert_eq!(
            answer(b"SELECT 7 / 2, 1 / 0"),
            r#"{"ok":true,"rows":[["3",null]]}"#
        );
        assert_eq!(
            answer(b"SELECT 1 FROM t"),
            r#"{"ok":false,"error":"unsupported syntax at Word(\"FROM\")"}"#
        );
        assert_eq!(
            answer(b"\xff"),
            r#"{"ok":false,"error":"The SQL is not UTF-8."}"#
        );
    }

    #[test]
    fn answer_has_a_size_limit() {
        let long = format!("SELECT {}", " ".repeat(MAX_SQL));
        assert_eq!(
            answer(long.as_bytes()),
            r#"{"ok":false,"error":"The SQL is longer than 2000 bytes."}"#
        );
        let sql = format!("SELECT 1{}", " ".repeat(MAX_SQL - 8));
        assert_eq!(answer(sql.as_bytes()), r#"{"ok":true,"rows":[["1"]]}"#);
    }

    #[test]
    fn quote_escapes() {
        let mut json = String::new();
        quote(&mut json, "a\"b\\c\nd\u{e9}");
        assert_eq!(json, "\"a\\\"b\\\\c\\u000ad\u{e9}\"");
    }
}
