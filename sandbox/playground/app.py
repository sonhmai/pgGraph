import base64
import json
import os
import time
from pathlib import Path

import psycopg
import streamlit as st
from psycopg.rows import dict_row

from queries import (
    DEFAULT_QUESTION,
    DEFAULT_SQL,
    PLAYGROUND_CONTEXT,
    QUERY_QUESTIONS,
    QUERY_SECTIONS,
)

GRAPH_BUSY_SQLSTATE = "PG006"
GRAPH_BUILD_WAIT_SECONDS = 600


def asset_path(name: str) -> Path:
    return Path(os.environ.get("PGGRAPH_ASSETS_DIR", "assets")) / name


def svg_data_uri(path: Path) -> str:
    if not path.exists():
        return ""
    payload = base64.b64encode(path.read_bytes()).decode("ascii")
    return f"data:image/svg+xml;base64,{payload}"


@st.cache_resource(show_spinner=False)
def connection() -> psycopg.Connection:
    conn = psycopg.connect(os.environ["PGGRAPH_DSN"], row_factory=dict_row, autocommit=True)
    ensure_graph_loaded(conn)
    return conn


def fetch_one(conn: psycopg.Connection, sql: str) -> dict:
    with conn.cursor() as cur:
        cur.execute(sql)
        row = cur.fetchone()
        return dict(row) if row else {}


def is_graph_busy(exc: Exception) -> bool:
    return isinstance(exc, psycopg.Error) and getattr(exc, "sqlstate", None) == GRAPH_BUSY_SQLSTATE


def graph_status(cur: psycopg.Cursor) -> dict:
    cur.execute("SELECT node_count, edge_count FROM graph.status();")
    row = cur.fetchone()
    return dict(row) if row else {"node_count": 0, "edge_count": 0}


def ensure_active_graph(cur: psycopg.Cursor) -> None:
    deadline = time.monotonic() + GRAPH_BUILD_WAIT_SECONDS
    attempt = 0

    while True:
        status = graph_status(cur)
        if status["node_count"] > 0 and status["edge_count"] > 0:
            return

        try:
            cur.execute("SELECT * FROM graph.build();")
            if cur.description:
                cur.fetchall()
            return
        except Exception as exc:
            if not is_graph_busy(exc):
                raise
            if time.monotonic() >= deadline:
                raise RuntimeError("Timed out waiting for another graph build or vacuum to finish.") from exc
            time.sleep(min(5, 1 + attempt))
            attempt += 1


def ensure_graph_loaded(conn: psycopg.Connection) -> None:
    with conn.cursor() as cur:
        cur.execute("CREATE EXTENSION IF NOT EXISTS graph;")
        cur.execute("SELECT graph.test_enabled();")
        cur.execute("SELECT count(*) AS nodes FROM panama.nodes;")
        source_nodes = cur.fetchone()["nodes"]
        if source_nodes == 0:
            raise RuntimeError("Panama tables are empty. Rerun sandbox/start_playground.sh to prepare the dataset.")

        cur.execute(
            """
            SELECT EXISTS (
              SELECT 1
              FROM graph.registered_tables()
              WHERE table_name = 'panama.nodes'
            ) AS registered;
            """
        )
        if not cur.fetchone()["registered"]:
            cur.execute("SELECT graph.reset();")
            cur.execute(
                """
                TRUNCATE graph._registered_filter_columns,
                         graph._registered_edges,
                         graph._registered_tables,
                         graph._build_jobs,
                         graph._maintenance_jobs,
                         graph._sync_log,
                         graph._sync_buffer
                RESTART IDENTITY;
                """
            )
            cur.execute(
                """
                SELECT graph.add_table(
                  'panama.nodes'::regclass,
                  'node_id',
                  ARRAY['name', 'countries', 'country_codes', 'label']
                );
                """
            )
            cur.execute(
                """
                SELECT graph.add_edge(
                  from_table := 'panama.edges'::regclass,
                  from_column := 'start_id',
                  to_table := 'panama.nodes'::regclass,
                  to_column := 'end_id',
                  label := 'related_to',
                  bidirectional := true,
                  label_column := 'rel_type'
                );
                """
            )

        ensure_active_graph(cur)

        cur.execute(
            """
            WITH seed AS (
              SELECT start_id
              FROM panama.edges
              GROUP BY start_id
              ORDER BY count(*) DESC
              LIMIT 1
            )
            SELECT count(*)
            FROM seed, LATERAL graph.traverse(
              'panama.nodes'::regclass,
              seed.start_id,
              1,
              hydrate := false,
              max_rows := 1
            );
            """
        )


def format_elapsed(seconds: float) -> str:
    if seconds < 1:
        return f"{seconds * 1000:.0f} ms"
    return f"{seconds:.2f} s"


