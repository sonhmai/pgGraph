//! # ACL — Access Control List pre-flight checks
//!
//! Query helpers call `check_table_acl()` before reading source-table rows or
//! returning hydrated data. This verifies the current user has `SELECT`
//! privilege on the relevant table.
//!
//! See: `docs/contributor_guide/safety-security.mdx`

use crate::safety::{GraphError, GraphResult};

/// Check if the current user has SELECT privilege on the given table OID.
///
/// Uses PostgreSQL's native relation ACL checker.
///
/// # Errors
/// Returns `GraphError::AclDenied` if the user lacks SELECT on the table.
pub fn check_table_acl(table_oid: u32) -> GraphResult<()> {
    let acl_result = unsafe {
        // SAFETY: This runs inside a PostgreSQL backend process. `table_oid` is
        // an OID supplied by callers that already resolved catalog objects, and
        // the ACL checker does not take ownership of Rust-managed memory.
        pgrx::pg_sys::pg_class_aclcheck(
            pgrx::pg_sys::Oid::from_u32(table_oid),
            pgrx::pg_sys::GetUserId(),
            pgrx::pg_sys::ACL_SELECT as pgrx::pg_sys::AclMode,
        )
    };

    if acl_result != pgrx::pg_sys::AclResult::ACLCHECK_OK {
        return Err(GraphError::AclDenied {
            table: format!("OID {}", table_oid),
        });
    }
    Ok(())
}
