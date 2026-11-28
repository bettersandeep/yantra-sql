# Report: ticket #45, real numbers

## 1. What changed

- `src/select.rs`: a value is now `Num::Int` or `Num::Real` instead of an integer.
  - `CAST ( <expr> AS REAL )` and `AVG` (with `ALL` / `DISTINCT`) are built.
  - `+ - * /` are real when one operand is real; `%` casts both operands to INTEGER and gives a real result when an operand is real.
  - Comparisons, `NULLIF` and `CASE <base>` compare an integer and a real by exact value, not through a lossy conversion.
  - A real result that is not finite is the error `the result is not a finite number`.
- `src/lib.rs`: new public `lekha::real_text(f64) -> Option<String>`, the text SQLite gives for `CAST ( <real> AS TEXT )`. It returns `None` for a non-finite value.
- `server/src/lib.rs` and `site/wasm/src/lib.rs`: a real value prints through `lekha::real_text`. The old error-text test in `server/` is changed, and both crates got a test with measured texts.
- `site/index.html`: `SELECT AVG(1)` is removed from "Examples that are not built yet"; `SELECT AVG(7), CAST(7 AS REAL) / 2, CAST(1 AS REAL) / 3` is added to the working examples.
- `tests/slt/baseline.txt`: 9644 to 9986.
- `README.md` and `server/README.md`: status text and score updated.

Existing engine tests that expected an error for `CAST(1 AS REAL)` and `AVG(...)` were changed; the "unsupported function" checks now use `TOTAL`.

## 2. Acceptance criteria

All SQLite answers are from `yt sqlite`, SQLite 3.46.1. Real texts were measured with `CAST ( ... AS TEXT )`.

