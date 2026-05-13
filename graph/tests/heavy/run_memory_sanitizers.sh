#!/usr/bin/env bash
set -euo pipefail

DBNAME="${DBNAME:-postgres}"
PG_VERSION_FEATURE="${PG_VERSION_FEATURE:-pg17}"
RUN_ASAN="${RUN_ASAN:-0}"
RUN_PGRX="${RUN_PGRX:-1}"
RUN_PGBENCH="${RUN_PGBENCH:-1}"
RUN_VALGRIND="${RUN_VALGRIND:-0}"

echo "Memory/FFI boundary runs for $PG_VERSION_FEATURE."

if [[ "$RUN_ASAN" == "1" ]]; then
  RUSTFLAGS="-Zsanitizer=address" cargo +nightly test --features "$PG_VERSION_FEATURE" -Zbuild-std
else
  echo "Skipping ASan. Set RUN_ASAN=1 to run nightly address-sanitizer tests."
fi

if [[ "$RUN_PGRX" == "1" ]]; then
  cargo pgrx test "$PG_VERSION_FEATURE"
fi

if [[ "$RUN_PGBENCH" == "1" ]]; then
  DBNAME="$DBNAME" CLIENTS="${CLIENTS:-16}" JOBS="${JOBS:-4}" TIME="${TIME:-600}" ./tests/heavy/run_pgbench_sync.sh
fi

if [[ "$RUN_VALGRIND" == "1" ]]; then
  : "${PGDATA:?PGDATA is required when RUN_VALGRIND=1}"
  : "${POSTGRES_BIN:=postgres}"
  valgrind --leak-check=full --trace-children=yes "$POSTGRES_BIN" -D "$PGDATA"
else
  echo "Skipping Valgrind. Set RUN_VALGRIND=1 with PGDATA and POSTGRES_BIN to run it."
fi

echo "Memory/FFI boundary checks completed."
