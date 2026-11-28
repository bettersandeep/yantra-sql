# Report: review round 1, ticket #45

## 1. What changed

Nothing in this round. The one finding is true as a description, but its fix is outside the `Scope:` line, so I changed no file and a person has to decide.

The changes from the first start are still uncommitted: `README.md`, `src/select.rs`, `tests/slt/baseline.txt` (from `yt status`).

## 2. Acceptance criteria

In this round I ran only `yt check`, `yt status` and `yt diff`. I did not run `yt sqlite` or `yt failures`, and the SQLite measurements for the zero-division criterion are not repeated here.

`yt check` passed: fmt and clippy clean, 61 unit tests and 11 harness tests ok, `slt: random/expr/slt_good_0.test 9986/10012`.

| Criterion | Check run this round | Result seen |
|---|---|---|
| `CAST ( <expr> AS REAL )` gives a real, NULL for NULL | `cast_to_real_gives_a_real_value` | ok |
| `AVG` in its three forms gives a real, NULL with no non-NULL input, nesting rules as `SUM` | `avg_of_one_value_is_a_real_value`, `aggregates_with_real_values`, `aggregate_inside_an_aggregate_is_an_error` | ok |
| `+ - * /` real with a real operand, `%` casts to INTEGER, unary minus keeps real | `arithmetic_with_a_real_operand_is_real`, `modulo_casts_both_operands_to_integer` | ok |
| Real divided by zero and `%` by zero as SQLite | `real_division_and_modulo_by_zero_give_null` | ok; `yt sqlite` not run this round |
| Comparisons, `CASE`, `COALESCE`, `NULLIF`, `CAST AS INTEGER`, `COUNT`, `MIN`, `MAX`, `SUM` accept reals; real is true when not zero | `comparison_with_real_values`, `case_with_real_values`, `coalesce_and_nullif_with_real_values`, `cast_of_a_real_value_to_integer_truncates`, `aggregates_with_real_values`, `real_value_is_true_when_it_is_not_zero` | ok |
| Integer overflow stays an error | `overflow_is_an_error` | ok |
| A real literal stays an error | `overflow_real_literal_and_not_finite_are_errors` | ok |
| A non-finite result is an error | same test | ok |
| Baseline has the new exact count, and it goes up | `slt` test against `tests/slt/baseline.txt` | 9986/10012, baseline reads 9986. The ticket's 368 failing records imply 9644 before; I did not re-measure the old number this round |
| No record that passed before fails now | not checked per record this round | only the total was seen: 9986 |

## 3. Known differences from SQLite

- Integer overflow is an error. SQLite changes to a real number (decision of ticket #2).
- A non-finite real result is an error. SQLite gives `Inf` (ticket criterion).
- A real literal such as `1.5` is an error (ticket criterion).
- The HTTP server and the browser playground answer `ok:false` for any row with a real value. See part 5.

## 4. Text for new tickets

**Server and playground: print a real value**

- `result_json` in `server/src/lib.rs` (line 170) and `answer` in `site/wasm/src/lib.rs` (line 70) return an error for `Value::Real`. Both were written when the engine returned no real value, and both carry a comment that says not to guess the format.
- Since ticket #45, `SELECT AVG(1)` and `SELECT CAST(1 AS REAL)` return `Value::Real`, so both clients answer `{"ok":false,...}` for a query that the engine ran correctly.
- To build: both writers print a real value as SQLite prints it. Measure the text form with the SQLite tool first (for example `1.0`, `0.5`, `1.0e+19`, `-0.0`). Rule 5 applies.
- The server test at `server/src/lib.rs` line 555 asserts today's error text and must change with it.
- Scope: `server/src/lib.rs`, `site/wasm/src/lib.rs`, and `server/README.md` if it describes the value format.

## 5. Findings

**Finding 1: Real queries fail in both SQL clients.** Not fixed, because the fix is outside the scope of this ticket.

- The facts are correct. `select()` now returns `Value::Real` at `src/select.rs:878`, and both JSON writers turn that into an error answer.
- The two writers are in `server/src/lib.rs` and `site/wasm/src/lib.rs`. The scope is `src/`, `tests/slt/baseline.txt` and `README.md`, and one path outside it fails the run.
- There is no honest fix inside `src/`. The acceptance criteria require a real value from `CAST ... AS REAL` and `AVG`, so returning an integer or an error there would break the ticket.
- What the clients do today is an error, not a wrong value, so Rule 1 holds. The cost is that the two clients cannot show a result of the new feature until the ticket in part 4 is done.
- The correct format needs a measurement of how SQLite prints a real number, which is its own piece of work.

Decision for the person: merge #45 with this limit and open the ticket in part 4, or widen the scope of #45 to include the two client files.
