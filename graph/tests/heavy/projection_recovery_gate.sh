#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GRAPH_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
PG_VERSION_FEATURE="${PG_VERSION_FEATURE:-pg17}"
DEVELOPMENT_FEATURES="$PG_VERSION_FEATURE development"

cd "$GRAPH_ROOT"

cargo test --features "$PG_VERSION_FEATURE" projection::recovery
cargo test --features "$PG_VERSION_FEATURE" projection::gc
cargo pgrx test --features "$DEVELOPMENT_FEATURES" "$PG_VERSION_FEATURE" projection_repair
cargo pgrx test --features "$DEVELOPMENT_FEATURES" "$PG_VERSION_FEATURE" full_rebuild_restores_valid_projection_generation

echo "Projection recovery gate passed for $PG_VERSION_FEATURE"
