//! Structured SQL filter parsing and conversion into in-memory filter operations.

use crate::catalog::{read_catalog, table_oid_from_name};
use crate::quote::quote_literal;
use crate::{acl, filter_index, safety, types};
use pgrx::prelude::*;
use std::collections::HashSet;

pub(crate) fn filter_helper(column_name: &str, operator: &str, value: pgrx::JsonB) -> pgrx::JsonB {
    validate_filter_identifier(column_name).unwrap_or_else(|err| err.report());
    let mut predicate = serde_json::Map::new();
    predicate.insert(operator.to_string(), value.0);
    let mut where_clause = serde_json::Map::new();
    where_clause.insert(
        column_name.to_string(),
        serde_json::Value::Object(predicate),
    );
    pgrx::JsonB(serde_json::json!({ "where": where_clause }))
}

#[derive(Debug, Clone)]
pub(crate) struct ParsedStructuredFilter {
    pub(crate) pushdown_filters: Vec<PushdownFilter>,
    pub(crate) hydration_filters: Vec<HydrationFilter>,
}

#[derive(Debug, Clone)]
pub(crate) struct PushdownFilter {
    pub(crate) column: String,
    pub(crate) operator: String,
    pub(crate) value: serde_json::Value,
}

#[derive(Debug, Clone)]
pub(crate) struct HydrationFilter {
    pub(crate) table_oid: u32,
    pub(crate) column: String,
    pub(crate) operator: HydrationFilterOperator,
}

#[derive(Debug, Clone)]
pub(crate) enum HydrationFilterOperator {
    Eq(serde_json::Value),
    Neq(serde_json::Value),
    Gt(serde_json::Value),
    Gte(serde_json::Value),
    Lt(serde_json::Value),
    Lte(serde_json::Value),
    Between(serde_json::Value, serde_json::Value),
}

#[derive(Debug, Clone)]
pub(crate) struct FilterColumnResolution {
    pub(crate) table_oid: u32,
    pub(crate) column_type: Option<String>,
}

pub(crate) fn parse_structured_filter(
    filter: &pgrx::JsonB,
    requested_table_oids: &HashSet<u32>,
) -> safety::GraphResult<ParsedStructuredFilter> {
    let filter_object = filter
        .0
        .as_object()
        .ok_or_else(|| safety::GraphError::InvalidFilter {
            reason: "structured filter must be a JSON object".to_string(),
        })?;
    let where_clause = match (filter_object.get("node"), filter_object.get("where")) {
        (Some(_), Some(_)) => {
            return Err(safety::GraphError::InvalidFilter {
                reason: "structured filter must use either node.where or where, not both"
                    .to_string(),
            });
        }
        (Some(node), None) => {
            let node_object =
                node.as_object()
                    .ok_or_else(|| safety::GraphError::InvalidFilter {
                        reason: "structured filter node must be an object".to_string(),
                    })?;
            if filter_object.len() != 1 || node_object.keys().any(|key| key != "where") {
                return Err(safety::GraphError::InvalidFilter {
                    reason: "structured filter supports only node.where for traversal".to_string(),
                });
            }
            node_object
                .get("where")
                .ok_or_else(|| safety::GraphError::InvalidFilter {
                    reason: "structured filter must contain node.where".to_string(),
                })?
        }
        (None, Some(where_clause)) => {
            if filter_object.len() != 1 {
                return Err(safety::GraphError::InvalidFilter {
                    reason: "structured filter supports only where for helper-built filters"
                        .to_string(),
                });
            }
            where_clause
        }
        (None, None) => {
            return Err(safety::GraphError::InvalidFilter {
                reason: "structured filter must contain node.where".to_string(),
            });
        }
    };

    let predicates = where_clause
        .as_object()
        .ok_or_else(|| safety::GraphError::InvalidFilter {
            reason: "structured filter node.where must be an object".to_string(),
        })?;
    if predicates.is_empty() {
        return Ok(ParsedStructuredFilter {
            pushdown_filters: Vec::new(),
            hydration_filters: Vec::new(),
        });
    }

    let mut pushdown_filters = Vec::with_capacity(predicates.len());
    let mut hydration_filters = Vec::new();
    for (column, predicate) in predicates {
        validate_filter_identifier(column)?;
        let resolved = resolve_structured_filter_column(column, requested_table_oids)?;
        let operators = predicate
            .as_object()
            .ok_or_else(|| safety::GraphError::InvalidFilter {
                reason: format!("filter for '{}' must be an operator object", column),
            })?;
        if operators.len() != 1 {
            return Err(safety::GraphError::InvalidFilter {
                reason: format!("filter for '{}' must contain exactly one operator", column),
            });
        }
        let Some((operator, value)) = operators.iter().next() else {
            return Err(safety::GraphError::InvalidFilter {
                reason: format!("filter for '{}' must contain exactly one operator", column),
            });
        };
        if resolved.column_type.is_some() {
            validate_structured_operator_shape(column, operator, value)?;
            pushdown_filters.push(PushdownFilter {
                column: column.clone(),
                operator: operator.clone(),
                value: value.clone(),
            });
        } else {
            hydration_filters.push(HydrationFilter {
                table_oid: resolved.table_oid,
                column: column.clone(),
                operator: hydration_filter_operator(column, operator, value)?,
            });
        }
    }

    Ok(ParsedStructuredFilter {
        pushdown_filters,
        hydration_filters,
    })
}

