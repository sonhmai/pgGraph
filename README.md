<p align="center">
  <img src="assets/pggraph-banner.png" alt="pgGraph Banner" />
</p>

# pgGraph

Graph database superpowers for your existing Postgres data.

[![License: Apache-2.0](https://img.shields.io/badge/License-Apache--2.0-blue.svg)](LICENSE)
[![Version: 0.1.0](https://img.shields.io/badge/version-0.1.0-2ea44f.svg)](graph/Cargo.toml)
[![PostgreSQL 13-18](https://img.shields.io/badge/PostgreSQL-13--18-336791.svg)](https://www.postgresql.org/)

[![Built by Evokoa](https://img.shields.io/badge/Built_by-Evokoa-ff6b35.svg?logo=data:image/svg%2bxml;base64,PHN2ZyB3aWR0aD0iMzIiIGhlaWdodD0iMzIiIHZpZXdCb3g9IjAgMCAzMiAzMiIgZmlsbD0ibm9uZSIgeG1sbnM9Imh0dHA6Ly93d3cudzMub3JnLzIwMDAvc3ZnIj48cmVjdCB3aWR0aD0iMzIiIGhlaWdodD0iMzIiIHJ4PSI2IiBmaWxsPSJ3aGl0ZSIvPjxnIHRyYW5zZm9ybT0idHJhbnNsYXRlKDMsMykgc2NhbGUoMC4wNjk2KSI+PHBhdGggZD0iTTE4Mi4zMzUgMTkwLjUwMUwxODIuMzM2IDIwNS41MDFDMTgyLjMzNiAyOTMuMTk2IDE1Ny4wNzYgMzQyLjU4MyAxMjEuMDI5IDM2My4yOTVDODUuMzgxMiAzODMuNzc3IDQ0LjY5NCAzNzIuNzQ5IDIzLjM0MDcgMzUxLjM5NkwyMi44NDQ2IDM1MC44OTRDMS45MTI5MiAzMjkuNTA2IC05LjQ0MjY2IDI4OC43NzggMTAuMDU2NSAyNTIuODM3QzMwLjA3NTkgMjE1LjkzOSA3OS4wMzYyIDE5MC4wMDIgMTY2LjgzNiAxOTAuMDAyTDE4MS44MzUgMTkwLjAwMUgxODIuMzM1VjE5MC41MDFaTTIwMy41MDIgMTkxLjAwMkMyOTEuMTk2IDE5MS4wMDIgMzQwLjU4NCAyMTYuMjYxIDM2MS4yOTYgMjUyLjMwOUMzODEuNzc3IDI4Ny45NTcgMzcwLjc0OSAzMjguNjQzIDM0OS4zOTYgMzQ5Ljk5NkwzNDguODk0IDM1MC40OTRDMzI3LjUwNyAzNzEuNDI1IDI4Ni43NzggMzgyLjc4IDI1MC44MzggMzYzLjI4MkMyMTMuOTM5IDM0My4yNjIgMTg4LjAwMyAyOTQuMzAyIDE4OC4wMDMgMjA2LjUwMkwxODguMDAyIDE5MS41MDJWMTkxLjAwM0gxODguNTAyTDIwMy41MDIgMTkxLjAwMlpNMTUxLjAzMSAyMjEuMzE3Qzc5LjE2MTUgMjI0LjIyNSA0OC40OTQ0IDI0Ni45OTcgMzcuMzA0NiAyNjcuNjJDMjUuMTU0NCAyOTAuMDE2IDMyLjMwNTkgMzE1LjkyOSA0NC42Njg4IDMyOC44N0w0NS4yNjE2IDMyOS40NzVDNTcuODc5NyAzNDIuMDkzIDgzLjQ1OTYgMzQ5LjEzIDEwNS41ODcgMzM2LjQxNkMxMjUuODg2IDMyNC43NTIgMTQ4LjE4NyAyOTMuMzc3IDE1MS4wMzEgMjIxLjMxN1pNMjE5LjMxNiAyMjIuMzA3QzIyMi4yMjUgMjk0LjE3NyAyNDQuOTk3IDMyNC44NDMgMjY1LjYyMSAzMzYuMDMzQzI4OC4wMTYgMzQ4LjE4MyAzMTMuOTI4IDM0MS4wMzIgMzI2Ljg2OSAzMjguNjY5TDMyNy40NzUgMzI4LjA3N0MzNDAuMDkzIDMxNS40NTkgMzQ3LjEzIDI4OS44NzggMzM0LjQxNyAyNjcuNzUxQzMyMi43NTMgMjQ3LjQ1MiAyOTEuMzc3IDIyNS4xNTEgMjE5LjMxNiAyMjIuMzA3Wk0yMy4zMzk3IDIyLjc1NjJDNDEuMzczMiA0LjcyMzIyIDcyLjc1NzkgLTUuNzE4MzUgMTAzLjM5MSAzLjMwMTE1QzEzNS4xNjQgMTIuNjU3IDE2Mi4zOTkgNDEuNTkzNSAxNzQuODU0IDk0Ljk5NjVDMTc2LjU1NiAxMDIuMjkyIDE3Ny4zNjcgMTA3Ljg2MiAxNzcuNzk5IDExMC4yMDJMMTgyLjAyNiAxMzMuMDkzTDE4Mi4xNjcgMTMzLjg1N0wxODEuNDE0IDEzMy42NjlMMTU4LjgyMSAxMjguMDU5QzE1Mi4zNDEgMTI2LjQ1IDExMC45MDcgMTE0LjMxNCA4OC4wNDI5IDEwNy4zNTZMNzMuNjkyMyAxMDIuOTg5TDczLjIxMzggMTAyLjg0Mkw3My4zNTkzIDEwMi4zNjRMODIuMDk0NiA3My42NjQ0TDgyLjI0MDEgNzMuMTg1OUw4Mi43MTg2IDczLjMzMTRMOTcuMDY5MiA3Ny42OTg2QzEwOS42MTEgODEuNTE1NyAxMjcuMjg3IDg2Ljc0NyAxNDEuNzIyIDkwLjk2MDNDMTMwLjM2MiA1My4xNDc4IDExMS4xNTYgMzcuOTAzNSA5NC42MzM3IDMzLjAzODVDNzUuNDQ4NiAyNy4zODk3IDU1LjY5MzEgMzQuMjQ1MiA0NS4yNjA2IDQ0LjY3NzFMNDQuNjY3OSA0NS4yODI2QzMyLjMwNDggNTguMjIzNCAyNS4xNTM2IDg0LjEzNjEgMzcuMzAzNiAxMDYuNTMyQzQ5LjI4MTEgMTI4LjYwOCA4My41NzkyIDE1My4xNSAxNjYuODM1IDE1My4xNUwxODEuODM1IDE1My4xNTFIMTgyLjMzNVYxNTMuNjUxTDE4Mi4zMzQgMTgzLjY1MVYxODQuMTUxSDE4MS44MzRMMTY2LjgzNSAxODQuMTVDNzkuMDM0OCAxODQuMTUgMzAuMDc0OCAxNTguMjEzIDEwLjA1NTUgMTIxLjMxNUMtOS40NDM0NSA4NS4zNzQyIDEuOTExOSA0NC42NDU5IDIyLjg0MzYgMjMuMjU4MkwyMy4zMzk3IDIyLjc1NjJaTTI1MC44MzcgMTEuMDU3QzI4Ni43NzcgLTguNDQyMSAzMjcuNTA2IDIuOTEzMzkgMzQ4Ljg5MyAyMy44NDUxTDM0OS4zOTUgMjQuMzQxMkMzNzAuNzQ4IDQ1LjY5NDUgMzgxLjc3NyA4Ni4zODE5IDM2MS4yOTYgMTIyLjAzQzM0MC41ODQgMTU4LjA3NyAyOTEuMTk2IDE4My4zMzYgMjAzLjUwMiAxODMuMzM2TDE4OC41MDEgMTgzLjMzNUgxODguMDAxVjE4Mi44MzVMMTg4LjAwMyAxNjcuODM2QzE4OC4wMDMgODAuMDM2NSAyMTMuOTM4IDMxLjA3NjMgMjUwLjgzNyAxMS4wNTdaTTMyNi44NjkgNDUuNjY5M0MzMTMuOTI4IDMzLjMwNjMgMjg4LjAxNiAyNi4xNTU3IDI2NS42MiAzOC4zMDZDMjQ0Ljk5NiA0OS40OTU4IDIyMi4yMjUgODAuMTYxNyAyMTkuMzE2IDE1Mi4wMzJDMjkxLjM3NyAxNDkuMTg4IDMyMi43NTMgMTI2Ljg4NyAzMzQuNDE3IDEwNi41ODdDMzQ3LjEzIDg0LjQ2MDQgMzQwLjA5MyA1OC44ODAzIDMyNy40NzUgNDYuMjYyMUwzMjYuODY5IDQ1LjY2OTNaIiBmaWxsPSIjMWExYTFhIi8+PC9nPjwvc3ZnPg==)](https://evokoa.com)

<p>
  <a href="https://x.com/evokoa_ai"><img src="https://img.shields.io/badge/Follow%20on%20X-000000?logo=x&logoColor=white&style=for-the-badge" alt="Follow on X"></a>
  <a href="https://discord.gg/HyUvAzmHej"><img src="https://img.shields.io/badge/Join%20our%20Discord-5865F2?logo=discord&logoColor=white&style=for-the-badge" alt="Join our Discord"></a>
  <a href="https://www.producthunt.com/@evokoa"><img src="https://img.shields.io/badge/Follow%20on%20Product%20Hunt-DA552E?logo=product-hunt&logoColor=white&style=for-the-badge" alt="Follow on Product Hunt"></a>
</p>

pgGraph is a PostgreSQL extension for running graph search, traversal, shortest
path, and relationship queries directly against ordinary PostgreSQL tables.

Your tables stay the source of truth. pgGraph builds a derived graph index and
lets you query it from SQL using functions in the `graph` schema.

> [!IMPORTANT]
> pgGraph is in early alpha. Please avoid production use for now; try it in
> Docker or a dedicated development database and share feedback to help the
> project grow.

## Why pgGraph?

PostgreSQL is great at relational queries, but graph-style questions often
require custom recursive SQL for each schema:

- “Find records related to Alice within 2 hops.”
- “Find the shortest path between this person and this company.”
- “Search nodes across registered tables.”
- “Explore connected records without moving data into another database.”

pgGraph adds graph queries on top of your existing PostgreSQL tables, without
requiring a separate graph database, graph-specific storage system, or a new
query language.

## Quickstart

The fastest way to try pgGraph is the included quickstart script.

It starts a disposable Docker-backed PostgreSQL database, installs pgGraph,
creates two normal PostgreSQL tables, discovers the foreign key relationship,
builds the graph, and runs example queries.

You need Docker or Docker Desktop installed and running:

- macOS: install Docker Desktop.
- Windows: install Docker Desktop with WSL2 enabled, then run the script from
  WSL2 or Git Bash.
- Linux: install Docker Engine and the Docker Compose plugin.

```bash
git clone https://github.com/evokoa/pggraph.git
cd pggraph
scripts/quickstart.sh               # run the full quickstart demo
scripts/quickstart.sh docker my-postgres 17 appdb postgres  # install into existing Postgres Docker container
scripts/quickstart.sh pgrx           # source build/install with pgrx into local PostgreSQL
scripts/quickstart.sh playground panama # start Streamlit playground with a preset dataset (panama|ldbc)
```

The script works on macOS and Linux from a normal terminal, and on Windows from
WSL2 or Git Bash with Docker Desktop. It is not a native PowerShell or Command
Prompt script.

## Docker And Install Options

| Path | Use when | Start here |
|---|---|---|
| Quickstart script | You want the fastest local trial | `scripts/quickstart.sh` |
| Docker Compose scratch database | You want a disposable pgGraph database | `docker compose up --build -d` then `docker compose exec postgres psql -U postgres -d graph` |
| Streamlit playground | You want a browser SQL inspector with a real dataset | `sandbox/start_playground.sh` |
| Existing PostgreSQL Docker container | You already have a container to install into | `scripts/install_into_docker_postgres.sh CONTAINER 17 DB_NAME postgres` |
| Source build with pgrx | You are developing pgGraph or building against local PostgreSQL headers | [Installation docs](docs/user_guide/installation.mdx) |

The root Docker image currently runs PostgreSQL 17. Package scripts can build
extension artifacts for PostgreSQL 13 through 18. The PostgreSQL major version
of the extension package must match the target server.

For installation details, see:

https://docs.evokoa.com/pggraph/user_guide

## Community

pgGraph is built by [Evokoa](https://evokoa.com). Follow the project through
the links at the top of this README.

## License

Apache-2.0. See [LICENSE](LICENSE).
