//! Runs the sqllogictest files under `tests/slt/` against the engine.
//!
//! Prints one line for each file: `slt: <file> <passed>/<applicable>`.
//! `tests/slt/baseline.txt` holds the exact pass count of each file.
//! Set `SLT_FAILURES=<n>` to print the first n records that fail in each file.

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::panic::{self, AssertUnwindSafe};
use std::path::Path;
use std::sync::Once;
use std::{env, fs};

use lekha::{Database, Value};

/// The database name that `skipif` and `onlyif` lines are compared with.
const IDENTITY: &str = "sqlite";

type Rows = Vec<Vec<Value>>;

/// The type of one result column.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Type {
    Integer,
    Real,
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Sort {
    None,
    Row,
    Value,
}

#[derive(Debug, PartialEq)]
enum Kind {
    /// `ok` is true for `statement ok` and false for `statement error`.
    Statement { ok: bool },
    /// `expected` is `None` when the file gives only an md5 hash of the values.
    Query {
        types: Vec<Type>,
        sort: Sort,
        expected: Option<Vec<String>>,
    },
}

#[derive(Debug, PartialEq)]
struct Record {
    /// The line number of the record header.
    line: usize,
    sql: String,
    kind: Kind,
}

/// True for `<N> values hashing to <32 hex chars>`.
fn is_hash(line: &str) -> bool {
    match line.split(' ').collect::<Vec<_>>()[..] {
        [count, "values", "hashing", "to", hash] => {
            count.parse::<u64>().is_ok()
                && hash.len() == 32
                && hash.bytes().all(|b| b.is_ascii_hexdigit())
        }
        _ => false,
    }
}

