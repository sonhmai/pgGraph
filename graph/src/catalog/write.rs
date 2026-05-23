use crate::{builder, safety};
use pgrx::prelude::*;

pub(crate) fn insert_registered_table(
    table_name: &str,
    id_columns: impl Into<builder::PrimaryKeySpec>,
    columns: impl Into<builder::PropertyColumns>,
    tenant_column: Option<&str>,
) -> safety::GraphResult<()> {
    let id_columns = id_columns.into();
    let columns = columns.into();
    let id_column = id_columns.as_catalog_text();
    let columns = columns.as_catalog_text();
    Spi::run_with_args(
        "INSERT INTO graph._registered_tables (table_name, id_column, columns, tenant_column)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (table_name) DO UPDATE SET
           id_column = EXCLUDED.id_column,
           columns = EXCLUDED.columns,
           tenant_column = EXCLUDED.tenant_column",
        &[
            table_name.into(),
            id_column.into(),
            columns.into(),
            tenant_column.map(|value| value.to_string()).into(),
        ],
    )
    .map_err(|e| safety::GraphError::Internal(format!("registered table write failed: {}", e)))
}

pub(crate) struct RegisteredEdgeInsert<'a> {
    pub(crate) from_table: &'a str,
    pub(crate) from_column: &'a str,
    pub(crate) to_table: &'a str,
    pub(crate) to_column: &'a str,
    pub(crate) label: &'a str,
    pub(crate) bidirectional: bool,
    pub(crate) weight_column: Option<&'a str>,
    pub(crate) label_column: Option<&'a str>,
}

pub(crate) fn insert_registered_edge(edge: RegisteredEdgeInsert<'_>) -> safety::GraphResult<()> {
    Spi::run_with_args(
        "INSERT INTO graph._registered_edges
           (from_table, from_column, to_table, to_column, label, bidirectional, weight_column, label_column)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         ON CONFLICT (from_table, from_column, to_table, to_column, label)
         DO UPDATE SET
            bidirectional = EXCLUDED.bidirectional,
            weight_column = EXCLUDED.weight_column,
            label_column = EXCLUDED.label_column",
        &[
            edge.from_table.into(),
            edge.from_column.into(),
            edge.to_table.into(),
            edge.to_column.into(),
            edge.label.into(),
            edge.bidirectional.into(),
            edge.weight_column.map(|value| value.to_string()).into(),
            edge.label_column.map(|value| value.to_string()).into(),
        ],
    )
    .map_err(|e| safety::GraphError::Internal(format!("registered edge write failed: {}", e)))
}
