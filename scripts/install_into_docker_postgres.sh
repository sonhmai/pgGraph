#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Install pgGraph into an existing running PostgreSQL Docker container.

Usage:
  scripts/install_into_docker_postgres.sh CONTAINER [PG_MAJOR] [DB_NAME] [DB_USER]

Arguments:
  CONTAINER  Running PostgreSQL container name or ID.
  PG_MAJOR   PostgreSQL major version in the target container. Default: 17.
  DB_NAME    Database where CREATE EXTENSION should run. Default: postgres.
  DB_USER    Database user for psql. Default: postgres.

Environment:
  BUILDER_IMAGE_PREFIX Builder image prefix. Default: pggraph-builder
  SKIP_CREATE_EXTENSION=1  Copy files only; do not run CREATE EXTENSION.

Example:
  scripts/install_into_docker_postgres.sh my-postgres 17 appdb postgres
USAGE
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if [[ $# -lt 1 || $# -gt 4 ]]; then
  usage >&2
  exit 2
fi

target_container="$1"
pg_major="${2:-17}"
db_name="${3:-postgres}"
db_user="${4:-postgres}"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmpdir="$(mktemp -d "${TMPDIR:-/tmp}/pggraph-docker-install.XXXXXX")"

cleanup() {
  rm -rf "$tmpdir"
}
trap cleanup EXIT

if ! docker inspect "$target_container" >/dev/null 2>&1; then
  echo "Target container not found: $target_container" >&2
  exit 1
fi

if [[ "$(docker inspect -f '{{.State.Running}}' "$target_container")" != "true" ]]; then
  echo "Target container is not running: $target_container" >&2
  exit 1
fi

"$repo_root/scripts/build_docker_pggraph_package.sh" "$pg_major" "$tmpdir"
"$repo_root/scripts/copy_pggraph_package_to_docker_postgres.sh" \
  "$target_container" \
  "$tmpdir/graph-pg${pg_major}" \
  "$pg_major" \
  "$db_name" \
  "$db_user"
