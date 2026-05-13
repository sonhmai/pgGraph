//! SQL sync-log replay, trigger management, and tenant-scope helpers.

use crate::catalog::{read_catalog, table_oid_from_name};
use crate::filter_index::{EncodedFilterValue, FilterColumnType};
use crate::quote::quote_literal;
use crate::sql_filters::{
    encode_date_filter_value, encode_timestamptz_filter_value, parse_uuid_u128,
};
use crate::{builder, config, engine, safety, sync, ENGINE};
use pgrx::prelude::*;
use std::collections::{HashMap, HashSet};

pub(crate) fn current_sync_mode() -> safety::GraphResult<config::SyncMode> {
    match config::parsed_sync_mode() {
        Some(config::SyncMode::Wal) => Err(safety::GraphError::InvalidFilter {
            reason:
                "graph.sync_mode = 'wal' is reserved for roadmap work; please use 'trigger' or 'manual'"
                    .to_string(),
        }),
        Some(mode) => Ok(mode),
        None => Err(safety::GraphError::InvalidFilter {
            reason: format!(
                "unsupported graph.sync_mode '{}'; expected 'manual', 'trigger', or 'wal'",
                config::sync_mode()
            ),
        }),
    }
}

pub(crate) fn install_sync_triggers() -> safety::GraphResult<usize> {
    let (tables, _edges, filter_columns) = read_catalog()?;
    let mut installed = 0usize;
    for table in &tables {
        let oid = table_oid_from_name(&table.table_name)?;
        let qt = sync::get_qualified_table(oid)?;
        let mut trigger_columns = table.columns.clone();
        for filter in filter_columns
            .iter()
            .filter(|filter| filter.table_name == table.table_name)
        {
            if !trigger_columns
                .iter()
                .any(|column| column == &filter.column_name)
            {
                trigger_columns.push(filter.column_name.clone());
            }
        }
        if let Some(tenant_column) = &table.tenant_column {
            if !trigger_columns.iter().any(|column| column == tenant_column) {
                trigger_columns.push(tenant_column.clone());
            }
        }
        let trigger_sql = sync::generate_trigger_sql(&qt, &table.id_column, &trigger_columns);
        Spi::run(&trigger_sql).map_err(|e| {
            safety::GraphError::Internal(format!(
                "trigger creation failed for {}: {}",
                table.table_name, e
            ))
        })?;
        installed += 1;
    }

    Ok(installed)
}

pub(crate) fn remove_sync_triggers() -> safety::GraphResult<usize> {
    let (tables, _edges, _filter_columns) = read_catalog()?;
    let mut removed = 0usize;
    for table in &tables {
        let oid = table_oid_from_name(&table.table_name)?;
        let qt = sync::get_qualified_table(oid)?;
        let table_sql = sync::qualified_table_sql(&qt);
        Spi::run(&format!(
            "DROP TRIGGER IF EXISTS graph_sync_insert ON {table_sql};
             DROP TRIGGER IF EXISTS graph_sync_update ON {table_sql};
             DROP TRIGGER IF EXISTS graph_sync_delete ON {table_sql};
             DROP TRIGGER IF EXISTS graph_sync_truncate ON {table_sql};",
        ))
        .map_err(|err| {
            safety::GraphError::Internal(format!(
                "trigger removal failed for {}: {}",
                table.table_name, err
            ))
        })?;
        removed += 1;
    }

    Ok(removed)
}

pub(crate) fn disabled_graph_trigger_count() -> safety::GraphResult<i32> {
    Spi::connect(|client| {
        let result = client.select(
            "SELECT count(*)::int
             FROM pg_trigger
             WHERE tgname IN ('graph_sync_insert', 'graph_sync_update', 'graph_sync_delete', 'graph_sync_truncate')
               AND tgenabled = 'D'",
            None,
            &[],
        )?;
        Ok::<_, pgrx::spi::SpiError>(result.first().get::<i32>(1)?.unwrap_or(0))
    })
    .map_err(|e| safety::GraphError::Internal(format!("trigger status check failed: {}", e)))
}

