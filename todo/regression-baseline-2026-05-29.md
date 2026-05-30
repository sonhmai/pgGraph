# Regression Baseline: 2026-05-29

> Reminder: delete this tracking file before merging `feat/mutable-graph-projections` into `main`.

## Context

- Branch: `feat/mutable-graph-projections`
- Git commit: `0574e6b`
- Captured at: `2026-05-29T18:27:59Z`
- Purpose: baseline current regression state before GQL, SQL/PGQ, optional
  openCypher compatibility, and mutable overlay planning work turns into
  implementation.

## Worktree State

Tracked dirty file not included in this baseline work:

- `docs/known-issues.mdx`

New baseline/planning files under `todo/` are intended to be tracked on this
branch and deleted before merging to `main`.

## Baseline Commands

### Cargo Test

Command:

```sh
cd graph
sfw cargo test --features pg17
```

Result:

```text
test result: ok. 288 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 5.34s
Doc-tests graph: 0 passed; 0 failed
```

Exit code: `0`

Notes:

- This command was run outside the sandbox with user approval.
- Socket Firewall reported no package fetch attempts.

### pgrx SQL Suite

Command:

```sh
cd graph
sfw cargo pgrx test pg17
```

Result:

```text
test result: ok. 393 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 10.85s
Doc-tests graph: 0 passed; 0 failed
```

Notes:

- This command was run outside the sandbox with user approval.
- Socket Firewall reported no package fetch attempts.
- pgrx used PostgreSQL 17 from
  `/opt/homebrew/opt/postgresql@17/bin/pg_config`.

## Superseded Earlier Attempt

An earlier sandbox-bound attempt used an already-built test binary directly:

```sh
graph/target/debug/deps/graph-022f1ec96eb5a8d6 --nocapture
```

Result:

```text
test result: FAILED. 286 passed; 107 failed; 1 ignored; 0 measured; 0 filtered out; finished in 5.02s
```

Primary failure reason:

```text
Could not initialize test framework: failed to bind to an ephemeral port for test Postgres
Caused by:
    Operation not permitted (os error 1)
```

Follow-on failures were pgrx test mutex failures after the first pgrx framework
initialization failure.

## Comparison Guidance

For later regression comparison, use:

```sh
cd graph
cargo test --features pg17
cargo pgrx test pg17
```

Expected baseline:

```text
cargo test: 288 passed; 0 failed; 1 ignored
cargo pgrx test pg17: 393 passed; 0 failed; 1 ignored
```

## Performance Baseline Commands

### Criterion Engine Benchmarks

Command:

```sh
cd graph
cargo bench --features pg17 --bench bfs_bench -- --save-baseline pre_gql_mutable_overlay
```

Result: exit code `0`.

Criterion saved the baseline under `graph/target/criterion/**/pre_gql_mutable_overlay`.

Note: this historical baseline was captured through `sfw cargo`. Current policy
requires `sfw` only for dependency-changing operations; direct `cargo bench` is
the comparison command.

Selected results:

| Benchmark | Median |
|---|---:|
| `bfs_traverse/d1_supernode/10k` | `1.9342 us` |
| `bfs_traverse/d3_supernode/10k` | `58.937 us` |
| `bfs_traverse/d5_supernode/10k` | `486.06 us` |
| `bfs_traverse/d3_leaf/10k` | `6.3978 us` |
| `bfs_traverse/d1_supernode/100k` | `8.3883 us` |
| `bfs_traverse/d3_supernode/100k` | `87.302 us` |
| `bfs_traverse/d5_supernode/100k` | `1.6485 ms` |
| `bfs_traverse/d3_leaf/100k` | `9.8151 us` |
| `bfs_traverse/d1_supernode/500k` | `35.497 us` |
| `bfs_traverse/d3_supernode/500k` | `122.04 us` |
| `bfs_traverse/d5_supernode/500k` | `4.9527 ms` |
| `bfs_traverse/d3_leaf/500k` | `42.524 us` |
| `bfs_traverse/d1_supernode/2M_panama` | `137.16 us` |
| `bfs_traverse/d3_supernode/2M_panama` | `253.50 us` |
| `bfs_traverse/d5_supernode/2M_panama` | `16.633 ms` |
| `bfs_traverse/d3_leaf/2M_panama` | `148.63 us` |
| `graph_construction/build/10k` | `1.8116 ms` |
| `graph_construction/build/100k` | `20.736 ms` |
| `graph_construction/build/500k` | `111.54 ms` |
| `bfs_overlay_paths/no_overlay_d3` | `87.061 us` |
| `bfs_overlay_paths/sparse_overlay_d3` | `86.844 us` |
| `bfs_overlay_paths/dense_overlay_d3` | `100.60 us` |
| `bfs_filter_index_paths/score_gte_50_d3/sparse_10pct` | `7.9708 us` |
| `bfs_filter_index_paths/score_gte_50_d3/dense_100pct` | `19.649 us` |

