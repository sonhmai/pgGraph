#[pg_test]
fn traverse_uses_graph_default_max_depth_when_omitted() {
    reset_and_create_fixtures();
    Spi::run("SELECT * FROM graph.auto_discover('public')").expect("auto_discover failed");
    Spi::run("SET graph.default_max_depth = 1").expect("set default depth failed");

    let max_depth_default = Spi::get_one::<i32>(
        "SELECT max(depth) FROM graph.traverse('graph_test_users_pgtest'::regclass, 'u1')",
    )
    .expect("default depth traverse failed")
    .unwrap_or(-1);
    let max_depth_explicit = Spi::get_one::<i32>(
        "SELECT max(depth) FROM graph.traverse('graph_test_users_pgtest'::regclass, 'u1', 5)",
    )
    .expect("explicit depth traverse failed")
    .unwrap_or(-1);

    assert_eq!(max_depth_default, 1);
    assert!(max_depth_explicit >= 2);
}

#[pg_test]
fn traverse_hydrates_source_rows_by_default_and_can_opt_out() {
    reset_and_create_fixtures();
    Spi::run("SELECT * FROM graph.auto_discover('public')").expect("auto_discover failed");

    let hydrated_name = Spi::get_one::<String>(
        "SELECT node->>'name'
             FROM graph.traverse('graph_test_users_pgtest'::regclass, 'u1', 0)
             WHERE node_id = 'u1'",
    )
    .expect("hydrated traverse failed")
    .expect("hydrated node missing");
    let opt_out_count = Spi::get_one::<i64>(
        "SELECT count(*)
             FROM graph.traverse('graph_test_users_pgtest'::regclass, 'u1', 0, hydrate := false)
             WHERE node IS NULL",
    )
    .expect("coordinate-only traverse failed")
    .unwrap_or(0);

    assert_eq!(hydrated_name, "Alice");
    assert_eq!(opt_out_count, 1);
}

#[pg_test]
fn shortest_path_hydrates_source_rows_by_default_and_can_opt_out() {
    reset_and_create_fixtures();
    Spi::run("SELECT * FROM graph.auto_discover('public')").expect("auto_discover failed");

    let hydrated_name = Spi::get_one::<String>(
        "SELECT node->>'name'
             FROM graph.shortest_path(
                'graph_test_users_pgtest'::regclass,
                'u1',
                'graph_test_users_pgtest'::regclass,
                'u2',
                5
             )
             WHERE node_id = 'u2'",
    )
    .expect("hydrated shortest_path failed")
    .expect("hydrated path node missing");
    let opt_out_count = Spi::get_one::<i64>(
        "SELECT count(*)
             FROM graph.shortest_path(
                'graph_test_users_pgtest'::regclass,
                'u1',
                'graph_test_users_pgtest'::regclass,
                'u2',
                5,
                hydrate := false
             )
             WHERE node IS NULL",
    )
    .expect("coordinate-only shortest_path failed")
    .unwrap_or(0);

    assert_eq!(hydrated_name, "Bob");
    assert_eq!(opt_out_count, 3);
}

#[pg_test]
fn shortest_path_v1_acceptance_shape_limits_and_empty_results() {
    reset_and_create_fixtures();
    Spi::run(
        "INSERT INTO public.graph_test_users_pgtest (id, name, age)
             VALUES ('u3', 'Carol', 29)",
    )
    .expect("insert disconnected user failed");
    Spi::run("SELECT * FROM graph.auto_discover('public')").expect("auto_discover failed");

    let column_shape = Spi::get_one::<bool>(
            "SELECT pg_get_function_result(p.oid) =
                    'TABLE(step integer, node_table oid, node_id text, edge_label text, node jsonb, node_table_name text)'
             FROM pg_proc p
             JOIN pg_namespace n ON n.oid = p.pronamespace
             WHERE n.nspname = 'graph'
               AND p.proname = 'shortest_path'",
        )
        .expect("shortest_path shape inspection failed")
        .unwrap_or(false);
    let ordered_path = Spi::get_one::<Vec<String>>(
        "SELECT array_agg(node_id ORDER BY step)
             FROM graph.shortest_path(
                'graph_test_users_pgtest'::regclass,
                'u1',
                'graph_test_users_pgtest'::regclass,
                'u2',
                5,
                hydrate := false
             )",
    )
    .expect("ordered shortest_path failed")
    .unwrap_or_default();
    let max_depth_blocked = Spi::get_one::<i64>(
        "SELECT count(*)
             FROM graph.shortest_path(
                'graph_test_users_pgtest'::regclass,
                'u1',
                'graph_test_users_pgtest'::regclass,
                'u2',
                0,
                hydrate := false
             )",
    )
    .expect("max_depth shortest_path failed")
    .unwrap_or(-1);
    let no_path_count = Spi::get_one::<i64>(
        "SELECT count(*)
             FROM graph.shortest_path(
                'graph_test_users_pgtest'::regclass,
                'u1',
                'graph_test_users_pgtest'::regclass,
                'u3',
                5,
                hydrate := false
             )",
    )
    .expect("no-path shortest_path failed")
    .unwrap_or(-1);

    assert!(column_shape);
    assert_eq!(
        ordered_path,
        vec!["u1".to_string(), "f1".to_string(), "u2".to_string()]
    );
    assert_eq!(max_depth_blocked, 0);
    assert_eq!(no_path_count, 0);
}

