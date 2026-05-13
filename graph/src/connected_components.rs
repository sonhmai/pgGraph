//! # Connected Components — Label each node with its component ID
//!
//! Computes connected components using union-find (disjoint-set) over the CSR.
//! O(V + E × α(V)) where α is the inverse Ackermann function (effectively O(V+E)).
//!
//! This is a global graph algorithm — it touches every node and every edge.
//! Runtime scales linearly with graph size.
//!
//! ## Design
//!
//! We use union-find with path compression and union-by-rank for near-linear
//! performance. The component ID for each node is the union-find root after all
//! unions; callers should treat it as a stable label for the current
//! computation, not as a persistent node identifier.
//!
//! See: `docs/contributor_guide/traversal-search-paths.mdx`

use crate::edge_store::EdgeStore;
use crate::node_store::NodeStore;
use crate::types::TableOid;

/// Result of connected components computation.
#[derive(Debug)]
pub struct ComponentResult {
    /// Component ID for each node. `component[i]` is the component root for node `i`.
    pub component: Vec<u32>,
    /// Number of distinct components.
    pub num_components: u32,
    /// Size of the largest component.
    pub largest_component_size: u32,
}

/// Union-Find (Disjoint-Set) data structure.
struct UnionFind {
    parent: Vec<u32>,
    rank: Vec<u8>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        let parent: Vec<u32> = (0..n as u32).collect();
        let rank = vec![0u8; n];
        Self { parent, rank }
    }

    /// Find the root of node `x` with path compression.
    fn find(&mut self, x: u32) -> u32 {
        if self.parent[x as usize] != x {
            self.parent[x as usize] = self.find(self.parent[x as usize]);
        }
        self.parent[x as usize]
    }

    /// Union the sets containing `x` and `y`. Uses union-by-rank.
    fn union(&mut self, x: u32, y: u32) {
        let rx = self.find(x);
        let ry = self.find(y);
        if rx == ry {
            return;
        }
        // Union by rank keeps trees shallow; equal ranks choose `rx`, which is
        // deterministic for this edge iteration order.
        match self.rank[rx as usize].cmp(&self.rank[ry as usize]) {
            std::cmp::Ordering::Less => {
                self.parent[rx as usize] = ry;
            }
            std::cmp::Ordering::Greater => {
                self.parent[ry as usize] = rx;
            }
            std::cmp::Ordering::Equal => {
                self.parent[ry as usize] = rx;
                self.rank[rx as usize] += 1;
            }
        }
    }
}

/// Compute connected components for all active nodes in the graph.
///
/// # Arguments
/// * `node_store` — to check which nodes are active
/// * `edge_store` — CSR edge data
///
/// # Returns
/// ComponentResult with per-node component labels and summary stats.
pub fn compute_components(node_store: &NodeStore, edge_store: &EdgeStore) -> ComponentResult {
    let node_count = node_store.node_count() as usize;

    if node_count == 0 {
        return ComponentResult {
            component: vec![],
            num_components: 0,
            largest_component_size: 0,
        };
    }

    let mut uf = UnionFind::new(node_count);

    // Iterate through all edges in the CSR — sequential, cache-friendly
    for node in 0..node_count as u32 {
        if !node_store.is_active(node) {
            continue;
        }

        let (targets, _type_ids) = edge_store.neighbors(node);
        for &target in targets {
            if node_store.is_active(target) {
                uf.union(node, target);
            }
        }
    }

    // Finalize: compress all paths and compute stats
    let mut component = vec![0u32; node_count];
    let mut component_sizes = std::collections::HashMap::new();

    for node in 0..node_count as u32 {
        if !node_store.is_active(node) {
            component[node as usize] = u32::MAX; // inactive
            continue;
        }
        let root = uf.find(node);
        component[node as usize] = root;
        *component_sizes.entry(root).or_insert(0u32) += 1;
    }

    let num_components = component_sizes.len() as u32;
    let largest = component_sizes.values().copied().max().unwrap_or(0);

    ComponentResult {
        component,
        num_components,
        largest_component_size: largest,
    }
}

