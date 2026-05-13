# pg_regress Smoke

Sticky note for contributors and agents: use
[SQL Tests](../../../docs/contributor_guide/sql-tests.mdx) for the current SQL
test inventory.

This directory is only a minimal extension setup smoke for PostgreSQL
packaging-style checks. It is not the primary SQL behavior suite. Use
`graph/src/pg_tests/` for pgrx SQL behavior tests and `graph/tests/heavy/` for
client/server and operational boundaries.
