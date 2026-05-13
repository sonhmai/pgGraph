// Workflow-level SQL functions for common application and AI-tool queries.
//
// These wrappers keep the primitive APIs stable while providing smaller,
// opinionated call shapes for the common search, expand, relationship, and
// path workflows documented in `docs/user_guide/querying.mdx`.

/// Search source rows with hydrated output and app-friendly row pagination.
#[pg_extern(schema = "graph")]
#[allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    reason = "pgrx SQL ABI exposes each SQL argument and row column"
)]
fn find(
    property_key: &str,
    property_value: &str,
    table_name: default!(Option<pgrx::pg_sys::Oid>, "NULL"),
    mode: default!(&str, "'contains'"),
    case_sensitive: default!(bool, "false"),
    max_rows: default!(i32, 20),
    row_offset: default!(i32, 0),
    tenant: default!(Option<String>, "NULL"),
) -> TableIterator<
    'static,
    (
        name!(node_table, pgrx::pg_sys::Oid),
        name!(node_table_name, String),
        name!(node_id, String),
        name!(match_type, String),
        name!(score, f32),
        name!(verified, bool),
        name!(rank, i32),
        name!(node, Option<pgrx::JsonB>),
    ),
> {
    with_panic_boundary("find()", || {
        validate_nonnegative_arg(max_rows, "max_rows").unwrap_or_else(|err| err.report());
        let row_offset_usize =
            validate_nonnegative_arg(row_offset, "row_offset").unwrap_or_else(|err| err.report());
        let rows = search(
            property_key,
            property_value,
            table_name,
            mode,
            case_sensitive,
            max_rows,
            row_offset,
            tenant,
            true,
        )
        .enumerate()
        .map(
            |(idx, (node_table, node_id, match_type, score, verified, node, node_table_name))| {
                (
                    node_table,
                    node_table_name,
                    node_id,
                    match_type,
                    score,
                    verified,
                    rank_from_offset(row_offset_usize, idx),
                    node,
                )
            },
        )
        .collect::<Vec<_>>();
        TableIterator::new(rows)
    })
}

/// Expand from a known graph node using hydrated rows and readable paths.
#[pg_extern(schema = "graph")]
#[allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    reason = "pgrx SQL ABI exposes each SQL argument and row column"
)]
fn expand(
    seed_table: pgrx::pg_sys::Oid,
    seed_id: &str,
    max_depth: default!(i32, "current_setting('graph.default_max_depth')::int"),
    edge_types: default!(Option<Vec<String>>, "NULL"),
    direction: default!(&str, "'any'"),
    target_table: default!(Option<pgrx::pg_sys::Oid>, "NULL"),
    target_tables: default!(Option<Vec<pgrx::pg_sys::Oid>>, "NULL"),
    where_node: default!(Option<pgrx::JsonB>, "NULL"),
    tenant: default!(Option<String>, "NULL"),
    max_rows: default!(i32, 50),
    row_offset: default!(i32, 0),
    include_start: default!(bool, "false"),
) -> TableIterator<
    'static,
    (
        name!(root_table, pgrx::pg_sys::Oid),
        name!(root_id, String),
        name!(root_table_name, String),
        name!(node_table, pgrx::pg_sys::Oid),
        name!(node_table_name, String),
        name!(node_id, String),
        name!(depth, i32),
        name!(rank, i32),
        name!(path, pgrx::JsonB),
        name!(edge_path, pgrx::JsonB),
        name!(readable_path, String),
        name!(node, Option<pgrx::JsonB>),
        name!(truncated, bool),
    ),
> {
    with_panic_boundary("expand()", || {
        let row_offset_usize =
            validate_nonnegative_arg(row_offset, "row_offset").unwrap_or_else(|err| err.report());
        validate_nonnegative_arg(max_rows, "max_rows").unwrap_or_else(|err| err.report());
        let node_tables =
            workflow_target_tables(target_table, target_tables).unwrap_or_else(|err| err.report());
        let rows = traverse(
            seed_table,
            seed_id,
            max_depth,
            edge_types,
            direction,
            node_tables,
            where_node,
            tenant,
            "bfs",
            "node_global",
            include_start,
            true,
            max_rows,
            row_offset,
            config::MAX_NODES.get(),
            config::MAX_FRONTIER.get(),
        )
        .collect::<Vec<_>>();
        let truncated = max_rows > 0 && rows.len() == max_rows as usize;
        TableIterator::new(
            rows.into_iter()
                .enumerate()
                .map(
                    |(
                        idx,
                        (
                            root_table,
                            root_id,
                            node_table,
                            node_id,
                            depth,
                            path,
                            edge_path,
                            node,
                            root_table_name,
                            node_table_name,
                        ),
                    )| {
                        let readable_path = readable_path(&path, &edge_path)
                            .unwrap_or_else(|err| err.report());
                        (
                            root_table,
                            root_id,
                            root_table_name,
                            node_table,
                            node_table_name,
                            node_id,
                            depth,
                            rank_from_offset(row_offset_usize, idx),
                            path,
                            edge_path,
                            readable_path,
                            node,
                            truncated,
                        )
                    },
                )
                .collect::<Vec<_>>(),
        )
    })
}

