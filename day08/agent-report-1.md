# Report: ticket #45, real numbers

## 1. What changed

- `src/select.rs`
  - New internal type `Num { Int(i64), Real(f64) }`. `Node.value` is now `Option<Num>`. A real value is always finite.
  - `CAST ( <expr> AS REAL )` added. `CAST ( <expr> AS INTEGER )` now truncates a real toward zero and clamps at the two integer limits.
  - `AVG` added as an aggregate, with `ALL` and `DISTINCT`, on the same code path as `SUM` (same "misuse of aggregate function" rule, same argument count errors).
  - `+ - * /` use real arithmetic when one operand is real. `%` casts both operands to INTEGER, and its result is real when one operand is real.
  - Unary minus on a real is `0.0 - x`, because SQLite gives `0.0` for `- CAST(0 AS REAL)`, not a negative zero.
  - Comparisons, `NULLIF` and `CASE <base>` compare an integer and a real by exact value, with no conversion of the integer to a real first.
  - Truth of a value (`NOT`, `AND`, `OR`, `CASE WHEN`) is "not zero" for both kinds, so 0.5 is true.
  - A real result that is not finite gives the error `the result is not a finite number`.
  - 12 new unit tests. Two old tests changed: `AVG` and `CAST(1 AS REAL)` left the error lists; `TOTAL`, `CAST(1 AS FLOAT)`, `CAST(1 AS DOUBLE)` and three malformed `REAL` casts joined them.
- `tests/slt/baseline.txt`: 9644 to 9986.
- `README.md`: Status section and score.

Each copied rule names its source in a code comment: the SQLite document quote from the ticket, or the `yt sqlite` query and its answer (SQLite 3.46.1).

## 2. Acceptance criteria

All SQLite answers below are from `yt sqlite .yt/q.sql`, SQLite 3.46.1. Each is also an assertion in a unit test, and `yt check` passed (61 unit tests, 11 harness tests).

| Criterion | Query or check | Result seen |
|---|---|---|
| `CAST(<expr> AS REAL)` gives a real, NULL for NULL | `SELECT CAST(1 AS REAL), typeof(CAST(1 AS REAL)), CAST(NULL AS REAL), CAST(- 5 AS REAL)` | SQLite: `1.0, 'real', NULL, -5.0`. Engine: same, test `cast_to_real_gives_a_real_value` |
| `AVG`, `AVG(ALL)`, `AVG(DISTINCT)` give a real, NULL with no non-NULL input | `SELECT AVG(3), typeof(AVG(3)), AVG(ALL 4), AVG(DISTINCT 5), AVG(NULL), AVG(DISTINCT NULL), AVG(9223372036854775807)` | SQLite: `3.0, 'real', 4.0, 5.0, NULL, NULL, 9.223372036854776e+18`. Engine: same |
| Aggregate inside aggregate, same as `SUM` | `SELECT AVG(COUNT(*))`, `SELECT MAX(AVG(1))`, `SELECT AVG(AVG(1))` | SQLite: `misuse of aggregate function COUNT()` / `AVG()` / `AVG()`. Engine: same error without the `()`, as for `SUM` today |
| `+ - * /` real when one operand is real | `SELECT CAST(7 AS REAL) + 1, 1 - CAST(7 AS REAL), 2 * CAST(7 AS REAL), 7 / CAST(2 AS REAL)` | SQLite: `8.0, -6.0, 14.0, 3.5`, `typeof` is `real` for each. Engine: same |
| `%` casts both operands to INTEGER | `SELECT CAST(7 AS REAL) % 2, CAST(- 7 AS REAL) % 2, (CAST(15 AS REAL) / 2) % 2, 7 % (CAST(5 AS REAL) / 2), 7 % CAST(- 1 AS REAL)` | SQLite: `1.0, -1.0, 1.0, 1.0, 0.0`, type `real`. Engine: same |
| Unary minus keeps a real value real | `SELECT - CAST(7 AS REAL), - - CAST(7 AS REAL), - CAST(0 AS REAL), - AVG(3)` | SQLite: `-7.0, 7.0, 0.0, -3.0`. Engine: same, and the zero is positive |
| Real divided by zero | `SELECT CAST(1 AS REAL) / 0, 1 / CAST(0 AS REAL), CAST(1 AS REAL) / CAST(0 AS REAL), CAST(0 AS REAL) / 0, CAST(1 AS REAL) / - CAST(0 AS REAL), CAST(- 1 AS REAL) / 0` | SQLite: six NULLs. Engine: same |
| `%` with a zero right operand | `SELECT CAST(1 AS REAL) % 0, 1 % CAST(0 AS REAL), CAST(0 AS REAL) % 0, 5 % (0 * CAST(1 AS REAL)), AVG(5) % 0, 5 % AVG(0), 7 % (CAST(1 AS REAL) / 2)` | SQLite: all NULL. The last one is zero only after the cast. Engine: same |
| Comparisons accept reals | `SELECT CAST(1 AS REAL) = 1, CAST(3 AS REAL) / 2 > 1, 9223372036854775807 = CAST(9223372036854775807 AS REAL), 9007199254740993 > CAST(9007199254740993 AS REAL)` | SQLite: `1, 1, 0, 1`. Engine: same |
| `CASE` accepts reals | `SELECT CASE WHEN CAST(1 AS REAL) / 2 THEN 1 ELSE 2 END, CASE 1 WHEN CAST(1 AS REAL) THEN 5 ELSE 6 END, CASE CAST(3 AS REAL) / 2 WHEN 1 THEN 5 ELSE 6 END` | SQLite: `1, 5, 6`. Engine: same |
| `COALESCE`, `NULLIF` accept reals | `SELECT COALESCE(CAST(NULL AS REAL), 2), COALESCE(CAST(1 AS REAL), 2), NULLIF(1, CAST(1 AS REAL)), NULLIF(CAST(1 AS REAL), 2), NULLIF(1, CAST(3 AS REAL) / 2)` | SQLite: `2, 1.0, NULL, 1.0, 1`. Engine: same |
| `CAST(... AS INTEGER)` accepts reals | `SELECT CAST(CAST(7 AS REAL) / 2 AS INTEGER), CAST(CAST(- 7 AS REAL) / 2 AS INTEGER), CAST(CAST(9223372036854775807 AS REAL) * 2 AS INTEGER), CAST(CAST(9223372036854775807 AS REAL) * - 2 AS INTEGER)` | SQLite: `3, -3, 9223372036854775807, -9223372036854775808`. Engine: same |
| `COUNT`, `MIN`, `MAX`, `SUM` accept reals | `SELECT COUNT(CAST(0 AS REAL)), MIN(CAST(3 AS REAL)), MAX(CAST(3 AS REAL) / 2), SUM(CAST(3 AS REAL)), SUM(CAST(NULL AS REAL))` | SQLite: `1, 3.0, 1.5, 3.0, NULL`. Engine: same |
| A real is true when not zero | `SELECT NOT CAST(1 AS REAL) / 2, CAST(1 AS REAL) / 2 AND 1, CAST(0 AS REAL) OR 0, NOT - CAST(0 AS REAL)` | SQLite: `0, 1, 0, 1`, type `integer`. Engine: same |
| Integer overflow stays an error | `SELECT 9223372036854775807 + 1`, `SELECT CAST(1 AS REAL) + (9223372036854775807 + 1)` | SQLite: `9.223372036854776e+18` for both. Engine: error, by ticket #2 |
| A real literal stays an error | `SELECT 1.5`, `SELECT CAST(1 AS REAL) + 1.5` | SQLite: `1.5`, `2.5`. Engine: error |
| A non-finite result is an error | Product of 16 and of 17 times `CAST(9223372036854775807 AS REAL)` | SQLite: `2.7430620343968443e+303`, then `inf`. Engine: the same number for 16, error for 17 |
| Baseline has the new exact count, number goes up | `yt check` | `slt: random/expr/slt_good_0.test 9986/10012`, up from 9644 |
| No record that passed before fails now | `yt failures 30` | 26 failures left, which is 368 minus 342. All 26 are errors for syntax the engine never had: `CREATE`, `INSERT`, `IN`, `BETWEEN`. I did not compare a per-record pass map before and after |

