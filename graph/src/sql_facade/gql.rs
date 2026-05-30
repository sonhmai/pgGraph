use super::admin::{check_enabled_result, with_panic_boundary};
use super::runtime::{current_query_freshness, ensure_current_graph_for_query};
use super::*;

/// Development-only GQL plan inspection.
#[pg_extern(schema = "graph")]
fn gql_explain(query: &str) -> String {
    with_panic_boundary("gql_explain()", || {
        check_enabled_result().unwrap_or_else(|err| err.report());
        let plan = build_plan(query).unwrap_or_else(|err| gql_error_to_graph_error(err).report());
        check_plan_acl(&plan);
        crate::query::explain::explain(&plan)
    })
}

/// Development-only coordinate-returning GQL execution.
#[pg_extern(schema = "graph", cost = 1000)]
#[allow(
    clippy::type_complexity,
    reason = "pgrx SQL ABI exposes tuple row columns"
)]
fn gql(query: &str) -> TableIterator<'static, (name!(row, pgrx::JsonB),)> {
    with_panic_boundary("gql()", || {
        check_enabled_result().unwrap_or_else(|err| err.report());
        let freshness = current_query_freshness().unwrap_or_else(|err| err.report());
        ensure_current_graph_for_query(freshness).unwrap_or_else(|err| err.report());
        let plan = build_plan(query).unwrap_or_else(|err| gql_error_to_graph_error(err).report());
        check_plan_acl(&plan);
        let rows = ENGINE
            .with(|engine| crate::query::execute::execute(&engine.borrow(), &plan))
            .unwrap_or_else(|err| err.report())
            .into_iter()
            .map(|row| (pgrx::JsonB(gql_row_json(row)),))
            .collect();
        TableIterator::new(rows)
    })
}

fn build_plan(
    query: &str,
) -> Result<crate::query::physical_plan::PhysicalPlan, crate::gql::errors::GqlError> {
    let ast = crate::gql::parse(query)?;
    let catalog = crate::query::catalog_snapshot::CatalogSnapshotImpl::load()
        .map_err(|err| crate::gql::errors::GqlError::bind(ast.span, err.to_string()))?;
    let logical = crate::query::semantics::bind(&ast, &catalog)?;
    Ok(crate::query::lower::lower(logical))
}

fn check_plan_acl(plan: &crate::query::physical_plan::PhysicalPlan) {
    for table_oid in plan.required_table_oids() {
        acl::check_table_acl(table_oid).unwrap_or_else(|err| err.report());
    }
}

fn gql_row_json(row: crate::query::execute::GqlRow) -> serde_json::Value {
    serde_json::Value::Array(
        row.values
            .into_iter()
            .map(|value| {
                serde_json::json!({
                    "name": value.name,
                    "table_oid": value.coordinate.table_oid,
                    "node_id": value.coordinate.node_id,
                })
            })
            .collect(),
    )
}

fn gql_error_to_graph_error(err: crate::gql::errors::GqlError) -> safety::GraphError {
    safety::GraphError::InvalidFilter {
        reason: err.to_string(),
    }
}