#[pg_test]
fn aggregate_sums_averages_and_counts_returned_nodes() {
    reset_and_create_fixtures();
    build_friendship_fixture_graph();

    let result = Spi::get_one::<pgrx::JsonB>(
            "WITH req AS (
                SELECT jsonb_build_object(
                    'starts',
                    jsonb_build_array(graph.node_ref_string('graph_test_users_pgtest'::regclass, 'u1')),
                    'direction', 'out',
                    'min_depth', 0,
                    'max_depth', 1,
                    'edge_types', jsonb_build_array('friend'),
                    'node_tables', jsonb_build_array('graph_test_users_pgtest')
                ) AS traversal
             )
             SELECT graph.aggregate(
                traversal,
                '{
                    \"sum\":[{\"table\":\"graph_test_users_pgtest\",\"column\":\"age\",\"as\":\"total_age\"}],
                    \"avg\":[{\"table\":\"graph_test_users_pgtest\",\"column\":\"age\",\"as\":\"avg_age\"}],
                    \"count\":[{\"table\":\"graph_test_users_pgtest\",\"column\":\"id\",\"as\":\"user_count\"}]
                }'::jsonb
             )
             FROM req",
        )
        .expect("aggregate query failed")
        .expect("aggregate result missing")
        .0;

    assert_eq!(
        result.get("total_age").and_then(|value| value.as_f64()),
        Some(78.0)
    );
    assert_eq!(
        result.get("avg_age").and_then(|value| value.as_f64()),
        Some(39.0)
    );
    assert_eq!(
        result.get("user_count").and_then(|value| value.as_u64()),
        Some(2)
    );
}

#[pg_test]
fn aggregate_supports_chosen_parent_path_scope() {
    reset_and_create_fixtures();
    Spi::run(
        "INSERT INTO public.graph_test_users_pgtest (id, name, age)
             VALUES ('u3', 'Carol', 29)",
    )
    .expect("insert path endpoint failed");
    Spi::run(
        "INSERT INTO public.graph_test_friendships_pgtest (id, user_id, friend_id)
             VALUES ('f2', 'u2', 'u3')",
    )
    .expect("insert second friendship failed");
    build_friendship_fixture_graph();

    let result = Spi::get_one::<pgrx::JsonB>(
            "WITH req AS (
                SELECT jsonb_build_object(
                    'starts',
                    jsonb_build_array(graph.node_ref_string('graph_test_users_pgtest'::regclass, 'u1')),
                    'direction', 'out',
                    'min_depth', 2,
                    'max_depth', 2,
                    'edge_types', jsonb_build_array('friend'),
                    'node_tables', jsonb_build_array('graph_test_users_pgtest')
                ) AS traversal
             )
             SELECT graph.aggregate(
                traversal,
                '{\"sum\":[{\"table\":\"graph_test_users_pgtest\",\"column\":\"age\",\"as\":\"path_age\"}]}'::jsonb,
                scope := 'chosen_parent_path'
             )
             FROM req",
        )
        .expect("chosen parent path aggregate query failed")
        .expect("chosen parent path aggregate result missing")
        .0;

    assert_eq!(
        result.get("path_age").and_then(|value| value.as_f64()),
        Some(107.0)
    );
}

#[pg_test]
fn path_count_estimate_reports_exact_and_capped_counts() {
    reset_and_create_fixtures();
    build_friendship_fixture_graph();

    let exact = Spi::get_one::<bool>(
            "WITH req AS (
                SELECT jsonb_build_object(
                    'starts',
                    jsonb_build_array(graph.node_ref_string('graph_test_users_pgtest'::regclass, 'u1')),
                    'direction', 'out',
                    'min_depth', 0,
                    'max_depth', 1,
                    'edge_types', jsonb_build_array('friend'),
                    'node_tables', jsonb_build_array('graph_test_users_pgtest')
                ) AS traversal
             )
             SELECT estimated_paths = 2 AND exact AND NOT capped
             FROM graph.path_count_estimate((SELECT traversal FROM req))",
        )
        .expect("exact path count estimate failed")
        .unwrap_or(false);

    Spi::run("SET graph.max_exact_path_count = 1").expect("set path count cap failed");
    let capped = Spi::get_one::<bool>(
            "WITH req AS (
                SELECT jsonb_build_object(
                    'starts',
                    jsonb_build_array(graph.node_ref_string('graph_test_users_pgtest'::regclass, 'u1')),
                    'direction', 'out',
                    'min_depth', 0,
                    'max_depth', 1,
                    'edge_types', jsonb_build_array('friend'),
                    'node_tables', jsonb_build_array('graph_test_users_pgtest')
                ) AS traversal
             )
             SELECT estimated_paths = 1 AND NOT exact AND capped
             FROM graph.path_count_estimate((SELECT traversal FROM req))",
        )
        .expect("capped path count estimate failed")
        .unwrap_or(false);
    Spi::run("SET graph.max_exact_path_count = 100000").expect("reset path count cap failed");

    assert!(exact);
    assert!(capped);
}