/// Search for a seed row, traverse related nodes, and hydrate the final page.
#[pg_extern(schema = "graph")]
#[allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    reason = "pgrx SQL ABI exposes each SQL argument and row column"
)]
fn find_related(
    property_key: &str,
    property_value: &str,
    source_table: default!(Option<pgrx::pg_sys::Oid>, "NULL"),
    search_mode: default!(
        &str,
        "COALESCE(NULLIF(current_setting('graph.default_search_mode'), ''), 'contains')"
    ),
    case_sensitive: default!(
        bool,
        "current_setting('graph.default_case_sensitive')::boolean"
    ),
    search_max_rows: default!(i32, 1),
    search_row_offset: default!(i32, 0),
    max_depth: default!(i32, "current_setting('graph.default_max_depth')::int"),
    edge_types: default!(Option<Vec<String>>, "NULL"),
    direction: default!(&str, "'any'"),
    target_table: default!(Option<pgrx::pg_sys::Oid>, "NULL"),
    target_tables: default!(Option<Vec<pgrx::pg_sys::Oid>>, "NULL"),
    where_node: default!(Option<pgrx::JsonB>, "NULL"),
    tenant: default!(Option<String>, "NULL"),
    max_rows: default!(i32, 20),
    row_offset: default!(i32, 0),
    include_counts: default!(bool, "false"),
    candidate_limit: default!(i32, 10000),
    include_start: default!(bool, "false"),
) -> TableIterator<
    'static,
    (
        name!(root_table, pgrx::pg_sys::Oid),
        name!(root_id, String),
        name!(root_table_name, String),
        name!(node_table, pgrx::pg_sys::Oid),
        name!(node_table_name, String),
        name!(node_id, String),
        name!(depth, i32),
        name!(score, f32),
        name!(rank, i32),
        name!(path, pgrx::JsonB),
        name!(edge_path, pgrx::JsonB),
        name!(readable_path, String),
        name!(node, Option<pgrx::JsonB>),
        name!(candidate_count, Option<i64>),
        name!(filtered_count, Option<i64>),
        name!(truncated, bool),
    ),
> {
    with_panic_boundary("find_related()", || {
        let row_offset_usize =
            validate_nonnegative_arg(row_offset, "row_offset").unwrap_or_else(|err| err.report());
        validate_nonnegative_arg(max_rows, "max_rows").unwrap_or_else(|err| err.report());
        validate_nonnegative_arg(search_max_rows, "search_max_rows")
            .unwrap_or_else(|err| err.report());
        validate_nonnegative_arg(candidate_limit, "candidate_limit")
            .unwrap_or_else(|err| err.report());
        let node_tables =
            workflow_target_tables(target_table, target_tables).unwrap_or_else(|err| err.report());
        let filtered = traverse_search(
            property_key,
            property_value,
            source_table,
            search_mode,
            case_sensitive,
            search_max_rows,
            search_row_offset,
            max_depth,
            edge_types.clone(),
            direction,
            node_tables,
            where_node,
            tenant.clone(),
            "bfs",
            "node_per_root",
            include_start,
            false,
            candidate_limit,
            0,
        )
        .collect::<Vec<_>>();
        let broad_count = if include_counts {
            Some(
                traverse_search(
                    property_key,
                    property_value,
                    source_table,
                    search_mode,
                    case_sensitive,
                    search_max_rows,
                    search_row_offset,
                    max_depth,
                    edge_types,
                    direction,
                    None,
                    None,
                    tenant,
                    "bfs",
                    "node_per_root",
                    include_start,
                    false,
                    candidate_limit,
                    0,
                )
                .count() as i64,
            )
        } else {
            None
        };
        let filtered_count = include_counts.then_some(filtered.len() as i64);
        let truncated = candidate_limit > 0 && filtered.len() == candidate_limit as usize;
        let rows = filtered
            .into_iter()
            .skip(row_offset_usize)
            .take(max_rows as usize)
            .enumerate()
            .map(
                |(
                    idx,
                    (
                        root_table,
                        root_id,
                        node_table,
                        node_id,
                        depth,
                        path,
                        edge_path,
                        _node,
                        root_table_name,
                        node_table_name,
                    ),
                )| {
                    let node =
                        hydrate_node(node_table.to_u32(), &node_id).unwrap_or_else(|err| err.report());
                    let readable_path = readable_path(&path, &edge_path)
                        .unwrap_or_else(|err| err.report());
                    (
                        root_table,
                        root_id,
                        root_table_name,
                        node_table,
                        node_table_name,
                        node_id,
                        depth,
                        1.0,
                        rank_from_offset(row_offset_usize, idx),
                        path,
                        edge_path,
                        readable_path,
                        node,
                        broad_count,
                        filtered_count,
                        truncated,
                    )
                },
            )
            .collect::<Vec<_>>();
        TableIterator::new(rows)
    })
}

