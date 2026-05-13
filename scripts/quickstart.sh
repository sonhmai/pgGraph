#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

usage() {
  cat <<'USAGE'
Run the pgGraph quickstart against the repository Docker Compose database.

Usage:
  scripts/quickstart.sh [demo|setup|psql|clean]

Platforms:
  macOS/Linux terminal, or Windows via WSL2/Git Bash with Docker Desktop.
  This is not a native PowerShell or Command Prompt script.

Commands:
  demo   Build/start PostgreSQL, create the people/companies dataset, build pgGraph, and run example queries. Default.
  setup  Build/start PostgreSQL with pgGraph installed, but do not create demo tables.
  psql   Build/start PostgreSQL, create/build the demo graph, then open psql.
  clean  Stop the Compose database and remove its volume.
USAGE
}

docker_install_help() {
  cat <<'EOF'
If you need to install Docker, see:
  Mac:     https://docs.docker.com/desktop/setup/install/mac-install/
  Windows: https://docs.docker.com/desktop/setup/install/windows-install/
  Linux:   https://docs.docker.com/desktop/setup/install/linux/
EOF
}

require_docker_compose() {
  if ! command -v docker >/dev/null 2>&1; then
    echo "Error: Docker is not installed or not available on PATH." >&2
    docker_install_help >&2
    exit 1
  fi

  if ! docker info >/dev/null 2>&1; then
    echo "Error: Cannot connect to the Docker daemon. Is Docker Desktop running?" >&2
    docker_install_help >&2
    exit 1
  fi

  if ! docker compose version >/dev/null 2>&1; then
    echo "Error: Docker Compose v2 is required. Install Docker Desktop or the docker compose plugin." >&2
    docker_install_help >&2
    exit 1
  fi
}

compose() {
  docker compose --project-directory "${ROOT_DIR}" "$@"
}

wait_for_postgres() {
  echo "Waiting for PostgreSQL..."
  for _ in $(seq 1 60); do
    if compose exec -T postgres pg_isready -U postgres >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done

  echo "Error: PostgreSQL did not become ready." >&2
  compose logs postgres >&2 || true
  exit 1
}

start_postgres() {
  require_docker_compose
  compose up --build -d postgres
  wait_for_postgres
  compose exec -T postgres psql -U postgres -d graph -v ON_ERROR_STOP=1 \
    -c 'CREATE EXTENSION IF NOT EXISTS graph;' >/dev/null
}

prepare_demo_graph() {
  compose exec -T postgres psql -U postgres -d graph -v ON_ERROR_STOP=1 >/dev/null <<'SQL'
CREATE EXTENSION IF NOT EXISTS graph;
SELECT graph.reset();

DROP TABLE IF EXISTS people;
DROP TABLE IF EXISTS companies;

CREATE TABLE companies (
    id text PRIMARY KEY,
    name text NOT NULL
);

CREATE TABLE people (
    id text PRIMARY KEY,
    name text NOT NULL,
    company_id text REFERENCES companies(id)
);

INSERT INTO companies VALUES
    ('c1', 'Acme Bank'),
    ('c2', 'Northwind Trading');

INSERT INTO people VALUES
    ('p1', 'Alice', 'c1'),
    ('p2', 'Bob', 'c1'),
    ('p3', 'Carol', 'c2');

SELECT * FROM graph.auto_discover('public');
SQL
}

run_demo_sql() {
  compose exec -T postgres psql -U postgres -d graph -v ON_ERROR_STOP=1 <<'SQL'
\pset pager off

CREATE EXTENSION IF NOT EXISTS graph;
SELECT graph.reset();

DROP TABLE IF EXISTS people;
DROP TABLE IF EXISTS companies;

CREATE TABLE companies (
    id text PRIMARY KEY,
    name text NOT NULL
);

CREATE TABLE people (
    id text PRIMARY KEY,
    name text NOT NULL,
    company_id text REFERENCES companies(id)
);

INSERT INTO companies VALUES
    ('c1', 'Acme Bank'),
    ('c2', 'Northwind Trading');

INSERT INTO people VALUES
    ('p1', 'Alice', 'c1'),
    ('p2', 'Bob', 'c1'),
    ('p3', 'Carol', 'c2');

\echo ''
\echo 'pgGraph discovery and build'
SELECT * FROM graph.auto_discover('public');

\echo ''
\echo 'Example data: ordinary PostgreSQL tables'
SELECT
    p.name AS person,
    c.name AS company
FROM people p
JOIN companies c ON c.id = p.company_id
ORDER BY p.name;

\echo ''
\echo 'Graph search: name = Alice'
SELECT node_table_name, node_id, node->>'name' AS name
FROM graph.search('name', 'Alice', mode := 'exact');

\echo ''
\echo 'Graph traversal: Alice, 1 hop'
SELECT depth, node_table_name, node_id, node->>'name' AS name, edge_path
FROM graph.traverse('people'::regclass, 'p1', 1)
ORDER BY depth, node_table_name, node_id;

\echo ''
\echo 'Graph shortest path: Alice -> Acme Bank'
SELECT step, node_table_name, node_id, node->>'name' AS name, edge_label
FROM graph.shortest_path(
    'people'::regclass, 'p1',
    'companies'::regclass, 'c1'
)
ORDER BY step;
SQL
}

main() {
  local command="${1:-demo}"

  case "${command}" in
    demo)
      start_postgres
      run_demo_sql
      echo ""
      echo "Done. Re-run queries with: scripts/quickstart.sh psql"
      ;;
    setup)
      start_postgres
      echo "PostgreSQL is running with pgGraph installed."
      echo "Open psql with: scripts/quickstart.sh psql"
      ;;
    psql)
      start_postgres
      prepare_demo_graph
      echo "Demo graph is ready: people, companies, and pgGraph metadata are built."
      exec docker compose --project-directory "${ROOT_DIR}" exec postgres psql -U postgres -d graph
      ;;
    clean)
      require_docker_compose
      compose down -v
      ;;
    -h|--help|help)
      usage
      ;;
    *)
      usage >&2
      exit 2
      ;;
  esac
}

main "$@"