def run_sql(sql: str) -> dict:
    started = time.perf_counter()
    conn = connection()
    ensure_graph_loaded(conn)
    result_sets: list[dict] = []
    messages: list[str] = []
    with conn.cursor() as cur:
        cur.execute(sql)
        result_index = 1
        while True:
            if cur.description:
                rows = [dict(row) for row in cur.fetchall()]
                result_sets.append(
                    {
                        "index": result_index,
                        "row_count": len(rows),
                        "rows": rows,
                    }
                )
                result_index += 1
            elif cur.statusmessage:
                messages.append(cur.statusmessage)
            if not cur.nextset():
                break

    elapsed_seconds = time.perf_counter() - started
    return {
        "ok": True,
        "elapsed_seconds": elapsed_seconds,
        "elapsed": format_elapsed(elapsed_seconds),
        "result_sets": result_sets,
        "messages": messages or ["Query completed."],
    }


def run_sql_with_error_handling(sql: str) -> dict:
    started = time.perf_counter()
    try:
        return run_sql(sql)
    except Exception as exc:
        elapsed_seconds = time.perf_counter() - started
        return {
            "ok": False,
            "elapsed_seconds": elapsed_seconds,
            "elapsed": format_elapsed(elapsed_seconds),
            "error": f"{type(exc).__name__}: {exc}",
            "result_sets": [],
            "messages": [],
        }


def render_result(result: dict) -> None:
    if not result:
        st.code("Run a query to see results.", language="json")
        return

    elapsed = result.get("elapsed", "unknown")
    if result.get("ok"):
        st.caption(f"Completed in {elapsed}")
    else:
        st.caption(f"Failed after {elapsed}")
        st.error(result.get("error", "Query failed."))
        return

    result_sets = result.get("result_sets", [])
    messages = result.get("messages", [])
    view = st.radio("Result view", ["Table", "Raw JSON"], horizontal=True, label_visibility="collapsed")

    if view == "Raw JSON":
        st.code(json.dumps(result_sets or messages, indent=2, default=str), language="json")
        return

    if not result_sets:
        st.info("\n".join(messages))
        return

    for result_set in result_sets:
        rows = result_set["rows"]
        label = f"Result {result_set['index']} - {result_set['row_count']:,} rows"
        st.caption(label)
        if rows:
            st.dataframe(rows, use_container_width=True, hide_index=True)
        else:
            st.info("No rows returned.")


def render_loading_button(slot) -> None:
    slot.markdown(
        """
        <div class="loading-button">
          <span class="loading-spinner"></span>
          <span>Running SQL...</span>
        </div>
        """,
        unsafe_allow_html=True,
    )


def apply_css() -> None:
    st.markdown(
        """
        <style>
        :root {
          --bg: #050505;
          --panel: #1d1d1f;
          --panel-2: #111113;
          --line: #303035;
          --text: #f4f4f5;
          --muted: #8d8d95;
          --accent: #f7f7f8;
        }
        .stApp {
          background: var(--bg);
          color: var(--text);
        }
        header, [data-testid="stToolbar"], [data-testid="stDecoration"] {
          display: none !important;
        }
        section[data-testid="stSidebar"] {
          width: 366px !important;
          background: var(--panel);
          border-right: 1px solid #2b2b30;
        }
        section[data-testid="stSidebar"] > div {
          padding: 24px 24px 32px;
        }
        .main .block-container {
          max-width: 1296px;
          padding: 42px 64px 54px;
        }
        .brand-row {
          display: flex;
          align-items: center;
          gap: 12px;
          padding-bottom: 28px;
          border-bottom: 1px solid var(--line);
          margin-bottom: 38px;
        }
        .brand-row img {
          height: 28px;
          width: 28px;
        }
        .brand-name {
          font-weight: 700;
          font-size: 17px;
          color: var(--text);
        }
        .brand-product {
          color: var(--muted);
          font-size: 16px;
        }
        .main-top {
          display: flex;
          justify-content: space-between;
          align-items: center;
          border-bottom: 1px solid #1f1f22;
          padding-bottom: 22px;
          margin-bottom: 28px;
        }
        .dataset-label {
          color: var(--muted);
          font-size: 14px;
        }
        .metric-strip {
          display: grid;
          grid-template-columns: repeat(4, minmax(0, 1fr));
          gap: 12px;
          margin-bottom: 22px;
        }
        .metric {
          background: var(--panel-2);
          border: 1px solid #222228;
          border-radius: 8px;
          padding: 14px 16px;
        }
        .metric-label {
          color: var(--muted);
          font-size: 12px;
          margin-bottom: 6px;
        }
        .metric-value {
          color: var(--text);
          font-size: 20px;
          font-weight: 700;
        }
        .workflow {
          color: var(--muted);
          font-size: 13px;
          margin: 18px 0 8px;
        }
        .stButton > button {
          width: 100%;
          border-radius: 8px;
          border: 1px solid #33333a;
          background: #151518;
          color: var(--text);
          text-align: left;
          padding: 9px 12px;
        }
        .stButton > button:hover {
          border-color: #56565f;
          background: #202026;
          color: #fff;
        }
        .loading-button {
          width: 100%;
          min-height: 38px;
          display: flex;
          align-items: center;
          justify-content: center;
          gap: 10px;
          border-radius: 8px;
          border: 1px solid #44444c;
          background: #202026;
          color: var(--text);
          font-size: 14px;
          font-weight: 600;
        }
        .loading-spinner {
          width: 14px;
          height: 14px;
          border: 2px solid #6f6f78;
          border-top-color: #f4f4f5;
          border-radius: 999px;
          animation: pggraph-spin 0.8s linear infinite;
        }
        @keyframes pggraph-spin {
          to { transform: rotate(360deg); }
        }
        .stTextArea textarea {
          background: #0f0f12 !important;
          color: #f4f4f5 !important;
          border: 1px solid #2b2b30 !important;
          border-radius: 8px !important;
          font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
          font-size: 13px;
        }
        .stCodeBlock pre {
          background: #0b0b0d !important;
          border: 1px solid #24242a;
          border-radius: 8px;
        }
        a {
          color: #d7d7dc !important;
        }
        </style>
        """,
        unsafe_allow_html=True,
    )