/// Return a hydrated shortest path with a repeated readable path summary.
#[pg_extern(schema = "graph")]
#[allow(
    clippy::type_complexity,
    reason = "pgrx SQL ABI row shape is intentionally explicit"
)]
fn path(
    source_table: pgrx::pg_sys::Oid,
    source_id: &str,
    target_table: pgrx::pg_sys::Oid,
    target_id: &str,
    max_depth: default!(i32, 20),
) -> TableIterator<
    'static,
    (
        name!(step, i32),
        name!(node_table, pgrx::pg_sys::Oid),
        name!(node_table_name, String),
        name!(node_id, String),
        name!(edge_label, Option<String>),
        name!(readable_path, String),
        name!(node, Option<pgrx::JsonB>),
    ),
> {
    with_panic_boundary("path()", || {
        let rows = shortest_path(source_table, source_id, target_table, target_id, max_depth, true)
            .collect::<Vec<_>>();
        let readable = readable_shortest_path(&rows);
        TableIterator::new(
            rows.into_iter()
                .map(
                    |(step, node_table, node_id, edge_label, node, node_table_name)| {
                        (
                            step,
                            node_table,
                            node_table_name,
                            node_id,
                            edge_label,
                            readable.clone(),
                            node,
                        )
                    },
                )
                .collect::<Vec<_>>(),
        )
    })
}

/// Search both endpoint names and return the shortest discovered connection.
#[pg_extern(schema = "graph")]
#[allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    reason = "pgrx SQL ABI exposes each SQL argument and row column"
)]
fn connection(
    source_key: &str,
    source_value: &str,
    target_key: &str,
    target_value: &str,
    source_table: default!(Option<pgrx::pg_sys::Oid>, "NULL"),
    target_table: default!(Option<pgrx::pg_sys::Oid>, "NULL"),
    source_k: default!(i32, 3),
    target_k: default!(i32, 3),
    search_mode: default!(&str, "'contains'"),
    max_depth: default!(i32, 6),
) -> TableIterator<
    'static,
    (
        name!(source_table_name, String),
        name!(source_id, String),
        name!(target_table_name, String),
        name!(target_id, String),
        name!(hop_count, i32),
        name!(step, i32),
        name!(node_table, pgrx::pg_sys::Oid),
        name!(node_table_name, String),
        name!(node_id, String),
        name!(edge_label, Option<String>),
        name!(readable_path, String),
        name!(node, Option<pgrx::JsonB>),
    ),
> {
    with_panic_boundary("connection()", || {
        validate_nonnegative_arg(source_k, "source_k").unwrap_or_else(|err| err.report());
        validate_nonnegative_arg(target_k, "target_k").unwrap_or_else(|err| err.report());
        let sources = search(
            source_key,
            source_value,
            source_table,
            search_mode,
            false,
            source_k,
            0,
            None,
            false,
        )
        .collect::<Vec<_>>();
        let targets = search(
            target_key,
            target_value,
            target_table,
            search_mode,
            false,
            target_k,
            0,
            None,
            false,
        )
        .collect::<Vec<_>>();

        for (source_oid, source_id, _match_type, _score, source_verified, _node, source_name) in
            &sources
        {
            if !source_verified {
                continue;
            }
            for (target_oid, target_id, _match_type, _score, target_verified, _node, target_name) in
                &targets
            {
                if !target_verified {
                    continue;
                }
                let path_rows =
                    shortest_path(*source_oid, source_id, *target_oid, target_id, max_depth, true)
                        .collect::<Vec<_>>();
                if path_rows.is_empty() {
                    continue;
                }
                let readable = readable_shortest_path(&path_rows);
                let hop_count = path_rows.len().saturating_sub(1).min(i32::MAX as usize) as i32;
                let rows = path_rows
                    .into_iter()
                    .map(
                        |(step, node_table, node_id, edge_label, node, node_table_name)| {
                            (
                                source_name.clone(),
                                source_id.clone(),
                                target_name.clone(),
                                target_id.clone(),
                                hop_count,
                                step,
                                node_table,
                                node_table_name,
                                node_id,
                                edge_label,
                                readable.clone(),
                                node,
                            )
                        },
                    )
                    .collect::<Vec<_>>();
                return TableIterator::new(rows);
            }
        }
        TableIterator::new(Vec::new())
    })
}

