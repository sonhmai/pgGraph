#!/usr/bin/env bash
# Run the synthetic release smoke and capture reproducible evidence metadata.
#
# Usage:
#   cd graph
#   ./tests/heavy/run_synthetic_release_evidence.sh
#
# Useful knobs mirror synthetic_release_smoke.sh:
#   DBNAME=pggraph_synthetic_evidence
#   NODE_COUNT=50000
#   HUB_FANOUT=1000
#   MAX_BUILD_MS=60000
#   MAX_QUERY_MS=1000
#   OUT_DIR=target/release-evidence
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GRAPH_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
ROOT_DIR="$(cd "$GRAPH_DIR/.." && pwd)"

DBNAME="${DBNAME:-pggraph_synthetic_evidence}"
NODE_COUNT="${NODE_COUNT:-50000}"
HUB_FANOUT="${HUB_FANOUT:-1000}"
MAX_BUILD_MS="${MAX_BUILD_MS:-60000}"
MAX_QUERY_MS="${MAX_QUERY_MS:-1000}"
OUT_DIR="${OUT_DIR:-$GRAPH_DIR/target/release-evidence}"

mkdir -p "$OUT_DIR"

timestamp="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
safe_timestamp="$(date -u +"%Y%m%dT%H%M%SZ")"
log_file="$OUT_DIR/synthetic_release_${safe_timestamp}.log"
metadata_file="$OUT_DIR/synthetic_release_${safe_timestamp}.json"
artifact_report_file="$OUT_DIR/synthetic_artifact_${safe_timestamp}.json"

set +e
DBNAME="$DBNAME" \
NODE_COUNT="$NODE_COUNT" \
HUB_FANOUT="$HUB_FANOUT" \
MAX_BUILD_MS="$MAX_BUILD_MS" \
MAX_QUERY_MS="$MAX_QUERY_MS" \
"$SCRIPT_DIR/synthetic_release_smoke.sh" 2>&1 | tee "$log_file"
status="${PIPESTATUS[0]}"
set -e

artifact_path="$(awk -F'artifact=' '/Synthetic release smoke passed:/ { print $2 }' "$log_file" | tail -1)"
if [[ -n "$artifact_path" && -f "$artifact_path" ]]; then
  python3 "$ROOT_DIR/scripts/inspect_pggraph_artifact.py" "$artifact_path" > "$artifact_report_file"
else
  artifact_report_file=""
fi

git_sha="$(git -C "$ROOT_DIR" rev-parse HEAD 2>/dev/null || echo unknown)"
postgres_version="$(psql -X -qAt "$DBNAME" -c "SHOW server_version;" 2>/dev/null || echo unknown)"

python3 - "$metadata_file" <<PY
import json
import os
import platform
import sys

metadata = {
    "timestamp": "$timestamp",
    "status": int("$status"),
    "git_sha": "$git_sha",
    "postgres_version": "$postgres_version",
    "dataset": {
        "shape": "deterministic-synthetic-release",
        "node_count": int("$NODE_COUNT"),
        "hub_fanout": int("$HUB_FANOUT"),
    },
    "thresholds": {
        "max_build_ms": int("$MAX_BUILD_MS"),
        "max_query_ms": int("$MAX_QUERY_MS"),
    },
    "environment": {
        "os": platform.system(),
        "release": platform.release(),
        "machine": platform.machine(),
    },
    "log_file": "$log_file",
    "artifact_path": "$artifact_path" or None,
    "artifact_report": "$artifact_report_file" or None,
}

with open(sys.argv[1], "w") as handle:
    json.dump(metadata, handle, indent=2)
    handle.write("\n")
PY

echo "synthetic release evidence: $metadata_file"
exit "$status"
