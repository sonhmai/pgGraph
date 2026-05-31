//! Lowering for openCypher compatibility statements.

use crate::gql::errors::GqlError;
use crate::query::catalog_snapshot::CatalogSnapshot;
use crate::query::physical_plan::PhysicalStatement;

/// Bind and lower openCypher compatibility input into the shared physical plan.
///
/// # Errors
///
/// Returns [`GqlError`] for unsupported openCypher constructs, catalog binding
/// failures, or any shared GQL semantic rejection.
pub(crate) fn lower_statement(
    statement: &super::ast::CypherStatement,
    catalog: &impl CatalogSnapshot,
) -> Result<PhysicalStatement, GqlError> {
    let logical = super::semantics::bind_statement(statement, catalog)?;
    Ok(crate::query::lower::lower_statement(logical))
}
