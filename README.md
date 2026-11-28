# lekha

A small SQL database that gives the same answers as SQLite.
"Lekha" is Sanskrit for a written record.

AI coding agents write this code.
SQLite's public test suite scores it, and Qodo reviews each change before it merges.

This repo is an entry in the Qodo Software Factory Hackathon.
It holds two things: the database, and the factory around it.
The factory is the working process that takes a ticket to a reviewed pull request: the tickets, the rules for the agents, the checks, the review and the gates.
The parts of the factory are added to this repo one at a time.

## Rule of this repo

The first commit is a seed made by hand: an empty crate, CI and the licence.
Each change after the seed arrives as a pull request with a Qodo review.

## Design decisions

- Language: Rust, with no dependencies.
- Answers: the same as SQLite, checked with SQLite's public test suite.
- When the engine does not support an input, it returns an error. It never returns a guessed value.

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
An expression can use integer literals, `NULL`, unary `+` and `-`, binary `+ - * / %`, parentheses, `COALESCE` and `NULLIF`.
The arithmetic follows SQLite.
Division truncates toward zero.
The result of `%` has the sign of the left operand.
Division or modulo by zero gives NULL.
Arithmetic with a NULL operand gives NULL.
`COALESCE` takes two or more arguments and returns the first one that is not NULL, or NULL when all are NULL.
`NULLIF` takes two arguments and returns NULL when they are equal, and the first argument in each other case.
Function names are case-insensitive.
The engine computes each argument of a function, so an error in an argument is an error for the statement.
The engine returns an error for each other statement, expression or function, for a wrong number of arguments and for integer overflow.
A SQLite keyword is not accepted as an alias.
An expression that is nested more than 100 levels deep is an error.
Score: 5021 of 10012 records pass in `random/expr/slt_good_0.test`.

## Rules and checks

`AGENTS.md` has the rules for the agents, and each rule says which check enforces it.
`tests/slt/FORMAT.wiki` is the format document of sqllogictest, copied without change from the same source as the test file.
`tests/slt/SHA256SUMS` has the checksum of each test file.
`ci/guard.sh` runs in CI before the build and checks the test data, the dependencies, and that a pull request does not change the engine and its measure in one change.