## 3. Known differences from SQLite

- Integer overflow is an error. SQLite gives a real number. Decision of ticket #2.
- A real result that is not finite is an error. SQLite gives `inf`. Decision of this ticket.
- Real literals (`1.5`, `1e2`, `.5`) are errors. SQLite accepts them.
- `CAST` to `FLOAT`, `DOUBLE` or any other type name is an error. SQLite gives `1.0` for `CAST(1 AS FLOAT)` and `CAST(1 AS DOUBLE)`.
- The "misuse of aggregate function" message has no `()` after the name. Same as the existing messages.
- `AVG()` and `AVG(*)`: SQLite says `wrong number of arguments to function AVG()`. The engine says `unsupported syntax at ...`, as it does for `SUM()` and `SUM(*)`.
- Each argument and each `CASE` part is still computed, so an error in a part SQLite skips is an error here. Unchanged.

Not measured:
- The expression tree height of `AVG` and of `CAST(... AS REAL)`. They use the same code path as `SUM` and `CAST(... AS INTEGER)`, whose heights were measured earlier.
- Whether SQLite gives a negative zero for `CAST(0 AS REAL) * - 1`. The engine does the IEEE multiplication. The harness renders both zeros as `0`.

## 4. Text for new tickets

- **Real literals.** `1.5`, `.5`, `1e5` in the lexer and parser. With them, the `R` column type and real rendering start to matter.
- **`CAST` to other type names.** SQLite accepts `FLOAT`, `DOUBLE`, `INT`, `NUMERIC`, `TEXT` and more by affinity rules. `FLOAT` and `DOUBLE` are now cheap, since they are the same as `REAL`.
- **`BETWEEN` and `NOT BETWEEN`.** 8 of the 26 failing records stop there.
- **`IN (...)` and `NOT IN (...)`.** 6 of the 26 failing records stop there.
- **`CREATE TABLE` and `INSERT`.** The other 12 failing records.
- **`TOTAL`.** The other aggregate of SQLite that always gives a real. Still an error.

## 5. Review round

Not a review round. No findings file was given.
