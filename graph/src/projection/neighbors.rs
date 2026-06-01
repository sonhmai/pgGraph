//! Overlay-aware neighbor iteration for graph algorithms.
//!
//! The committed CSR remains the fast path. Pending sync and future mutable
//! projection deltas are layered as insert/delete maps without materializing a
//! per-node neighbor vector for clean reads.

use std::collections::{HashMap, HashSet};

use crate::edge_store::EdgeStore;

/// Pending edge inserts keyed by source node.
pub(crate) type OverlayInserts = HashMap<u32, Vec<(u32, u8)>>;
/// Pending edge deletes keyed by source node.
pub(crate) type OverlayDeletes = HashMap<u32, HashSet<(u32, u8)>>;
/// Insert and delete overlay maps for one edge orientation.
pub(crate) type EdgeOverlay = (OverlayInserts, OverlayDeletes);

/// Source of graph neighbors for algorithms that must work over clean CSR and
/// overlay-augmented projections.
pub(crate) trait NeighborSource {
    /// Iterate neighbors in base CSR order followed by non-duplicate inserts.
    fn neighbors(&self, node_idx: u32) -> NeighborIter<'_>;

    /// Iterate neighbors in reverse expansion order for DFS stack pushes.
    fn neighbors_reversed(&self, node_idx: u32) -> NeighborIter<'_>;
}

/// Clean CSR neighbor source.
pub(crate) struct CsrNeighbors<'a> {
    edge_store: &'a EdgeStore,
}

impl<'a> CsrNeighbors<'a> {
    /// Borrow an [`EdgeStore`] as a clean neighbor source.
    pub(crate) fn new(edge_store: &'a EdgeStore) -> Self {
        Self { edge_store }
    }
}

impl NeighborSource for CsrNeighbors<'_> {
    fn neighbors(&self, node_idx: u32) -> NeighborIter<'_> {
        let (targets, type_ids) = self.edge_store.neighbors(node_idx);
        NeighborIter::Csr(CsrNeighborIter::forward(targets, type_ids))
    }

    fn neighbors_reversed(&self, node_idx: u32) -> NeighborIter<'_> {
        let (targets, type_ids) = self.edge_store.neighbors(node_idx);
        NeighborIter::Csr(CsrNeighborIter::reversed(targets, type_ids))
    }
}

/// CSR plus pending edge overlay source.
pub(crate) struct OverlayNeighbors<'a> {
    edge_store: &'a EdgeStore,
    inserts: &'a OverlayInserts,
    deletes: &'a OverlayDeletes,
}

impl<'a> OverlayNeighbors<'a> {
    /// Borrow a base CSR and orientation-specific overlay maps.
    pub(crate) fn new(
        edge_store: &'a EdgeStore,
        inserts: &'a OverlayInserts,
        deletes: &'a OverlayDeletes,
    ) -> Self {
        Self {
            edge_store,
            inserts,
            deletes,
        }
    }
}

impl NeighborSource for OverlayNeighbors<'_> {
    fn neighbors(&self, node_idx: u32) -> NeighborIter<'_> {
        let (targets, type_ids) = self.edge_store.neighbors(node_idx);
        NeighborIter::Overlay(OverlayNeighborIter::forward(
            targets,
            type_ids,
            self.inserts.get(&node_idx).map(Vec::as_slice),
            self.deletes.get(&node_idx),
        ))
    }

    fn neighbors_reversed(&self, node_idx: u32) -> NeighborIter<'_> {
        let (targets, type_ids) = self.edge_store.neighbors(node_idx);
        NeighborIter::Overlay(OverlayNeighborIter::reversed(
            targets,
            type_ids,
            self.inserts.get(&node_idx).map(Vec::as_slice),
            self.deletes.get(&node_idx),
        ))
    }
}

/// Neighbor stream item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Neighbor {
    /// Target node index.
    pub(crate) target: u32,
    /// Edge type identifier.
    pub(crate) type_id: u8,
}

