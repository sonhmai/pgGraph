//! Physical GQL plan execution against immutable engine stores.

use crate::engine::Engine;
use crate::safety::{GraphError, GraphResult};

use super::physical_plan::{PhysicalPlan, ReturnSlot};

/// Coordinate-only node value returned by Phase 1B.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GqlNodeCoordinate {
    /// Backing source table OID.
    pub(crate) table_oid: u32,
    /// Source row primary-key text.
    pub(crate) node_id: String,
}

/// Named value in a result row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GqlValue {
    /// Return column name.
    pub(crate) name: String,
    /// Coordinate-only node value.
    pub(crate) coordinate: GqlNodeCoordinate,
}

/// One GQL result row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GqlRow {
    /// Values in requested `RETURN` order.
    pub(crate) values: Vec<GqlValue>,
}

/// Execute a physical one-hop plan.
///
/// # Errors
///
/// Returns [`GraphError`] when the graph is not built or the requested
/// relationship type is not present in the built engine registry.
pub(crate) fn execute(engine: &Engine, plan: &PhysicalPlan) -> GraphResult<Vec<GqlRow>> {
    if !engine.built {
        return Err(GraphError::NotBuilt);
    }
    let rel_type_id = edge_type_id(engine, &plan.rel_type)?;
    let mut rows = Vec::new();
    for source_idx in source_nodes(engine, plan.source_table_oid) {
        if !engine.node_store.is_active(source_idx) {
            continue;
        }
        let (targets, type_ids) = engine.edge_store.neighbors(source_idx);
        for (&target_idx, &type_id) in targets.iter().zip(type_ids.iter()) {
            if type_id == rel_type_id && target_matches(engine, target_idx, plan.target_table_oid) {
                rows.push(project_row(engine, plan, source_idx, target_idx));
            }
        }
    }
    Ok(rows)
}

fn edge_type_id(engine: &Engine, rel_type: &str) -> GraphResult<u8> {
    engine
        .edge_type_registry
        .iter()
        .position(|label| label == rel_type)
        .map(|idx| idx as u8)
        .ok_or_else(|| GraphError::InvalidFilter {
            reason: format!("relationship type `{rel_type}` is not present in the built graph"),
        })
}

fn source_nodes(engine: &Engine, table_oid: u32) -> Vec<u32> {
    if let Some(nodes) = engine.table_membership.get(&table_oid) {
        return nodes.iter().collect();
    }
    (0..engine.node_store.node_count())
        .filter(|&idx| engine.node_store.table_oid(idx) == table_oid)
        .collect()
}

fn target_matches(engine: &Engine, target_idx: u32, table_oid: u32) -> bool {
    target_idx < engine.node_store.node_count()
        && engine.node_store.is_active(target_idx)
        && engine.node_store.table_oid(target_idx) == table_oid
}

fn project_row(engine: &Engine, plan: &PhysicalPlan, source_idx: u32, target_idx: u32) -> GqlRow {
    let values = plan
        .returns
        .iter()
        .map(|slot| match slot {
            ReturnSlot::Source { name } => GqlValue {
                name: name.clone(),
                coordinate: coordinate(engine, source_idx),
            },
            ReturnSlot::Target { name } => GqlValue {
                name: name.clone(),
                coordinate: coordinate(engine, target_idx),
            },
        })
        .collect();
    GqlRow { values }
}

fn coordinate(engine: &Engine, node_idx: u32) -> GqlNodeCoordinate {
    GqlNodeCoordinate {
        table_oid: engine.node_store.table_oid(node_idx),
        node_id: engine.node_store.primary_key(node_idx).to_string(),
    }
}
