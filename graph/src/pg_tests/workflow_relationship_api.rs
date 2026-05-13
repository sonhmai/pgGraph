// Workflow relationship tests cover path-oriented wrappers and summaries.
// These wrappers add readable paths, endpoint search, and grouped samples on
// top of primitive shortest-path and traversal APIs.

#[pg_test]
fn workflow_path_returns_ordered_hydrated_steps_with_readable_summary() {
    reset_and_create_fixtures();
    build_friendship_fixture_graph();

    let (steps, ordered_ids, readable_path, target_name) = Spi::connect(|client| {
        let result = client
            .select(
                "SELECT count(*)::bigint,
                        array_agg(node_id ORDER BY step),
                        max(readable_path),
                        max(node->>'name') FILTER (WHERE node_id = 'u2')
                   FROM graph.path(
                        'graph_test_users_pgtest'::regclass,
                        'u1',
                        'graph_test_users_pgtest'::regclass,
                        'u2'
                   )",
                None,
                &[],
            )
            .expect("workflow path failed");
        let row = result.first();
        Ok::<_, pgrx::spi::Error>((
            row.get::<i64>(1)?.unwrap_or_default(),
            row.get::<Vec<String>>(2)?.unwrap_or_default(),
            row.get::<String>(3)?.unwrap_or_default(),
            row.get::<String>(4)?.unwrap_or_default(),
        ))
    })
    .expect("workflow path result read failed");

    assert_eq!(steps, 2);
    assert_eq!(ordered_ids, vec!["u1".to_string(), "u2".to_string()]);
    assert_eq!(
        readable_path,
        "graph_test_users_pgtest:u1 --friend--> graph_test_users_pgtest:u2"
    );
    assert_eq!(target_name, "Bob");
}

#[pg_test]
fn workflow_connection_searches_endpoints_and_returns_first_reachable_path() {
    reset_and_create_fixtures();
    build_friendship_fixture_graph();

    let (steps, hop_count, ordered_ids, source_name, target_name, readable_path) =
        Spi::connect(|client| {
            let result = client
                .select(
                    "SELECT count(*)::bigint,
                            max(hop_count),
                            array_agg(node_id ORDER BY step),
                            min(source_table_name),
                            min(target_table_name),
                            max(readable_path)
                       FROM graph.connection(
                            source_key := 'name',
                            source_value := 'Alice',
                            target_key := 'name',
                            target_value := 'Bob',
                            source_table := 'graph_test_users_pgtest'::regclass,
                            target_table := 'graph_test_users_pgtest'::regclass
                       )",
                    None,
                    &[],
                )
                .expect("workflow connection failed");
            let row = result.first();
            Ok::<_, pgrx::spi::Error>((
                row.get::<i64>(1)?.unwrap_or_default(),
                row.get::<i32>(2)?.unwrap_or_default(),
                row.get::<Vec<String>>(3)?.unwrap_or_default(),
                row.get::<String>(4)?.unwrap_or_default(),
                row.get::<String>(5)?.unwrap_or_default(),
                row.get::<String>(6)?.unwrap_or_default(),
            ))
        })
        .expect("workflow connection result read failed");

    assert_eq!(steps, 2);
    assert_eq!(hop_count, 1);
    assert_eq!(ordered_ids, vec!["u1".to_string(), "u2".to_string()]);
    assert_eq!(source_name, "graph_test_users_pgtest");
    assert_eq!(target_name, "graph_test_users_pgtest");
    assert!(readable_path.contains("--friend-->"));
}

#[pg_test]
fn workflow_neighborhood_groups_counts_samples_and_limit_truncation() {
    reset_and_create_fixtures();
    Spi::run(
        "INSERT INTO public.graph_test_users_pgtest (id, name, age)
             VALUES ('u3', 'Carol', 55)",
    )
    .expect("insert neighborhood user failed");
    Spi::run(
        "INSERT INTO public.graph_test_friendships_pgtest (id, user_id, friend_id)
             VALUES ('f2', 'u1', 'u3')",
    )
    .expect("insert neighborhood edge failed");
    build_friendship_fixture_graph();

    let (depth, node_count, sample_count, truncated) = Spi::connect(|client| {
        let result = client
            .select(
                "SELECT depth,
                        node_count,
                        jsonb_array_length(sample_nodes),
                        truncated
                   FROM graph.neighborhood(
                        'name',
                        'Alice',
                        source_table := 'graph_test_users_pgtest'::regclass,
                        max_depth := 1,
                        sample_k := 1,
                        node_limit := 1
                   )",
                None,
                &[],
            )
            .expect("workflow neighborhood failed");
        let row = result.first();
        Ok::<_, pgrx::spi::Error>((
            row.get::<i32>(1)?.unwrap_or_default(),
            row.get::<i64>(2)?.unwrap_or_default(),
            row.get::<i32>(3)?.unwrap_or_default(),
            row.get::<bool>(4)?.unwrap_or(false),
        ))
    })
    .expect("workflow neighborhood result read failed");

    assert_eq!(depth, 1);
    assert_eq!(node_count, 1);
    assert_eq!(sample_count, 1);
    assert!(truncated);
}

#[pg_test]
fn workflow_relationship_wrappers_return_empty_sets_for_unreachable_inputs() {
    reset_and_create_fixtures();
    Spi::run(
        "INSERT INTO public.graph_test_users_pgtest (id, name, age)
             VALUES ('u3', 'Carol', 29)",
    )
    .expect("insert disconnected user failed");
    build_friendship_fixture_graph();

    let no_path_count = Spi::get_one::<i64>(
        "SELECT count(*)
           FROM graph.path(
                'graph_test_users_pgtest'::regclass,
                'u1',
                'graph_test_users_pgtest'::regclass,
                'u3'
           )",
    )
    .expect("workflow no-path query failed")
    .unwrap_or(-1);
    let no_connection_count = Spi::get_one::<i64>(
        "SELECT count(*)
           FROM graph.connection(
                source_key := 'name',
                source_value := 'Alice',
                target_key := 'name',
                target_value := 'Carol',
                source_table := 'graph_test_users_pgtest'::regclass,
                target_table := 'graph_test_users_pgtest'::regclass
           )",
    )
    .expect("workflow no-connection query failed")
    .unwrap_or(-1);
    let no_neighborhood_count = Spi::get_one::<i64>(
        "SELECT count(*)
           FROM graph.neighborhood(
                'name',
                'Nobody',
                source_table := 'graph_test_users_pgtest'::regclass
           )",
    )
    .expect("workflow empty neighborhood query failed")
    .unwrap_or(-1);

    assert_eq!(no_path_count, 0);
    assert_eq!(no_connection_count, 0);
    assert_eq!(no_neighborhood_count, 0);
}