pub(crate) fn pending_sync_rows(applied_sync_id: i64) -> safety::GraphResult<i64> {
    Spi::connect(|client| {
        let query = format!(
            "SELECT CASE
                WHEN to_regclass('graph._sync_log') IS NULL THEN 0::bigint
                ELSE (SELECT count(*)::bigint FROM graph._sync_log WHERE id > {})
             END",
            applied_sync_id
        );
        let result = client.select(&query, None, &[])?;
        Ok::<_, pgrx::spi::SpiError>(result.first().get::<i64>(1)?.unwrap_or(0))
    })
    .map_err(|e| safety::GraphError::Internal(format!("sync status check failed: {}", e)))
}

pub(crate) fn max_sync_log_id() -> safety::GraphResult<i64> {
    Spi::connect(|client| {
        let result = client.select(
            "SELECT CASE
                WHEN to_regclass('graph._sync_log') IS NULL THEN 0::bigint
                ELSE (SELECT COALESCE(max(id), 0)::bigint FROM graph._sync_log)
             END",
            None,
            &[],
        )?;
        Ok::<_, pgrx::spi::SpiError>(result.first().get::<i64>(1)?.unwrap_or(0))
    })
    .map_err(|e| safety::GraphError::Internal(format!("sync checkpoint read failed: {}", e)))
}

#[derive(Default)]
pub(crate) struct SyncApplyStats {
    pub(crate) inserts: i64,
    pub(crate) updates: i64,
    pub(crate) deletes: i64,
    pub(crate) truncates: i64,
}

pub(crate) struct SyncLogEntry {
    pub(crate) id: i64,
    pub(crate) op: String,
    pub(crate) table_oid: Option<u32>,
    pub(crate) table_name: String,
    pub(crate) old_pk: Option<String>,
    pub(crate) new_pk: Option<String>,
    pub(crate) properties: Option<String>,
    pub(crate) old_row: Option<String>,
    pub(crate) new_row: Option<String>,
}

fn required_sync_i64(value: Option<i64>, column: &str) -> safety::GraphResult<i64> {
    value.ok_or_else(|| {
        safety::GraphError::Internal(format!("sync row missing required column {column}"))
    })
}

fn required_sync_string(value: Option<String>, column: &str) -> safety::GraphResult<String> {
    value.ok_or_else(|| {
        safety::GraphError::Internal(format!("sync row missing required column {column}"))
    })
}

pub(crate) fn apply_sync_internal() -> safety::GraphResult<SyncApplyStats> {
    let mut stats = SyncApplyStats::default();
    let applied_sync_id = ENGINE.with(|e| {
        let eng = e.borrow();
        if eng.built {
            Ok(eng.applied_sync_id)
        } else {
            Err(safety::GraphError::NotBuilt)
        }
    })?;

    let log_entries = read_sync_log_entries(applied_sync_id)?;
    guard_edge_buffer_capacity_for_sync(&log_entries)?;
    for entry in log_entries {
        apply_sync_log_entry(&entry, &mut stats)?;
        ENGINE.with(|e| {
            e.borrow_mut().applied_sync_id = entry.id;
        });
    }

    apply_legacy_sync_buffer(&mut stats)?;

    let pending = ENGINE.with(|e| pending_sync_rows(e.borrow().applied_sync_id))?;
    ENGINE.with(|e| {
        let mut eng = e.borrow_mut();
        eng.pending_sync_rows = pending;
    });

    Ok(stats)
}

pub(crate) fn guard_edge_buffer_capacity_for_sync(
    entries: &[SyncLogEntry],
) -> safety::GraphResult<()> {
    if entries.is_empty() {
        return Ok(());
    }
    let (_tables, edges, _filters) = read_catalog()?;
    let edge_source_tables = edges
        .iter()
        .map(|edge| edge.from_table.as_str())
        .collect::<HashSet<_>>();
    let edge_source_oids = edges
        .iter()
        .filter_map(|edge| table_oid_from_name(&edge.from_table).ok())
        .collect::<HashSet<_>>();
    if edge_source_tables.is_empty() && edge_source_oids.is_empty() {
        return Ok(());
    }
    let estimated_edge_deltas = entries
        .iter()
        .filter(|entry| {
            entry
                .table_oid
                .is_some_and(|oid| edge_source_oids.contains(&oid))
                || edge_source_tables.contains(entry.table_name.as_str())
        })
        .map(|entry| match entry.op.trim() {
            "U" => 2usize,
            "I" | "D" => 1usize,
            _ => 0usize,
        })
        .sum::<usize>();
    if estimated_edge_deltas == 0 {
        return Ok(());
    }
    ENGINE.with(|e| {
        let mut eng = e.borrow_mut();
        let used = eng.edge_buffer.len();
        let limit = crate::config::EDGE_BUFFER_SIZE.get() as usize;
        if used.saturating_add(estimated_edge_deltas) > limit {
            eng.is_read_only = true;
            eng.sync_status = engine::SyncStatus::ReadOnly;
            return Err(safety::GraphError::EdgeBufferFull { size: used });
        }
        Ok(())
    })
}