pub(crate) fn resolve_structured_filter_column(
    column: &str,
    requested_table_oids: &HashSet<u32>,
) -> safety::GraphResult<FilterColumnResolution> {
    let registered = Spi::connect(|client| {
        let query = format!(
            "SELECT to_regclass(table_name)::oid::integer, column_type
             FROM graph._registered_filter_columns
             WHERE column_name = {}
               AND to_regclass(table_name) IS NOT NULL
             ORDER BY table_name",
            quote_literal(column)
        );
        let result = client.select(&query, None, &[]).map_err(|err| {
            safety::GraphError::Internal(format!("filter catalog validation failed: {}", err))
        })?;
        let mut rows = Vec::new();
        for row in result {
            let table_oid = row
                .get::<i32>(1)
                .map_err(|err| safety::GraphError::Internal(err.to_string()))?
                .map(|oid| oid as u32);
            let column_type = row
                .get::<String>(2)
                .map_err(|err| safety::GraphError::Internal(err.to_string()))?
                .unwrap_or_default();
            if let Some(table_oid) = table_oid {
                rows.push((table_oid, column_type));
            }
        }
        Ok::<_, safety::GraphError>(rows)
    })?;

    let registrations = registered
        .into_iter()
        .filter(|(table_oid, _column_type)| {
            requested_table_oids.is_empty() || requested_table_oids.contains(table_oid)
        })
        .collect::<Vec<_>>();

    if registrations.len() > 1 {
        return Err(safety::GraphError::InvalidFilter {
            reason: format!(
                "filter column '{}' is registered on multiple tables; table-scoped structured filters are required",
                column
            ),
        });
    }
    if let Some((table_oid, column_type)) = registrations.into_iter().next() {
        return Ok(FilterColumnResolution {
            table_oid,
            column_type: Some(column_type),
        });
    }

    let candidates = source_tables_with_column(column, requested_table_oids)?;
    if candidates.is_empty() {
        return Err(safety::GraphError::InvalidFilter {
            reason: format!(
                "filter column '{}' is not present on registered node tables",
                column
            ),
        });
    }
    if candidates.len() > 1 {
        return Err(safety::GraphError::InvalidFilter {
            reason: format!(
                "filter column '{}' exists on multiple registered node tables; table-scoped structured filters are required",
                column
            ),
        });
    }

    Ok(FilterColumnResolution {
        table_oid: candidates[0],
        column_type: None,
    })
}