#[pg_test]
fn aggregate_all_possible_paths_counts_duplicate_path_occurrences() {
    reset_and_create_fixtures();
    Spi::run(
        "INSERT INTO public.graph_test_users_pgtest (id, name, age)
             VALUES ('u3', 'Carol', 29)",
    )
    .expect("insert branch user failed");
    Spi::run(
        "INSERT INTO public.graph_test_friendships_pgtest (id, user_id, friend_id)
             VALUES ('f2', 'u1', 'u3')",
    )
    .expect("insert branch friendship failed");
    build_friendship_fixture_graph();

    let result = Spi::get_one::<pgrx::JsonB>(
            "WITH req AS (
                SELECT jsonb_build_object(
                    'starts',
                    jsonb_build_array(graph.node_ref_string('graph_test_users_pgtest'::regclass, 'u1')),
                    'direction', 'out',
                    'min_depth', 0,
                    'max_depth', 1,
                    'edge_types', jsonb_build_array('friend'),
                    'node_tables', jsonb_build_array('graph_test_users_pgtest')
                ) AS traversal
             )
             SELECT graph.aggregate(
                traversal,
                '{
                    \"sum\":[{\"table\":\"graph_test_users_pgtest\",\"column\":\"age\",\"as\":\"path_age\"}],
                    \"count\":[{\"table\":\"graph_test_users_pgtest\",\"column\":\"id\",\"as\":\"path_node_count\"}]
                }'::jsonb,
                scope := 'all_possible_paths'
             )
             FROM req",
        )
        .expect("all possible paths aggregate query failed")
        .expect("all possible paths aggregate result missing")
        .0;

    assert_eq!(
        result.get("path_age").and_then(|value| value.as_f64()),
        Some(181.0)
    );
    assert_eq!(
        result
            .get("path_node_count")
            .and_then(|value| value.as_u64()),
        Some(5)
    );
}

#[pg_test]
fn aggregate_and_path_count_enforce_strict_json_contract() {
    reset_and_create_fixtures();
    build_friendship_fixture_graph();

    let bad_traversal_key = sql_raises(
            "WITH req AS (
                SELECT jsonb_build_object(
                    'starts',
                    jsonb_build_array(graph.node_ref_string('graph_test_users_pgtest'::regclass, 'u1')),
                    'direction', 'out',
                    'min_depth', 0,
                    'max_depth', 1,
                    'unexpected', true
                ) AS traversal
             )
             SELECT * FROM graph.path_count_estimate((SELECT traversal FROM req))",
        );
    let bad_aggregate_key = sql_raises(
            "WITH req AS (
                SELECT jsonb_build_object(
                    'starts',
                    jsonb_build_array(graph.node_ref_string('graph_test_users_pgtest'::regclass, 'u1')),
                    'direction', 'out',
                    'min_depth', 0,
                    'max_depth', 1
                ) AS traversal
             )
             SELECT graph.aggregate(
                traversal,
                '{\"median\":[{\"table\":\"graph_test_users_pgtest\",\"column\":\"age\",\"as\":\"age_median\"}]}'::jsonb
             )
             FROM req",
        );
    let all_paths_rejected = sql_raises(
            "WITH req AS (
                SELECT jsonb_build_object(
                    'starts',
                    jsonb_build_array(graph.node_ref_string('graph_test_users_pgtest'::regclass, 'u1')),
                    'direction', 'out',
                    'min_depth', 0,
                    'max_depth', 1
                ) AS traversal
             )
             SELECT graph.aggregate(
                traversal,
                '{\"count\":[{\"table\":\"graph_test_users_pgtest\",\"column\":\"id\",\"as\":\"user_count\"}]}'::jsonb,
                scope := 'all_possible_paths',
                path_limit := 1
             )
             FROM req",
        );

    assert!(bad_traversal_key);
    assert!(bad_aggregate_key);
    assert!(all_paths_rejected);
}

