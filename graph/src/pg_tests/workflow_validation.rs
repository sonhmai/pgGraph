// Workflow validation tests cover bad paths introduced by the wrapper layer.
// Delegated primitive validation remains covered by the primitive API tests.

#[pg_test]
fn workflow_rejects_conflicting_target_filters() {
    reset_and_create_fixtures();
    build_friendship_fixture_graph();

    assert!(sql_raises(
        "SELECT *
           FROM graph.expand(
                'graph_test_users_pgtest'::regclass,
                'u1',
                target_table := 'graph_test_users_pgtest'::regclass,
                target_tables := ARRAY['graph_test_users_pgtest'::regclass]
           )"
    ));
    assert!(sql_raises(
        "SELECT *
           FROM graph.find_related(
                'name',
                'Alice',
                target_table := 'graph_test_users_pgtest'::regclass,
                target_tables := ARRAY['graph_test_users_pgtest'::regclass]
           )"
    ));
}

#[pg_test]
fn workflow_rejects_negative_limits_and_offsets() {
    reset_and_create_fixtures();
    build_friendship_fixture_graph();

    let invalid_calls = [
        "SELECT * FROM graph.find('name', 'Alice', max_rows := -1)",
        "SELECT * FROM graph.find('name', 'Alice', row_offset := -1)",
        "SELECT * FROM graph.expand('graph_test_users_pgtest'::regclass, 'u1', max_rows := -1)",
        "SELECT * FROM graph.expand('graph_test_users_pgtest'::regclass, 'u1', row_offset := -1)",
        "SELECT * FROM graph.find_related('name', 'Alice', search_max_rows := -1)",
        "SELECT * FROM graph.find_related('name', 'Alice', max_rows := -1)",
        "SELECT * FROM graph.find_related('name', 'Alice', row_offset := -1)",
        "SELECT * FROM graph.find_related('name', 'Alice', candidate_limit := -1)",
        "SELECT * FROM graph.connection('name', 'Alice', 'name', 'Bob', source_k := -1)",
        "SELECT * FROM graph.connection('name', 'Alice', 'name', 'Bob', target_k := -1)",
        "SELECT * FROM graph.neighborhood('name', 'Alice', search_max_rows := -1)",
        "SELECT * FROM graph.neighborhood('name', 'Alice', sample_k := -1)",
        "SELECT * FROM graph.neighborhood('name', 'Alice', node_limit := -1)",
    ];

    for call in invalid_calls {
        assert!(sql_raises(call), "expected workflow call to fail: {call}");
    }
}

#[pg_test]
fn workflow_rejects_unsupported_search_and_traversal_options() {
    reset_and_create_fixtures();
    build_friendship_fixture_graph();

    let invalid_calls = [
        "SELECT * FROM graph.find('name', 'Alice', mode := 'unsupported')",
        "SELECT * FROM graph.find_related('name', 'Alice', search_mode := 'unsupported')",
        "SELECT * FROM graph.connection('name', 'Alice', 'name', 'Bob', search_mode := 'unsupported')",
        "SELECT * FROM graph.expand('graph_test_users_pgtest'::regclass, 'u1', direction := 'sideways')",
        "SELECT * FROM graph.find_related('name', 'Alice', direction := 'sideways')",
        "SELECT * FROM graph.neighborhood('name', 'Alice', direction := 'sideways')",
    ];

    for call in invalid_calls {
        assert!(sql_raises(call), "expected workflow call to fail: {call}");
    }
}