Notes:

- This is the engine-level baseline for CSR traversal, current overlay paths,
  filter-index traversal, and synthetic graph construction.
- Socket Firewall reported no package fetch attempts.

### SQL-Facing Panama Benchmark

Command:

```sh
sandbox/run_benchmarks.sh panama --yes
```

Result: exit code `0`.

Report:

```text
sandbox/benchmark/results/20260529T184114Z/report.json
```

Dataset/build:

```text
nodes: 2,016,523
edges reported by graph.status(): 5,802,586
load_seconds: 63.316369375010254
build_seconds: 60.009269375004806
build_output build_time_ms: 39552.027227
build_output memory_used_mb: 185.5018720626831
status memory_used_mb: 170.96454048156738
```

Summary:

| Query | Cold median | Hot median |
|---|---:|---:|
| `status` | `557.303 ms` | `26.968 ms` |
| `entity_search` | `613.355 ms` | `77.892 ms` |
| `traverse_depth_2` | `583.577 ms` | `102.547 ms` |
| `shortest_path` | `461.034 ms` | `2.984 ms` |
| `component_stats` | `595.683 ms` | `142.487 ms` |
| `largest_component` | `1014.377 ms` | `541.423 ms` |

### SQL-Facing LDBC Benchmark

Command:

```sh
sandbox/run_benchmarks.sh ldbc --yes
```

Result: exit code `0`.

Report:

```text
sandbox/benchmark/results/20260529T184427Z/report.json
```

Dataset/build:

```text
nodes: 3,181,724
edges reported by graph.status(): 34,512,076
transform_seconds: 41.985063125001034
load_seconds: 186.30184433297836
build_seconds: 210.02369754199754
build_output build_time_ms: 184604.9585
build_output memory_used_mb: 733.198676109314
status memory_used_mb: 511.5718116760254
```

Summary:

| Query | Cold median | Hot median |
|---|---:|---:|
| `status` | `2780.355 ms` | `27.988 ms` |
| `person_search` | `2689.049 ms` | `7.900 ms` |
| `friend_traversal_depth_1` | `2741.755 ms` | `32.837 ms` |
| `person_content_neighborhood` | `2971.883 ms` | `171.596 ms` |
| `forum_neighborhood` | `2901.672 ms` | `175.667 ms` |
| `post_to_tag_path` | `2795.113 ms` | `6.267 ms` |
| `tag_to_tagclass_path` | `2757.831 ms` | `5.872 ms` |
| `component_stats` | `3184.236 ms` | `422.177 ms` |

### Build RSS Heavy Memory Script

Command:

```sh
cd graph
bash -lc 'function cargo(){ sfw cargo "$@"; }; export -f cargo; PG_VERSION_FEATURE=pg17 DBNAME=pggraph_gql_baseline_rss NODE_COUNT=200000 EDGE_COUNT=199999 ./tests/heavy/measure_build_rss.sh'
```

Result: exit code `0`.

The shell function ensures the script's internal `cargo pgrx install` call is
forced through `sfw cargo`.

Result:

```text
graph.build(): 200000 nodes, 199999 edges, build_time_ms=3595.534084, memory_used_mb=15.088253021240234, sync_mode=manual
Peak backend RSS: 143MB
Database size: 39.6MB
Temp bytes recorded: 86548278
```

### mmap PSS Heavy Memory Script

Command:

```sh
cd graph
DBNAME=pggraph_gql_baseline_rss BACKENDS=3 SLEEP_SECONDS=5 ./tests/heavy/measure_mmap_pss.sh
```

Result: exit code `2`.

Expected host limitation:

```text
measure_mmap_pss.sh requires Linux /proc/<pid>/smaps_rollup for PSS accounting.
macOS RSS/vmmap can confirm file mappings, but cannot prove shared page-cache cost.
```

Run this on Linux before making claims about shared mmap page-cache cost across
multiple PostgreSQL backends.