pub(crate) fn source_tables_with_column(
    column: &str,
    requested_table_oids: &HashSet<u32>,
) -> safety::GraphResult<Vec<u32>> {
    let (tables, _edges, _filter_columns) = read_catalog()?;
    let mut candidates = Vec::new();
    for table in tables {
        let table_oid = table_oid_from_name(&table.table_name)?;
        if !requested_table_oids.is_empty() && !requested_table_oids.contains(&table_oid) {
            continue;
        }
        if table_has_column(table_oid, column)? {
            acl::check_table_acl(table_oid)?;
            candidates.push(table_oid);
        }
    }
    Ok(candidates)
}

pub(crate) fn table_has_column(table_oid: u32, column: &str) -> safety::GraphResult<bool> {
    Spi::connect(|client| {
        let table_oid = pgrx::pg_sys::Oid::from_u32(table_oid);
        let result = client
            .select(
                "SELECT EXISTS (
                SELECT 1
                FROM pg_attribute
                WHERE attrelid = $1::oid
                  AND attname = $2
                  AND attnum > 0
                  AND NOT attisdropped
            )",
                None,
                &[table_oid.into(), column.into()],
            )
            .map_err(|e| safety::GraphError::Internal(format!("column lookup failed: {}", e)))?;
        let row = result.first();
        row.get::<bool>(1)
            .map_err(|e| safety::GraphError::Internal(format!("column lookup read failed: {}", e)))
            .map(|value| value.unwrap_or(false))
    })
}

pub(crate) fn validate_structured_operator_shape(
    column: &str,
    operator: &str,
    value: &serde_json::Value,
) -> safety::GraphResult<()> {
    match operator {
        "eq" | "neq" | "gt" | "gte" | "lt" | "lte" => Ok(()),
        "between" => {
            value
                .as_array()
                .filter(|bounds| bounds.len() == 2)
                .ok_or_else(|| safety::GraphError::InvalidFilter {
                    reason: format!("between filter for '{}' must be a two-item array", column),
                })?;
            Ok(())
        }
        _ => Err(safety::GraphError::InvalidFilter {
            reason: format!("unsupported structured filter operator '{}'", operator),
        }),
    }
}

