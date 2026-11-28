# Report: review round 1, ticket #45

## 1. What changed

Only `src/lib.rs` changed in this round.

- `real_text` now formats 18 significant digits first, not 767.
- It asks for the exact 767 digits only when digits 16 to 18 are `500`. That is the one case where the 18 rounded digits and the exact number can round to a different 15th digit.
- The function is split into `decimal_digits` (digits and exponent at a given precision) and `rounded_text` (the rounding to 15 digits and the layout). The rounding and layout code is the same as before.
- New test `real_text_of_18_digits_is_the_text_of_the_exact_number` compares `real_text` with the exact-digit path, for both signs, on:
  - 200 random 16-digit numbers that end in 5, each scaled up and down by 10^0 to 10^23, plus the next value above and below each one;
  - 5,000 random bit patterns;
  - `f64::MIN_POSITIVE`, the smallest subnormal and `f64::MAX`.

The output of `real_text` did not change: the 65 SQLite-measured texts in `real_text_is_the_text_of_sqlite` still pass without edits.

## 2. Acceptance criteria

This round ran `yt check` twice and `yt status` once. It ran no `yt sqlite` query, and I did not measure the speed of `real_text` before or after.

| Criterion | Check run in this round | Result seen |
|---|---|---|
| `CAST ( <expr> AS REAL )` gives a real, NULL for NULL | `yt check`: `cast_to_real_gives_a_real_value` | ok |
| `AVG` in its three forms; nested aggregate rules as `SUM` | `yt check`: `avg_gives_a_real_value`, `aggregate_inside_an_aggregate_is_an_error` | ok |
| `+ - * /` real with one real operand, `%` casts to INTEGER, unary minus keeps real | `yt check`: `arithmetic_with_a_real_operand_is_real` | ok |
| Real divided by zero and `%` with zero as SQLite | `yt check`: `real_division_by_zero_and_modulo`. No SQLite query in this round | ok |
| Comparisons, `CASE`, `COALESCE`, `NULLIF`, `CAST AS INTEGER`, `COUNT`, `MIN`, `MAX`, `SUM` with reals | `yt check`: `comparison_with_a_real_operand`, `boolean_operators_and_case_with_real_values`, `functions_with_real_values` | ok |
| Integer overflow stays an error | `yt check`: `overflow_is_an_error` | ok |
| A real literal stays an error | `yt check`: `real_values_that_stay_errors` | ok |
| Server and page print a real value as SQLite's `CAST AS TEXT` | `yt check`: `real_text_is_the_text_of_sqlite` (65 measured texts) and the new comparison test. The tests of `server/` and `site/wasm/` were not run; `yt check` does not run them | engine tests ok; server and wasm tests not run |
| Page example list (`site/index.html`) | Not checked in this round, file not touched | not run |
| Tests of `server/` and `site/wasm/` change with the code | Not touched in this round | not run |
| A non-finite result is an error | `yt check`: `real_values_that_stay_errors`; `real_text` returns `None` for infinity and NaN | ok |
| `tests/slt/baseline.txt` has the exact count | `yt check`: `baseline_must_match_exactly` | ok, `slt: random/expr/slt_good_0.test 9986/10012` |
| No record that passed before fails now | `yt check` prints 9986/10012, the same as the baseline | count unchanged in this round; no per-record comparison run |

## 3. Known differences from SQLite

- None new in this round.
- Integer overflow is an error here and a real number in SQLite (decision of ticket #2).
- SQLite prints `Inf` for an infinity; this engine returns an error for a non-finite result.

## 4. Text for new tickets

Nothing new from this round.

## 5. Findings

**Finding 1, "Real-heavy queries format unused digits": fixed.**

- The finding is real: each finite nonzero value was formatted to 767 digits and 15 were kept.
- Now the common path formats 18 digits. The 767-digit path stays as a fallback for the `500` case, which includes every exact half (for example `CAST(1000000000000025 AS REAL)`).
- Why 18 digits are enough: the 18-digit text is the nearest 18-digit number to the exact one, and every half of the 15th digit is itself an 18-digit number. So the two can only land on different sides of a half when the 18-digit text is that half.
- The brief asks for a test that fails without the fix. This fix changes speed and not output, so no test can fail on the old code. The new test guards the other direction: it fails if the 18-digit path ever gives a different text from the exact path.