/// Output row for the SQL function.
#[derive(Debug)]
pub struct ComponentRow {
    pub node_table: TableOid,
    pub node_id: String,
    pub component_id: u32,
    pub component_size: u32,
}

/// Convert ComponentResult into SQL output rows.
pub fn to_component_rows(result: &ComponentResult, node_store: &NodeStore) -> Vec<ComponentRow> {
    let node_count = node_store.node_count() as usize;

    // Pre-compute component sizes
    let mut sizes = std::collections::HashMap::new();
    for &comp in &result.component {
        if comp != u32::MAX {
            *sizes.entry(comp).or_insert(0u32) += 1;
        }
    }

    let mut rows = Vec::with_capacity(node_count);
    for node_idx in 0..node_count as u32 {
        if result.component[node_idx as usize] == u32::MAX {
            continue; // inactive
        }
        let comp_id = result.component[node_idx as usize];
        rows.push(ComponentRow {
            node_table: TableOid(node_store.table_oid(node_idx)),
            node_id: node_store.primary_key(node_idx).to_string(),
            component_id: comp_id,
            component_size: *sizes.get(&comp_id).unwrap_or(&0),
        });
    }

    rows
}

#[cfg(test)]
mod tests {
    //! Covers weakly connected component computation and protects component
    //! cardinality, isolation, and representative-selection invariants.

    use super::*;
    use crate::edge_store::RawEdge;

    #[test]
    fn single_component() {
        // 0 — 1 — 2 (all connected)
        let mut ns = NodeStore::new();
        for i in 0..3u32 {
            ns.add_node(100, format!("N{}", i));
        }
        let edges = vec![
            RawEdge {
                source: 0,
                target: 1,
                type_id: 1,
                weight: None,
            },
            RawEdge {
                source: 1,
                target: 0,
                type_id: 1,
                weight: None,
            },
            RawEdge {
                source: 1,
                target: 2,
                type_id: 1,
                weight: None,
            },
            RawEdge {
                source: 2,
                target: 1,
                type_id: 1,
                weight: None,
            },
        ];
        let es = EdgeStore::from_edges(3, edges, false);

        let result = compute_components(&ns, &es);
        assert_eq!(result.num_components, 1);
        assert_eq!(result.largest_component_size, 3);
        // All nodes should have the same component
        let c0 = result.component[0];
        assert_eq!(result.component[1], c0);
        assert_eq!(result.component[2], c0);
    }

    #[test]
    fn two_components() {
        // 0 — 1   2 — 3 (two separate pairs)
        let mut ns = NodeStore::new();
        for i in 0..4u32 {
            ns.add_node(100, format!("N{}", i));
        }
        let edges = vec![
            RawEdge {
                source: 0,
                target: 1,
                type_id: 1,
                weight: None,
            },
            RawEdge {
                source: 1,
                target: 0,
                type_id: 1,
                weight: None,
            },
            RawEdge {
                source: 2,
                target: 3,
                type_id: 1,
                weight: None,
            },
            RawEdge {
                source: 3,
                target: 2,
                type_id: 1,
                weight: None,
            },
        ];
        let es = EdgeStore::from_edges(4, edges, false);

        let result = compute_components(&ns, &es);
        assert_eq!(result.num_components, 2);
        assert_eq!(result.largest_component_size, 2);
        // 0 and 1 share a component, 2 and 3 share a different one
        assert_eq!(result.component[0], result.component[1]);
        assert_eq!(result.component[2], result.component[3]);
        assert_ne!(result.component[0], result.component[2]);
    }

    #[test]
    fn isolated_nodes() {
        let mut ns = NodeStore::new();
        for i in 0..5u32 {
            ns.add_node(100, format!("N{}", i));
        }
        let es = EdgeStore::from_edges(5, vec![], false);

        let result = compute_components(&ns, &es);
        assert_eq!(result.num_components, 5);
        assert_eq!(result.largest_component_size, 1);
    }