pub(crate) fn typed_pushdown_filter_op(
    filter_index: &filter_index::FilterIndex,
    filter: &PushdownFilter,
) -> safety::GraphResult<types::FilterOp> {
    let column_idx = filter_index.find_column(&filter.column).ok_or_else(|| {
        safety::GraphError::InvalidFilter {
            reason: format!("filter column '{}' is not indexed", filter.column),
        }
    })?;
    if filter.value.is_null() {
        return match filter.operator.as_str() {
            "eq" => Ok(types::FilterOp::IsNull(column_idx)),
            "neq" => Ok(types::FilterOp::IsNotNull(column_idx)),
            other => Err(safety::GraphError::InvalidFilter {
                reason: format!("operator '{}' is not supported for NULL filters", other),
            }),
        };
    }
    let column_type =
        filter_index
            .column_type(column_idx)
            .ok_or_else(|| safety::GraphError::InvalidFilter {
                reason: format!("filter column '{}' is not indexed", filter.column),
            })?;
    match column_type {
        filter_index::FilterColumnType::Numeric => typed_i64_op(
            column_idx,
            &filter.operator,
            &filter.value,
            jsonb_filter_i64,
        ),
        filter_index::FilterColumnType::Boolean => {
            let value =
                filter
                    .value
                    .as_bool()
                    .ok_or_else(|| safety::GraphError::InvalidFilter {
                        reason: format!(
                            "filter value for '{}.{}' must be boolean",
                            filter.column, filter.operator
                        ),
                    })?;
            match filter.operator.as_str() {
                "eq" => Ok(types::FilterOp::EqBool(column_idx, value)),
                "neq" => Ok(types::FilterOp::NeqBool(column_idx, value)),
                other => Err(safety::GraphError::InvalidFilter {
                    reason: format!("operator '{}' is not supported for boolean filters", other),
                }),
            }
        }
        filter_index::FilterColumnType::Text => {
            let value = filter
                .value
                .as_str()
                .ok_or_else(|| safety::GraphError::InvalidFilter {
                    reason: format!(
                        "filter value for '{}.{}' must be text",
                        filter.column, filter.operator
                    ),
                })?;
            let token = filter_index
                .lookup_text_value(column_idx, value)
                .unwrap_or(u32::MAX);
            match filter.operator.as_str() {
                "eq" => Ok(types::FilterOp::EqToken(column_idx, token)),
                "neq" => Ok(types::FilterOp::NeqToken(column_idx, token)),
                other => Err(safety::GraphError::InvalidFilter {
                    reason: format!("operator '{}' is not supported for text filters", other),
                }),
            }
        }
        filter_index::FilterColumnType::Date => {
            let op = typed_i64_op(
                column_idx,
                &filter.operator,
                &filter.value,
                encode_date_filter_value,
            )?;
            Ok(op)
        }
        filter_index::FilterColumnType::Timestamptz => {
            let op = typed_i64_op(
                column_idx,
                &filter.operator,
                &filter.value,
                encode_timestamptz_filter_value,
            )?;
            Ok(op)
        }
        filter_index::FilterColumnType::Uuid => {
            let value = filter
                .value
                .as_str()
                .ok_or_else(|| safety::GraphError::InvalidFilter {
                    reason: format!(
                        "filter value for '{}.{}' must be uuid text",
                        filter.column, filter.operator
                    ),
                })?;
            let value = parse_uuid_u128(value)?;
            match filter.operator.as_str() {
                "eq" => Ok(types::FilterOp::EqUuid(column_idx, value)),
                "neq" => Ok(types::FilterOp::NeqUuid(column_idx, value)),
                other => Err(safety::GraphError::InvalidFilter {
                    reason: format!("operator '{}' is not supported for uuid filters", other),
                }),
            }
        }
    }
}

pub(crate) fn typed_i64_op(
    column_idx: usize,
    operator: &str,
    value: &serde_json::Value,
    encoder: fn(&serde_json::Value) -> safety::GraphResult<i64>,
) -> safety::GraphResult<types::FilterOp> {
    match operator {
        "eq" => Ok(types::FilterOp::EqI64(column_idx, encoder(value)?)),
        "neq" => Ok(types::FilterOp::NeqI64(column_idx, encoder(value)?)),
        "gt" => Ok(types::FilterOp::GtI64(column_idx, encoder(value)?)),
        "gte" => Ok(types::FilterOp::GteI64(column_idx, encoder(value)?)),
        "lt" => Ok(types::FilterOp::LtI64(column_idx, encoder(value)?)),
        "lte" => Ok(types::FilterOp::LteI64(column_idx, encoder(value)?)),
        "between" => {
            let bounds = value
                .as_array()
                .filter(|bounds| bounds.len() == 2)
                .ok_or_else(|| safety::GraphError::InvalidFilter {
                    reason: "between filter must be a two-item array".to_string(),
                })?;
            Ok(types::FilterOp::BetweenI64(
                column_idx,
                encoder(&bounds[0])?,
                encoder(&bounds[1])?,
            ))
        }
        other => Err(safety::GraphError::InvalidFilter {
            reason: format!("unsupported numeric filter operator '{}'", other),
        }),
    }
}