/// Parses a test file and returns the records that apply to us.
/// Each record is parsed, also a record that a condition skips.
/// Returns an error for input that it does not understand.
fn parse(text: &str) -> Result<Vec<Record>, String> {
    let mut records = Vec::new();
    // Comment lines are removed before the records are read, as the format document says.
    // A comment line is not a blank line, so it does not end a record.
    // Known limit: an expected value that starts with `#` is removed too.
    let mut lines = text
        .lines()
        .enumerate()
        .map(|(i, l)| (i + 1, l))
        .filter(|(_, l)| !l.starts_with('#'))
        .peekable();
    loop {
        // Blank lines and conditions come before the record header.
        let mut conditioned = false;
        let mut skip = false;
        let (line, header) = loop {
            let Some((line, text)) = lines.next() else {
                if conditioned {
                    return Err("end of file after a condition".to_string());
                }
                return Ok(records);
            };
            let words: Vec<&str> = text.split_whitespace().collect();
            match words[..] {
                [] if conditioned => {
                    return Err(format!("line {line}: blank line after a condition"));
                }
                [] => {}
                ["skipif", db, ..] => {
                    skip |= db == IDENTITY;
                    conditioned = true;
                }
                ["onlyif", db, ..] => {
                    skip |= db != IDENTITY;
                    conditioned = true;
                }
                _ => break (line, words),
            }
        };
        // The record continues to the next blank line.
        let mut body = Vec::new();
        while let Some((_, text)) = lines.next_if(|(_, text)| !text.trim().is_empty()) {
            body.push(text);
        }
        let (sql, kind) = match header[..] {
            ["hash-threshold", n] if body.is_empty() && n.parse::<u64>().is_ok() => continue,
            ["halt"] if body.is_empty() && skip => continue,
            ["halt"] if body.is_empty() => return Ok(records),
            ["statement", "ok"] => (&body[..], Kind::Statement { ok: true }),
            ["statement", "error"] => (&body[..], Kind::Statement { ok: false }),
            ["query", types, ref rest @ ..] if rest.len() <= 2 => {
                let types = types
                    .chars()
                    .map(|c| match c {
                        'I' => Ok(Type::Integer),
                        'R' => Ok(Type::Real),
                        'T' => Ok(Type::Text),
                        _ => Err(format!("line {line}: unknown column type {c}")),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                // The word after the sort mode is a label. It has no effect here.
                let sort = match rest.first().copied() {
                    None | Some("nosort") => Sort::None,
                    Some("rowsort") => Sort::Row,
                    Some("valuesort") => Sort::Value,
                    Some(other) => return Err(format!("line {line}: unknown sort mode {other}")),
                };
                // Without a `----` line the whole body is SQL and the expected result is empty.
                let split = body.iter().position(|l| *l == "----").unwrap_or(body.len());
                let values = body.get(split + 1..).unwrap_or_default();
                let expected = if let [one] = values
                    && is_hash(one)
                {
                    None
                } else if values.len() % types.len() != 0 {
                    return Err(format!(
                        "line {line}: {} expected values do not fill rows of {} columns",
                        values.len(),
                        types.len()
                    ));
                } else {
                    Some(values.iter().map(|v| v.to_string()).collect())
                };
                let kind = Kind::Query {
                    types,
                    sort,
                    expected,
                };
                (&body[..split], kind)
            }
            _ => {
                return Err(format!(
                    "line {line}: cannot understand this record: {}",
                    header.join(" ")
                ));
            }
        };
        if sql.is_empty() {
            return Err(format!("line {line}: record without SQL"));
        }
        if !skip {
            records.push(Record {
                line,
                sql: sql.join("\n"),
                kind,
            });
        }
    }
}

/// Renders a value the way the test files write it.
/// Returns `None` when the value does not fit the column type.
fn render(value: &Value, ty: Type) -> Option<String> {
    const TWO_POW_63: f64 = 9_223_372_036_854_775_808.0;
    match (ty, value) {
        (_, Value::Null) => Some("NULL".to_string()),
        (Type::Integer | Type::Text, Value::Integer(i)) => Some(i.to_string()),
        // Truncate toward zero. A real outside the i64 range has no rendering.
        (Type::Integer, Value::Real(r)) => {
            (r.trunc() >= -TWO_POW_63 && r.trunc() < TWO_POW_63).then(|| (*r as i64).to_string())
        }
        (Type::Real, Value::Integer(i)) => Some(format!("{:.3}", *i as f64)),
        (Type::Real, Value::Real(r)) => Some(format!("{r:.3}")),
        (Type::Text, Value::Text(s)) if s.is_empty() => Some("(empty)".to_string()),
        // A byte outside printable ASCII becomes `@`, as in the C program with `isprint`.
        // The C program works on bytes, so a non-ASCII char gives one `@` for each UTF-8 byte.
        (Type::Text, Value::Text(s)) => Some(
            s.bytes()
                .map(|b| {
                    if (0x20..=0x7e).contains(&b) {
                        b as char
                    } else {
                        '@'
                    }
                })
                .collect(),
        ),
        (Type::Integer | Type::Real, Value::Text(_)) | (Type::Text, Value::Real(_)) => None,
    }
}

/// Compares result rows with the expected values.
/// The error says what the engine returned.
fn compare(rows: &Rows, types: &[Type], sort: Sort, expected: &[String]) -> Result<(), String> {
    let mut rendered = Vec::new();
    for row in rows {
        if row.len() != types.len() {
            return Err(format!(
                "a row of {} values for {} columns",
                row.len(),
                types.len()
            ));
        }
        let mut cells = Vec::new();
        for (value, ty) in row.iter().zip(types) {
            let Some(cell) = render(value, *ty) else {
                return Err(format!("{value:?} in a column of type {ty:?}"));
            };
            cells.push(cell);
        }
        rendered.push(cells);
    }
    if sort == Sort::Row {
        rendered.sort();
    }
    let mut got = rendered.concat();
    if sort == Sort::Value {
        got.sort();
    }
    if got == expected {
        Ok(())
    } else {
        Err(got.join(", "))
    }
}

thread_local! {
    /// True while this thread is inside an engine call.
    static IN_ENGINE: Cell<bool> = const { Cell::new(false) };
}

/// Stops the default panic print for panics inside an engine call.
/// Other panics, such as a failed assert, print as usual.
fn quiet_engine_panics() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let default = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            if !IN_ENGINE.get() {
                default(info);
            }
        }));
    });
}

