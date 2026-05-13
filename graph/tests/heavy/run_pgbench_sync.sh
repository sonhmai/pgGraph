#!/usr/bin/env bash
set -euo pipefail

DBNAME="${DBNAME:-postgres}"
CLIENTS="${CLIENTS:-16}"
JOBS="${JOBS:-4}"
TIME="${TIME:-120}"
RATE="${RATE:-100}"
MAX_APPLY_MS="${MAX_APPLY_MS:-5000}"
MAX_QUERY_MS="${MAX_QUERY_MS:-250}"
MIN_ROWS_APPLIED="${MIN_ROWS_APPLIED:-0}"
CREATE_DB="${CREATE_DB:-1}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [[ "$CREATE_DB" == "1" && "$DBNAME" != "postgres" ]]; then
  dropdb --if-exists "$DBNAME" >/dev/null 2>&1 || true
  createdb "$DBNAME"
fi

psql -X -v ON_ERROR_STOP=1 "$DBNAME" <<'SQL'
CREATE EXTENSION IF NOT EXISTS graph;
SELECT graph.reset();
DROP TABLE IF EXISTS public.graph_pgbench_edges CASCADE;
DROP TABLE IF EXISTS public.graph_pgbench_nodes CASCADE;
DROP SEQUENCE IF EXISTS public.graph_pgbench_node_seq;
CREATE TABLE public.graph_pgbench_nodes (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    score INT NOT NULL
);
CREATE SEQUENCE public.graph_pgbench_node_seq START WITH 10000001;
CREATE TABLE public.graph_pgbench_edges (
    id BIGSERIAL PRIMARY KEY,
    from_id TEXT NOT NULL REFERENCES public.graph_pgbench_nodes(id) ON UPDATE CASCADE ON DELETE CASCADE,
    to_id TEXT NOT NULL REFERENCES public.graph_pgbench_nodes(id) ON UPDATE CASCADE ON DELETE CASCADE,
    weight INT NOT NULL DEFAULT 1
);
INSERT INTO public.graph_pgbench_nodes (id, name, score)
SELECT i::text, 'seed-' || i::text, i % 1000
FROM generate_series(1, 10000) AS i;
INSERT INTO public.graph_pgbench_edges (from_id, to_id, weight)
SELECT i::text, (i + 1)::text, 1
FROM generate_series(1, 9999) AS i;
SELECT graph.add_table('public.graph_pgbench_nodes'::regclass, 'id', ARRAY['name', 'score']);
SELECT graph.add_edge('public.graph_pgbench_edges'::regclass, 'from_id', 'public.graph_pgbench_nodes'::regclass, 'id', 'pgbench', false, 'weight');
SET graph.persist_on_build = on;
SELECT * FROM graph.build();
SELECT graph.enable_sync();
SQL

pgbench "$DBNAME" \
  --client="$CLIENTS" \
  --jobs="$JOBS" \
  --time="$TIME" \
  --rate="$RATE" \
  --file="$SCRIPT_DIR/pgbench_sync.sql"

apply_ms="$(
  psql -X -q -v ON_ERROR_STOP=1 -tA "$DBNAME" <<'SQL'
SET graph.auto_load = on;
WITH started AS (SELECT clock_timestamp() AS ts),
     loaded AS (
       SELECT count(*) AS rows_seen
       FROM graph.traverse('public.graph_pgbench_nodes'::regclass, '1', 1, hydrate := false)
     ),
     applied AS (SELECT a.* FROM graph.apply_sync() AS a, loaded WHERE loaded.rows_seen >= 0),
     finished AS (SELECT clock_timestamp() AS ts)
SELECT ((EXTRACT(EPOCH FROM (finished.ts - started.ts)) * 1000)::bigint)::text
       || '|' || applied.inserts_applied::text
       || '|' || applied.updates_applied::text
       || '|' || applied.deletes_applied::text
FROM started, loaded, applied, finished;
SQL
)"
IFS='|' read -r apply_duration inserts updates deletes <<<"$apply_ms"
rows_applied=$((inserts + updates + deletes))

if (( rows_applied < MIN_ROWS_APPLIED )); then
  echo "Expected at least $MIN_ROWS_APPLIED sync rows applied, got $rows_applied"
  exit 1
fi
if (( apply_duration > MAX_APPLY_MS )); then
  echo "apply_sync exceeded threshold: ${apply_duration}ms > ${MAX_APPLY_MS}ms"
  exit 1
fi

query_ms="$(
  psql -X -q -v ON_ERROR_STOP=1 -tA "$DBNAME" <<'SQL'
SET graph.auto_load = on;
WITH started AS (SELECT clock_timestamp() AS ts),
     loaded AS (
       SELECT count(*) AS rows_seen
       FROM graph.status()
     ),
     traversed AS (
       SELECT count(*) AS rows_seen
       FROM graph.traverse('public.graph_pgbench_nodes'::regclass, '1', 3, hydrate := false)
     ),
     searched AS (
       SELECT count(*) AS rows_seen
       FROM graph.search('name', 'renamed', table_filter := 'public.graph_pgbench_nodes'::regclass, max_rows := 20)
     ),
     finished AS (SELECT clock_timestamp() AS ts)
SELECT ((EXTRACT(EPOCH FROM (finished.ts - started.ts)) * 1000)::bigint)::text
       || '|' || traversed.rows_seen::text
       || '|' || searched.rows_seen::text
FROM started, loaded, traversed, searched, finished;
SQL
)"
IFS='|' read -r query_duration traversal_rows search_rows <<<"$query_ms"

if (( traversal_rows < 1 )); then
  echo "Expected traversal rows after sync stress, got $traversal_rows"
  exit 1
fi
if (( query_duration > MAX_QUERY_MS )); then
  echo "post-sync query smoke exceeded threshold: ${query_duration}ms > ${MAX_QUERY_MS}ms"
  exit 1
fi

sync_log_rows="$(psql -X -q -v ON_ERROR_STOP=1 -tA "$DBNAME" -c "SELECT count(*) FROM graph._sync_log")"
if (( sync_log_rows < 1 )); then
  echo "Expected pgbench workload to create durable sync log rows"
  exit 1
fi

psql -X -v ON_ERROR_STOP=1 "$DBNAME" <<SQL
SET graph.auto_load = on;
SELECT node_count, edge_count, sync_status, pending_sync_rows
FROM graph.status();
SQL

echo "pgbench sync stress passed: sync_log_rows=$sync_log_rows rows_applied=$rows_applied apply_ms=$apply_duration query_ms=$query_duration search_rows=$search_rows"
