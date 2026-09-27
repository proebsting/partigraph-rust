//! Minimum spanning tree over caller-supplied edge weights (Kruskal).

use rand::RngCore;

use crate::graph::Induced;
use crate::sampler::EdgeWeight;
use crate::tree::Tree;
use crate::Error;

fn find(parent: &mut [u32], mut x: u32) -> u32 {
    while parent[x as usize] != x {
        parent[x as usize] = parent[parent[x as usize] as usize];
        x = parent[x as usize];
    }
    x
}

/// Weight every edge of the induced subgraph, then take a minimum spanning tree.
///
/// Ties are broken by the sort, which is stable over the order edges are
/// enumerated in, so a weight function that returns a constant yields a
/// deterministic tree rather than a random one.
pub fn min_weight_spanning_tree(
    subgraph: &Induced,
    weights: &impl EdgeWeight,
    rng: &mut dyn RngCore,
) -> Result<Tree, Error> {
    let k = subgraph.len();
    if k == 0 {
        return Err(Error::EmptySubset);
    }

    let mut edges: Vec<(f64, u32, u32)> = Vec::new();
    for a in 0..k as u32 {
        for &b in subgraph.neighbors(a) {
            if a < b {
                let w = weights.weight(subgraph.global(a), subgraph.global(b), rng);
                edges.push((w, a, b));
            }
        }
    }
    edges.sort_by(|x, y| x.0.total_cmp(&y.0));

    let mut parent: Vec<u32> = (0..k as u32).collect();
    let mut adjacency: Vec<Vec<u32>> = vec![Vec::new(); k];
    let mut taken = 0;
    for (_, a, b) in edges {
        let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
        if ra != rb {
            parent[ra as usize] = rb;
            adjacency[a as usize].push(b);
            adjacency[b as usize].push(a);
            taken += 1;
            if taken == k - 1 {
                break;
            }
        }
    }
    // Fewer than k-1 edges taken means the subgraph was never connected.
    if taken != k - 1 {
        return Err(Error::Disconnected);
    }

    // Orient the edge set into a parent array. Any root gives the same tree;
    // aggregation over it does not depend on the choice.
    let root = 0u32;
    let mut parents = vec![root; k];
    let mut seen = vec![false; k];
    let mut stack = vec![root];
    seen[0] = true;
    while let Some(v) = stack.pop() {
        for &w in &adjacency[v as usize] {
            if !seen[w as usize] {
                seen[w as usize] = true;
                parents[w as usize] = v;
                stack.push(w);
            }
        }
    }

    let nodes = (0..k as u32).map(|l| subgraph.global(l)).collect();
    Ok(Tree::from_parents(nodes, parents, root))
}
