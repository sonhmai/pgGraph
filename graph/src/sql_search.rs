//! SQL-layer search validation and source-row verification helpers.

use crate::api_types::SearchOutputRow;
use crate::catalog::{
    primary_key_expr, read_catalog, regclass_text, sql_table_name_from_catalog,
    table_oid_from_name, validate_column_exists,
};
use crate::quote::{quote_ident, quote_literal};
use crate::{acl, safety, types};
use pgrx::prelude::*;
use std::collections::HashSet;

struct SourceSearchStatement {
    table_oid: u32,
    table_name: String,
    query: String,
}

pub(crate) fn validate_search_request(
    property_key: &str,
    table_filter: Option<u32>,
    _tenant: Option<&str>,
) -> safety::GraphResult<()> {
    let (tables, _edges, _filter_columns) = read_catalog()?;
    let mut saw_registered_property = false;
    for table in &tables {
        if !table.columns.iter().any(|column| column == property_key) {
            continue;
        }
        let oid = table_oid_from_name(&table.table_name)?;
        if table_filter.is_none_or(|filter| filter == oid) {
            saw_registered_property = true;
            acl::check_table_acl(oid)?;
            validate_column_exists(oid, property_key)?;
        }
    }

    if let Some(filter) = table_filter {
        acl::check_table_acl(filter)?;
        if !saw_registered_property {
            validate_column_exists(filter, property_key)?;
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn source_table_search_statements(
    property_key: &str,
    property_value: &str,
    table_filter: Option<u32>,
    mode: types::SearchMode,
    case_sensitive: bool,
    tenant: Option<&str>,
    hydrate: bool,
) -> safety::GraphResult<Vec<SourceSearchStatement>> {
    let (tables, _edges, _filter_columns) = read_catalog()?;
    let mut statements = Vec::new();

    for table in tables {
        if !table.columns.iter().any(|column| column == property_key) {
            continue;
        }

        let table_name = sql_table_name_from_catalog(&table.table_name)?;
        let table_oid = table_name.oid();
        if table_filter.is_some_and(|filter| filter != table_oid) {
            continue;
        }
        acl::check_table_acl(table_oid)?;
        validate_column_exists(table_oid, property_key)?;

        let pk_expr = primary_key_expr("src", &table.id_column);
        let value_expr = format!("src.{}::text", quote_ident(property_key));
        let node_select = if hydrate {
            "to_jsonb(src.*)"
        } else {
            "NULL::jsonb"
        };
        let mut predicates = vec![
            format!("src.{} IS NOT NULL", quote_ident(property_key)),
            search_sql_predicate(&value_expr, property_value, mode, case_sensitive),
        ];
        if let (Some(tenant), Some(tenant_column)) = (tenant, table.tenant_column.as_deref()) {
            validate_column_exists(table_oid, tenant_column)?;
            predicates.push(format!(
                "src.{}::text = {}",
                quote_ident(tenant_column),
                quote_literal(tenant)
            ));
        }

        let query = format!(
            "SELECT {} AS graph_node_id, {} AS graph_value, {} AS graph_node
             FROM {} src
             WHERE {}
             ORDER BY graph_node_id",
            pk_expr,
            value_expr,
            node_select,
            table_name.as_sql(),
            predicates.join(" AND ")
        );

        statements.push(SourceSearchStatement {
            table_oid,
            table_name: table_name.as_sql().to_string(),
            query,
        });
    }

    Ok(statements)
}

#[cfg(feature = "pg_test")]
#[allow(clippy::too_many_arguments)]
pub(crate) fn source_table_search_sql_for_test(
    property_key: &str,
    property_value: &str,
    table_filter: Option<u32>,
    mode: types::SearchMode,
    case_sensitive: bool,
    tenant: Option<&str>,
    hydrate: bool,
) -> safety::GraphResult<Vec<String>> {
    Ok(source_table_search_statements(
        property_key,
        property_value,
        table_filter,
        mode,
        case_sensitive,
        tenant,
        hydrate,
    )?
    .into_iter()
    .map(|statement| statement.query)
    .collect())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn source_table_search_rows(
    property_key: &str,
    property_value: &str,
    table_filter: Option<u32>,
    mode: types::SearchMode,
    case_sensitive: bool,
    tenant: Option<&str>,
    hydrate: bool,
    offset: usize,
    limit: usize,
) -> safety::GraphResult<Vec<SearchOutputRow>> {
    if limit == 0 {
        return Ok(Vec::new());
    }

    let statements = source_table_search_statements(
        property_key,
        property_value,
        table_filter,
        mode,
        case_sensitive,
        tenant,
        hydrate,
    )?;
    let mut rows = Vec::new();

    for statement in statements {
        Spi::connect(|client| {
            let result = client.select(&statement.query, None, &[]).map_err(|e| {
                safety::GraphError::Internal(format!(
                    "source-table search failed for {}: {}",
                    statement.table_name, e
                ))
            })?;
            for row in result {
                let node_id = row
                    .get::<String>(1)
                    .map_err(|e| {
                        safety::GraphError::Internal(format!("search PK read failed: {}", e))
                    })?
                    .ok_or_else(|| {
                        safety::GraphError::Internal("search returned NULL PK".to_string())
                    })?;
                let Some(actual) = row.get::<String>(2).map_err(|e| {
                    safety::GraphError::Internal(format!("search value read failed: {}", e))
                })?
                else {
                    continue;
                };
                if !search_value_matches(&actual, property_value, mode, case_sensitive) {
                    continue;
                }
                let node = row.get::<pgrx::JsonB>(3).map_err(|e| {
                    safety::GraphError::Internal(format!("search hydration read failed: {}", e))
                })?;
                rows.push((
                    pgrx::pg_sys::Oid::from_u32(statement.table_oid),
                    node_id,
                    mode.as_match_type().to_string(),
                    1.0,
                    true,
                    node,
                    regclass_text(statement.table_oid)
                        .unwrap_or_else(|_| statement.table_oid.to_string()),
                ));
            }
            Ok::<(), safety::GraphError>(())
        })?;
    }

    rows.sort_by(|left, right| {
        left.0
            .to_u32()
            .cmp(&right.0.to_u32())
            .then_with(|| left.1.cmp(&right.1))
    });
    rows.dedup_by(|left, right| left.0 == right.0 && left.1 == right.1);

    Ok(rows.into_iter().skip(offset).take(limit).collect())
}

fn search_sql_predicate(
    value_expr: &str,
    property_value: &str,
    mode: types::SearchMode,
    case_sensitive: bool,
) -> String {
    let comparable_expr = if case_sensitive {
        value_expr.to_string()
    } else {
        format!("lower({})", value_expr)
    };
    let comparable_value = if case_sensitive {
        property_value.to_string()
    } else {
        property_value.to_lowercase()
    };

    match mode {
        types::SearchMode::Exact => {
            format!("{} = {}", comparable_expr, quote_literal(&comparable_value))
        }
        types::SearchMode::Prefix => {
            let pattern = format!("{}%", escape_like_pattern(&comparable_value));
            format!(
                "{} LIKE {} ESCAPE '\\'",
                comparable_expr,
                quote_literal(&pattern)
            )
        }
        types::SearchMode::Contains => {
            let pattern = format!("%{}%", escape_like_pattern(&comparable_value));
            format!(
                "{} LIKE {} ESCAPE '\\'",
                comparable_expr,
                quote_literal(&pattern)
            )
        }
        types::SearchMode::Token => {
            let predicates = comparable_value
                .split(|ch: char| !ch.is_alphanumeric())
                .filter(|token| !token.is_empty())
                .map(|token| {
                    let pattern = format!("%{}%", escape_like_pattern(token));
                    format!(
                        "{} LIKE {} ESCAPE '\\'",
                        comparable_expr,
                        quote_literal(&pattern)
                    )
                })
                .collect::<Vec<_>>();
            if predicates.is_empty() {
                "TRUE".to_string()
            } else {
                predicates.join(" AND ")
            }
        }
    }
}

fn escape_like_pattern(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

pub(crate) fn search_value_matches(
    actual: &str,
    expected: &str,
    mode: types::SearchMode,
    case_sensitive: bool,
) -> bool {
    let (actual, expected) = if case_sensitive {
        (actual.to_string(), expected.to_string())
    } else {
        (actual.to_lowercase(), expected.to_lowercase())
    };
    match mode {
        types::SearchMode::Contains => actual.contains(&expected),
        types::SearchMode::Exact => actual == expected,
        types::SearchMode::Prefix => actual.starts_with(&expected),
        types::SearchMode::Token => {
            let actual_tokens = actual
                .split(|ch: char| !ch.is_alphanumeric())
                .filter(|token| !token.is_empty())
                .collect::<HashSet<_>>();
            expected
                .split(|ch: char| !ch.is_alphanumeric())
                .filter(|token| !token.is_empty())
                .all(|token| actual_tokens.contains(token))
        }
    }
}
