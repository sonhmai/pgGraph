//! Canonical SQL quoting helpers for dynamic SQL fragments.
//!
//! Values passed as data should still use SPI parameters. These helpers are
//! only for SQL identifiers and literal fragments in places where PostgreSQL
//! does not allow bind parameters.

/// Quote a PostgreSQL identifier.
#[cfg(not(test))]
pub(crate) fn quote_ident(identifier: &str) -> String {
    pgrx::spi::quote_identifier(identifier)
}

/// Quote a PostgreSQL identifier for pure Rust unit tests that run outside a
/// PostgreSQL backend.
#[cfg(test)]
pub(crate) fn quote_ident(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

/// Quote a PostgreSQL string literal.
#[cfg(not(test))]
pub(crate) fn quote_literal(value: &str) -> String {
    pgrx::spi::quote_literal(value)
}

/// Quote a PostgreSQL string literal for pure Rust unit tests that run outside
/// a PostgreSQL backend.
#[cfg(test)]
pub(crate) fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
