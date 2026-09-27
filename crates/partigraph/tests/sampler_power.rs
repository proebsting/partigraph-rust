//! The uniformity tests must be able to fail.
//!
//! A chi-square test with too few draws passes for any sampler, which would make
//! `uniformity.rs` vacuous. This checks the bar has teeth by running a sampler
//! that is known *not* to be uniform through the same machinery and requiring it
//! to be rejected.
//!
//! The stand-in is a random-weight minimum spanning tree — which is what
//! GerryChain's `random_spanning_tree` does. It is a reasonable way to draw a
//! varied tree, but it is not uniform over spanning trees: on K4 it
//! over-produces stars and under-produces paths.

mod common;

use common::{all_spanning_trees, canonical, complete};
use partigraph::{random_spanning_tree, Graph, NodeId, NodeSet};
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::collections::HashMap;

/// Kruskal over uniformly random edge weights.
fn random_weight_mst(g: &Graph, rng: &mut impl Rng) -> Vec<(u32, u32)> {
    let mut edges: Vec<(f64, u32, u32)> = Vec::new();
    for v in 0..g.node_count() as u32 {
        for &w in g.neighbors(NodeId(v)) {
            if v < w {
                edges.push((rng.gen::<f64>(), v, w));
            }
        }
    }
    edges.sort_by(|a, b| a.0.total_cmp(&b.0));

    fn find(p: &mut [u32], mut x: u32) -> u32 {
        while p[x as usize] != x {
            p[x as usize] = p[p[x as usize] as usize];
            x = p[x as usize];
        }
        x
    }
    let mut parent: Vec<u32> = (0..g.node_count() as u32).collect();
    let mut out = Vec::new();
    for (_, a, b) in edges {
        let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
        if ra != rb {
            parent[ra as usize] = rb;
            out.push((a.min(b), a.max(b)));
        }
    }
    out.sort_unstable();
    out
}

fn chi_square(
    g: &Graph,
    draws: usize,
    seed: u64,
    mut draw: impl FnMut(&Graph, &mut StdRng) -> Vec<(u32, u32)>,
) -> f64 {
    let trees = all_spanning_trees(g);
    let index: HashMap<Vec<(u32, u32)>, usize> = trees
        .iter()
        .cloned()
        .enumerate()
        .map(|(i, t)| (t, i))
        .collect();
    let mut rng = StdRng::seed_from_u64(seed);
    let mut counts = vec![0usize; trees.len()];
    for _ in 0..draws {
        counts[index[&draw(g, &mut rng)]] += 1;
    }
    let expected = draws as f64 / trees.len() as f64;
    counts
        .iter()
        .map(|&o| (o as f64 - expected).powi(2) / expected)
        .sum()
}

#[test]
fn a_non_uniform_sampler_is_rejected_where_wilsons_passes() {
    // Same graph, same draw count, same seed, same threshold as `k4_is_uniform`.
    let g = complete(4);
    let bar = 37.70; // df = 15, upper 99.9% point

    let subset = NodeSet::all(&g);
    let wilson = chi_square(&g, 64_000, 0xC0FFEE, |g, rng| {
        canonical(&random_spanning_tree(g, &subset, rng).unwrap())
    });
    let mst = chi_square(&g, 64_000, 0xC0FFEE, random_weight_mst);

    assert!(
        wilson < bar,
        "Wilson's should pass the bar, got chi-square {wilson}"
    );
    assert!(
        mst > bar * 2.0,
        "the bar has no teeth: a random-weight MST scored only {mst} against a bar of {bar}"
    );
}