pub(crate) fn read_sync_log_entries(
    applied_sync_id: i64,
) -> safety::GraphResult<Vec<SyncLogEntry>> {
    Spi::connect(|client| {
        let query = format!(
            "SELECT id, op::text, table_oid::oid::integer, table_name,
                    old_pk, new_pk, properties::text, old_row::text, new_row::text
             FROM graph._sync_log
             WHERE id > {}
             ORDER BY id",
            applied_sync_id
        );
        let result = client.select(&query, None, &[]);
        let Ok(rows) = result else {
            return Ok(Vec::new());
        };
        let mut entries = Vec::new();
        for row in rows {
            let table_oid = row
                .get::<i32>(3)
                .map_err(|e| {
                    safety::GraphError::Internal(format!("sync table_oid read failed: {e}"))
                })?
                .map(|oid| oid as u32);
            entries.push(SyncLogEntry {
                id: required_sync_i64(
                    row.get::<i64>(1).map_err(|e| {
                        safety::GraphError::Internal(format!("sync id read failed: {e}"))
                    })?,
                    "id",
                )?,
                op: required_sync_string(
                    row.get::<String>(2).map_err(|e| {
                        safety::GraphError::Internal(format!("sync op read failed: {e}"))
                    })?,
                    "op",
                )?,
                table_oid,
                table_name: required_sync_string(
                    row.get::<String>(4).map_err(|e| {
                        safety::GraphError::Internal(format!("sync table_name read failed: {e}"))
                    })?,
                    "table_name",
                )?,
                old_pk: row.get::<String>(5).map_err(|e| {
                    safety::GraphError::Internal(format!("sync old_pk read failed: {e}"))
                })?,
                new_pk: row.get::<String>(6).map_err(|e| {
                    safety::GraphError::Internal(format!("sync new_pk read failed: {e}"))
                })?,
                properties: row.get::<String>(7).map_err(|e| {
                    safety::GraphError::Internal(format!("sync properties read failed: {e}"))
                })?,
                old_row: row.get::<String>(8).map_err(|e| {
                    safety::GraphError::Internal(format!("sync old_row read failed: {e}"))
                })?,
                new_row: row.get::<String>(9).map_err(|e| {
                    safety::GraphError::Internal(format!("sync new_row read failed: {e}"))
                })?,
            });
        }
        Ok::<_, safety::GraphError>(entries)
    })
}

