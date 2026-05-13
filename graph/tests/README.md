# SQL Test Directories

Sticky note for contributors and agents: the main overview is
[SQL Tests](../../docs/contributor_guide/sql-tests.mdx).

- `heavy/` contains shell-driven SQL and operational tests that need real
  client connections, disposable databases, Docker, or disposable PostgreSQL
  clusters.
- `pg_regress/` contains a minimal extension setup smoke.

The pgrx SQL tests live under `graph/src/pg_tests/`.
