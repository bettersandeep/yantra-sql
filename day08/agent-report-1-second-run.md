# Report: ticket #45, real numbers

## 1. What changed

- **`src/select.rs`**: a value is now an integer, a real number or NULL.
  - `CAST(<expr> AS REAL)` and `AVG` (with `ALL` or `DISTINCT`) are new.
  - `+ - * /` give a real result when one operand is real. `%` casts both operands to INTEGER and gives a real result. Unary minus keeps a real value real.
  - Comparisons, `NULLIF` and `CASE <base>` compare an integer with a real number by exact value.
  - `CAST(<real> AS INTEGER)` truncates toward zero and stops at the two integer limits.
  - A real result that is not finite is the error `the result is not a finite number`.
  - Integer overflow is still `integer overflow`, and the lexer is untouched, so `1.5` is still an error.
- **`src/lib.rs`**: new `pub fn real_text(f64) -> Option<String>`, the text SQLite gives for `CAST(<real> AS TEXT)`. It returns `None` for a value that is not finite.
- **`server/src/lib.rs`** and **`site/wasm/src/lib.rs`**: a real value is printed as a JSON string with `lekha::real_text`. The server test that expected `this server cannot print a real number yet` now expects `"1.5"`.
- **`README.md`**, **`server/README.md`**: status text and the score.
- **`tests/slt/baseline.txt`**: 9644 to 9986.

`yt check` passes: fmt, clippy, 58 unit tests, and the slt test at 9986/10012.

**Not run:** the tests of `server/` and `site/wasm/`. `yt check` does not run them, so the new tests there, and the formatting and clippy state of those two crates, are unverified until CI.

## 2. Acceptance criteria

All SQLite answers are from `yt sqlite`, SQLite 3.46.1. `R(x)` stands for `CAST(x AS REAL)`. The type of each result was measured with `typeof`, and the text with `CAST(... AS TEXT)`.

| Criterion | Query or check | Result seen |
|---|---|---|
| `CAST AS REAL` gives real, NULL for NULL | `typeof(R(1))`, `CAST(R(1) AS TEXT)`, `CAST(NULL AS REAL)` | SQLite: `real`, `1.0`, NULL. Test `cast_to_real_gives_a_real_value` passes |
| `AVG`, `AVG(ALL)`, `AVG(DISTINCT)` real, NULL without input, nesting as `SUM` | `typeof(AVG(1))`, `AVG(NULL)`, `CAST(AVG(DISTINCT 7) AS TEXT)`, `MAX(AVG(1))`, `AVG(COUNT(*))` | SQLite: `real`, NULL, `7.0`, two misuse errors. Test `avg_gives_a_real_value_or_null` passes |
| `+ - * /` real with a real operand, `%` casts to INTEGER, unary minus stays real | `R(1) + 1`, `1 - R(3)`, `2 * R(3)`, `7 / R(2)`, `R(7) / 2 % 2`, `-7 % (R(5) / 2)`, `- R(7)` | SQLite: `2.0`, `-2.0`, `6.0`, `3.5`, `1.0`, `-1.0`, `-7.0`, all `real`. Test `arithmetic_with_a_real_operand_is_real` passes |
| Real divided by zero, `%` with zero right operand | `R(1) / 0`, `R(1) / R(0)`, `1 / R(0)`, `R(0) / 0`, `- R(1) / 0`, `1 / (R(0) * -1)`, `R(1) % 0`, `R(1) % R(0)`, `1 % R(0)`, `5 % (R(1) / 2)`, `R(5) % (R(1) / 2)`, `5 % (- R(1) / 2)` | SQLite: NULL for all 12. Test `real_division_and_modulo_by_zero_give_null` passes |
| Comparisons, `CASE`, `COALESCE`, `NULLIF`, `CAST AS INTEGER`, `COUNT`, `MIN`, `MAX`, `SUM` accept reals, real is true when not zero | `9223372036854775807 = R(9223372036854775807)` (0), `NOT R(1) / 2` (0), `NULLIF(R(1), 1)` (NULL), `CAST(- R(7) / 2 AS INTEGER)` (-3), `typeof(SUM(R(1)))` (`real`), `typeof(COALESCE(1, R(2)))` (`integer`), and others | Tests `comparison_with_a_real_operand`, `real_value_is_true_when_it_is_not_zero`, `functions_and_casts_with_a_real_value` pass |
| Integer overflow stays an error | `9223372036854775807 + 1`, `CAST(9223372036854775807 + 1 AS REAL)`, `AVG(9223372036854775807 + 1)` and 3 more | SQLite gives `real` for each. Engine gives `integer overflow`. Test `integer_overflow_stays_an_error` passes |
| Real literal stays an error | `SELECT 1.5`, `1.0`, `.5`, `1e5` | Engine: error, same test |
| Server and page print a real value, each tested value measured | 10 values in each crate, 55 in `real_text_is_the_text_of_sqlite`, for example `1.0`, `3.5`, `0.333333333333333`, `1.0e-05`, `9.22337203685478e+18`, `1.0e+100` | The engine test passes. The server and wasm tests were **not run** |
| Server and wasm tests that expect the old error change | `query_results` in `server/src/lib.rs` | Changed, **not run**. `site/wasm` and `server/tests/http.rs` had no test with the old text |
| Not finite is an error | product of 17 factors `R(9223372036854775807)` | SQLite: text `Inf`. Engine: `the result is not a finite number`. 16 factors work. Test passes |
| Baseline has the new count, and it goes up | `yt check` | `slt: random/expr/slt_good_0.test 9986/10012`, baseline 9986 |
| No record that passed before fails now | `yt failures 30` | 26 records fail, all with `unsupported syntax` at `CREATE`, `INSERT`, `IN`, `BETWEEN` or `NOT`. 368 − 342 = 26, so the gain is the ticket's 342. This is by count and failure kind, not a per-record diff against the old run |

## 3. Known differences from SQLite

- A result that is not finite is an error. SQLite gives `Inf`.
- Integer overflow is an error. SQLite changes to a real number.
- `CAST(1 AS FLOAT)`, `DOUBLE`, `REAL(5)` and `REALS` are errors. SQLite gives `1.0` for each.
- A real literal is an error. SQLite gives `1.5`.
- Error texts differ: `misuse of aggregate function AVG` against SQLite's `... AVG()`, and `AVG()` or `AVG(*)` give `unsupported syntax ...` against SQLite's `wrong number of arguments to function AVG()`.
- `real_text` rounds the exact decimal value to 15 digits, half away from zero. SQLite rounds its own shorter decimal value, so the last digit can differ for a number whose digits from the 16th on are very near `5000...`. No measured value shows a difference, and the code comment says this.
- `- R(0)` is a negative zero inside the engine. I did not measure which zero SQLite holds. Nothing supported shows it: both print `0.0`, compare equal to 0 and divide to NULL.

## 4. Text for new tickets

- `IN` and `NOT IN` with a list: 6 of the 26 failing records.
- `BETWEEN` and `NOT BETWEEN`: 8 of the 26.
- `CREATE TABLE` and `INSERT`: 12 of the 26.
- Real literals such as `1.5` and `1e5`.
- `CAST` to the other type names that SQLite accepts.
- `yt check` does not run the tests, fmt or clippy of `server/` and `site/wasm/`, so an agent cannot verify a change there before CI.
- The slt harness could record which records passed, so "no record that passed before fails now" has a per-record check.

## 5. Review round

Not a review round.