pub(crate) fn apply_sync_log_entry(
    entry: &SyncLogEntry,
    stats: &mut SyncApplyStats,
) -> safety::GraphResult<()> {
    let table_oid = match entry.table_oid {
        Some(oid) => oid,
        None => table_oid_from_name(&entry.table_name)?,
    };
    let parsed = parse_sync_properties(entry.properties.as_deref());
    let tenant = tenant_from_properties(table_oid, &parsed)?;

    ENGINE.with(|e| {
        let mut eng = e.borrow_mut();
        let (tables, edges, filters) = read_catalog()?;
        match entry.op.trim() {
            "I" => {
                let pk = entry
                    .new_pk
                    .as_deref()
                    .or(entry.old_pk.as_deref())
                    .ok_or_else(|| {
                        safety::GraphError::Internal(format!(
                            "sync row {} missing insert pk",
                            entry.id
                        ))
                    })?;
                sync::sync_insert(&mut eng, table_oid, pk, tenant.as_deref())?;
                refresh_filter_index_from_sync(&mut eng, table_oid, pk, &filters, entry)?;
                apply_row_edge_mutations(
                    &mut eng,
                    &tables,
                    &edges,
                    table_oid,
                    entry.new_row.as_deref(),
                    engine::MutationKind::Insert,
                )?;
                stats.inserts += 1;
            }
            "U" => {
                let old_pk = entry.old_pk.as_deref().ok_or_else(|| {
                    safety::GraphError::Internal(format!("sync row {} missing old_pk", entry.id))
                })?;
                let new_pk = entry.new_pk.as_deref().ok_or_else(|| {
                    safety::GraphError::Internal(format!("sync row {} missing new_pk", entry.id))
                })?;
                apply_row_edge_mutations(
                    &mut eng,
                    &tables,
                    &edges,
                    table_oid,
                    entry.old_row.as_deref(),
                    engine::MutationKind::Delete,
                )?;
                if old_pk == new_pk {
                    sync::sync_update(&mut eng, table_oid, new_pk, tenant.as_deref())?;
                    refresh_filter_index_from_sync(&mut eng, table_oid, new_pk, &filters, entry)?;
                } else {
                    sync::sync_replace_pk(&mut eng, table_oid, old_pk, new_pk, tenant.as_deref())?;
                    refresh_filter_index_from_sync(&mut eng, table_oid, new_pk, &filters, entry)?;
                }
                apply_row_edge_mutations(
                    &mut eng,
                    &tables,
                    &edges,
                    table_oid,
                    entry.new_row.as_deref(),
                    engine::MutationKind::Insert,
                )?;
                stats.updates += 1;
            }
            "D" => {
                let pk = entry
                    .old_pk
                    .as_deref()
                    .or(entry.new_pk.as_deref())
                    .ok_or_else(|| {
                        safety::GraphError::Internal(format!(
                            "sync row {} missing delete pk",
                            entry.id
                        ))
                    })?;
                apply_row_edge_mutations(
                    &mut eng,
                    &tables,
                    &edges,
                    table_oid,
                    entry.old_row.as_deref(),
                    engine::MutationKind::Delete,
                )?;
                sync::sync_delete(&mut eng, table_oid, pk)?;
                stats.deletes += 1;
            }
            "T" => {
                sync::sync_truncate(&mut eng, table_oid)?;
                stats.truncates += 1;
            }
            other => {
                return Err(safety::GraphError::Internal(format!(
                    "sync row {} has unsupported operation '{}'",
                    entry.id, other
                )));
            }
        }
        Ok::<_, safety::GraphError>(())
    })
}

fn refresh_filter_index_from_sync(
    eng: &mut engine::Engine,
    table_oid: u32,
    pk: &str,
    filters: &[builder::RegisteredFilterColumn],
    entry: &SyncLogEntry,
) -> safety::GraphResult<()> {
    let Some(node_idx) = eng.resolve(table_oid, pk) else {
        return Ok(());
    };
    let properties = parse_sync_properties(entry.properties.as_deref())
        .into_iter()
        .collect::<HashMap<_, _>>();
    let row = entry
        .new_row
        .as_deref()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok());

    for filter in filters {
        if table_oid_from_name(&filter.table_name)? != table_oid {
            continue;
        }
        let Some(column_idx) = eng.filter_index.find_column(&filter.column_name) else {
            continue;
        };
        let value = filter_value_from_row_or_properties(
            &filter.column_name,
            eng.filter_index.column_type(column_idx),
            row.as_ref(),
            &properties,
            &mut eng.filter_index,
            column_idx,
        )?;
        eng.filter_index
            .set_encoded_value(column_idx, node_idx, value);
    }

    Ok(())
}

