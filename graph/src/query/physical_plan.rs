//! Physical plans executable against immutable CSR stores.

/// Single-hop physical plan for Phase 1B.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PhysicalPlan {
    /// Source node variable.
    pub(crate) source_var: String,
    /// Source table OID.
    pub(crate) source_table_oid: u32,
    /// Relationship type label.
    pub(crate) rel_type: String,
    /// Target node variable.
    pub(crate) target_var: String,
    /// Target table OID.
    pub(crate) target_table_oid: u32,
    /// Return slots in requested order.
    pub(crate) returns: Vec<ReturnSlot>,
}

/// Physical return slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReturnSlot {
    /// Source coordinate.
    Source { name: String },
    /// Target coordinate.
    Target { name: String },
}

impl PhysicalPlan {
    /// Table OIDs whose rows must be visible to the current SQL role.
    pub(crate) fn required_table_oids(&self) -> [u32; 2] {
        [self.source_table_oid, self.target_table_oid]
    }
}
