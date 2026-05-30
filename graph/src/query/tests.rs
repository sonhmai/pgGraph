use super::catalog_snapshot::FakeCatalog;
use super::execute::execute;
use super::explain::explain;
use super::lower::lower;
use super::physical_plan::ReturnSlot;
use super::semantics::bind;
use crate::edge_store::{EdgeStore, RawEdge};
use crate::engine::Engine;
use crate::gql::errors::GqlErrorKind;
use crate::gql::parse;

fn fake_catalog() -> FakeCatalog {
    FakeCatalog::new()
        .with_label("users", 10, ["id", "name"])
        .with_label("companies", 20, ["id", "name"])
        .with_edge("works_at", 10, 20)
}

fn bind_query(query: &str) -> super::logical_plan::LogicalPlan {
    let ast = parse(query).unwrap();
    bind(&ast, &fake_catalog()).unwrap()
}

#[test]
fn binder_accepts_single_directed_match_returning_coordinates() {
    let plan = bind_query("MATCH (u:users)-[:works_at]->(c:companies) RETURN u, c");

    assert_eq!(plan.source.var, "u");
    assert_eq!(plan.source.table_oid, 10);
    assert_eq!(plan.relationship.rel_type, "works_at");
    assert_eq!(plan.target.var, "c");
    assert_eq!(plan.target.table_oid, 20);
    assert_eq!(plan.returns.len(), 2);
}

#[test]
fn binder_rejects_unknown_label_and_relationship_type() {
    let unknown_label = parse("MATCH (u:missing)-[:works_at]->(c:companies) RETURN u").unwrap();
    let label_err = bind(&unknown_label, &fake_catalog()).unwrap_err();
    assert!(matches!(label_err.kind, GqlErrorKind::Bind { .. }));

    let unknown_type = parse("MATCH (u:users)-[:owns]->(c:companies) RETURN u").unwrap();
    let type_err = bind(&unknown_type, &fake_catalog()).unwrap_err();
    assert!(matches!(type_err.kind, GqlErrorKind::Bind { .. }));
}

#[test]
fn binder_rejects_out_of_slice_1b_shapes() {
    for query in [
        "MATCH (u:users)<-[:works_at]-(c:companies) RETURN u",
        "MATCH (u:users)-[:works_at*1..2]->(c:companies) RETURN u",
        "MATCH (u:users {id: 'u1'})-[:works_at]->(c:companies) RETURN u",
        "MATCH (u:users)-[:works_at]->(c:companies) WHERE u.id = 'u1' RETURN u",
        "MATCH (u:users)-[:works_at]->(c:companies) RETURN u.name",
        "MATCH (u:users)-[:works_at]->(c:companies) RETURN count(u)",
        "MATCH (u:users)-[:works_at]->(c:companies) RETURN DISTINCT u",
    ] {
        let ast = parse(query).unwrap();
        let err = bind(&ast, &fake_catalog()).unwrap_err();
        assert!(
            matches!(err.kind, GqlErrorKind::Unsupported { .. }),
            "{query}"
        );
    }
}

#[test]
fn lowering_preserves_bound_tables_and_return_slots() {
    let logical = bind_query("MATCH (u:users)-[:works_at]->(c:companies) RETURN c, u");
    let physical = lower(logical);

    assert_eq!(physical.source_table_oid, 10);
    assert_eq!(physical.target_table_oid, 20);
    assert_eq!(physical.rel_type, "works_at");
    assert_eq!(
        physical.returns,
        vec![
            ReturnSlot::Target { name: "c".into() },
            ReturnSlot::Source { name: "u".into() }
        ]
    );
}

#[test]
fn executor_returns_one_hop_coordinate_rows() {
    let logical = bind_query("MATCH (u:users)-[:works_at]->(c:companies) RETURN u, c");
    let physical = lower(logical);
    let engine = engine_fixture();

    let rows = execute(&engine, &physical).unwrap();

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].values[0].name, "u");
    assert_eq!(rows[0].values[0].coordinate.table_oid, 10);
    assert_eq!(rows[0].values[0].coordinate.node_id, "u1");
    assert_eq!(rows[0].values[1].name, "c");
    assert_eq!(rows[0].values[1].coordinate.table_oid, 20);
    assert_eq!(rows[0].values[1].coordinate.node_id, "c1");
    assert_eq!(rows[1].values[0].coordinate.node_id, "u2");
}

#[test]
fn executor_filters_wrong_target_table_and_edge_type() {
    let logical = bind_query("MATCH (u:users)-[:works_at]->(c:companies) RETURN u, c");
    let physical = lower(logical);
    let mut engine = engine_fixture();
    let owns = engine.register_edge_type("owns").unwrap();
    engine.edge_store = EdgeStore::from_edges(
        engine.node_store.node_count(),
        vec![
            RawEdge {
                source: 0,
                target: 2,
                type_id: 1,
                weight: None,
            },
            RawEdge {
                source: 0,
                target: 1,
                type_id: owns,
                weight: None,
            },
            RawEdge {
                source: 1,
                target: 3,
                type_id: 1,
                weight: None,
            },
        ],
        false,
    );

    let rows = execute(&engine, &physical).unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].values[0].coordinate.node_id, "u1");
    assert_eq!(rows[0].values[1].coordinate.node_id, "c1");
}

#[test]
fn explain_contains_stable_1b_plan_shape() {
    let logical = bind_query("MATCH (u:users)-[:works_at]->(c:companies) RETURN u, c");
    let physical = lower(logical);

    assert_eq!(
        explain(&physical),
        "OneHopExpand(source=u:10, rel=works_at, target=c:20, return=[u, c])"
    );
}

fn engine_fixture() -> Engine {
    let mut engine = Engine::new();
    for (oid, pk) in [(10, "u1"), (10, "u2"), (20, "c1"), (20, "c2")] {
        let node_idx = engine.node_store.add_node(oid, pk.to_string());
        engine.resolution_insert(oid, pk, node_idx);
        engine.insert_table_membership(oid, node_idx);
    }
    let works_at = engine.register_edge_type("works_at").unwrap();
    engine.edge_store = EdgeStore::from_edges(
        engine.node_store.node_count(),
        vec![
            RawEdge {
                source: 0,
                target: 2,
                type_id: works_at,
                weight: None,
            },
            RawEdge {
                source: 1,
                target: 3,
                type_id: works_at,
                weight: None,
            },
        ],
        false,
    );
    engine.reverse_edge_store = engine.edge_store.reversed();
    engine.built = true;
    engine
}
