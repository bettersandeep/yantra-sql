//! Splits SQL text into tokens.

use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Token<'a> {
    Integer(i64),
    /// A keyword or an identifier, as written.
    Word(&'a str),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    /// `=` or `==`.
    Equal,
    /// `<>` or `!=`.
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    LeftParen,
    RightParen,
    Comma,
}

/// Returns the tokens of `sql`.
/// Returns an error for text that is not a supported token.
pub fn lex(sql: &str) -> Result<Vec<Token<'_>>, Error> {
    let bytes = sql.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        i += 1;
        let token = match bytes[start] {
            b' ' | b'\t' | b'\n' | b'\x0c' | b'\r' => continue,
            b'+' => Token::Plus,
            // `--` starts a comment in SQLite. It is not two minus signs.
            b'-' if bytes.get(i) == Some(&b'-') => {
                return Err(Error("comments are not supported".to_string()));
            }
            b'-' => Token::Minus,
            b'*' => Token::Star,
            b'/' => Token::Slash,
            b'%' => Token::Percent,
            // The spellings of the comparison operators are from section 2 of
            // https://www.sqlite.org/lang_expr.html.
            // The two characters of an operator have no white space between them:
            // SQLite 3.46.1 gives a syntax error for `1 < > 2`, `1 < = 2` and `1 = = 1`.
            b'=' => {
                if bytes.get(i) == Some(&b'=') {
                    i += 1;
                }
                Token::Equal
            }
            b'!' if bytes.get(i) == Some(&b'=') => {
                i += 1;
                Token::NotEqual
            }
            b'<' => match bytes.get(i) {
                Some(b'=') => {
                    i += 1;
                    Token::LessEqual
                }
                Some(b'>') => {
                    i += 1;
                    Token::NotEqual
                }
                // `<<` is a bit operator. It is not two comparisons.
                Some(b'<') => {
                    return Err(Error("the operator << is not supported".to_string()));
                }
                _ => Token::Less,
            },
            b'>' => match bytes.get(i) {
                Some(b'=') => {
                    i += 1;
                    Token::GreaterEqual
                }
                // `>>` is a bit operator. It is not two comparisons.
                Some(b'>') => {
                    return Err(Error("the operator >> is not supported".to_string()));
                }
                _ => Token::Greater,
            },
            b'(' => Token::LeftParen,
            b')' => Token::RightParen,
            b',' => Token::Comma,
            b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z' | b'_' => {
                while bytes
                    .get(i)
                    .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
                {
                    i += 1;
                }
                let word = &sql[start..i];
                if bytes[start].is_ascii_digit() {
                    // The parse fails for a number that does not fit i64.
                    // It also fails for text such as `1e5`, `0x10` and `1_000`.
                    let Ok(integer) = word.parse() else {
                        return Err(Error(format!("unsupported number: {word}")));
                    };
                    Token::Integer(integer)
                } else {
                    Token::Word(word)
                }
            }
            _ => {
                return Err(Error(format!("unsupported character at byte {start}")));
            }
        };
        tokens.push(token);
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_words_numbers_and_operators() {
        assert_eq!(
            lex("SELECT\t- -\n+ 5*(007/x_1)%2 ,\r\n9223372036854775807").unwrap(),
            [
                Token::Word("SELECT"),
                Token::Minus,
                Token::Minus,
                Token::Plus,
                Token::Integer(5),
                Token::Star,
                Token::LeftParen,
                Token::Integer(7),
                Token::Slash,
                Token::Word("x_1"),
                Token::RightParen,
                Token::Percent,
                Token::Integer(2),
                Token::Comma,
                Token::Integer(i64::MAX),
            ]
        );
        assert_eq!(lex(" \n").unwrap(), []);
    }

    #[test]
    fn splits_comparison_operators() {
        assert_eq!(
            lex("= == <> != < <= > >= <1>2=3").unwrap(),
            [
                Token::Equal,
                Token::Equal,
                Token::NotEqual,
                Token::NotEqual,
                Token::Less,
                Token::LessEqual,
                Token::Greater,
                Token::GreaterEqual,
                Token::Less,
                Token::Integer(1),
                Token::Greater,
                Token::Integer(2),
                Token::Equal,
                Token::Integer(3),
            ]
        );
        // White space ends an operator, and the longest operator wins.
        assert_eq!(
            lex("< > < = = = === =< => >< >=- <=>").unwrap(),
            [
                Token::Less,
                Token::Greater,
                Token::Less,
                Token::Equal,
                Token::Equal,
                Token::Equal,
                Token::Equal,
                Token::Equal,
                Token::Equal,
                Token::Less,
                Token::Equal,
                Token::Greater,
                Token::Greater,
                Token::Less,
                Token::GreaterEqual,
                Token::Minus,
                Token::LessEqual,
                Token::Greater,
            ]
        );
    }

    #[test]
    fn rejects_unsupported_text() {
        for sql in [
            "9223372036854775808",
            "1.5",
            ".5",
            "1e5",
            "0x10",
            "1_000",
            "1x",
            "'text'",
            "\"name\"",
            "1 -- comment",
            "1 --2",
            "1 ! = 2",
            "1 ! 2",
            "!1",
            "1 << 2",
            "8 >> 1",
            "1 <<= 2",
            "1 || 2",
            "1 | 2",
            "1 & 3",
            "~1",
            "1;",
            "a.b",
            "caf\u{e9}",
            "1\u{a0}+ 1",
        ] {
            assert!(lex(sql).is_err(), "lexed: {sql}");
        }
    }
}
