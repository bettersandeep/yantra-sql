//! Parses and evaluates `SELECT` without `FROM`.
//!
//! The parser computes each value while it reads the tokens.
//! `None` is the SQL NULL.
//! Each function argument is computed, so an error in an argument is an error for the statement.
//! SQLite 3.49.1 does not compute the arguments of `COALESCE` after the first one that is not NULL:
//! `SELECT COALESCE(1, abs(-9223372036854775807 - 1))` gives 1 there and an error here.
//! A `CAST` is not a function call. Its only supported target type is `INTEGER`.

use crate::lexer::{Token, lex};
use crate::{Error, Value};

/// The keywords of SQLite, from https://www.sqlite.org/lang_keywords.html.
/// A keyword is not accepted as an alias.
/// SQLite accepts some of them, but `ISNULL` and `NOTNULL` are operators that change the value.
#[rustfmt::skip]
const KEYWORDS: [&str; 147] = [
    "ABORT", "ACTION", "ADD", "AFTER", "ALL", "ALTER", "ALWAYS", "ANALYZE", "AND", "AS", "ASC",
    "ATTACH", "AUTOINCREMENT", "BEFORE", "BEGIN", "BETWEEN", "BY", "CASCADE", "CASE", "CAST",
    "CHECK", "COLLATE", "COLUMN", "COMMIT", "CONFLICT", "CONSTRAINT", "CREATE", "CROSS",
    "CURRENT", "CURRENT_DATE", "CURRENT_TIME", "CURRENT_TIMESTAMP", "DATABASE", "DEFAULT",
    "DEFERRABLE", "DEFERRED", "DELETE", "DESC", "DETACH", "DISTINCT", "DO", "DROP", "EACH",
    "ELSE", "END", "ESCAPE", "EXCEPT", "EXCLUDE", "EXCLUSIVE", "EXISTS", "EXPLAIN", "FAIL",
    "FILTER", "FIRST", "FOLLOWING", "FOR", "FOREIGN", "FROM", "FULL", "GENERATED", "GLOB",
    "GROUP", "GROUPS", "HAVING", "IF", "IGNORE", "IMMEDIATE", "IN", "INDEX", "INDEXED",
    "INITIALLY", "INNER", "INSERT", "INSTEAD", "INTERSECT", "INTO", "IS", "ISNULL", "JOIN", "KEY",
    "LAST", "LEFT", "LIKE", "LIMIT", "MATCH", "MATERIALIZED", "NATURAL", "NO", "NOT", "NOTHING",
    "NOTNULL", "NULL", "NULLS", "OF", "OFFSET", "ON", "OR", "ORDER", "OTHERS", "OUTER", "OVER",
    "PARTITION", "PLAN", "PRAGMA", "PRECEDING", "PRIMARY", "QUERY", "RAISE", "RANGE",
    "RECURSIVE", "REFERENCES", "REGEXP", "REINDEX", "RELEASE", "RENAME", "REPLACE", "RESTRICT",
    "RETURNING", "RIGHT", "ROLLBACK", "ROW", "ROWS", "SAVEPOINT", "SELECT", "SET", "TABLE",
    "TEMP", "TEMPORARY", "THEN", "TIES", "TO", "TRANSACTION", "TRIGGER", "UNBOUNDED", "UNION",
    "UNIQUE", "UPDATE", "USING", "VACUUM", "VALUES", "VIEW", "VIRTUAL", "WHEN", "WHERE",
    "WINDOW", "WITH", "WITHOUT",
];

/// ponytail: a fixed limit on nested parentheses and unary operators.
/// Deeper input is an error and not a stack overflow.
/// SQLite permits a depth of 1000. Raise the limit if real queries need it.
const MAX_DEPTH: usize = 100;

fn overflow() -> Error {
    Error("integer overflow".to_string())
}

/// Applies a binary operator with the integer rules of SQLite.
fn apply(op: Token, left: Option<i64>, right: Option<i64>) -> Result<Option<i64>, Error> {
    // Arithmetic with NULL gives NULL.
    let (Some(a), Some(b)) = (left, right) else {
        return Ok(None);
    };
    let result = match op {
        Token::Plus => a.checked_add(b),
        Token::Minus => a.checked_sub(b),
        Token::Star => a.checked_mul(b),
        // Division or modulo by zero gives NULL.
        Token::Slash | Token::Percent if b == 0 => return Ok(None),
        // The division truncates toward zero. Only `i64::MIN / -1` overflows.
        Token::Slash => a.checked_div(b),
        // The result has the sign of the left operand. `i64::MIN % -1` is 0.
        Token::Percent => Some(a.wrapping_rem(b)),
        _ => return Err(Error(format!("{op:?} is not a binary operator"))),
    };
    result.map(Some).ok_or_else(overflow)
}

