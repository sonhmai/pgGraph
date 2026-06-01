#!/usr/bin/env bash
set -euo pipefail

DBNAME="${DBNAME:-pggraph_metadata}"
PG_VERSION_FEATURE="${PG_VERSION_FEATURE:-pg17}"
PG_MAJOR="${PG_VERSION_FEATURE#pg}"
PG_CONFIG="${PG_CONFIG:-}"

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

cargo pgrx install --pg-config "$PG_CONFIG" --features "$PG_VERSION_FEATURE" --no-default-features
dropdb --if-exists "$DBNAME" >/dev/null 2>&1 || true
createdb "$DBNAME"

psql "$DBNAME" -v ON_ERROR_STOP=1 -c "CREATE EXTENSION IF NOT EXISTS graph"

violations="$(psql "$DBNAME" -qAt <<'SQL'
WITH exported AS (
    SELECT p.oid,
           p.proname,
           pg_get_function_identity_arguments(p.oid) AS args,
           p.provolatile,
           p.proparallel,
           p.prosecdef,
           p.proleakproof,
           p.procost,
           p.prorows
    FROM pg_proc p
    JOIN pg_namespace n ON n.oid = p.pronamespace
    WHERE n.nspname = 'graph'
),
violations AS (
    SELECT format('%s(%s): security definer is not expected', proname, args) AS problem
    FROM exported
    WHERE prosecdef

    UNION ALL
    SELECT format('%s(%s): leakproof is not expected', proname, args)
    FROM exported
    WHERE proleakproof

    UNION ALL
    SELECT format('%s(%s): traversal set-returning function needs non-default COST', proname, args)
    FROM exported
    WHERE proname = 'traverse'
      AND procost <= 100

    UNION ALL
    SELECT format('%s(%s): traversal set-returning function needs ROWS estimate', proname, args)
    FROM exported
    WHERE proname = 'traverse'
      AND prorows <= 0

    UNION ALL
    SELECT format('%s(%s): mutation/admin function must be volatile', proname, args)
    FROM exported
    WHERE proname IN (
        'add_table', 'add_edge', 'reset', 'build', 'vacuum', 'maintenance',
        'apply_sync', 'enable_sync', 'disable_sync', 'enable', 'disable', 'gql'
    )
      AND provolatile <> 'v'
)
SELECT problem FROM violations ORDER BY problem;
SQL
)"

if [[ -n "$violations" ]]; then
  echo "graph SQL function metadata audit failed:"
  echo "$violations"
  exit 1
fi

echo "Function metadata audit passed for $DBNAME"