/// Neighbor iterator for clean and overlay-backed sources.
pub(crate) enum NeighborIter<'a> {
    Csr(CsrNeighborIter<'a>),
    Overlay(OverlayNeighborIter<'a>),
}

impl Iterator for NeighborIter<'_> {
    type Item = Neighbor;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Csr(iter) => iter.next(),
            Self::Overlay(iter) => iter.next(),
        }
    }
}

pub(crate) struct CsrNeighborIter<'a> {
    targets: &'a [u32],
    type_ids: &'a [u8],
    pos: usize,
    reversed: bool,
}

impl<'a> CsrNeighborIter<'a> {
    fn forward(targets: &'a [u32], type_ids: &'a [u8]) -> Self {
        Self {
            targets,
            type_ids,
            pos: 0,
            reversed: false,
        }
    }

    fn reversed(targets: &'a [u32], type_ids: &'a [u8]) -> Self {
        Self {
            targets,
            type_ids,
            pos: targets.len(),
            reversed: true,
        }
    }
}

impl Iterator for CsrNeighborIter<'_> {
    type Item = Neighbor;

    fn next(&mut self) -> Option<Self::Item> {
        let pos = if self.reversed {
            self.pos = self.pos.checked_sub(1)?;
            self.pos
        } else {
            if self.pos >= self.targets.len() {
                return None;
            }
            let pos = self.pos;
            self.pos += 1;
            pos
        };
        Some(Neighbor {
            target: self.targets[pos],
            type_id: self.type_ids[pos],
        })
    }
}

enum OverlayPhase {
    Base,
    Inserts,
}

pub(crate) struct OverlayNeighborIter<'a> {
    targets: &'a [u32],
    type_ids: &'a [u8],
    deleted: Option<&'a HashSet<(u32, u8)>>,
    inserted: Option<&'a [(u32, u8)]>,
    base: CsrNeighborIter<'a>,
    insert_pos: usize,
    phase: OverlayPhase,
    reversed: bool,
}

impl<'a> OverlayNeighborIter<'a> {
    fn forward(
        targets: &'a [u32],
        type_ids: &'a [u8],
        inserted: Option<&'a [(u32, u8)]>,
        deleted: Option<&'a HashSet<(u32, u8)>>,
    ) -> Self {
        Self {
            targets,
            type_ids,
            deleted,
            inserted,
            base: CsrNeighborIter::forward(targets, type_ids),
            insert_pos: 0,
            phase: OverlayPhase::Base,
            reversed: false,
        }
    }

    fn reversed(
        targets: &'a [u32],
        type_ids: &'a [u8],
        inserted: Option<&'a [(u32, u8)]>,
        deleted: Option<&'a HashSet<(u32, u8)>>,
    ) -> Self {
        Self {
            targets,
            type_ids,
            deleted,
            inserted,
            base: CsrNeighborIter::reversed(targets, type_ids),
            insert_pos: inserted.map_or(0, <[_]>::len),
            phase: OverlayPhase::Inserts,
            reversed: true,
        }
    }

    fn base_contains(&self, target: u32, type_id: u8) -> bool {
        self.targets
            .iter()
            .zip(self.type_ids.iter())
            .any(|(&base_target, &base_type)| base_target == target && base_type == type_id)
    }

    fn inserted_duplicate(&self, pos: usize, target: u32, type_id: u8) -> bool {
        self.inserted
            .is_some_and(|inserted| inserted[..pos].contains(&(target, type_id)))
    }

    fn next_base(&mut self) -> Option<Neighbor> {
        for neighbor in self.base.by_ref() {
            if self
                .deleted
                .is_some_and(|deleted| deleted.contains(&(neighbor.target, neighbor.type_id)))
            {
                continue;
            }
            return Some(neighbor);
        }
        None
    }

