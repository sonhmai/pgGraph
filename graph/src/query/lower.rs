//! Lowering from logical GQL plans to physical CSR plans.

use super::logical_plan::{LogicalPlan, ReturnBinding};
use super::physical_plan::{PhysicalPlan, ReturnSlot};

/// Lower a bound logical plan into the executable Phase 1B physical plan.
pub(crate) fn lower(plan: LogicalPlan) -> PhysicalPlan {
    PhysicalPlan {
        source_var: plan.source.var,
        source_table_oid: plan.source.table_oid,
        rel_type: plan.relationship.rel_type,
        target_var: plan.target.var,
        target_table_oid: plan.target.table_oid,
        returns: plan
            .returns
            .into_iter()
            .map(|slot| match slot {
                ReturnBinding::Source { name } => ReturnSlot::Source { name },
                ReturnBinding::Target { name } => ReturnSlot::Target { name },
            })
            .collect(),
    }
}
