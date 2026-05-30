//! Logical plan produced by GQL semantic binding.

/// Bound read-only logical query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LogicalPlan {
    /// Source node binding.
    pub(crate) source: BoundNode,
    /// Single relationship expansion.
    pub(crate) relationship: BoundRel,
    /// Target node binding.
    pub(crate) target: BoundNode,
    /// Return slots in requested order.
    pub(crate) returns: Vec<ReturnBinding>,
}

/// Bound node variable and table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BoundNode {
    /// GQL variable name.
    pub(crate) var: String,
    /// GQL label text.
    pub(crate) label: String,
    /// Backing source table OID.
    pub(crate) table_oid: u32,
}

/// Bound relationship type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BoundRel {
    /// GQL relationship type text.
    pub(crate) rel_type: String,
}

/// Bound `RETURN` variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReturnBinding {
    /// Source node variable.
    Source { name: String },
    /// Target node variable.
    Target { name: String },
}