/// Runs one record. `Ok` is a pass. `Err` says what the engine did.
fn run(
    record: &Record,
    execute: &mut dyn FnMut(&str) -> Result<Rows, String>,
) -> Result<(), String> {
    quiet_engine_panics();
    IN_ENGINE.set(true);
    let result = panic::catch_unwind(AssertUnwindSafe(|| execute(&record.sql)));
    IN_ENGINE.set(false);
    // A panic fails each kind of record, also `statement error`.
    let result = result.map_err(|payload| {
        let message = payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&str>().copied())
            .unwrap_or("no message");
        format!("panic: {message}")
    })?;
    match (&record.kind, result) {
        (Kind::Statement { ok: true }, Ok(_)) | (Kind::Statement { ok: false }, Err(_)) => Ok(()),
        (Kind::Statement { ok: false }, Ok(_)) => Err("no error".to_string()),
        (Kind::Query { expected: None, .. }, _) => {
            Err("the harness cannot check an md5 hash".to_string())
        }
        (
            Kind::Query {
                types,
                sort,
                expected: Some(expected),
            },
            Ok(rows),
        ) => compare(&rows, types, *sort, expected),
        (_, Err(error)) => Err(format!("error: {error}")),
    }
}

/// Checks the measured pass counts against the text of the baseline file.
/// Returns one message for each problem.
fn check_baseline(measured: &BTreeMap<String, usize>, baseline: &str) -> Vec<String> {
    let mut errors = Vec::new();
    let mut seen = BTreeSet::new();
    for line in baseline.lines() {
        let entry = line
            .rsplit_once(' ')
            .and_then(|(file, passed)| Some((file, passed.parse::<usize>().ok()?)));
        match entry {
            None => errors.push(format!("baseline: cannot understand the line: {line}")),
            Some((file, _)) if !seen.insert(file) => {
                errors.push(format!("baseline: more than one line for {file}"));
            }
            Some((file, want)) => match measured.get(file) {
                None => errors.push(format!("baseline: {file} is not a test file")),
                Some(got) if *got != want => errors.push(format!(
                    "{file}: {got} records pass, but the baseline says {want}. \
                     Set the number in tests/slt/baseline.txt to exactly {got}."
                )),
                Some(_) => {}
            },
        }
    }
    for file in measured.keys() {
        if !seen.contains(file.as_str()) {
            errors.push(format!("baseline: no line for {file}"));
        }
    }
    errors
}

/// Collects the `*.test` files under `dir`, with paths relative to the first `dir`.
fn find_tests(dir: &Path, prefix: &str, files: &mut Vec<String>) {
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().into_string().unwrap();
        if entry.path().is_dir() {
            find_tests(&entry.path(), &format!("{prefix}{name}/"), files);
        } else if name.ends_with(".test") {
            files.push(format!("{prefix}{name}"));
        }
    }
}

