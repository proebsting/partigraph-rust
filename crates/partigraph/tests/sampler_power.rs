//! The uniformity tests must be able to fail.
//!
//! A chi-square test with too few draws passes for any sampler, which would make
//! `uniformity.rs` vacuous. This checks the bar has teeth by running a sampler
//! that is known *not* to be uniform through the same machinery and requiring it
//! to be rejected.
//!
//! The stand-in is the library's own [`MinWeightTree`] with random weights,
//! which is what GerryChain's `random_spanning_tree` does. It is a perfectly
//! good way to draw a varied tree and it is a supported sampler here, but it is
//! not uniform over spanning trees: on K4 it over-produces stars.
//!
//! If this test ever fails because the MST sampler has become uniform,
//! something is wrong with the MST, not with the bar.

mod common;

use common::{all_spanning_trees, canonical, complete};
use partigraph::{Graph, Induced, MinWeightTree, NodeSet, TreeSampler, Uniform};
use rand::{rngs::StdRng, SeedableRng};
use std::collections::HashMap;

fn chi_square(g: &Graph, sampler: &impl TreeSampler, draws: usize, seed: u64) -> f64 {
    let trees = all_spanning_trees(g);
    let index: HashMap<Vec<(u32, u32)>, usize> = trees
        .iter()
        .cloned()
        .enumerate()
        .map(|(i, t)| (t, i))
        .collect();

    let ind = Induced::new(g, &NodeSet::all(g));
    let mut rng = StdRng::seed_from_u64(seed);
    let mut counts = vec![0usize; trees.len()];
    for _ in 0..draws {
        let key = canonical(&sampler.sample(&ind, &mut rng).unwrap());
        counts[*index.get(&key).expect("not a spanning tree")] += 1;
    }
    let expected = draws as f64 / trees.len() as f64;
    counts
        .iter()
        .map(|&o| (o as f64 - expected).powi(2) / expected)
        .sum()
}

#[test]
fn a_non_uniform_sampler_is_rejected_where_wilsons_passes() {
    // Same graph, draw count, seed and threshold as `uniformity::k4_is_uniform`.
    let g = complete(4);
    let bar = 37.70; // df = 15, upper 99.9% point
    let draws = 64_000;

    let wilson = chi_square(&g, &Uniform, draws, 0xC0FFEE);
    let mst = chi_square(&g, &MinWeightTree::random(), draws, 0xC0FFEE);

    assert!(
        wilson < bar,
        "Wilson's should pass the bar, got chi-square {wilson}"
    );
    assert!(
        mst > bar * 2.0,
        "the bar has no teeth: a random-weight MST scored only {mst} against a bar of {bar}"
    );
}

#[test]
fn the_mst_bias_on_k4_is_toward_stars() {
    // Naming the shape of the bias, so the test above is not just an opaque
    // number: K4 has 4 stars and 12 paths, and the MST sampler favours stars.
    let g = complete(4);
    let ind = Induced::new(&g, &NodeSet::all(&g));
    let mut rng = StdRng::seed_from_u64(0xC0FFEE);

    let mut counts = [0usize; 2]; // [paths, stars]
    let draws = 40_000;
    for _ in 0..draws {
        let t = MinWeightTree::random().sample(&ind, &mut rng).unwrap();
        let mut degree = [0usize; 4];
        for child in t.edges() {
            degree[t.global(child).index()] += 1;
            degree[t.global(t.parent(child).unwrap()).index()] += 1;
        }
        let is_star = degree.contains(&3);
        counts[usize::from(is_star)] += 1;
    }

    // Uniform would give 4 stars out of 16 trees, so a share of 0.25.
    let uniform_share = 4.0 / 16.0;
    let star_share = counts[1] as f64 / draws as f64;
    let sd = (uniform_share * (1.0 - uniform_share) / draws as f64).sqrt();
    let sigmas = (star_share - uniform_share) / sd;
    assert!(
        sigmas > 4.0,
        "expected stars to be over-produced; share {star_share:.4} is only {sigmas:.1} sd above {uniform_share}"
    );
}
