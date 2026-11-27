//! yantra-sql: a SQL database that gives the same answers as SQLite.

use std::fmt;

/// The version of this build.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// One SQL value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
}

/// The reason why the database did not run a statement.
#[derive(Debug)]
pub struct Error(String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// One database in memory.
#[derive(Default)]
pub struct Database;

impl Database {
    pub fn new() -> Self {
        Self
    }

    /// Runs one SQL statement and returns its result rows.
    /// Returns an error for each statement that it does not support.
    pub fn execute(&mut self, sql: &str) -> Result<Vec<Vec<Value>>, Error> {
        Err(Error(format!("unsupported: {sql}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_set() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn execute_supports_nothing() {
        let error = Database::new().execute("SELECT 1").unwrap_err();
        assert_eq!(error.to_string(), "unsupported: SELECT 1");
    }
}
