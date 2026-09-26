//! One disjoint-set (union-find) structure for the whole sketcher.
//!
//! Four near-identical copies used to live here: `analysis.rs` and
//! `validation.rs` each carried a byte-identical `Dsu<T>`, `region.rs` an
//! inline `fn find` over a loose `BTreeMap`, and `solver/mod.rs` a
//! `Vec`-backed `DisjointSet` over dense column indices. They all cluster
//! the same way — link the roots, compress the path — so they are one type.

use std::collections::BTreeMap;

/// Disjoint Set Union (Union-Find) over any copyable, ordered key.
///
/// Keys are implicit: [`Dsu::find`] inserts an unseen key as its own root,
/// so there is no capacity to size up front and a sparse `PointId` space
/// costs no more than a dense column index. `union` links the first root
/// under the second (no union-by-rank) — path compression alone keeps
/// lookups near-constant at sketch sizes, and the rule is stable, so the
/// representative a cluster ends up with does not depend on tree shape.
#[derive(Debug, Clone, Default)]
pub(crate) struct Dsu<T: Ord + Copy> {
    parent: BTreeMap<T, T>,
}

impl<T: Ord + Copy> Dsu<T> {
    pub(crate) fn new() -> Self {
        Self {
            parent: BTreeMap::new(),
        }
    }

    /// The cluster representative of `i`, registering `i` if it is new.
    pub(crate) fn find(&mut self, i: T) -> T {
        let p = *self.parent.entry(i).or_insert(i);
        if p == i {
            i
        } else {
            let root = self.find(p);
            self.parent.insert(i, root);
            root
        }
    }

    /// Merges the clusters of `i` and `j`. Returns `false` when they were
    /// already one cluster — callers use that to tell a *new* connection
    /// from a redundant one.
    pub(crate) fn union(&mut self, i: T, j: T) -> bool {
        let root_i = self.find(i);
        let root_j = self.find(j);
        if root_i != root_j {
            self.parent.insert(root_i, root_j);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Dsu;

    #[test]
    fn clusters_are_transitive_and_report_new_links() {
        let mut dsu = Dsu::new();
        assert!(dsu.union(1u32, 2));
        assert!(dsu.union(2, 3));
        assert!(!dsu.union(1, 3), "already one cluster");
        assert_eq!(dsu.find(1), dsu.find(3));
        assert_ne!(dsu.find(1), dsu.find(4), "untouched key is its own root");
    }
}
