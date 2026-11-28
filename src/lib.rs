//! lekha: a SQL database that gives the same answers as SQLite.

#![forbid(unsafe_code)]

use std::fmt;

mod lexer;
mod select;

/// The version of this build.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// One SQL value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
}

/// The text of a real value, as `CAST ( <real> AS TEXT )` of SQLite gives it.
/// Returns `None` for a value that is not a finite number. The engine returns no such value.
///
/// The server and the page print a real value with this function.
/// The rules are measured on SQLite 3.46.1. The tests of this function have the measured texts.
/// - The text has the first 15 digits of the number. A 16th digit of 5 or more rounds the
///   15th digit up, also when the rest is exactly a half:
///   `CAST(CAST(1000000000000025 AS REAL) AS TEXT)` is `1.00000000000003e+15`.
/// - Zeros at the end are removed, and one digit stays after the point: `1.0`, `3.5`.
/// - With `E` as the power of ten of the first digit, the text has an exponent when `E` is
///   below -4 or above 14: `0.0001` and `1.0e-05`, `999999999999999.0` and `1.0e+15`.
///   The exponent has a sign and two digits or more: `1.0e+100`.
/// - Each zero is `0.0`, also the negative zero: `CAST(0 * - CAST(1 AS REAL) AS TEXT)`.
pub fn real_text(real: f64) -> Option<String> {
    if !real.is_finite() {
        return None;
    }
    if real == 0.0 {
        return Some("0.0".to_string());
    }
    // The 18 digits of Rust are the exact number, rounded to the nearest 18 digits. The
    // rounding to 15 digits below gives the same text for them as for the exact number, with
    // one exception: the 18 digits are a half of the 15th digit (`500` at the end) and the
    // exact number is below them. Only then the exact digits are needed.
    let (mut digits, mut exponent) = decimal_digits(real.abs(), SHORT_PRECISION)?;
    if digits.get(DIGITS..) == Some(&[5, 0, 0][..]) {
        (digits, exponent) = decimal_digits(real.abs(), EXACT_PRECISION)?;
    }
    Some(rounded_text(real < 0.0, digits, exponent))
}

/// The number of digits that SQLite prints for a real number.
const DIGITS: usize = 15;

/// The digits after the point that `real_text` asks for first: 18 digits in all.
const SHORT_PRECISION: usize = DIGITS + 2;

/// A real number has 767 digits at most, so a text with this many digits after the point is
/// the exact number, not rounded.
const EXACT_PRECISION: usize = 766;

/// The digits of a positive finite number and the power of ten of the first digit.
/// The number is rounded to `precision` digits after the first one.
fn decimal_digits(real: f64, precision: usize) -> Option<(Vec<u8>, i32)> {
    let text = format!("{real:.precision$e}");
    let (mantissa, exponent) = text.split_once('e')?;
    let exponent: i32 = exponent.parse().ok()?;
    let digits: Vec<u8> = mantissa
        .bytes()
        .filter(u8::is_ascii_digit)
        .map(|b| b - b'0')
        .collect();
    Some((digits, exponent))
}

/// The text of the number with these digits, rounded to 15 digits as `real_text` says.
fn rounded_text(negative: bool, mut digits: Vec<u8>, mut exponent: i32) -> String {
    let round_up = digits.get(DIGITS).is_some_and(|digit| *digit >= 5);
    digits.truncate(DIGITS);
    if round_up {
        // Add 1 at the last digit. A carry out of the first digit makes the number `1` with
        // the next exponent.
        match digits.iter().rposition(|digit| *digit != 9) {
            Some(at) => {
                digits[at] += 1;
                digits.truncate(at + 1);
            }
            None => {
                digits = vec![1];
                exponent += 1;
            }
        }
    }
    while digits.len() > 1 && digits.last() == Some(&0) {
        digits.pop();
    }
    let digits: String = digits
        .iter()
        .map(|digit| char::from(b'0' + digit))
        .collect();
    let sign = if negative { "-" } else { "" };
    if !(-4..DIGITS as i32).contains(&exponent) {
        let (first, rest) = digits.split_at(1);
        let rest = if rest.is_empty() { "0" } else { rest };
        let exponent_sign = if exponent < 0 { '-' } else { '+' };
        format!(
            "{sign}{first}.{rest}e{exponent_sign}{:02}",
            exponent.unsigned_abs()
        )
    } else if exponent < 0 {
        let zeros = "0".repeat(exponent.unsigned_abs() as usize - 1);
        format!("{sign}0.{zeros}{digits}")
    } else {
        // The number of digits before the point.
        let whole = exponent.unsigned_abs() as usize + 1;
        if digits.len() <= whole {
            format!("{sign}{digits:0<whole$}.0")
        } else {
            let (before, after) = digits.split_at(whole);
            format!("{sign}{before}.{after}")
        }
    }
}

