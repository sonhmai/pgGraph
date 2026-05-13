#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SANDBOX_DIR="${ROOT_DIR}/sandbox"

CONTAINER_NAME="${PGGRAPH_CONTAINER_NAME:-pggraph-sandbox}"
IMAGE_NAME="${PGGRAPH_IMAGE_NAME:-pggraph-postgres:17}"

CLEANUP_TARGETS=()
CLEANUP_FLAGS=()
for arg in "$@"; do
  case "${arg}" in
    datasets|results|venv|docker|all) CLEANUP_TARGETS+=("${arg}") ;;
    *) CLEANUP_FLAGS+=("${arg}") ;;
  esac
done
if [ "${#CLEANUP_TARGETS[@]}" -eq 0 ]; then
  CLEANUP_TARGETS=(all)
fi

python3 "${SANDBOX_DIR}/common/run_benchmarks.py" \
  --cleanup "${CLEANUP_TARGETS[@]}" \
  --container "${CONTAINER_NAME}" \
  --image "${IMAGE_NAME}" \
  --datasets-dir "${SANDBOX_DIR}/benchmark/datasets" \
  --results-dir "${SANDBOX_DIR}/benchmark/results" \
  --benchmark-venv "${SANDBOX_DIR}/benchmark/.venv" \
  --playground-venv "${SANDBOX_DIR}/playground/.venv" \
  ${CLEANUP_FLAGS[@]+"${CLEANUP_FLAGS[@]}"}
