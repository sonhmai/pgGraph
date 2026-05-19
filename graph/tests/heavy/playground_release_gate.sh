#!/usr/bin/env bash
# Validate that Streamlit playground SQL examples still produce expected results.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
SANDBOX_DIR="${ROOT_DIR}/sandbox"

PREPARE_PLAYGROUND="${PREPARE_PLAYGROUND:-1}"
PG_PORT="${PGGRAPH_PG_PORT:-55432}"
CONTAINER_NAME="${PGGRAPH_CONTAINER_NAME:-pggraph-sandbox}"
IMAGE_NAME="${PGGRAPH_IMAGE_NAME:-pggraph-postgres:17}"
PLAYGROUND_DATASET="${PGGRAPH_PLAYGROUND_DATASET:-panama}"

if [[ "${PREPARE_PLAYGROUND}" == "1" ]]; then
  # shellcheck source=../../../sandbox/common/docker.sh
  source "${SANDBOX_DIR}/common/docker.sh"

  require_docker
  ensure_pggraph_image "${ROOT_DIR}" "${IMAGE_NAME}"
  ensure_pggraph_container "${CONTAINER_NAME}" "${IMAGE_NAME}" "${PG_PORT}"
  ACTUAL_PG_PORT="$(pggraph_container_host_port "${CONTAINER_NAME}")"

  prepare_args=(
    "${SANDBOX_DIR}/common/run_benchmarks.py"
    --dataset "${PLAYGROUND_DATASET}"
    --container "${CONTAINER_NAME}"
    --host 127.0.0.1
    --port "${ACTUAL_PG_PORT}"
    --database postgres
    --user postgres
    --password postgres
    --datasets-dir "${SANDBOX_DIR}/benchmark/datasets"
    --results-dir "${SANDBOX_DIR}/benchmark/results"
    --prepare-only
  )
  if [[ "${PGGRAPH_PLAYGROUND_YES:-0}" == "1" ]]; then
    prepare_args+=(--yes)
  fi

  python3 "${prepare_args[@]}"

  if [[ -z "${PGGRAPH_DSN:-}" && -z "${PGGRAPH_PLAYGROUND_DSN:-}" ]]; then
    export PGGRAPH_DSN="postgresql://postgres:postgres@localhost:${ACTUAL_PG_PORT}/postgres"
  fi
fi

python3 "${SCRIPT_DIR}/playground_release_gate.py" "$@"
