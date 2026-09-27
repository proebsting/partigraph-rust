//! Wilson's algorithm must sample spanning trees uniformly.
//!
//! This is the property the library exists to provide, and the one most easily
//! broken in silence by an indexing slip: a biased sampler still returns
//! perfectly valid trees. Each case enumerates every spanning tree by brute
//! force, draws many, and checks the counts against uniform with chi-square.
//!
//! Seeds are fixed, so these are deterministic.

mod common;

use common::{all_spanning_trees, canonical, complete, cycle, grid};
use partigraph::{random_spanning_tree, Graph, NodeSet};
use rand::{rngs::StdRng, SeedableRng};
use std::collections::HashMap;

/// Draw `draws` trees and return the chi-square statistic against uniform,
/// along with the number of distinct trees the graph has.
fn chi_square(graph: &Graph, draws: usize, seed: u64) -> (f64, usize) {
    let trees = all_spanning_trees(graph);
    let expected_kinds = trees.len();
    assert!(expected_kinds > 1);

    let mut index: HashMap<Vec<(u32, u32)>, usize> = HashMap::new();
    for (i, t) in trees.iter().enumerate() {
        let mut key = t.clone();
        key.sort_unstable();
        index.insert(key, i);
    }

    let subset = NodeSet::all(graph);
    let mut rng = StdRng::seed_from_u64(seed);
    let mut counts = vec![0usize; expected_kinds];
    for _ in 0..draws {
        let tree = random_spanning_tree(graph, &subset, &mut rng).unwrap();
        assert_eq!(tree.len(), graph.node_count(), "tree does not span");
        assert_eq!(tree.edges().count(), graph.node_count() - 1);
        let key = canonical(&tree);
        let i = index
            .get(&key)
            .expect("sampled something that is not a spanning tree");
        counts[*i] += 1;
    }

    let expected = draws as f64 / expected_kinds as f64;
    let stat: f64 = counts
        .iter()
        .map(|&o| (o as f64 - expected).powi(2) / expected)
        .sum();
    (stat, expected_kinds)
}

#[test]
fn cycle_of_five_is_uniform() {
    // A cycle on n nodes has exactly n spanning trees: drop any one edge.
    let g = cycle(5);
    let (stat, kinds) = chi_square(&g, 40_000, 0x5EED);
    assert_eq!(kinds, 5);
    // df = 4, upper 99.9% point of chi-square is 18.47.
    assert!(
        stat < 18.47,
        "chi-square {stat} on a 5-cycle suggests a biased sampler"
    );
}

#[test]
fn k4_is_uniform() {
    // Cayley's formula: n^(n-2) = 16 labelled trees on 4 nodes, all present in K4.
    let g = complete(4);
    let (stat, kinds) = chi_square(&g, 64_000, 0xC0FFEE);
    assert_eq!(kinds, 16);
    // df = 15, upper 99.9% point is 37.70.
    assert!(
        stat < 37.70,
        "chi-square {stat} on K4 suggests a biased sampler"
    );
}

#[test]
fn k5_is_uniform() {
    let g = complete(5);
    let (stat, kinds) = chi_square(&g, 125_000, 0xBEEF);
    assert_eq!(kinds, 125);
    // df = 124, upper 99.9% point is 187.5.
    assert!(
        stat < 187.5,
        "chi-square {stat} on K5 suggests a biased sampler"
    );
}

#[test]
fn grid_2x3_is_uniform() {
    // The 2xn grid spanning tree counts run 1, 4, 15, 56, 209: 2x3 has 15.
    let g = grid(2, 3);
    let (stat, kinds) = chi_square(&g, 60_000, 0x1234);
    assert_eq!(kinds, 15);
    // df = 14, upper 99.9% point is 36.12.
    assert!(
        stat < 36.12,
        "chi-square {stat} on a 2x3 grid suggests a biased sampler"
    );
}

#[test]
fn every_spanning_tree_is_reachable() {
    // Uniformity implies full support; check support directly too, since a
    // sampler that never emits one particular tree is a distinct kind of bug.
    let g = grid(2, 3);
    let subset = NodeSet::all(&g);
    let mut rng = StdRng::seed_from_u64(99);
    let mut seen = std::collections::HashSet::new();
    for _ in 0..5_000 {
        seen.insert(canonical(
            &random_spanning_tree(&g, &subset, &mut rng).unwrap(),
        ));
    }
    assert_eq!(seen.len(), 15, "some spanning trees are never produced");
}

#[test]
fn spans_only_the_subset() {
    let g = grid(4, 4);
    // The top-left 2x2 block.
    let subset = NodeSet::from_nodes(16, [0, 1, 4, 5].map(partigraph::NodeId));
    let mut rng = StdRng::seed_from_u64(7);
    for _ in 0..200 {
        let tree = random_spanning_tree(&g, &subset, &mut rng).unwrap();
        assert_eq!(tree.len(), 4);
        let mut got: Vec<u32> = (0..4).map(|l| tree.global(l).0).collect();
        got.sort_unstable();
        assert_eq!(got, vec![0, 1, 4, 5]);
    }
}
