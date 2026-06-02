# Measurements

## 2026-06-02 Phase 1 Parser Slice

- `cargo fmt` from `graph/`: passed.
- `cargo test --features pg17 gql::` from `graph/`: passed, 24 tests.

## 2026-06-02 Phase 1 Binder Slice

- `cargo fmt` from `graph/`: passed.
- `cargo test --features pg17 query::tests::binder_` from `graph/`: passed, 48 tests.

## 2026-06-02 Phase 1 Executor and SQL Slice

- `cargo fmt --check` from `graph/`: passed.
- `cargo test --features pg17 query::tests::binder_rejects_unsupported_wildcard_path_shapes` from `graph/`: passed, 1 test.
- `git diff --check` from repository root: passed.
- `cargo test --features pg17 query::tests` from `graph/`: passed, 99 tests.
- `cargo test --features pg17 gql::` from `graph/`: passed, 24 tests.
- `cargo test --features pg17` from `graph/`: passed, 453 tests, 1 ignored.
- `cargo pgrx test --features "pg17 development" gql_wildcard_path_values_and_functions_have_stable_shape` from `graph/`: passed, 1 pgrx test.

## 2026-06-02 Phase 2 Named Element Slice

- `cargo fmt --check` from `graph/`: passed.
- `git diff --check` from repository root: passed.
- `cargo test --features pg17 query::tests::` from `graph/`: passed, 102 tests.
- `cargo test --features pg17 query::tests::wildcard_path_` from `graph/`: passed, 5 tests.
- `cargo test --features pg17` from `graph/`: passed, 458 tests, 1 ignored.
- `cargo pgrx test --features "pg17 development" gql_wildcard_path_values_and_functions_have_stable_shape` from `graph/`: passed, 1 pgrx test.