/// The reason why the database did not run a statement.
#[derive(Debug)]
pub struct Error(String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// One database in memory.
#[derive(Default)]
pub struct Database;

impl Database {
    pub fn new() -> Self {
        Self
    }

    /// Runs one SQL statement and returns its result rows.
    /// Returns an error for each statement that it does not support.
    /// The only supported statement is `SELECT` without `FROM`.
    /// `README.md` lists the supported expressions.
    pub fn execute(&mut self, sql: &str) -> Result<Vec<Vec<Value>>, Error> {
        Ok(vec![select::select(sql)?])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_set() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn execute_returns_one_row() {
        let mut db = Database::new();
        let rows = db.execute("SELECT 1 + 2 AS a, 4 / 0").unwrap();
        assert_eq!(rows, [[Value::Integer(3), Value::Null]]);
        let error = db.execute("SELECT 1 FROM t").unwrap_err();
        assert_eq!(error.to_string(), "unsupported syntax at Word(\"FROM\")");
        let rows = db.execute("SELECT CAST(7 AS REAL) / 2, AVG(1)").unwrap();
        assert_eq!(rows, [[Value::Real(3.5), Value::Real(1.0)]]);
    }

    /// The one value of `SELECT <expr>`, as a real number.
    fn real(expr: &str) -> f64 {
        let sql = format!("SELECT {expr}");
        match Database::new().execute(&sql).as_deref() {
            Ok([row]) => match row[..] {
                [Value::Real(real)] => real,
                _ => panic!("{sql}: {row:?}"),
            },
            other => panic!("{sql}: {other:?}"),
        }
    }

    // Each text is the answer of SQLite 3.46.1 for `SELECT CAST(<expr> AS TEXT)`.
    #[test]
    fn real_text_is_the_text_of_sqlite() {
        let big = "CAST(9223372036854775807 AS REAL)";
        let big6 = [big; 6].join(" * ");
        let big16 = [big; 16].join(" * ");
        let e9 = "CAST(1000000000 AS REAL)";
        for (expr, want) in [
            ("CAST(1 AS REAL)", "1.0"),
            ("CAST(0 AS REAL)", "0.0"),
            ("0 * - CAST(1 AS REAL)", "0.0"),
            ("- CAST(0 AS REAL)", "0.0"),
            ("CAST(12 AS REAL)", "12.0"),
            ("CAST(1200 AS REAL)", "1200.0"),
            ("CAST(7 AS REAL) / 2", "3.5"),
            ("- CAST(7 AS REAL) / 2", "-3.5"),
            ("CAST(5 AS REAL) / 4", "1.25"),
            ("CAST(1 AS REAL) / 2", "0.5"),
            ("CAST(1 AS REAL) / 10", "0.1"),
            ("CAST(1 AS REAL) / 3", "0.333333333333333"),
            ("CAST(2 AS REAL) / 3", "0.666666666666667"),
            ("- CAST(1 AS REAL) / 3", "-0.333333333333333"),
            ("CAST(1 AS REAL) / 7", "0.142857142857143"),
            ("CAST(100 AS REAL) / 3", "33.3333333333333"),
            ("CAST(1 AS REAL) / 10 + CAST(2 AS REAL) / 10", "0.3"),
            ("CAST(123456789 AS REAL) / 1000", "123456.789"),
            (
                "CAST(314159265358979 AS REAL) / 100000000000000",
                "3.14159265358979",
            ),
            (
                "CAST(271828182845904523 AS REAL) / 100000000000000000",
                "2.71828182845905",
            ),
            ("CAST(4503599627370497 AS REAL) / 4503599627370496", "1.0"),
            // The last number without an exponent, and the first one with it.
            ("CAST(100000000000000 AS REAL)", "100000000000000.0"),
            ("CAST(123456789012345 AS REAL)", "123456789012345.0"),
            ("CAST(999999999999999 AS REAL)", "999999999999999.0"),
            ("CAST(999999999999999 AS REAL) / 10", "99999999999999.9"),
            ("CAST(9007199254740991 AS REAL) / 1000", "9007199254740.99"),
            ("CAST(1000000000000000 AS REAL)", "1.0e+15"),
            ("CAST(1999999999999999 AS REAL) / 2", "1.0e+15"),
            ("CAST(1234567890123456 AS REAL)", "1.23456789012346e+15"),
            ("CAST(9007199254740991 AS REAL)", "9.00719925474099e+15"),
            ("CAST(9999999999999999 AS REAL)", "1.0e+16"),
            ("CAST(10000000000000000 AS REAL)", "1.0e+16"),
            ("CAST(123456789012345678 AS REAL)", "1.23456789012346e+17"),
            ("CAST(9223372036854775807 AS REAL)", "9.22337203685478e+18"),
            (
                "CAST(-9223372036854775807 - 1 AS REAL)",
                "-9.22337203685478e+18",
            ),
            // A 16th digit that is exactly a half rounds up.
            ("CAST(1000000000000005 AS REAL)", "1.00000000000001e+15"),
            ("CAST(1000000000000015 AS REAL)", "1.00000000000002e+15"),
            ("CAST(1000000000000025 AS REAL)", "1.00000000000003e+15"),
            ("CAST(1000000000000035 AS REAL)", "1.00000000000004e+15"),
            ("CAST(1000000000000045 AS REAL)", "1.00000000000005e+15"),
            ("- CAST(1000000000000025 AS REAL)", "-1.00000000000003e+15"),
            ("CAST(8000000000000065 AS REAL)", "8.00000000000007e+15"),
            ("CAST(8000000000000075 AS REAL)", "8.00000000000008e+15"),
            ("CAST(1000000000000024 AS REAL)", "1.00000000000002e+15"),
            ("CAST(1000000000000026 AS REAL)", "1.00000000000003e+15"),
            ("CAST(1000000000000025 AS REAL) / 10", "100000000000003.0"),
            ("CAST(1000000000000035 AS REAL) / 10", "100000000000004.0"),
            // Small numbers.
            ("CAST(1 AS REAL) / 1000", "0.001"),
            ("CAST(1 AS REAL) / 10000", "0.0001"),
            ("CAST(1 AS REAL) / 100000", "1.0e-05"),
            ("CAST(15 AS REAL) / 1000000", "1.5e-05"),
            ("CAST(1 AS REAL) / 3 / 10000", "3.33333333333333e-05"),
            ("CAST(1 AS REAL) / 3 / 100000", "3.33333333333333e-06"),
            ("- CAST(1 AS REAL) / 8 / 1000000", "-1.25e-07"),
            (
                "CAST(1 AS REAL) / 9223372036854775807",
                "1.0842021724855e-19",
            ),
            // An exponent of three digits, and the smallest numbers.
            (big6.as_str(), "6.15656346818664e+113"),
            (&format!("1 / ({big6})"), "1.62428277588202e-114"),
            (&[e9; 11].join(" * "), "1.0e+99"),
            (&format!("{} * 10", [e9; 11].join(" * ")), "1.0e+100"),
            (&[e9; 12].join(" * "), "1.0e+108"),
            (big16.as_str(), "2.74306203439684e+303"),
            (&format!("1 / ({big16})"), "3.6455610097782e-304"),
            (
                &format!("1 / ({big16}) / 9223372036854775807"),
                "3.95252516672997e-323",
            ),
            (
                &format!("1 / ({big16}) / 9223372036854775807 / 9223372036854775807"),
                "0.0",
            ),
        ] {
            assert_eq!(real_text(real(expr)).as_deref(), Some(want), "{expr}");
        }
        // SQLite has `Inf` for an infinity. The engine returns no such value.
        for real in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
            assert_eq!(real_text(real), None);
        }
    }

    /// The text of a positive finite number from its exact digits. It never uses the 18 digits.
    fn exact_text(real: f64) -> String {
        let (digits, exponent) = decimal_digits(real, EXACT_PRECISION).unwrap();
        rounded_text(false, digits, exponent)
    }

    // `real_text` reads 18 digits and not the exact number. Its text is the same.
    #[test]
    fn real_text_of_18_digits_is_the_text_of_the_exact_number() {
        let check = |real: f64| {
            if real.is_finite() && real > 0.0 {
                assert_eq!(real_text(real), Some(exact_text(real)), "{real:e}");
                assert_eq!(
                    real_text(-real),
                    Some(format!("-{}", exact_text(real))),
                    "{real:e}"
                );
            }
        };
        // Numbers at a half of the 15th digit, and the numbers next to them.
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = || {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            state >> 11
        };
        for _ in 0..200 {
            let half = (next() % 900_000_000_000_000 + 100_000_000_000_000) * 10 + 5;
            let mut scale = 1.0;
            for _ in 0..24 {
                for real in [half as f64 / scale, half as f64 * scale] {
                    check(real);
                    check(f64::from_bits(real.to_bits() + 1));
                    check(f64::from_bits(real.to_bits() - 1));
                }
                scale *= 10.0;
            }
        }
        // Numbers of each size: each bit pattern is a number, also the smallest ones.
        for _ in 0..5000 {
            check(f64::from_bits(next() << 11 >> 1));
        }
        check(f64::MIN_POSITIVE);
        check(f64::from_bits(1));
        check(f64::MAX);
    }
}
