#!/usr/bin/env bash
set -euo pipefail

DBNAME="${DBNAME:-pggraph_cross_backend_durable}"
PG_VERSION_FEATURE="${PG_VERSION_FEATURE:-pg17}"
PG_MAJOR="${PG_VERSION_FEATURE#pg}"
PG_CONFIG="${PG_CONFIG:-}"
TMPDIR_ROOT="${TMPDIR:-/tmp}"
WORKDIR="$(mktemp -d "$TMPDIR_ROOT/pggraph-cross-backend-durable.XXXXXX")"

cleanup() {
  rm -rf "$WORKDIR"
}
trap cleanup EXIT

if [[ -z "$PG_CONFIG" ]]; then
  if [[ -x "/usr/lib/postgresql/${PG_MAJOR}/bin/pg_config" ]]; then
    PG_CONFIG="/usr/lib/postgresql/${PG_MAJOR}/bin/pg_config"
  elif [[ -x "/opt/homebrew/opt/postgresql@${PG_MAJOR}/bin/pg_config" ]]; then
    PG_CONFIG="/opt/homebrew/opt/postgresql@${PG_MAJOR}/bin/pg_config"
  else
    echo "PG_CONFIG is required for $PG_VERSION_FEATURE"
    exit 2
  fi
fi

cargo pgrx install --pg-config "$PG_CONFIG" \
  --features "$PG_VERSION_FEATURE pg_test" \
  --no-default-features
dropdb --if-exists "$DBNAME" >/dev/null 2>&1 || true
createdb "$DBNAME"

run_sql() {
  local sql="$1"
  psql -X -v ON_ERROR_STOP=1 -d "$DBNAME" -c "$sql" >/dev/null
}

expect_value() {
  local expected="$1"
  local sql="$2"
  local label="${3:-SQL value check}"
  local out

  out="$(psql -X -q -v ON_ERROR_STOP=1 -tA -d "$DBNAME" -c "
    DO \$\$ BEGIN PERFORM graph.test_enabled(); END \$\$;
    SET graph.mutable_enabled = on;
    SET graph.default_projection_mode = 'mutable_overlay';
    SET graph.persist_on_build = on;
    SET graph.sync_mode = 'trigger';
    $sql
  ")"
  if [[ "$out" != "$expected" ]]; then
    echo "$label expected '$expected' but got '$out' for SQL:"
    echo "$sql"
    exit 1
  fi
}

cat >"$WORKDIR/fixture.sql" <<'SQL'
CREATE EXTENSION IF NOT EXISTS graph;
SELECT graph.reset();
SET graph.mutable_enabled = on;
SET graph.persist_on_build = on;
SET graph.sync_mode = 'trigger';
DROP TABLE IF EXISTS public.graph_cross_backend_durable_nodes CASCADE;
CREATE TABLE public.graph_cross_backend_durable_nodes (
    id TEXT PRIMARY KEY,
    parent_id TEXT REFERENCES public.graph_cross_backend_durable_nodes(id),
    name TEXT NOT NULL
);
INSERT INTO public.graph_cross_backend_durable_nodes (id, parent_id, name)
VALUES ('root', NULL, 'Root'), ('child', NULL, 'Child');
SELECT graph.add_table(
    'public.graph_cross_backend_durable_nodes'::regclass,
    id_column := 'id',
    columns := ARRAY['name', 'parent_id']
);
SELECT graph.add_edge(
    'public.graph_cross_backend_durable_nodes'::regclass,
    'parent_id',
    'public.graph_cross_backend_durable_nodes'::regclass,
    'id',
    'parent',
    bidirectional := false
);
SELECT * FROM graph.build(mode := 'mutable_overlay');
SELECT graph.enable_sync();
SQL

psql -X -v ON_ERROR_STOP=1 -d "$DBNAME" -f "$WORKDIR/fixture.sql" >/dev/null
run_sql "ALTER DATABASE $DBNAME SET graph.mutable_enabled = on;"
run_sql "ALTER DATABASE $DBNAME SET graph.default_projection_mode = 'mutable_overlay';"
run_sql "ALTER DATABASE $DBNAME SET graph.persist_on_build = on;"
run_sql "ALTER DATABASE $DBNAME SET graph.sync_mode = 'trigger';"

# Session A commits a source-table edge update.
run_sql "UPDATE public.graph_cross_backend_durable_nodes SET parent_id = 'root' WHERE id = 'child';"

# Session B applies the committed sync log and must observe the durable segment
# without a full graph rebuild or backend-local edge buffer.
expect_value "1|0|0|1" "
  DO \$\$ BEGIN
    PERFORM 1 FROM graph.sync_health();
    PERFORM 1 FROM graph.apply_sync();
  END \$\$;
  WITH status AS (
    SELECT edge_buffer_used FROM graph.sync_health()
  ),
  projection AS (
    SELECT pending_durable_rows, (segment_count > 0)::int AS has_segment
    FROM graph.projection_status()
  ),
  visible AS (
    SELECT count(*)::bigint AS reaches_root
    FROM graph.traverse(
      'public.graph_cross_backend_durable_nodes'::regclass,
      'child',
      1,
      hydrate := false
    )
    WHERE node_id = 'root'
  )
  SELECT visible.reaches_root || '|' || status.edge_buffer_used || '|' ||
         projection.pending_durable_rows || '|' || projection.has_segment
  FROM visible, status, projection;" \
  "cross-backend durable apply"

expect_value "0" "
  SELECT count(*)::bigint
  FROM graph._sync_log
  WHERE id > (SELECT manifest_watermark FROM graph.projection_status());" \
  "durable projection watermark"

echo "Cross-backend durable projection checks passed on database: $DBNAME"