/// Summarize graph neighborhood size by depth and table before hydration.
#[pg_extern(schema = "graph")]
#[allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    reason = "pgrx SQL ABI exposes each SQL argument and row column"
)]
fn neighborhood(
    property_key: &str,
    property_value: &str,
    source_table: default!(Option<pgrx::pg_sys::Oid>, "NULL"),
    search_mode: default!(&str, "'contains'"),
    search_max_rows: default!(i32, 1),
    max_depth: default!(i32, 4),
    edge_types: default!(Option<Vec<String>>, "NULL"),
    direction: default!(&str, "'any'"),
    tenant: default!(Option<String>, "NULL"),
    sample_k: default!(i32, 5),
    node_limit: default!(i32, 10000),
) -> TableIterator<
    'static,
    (
        name!(depth, i32),
        name!(node_table, pgrx::pg_sys::Oid),
        name!(node_table_name, String),
        name!(node_count, i64),
        name!(sample_nodes, pgrx::JsonB),
        name!(truncated, bool),
    ),
> {
    with_panic_boundary("neighborhood()", || {
        validate_nonnegative_arg(search_max_rows, "search_max_rows")
            .unwrap_or_else(|err| err.report());
        let sample_k =
            validate_nonnegative_arg(sample_k, "sample_k").unwrap_or_else(|err| err.report());
        validate_nonnegative_arg(node_limit, "node_limit").unwrap_or_else(|err| err.report());
        let rows = traverse_search(
            property_key,
            property_value,
            source_table,
            search_mode,
            false,
            search_max_rows,
            0,
            max_depth,
            edge_types,
            direction,
            None,
            None,
            tenant,
            "bfs",
            "node_per_root",
            false,
            false,
            node_limit,
            0,
        )
        .collect::<Vec<_>>();
        let truncated = node_limit > 0 && rows.len() == node_limit as usize;
        let mut grouped = std::collections::BTreeMap::<(i32, u32, String), Vec<String>>::new();
        for (_root_table, _root_id, node_table, node_id, depth, _path, _edge_path, _node, _root_table_name, node_table_name) in rows {
            grouped
                .entry((depth, node_table.to_u32(), node_table_name))
                .or_default()
                .push(node_id);
        }
        let summary = grouped
            .into_iter()
            .map(|((depth, table_oid, node_table_name), node_ids)| {
                let sample_nodes = node_ids
                    .iter()
                    .take(sample_k)
                    .map(|node_id| {
                        serde_json::json!({
                            "table": node_table_name,
                            "id": node_id,
                        })
                    })
                    .collect::<Vec<_>>();
                (
                    depth,
                    pgrx::pg_sys::Oid::from_u32(table_oid),
                    node_table_name,
                    node_ids.len() as i64,
                    pgrx::JsonB(serde_json::Value::Array(sample_nodes)),
                    truncated,
                )
            })
            .collect::<Vec<_>>();
        TableIterator::new(summary)
    })
}

fn workflow_target_tables(
    target_table: Option<pgrx::pg_sys::Oid>,
    target_tables: Option<Vec<pgrx::pg_sys::Oid>>,
) -> safety::GraphResult<Option<Vec<pgrx::pg_sys::Oid>>> {
    match (target_table, target_tables) {
        (Some(_), Some(_)) => Err(safety::GraphError::InvalidFilter {
            reason: "target_table and target_tables cannot both be set".to_string(),
        }),
        (Some(table), None) => Ok(Some(vec![table])),
        (None, Some(tables)) => Ok(Some(tables)),
        (None, None) => Ok(None),
    }
}

fn validate_nonnegative_arg(value: i32, name: &str) -> safety::GraphResult<usize> {
    usize_from_nonnegative(value, name)
}

fn rank_from_offset(offset: usize, idx: usize) -> i32 {
    offset.saturating_add(idx).saturating_add(1).min(i32::MAX as usize) as i32
}

fn readable_path(path: &pgrx::JsonB, edge_path: &pgrx::JsonB) -> safety::GraphResult<String> {
    format_path_value(&path.0, &edge_path.0, " | ")
}

type WorkflowPathRow = (
    i32,
    pgrx::pg_sys::Oid,
    String,
    Option<String>,
    Option<pgrx::JsonB>,
    String,
);

fn readable_shortest_path(rows: &[WorkflowPathRow]) -> String {
    rows.windows(2)
        .map(|pair| {
            let from = &pair[0];
            let to = &pair[1];
            let label = to.edge_label_string();
            format!("{}:{} --{}--> {}:{}", from.5, from.2, label, to.5, to.2)
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

trait ShortestPathTupleExt {
    fn edge_label_string(&self) -> &str;
}

impl ShortestPathTupleExt for WorkflowPathRow {
    fn edge_label_string(&self) -> &str {
        self.3.as_deref().unwrap_or("")
    }
}