fn filter_value_from_row_or_properties(
    column_name: &str,
    column_type: Option<FilterColumnType>,
    row: Option<&serde_json::Value>,
    properties: &HashMap<String, String>,
    filter_index: &mut crate::filter_index::FilterIndex,
    column_idx: usize,
) -> safety::GraphResult<Option<EncodedFilterValue>> {
    let raw = row
        .and_then(|row| row.get(column_name))
        .cloned()
        .or_else(|| {
            properties
                .get(column_name)
                .map(|value| serde_json::Value::String(value.clone()))
        });
    let Some(raw) = raw else {
        return Ok(None);
    };
    if raw.is_null() {
        return Ok(None);
    }
    let Some(column_type) = column_type else {
        return Ok(None);
    };
    match column_type {
        FilterColumnType::Numeric => Ok(Some(EncodedFilterValue::Numeric(json_value_i64(&raw)?))),
        FilterColumnType::Boolean => Ok(Some(EncodedFilterValue::Boolean(json_value_bool(&raw)?))),
        FilterColumnType::Text => {
            let value = json_value_text(&raw)?;
            let token = filter_index.intern_text_value(column_idx, &value);
            Ok(Some(EncodedFilterValue::Text(token)))
        }
        FilterColumnType::Date => Ok(Some(EncodedFilterValue::Date(encode_date_filter_value(
            &string_filter_value(&raw)?,
        )?))),
        FilterColumnType::Timestamptz => Ok(Some(EncodedFilterValue::Timestamptz(
            encode_timestamptz_filter_value(&string_filter_value(&raw)?)?,
        ))),
        FilterColumnType::Uuid => {
            let value = json_value_text(&raw)?;
            Ok(Some(EncodedFilterValue::Uuid(parse_uuid_u128(&value)?)))
        }
    }
}

fn string_filter_value(raw: &serde_json::Value) -> safety::GraphResult<serde_json::Value> {
    Ok(serde_json::Value::String(json_value_text(raw)?))
}

fn json_value_text(raw: &serde_json::Value) -> safety::GraphResult<String> {
    match raw {
        serde_json::Value::String(value) => Ok(value.clone()),
        other => Ok(other.to_string()),
    }
}

fn json_value_i64(raw: &serde_json::Value) -> safety::GraphResult<i64> {
    if let Some(value) = raw.as_i64() {
        return Ok(value);
    }
    let text = json_value_text(raw)?;
    text.parse::<i64>()
        .map_err(|_| safety::GraphError::InvalidFilter {
            reason: "numeric sync filter values must be signed 64-bit integers".to_string(),
        })
}

fn json_value_bool(raw: &serde_json::Value) -> safety::GraphResult<bool> {
    if let Some(value) = raw.as_bool() {
        return Ok(value);
    }
    let text = json_value_text(raw)?;
    text.parse::<bool>()
        .map_err(|_| safety::GraphError::InvalidFilter {
            reason: "boolean sync filter values must be true or false".to_string(),
        })
}

pub(crate) fn apply_row_edge_mutations(
    eng: &mut engine::Engine,
    tables: &[builder::RegisteredTable],
    edges: &[builder::RegisteredEdge],
    table_oid: u32,
    row_json: Option<&str>,
    kind: engine::MutationKind,
) -> safety::GraphResult<()> {
    let Some(row_json) = row_json else {
        return Ok(());
    };
    let row: serde_json::Value = serde_json::from_str(row_json).map_err(|e| {
        safety::GraphError::Internal(format!("sync row JSON parse failed for edge deltas: {}", e))
    })?;
    let all_oids = tables
        .iter()
        .filter_map(|table| table_oid_from_name(&table.table_name).ok())
        .collect::<Vec<_>>();

    for edge in edges {
        let from_oid = table_oid_from_name(&edge.from_table).ok();
        if from_oid != Some(table_oid) {
            continue;
        }
        let Some(from_table) = tables
            .iter()
            .find(|table| table.table_name == edge.from_table)
        else {
            continue;
        };
        let Some(from_pk) = row_pk_value(&row, &from_table.id_column) else {
            continue;
        };
        let Some(to_pk) = row_text_value(&row, &edge.from_column) else {
            continue;
        };
        let edge_label = edge
            .label_column
            .as_deref()
            .and_then(|column| row_text_value(&row, column))
            .filter(|label| !label.trim().is_empty())
            .unwrap_or_else(|| edge.label.clone());
        let type_id = eng.register_edge_type(&edge_label)?;
        let source = resolve_sync_endpoint(eng, from_oid, &from_pk, &all_oids);
        let target_oid = table_oid_from_name(&edge.to_table).ok();
        let target = resolve_sync_endpoint(eng, target_oid, &to_pk, &all_oids);
        if let (Some(source), Some(target)) = (source, target) {
            push_sync_edge_delta(eng, source, target, type_id, kind)?;
            if edge.bidirectional {
                push_sync_edge_delta(eng, target, source, type_id, kind)?;
            }
        }
    }
    Ok(())
}

