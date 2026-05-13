FROM rust:1-bookworm AS builder

ARG PG_MAJOR=17
ARG PGRX_VERSION=0.18.0

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        gnupg \
        lsb-release \
    && curl -fsSL https://www.postgresql.org/media/keys/ACCC4CF8.asc \
        | gpg --dearmor -o /usr/share/keyrings/postgresql.gpg \
    && echo "deb [signed-by=/usr/share/keyrings/postgresql.gpg] http://apt.postgresql.org/pub/repos/apt $(lsb_release -cs)-pgdg main" \
        > /etc/apt/sources.list.d/pgdg.list \
    && apt-get update \
    && apt-get install -y --no-install-recommends \
        postgresql-${PG_MAJOR} \
        postgresql-server-dev-${PG_MAJOR} \
    && rm -rf /var/lib/apt/lists/*

RUN cargo install cargo-pgrx --version ${PGRX_VERSION} --locked

WORKDIR /src/graph
COPY graph/ /src/graph/
RUN cargo pgrx init --pg${PG_MAJOR}=/usr/lib/postgresql/${PG_MAJOR}/bin/pg_config \
    && cargo pgrx package --pg-config=/usr/lib/postgresql/${PG_MAJOR}/bin/pg_config

FROM postgres:17-bookworm

ARG PG_MAJOR=17

COPY --from=builder /src/graph/target/release/graph-pg${PG_MAJOR}/usr/share/postgresql/${PG_MAJOR}/extension/graph* /usr/share/postgresql/${PG_MAJOR}/extension/
COPY --from=builder /src/graph/target/release/graph-pg${PG_MAJOR}/usr/lib/postgresql/${PG_MAJOR}/lib/graph.so /usr/lib/postgresql/${PG_MAJOR}/lib/

RUN mkdir -p /docker-entrypoint-initdb.d \
    && echo 'CREATE EXTENSION IF NOT EXISTS graph;' > /docker-entrypoint-initdb.d/01-create-extension.sql