pub(crate) fn encode_date_filter_value(value: &serde_json::Value) -> safety::GraphResult<i64> {
    if let Some(value) = value.as_i64() {
        return Ok(value);
    }
    let text = value
        .as_str()
        .ok_or_else(|| safety::GraphError::InvalidFilter {
            reason: "date filter values must be ISO date text".to_string(),
        })?;
    Spi::connect(|client| {
        let query = format!(
            "SELECT (({})::date - DATE '2000-01-01')::bigint",
            quote_literal(text)
        );
        let result =
            client
                .select(&query, None, &[])
                .map_err(|err| safety::GraphError::InvalidFilter {
                    reason: format!("invalid date filter value '{}': {}", text, err),
                })?;
        result
            .first()
            .get::<i64>(1)
            .map_err(|err| safety::GraphError::Internal(err.to_string()))?
            .ok_or_else(|| safety::GraphError::InvalidFilter {
                reason: format!("invalid date filter value '{}'", text),
            })
    })
}

pub(crate) fn encode_timestamptz_filter_value(
    value: &serde_json::Value,
) -> safety::GraphResult<i64> {
    if let Some(value) = value.as_i64() {
        return Ok(value);
    }
    let text = value
        .as_str()
        .ok_or_else(|| safety::GraphError::InvalidFilter {
            reason: "timestamptz filter values must be timestamp text".to_string(),
        })?;
    Spi::connect(|client| {
        let query = format!(
            "SELECT (EXTRACT(EPOCH FROM ({})::timestamptz) * 1000000)::bigint",
            quote_literal(text)
        );
        let result =
            client
                .select(&query, None, &[])
                .map_err(|err| safety::GraphError::InvalidFilter {
                    reason: format!("invalid timestamptz filter value '{}': {}", text, err),
                })?;
        result
            .first()
            .get::<i64>(1)
            .map_err(|err| safety::GraphError::Internal(err.to_string()))?
            .ok_or_else(|| safety::GraphError::InvalidFilter {
                reason: format!("invalid timestamptz filter value '{}'", text),
            })
    })
}

pub(crate) fn parse_uuid_u128(value: &str) -> safety::GraphResult<u128> {
    let compact = value.chars().filter(|ch| *ch != '-').collect::<String>();
    if compact.len() != 32 || !compact.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(safety::GraphError::InvalidFilter {
            reason: format!("invalid uuid filter value '{}'", value),
        });
    }
    u128::from_str_radix(&compact, 16).map_err(|err| safety::GraphError::InvalidFilter {
        reason: format!("invalid uuid filter value '{}': {}", value, err),
    })
}

pub(crate) fn hydration_filter_operator(
    column: &str,
    operator: &str,
    value: &serde_json::Value,
) -> safety::GraphResult<HydrationFilterOperator> {
    match operator {
        "eq" => Ok(HydrationFilterOperator::Eq(value.clone())),
        "neq" => Ok(HydrationFilterOperator::Neq(value.clone())),
        "gt" => Ok(HydrationFilterOperator::Gt(value.clone())),
        "gte" => Ok(HydrationFilterOperator::Gte(value.clone())),
        "lt" => Ok(HydrationFilterOperator::Lt(value.clone())),
        "lte" => Ok(HydrationFilterOperator::Lte(value.clone())),
        "between" => {
            let bounds = value
                .as_array()
                .filter(|bounds| bounds.len() == 2)
                .ok_or_else(|| safety::GraphError::InvalidFilter {
                    reason: format!("between filter for '{}' must be a two-item array", column),
                })?;
            Ok(HydrationFilterOperator::Between(
                bounds[0].clone(),
                bounds[1].clone(),
            ))
        }
        _ => Err(safety::GraphError::InvalidFilter {
            reason: format!("unsupported structured filter operator '{}'", operator),
        }),
    }
}

pub(crate) fn validate_filter_identifier(identifier: &str) -> safety::GraphResult<()> {
    let valid = !identifier.is_empty()
        && identifier
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_');
    if valid {
        Ok(())
    } else {
        Err(safety::GraphError::InvalidFilter {
            reason: format!("unsupported filter column identifier '{}'", identifier),
        })
    }
}