struct Parser<'a> {
    tokens: Vec<Token<'a>>,
    pos: usize,
    /// The number of `unary` calls that are in progress.
    depth: usize,
    /// True while the parser reads the argument of an aggregate function.
    in_aggregate: bool,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<Token<'a>> {
        self.tokens.get(self.pos).copied()
    }

    /// The error for a token that the grammar does not permit here.
    fn unexpected(&self) -> Error {
        match self.peek() {
            Some(token) => Error(format!("unsupported syntax at {token:?}")),
            None => Error("unexpected end of the statement".to_string()),
        }
    }

    /// Consumes the next token if it is `keyword`.
    fn keyword(&mut self, keyword: &str) -> bool {
        let found = matches!(self.peek(), Some(Token::Word(w)) if w.eq_ignore_ascii_case(keyword));
        if found {
            self.pos += 1;
        }
        found
    }

    /// `term {(+ | -) term}`
    fn expr(&mut self) -> Result<Option<i64>, Error> {
        let mut left = self.term()?;
        while let Some(op @ (Token::Plus | Token::Minus)) = self.peek() {
            self.pos += 1;
            left = apply(op, left, self.term()?)?;
        }
        Ok(left)
    }

    /// `unary {(* | / | %) unary}`
    fn term(&mut self) -> Result<Option<i64>, Error> {
        let mut left = self.unary()?;
        while let Some(op @ (Token::Star | Token::Slash | Token::Percent)) = self.peek() {
            self.pos += 1;
            left = apply(op, left, self.unary()?)?;
        }
        Ok(left)
    }

    /// A function call. The parser is at the name, and the next token is `(`.
    /// `COALESCE ( expr , expr {, expr} )` and `NULLIF ( expr , expr )` are from
    /// https://www.sqlite.org/lang_corefunc.html.
    /// `COUNT ( * )` and `(COUNT | MIN | MAX | SUM) ( [ALL | DISTINCT] expr )` are from
    /// https://www.sqlite.org/lang_aggfunc.html.
    /// A SELECT without FROM has one row, so each aggregate sees one value.
    fn call(&mut self, name: &str) -> Result<Option<i64>, Error> {
        let function = name.to_ascii_uppercase();
        let aggregate = matches!(function.as_str(), "COUNT" | "MIN" | "MAX" | "SUM");
        if !aggregate && function != "COALESCE" && function != "NULLIF" {
            return Err(self.unexpected());
        }
        self.pos += 2;
        if aggregate {
            // SQLite 3.49.1 gives "misuse of aggregate function COUNT()" for `SELECT MAX(COUNT(*))`.
            if self.in_aggregate {
                return Err(Error(format!("misuse of aggregate function {name}")));
            }
            // The number of rows.
            if function == "COUNT"
                && self.tokens[self.pos..].starts_with(&[Token::Star, Token::RightParen])
            {
                self.pos += 2;
                return Ok(Some(1));
            }
            // One value has no duplicates, so DISTINCT changes nothing.
            if !self.keyword("ALL") {
                self.keyword("DISTINCT");
            }
            self.in_aggregate = true;
        }
        let mut args = Vec::new();
        loop {
            args.push(self.expr()?);
            match self.peek() {
                Some(Token::Comma) => self.pos += 1,
                Some(Token::RightParen) => break,
                _ => return Err(self.unexpected()),
            }
        }
        self.pos += 1;
        if aggregate {
            self.in_aggregate = false;
        }
        match (function.as_str(), &args[..]) {
            // The first argument that is not NULL, or NULL.
            ("COALESCE", [_, _, ..]) => Ok(args.iter().find_map(|arg| *arg)),
            // NULL if the arguments are the same, else the first argument.
            // If `a` is NULL, the two branches give the same result.
            // If only `b` is NULL, the arguments are different.
            ("NULLIF", [a, b]) => Ok(if a == b { None } else { *a }),
            // The number of values that are not NULL.
            ("COUNT", [arg]) => Ok(Some(i64::from(arg.is_some()))),
            // The minimum, the maximum and the sum of one value are that value.
            // Each one is NULL when there is no value that is not NULL.
            ("MIN" | "MAX" | "SUM", [arg]) => Ok(*arg),
            // With two or more arguments, these are scalar functions in SQLite.
            // See `max(X,Y,...)` in https://www.sqlite.org/lang_corefunc.html.
            ("MIN" | "MAX", [_, _, ..]) => Err(Error(format!(
                "the function {name} with more than one argument is not supported"
            ))),
            _ => Err(Error(format!(
                "wrong number of arguments to function {name}"
            ))),
        }
    }

    /// `CAST ( expr AS INTEGER )`. The parser is at `CAST`, and the next token is `(`.
    /// The syntax is from https://www.sqlite.org/lang_expr.html#castexpr.
    /// Each value of this engine is an integer or NULL.
    /// SQLite 3.46.1 gives `1, -5, NULL` for
    /// `SELECT CAST(1 AS INTEGER), CAST(- 5 AS INTEGER), CAST(NULL AS INTEGER)`,
    /// so the cast returns its argument without change.
    /// SQLite accepts each type name, and also no type name. Only `INTEGER` is supported here.
    fn cast(&mut self) -> Result<Option<i64>, Error> {
        self.pos += 2;
        let value = self.expr()?;
        if !self.keyword("AS") {
            return Err(self.unexpected());
        }
        match self.peek() {
            Some(Token::Word(name)) if name.eq_ignore_ascii_case("INTEGER") => self.pos += 1,
            Some(Token::Word(name)) => {
                return Err(Error(format!("CAST to {name} is not supported")));
            }
            _ => return Err(self.unexpected()),
        }
        if self.peek() != Some(Token::RightParen) {
            return Err(self.unexpected());
        }
        self.pos += 1;
        Ok(value)
    }

    /// `+ unary`, `- unary`, an integer literal, `NULL`, `( expr )`, a `CAST` or a function call.
    /// Each recursion of the parser goes through this function.
    fn unary(&mut self) -> Result<Option<i64>, Error> {
        if self.depth == MAX_DEPTH {
            return Err(Error("the expression is nested too deep".to_string()));
        }
        self.depth += 1;
        let value = match self.peek() {
            Some(Token::Plus) => {
                self.pos += 1;
                self.unary()?
            }
            Some(Token::Minus) => {
                self.pos += 1;
                match self.unary()? {
                    Some(v) => Some(v.checked_neg().ok_or_else(overflow)?),
                    None => None,
                }
            }
            Some(Token::Integer(v)) => {
                self.pos += 1;
                Some(v)
            }
            Some(Token::Word(word)) if word.eq_ignore_ascii_case("NULL") => {
                self.pos += 1;
                None
            }
            Some(Token::Word(name)) if self.tokens.get(self.pos + 1) == Some(&Token::LeftParen) => {
                if name.eq_ignore_ascii_case("CAST") {
                    self.cast()?
                } else {
                    self.call(name)?
                }
            }
            Some(Token::LeftParen) => {
                self.pos += 1;
                let v = self.expr()?;
                if self.peek() != Some(Token::RightParen) {
                    return Err(self.unexpected());
                }
                self.pos += 1;
                v
            }
            _ => return Err(self.unexpected()),
        };
        self.depth -= 1;
        Ok(value)
    }
}

