#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Delete generated local artifacts that should not ship with the repository.

Usage:
  scripts/clean_generated_artifacts.sh [--dry-run]

Deletes:
  graph/target/
  graph/fuzz/target/
  all .DS_Store files under the repository

Keeps useful test and release assets, including:
  graph/fuzz/corpus/
  graph/fuzz/fuzz_targets/
  graph/proptest-regressions/lib.txt
  graph/tests/heavy/
  graph/src/pg_test.rs
USAGE
}

dry_run=0
if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
elif [[ "${1:-}" == "--dry-run" ]]; then
  dry_run=1
elif [[ $# -gt 0 ]]; then
  usage >&2
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

delete_path() {
  local path="$1"

  if [[ ! -e "$path" ]]; then
    return
  fi

  if [[ "$dry_run" == "1" ]]; then
    printf 'would delete %s\n' "${path#$repo_root/}"
  else
    rm -rf "$path"
    printf 'deleted %s\n' "${path#$repo_root/}"
  fi
}

delete_path "$repo_root/graph/target"
delete_path "$repo_root/graph/fuzz/target"

while IFS= read -r -d '' path; do
  delete_path "$path"
done < <(find "$repo_root" -name .DS_Store -type f -print0)