| Criterion | Query or check | Result seen |
| --- | --- | --- |
| `CAST AS REAL` gives real, NULL for NULL | `typeof(CAST(1 AS REAL))`, `CAST(CAST(1 AS REAL) AS TEXT)`, `CAST(NULL AS REAL)` | SQLite: `real`, `1.0`, NULL. Engine test `cast_to_real_gives_a_real_value` passes. |
| `AVG`, `AVG(ALL)`, `AVG(DISTINCT)`; nesting rules as `SUM` | `typeof(AVG(1))`, `AVG(NULL)`, `AVG(ALL 3)`, `AVG(DISTINCT - 4)`, `SUM(AVG(1))`, `AVG(COUNT(*))` | SQLite: `real`, NULL, `3.0`, `-4.0`, "misuse of aggregate function AVG()", "... COUNT()". Test `avg_gives_a_real_value` passes. |
| `+ - * /` real with a real operand; `%` casts to INTEGER; unary minus stays real | `CAST(7 AS REAL) / 2`, `1 - CAST(3 AS REAL)`, `typeof(CAST(7 AS REAL) % 2)`, `(CAST(15 AS REAL) / 2) % 2`, `typeof(- CAST(1 AS REAL))` | SQLite: `3.5`, `-2.0`, `real`, `1.0`, `real`. Tests `arithmetic_with_a_real_operand_is_real` and `real_division_by_zero_and_modulo` pass. |
| Real divided by zero, `%` with zero right operand | `SELECT CAST(1 AS REAL) / 0, CAST(1 AS REAL) / CAST(0 AS REAL), 1 / CAST(0 AS REAL), CAST(0 AS REAL) / 0, CAST(0 AS REAL) / CAST(0 AS REAL), - CAST(1 AS REAL) / 0, CAST(1 AS REAL) / (0 * - CAST(1 AS REAL))` and `SELECT CAST(7 AS REAL) % 0, 7 % CAST(0 AS REAL), CAST(7 AS REAL) % CAST(0 AS REAL), 7 % (CAST(1 AS REAL) / 2), CAST(0 AS REAL) % 0, 7 % (- CAST(1 AS REAL) / 2)` | SQLite: NULL for every one. The engine gives the same. |
| Comparisons, `CASE`, `COALESCE`, `NULLIF`, `CAST AS INTEGER`, `COUNT`, `MIN`, `MAX`, `SUM`; real is true when not zero | `CAST(9223372036854775807 AS REAL) = 9223372036854775807` (0), `CAST(9007199254740993 AS REAL) < 9007199254740993` (1), `NULLIF(CAST(1 AS REAL), 1)` (NULL), `CASE WHEN CAST(1 AS REAL) / 2 THEN 1 ELSE 2 END` (1), `CAST(- CAST(7 AS REAL) / 2 AS INTEGER)` (-3), `CAST(CAST(9223372036854775807 AS REAL) * 4 AS INTEGER)` (9223372036854775807), `typeof(SUM(CAST(1 AS REAL)))` (real), `typeof(COUNT(CAST(1 AS REAL)))` (integer) | Tests `comparison_with_a_real_operand`, `boolean_operators_and_case_with_real_values`, `functions_with_real_values` pass with the measured values. |
| Integer overflow stays an error | `SELECT 9223372036854775807 + 1`, `CAST(9223372036854775807 + 1 AS REAL)`, `AVG(4611686018427387904 * 2)` | Error in each case (`real_values_that_stay_errors`, `overflow_is_an_error`). |
| Real literal stays an error | `SELECT 1.5`, `.5`, `1e5`, `CAST(1.5 AS REAL)` | Error in each case. |
| Server and page print a real value, with measured text in a test | `1.0`, `3.5`, `-3.5`, `0.333333333333333`, `9.22337203685478e+18`, `1.0e-05`, `0.0001`, `999999999999999.0`, `1.0e+15`, `1.00000000000003e+15`, `0.0` | 65 measured texts pass in the engine test `real_text_is_the_text_of_sqlite`. The `server/` and `site/wasm/` tests are written but **not run**: `yt check` does not run those crates. Their formatting is also unchecked by me. |
| Page lists no working query as "not built", and has one real example | Read `site/index.html` | `SELECT AVG(1)` removed; one real example added. Not run in a browser. |
| `server/` and `site/wasm/` tests that expected the old error change | Edited `query_results` in `server/src/lib.rs` | Not run. `site/wasm/` had no test with the old text. |
| Non-finite result is an error | Product of 17 × `CAST(9223372036854775807 AS REAL)` | SQLite: `Inf`. Engine: `the result is not a finite number`. Product of 16 gives `2.74306203439684e+303` in both. |
| Baseline has the new exact count, higher | `yt check` | `slt: random/expr/slt_good_0.test 9986/10012`, check green. |
| No record that passed before fails now | `yt failures 30` | 26 failures remain: 12 `CREATE`/`INSERT`, 14 `IN`/`BETWEEN`. 368 − 342 = 26, so all 342 real records pass and none regressed. |

## 3. Known differences from SQLite

- An infinity is an error here; SQLite keeps it (`Inf`).
- Integer overflow is an error here; SQLite switches to a real number (ticket #2).
- `CAST ( ... AS FLOAT )`, `AS DOUBLE`, `AS REAL(5)` and `AS REALS` give a real number in SQLite and an error here.
- Error text has no `()` after the function name: `misuse of aggregate function AVG` against SQLite's `... AVG()`. This is the existing convention.
- `real_text` rounds the exact decimal value half-up at 15 digits. All 65 measured values match, including exact ties. I did not copy SQLite's own digit algorithm, so a value extremely close to a tie without being one could differ in the last digit. I found no such case.
- `COALESCE` and `CASE` still compute every part, so a non-finite result in a part SQLite skips is an error here.

## 4. Text for new tickets

- Real literals (`1.5`, `.5`, `1e5`).
- `CAST` to `FLOAT`, `DOUBLE` and other type names with REAL affinity, and `CAST ( ... AS TEXT )`.
- `IN`, `NOT IN`, `BETWEEN`, `NOT BETWEEN`: 14 of the 26 remaining failures.
- `CREATE TABLE` and `INSERT`: the other 12.
- `yt check` does not run the `server/` and `site/wasm/` test sets, so an agent cannot verify changes there before CI.
- `TOTAL` and `GROUP_CONCAT` are still errors.

## 5. Review findings

Not a review round.