/// Runs `SELECT [ALL | DISTINCT] <item> {, <item>}` and returns its one row.
/// An item is `<expr> [[AS] <alias>]`.
/// Returns an error for each other statement.
pub fn select(sql: &str) -> Result<Vec<Value>, Error> {
    let mut parser = Parser {
        tokens: lex(sql)?,
        pos: 0,
        depth: 0,
        in_aggregate: false,
    };
    if !parser.keyword("SELECT") {
        return Err(parser.unexpected());
    }
    // One row has no duplicates, so DISTINCT changes nothing.
    if !parser.keyword("ALL") {
        parser.keyword("DISTINCT");
    }
    let mut row = Vec::new();
    loop {
        row.push(parser.expr()?.map_or(Value::Null, Value::Integer));
        // The alias has no effect on the result.
        let has_as = parser.keyword("AS");
        match parser.peek() {
            Some(Token::Word(word)) if !KEYWORDS.iter().any(|k| k.eq_ignore_ascii_case(word)) => {
                parser.pos += 1;
            }
            _ if has_as => return Err(parser.unexpected()),
            _ => {}
        }
        if parser.peek() != Some(Token::Comma) {
            break;
        }
        parser.pos += 1;
    }
    if parser.pos < parser.tokens.len() {
        return Err(parser.unexpected());
    }
    Ok(row)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(sql: &str) -> Vec<Value> {
        select(sql).unwrap_or_else(|error| panic!("{sql}: {error}"))
    }

    fn ints(values: &[i64]) -> Vec<Value> {
        values.iter().map(|v| Value::Integer(*v)).collect()
    }

    #[test]
    fn precedence_and_left_associativity() {
        assert_eq!(run("SELECT 2 + 3 * 4, (2 + 3) * 4"), ints(&[14, 20]));
        assert_eq!(run("SELECT 10 - 4 - 3, 10 - (4 - 3)"), ints(&[3, 9]));
        assert_eq!(run("SELECT 100 / 5 / 2, 100 / (5 / 2)"), ints(&[10, 50]));
        assert_eq!(run("SELECT 7 - 2 * 3 % 4, 20 % 7 * 2"), ints(&[5, 12]));
        assert_eq!(run("SELECT - 2 * 3, 2 * - 3, - 2 + 3"), ints(&[-6, -6, 1]));
    }

    #[test]
    fn unary_chains() {
        assert_eq!(
            run("SELECT - - + 5, + + 5, - + - - 5, -(-(5)), 3 - - 2, 3 - -2, +(-0)"),
            ints(&[5, 5, -5, 5, 5, 5, 0])
        );
    }

    #[test]
    fn division_and_modulo_signs() {
        assert_eq!(
            run("SELECT 7 / 2, -7 / 2, 7 / -2, -7 / -2"),
            ints(&[3, -3, -3, 3])
        );
        assert_eq!(
            run("SELECT 7 % 3, -7 % 3, 7 % -3, -7 % -3"),
            ints(&[1, -1, 1, -1])
        );
    }

    #[test]
    fn zero_division_and_null() {
        assert_eq!(
            run("SELECT 1 / 0, 1 % 0, 0 / 0, 1 / 0 + 1, 2 * (3 % 0), 4 - 1 / 0, - (1 / 0), 0 / 5"),
            [
                Value::Null,
                Value::Null,
                Value::Null,
                Value::Null,
                Value::Null,
                Value::Null,
                Value::Null,
                Value::Integer(0),
            ]
        );
        assert_eq!(
            run("SELECT (1 / 0) / 0, 1 / 0 % 0"),
            [Value::Null, Value::Null]
        );
    }

    #[test]
    fn null_literal_and_arithmetic() {
        assert_eq!(
            run(
                "SELECT NULL, null, Null, - NULL, + NULL, - - nULL, (NULL), NULL + 1, 1 - NULL, \
                 NULL * 0, NULL / 0, 0 % NULL, NULL % NULL, 2 * (3 + NULL), NULL AS a, NULL b"
            ),
            [const { Value::Null }; 16]
        );
        for sql in [
            "SELECT NULL NULL",
            "SELECT 1 NULL",
            "SELECT 1 AS NULL",
            "SELECT NULL(1)",
            "SELECT NULL 1",
            // SQLite gives NULL. The overflow is a real number there.
            "SELECT NULL + (9223372036854775807 + 1)",
        ] {
            assert!(select(sql).is_err(), "no error: {sql}");
        }
    }

    #[test]
    fn coalesce_returns_the_first_value_that_is_not_null() {
        assert_eq!(
            run(
                "SELECT COALESCE(NULL, 2), COALESCE(1, 2), COALESCE(0, 1), COALESCE(NULL, NULL, 3), \
                 COALESCE(1 / 0, 7 % 0, - 9, 4), COALESCE(NULL, COALESCE(NULL, 5), 6)"
            ),
            ints(&[2, 1, 0, 3, -9, 5])
        );
        assert_eq!(
            run(
                "SELECT COALESCE(NULL, NULL), COALESCE(NULL, 1 / 0, - NULL), 1 + COALESCE(NULL, NULL)"
            ),
            [Value::Null, Value::Null, Value::Null]
        );
        assert_eq!(
            run("SELECT - COALESCE(NULL, 5) * 2, COALESCE(1, 2) + NULLIF(3, 4) x"),
            ints(&[-10, 4])
        );
    }

    #[test]
    fn nullif_compares_its_two_arguments() {
        assert_eq!(
            run(
                "SELECT NULLIF(1, 2), NULLIF(1, NULL), NULLIF(0, NULL), NULLIF(0, 1 / 0), \
                 NULLIF(-1, 1), NULLIF(COALESCE(NULL, 2), 3)"
            ),
            ints(&[1, 1, 0, 0, -1, 2])
        );
        assert_eq!(
            run(
                "SELECT NULLIF(1, 1), NULLIF(NULL, 1), NULLIF(NULL, NULL), NULLIF(1 / 0, 1 / 0), \
                 NULLIF(2, 1 + 1), NULLIF(-0, 0), 3 + NULLIF(4, 4), NULLIF(COALESCE(NULL, 2), 2)"
            ),
            [const { Value::Null }; 8]
        );
    }

    #[test]
    fn function_names_ignore_case_and_white_space() {
        assert_eq!(
            run(
                "SELECT coalesce(NULL, 1), CoAlEsCe (NULL, 2), COALESCE\t\r\n  (\nNULL ,3 ), \
                 nullif(4, 5), NuLlIf (5, 6), NULLIF\n(\t6 , 7\n)"
            ),
            ints(&[1, 2, 3, 4, 5, 6])
        );
        // The two names are not keywords, so each one can be an alias.
        assert_eq!(run("SELECT 1 coalesce, 2 AS nullif"), ints(&[1, 2]));
        for sql in [
            "SELECT coalesce",
            "SELECT nullif",
            "SELECT 1 coalesce(2, 3)",
        ] {
            assert!(select(sql).is_err(), "no error: {sql}");
        }
    }

    #[test]
    fn wrong_argument_count_is_an_error() {
        for sql in [
            "SELECT COALESCE()",
            "SELECT COALESCE(1)",
            "SELECT COALESCE(NULL)",
            "SELECT NULLIF()",
            "SELECT NULLIF(1)",
            "SELECT NULLIF(1, 2, 3)",
            "SELECT NULLIF(NULL, NULL, NULL)",
            "SELECT COALESCE(1, NULLIF(1))",
            "SELECT 1, COALESCE(2)",
        ] {
            assert!(select(sql).is_err(), "no error: {sql}");
        }
        assert_eq!(
            select("SELECT nullif(1)").unwrap_err().to_string(),
            "wrong number of arguments to function nullif"
        );
    }

    #[test]
    fn error_in_an_argument_is_an_error() {
        for sql in [
            // SQLite gives 1 for the first three. It does not compute the second argument.
            "SELECT COALESCE(1, 9223372036854775807 + 1)",
            "SELECT COALESCE(1, abs(1))",
            "SELECT COALESCE(1, 2, COALESCE(3))",
            "SELECT COALESCE(1, x)",
            "SELECT COALESCE(1, 'a')",
            "SELECT COALESCE(9223372036854775807 + 1, 1)",
            "SELECT NULLIF(1, 9223372036854775807 + 1)",
            "SELECT NULLIF(NULL, - (-9223372036854775807 - 1))",
            "SELECT NULLIF(1, 1 +)",
            "SELECT COALESCE(1, 2,)",
            "SELECT COALESCE(, 1, 2)",
            "SELECT COALESCE(1 2)",
            "SELECT COALESCE(1, 2",
            "SELECT COALESCE(1, 2))",
            "SELECT COALESCE(1 AS a, 2)",
            "SELECT COALESCE(*)",
            "SELECT COALESCE((1, 2), 3)",
            // SQLite gives 1 for the next two.
            "SELECT COALESCE(DISTINCT 1, 2)",
            "SELECT COALESCE(ALL 1, 2)",
            "SELECT COALESCE(1, 2) FILTER (WHERE 1)",
            "SELECT COALESCE(1, 2) OVER ()",
        ] {
            assert!(select(sql).is_err(), "no error: {sql}");
        }
    }

    #[test]
    fn each_other_function_is_an_error() {
        for sql in [
            "SELECT abs(1)",
            "SELECT ifnull(NULL, 2)",
            "SELECT max(1, 2)",
            "SELECT iif(1, 2, 3)",
            "SELECT coalesce2(1, 2)",
            "SELECT nullifx(1, 2)",
            "SELECT null_if(1, 2)",
            "SELECT COALESCE(NULL, ifnull(NULL, 2))",
            "SELECT main.coalesce(1, 2)",
        ] {
            assert!(select(sql).is_err(), "no error: {sql}");
        }
        assert_eq!(
            select("SELECT abs (1)").unwrap_err().to_string(),
            "unsupported syntax at Word(\"abs\")"
        );
    }

    #[test]
    fn count_of_the_one_row() {
        assert_eq!(
            run(
                "SELECT COUNT(*), count ( * ), COUNT(1), COUNT(0), COUNT(ALL 5), COUNT(DISTINCT 5), \
                 COUNT(- 1), COUNT(ALL(2)), COUNT(DISTINCT(3))"
            ),
            ints(&[1; 9])
        );
        assert_eq!(
            run(
                "SELECT COUNT(NULL), COUNT(ALL NULL), COUNT(DISTINCT NULL), COUNT(1 / 0), \
                 COUNT(NULLIF(2, 2)), COUNT(1 + NULL)"
            ),
            ints(&[0; 6])
        );
        for sql in [
            // SQLite gives 1 for the next two.
            "SELECT COUNT()",
            "SELECT COUNT(ALL)",
            "SELECT COUNT(DISTINCT)",
            "SELECT COUNT(DISTINCT *)",
            "SELECT COUNT(ALL *)",
            "SELECT COUNT(1, 2)",
            "SELECT COUNT(DISTINCT 1, 2)",
            "SELECT COUNT(*, 1)",
            "SELECT COUNT(1, *)",
            "SELECT COUNT(**)",
            "SELECT COUNT((*))",
            "SELECT COUNT(DISTINCT ALL 1)",
            "SELECT COUNT(ALL DISTINCT 1)",
            "SELECT COUNT(*",
            "SELECT COUNT(1 AS a)",
            // SQLite gives 1. The overflow is a real number there.
            "SELECT COUNT(9223372036854775807 + 1)",
        ] {
            assert!(select(sql).is_err(), "no error: {sql}");
        }
    }

    #[test]
    fn min_max_and_sum_of_one_value() {
        assert_eq!(
            run(
                "SELECT MIN(3), MAX(4), SUM(5), min(ALL 6), Max ( DISTINCT 7 ), sUm(DISTINCT - 8), \
                 SUM(ALL(0)), MIN(2 * 3), MAX(-9223372036854775807 - 1), SUM(9223372036854775807)"
            ),
            ints(&[3, 4, 5, 6, 7, -8, 0, 6, i64::MIN, i64::MAX])
        );
        assert_eq!(
            run(
                "SELECT MIN(NULL), MAX(NULL), SUM(NULL), SUM(ALL NULL), MIN(DISTINCT NULL), \
                 MAX(1 / 0), SUM(NULLIF(3, 3))"
            ),
            [const { Value::Null }; 7]
        );
        for sql in [
            "SELECT SUM()",
            "SELECT MIN()",
            "SELECT MAX()",
            "SELECT SUM(*)",
            "SELECT MIN(*)",
            "SELECT MAX(*)",
            "SELECT SUM(1, 2)",
            "SELECT SUM(DISTINCT)",
            "SELECT SUM(DISTINCT DISTINCT 1)",
            "SELECT MAX(1",
            "SELECT SUM(9223372036854775807 + 1)",
        ] {
            assert!(select(sql).is_err(), "no error: {sql}");
        }
        assert_eq!(
            select("SELECT Sum()").unwrap_err().to_string(),
            "unsupported syntax at RightParen"
        );
        assert_eq!(
            select("SELECT Sum(1, 2)").unwrap_err().to_string(),
            "wrong number of arguments to function Sum"
        );
    }

    #[test]
    fn aggregate_in_a_larger_expression() {
        assert_eq!(
            run(
                "SELECT - COUNT(*) * 2 + MAX(3), - MIN(- 3), (COUNT(*)), COUNT(*) * COUNT(*), \
                 COALESCE(COUNT(NULL), 5), COALESCE(SUM(NULL), 5), COALESCE(NULL, MAX(4)), \
                 NULLIF(MAX(3), 4), SUM(COALESCE(NULL, 6)), MIN(COALESCE(NULL, - 7))"
            ),
            ints(&[1, 3, 1, 1, 0, 5, 4, 3, 6, -7])
        );
        assert_eq!(
            run(
                "SELECT 1 + SUM(NULL), MIN(4) / COUNT(NULL), MAX(7) % COUNT(NULL), \
                 NULLIF(COUNT(*), 1), NULLIF(SUM(1), SUM(1)), MAX(NULLIF(3, 3))"
            ),
            [const { Value::Null }; 6]
        );
        // Aggregates and plain expressions in one list, with ALL, DISTINCT and aliases.
        assert_eq!(
            run("SELECT COUNT(*), 5, MAX(2) + 1, NULL, COALESCE(NULL, 3)"),
            [
                Value::Integer(1),
                Value::Integer(5),
                Value::Integer(3),
                Value::Null,
                Value::Integer(3),
            ]
        );
        assert_eq!(
            run("SELECT DISTINCT COUNT(*) AS c, MAX(1) m, 2 count, 3 AS max"),
            ints(&[1, 1, 2, 3])
        );
        assert_eq!(run("SELECT ALL MAX(1), 4 min, 5 AS sum"), ints(&[1, 4, 5]));
        for sql in [
            // SQLite gives a real number.
            "SELECT SUM(9223372036854775807) + 1",
            "SELECT COUNT(*) COUNT(*)",
            "SELECT 1 count(*)",
            "SELECT count",
            "SELECT COUNT *",
            // SQLite gives 1 for the next three.
            "SELECT COUNT(*) OVER ()",
            "SELECT COUNT(*) FILTER (WHERE 1)",
            "SELECT SUM(1 ORDER BY 1)",
        ] {
            assert!(select(sql).is_err(), "no error: {sql}");
        }
    }

    #[test]
    fn aggregate_inside_an_aggregate_is_an_error() {
        for sql in [
            "SELECT COUNT(COUNT(*))",
            "SELECT MAX(MIN(1))",
            "SELECT SUM(1 + COUNT(*))",
            "SELECT MIN(- SUM(2))",
            "SELECT SUM((COUNT(1)))",
            "SELECT COUNT(DISTINCT COUNT(*))",
            "SELECT COUNT(COALESCE(NULL, MAX(1)))",
            "SELECT COUNT(NULLIF(COUNT(*), 1))",
            "SELECT COALESCE(1, COUNT(COUNT(*)))",
            "SELECT MIN(1) + MAX(SUM(2))",
            "SELECT 1, MAX(MAX(1))",
        ] {
            assert!(select(sql).is_err(), "no error: {sql}");
        }
        assert_eq!(
            select("SELECT MAX(Count(*))").unwrap_err().to_string(),
            "misuse of aggregate function Count"
        );
        // An aggregate after the end of an aggregate is not inside it.
        assert_eq!(
            run(
                "SELECT MAX(1) + MAX(2), COALESCE(SUM(NULL), MIN(4)), COUNT(COALESCE(NULL, 2)) + COUNT(*)"
            ),
            ints(&[3, 4, 2])
        );
    }

    #[test]
    fn other_aggregates_and_scalar_min_max_are_errors() {
        for sql in [
            // SQLite gives a value for each of these.
            "SELECT MIN(1, 2)",
            "SELECT MAX(1, 2)",
            "SELECT MAX(1, 2, 3)",
            "SELECT MIN(1, NULL)",
            "SELECT MIN(DISTINCT 1, 2)",
            "SELECT MAX(COUNT(*), 0)",
            "SELECT MAX(MAX(1), 2)",
            "SELECT MAX(MAX(1, 2))",
            "SELECT AVG(1)",
            "SELECT avg(NULL)",
            "SELECT TOTAL(1)",
            "SELECT GROUP_CONCAT(1)",
            "SELECT COUNT(*) + AVG(1)",
        ] {
            assert!(select(sql).is_err(), "no error: {sql}");
        }
        assert_eq!(
            select("SELECT max(1, 2)").unwrap_err().to_string(),
            "the function max with more than one argument is not supported"
        );
        assert_eq!(
            select("SELECT 1 + AVG (1)").unwrap_err().to_string(),
            "unsupported syntax at Word(\"AVG\")"
        );
    }

    // The expected values of the four `cast_` tests are the answers of SQLite 3.46.1.
    #[test]
    fn cast_to_integer_returns_the_argument() {
        assert_eq!(
            run(
                "SELECT CAST(1 AS INTEGER), CAST(- 5 AS INTEGER), CAST(2 * 3 + 1 AS INTEGER), \
                 CAST((7) AS INTEGER), CAST(9223372036854775807 AS INTEGER), \
                 CAST(-9223372036854775807 - 1 AS INTEGER), CAST(0 AS INTEGER)"
            ),
            ints(&[1, -5, 7, 7, i64::MAX, i64::MIN, 0])
        );
        assert_eq!(
            run("SELECT CAST(NULL AS INTEGER), CAST(1 / 0 AS INTEGER), 1 + CAST(NULL AS INTEGER)"),
            [const { Value::Null }; 3]
        );
        // Two records of `random/expr/slt_good_0.test`.
        assert_eq!(
            run("SELECT 20 / - - 96 + CAST ( 90 AS INTEGER ) AS col2"),
            ints(&[90])
        );
        assert_eq!(
            run("SELECT - MAX ( ALL + CAST ( NULL AS INTEGER ) )"),
            [Value::Null]
        );
    }

    #[test]
    fn cast_ignores_case_and_white_space() {
        assert_eq!(
            run("SELECT cast(1 as integer), CaSt (2 aS InTeGeR), CAST\t\r\n (\n3 AS INTEGER )"),
            ints(&[1, 2, 3])
        );
        assert_eq!(
            run("SELECT CAST(1 AS INTEGER) AS x, CAST(2 AS INTEGER) y, CAST(3 AS INTEGER)z"),
            ints(&[1, 2, 3])
        );
    }

    #[test]
    fn cast_with_each_other_expression() {
        assert_eq!(
            run(
                "SELECT CAST(COUNT(*) AS INTEGER), CAST(MAX(DISTINCT 4) AS INTEGER), \
                 CAST(COALESCE(NULL, 2) AS INTEGER), CAST(NULLIF(3, 4) AS INTEGER), \
                 CAST(MIN(1) + SUM(2) AS INTEGER), CAST(COUNT(NULL) AS INTEGER)"
            ),
            ints(&[1, 4, 2, 3, 3, 0])
        );
        assert_eq!(
            run(
                "SELECT COUNT(CAST(NULL AS INTEGER)), SUM(CAST(5 AS INTEGER)), \
                 COALESCE(CAST(NULL AS INTEGER), 7), - CAST(3 AS INTEGER) * 2, \
                 (CAST(4 AS INTEGER)), CAST(CAST(6 AS INTEGER) AS INTEGER), \
                 CAST(- CAST(8 AS INTEGER) AS INTEGER), MIN(ALL CAST(9 AS INTEGER)), \
                 NULLIF(CAST(1 AS INTEGER), 2), CAST(MAX(1) AS INTEGER) + CAST(MAX(2) AS INTEGER)"
            ),
            ints(&[0, 5, 7, -6, 4, 6, -8, 9, 1, 3])
        );
        assert_eq!(
            run(
                "SELECT CAST(SUM(NULL) AS INTEGER), CAST(NULLIF(3, 3) AS INTEGER), \
                 NULLIF(CAST(1 AS INTEGER), 1), MAX(CAST(NULL AS INTEGER))"
            ),
            [const { Value::Null }; 4]
        );
        // A CAST does not hide an aggregate that is inside an aggregate.
        for sql in [
            "SELECT MAX(CAST(COUNT(*) AS INTEGER))",
            "SELECT CAST(MAX(COUNT(*)) AS INTEGER)",
        ] {
            assert_eq!(
                select(sql).unwrap_err().to_string(),
                "misuse of aggregate function COUNT"
            );
        }
        // Each CAST is one level of nesting.
        let casts = |depth: usize| {
            let (open, close) = ("CAST(".repeat(depth), " AS INTEGER)".repeat(depth));
            format!("SELECT {open}7{close}")
        };
        assert_eq!(run(&casts(MAX_DEPTH - 1)), ints(&[7]));
        assert!(select(&casts(MAX_DEPTH)).is_err());
    }

    #[test]
    fn cast_errors() {
        for sql in [
            // SQLite gives a value for each one up to `CAST(1 AS)`.
            "SELECT CAST(1 AS INT)",
            "SELECT CAST(1 AS SIGNED)",
            "SELECT CAST(1 AS BIGINT)",
            "SELECT CAST(1 AS REAL)",
            "SELECT CAST(1 AS TEXT)",
            "SELECT CAST(1 AS NUMERIC)",
            "SELECT CAST(1 AS BLOB)",
            "SELECT CAST(NULL AS TEXT)",
            "SELECT CAST(1 AS INTEGERS)",
            "SELECT CAST(1 AS INTEGER(10))",
            "SELECT CAST(1 AS INTEGER INTEGER)",
            "SELECT CAST(1 AS UNSIGNED INTEGER)",
            "SELECT CAST(1 AS \"INTEGER\")",
            // The overflow is a real number there, and the cast gives 9223372036854775807.
            "SELECT CAST(9223372036854775807 + 1 AS INTEGER)",
            "SELECT CAST(1 AS)",
            "SELECT CAST(1 INTEGER)",
            "SELECT CAST(1)",
            "SELECT CAST()",
            "SELECT CAST(AS INTEGER)",
            "SELECT CAST(1 AS INTEGER 2)",
            "SELECT CAST(1 AS INTEGER, 2)",
            "SELECT CAST(1, 2 AS INTEGER)",
            "SELECT CAST(1 AS INTEGER",
            "SELECT CAST(1 AS x AS INTEGER)",
            "SELECT CAST(1 x AS INTEGER)",
            "SELECT CAST(1 AS INTEGER AS INTEGER)",
            "SELECT CAST(1 AS INTEGER) 2",
            "SELECT CAST(1 AS INTEGER))",
            "SELECT CAST 1 AS INTEGER",
            "SELECT CAST",
            "SELECT CAST(ALL 1 AS INTEGER)",
            "SELECT CAST(DISTINCT 1 AS INTEGER)",
            "SELECT CAST(* AS INTEGER)",
            "SELECT COUNT(CAST(* AS INTEGER))",
            "SELECT CAST(1 AS INTEGER) OVER ()",
            "SELECT CAST(abs(1) AS INTEGER)",
            "SELECT 1 CAST(2 AS INTEGER)",
            // SQLite gives 1.
            "SELECT 1 AS CAST",
        ] {
            assert!(select(sql).is_err(), "no error: {sql}");
        }
        assert_eq!(
            select("SELECT CAST(1 AS text)").unwrap_err().to_string(),
            "CAST to text is not supported"
        );
        assert_eq!(
            select("SELECT CAST(1 AS)").unwrap_err().to_string(),
            "unsupported syntax at RightParen"
        );
    }

    #[test]
    fn select_list_keywords_and_aliases() {
        assert_eq!(run("select all 1 AS a, 2 b ,3"), ints(&[1, 2, 3]));
        assert_eq!(run("SeLeCt DiStInCt 1 aS col0, 1 _x9"), ints(&[1, 1]));
        assert_eq!(run("\tSELECT\n1\r\n"), ints(&[1]));
        assert_eq!(run("SELECT(1)x,2"), ints(&[1, 2]));
    }

    #[test]
    fn i64_limits() {
        assert_eq!(
            run("SELECT 9223372036854775807, -9223372036854775807 - 1"),
            ints(&[i64::MAX, i64::MIN])
        );
        assert_eq!(run("SELECT (-9223372036854775807 - 1) % -1"), ints(&[0]));
    }

    #[test]
    fn overflow_is_an_error() {
        for sql in [
            "SELECT 9223372036854775807 + 1",
            "SELECT -9223372036854775807 - 2",
            "SELECT 4611686018427387904 * 2",
            "SELECT (-9223372036854775807 - 1) / -1",
            "SELECT - (-9223372036854775807 - 1)",
            "SELECT 9223372036854775808",
            "SELECT -9223372036854775808",
            "SELECT 1, 9223372036854775807 + 1",
        ] {
            assert!(select(sql).is_err(), "no error: {sql}");
        }
    }

    #[test]
    fn unsupported_input_is_an_error() {
        for sql in [
            "",
            "SELECT",
            "SELECT ,",
            "SELECT 1,",
            "SELECT 1 2",
            "SELECT 1 a b",
            "SELECT 1 AS",
            "SELECT 1 AS from",
            "SELECT 1 AS 2",
            "SELECT (1",
            "SELECT 1)",
            "SELECT ()",
            "SELECT (1, 2)",
            "SELECT 1 +",
            "SELECT * 2",
            "SELECT 2 * / 2",
            "SELECT ALL DISTINCT 1",
            "SELECT DISTINCT ALL 1",
            "SELECT 1 FROM t",
            "SELECT 1 FROM",
            "SELECT * FROM t",
            "SELECT 1 WHERE 1",
            "SELECT 1 UNION SELECT 2",
            "SELECT 1 ORDER BY 1",
            "SELECT 1 LIMIT 1",
            "SELECT 1; SELECT 2",
            "SELECT 1;",
            "SELECT 'a'",
            "SELECT 1.5",
            "SELECT 1e5",
            "SELECT 0x10",
            "SELECT a",
            "SELECT abs(1)",
            "SELECT CASE WHEN 1 THEN 2 END",
            "SELECT 1 = 1",
            "SELECT 1 < 2",
            "SELECT 1 AND 1",
            "SELECT NOT 1",
            "SELECT 1 IS NULL",
            "SELECT 5 ISNULL",
            "SELECT 5 notnull",
            "SELECT 1 BETWEEN 0 AND 2",
            "SELECT 1 IN (1)",
            "SELECT ~1",
            "SELECT 1 -- 2",
            "SELECT 1 --2",
            "SELECT 4 /* x */ / 2",
            "VALUES (1)",
            "CREATE TABLE t(a)",
            "INSERT INTO t VALUES(1)",
            "1",
        ] {
            assert!(select(sql).is_err(), "no error: {sql}");
        }
    }

    #[test]
    fn deep_nesting_is_an_error() {
        let nested = |depth: usize| format!("SELECT {}1{}", "(".repeat(depth), ")".repeat(depth));
        assert_eq!(run(&nested(MAX_DEPTH - 1)), ints(&[1]));
        assert!(select(&nested(MAX_DEPTH)).is_err());
        assert!(select(&nested(100_000)).is_err());
        assert_eq!(
            run(&format!("SELECT {}1", "- ".repeat(MAX_DEPTH - 1))),
            ints(&[-1])
        );
        assert!(select(&format!("SELECT {}1", "- ".repeat(100_000))).is_err());
        let calls = |depth: usize| {
            let (open, close) = ("COALESCE(NULL, ".repeat(depth), ")".repeat(depth));
            format!("SELECT {open}7{close}")
        };
        assert_eq!(run(&calls(MAX_DEPTH - 1)), ints(&[7]));
        assert!(select(&calls(MAX_DEPTH)).is_err());
    }
}
