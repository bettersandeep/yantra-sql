# Report: ticket #45, review round 1

## 1. What changed

Nothing in this round. The one finding is real, but its fix is in `site/index.html`, and the `Scope:` line covers only `site/wasm/` under `site/`. An edit there fails the run, so a person has to decide.

The uncommitted work from the first start is unchanged (`yt diff`):

- `src/select.rs`: a value is an integer, a real number or NULL. Adds `CAST(<expr> AS REAL)`, `AVG`, real arithmetic, exact integer/real comparison, real truth values, and an error for a result that is not finite.
- `src/lib.rs`: `lekha::real_text`, the text of a real as SQLite gives it for `CAST(<real> AS TEXT)`.
- `server/src/lib.rs`, `site/wasm/src/lib.rs`: print a real value with `real_text`, and their tests changed with the code.
- `README.md`, `server/README.md`: the new behaviour and score.
- `tests/slt/baseline.txt`: 9644 to 9986.

## 2. Acceptance criteria

In this round I ran only `yt check` and `yt diff`. I did not run `yt sqlite` in this round. The SQLite values in the tests and comments are the ones recorded in the code by the first start (SQLite 3.46.1).

`yt check` result: fmt and clippy clean, 58 unit tests pass, 11 harness tests pass, `slt: random/expr/slt_good_0.test 9986/10012`.

| Criterion | Check run this round | Result seen |
|---|---|---|
| `CAST(<expr> AS REAL)` gives a real, NULL for NULL | `yt check`: `cast_to_real_gives_a_real_value` | ok |
| `AVG`, `AVG(ALL)`, `AVG(DISTINCT)` real or NULL, nesting rules as `SUM` | `yt check`: `avg_gives_a_real_value_or_null`, `aggregate_inside_an_aggregate_is_an_error` | ok |
| `+ - * /` real with one real operand, `%` casts to INTEGER, unary minus keeps real | `yt check`: `arithmetic_with_a_real_operand_is_real` | ok |
| Real divided by zero and `%` by zero as SQLite | `yt check`: `real_division_and_modulo_by_zero_give_null` (12 queries, all NULL) | ok. Not measured again with `yt sqlite` this round |
| Comparisons, `CASE`, `COALESCE`, `NULLIF`, `CAST AS INTEGER`, `COUNT`, `MIN`, `MAX`, `SUM` with reals, truth of a real | `yt check`: `comparison_with_a_real_operand`, `real_value_is_true_when_it_is_not_zero`, `functions_and_casts_with_a_real_value` | ok |
| Integer overflow stays an error | `yt check`: `integer_overflow_stays_an_error`, `overflow_is_an_error` | ok |
| Real literal stays an error | `yt check`: `integer_overflow_stays_an_error` (`1.5`, `1.0`, `.5`, `1e5`) | ok |
| Server and page print a real with SQLite's text, each value in a test | `yt check`: `real_text_is_the_text_of_sqlite` (53 values) | ok for `real_text`. The tests in `server/` and `site/wasm/` were not run, `yt check` does not run them |
| Tests of `server/` and `site/wasm/` change with the code | Read the diff | Changed. Not run, CI runs them |
| A result that is not finite is an error | `yt check`: `result_that_is_not_finite_is_an_error` | ok |
| Baseline has the new exact count, number goes up | `yt check`: `baseline_must_match_exactly`, file reads 9986 | ok, 9644 to 9986 |
| No record that passed before fails now | Not run this round. `yt check` gives only the total | 9986 of 10012 pass, 26 fail. No per-record comparison was made in this round |

## 3. Known differences from SQLite

- Integer overflow is an error here. SQLite changes to a real number (decision of ticket #2).
- A real result that is not finite is an error here. SQLite 3.46.1 gives the text `Inf` for a product of 17 factors `CAST(9223372036854775807 AS REAL)`.
- `real_text` rounds the exact decimal value to 15 digits. SQLite rounds its own value of about 19 digits, so the last digit can differ for a number whose 16th and later digits are very near `5000...`. No measured value shows a difference.
- `CAST(1 AS FLOAT)`, `DOUBLE`, `REAL(5)` and `REALS` are errors here. SQLite gives 1.0 for each.
- Each part of `CASE` and each argument of `COALESCE` is computed, so an error in a part that SQLite skips is an error here (unchanged from before).
- The error text for an aggregate inside an aggregate is `misuse of aggregate function AVG`. SQLite's text ends with `AVG()`.

## 4. Text for new tickets

**Playground: `SELECT AVG(1)` is listed under "Examples that are not built yet"**

- `site/index.html` line 46 has the button `SELECT AVG(1)` under the heading "Examples that are not built yet". After ticket #45 that query gives `1.0`.
- Remove it from that list. `SELECT 1 FROM t` still gives an error and can stay.
- Add a working example with a real value to "Examples that work", for example `SELECT AVG(3), CAST(7 AS REAL) / 2`.
- If the "not built yet" list needs a second entry, `SELECT 1.5` and `SELECT TOTAL(1)` are errors today (both are in the engine's tests).
- Scope: `site/index.html`. Best merged together with #45, otherwise the page is wrong from the merge of #45 until this ticket.

## 5. Findings

**Finding 1, "Playground mislabels a working example": not fixed, because the fix is outside the scope of the ticket.**

- The finding is correct. I checked it against the code: `site/index.html:46` lists `SELECT AVG(1)` as not built, and `src/select.rs` now answers it with the real value 1.0 (test `avg_gives_a_real_value_or_null`).
- The only fix is an edit of `site/index.html`. The scope has `site/wasm/` and no other path under `site/`, and one path outside the scope fails the run.
- No change inside the scope removes the finding without breaking the ticket: `AVG` has to work.
- Decision for a person: widen the scope of #45 to `site/index.html` and restart, or merge with the new ticket from part 4 right behind it. I recommend widening the scope, because it is a two-line change in the page and keeps the page true at each commit on `main`.
