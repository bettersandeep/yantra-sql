//! Parses and evaluates `SELECT` without `FROM`.
//!
//! The parser computes each value while it reads the tokens.
//! `None` is the SQL NULL.

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

    /// `+ unary`, `- unary`, an integer literal or `( expr )`.
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
            "SELECT NULL",
            "SELECT 'a'",
            "SELECT 1.5",
            "SELECT 1e5",
            "SELECT 0x10",
            "SELECT a",
            "SELECT abs(1)",
            "SELECT COUNT(*)",
            "SELECT CASE WHEN 1 THEN 2 END",
            "SELECT CAST(1 AS INTEGER)",
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
    }
}
