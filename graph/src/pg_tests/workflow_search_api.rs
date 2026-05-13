// Workflow search and expansion tests cover wrapper-specific SQL contracts.
// Primitive search/traversal tests own the deeper engine behavior; these tests
// lock down aliases, defaults, pagination, ranking, hydration, and counts.

#[pg_test]
fn workflow_find_returns_hydrated_ranked_rows_with_aliases() {
    reset_and_create_fixtures();
    build_friendship_fixture_graph();

    let (matched, node_table_matches, table_name_present, rank, name) = Spi::connect(|client| {
        let result = client
            .select(
                "SELECT count(*)::bigint,
                        bool_and(node_table = 'graph_test_users_pgtest'::regclass),
                        bool_and(node_table_name <> ''),
                        min(rank),
                        min(node->>'name')
                   FROM graph.find(
                        'name',
                        'Alice',
                        table_name := 'graph_test_users_pgtest'::regclass,
                        mode := 'exact',
                        max_rows := 1,
                        row_offset := 0
                   )",
                None,
                &[],
            )
            .expect("workflow find failed");
        let row = result.first();
        Ok::<_, pgrx::spi::Error>((
            row.get::<i64>(1)?.unwrap_or_default(),
            row.get::<bool>(2)?.unwrap_or(false),
            row.get::<bool>(3)?.unwrap_or(false),
            row.get::<i32>(4)?.unwrap_or_default(),
            row.get::<String>(5)?.unwrap_or_default(),
        ))
    })
    .expect("workflow find result read failed");

    let no_match_count = Spi::get_one::<i64>(
        "SELECT count(*)
           FROM graph.find(
                'name',
                'Nobody',
                table_name := 'graph_test_users_pgtest'::regclass,
                mode := 'exact'
           )",
    )
    .expect("workflow find empty result failed")
    .unwrap_or(-1);

    assert_eq!(matched, 1);
    assert!(node_table_matches);
    assert!(table_name_present);
    assert_eq!(rank, 1);
    assert_eq!(name, "Alice");
    assert_eq!(no_match_count, 0);
}

#[pg_test]
fn workflow_expand_defaults_exclude_start_and_can_page_hydrated_rows() {
    reset_and_create_fixtures();
    build_friendship_fixture_graph();

    let (matched, start_rows, node_id, depth, rank, readable_path, name, truncated) =
        Spi::connect(|client| {
            let result = client
                .select(
                    "SELECT count(*)::bigint,
                            count(*) FILTER (WHERE node_id = 'u1'),
                            min(node_id),
                            min(depth),
                            min(rank),
                            max(readable_path),
                            min(node->>'name'),
                            bool_or(truncated)
                       FROM graph.expand(
                            'graph_test_users_pgtest'::regclass,
                            'u1',
                            max_depth := 1,
                            target_table := 'graph_test_users_pgtest'::regclass,
                            max_rows := 10
                       )",
                    None,
                    &[],
                )
                .expect("workflow expand failed");
            let row = result.first();
            Ok::<_, pgrx::spi::Error>((
                row.get::<i64>(1)?.unwrap_or_default(),
                row.get::<i64>(2)?.unwrap_or_default(),
                row.get::<String>(3)?.unwrap_or_default(),
                row.get::<i32>(4)?.unwrap_or_default(),
                row.get::<i32>(5)?.unwrap_or_default(),
                row.get::<String>(6)?.unwrap_or_default(),
                row.get::<String>(7)?.unwrap_or_default(),
                row.get::<bool>(8)?.unwrap_or(false),
            ))
        })
        .expect("workflow expand result read failed");

    let include_start_ids = Spi::get_one::<Vec<String>>(
        "SELECT array_agg(node_id ORDER BY rank)
           FROM graph.expand(
                'graph_test_users_pgtest'::regclass,
                'u1',
                max_depth := 1,
                target_table := 'graph_test_users_pgtest'::regclass,
                max_rows := 10,
                include_start := true
           )",
    )
    .expect("workflow expand include_start failed")
    .unwrap_or_default();

    assert_eq!(matched, 1);
    assert_eq!(start_rows, 0);
    assert_eq!(node_id, "u2");
    assert_eq!(depth, 1);
    assert_eq!(rank, 1);
    assert!(readable_path.contains("--friend-->"));
    assert_eq!(name, "Bob");
    assert!(!truncated);
    assert_eq!(include_start_ids, vec!["u1".to_string(), "u2".to_string()]);
}

#[pg_test]
fn workflow_find_related_counts_filtering_and_pages_after_candidates() {
    reset_and_create_fixtures();
    Spi::run(
        "INSERT INTO public.graph_test_users_pgtest (id, name, age)
             VALUES ('u3', 'Carol', 55)",
    )
    .expect("insert related user failed");
    Spi::run(
        "INSERT INTO public.graph_test_friendships_pgtest (id, user_id, friend_id)
             VALUES ('f2', 'u1', 'u3')",
    )
    .expect("insert related edge failed");
    build_friendship_fixture_graph();

    let (node_id, rank, name, candidate_count, filtered_count, truncated) =
        Spi::connect(|client| {
            let result = client
                .select(
                    "SELECT node_id,
                            rank,
                            node->>'name',
                            candidate_count,
                            filtered_count,
                            truncated
                       FROM graph.find_related(
                            'name',
                            'Alice',
                            source_table := 'graph_test_users_pgtest'::regclass,
                            max_depth := 1,
                            target_table := 'graph_test_users_pgtest'::regclass,
                            where_node := graph.gt('age', 40),
                            max_rows := 1,
                            row_offset := 1,
                            include_counts := true
                       )",
                    None,
                    &[],
                )
                .expect("workflow find_related page failed");
            let row = result.first();
            Ok::<_, pgrx::spi::Error>((
                row.get::<String>(1)?.unwrap_or_default(),
                row.get::<i32>(2)?.unwrap_or_default(),
                row.get::<String>(3)?.unwrap_or_default(),
                row.get::<i64>(4)?.unwrap_or_default(),
                row.get::<i64>(5)?.unwrap_or_default(),
                row.get::<bool>(6)?.unwrap_or(false),
            ))
        })
        .expect("workflow find_related page result read failed");

    let count_columns_are_null = Spi::get_one::<bool>(
        "SELECT bool_and(candidate_count IS NULL AND filtered_count IS NULL)
           FROM graph.find_related(
                'name',
                'Alice',
                source_table := 'graph_test_users_pgtest'::regclass,
                max_depth := 1,
                target_table := 'graph_test_users_pgtest'::regclass,
                max_rows := 10,
                include_counts := false
           )",
    )
    .expect("workflow find_related null counts failed")
    .unwrap_or(false);

    assert_eq!(node_id, "u3");
    assert_eq!(rank, 2);
    assert_eq!(name, "Carol");
    assert_eq!(candidate_count, 2);
    assert_eq!(filtered_count, 2);
    assert!(!truncated);
    assert!(count_columns_are_null);
}