    fn next_insert(&mut self) -> Option<Neighbor> {
        let inserted = self.inserted?;
        loop {
            let pos = if self.reversed {
                self.insert_pos = self.insert_pos.checked_sub(1)?;
                self.insert_pos
            } else {
                if self.insert_pos >= inserted.len() {
                    return None;
                }
                let pos = self.insert_pos;
                self.insert_pos += 1;
                pos
            };
            let (target, type_id) = inserted[pos];
            if self.base_contains(target, type_id) || self.inserted_duplicate(pos, target, type_id)
            {
                continue;
            }
            return Some(Neighbor { target, type_id });
        }
    }
}

impl Iterator for OverlayNeighborIter<'_> {
    type Item = Neighbor;

    fn next(&mut self) -> Option<Self::Item> {
        match self.phase {
            OverlayPhase::Base => self.next_base().or_else(|| {
                self.phase = OverlayPhase::Inserts;
                self.next_insert()
            }),
            OverlayPhase::Inserts => self.next_insert().or_else(|| {
                self.phase = OverlayPhase::Base;
                self.next_base()
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edge_store::RawEdge;
    use proptest::prelude::*;

    #[test]
    fn clean_neighbors_match_csr_order() {
        let edges = vec![
            RawEdge {
                source: 0,
                target: 1,
                type_id: 1,
                weight: None,
            },
            RawEdge {
                source: 0,
                target: 2,
                type_id: 2,
                weight: None,
            },
        ];
        let store = EdgeStore::from_edges(3, edges, false);
        let neighbors = CsrNeighbors::new(&store);

        let actual = neighbors.neighbors(0).collect::<Vec<_>>();

        assert_eq!(
            actual,
            vec![
                Neighbor {
                    target: 1,
                    type_id: 1
                },
                Neighbor {
                    target: 2,
                    type_id: 2
                }
            ]
        );
    }

    #[test]
    fn overlay_neighbors_hide_deletes_and_append_inserts() {
        let store = EdgeStore::from_edges(
            4,
            vec![
                RawEdge {
                    source: 0,
                    target: 1,
                    type_id: 1,
                    weight: None,
                },
                RawEdge {
                    source: 0,
                    target: 2,
                    type_id: 1,
                    weight: None,
                },
            ],
            false,
        );
        let mut inserts = OverlayInserts::new();
        inserts.insert(0, vec![(3, 1), (2, 1), (3, 1)]);
        let mut deletes = OverlayDeletes::new();
        deletes.insert(0, HashSet::from([(1, 1)]));
        let neighbors = OverlayNeighbors::new(&store, &inserts, &deletes);

        let actual = neighbors.neighbors(0).collect::<Vec<_>>();

        assert_eq!(
            actual,
            vec![
                Neighbor {
                    target: 2,
                    type_id: 1
                },
                Neighbor {
                    target: 3,
                    type_id: 1
                }
            ]
        );
    }

    proptest! {
        #[test]
        fn clean_overlay_matches_csr_neighbors(
            node_count in 1u32..16,
            raw_edges in prop::collection::vec((0u32..16, 0u32..16, 0u8..4), 0..96),
            query_node in 0u32..16,
        ) {
            let edges = raw_edges
                .into_iter()
                .filter(|(source, target, _)| *source < node_count && *target < node_count)
                .map(|(source, target, type_id)| RawEdge {
                    source,
                    target,
                    type_id,
                    weight: None,
                })
                .collect::<Vec<_>>();
            let store = EdgeStore::from_edges(node_count, edges, false);
            let inserts = OverlayInserts::new();
            let deletes = OverlayDeletes::new();
            let csr = CsrNeighbors::new(&store);
            let overlay = OverlayNeighbors::new(&store, &inserts, &deletes);

            prop_assert_eq!(
                csr.neighbors(query_node).collect::<Vec<_>>(),
                overlay.neighbors(query_node).collect::<Vec<_>>()
            );
            prop_assert_eq!(
                csr.neighbors_reversed(query_node).collect::<Vec<_>>(),
                overlay.neighbors_reversed(query_node).collect::<Vec<_>>()
            );
        }
    }
}
