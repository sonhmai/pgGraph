# Contributing to pgGraph

pgGraph is a PostgreSQL extension written in Rust with pgrx. The authoritative
product and implementation documentation lives in `docs/user_guide` and
`docs/contributor_guide`. Implementation work should keep code, SQL behavior,
and docs aligned.

## Development Setup

```bash
git clone https://github.com/evokoa/pggraph.git
cd pggraph/graph
cargo pgrx init
cargo test --features pg17
cargo pgrx test pg17
```

Use PostgreSQL 13 through 18 when validating compatibility-sensitive changes.
The default local feature is `pg17`.

## Pull Request Bar

- Keep SQL APIs aligned with `docs/user_guide/api-reference.mdx`.
- Add Rust unit tests for engine/data-structure changes.
- Add pgrx SQL tests for public SQL behavior, ACLs, SQLSTATEs, sync, and
  persistence behavior.
- Run `cargo fmt --check` before submitting.
- Include Criterion or SQL benchmark results for performance-sensitive changes.
- Update user-guide docs for SQL or operational behavior changes.
- Update contributor-guide docs for storage, loader, memory model, safety, or
  internal architecture changes.

## Scope

Accepted changes include bug fixes, crash-safety improvements, SQL API
conformance, memory/performance improvements, tests, docs, examples, and
benchmarking improvements.

Out of scope for V1 are new graph query languages, distributed consensus,
runtime dependencies outside PostgreSQL/Rust, and features that require a
separate graph service outside PostgreSQL.