def sidebar() -> None:
    logo = svg_data_uri(asset_path("favicon.svg"))
    logo_img = f'<img src="{logo}" alt="evokoa logo" />' if logo else ""
    st.sidebar.markdown(
        f"""
        <div class="brand-row">
          {logo_img}
          <span class="brand-name">evokoa</span>
          <span class="brand-product">pgGraph Playground</span>
        </div>
        """,
        unsafe_allow_html=True,
    )
    filter_text = st.sidebar.text_input("Filter queries...", label_visibility="collapsed", placeholder="Filter queries...")
    normalized_filter = filter_text.strip().lower()
    for section, queries in QUERY_SECTIONS:
        visible_queries = {
            label: sql
            for label, sql in queries.items()
            if not normalized_filter or normalized_filter in label.lower() or normalized_filter in section.lower()
        }
        if not visible_queries:
            continue
        st.sidebar.markdown(f'<div class="workflow">{section}</div>', unsafe_allow_html=True)
        for label, sql in visible_queries.items():
            if st.sidebar.button(label, use_container_width=True):
                st.session_state.sql = sql
                st.session_state.question = QUERY_QUESTIONS.get(label, DEFAULT_QUESTION)
                st.session_state.result = {}
    st.sidebar.divider()
    st.sidebar.link_button("Docs", "https://docs.evokoa.com/pggraph", use_container_width=True)


def main() -> None:
    st.set_page_config(
        page_title="pgGraph Playground",
        page_icon=str(asset_path("favicon.svg")),
        layout="wide",
        initial_sidebar_state="expanded",
    )
    apply_css()
    sidebar()

    if "sql" not in st.session_state:
        st.session_state.sql = DEFAULT_SQL
    if "question" not in st.session_state:
        st.session_state.question = DEFAULT_QUESTION
    if "result" not in st.session_state:
        st.session_state.result = {}

    conn = connection()
    status = fetch_one(conn, "SELECT * FROM graph.status();")

    st.markdown(
        """
        <div class="main-top">
          <div></div>
          <div class="dataset-label">ICIJ Offshore Leaks</div>
        </div>
        """,
        unsafe_allow_html=True,
    )

    st.markdown(
        f"""
        <div class="metric-strip">
          <div class="metric"><div class="metric-label">Nodes</div><div class="metric-value">{status.get("node_count", 0):,}</div></div>
          <div class="metric"><div class="metric-label">Edges</div><div class="metric-value">{status.get("edge_count", 0):,}</div></div>
          <div class="metric"><div class="metric-label">Schema</div><div class="metric-value">{status.get("schema_status", "unknown")}</div></div>
          <div class="metric"><div class="metric-label">Sync</div><div class="metric-value">{status.get("sync_status", "unknown")}</div></div>
        </div>
        """,
        unsafe_allow_html=True,
    )

    st.subheader(st.session_state.question)
    st.caption(PLAYGROUND_CONTEXT)

    left, right = st.columns(2, gap="large")
    with left:
        st.session_state.sql = st.text_area("SQL", value=st.session_state.sql, height=430)
        run_button_slot = st.empty()
        run_clicked = run_button_slot.button("Run SQL", type="primary")
        if run_clicked:
            render_loading_button(run_button_slot)
    with right:
        if run_clicked:
            with st.spinner("Running SQL..."):
                st.session_state.result = run_sql_with_error_handling(st.session_state.sql)
            st.rerun()
        render_result(st.session_state.result)


if __name__ == "__main__":
    main()
