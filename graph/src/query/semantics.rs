//! Semantic binding for the read-only GQL subset.

use crate::gql::ast::{
    Direction, Expr, MatchClause, NodePat, Pattern, RelPat, ReturnExpr, ReturnItem,
};
use crate::gql::errors::{GqlError, Span};

use super::catalog_snapshot::CatalogSnapshot;
use super::logical_plan::{BoundNode, BoundRel, LogicalPlan, ReturnBinding};

/// Bind parsed GQL into a logical plan.
///
/// # Errors
///
/// Returns [`GqlError`] when the query uses valid syntax outside the current
/// Phase 1B execution slice or when labels/types cannot resolve in the catalog.
pub(crate) fn bind(
    query: &crate::gql::ast::Query,
    catalog: &impl CatalogSnapshot,
) -> Result<LogicalPlan, GqlError> {
    reject_later_clauses(query)?;
    let (source_pat, rel_pat, target_pat) = single_outbound_hop(&query.match_)?;
    let source = bind_node(source_pat, catalog)?;
    let target = bind_node(target_pat, catalog)?;
    let rel_type = rel_pat.rel_type.as_ref().ok_or_else(|| {
        GqlError::unsupported(
            rel_pat.span,
            "anonymous relationship types require a later phase",
        )
    })?;
    let rel = catalog.resolve_rel_type(
        &rel_type.text,
        source.table_oid,
        target.table_oid,
        rel_type.span,
    )?;
    let returns = bind_returns(&query.return_.items, &source.var, &target.var)?;
    Ok(LogicalPlan {
        source,
        relationship: BoundRel {
            rel_type: rel.rel_type,
        },
        target,
        returns,
    })
}

fn reject_later_clauses(query: &crate::gql::ast::Query) -> Result<(), GqlError> {
    if query.where_.is_some() {
        return Err(GqlError::unsupported(
            query.where_.as_ref().map_or(query.span, expr_span),
            "WHERE predicates are implemented in a later read phase",
        ));
    }
    if query.return_.distinct {
        return Err(GqlError::unsupported(
            query.return_.span,
            "RETURN DISTINCT is implemented in a later phase",
        ));
    }
    if !query.order_by.is_empty() {
        return Err(GqlError::unsupported(
            query.order_by[0].span,
            "ORDER BY is implemented in a later read phase",
        ));
    }
    if query.skip.is_some() {
        return Err(GqlError::unsupported(
            query.return_.span,
            "SKIP is implemented in a later read phase",
        ));
    }
    if query.limit.is_some() {
        return Err(GqlError::unsupported(
            query.return_.span,
            "LIMIT is implemented in a later read phase",
        ));
    }
    Ok(())
}

fn expr_span(expr: &Expr) -> Span {
    match expr {
        Expr::And { span, .. }
        | Expr::Or { span, .. }
        | Expr::Not { span, .. }
        | Expr::Compare { span, .. } => *span,
    }
}

fn single_outbound_hop(match_: &MatchClause) -> Result<(&NodePat, &RelPat, &NodePat), GqlError> {
    let Pattern { start, tail, .. } = &match_.pattern;
    let [(rel, target)] = tail.as_slice() else {
        return Err(GqlError::unsupported(
            match_.pattern.span,
            "Phase 1B supports exactly one relationship in MATCH",
        ));
    };
    if rel.direction != Direction::Out {
        return Err(GqlError::unsupported(
            rel.span,
            "Phase 1B supports only outbound directed relationships",
        ));
    }
    if rel.var_len.is_some() {
        return Err(GqlError::unsupported(
            rel.var_len.map_or(rel.span, |var_len| var_len.span),
            "variable-length relationships are implemented in a later read phase",
        ));
    }
    if !start.props.is_empty() || !target.props.is_empty() || !rel.props.is_empty() {
        return Err(GqlError::unsupported(
            match_.pattern.span,
            "inline property maps are implemented in a later read phase",
        ));
    }
    Ok((start, rel, target))
}

fn bind_node(node: &NodePat, catalog: &impl CatalogSnapshot) -> Result<BoundNode, GqlError> {
    let var = node.var.as_ref().ok_or_else(|| {
        GqlError::unsupported(node.span, "anonymous node patterns require a later phase")
    })?;
    let label = node.label.as_ref().ok_or_else(|| {
        GqlError::unsupported(node.span, "unlabeled node patterns require a later phase")
    })?;
    let info = catalog.resolve_node_label(&label.text, label.span)?;
    Ok(BoundNode {
        var: var.text.clone(),
        label: info.label,
        table_oid: info.table_oid,
    })
}

fn bind_returns(
    items: &[ReturnItem],
    source_var: &str,
    target_var: &str,
) -> Result<Vec<ReturnBinding>, GqlError> {
    items
        .iter()
        .map(|item| match &item.expr {
            ReturnExpr::Var { var, .. } if var.text == source_var => Ok(ReturnBinding::Source {
                name: item
                    .alias
                    .as_ref()
                    .map_or_else(|| var.text.clone(), |alias| alias.text.clone()),
            }),
            ReturnExpr::Var { var, .. } if var.text == target_var => Ok(ReturnBinding::Target {
                name: item
                    .alias
                    .as_ref()
                    .map_or_else(|| var.text.clone(), |alias| alias.text.clone()),
            }),
            ReturnExpr::Var { var, span } => Err(GqlError::bind(
                *span,
                format!("unknown return variable `{}`", var.text),
            )),
            ReturnExpr::Property { span, .. } => Err(GqlError::unsupported(
                *span,
                "RETURN property projections are implemented in a later read phase",
            )),
            ReturnExpr::Func { span, .. } => Err(GqlError::unsupported(
                *span,
                "RETURN functions are implemented in a later read phase",
            )),
        })
        .collect()
}
