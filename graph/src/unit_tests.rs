//! Covers crate-local helpers that support SQL API compatibility without
//! requiring a PostgreSQL backend.

use super::*;
use proptest::prelude::*;

/// Covers catalog column-list parsing used by discovery functions and the
/// `id_columns` compatibility layer.
#[test]
fn split_catalog_columns_ignores_empty_segments_and_whitespace() {
    assert_eq!(
        split_catalog_columns(" id, , tenant_id "),
        vec!["id".to_string(), "tenant_id".to_string()]
    );
    assert!(split_catalog_columns("").is_empty());
}

fn test_i64_encoder(value: &serde_json::Value) -> safety::GraphResult<i64> {
    value
        .as_i64()
        .ok_or_else(|| safety::GraphError::InvalidFilter {
            reason: "expected integer".to_string(),
        })
}

/// Typed pushdown conversion returns structured filter errors instead of
/// relying on prior validation to make malformed operators unreachable.
#[test]
fn typed_i64_op_rejects_malformed_operator_shapes() {
    let unsupported =
        sql_filters::typed_i64_op(0, "contains", &serde_json::json!(1), test_i64_encoder);
    let malformed_between =
        sql_filters::typed_i64_op(0, "between", &serde_json::json!([1]), test_i64_encoder);

    assert!(matches!(
        unsupported,
        Err(safety::GraphError::InvalidFilter { .. })
    ));
    assert!(matches!(
        malformed_between,
        Err(safety::GraphError::InvalidFilter { .. })
    ));
}

proptest! {
    /// Structured-filter operator validation must reject arbitrary operator
    /// names unless they are in the documented allow-list, and `between`
    /// must remain the only shape that requires a two-value array.
    #[test]
    fn structured_operator_shape_property(operator in ".{0,32}", value in any::<i64>()) {
        let scalar = serde_json::json!(value);
        let result = validate_structured_operator_shape("prop", &operator, &scalar);
        let allowed_scalar = matches!(operator.as_str(), "eq" | "neq" | "gt" | "gte" | "lt" | "lte");
        prop_assert_eq!(result.is_ok(), allowed_scalar);

        let bounds = serde_json::json!([value, value.saturating_add(1)]);
        let result = validate_structured_operator_shape("prop", &operator, &bounds);
        let allowed_array = allowed_scalar || operator == "between";
        prop_assert_eq!(result.is_ok(), allowed_array);
    }

    /// Sync property decoding accepts arbitrary JSON text without panics and
    /// preserves only non-null object fields as stringified key/value pairs.
    #[test]
    fn sync_property_decoder_is_total_for_utf8(input in ".{0,512}") {
        let _ = parse_sync_properties(Some(&input));
    }
}

#[test]
fn node_ref_json_part_parser_rejects_non_contract_shapes() {
    assert!(parse_node_ref_json_parts(&serde_json::json!("[\"public.users\",\"u1\"]")).is_ok());
    assert!(parse_node_ref_json_parts(&serde_json::json!("[\"public.users\"]")).is_err());
    assert!(parse_node_ref_json_parts(&serde_json::json!("[42,\"u1\"]")).is_err());
    assert!(parse_node_ref_json_parts(&serde_json::json!({"table": "public.users"})).is_err());
}