pub(crate) fn jsonb_filter_i64(value: &serde_json::Value) -> safety::GraphResult<i64> {
    if let Some(number) = value.as_i64() {
        return Ok(number);
    }
    if let Some(text) = value.as_str() {
        return text
            .parse::<i64>()
            .map_err(|_| safety::GraphError::InvalidFilter {
                reason: "numeric filter values must be signed 64-bit integers".to_string(),
            });
    }
    Err(safety::GraphError::InvalidFilter {
        reason: "numeric filter values must be signed 64-bit integers".to_string(),
    })
}

pub(crate) fn hydration_filters_match(
    table_oid: u32,
    node: &pgrx::JsonB,
    filters: &[HydrationFilter],
) -> bool {
    filters
        .iter()
        .all(|filter| filter.table_oid == table_oid && hydration_filter_match(node, filter))
}

pub(crate) fn hydration_filter_match(node: &pgrx::JsonB, filter: &HydrationFilter) -> bool {
    let actual = node
        .0
        .get(&filter.column)
        .unwrap_or(&serde_json::Value::Null);
    match &filter.operator {
        HydrationFilterOperator::Eq(expected) => json_values_equal(actual, expected),
        HydrationFilterOperator::Neq(expected) => !json_values_equal(actual, expected),
        HydrationFilterOperator::Gt(expected) => json_value_compare(actual, expected)
            .is_some_and(|ordering| ordering == std::cmp::Ordering::Greater),
        HydrationFilterOperator::Gte(expected) => json_value_compare(actual, expected)
            .is_some_and(|ordering| ordering != std::cmp::Ordering::Less),
        HydrationFilterOperator::Lt(expected) => json_value_compare(actual, expected)
            .is_some_and(|ordering| ordering == std::cmp::Ordering::Less),
        HydrationFilterOperator::Lte(expected) => json_value_compare(actual, expected)
            .is_some_and(|ordering| ordering != std::cmp::Ordering::Greater),
        HydrationFilterOperator::Between(low, high) => {
            json_value_compare(actual, low)
                .is_some_and(|ordering| ordering != std::cmp::Ordering::Less)
                && json_value_compare(actual, high)
                    .is_some_and(|ordering| ordering != std::cmp::Ordering::Greater)
        }
    }
}

pub(crate) fn json_values_equal(actual: &serde_json::Value, expected: &serde_json::Value) -> bool {
    if actual.is_null() || expected.is_null() {
        return actual.is_null() && expected.is_null();
    }
    if let (Some(actual), Some(expected)) = (json_value_as_f64(actual), json_value_as_f64(expected))
    {
        return actual == expected;
    }
    if let (Some(actual), Some(expected)) = (actual.as_bool(), expected.as_bool()) {
        return actual == expected;
    }
    json_value_as_text(actual).is_some_and(|actual| {
        json_value_as_text(expected).is_some_and(|expected| actual == expected)
    })
}

pub(crate) fn json_value_compare(
    actual: &serde_json::Value,
    expected: &serde_json::Value,
) -> Option<std::cmp::Ordering> {
    if actual.is_null() || expected.is_null() {
        return None;
    }
    if let (Some(actual), Some(expected)) = (json_value_as_f64(actual), json_value_as_f64(expected))
    {
        return actual.partial_cmp(&expected);
    }
    let actual = json_value_as_text(actual)?;
    let expected = json_value_as_text(expected)?;
    Some(actual.cmp(&expected))
}

pub(crate) fn json_value_as_f64(value: &serde_json::Value) -> Option<f64> {
    match value {
        serde_json::Value::Number(number) => number.as_f64(),
        serde_json::Value::String(text) => text.parse::<f64>().ok(),
        _ => None,
    }
}

pub(crate) fn json_value_as_text(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Number(number) => Some(number.to_string()),
        serde_json::Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}