pub(crate) fn push_sync_edge_delta(
    eng: &mut engine::Engine,
    source: u32,
    target: u32,
    type_id: u8,
    kind: engine::MutationKind,
) -> safety::GraphResult<()> {
    eng.push_edge_mutation(engine::EdgeMutation {
        source,
        target,
        type_id,
        kind,
    })
}

pub(crate) fn resolve_sync_endpoint(
    eng: &engine::Engine,
    preferred_oid: Option<u32>,
    pk: &str,
    all_oids: &[u32],
) -> Option<u32> {
    if let Some(oid) = preferred_oid {
        if let Some(idx) = eng.resolve(oid, pk) {
            return Some(idx);
        }
    }
    all_oids.iter().find_map(|&oid| eng.resolve(oid, pk))
}

pub(crate) fn row_pk_value(row: &serde_json::Value, id_column: &str) -> Option<String> {
    if id_column.contains(',') {
        let values = id_column
            .split(',')
            .map(str::trim)
            .map(|column| row_text_value(row, column))
            .collect::<Option<Vec<_>>>()?;
        Some(
            serde_json::Value::Array(values.into_iter().map(serde_json::Value::String).collect())
                .to_string(),
        )
    } else {
        row_text_value(row, id_column)
    }
}

pub(crate) fn row_text_value(row: &serde_json::Value, column: &str) -> Option<String> {
    let value = row.get(column)?;
    match value {
        serde_json::Value::Null => None,
        serde_json::Value::String(text) => Some(text.clone()),
        other => Some(other.to_string().trim_matches('"').to_string()),
    }
}

pub(crate) fn apply_legacy_sync_buffer(stats: &mut SyncApplyStats) -> safety::GraphResult<()> {
    type LegacySyncEntry = (i64, String, String, String, String, Option<String>);
    let entries: Vec<LegacySyncEntry> = Spi::connect(|client| {
        let result = client.select(
            "SELECT id, op::text, table_name,
                    COALESCE(old_pk, pk) AS old_pk,
                    COALESCE(new_pk, pk) AS new_pk,
                    properties::text
             FROM graph._sync_buffer
             ORDER BY id",
            None,
            &[],
        );
        let Ok(rows) = result else {
            return Ok(Vec::new());
        };
        let mut entries = Vec::new();
        for row in rows {
            entries.push((
                required_sync_i64(
                    row.get::<i64>(1).map_err(|e| {
                        safety::GraphError::Internal(format!("legacy sync id read failed: {e}"))
                    })?,
                    "id",
                )?,
                required_sync_string(
                    row.get::<String>(2).map_err(|e| {
                        safety::GraphError::Internal(format!("legacy sync op read failed: {e}"))
                    })?,
                    "op",
                )?,
                required_sync_string(
                    row.get::<String>(3).map_err(|e| {
                        safety::GraphError::Internal(format!(
                            "legacy sync table_name read failed: {e}"
                        ))
                    })?,
                    "table_name",
                )?,
                required_sync_string(
                    row.get::<String>(4).map_err(|e| {
                        safety::GraphError::Internal(format!("legacy sync old_pk read failed: {e}"))
                    })?,
                    "old_pk",
                )?,
                required_sync_string(
                    row.get::<String>(5).map_err(|e| {
                        safety::GraphError::Internal(format!("legacy sync new_pk read failed: {e}"))
                    })?,
                    "new_pk",
                )?,
                row.get::<String>(6).map_err(|e| {
                    safety::GraphError::Internal(format!("legacy sync properties read failed: {e}"))
                })?,
            ));
        }
        Ok::<_, safety::GraphError>(entries)
    })?;

    let mut applied_ids = Vec::new();
    for (id, op, table_name, old_pk, new_pk, properties) in entries {
        let entry = SyncLogEntry {
            id,
            op,
            table_oid: Some(table_oid_from_name(&table_name)?),
            table_name,
            old_pk: Some(old_pk),
            new_pk: Some(new_pk),
            properties,
            old_row: None,
            new_row: None,
        };
        match apply_sync_log_entry(&entry, stats) {
            Ok(()) => applied_ids.push(id),
            Err(err) => {
                pgrx::warning!(
                    "graph.apply_sync(): legacy sync row {} failed and remains buffered: {}",
                    id,
                    err
                );
            }
        }
    }

    if !applied_ids.is_empty() {
        let ids = applied_ids
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");
        Spi::run(&format!(
            "DELETE FROM graph._sync_buffer WHERE id IN ({})",
            ids
        ))
        .map_err(|e| {
            safety::GraphError::Internal(format!("legacy sync buffer cleanup failed: {}", e))
        })?;
    }

    Ok(())
}

