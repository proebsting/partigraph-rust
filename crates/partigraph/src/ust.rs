//! Wilson's algorithm for uniform random spanning trees.
//!
//! Reference: David Bruce Wilson, "Generating Random Spanning Trees More Quickly
//! than the Cover Time" (1996).
//! <https://www.cs.cmu.edu/~15859n/RelatedWork/RandomTrees-Wilson.pdf>

use rand::{Rng, RngCore};

use crate::graph::{Graph, Induced, NodeSet};
use crate::tree::Tree;
use crate::Error;

/// Draw a spanning tree of the subgraph induced by `subset`, uniformly at random
/// from all of that subgraph's spanning trees.
///
/// Errors if the subset is empty or induces a disconnected subgraph; the walk
/// would not terminate on the latter.
pub fn random_spanning_tree(
    graph: &Graph,
    subset: &NodeSet,
    rng: &mut impl Rng,
) -> Result<Tree, Error> {
    wilson(&Induced::new(graph, subset), rng)
}

/// Wilson's algorithm over a prebuilt induced subgraph.
pub(crate) fn wilson(ind: &Induced, rng: &mut dyn RngCore) -> Result<Tree, Error> {
    let k = ind.len();
    if k == 0 {
        return Err(Error::EmptySubset);
    }
    // The Python's docstring claims a disconnected input yields a tree of one
    // component. In fact its `while u not in tree` walk from an unreachable node
    // never terminates, so check first.
    if !ind.is_connected() {
        return Err(Error::Disconnected);
    }

    let root = rng.gen_range(0..k as u32);
    let mut next = vec![u32::MAX; k];
    let mut in_tree = vec![false; k];
    in_tree[root as usize] = true;

    for i in 0..k as u32 {
        // Random walk until it hits the tree, overwriting `next` as it goes.
        // Revisiting a node rewrites its pointer, which is what erases the loop.
        let mut u = i;
        while !in_tree[u as usize] {
            let nbrs = ind.neighbors(u);
            let pick = nbrs[rng.gen_range(0..nbrs.len())];
            next[u as usize] = pick;
            u = pick;
        }
        // Adopt the loop-erased path.
        let mut u = i;
        while !in_tree[u as usize] {
            in_tree[u as usize] = true;
            u = next[u as usize];
        }
    }

    // Every node is now in the tree, and every node but the root was adopted
    // along some path, so has a pointer.
    let mut parent = next;
    debug_assert!(parent
        .iter()
        .enumerate()
        .all(|(v, &p)| v as u32 == root || p != u32::MAX));
    parent[root as usize] = root;

    let nodes = (0..k as u32).map(|l| ind.global(l)).collect();
    Ok(Tree::from_parents(nodes, parent, root))
}
