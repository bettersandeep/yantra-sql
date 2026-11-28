# Report: ticket #45, review round

## 1. What changed

Only `src/lib.rs`.

- The digit choice of `real_text` moved into a new private function, `text_digits`. It returns 18 digits, or all 767 digits when the 18 digits end in `500`. `real_text` calls it; the output text is unchanged.
- New test `real_text_formats_18_digits_when_they_are_enough`. It asserts the digit count that `text_digits` returns: 18 for 11 values (`1.0`, `3.5`, `1/3`, `f64::MAX`, the smallest numbers, neighbours of a half) and 767 for 3 values at a half of the 15th digit.

No engine behaviour, baseline, `server/` or `site/` file changed in this round.

## 2. Acceptance criteria

This round changed no behaviour, so I ran only the full check, not the per-criterion queries.

| Criterion | Check that I ran | Result that I saw |
|---|---|---|
| All engine criteria (CAST to REAL, AVG, arithmetic, zero division, comparisons and functions, overflow, real literal, non-finite result) | `yt check` | 59 unit tests pass, including the existing real-value tests in `src/select.rs` |
| Real text is SQLite's `CAST ( <real> AS TEXT )` | `yt check`: `real_text_is_the_text_of_sqlite`, `real_text_of_18_digits_is_the_text_of_the_exact_number` | both pass |
| Baseline is the exact count and went up | `yt check`: `baseline_must_match_exactly`, `slt` | `slt: random/expr/slt_good_0.test 9986/10012`, baseline matches |
| No record that passed before fails now | `yt check` | count unchanged at 9986 in this round |
| `fmt` and `clippy` | `yt check` | clean |
| Tests of `server/` and `site/wasm/` | not run; `yt check` does not run them | not seen. This round touched neither directory |

## 3. Known differences from SQLite

None new in this round. `yt sqlite` was not called in this round, because no SQLite rule was added or changed.

## 4. Text for new tickets

None from this round.

## 5. Findings

**Finding 1: Review fix has no test that fails first — fixed.**

The finding is real: the earlier test compared `real_text` with the exact-digit path, so it passed on both the slow and the fast code.

The new test observes which path ran, through the digit count. I checked that it fails without the fix:

- I changed `text_digits` so each number took the 767-digit path, and ran `yt check`.
- Result: `real_text_formats_18_digits_when_they_are_enough ... FAILED`, with `left: 767, right: 18` for `1e0`. The other 58 tests passed, which confirms the old tests gave no signal.
- I restored the condition and ran `yt check` again: 59 passed, `slt` 9986/10012.

One limit: the test fails on the slow path only as written in `text_digits`. The commit before this round has no `text_digits`, so the test does not compile there instead of failing with an assertion. Getting a test that could observe the path needed this small refactor.
