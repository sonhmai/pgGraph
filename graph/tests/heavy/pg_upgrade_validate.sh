#!/usr/bin/env bash
set -euo pipefail

OLD_BINDIR="${OLD_BINDIR:?Path to old PostgreSQL bin directory is required}"
NEW_BINDIR="${NEW_BINDIR:?Path to new PostgreSQL bin directory is required}"
OLD_DATADIR="${OLD_DATADIR:?Path to old disposable PGDATA is required}"
NEW_DATADIR="${NEW_DATADIR:?Path to new disposable PGDATA is required}"
DBNAME="${DBNAME:-pggraph_upgrade}"

"$OLD_BINDIR/pg_ctl" -D "$OLD_DATADIR" -w start
"$OLD_BINDIR/createdb" "$DBNAME" || true
"$OLD_BINDIR/psql" -X -v ON_ERROR_STOP=1 "$DBNAME" <<'SQL'
CREATE EXTENSION IF NOT EXISTS graph;
SELECT graph.reset();
CREATE TABLE IF NOT EXISTS graph_upgrade_nodes (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    parent_id TEXT REFERENCES graph_upgrade_nodes(id)
);
TRUNCATE graph_upgrade_nodes;
INSERT INTO graph_upgrade_nodes VALUES ('root', 'Root', NULL), ('child', 'Child', 'root');
SELECT graph.add_table('graph_upgrade_nodes'::regclass, 'id', ARRAY['name']);
SELECT graph.add_edge('graph_upgrade_nodes'::regclass, 'parent_id', 'graph_upgrade_nodes'::regclass, 'id', 'parent', false);
SET graph.persist_on_build = on;
SELECT * FROM graph.build();
SQL
"$OLD_BINDIR/pg_ctl" -D "$OLD_DATADIR" -w stop

"$NEW_BINDIR/initdb" -D "$NEW_DATADIR"
"$NEW_BINDIR/pg_upgrade" \
  --old-bindir="$OLD_BINDIR" \
  --new-bindir="$NEW_BINDIR" \
  --old-datadir="$OLD_DATADIR" \
  --new-datadir="$NEW_DATADIR" \
  --check
"$NEW_BINDIR/pg_upgrade" \
  --old-bindir="$OLD_BINDIR" \
  --new-bindir="$NEW_BINDIR" \
  --old-datadir="$OLD_DATADIR" \
  --new-datadir="$NEW_DATADIR"

"$NEW_BINDIR/pg_ctl" -D "$NEW_DATADIR" -w start
"$NEW_BINDIR/psql" -X -v ON_ERROR_STOP=1 "$DBNAME" <<'SQL'
CREATE EXTENSION IF NOT EXISTS graph;
SET graph.auto_load = on;
SELECT node_count, edge_count FROM graph.status();
SELECT count(*) FROM graph.search('Child', table_filter := 'graph_upgrade_nodes'::regclass);
SQL
"$NEW_BINDIR/pg_ctl" -D "$NEW_DATADIR" -w stop

echo "pg_upgrade validation passed from $OLD_BINDIR to $NEW_BINDIR"
