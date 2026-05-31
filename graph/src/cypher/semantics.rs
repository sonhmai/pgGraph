//! Semantic binding for openCypher compatibility statements.

use crate::gql::errors::GqlError;
use crate::query::catalog_snapshot::CatalogSnapshot;
use crate::query::logical_plan::LogicalStatement;

/// Bind a parsed openCypher compatibility statement into the shared logical IR.
///
/// # Errors
///
/// Returns [`GqlError`] when the statement uses an unsupported openCypher
/// feature or when normal shared GQL catalog binding fails.
pub(crate) fn bind_statement(
    statement: &super::ast::CypherStatement,
    catalog: &impl CatalogSnapshot,
) -> Result<LogicalStatement, GqlError> {
    match statement {
        super::ast::CypherStatement::Compatible { statement, .. } => {
            crate::query::semantics::bind_statement(statement, catalog)
        }
        super::ast::CypherStatement::Unsupported { feature, span } => Err(GqlError::unsupported(
            *span,
            format!(
                "openCypher feature `{feature}` does not map to pgGraph's compatibility subset"
            ),
        )),
    }
}
