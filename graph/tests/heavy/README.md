# Heavy SQL Tests

Sticky note for contributors and agents: the maintained inventory and layer
selection guide lives in [SQL Tests](../../../docs/contributor_guide/sql-tests.mdx).

Use these scripts when `cargo pgrx test` is not representative enough: client
SQLSTATEs, ACL/RLS role boundaries, crash recovery, backup/restore, package
validation, Docker, pg_upgrade, memory evidence, playground query stability, or
mixed concurrency.

Most scripts expect a disposable database and accept variables such as
`PG_VERSION_FEATURE=pg17`, `PG_CONFIG`, and `DBNAME`. Scripts that kill or
upgrade PostgreSQL require disposable `PGDATA` directories.