pub(crate) fn tenant_from_properties(
    table_oid: u32,
    properties: &[(String, String)],
) -> safety::GraphResult<Option<String>> {
    let (tables, _edges, _filter_columns) = read_catalog()?;
    let Some(table) = tables.iter().find(|table| {
        table_oid_from_name(&table.table_name)
            .map(|oid| oid == table_oid)
            .unwrap_or(false)
    }) else {
        return Ok(None);
    };
    let Some(tenant_column) = &table.tenant_column else {
        return Ok(None);
    };
    Ok(properties
        .iter()
        .find(|(column, _)| column == tenant_column)
        .map(|(_, value)| value.clone()))
}

pub(crate) fn resolve_tenant_scope(
    explicit_tenant: Option<&str>,
) -> safety::GraphResult<Option<String>> {
    if let Some(tenant) = explicit_tenant
        .map(str::trim)
        .filter(|tenant| !tenant.is_empty())
    {
        return Ok(Some(tenant.to_string()));
    }

    let tenant_setting = config::tenant_setting();
    if !tenant_setting.trim().is_empty() {
        let query = format!(
            "SELECT current_setting({}, true)",
            quote_literal(&tenant_setting)
        );
        let session_tenant = Spi::connect(|client| {
            let result = client.select(&query, None, &[])?;
            Ok::<_, pgrx::spi::SpiError>(result.first().get::<String>(1)?.unwrap_or_default())
        })
        .map_err(|e| {
            safety::GraphError::Internal(format!("tenant session setting read failed: {}", e))
        })?;
        if !session_tenant.trim().is_empty() {
            return Ok(Some(session_tenant));
        }
    }

    if config::ENFORCE_TENANT_SCOPE.get() && graph_has_tenanted_tables()? {
        return Err(safety::GraphError::InvalidFilter {
            reason: "tenant scope is required for registered tables with tenant_column; pass tenant or configure graph.tenant_setting".to_string(),
        });
    }

    Ok(None)
}

pub(crate) fn graph_has_tenanted_tables() -> safety::GraphResult<bool> {
    let (tables, _edges, _filter_columns) = read_catalog()?;
    Ok(tables.iter().any(|table| table.tenant_column.is_some()))
}

pub(crate) fn parse_sync_properties(raw: Option<&str>) -> Vec<(String, String)> {
    let Some(raw) = raw else {
        return Vec::new();
    };
    let Ok(serde_json::Value::Object(map)) = serde_json::from_str(raw) else {
        return Vec::new();
    };

    map.into_iter()
        .filter_map(|(key, value)| match value {
            serde_json::Value::Null => None,
            serde_json::Value::String(s) => Some((key, s)),
            other => Some((key, other.to_string())),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{required_sync_i64, required_sync_string};
    use crate::safety::GraphError;

    #[test]
    fn required_sync_i64_rejects_null_structural_values() {
        assert_eq!(required_sync_i64(Some(42), "id").unwrap(), 42);

        let err = required_sync_i64(None, "id").unwrap_err();

        assert!(matches!(err, GraphError::Internal(_)));
        assert!(err.to_string().contains("id"));
    }

    #[test]
    fn required_sync_string_preserves_empty_strings_but_rejects_null() {
        assert_eq!(
            required_sync_string(Some(String::new()), "op").unwrap(),
            String::new()
        );
        assert_eq!(
            required_sync_string(Some("users".to_string()), "table_name").unwrap(),
            "users"
        );

        let err = required_sync_string(None, "table_name").unwrap_err();

        assert!(matches!(err, GraphError::Internal(_)));
        assert!(err.to_string().contains("table_name"));
    }
}
