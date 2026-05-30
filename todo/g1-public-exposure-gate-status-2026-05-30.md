# G1 Public-Exposure Gate Status - 2026-05-30

## Status

Blocked on required Cargo-backed verification. Do not flip `graph.gql()` or
`graph.gql_explain()` out of `development` until this file is superseded by
real test and benchmark evidence.

## Completed Evidence

- Phase 1A through 1D implementation commits exist on
  `feat/mutable-graph-projections`.
- DR-5 SQLSTATE taxonomy is implemented and committed:
  `12f75a8 feat(gql): map stable SQLSTATEs`.
- Tenant-scope topology filtering is implemented and committed:
  `078f094 fix(gql): enforce tenant scope`.
- Public positioning now says SQL functions are the current public API while
  GQL and SQL/PGQ are planned work:
  `1e57650 docs(gql): align public query language positioning`.
- Relationship identity projection is implemented for single-hop relationship
  variables, including inbound orientation preserving registered edge start/end.
- Static hygiene passed:
  - `rustfmt --edition 2021 --check graph/src/gql/*.rs graph/src/query/*.rs graph/src/sql_facade/gql.rs graph/src/pg_tests/gql.rs graph/src/lib.rs`
  - `git diff --check`
- Standalone non-Cargo SQLSTATE/ACL boundary passed:
  - `DBNAME=pggraph_boundary_g1 graph/tests/heavy/run_sqlstate_acl_boundary.sh`

## Missing Evidence

The G1 gate in `todo/build-sequence.md` requires:

- Completion or explicit re-scoping of the Phase 1 return-shape matrix. Current
  code covers node/property returns and single-hop relationship identity
  returns, but not path variables or relationship source-row hydration.
- Cargo tests for the Phase 1 read-only GQL implementation.
- pgrx tests for the development `graph.gql()` SQL facade.
- `bfs_bench` comparison against `pre_gql_mutable_overlay` with zero regression.

The required wrapper currently returns only:

```text
Protected by Socket Firewall
```

for commands such as:

```sh
sfw cargo test --features "pg17 development" query::
sfw cargo pgrx test --features "pg17 development" gql
sfw cargo bench --features pg17 --bench bfs_bench -- --baseline pre_gql_mutable_overlay
```

That output is not valid test or benchmark evidence. A direct `cargo test`
bypass was rejected because the repository instructions require `sfw` unless the
user explicitly approves bypassing it.

## Next Action

Get one of these before proceeding past G1:

1. an environment where `sfw cargo ...` runs Cargo and produces real output; or
2. explicit user approval to bypass `sfw` for the required Cargo verification
   commands.

After valid verification exists, record the results in
`todo/regression-baseline-2026-05-29.md` or a replacement baseline note, then
reconsider whether `graph.gql()` can be moved out of the `development` feature.