#[test]
fn slt() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/slt");
    let show: usize = match env::var("SLT_FAILURES") {
        Ok(n) => n.parse().expect("SLT_FAILURES must be a number"),
        Err(env::VarError::NotPresent) => 0,
        Err(error) => panic!("SLT_FAILURES: {error}"),
    };
    let mut files = Vec::new();
    find_tests(&root, "", &mut files);
    files.sort();
    assert!(!files.is_empty(), "no test files under tests/slt");

    let mut measured = BTreeMap::new();
    for file in files {
        let text = fs::read_to_string(root.join(&file)).unwrap();
        let records = parse(&text).unwrap_or_else(|error| panic!("{file}: {error}"));
        let mut db = Database::new();
        let mut passed = 0;
        let mut shown = 0;
        for record in &records {
            match run(record, &mut |sql| {
                db.execute(sql).map_err(|e| e.to_string())
            }) {
                Ok(()) => passed += 1,
                Err(got) if shown < show => {
                    shown += 1;
                    let expected = match &record.kind {
                        Kind::Statement { ok: true } => "ok".to_string(),
                        Kind::Statement { ok: false } => "an error".to_string(),
                        Kind::Query { expected: None, .. } => "an md5 hash".to_string(),
                        Kind::Query {
                            expected: Some(values),
                            ..
                        } => values.join(", "),
                    };
                    println!(
                        "FAIL {file}:{}\n  sql: {}\n  expected: {expected}\n  got: {got}",
                        record.line, record.sql
                    );
                }
                Err(_) => {}
            }
        }
        println!("slt: {file} {passed}/{}", records.len());
        measured.insert(file, passed);
    }

    let baseline = fs::read_to_string(root.join("baseline.txt")).unwrap();
    let errors = check_baseline(&measured, &baseline);
    assert!(errors.is_empty(), "\n{}", errors.join("\n"));
}

fn query(types: Vec<Type>, sort: Sort, expected: &[&str]) -> Kind {
    Kind::Query {
        types,
        sort,
        expected: Some(expected.iter().map(|v| v.to_string()).collect()),
    }
}

#[test]
fn parse_reads_records_and_conditions() {
    let text = "hash-threshold 8\r\n\r\n# a comment\r\nstatement ok\r\nCREATE TABLE t(a)\r\n\r\n\
                onlyif mysql # not for us\r\nquery I\r\nSELECT 1 DIV 1\r\n----\r\n1\r\n\r\n\
                skipif sqlite\r\nstatement error\r\nSELECT 2\r\n\r\n\
                skipif mysql\r\nonlyif sqlite\r\nquery IT rowsort label-1\r\nSELECT a, b\r\n  FROM t\r\n----\r\n1\r\nx\r\n\r\n\
                query I valuesort\r\nSELECT 3\r\n----\r\n\r\n\
                onlyif mssql\r\nhalt\r\n\r\n\
                query R nosort\r\nSELECT 4\r\n----\r\n30 values hashing to 0123456789abcdef0123456789ABCDEF\r\n\r\n\
                halt\r\n\r\nthis line is after halt\r\n";
    let record = |line, sql: &str, kind| Record {
        line,
        sql: sql.to_string(),
        kind,
    };
    let want = vec![
        record(4, "CREATE TABLE t(a)", Kind::Statement { ok: true }),
        record(
            19,
            "SELECT a, b\n  FROM t",
            query(vec![Type::Integer, Type::Text], Sort::Row, &["1", "x"]),
        ),
        record(26, "SELECT 3", query(vec![Type::Integer], Sort::Value, &[])),
        record(
            33,
            "SELECT 4",
            Kind::Query {
                types: vec![Type::Real],
                sort: Sort::None,
                expected: None,
            },
        ),
    ];
    assert_eq!(parse(text), Ok(want));
}

#[test]
fn parse_fails_closed() {
    for text in [
        "select 1\n",
        "statement maybe\nSELECT 1\n",
        "statement ok\n",
        "statement ok now\nSELECT 1\n",
        "query\nSELECT 1\n----\n1\n",
        "query X\nSELECT 1\n----\n1\n",
        "query I colsort\nSELECT 1\n----\n1\n",
        "query I rowsort label-1 more\nSELECT 1\n----\n1\n",
        "query I\n----\n1\n",
        "query II\nSELECT 1, 2\n----\n1\n2\n3\n",
        "hash-threshold\n",
        "hash-threshold many\n",
        "halt now\n",
        "skipif\nstatement ok\nSELECT 1\n",
        "onlyif sqlite\n",
        "onlyif sqlite\n\nstatement ok\nSELECT 1\n",
        "onlyif mysql\nfrobnicate\n",
    ] {
        assert!(parse(text).is_err(), "parsed: {text:?}");
    }
}

