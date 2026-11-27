# yantra-sql

A SQL database that gives the same answers as SQLite, with Parquet storage.

AI coding agents write this code.
The [yantra](https://github.com/bettersandeep/yantra) factory gives them the work, checks each change, and decides what merges.

## Rule of this repo

The first commit is a seed made by hand: an empty crate, CI and the licence.
Each change after the seed arrives as a pull request from the factory.

## Design decisions

- Language: Rust.
- Answers: the same as SQLite, checked with SQLite's public test suite.
- Storage: Parquet files, with a small row area for new and changed rows.

## Tests

`tests/slt.rs` runs each `*.test` file under `tests/slt/` against a new database.
The files use the sqllogictest format of SQLite.
A record passes when the engine gives the same result as SQLite.
Records that are only for other databases do not count.
A record that gives only an md5 hash of its result counts as failed, because the harness has no md5.
The harness fails when it does not understand a line of a test file.

The test prints one line for each file: `slt: <file> <passed>/<applicable>`.
`<applicable>` is the number of records that apply to SQLite.
`<passed>` is the number of these records that pass.

`tests/slt/baseline.txt` holds the exact `<passed>` number of each file.
The test fails when the measured number is not equal to the number in the baseline.
A change that makes more records pass must set the new number in the baseline.
The number must not go down.

`SLT_FAILURES=20 cargo test -- --nocapture` also prints the first 20 records that fail in each file.

### Test data

`tests/slt/random/expr/slt_good_0.test` is the file `test/random/expr/slt_good_0.test` from https://github.com/gregrahn/sqllogictest at commit `c93ac416f05635ff5cc490009add936d9753d01d`.
That repository is a git mirror of https://www.sqlite.org/sqllogictest/.
The sha256 of the file is `90af1bebe0f8320c0221907bd7bcf1942c5bdf9ba9cf95642fe851136b9db42c`.
The licence of the file is in `tests/slt/COPYRIGHT.md`.
The author offers GPL, BSD, MIT or CC0, and this repo uses the file under MIT.

## Status

The engine runs `SELECT [ALL | DISTINCT] <item> {, <item>}` without `FROM`.
An item is an expression with an optional alias.
An expression can use integer literals, unary `+` and `-`, binary `+ - * / %` and parentheses.
The arithmetic follows SQLite.
Division truncates toward zero.
The result of `%` has the sign of the left operand.
Division or modulo by zero gives NULL.
The engine returns an error for each other statement or expression, and for integer overflow.
A SQLite keyword is not accepted as an alias.
An expression that is nested more than 100 levels deep is an error.
Score: 4577 of 10012 records pass in `random/expr/slt_good_0.test`.