    #[test]
    fn tombstoned_nodes_excluded() {
        let mut ns = NodeStore::new();
        for i in 0..3u32 {
            ns.add_node(100, format!("N{}", i));
        }
        ns.deactivate(1); // tombstone node 1

        let edges = vec![
            RawEdge {
                source: 0,
                target: 1,
                type_id: 1,
                weight: None,
            },
            RawEdge {
                source: 1,
                target: 0,
                type_id: 1,
                weight: None,
            },
            RawEdge {
                source: 1,
                target: 2,
                type_id: 1,
                weight: None,
            },
            RawEdge {
                source: 2,
                target: 1,
                type_id: 1,
                weight: None,
            },
        ];
        let es = EdgeStore::from_edges(3, edges, false);

        let result = compute_components(&ns, &es);
        assert_eq!(result.num_components, 2); // 0 and 2 are separate (1 is tombstoned)
        assert_eq!(result.component[1], u32::MAX); // tombstoned
    }

    #[test]
    fn empty_graph() {
        let ns = NodeStore::new();
        let es = EdgeStore::from_edges(0, vec![], false);
        let result = compute_components(&ns, &es);
        assert_eq!(result.num_components, 0);
        assert_eq!(result.largest_component_size, 0);
        assert!(result.component.is_empty());
    }

    #[test]
    fn star_topology_single_component() {
        // Hub node 0 connected to 1,2,3,4
        let mut ns = NodeStore::new();
        for i in 0..5u32 {
            ns.add_node(100, format!("N{}", i));
        }
        let mut edges = Vec::new();
        for i in 1..5u32 {
            edges.push(RawEdge {
                source: 0,
                target: i,
                type_id: 1,
                weight: None,
            });
            edges.push(RawEdge {
                source: i,
                target: 0,
                type_id: 1,
                weight: None,
            });
        }
        let es = EdgeStore::from_edges(5, edges, false);
        let result = compute_components(&ns, &es);
        assert_eq!(result.num_components, 1);
        assert_eq!(result.largest_component_size, 5);
    }

    #[test]
    fn self_loop_single_component() {
        let mut ns = NodeStore::new();
        ns.add_node(100, "self".to_string());
        let edges = vec![RawEdge {
            source: 0,
            target: 0,
            type_id: 1,
            weight: None,
        }];
        let es = EdgeStore::from_edges(1, edges, false);
        let result = compute_components(&ns, &es);
        assert_eq!(result.num_components, 1);
        assert_eq!(result.largest_component_size, 1);
    }

    #[test]
    fn to_component_rows_output_matches_components() {
        let mut ns = NodeStore::new();
        ns.add_node(100, "A".to_string());
        ns.add_node(100, "B".to_string());
        ns.add_node(200, "C".to_string());
        let edges = vec![
            RawEdge {
                source: 0,
                target: 1,
                type_id: 1,
                weight: None,
            },
            RawEdge {
                source: 1,
                target: 0,
                type_id: 1,
                weight: None,
            },
        ];
        let es = EdgeStore::from_edges(3, edges, false);
        let result = compute_components(&ns, &es);
        let rows = to_component_rows(&result, &ns);

        assert_eq!(rows.len(), 3);
        // A and B share a component
        let a_comp = rows.iter().find(|r| r.node_id == "A").unwrap().component_id;
        let b_comp = rows.iter().find(|r| r.node_id == "B").unwrap().component_id;
        let c_comp = rows.iter().find(|r| r.node_id == "C").unwrap().component_id;
        assert_eq!(a_comp, b_comp);
        assert_ne!(a_comp, c_comp);
        // Component sizes
        assert_eq!(
            rows.iter()
                .find(|r| r.node_id == "A")
                .unwrap()
                .component_size,
            2
        );
        assert_eq!(
            rows.iter()
                .find(|r| r.node_id == "C")
                .unwrap()
                .component_size,
            1
        );
    }
}