#[test]
fn render_follows_the_column_type() {
    let text = |s: &str| Value::Text(s.to_string());
    let cases = [
        (Type::Integer, Value::Null, Some("NULL")),
        (Type::Integer, Value::Integer(-7), Some("-7")),
        (Type::Integer, Value::Real(-2.9), Some("-2")),
        (Type::Integer, Value::Real(1e19), None),
        (Type::Integer, Value::Real(f64::NAN), None),
        (Type::Integer, text("7"), None),
        (Type::Real, Value::Null, Some("NULL")),
        (Type::Real, Value::Integer(2), Some("2.000")),
        (Type::Real, Value::Real(-0.12345), Some("-0.123")),
        (Type::Real, text("1.5"), None),
        (Type::Text, Value::Null, Some("NULL")),
        (Type::Text, text("abc"), Some("abc")),
        (Type::Text, text(""), Some("(empty)")),
        (Type::Text, Value::Integer(5), Some("5")),
        (Type::Text, Value::Real(1.5), None),
    ];
    for (ty, value, want) in cases {
        assert_eq!(render(&value, ty).as_deref(), want, "{value:?} as {ty:?}");
    }
}

#[test]
fn compare_applies_the_sort_mode() {
    let int = Value::Integer;
    let rows = vec![vec![int(2), int(10)], vec![int(10), int(1)]];
    let types = [Type::Integer, Type::Integer];
    let check = |sort, expected: &[&str]| {
        let expected: Vec<String> = expected.iter().map(|v| v.to_string()).collect();
        compare(&rows, &types, sort, &expected)
    };
    assert_eq!(check(Sort::None, &["2", "10", "10", "1"]), Ok(()));
    // Rows and values sort as strings: "10" is before "2".
    assert_eq!(check(Sort::Row, &["10", "1", "2", "10"]), Ok(()));
    assert_eq!(check(Sort::Value, &["1", "10", "10", "2"]), Ok(()));
    assert_eq!(
        check(Sort::None, &["10", "1", "2", "10"]),
        Err("2, 10, 10, 1".to_string())
    );
    assert!(check(Sort::None, &["2", "10"]).is_err());
    // A row that is too wide or too narrow fails.
    assert!(compare(&rows, &[Type::Integer], Sort::None, &[]).is_err());
    // A value that does not fit the column type fails.
    let text = vec![vec![Value::Text("1".to_string())]];
    assert!(compare(&text, &[Type::Integer], Sort::None, &["1".to_string()]).is_err());
}

#[test]
fn run_decides_pass_or_fail() {
    let record = |kind| Record {
        line: 1,
        sql: "SELECT 1".to_string(),
        kind,
    };
    let mut one = |_: &str| Ok(vec![vec![Value::Integer(1)]]);
    let mut error = |_: &str| Err("no".to_string());
    let mut panics = |_: &str| -> Result<Rows, String> { panic!("boom") };
    let hash = Kind::Query {
        types: vec![Type::Integer],
        sort: Sort::None,
        expected: None,
    };

    let statement_ok = record(Kind::Statement { ok: true });
    assert_eq!(run(&statement_ok, &mut one), Ok(()));
    assert_eq!(run(&statement_ok, &mut error), Err("error: no".to_string()));
    assert_eq!(
        run(&statement_ok, &mut panics),
        Err("panic: boom".to_string())
    );

    let statement_error = record(Kind::Statement { ok: false });
    assert_eq!(run(&statement_error, &mut error), Ok(()));
    assert_eq!(run(&statement_error, &mut one), Err("no error".to_string()));
    assert!(run(&statement_error, &mut panics).is_err());

    let query_one = record(query(vec![Type::Integer], Sort::None, &["1"]));
    assert_eq!(run(&query_one, &mut one), Ok(()));
    assert!(run(&query_one, &mut error).is_err());
    assert!(run(&query_one, &mut panics).is_err());
    let query_two = record(query(vec![Type::Integer], Sort::None, &["2"]));
    assert_eq!(run(&query_two, &mut one), Err("1".to_string()));
    // A hash cannot be checked, so the record fails for each result.
    assert!(run(&record(hash), &mut one).is_err());
}

