# pgrx SQL Tests

Sticky note for contributors and agents: the maintained test inventory lives in
[SQL Tests](../../../docs/contributor_guide/sql-tests.mdx).

This directory contains pgrx `#[pg_test]` modules. Use it for installed
extension behavior that can run inside the pgrx-managed PostgreSQL test
cluster. The tests create their own source tables and fixtures; they should not
depend on external datasets.

Run from `graph/`:

```bash
cargo pgrx test pg17
```

For client-visible SQLSTATEs, role-switching boundaries, crash recovery,
packaging, backup/restore, Docker, upgrade, and concurrency checks, use
`graph/tests/heavy/` instead.