#[test]
fn baseline_must_match_exactly() {
    let measured = BTreeMap::from([("a.test".to_string(), 3), ("dir/b c.test".to_string(), 0)]);
    let problems = |baseline| check_baseline(&measured, baseline).len();
    assert_eq!(problems("a.test 3\ndir/b c.test 0\n"), 0);
    // A lower or higher number, a file with no line, a line with no file,
    // a line that does not parse and a file with two lines are all errors.
    assert_eq!(problems("a.test 2\ndir/b c.test 0\n"), 1);
    assert_eq!(problems("a.test 4\ndir/b c.test 0\n"), 1);
    assert_eq!(problems("a.test 3\n"), 1);
    assert_eq!(problems("a.test 3\ndir/b c.test 0\ngone.test 0\n"), 1);
    assert_eq!(problems("a.test 3\ndir/b c.test 0\n\n"), 1);
    assert_eq!(problems("a.test three\ndir/b c.test 0\n"), 2);
    assert_eq!(problems("a.test 3\na.test 3\ndir/b c.test 0\n"), 1);
    let message = &check_baseline(&measured, "a.test 2\ndir/b c.test 0\n")[0];
    assert!(message.contains("3 records pass") && message.contains("exactly 3"));
}

#[test]
fn query_without_results_expects_an_empty_set() {
    let empty = |sql: &str| Record {
        line: 1,
        sql: sql.to_string(),
        kind: query(vec![Type::Integer], Sort::None, &[]),
    };
    // No `----` line: the whole body is SQL.
    assert_eq!(
        parse("query I\nSELECT 1\n  WHERE 0\n"),
        Ok(vec![empty("SELECT 1\n  WHERE 0")])
    );
    // A `----` line with no values after it.
    assert_eq!(
        parse("query I\nSELECT 1\n----\n"),
        Ok(vec![empty("SELECT 1")])
    );
    let mut no_rows = |_: &str| Ok(vec![]);
    let mut one_row = |_: &str| Ok(vec![vec![Value::Integer(1)]]);
    assert_eq!(run(&empty("SELECT 1"), &mut no_rows), Ok(()));
    assert!(run(&empty("SELECT 1"), &mut one_row).is_err());
}

#[test]
fn comment_lines_are_removed_before_parsing() {
    // Comments before the header, in the SQL and in the results.
    let text =
        "# c\nskipif mysql\n# c\nquery I\n# c\nSELECT 1\n# c\n  + 1\n# c\n----\n# c\n2\n# c\n";
    let want = Record {
        line: 4,
        sql: "SELECT 1\n  + 1".to_string(),
        kind: query(vec![Type::Integer], Sort::None, &["2"]),
    };
    assert_eq!(parse(text), Ok(vec![want]));
    // A comment line is not a blank line: it does not end a record.
    let text = "statement ok\nSELECT 1\n# c\nSELECT 2\n\n# c\n# c\n\nstatement ok\nSELECT 3\n";
    let statement = |line, sql: &str| Record {
        line,
        sql: sql.to_string(),
        kind: Kind::Statement { ok: true },
    };
    assert_eq!(
        parse(text),
        Ok(vec![
            statement(1, "SELECT 1\nSELECT 2"),
            statement(9, "SELECT 3")
        ])
    );
}

#[test]
fn unprintable_text_renders_as_at() {
    let render_text = |s: &str| render(&Value::Text(s.to_string()), Type::Text);
    assert_eq!(render_text(" a b~"), Some(" a b~".to_string()));
    assert_eq!(render_text("a\tb\r\n\u{7f}"), Some("a@b@@@".to_string()));
    // One `@` for each UTF-8 byte: U+00E9 has two bytes.
    assert_eq!(render_text("caf\u{e9}"), Some("caf@@".to_string()));
}
