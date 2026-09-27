//! The pluggable tree samplers.

mod common;

use common::{canonical, cycle, grid, is_contiguous};
use partigraph::{
    min_weight_spanning_tree, within_epsilon, Error, Graph, Induced, MinWeightTree, NodeId,
    NodeSet, Partitioner, RandomWeights, SearchOptions, TreeSampler, Uniform,
};
use rand::{rngs::StdRng, Rng, RngCore, SeedableRng};
use std::collections::HashSet;

fn draw(
    sampler: &impl TreeSampler,
    g: &Graph,
    subset: &NodeSet,
    rng: &mut StdRng,
) -> Vec<(u32, u32)> {
    let ind = Induced::new(g, subset);
    canonical(&sampler.sample(&ind, rng).unwrap())
}

#[test]
fn a_min_weight_tree_is_a_spanning_tree() {
    let g = grid(4, 5);
    let subset = NodeSet::all(&g);
    let ind = Induced::new(&g, &subset);
    let sampler = MinWeightTree::random();
    let mut rng = StdRng::seed_from_u64(1);

    for _ in 0..200 {
        let t = sampler.sample(&ind, &mut rng).unwrap();
        assert_eq!(t.len(), 20);
        assert_eq!(t.edges().count(), 19);
        // Every edge of the tree is an edge of the graph.
        for child in t.edges() {
            let (a, b) = (t.global(child), t.global(t.parent(child).unwrap()));
            assert!(
                g.neighbors(a).contains(&b.0),
                "{a:?}-{b:?} is not a graph edge"
            );
        }
        // Reaching every node from the root proves it is connected, and k-1
        // edges then proves it is acyclic.
        assert_eq!(t.preorder().len(), 20);
    }
}

#[test]
fn a_min_weight_tree_spans_only_its_subset() {
    let g = grid(4, 4);
    let subset = NodeSet::from_nodes(16, [0, 1, 4, 5].map(NodeId));
    let ind = Induced::new(&g, &subset);
    let mut rng = StdRng::seed_from_u64(2);
    for _ in 0..50 {
        let t = MinWeightTree::random().sample(&ind, &mut rng).unwrap();
        let mut got: Vec<u32> = (0..4).map(|l| t.global(l).0).collect();
        got.sort_unstable();
        assert_eq!(got, vec![0, 1, 4, 5]);
    }
}

#[test]
fn constant_weights_give_a_deterministic_tree() {
    // No randomness in the weights means no randomness in the tree, whatever
    // the rng is doing.
    let g = grid(3, 4);
    let ind = Induced::new(&g, &NodeSet::all(&g));
    let flat = MinWeightTree::new(|_: NodeId, _: NodeId, _: &mut dyn RngCore| 1.0);

    let first = canonical(&flat.sample(&ind, &mut StdRng::seed_from_u64(1)).unwrap());
    let second = canonical(&flat.sample(&ind, &mut StdRng::seed_from_u64(99)).unwrap());
    assert_eq!(first, second);
}

#[test]
fn the_heaviest_edge_of_a_cycle_is_always_dropped() {
    // A cycle has one spanning tree per edge: the one that omits it. With a
    // weight function that makes 0-1 dearest, that is the edge left out.
    let g = cycle(6);
    let ind = Induced::new(&g, &NodeSet::all(&g));
    let sampler = MinWeightTree::new(|a: NodeId, b: NodeId, _: &mut dyn RngCore| {
        let pair = (a.0.min(b.0), a.0.max(b.0));
        if pair == (0, 1) {
            100.0
        } else {
            1.0
        }
    });
    let mut rng = StdRng::seed_from_u64(3);
    for _ in 0..20 {
        let edges = canonical(&sampler.sample(&ind, &mut rng).unwrap());
        assert!(
            !edges.contains(&(0, 1)),
            "the dearest edge survived: {edges:?}"
        );
        assert_eq!(edges.len(), 5);
    }
}

#[test]
fn a_cost_function_can_keep_labelled_groups_whole() {
    // Label the two 2x4 halves of a 4x4 grid differently. Charging extra for
    // the edges that run between labels should leave a tree that crosses just
    // once, which is the least it can do and stay connected.
    let g = grid(4, 4);
    let ind = Induced::new(&g, &NodeSet::all(&g));
    let label = |v: NodeId| v.0 / 8; // rows 0-1 vs rows 2-3
    let sampler = MinWeightTree::new(move |a: NodeId, b: NodeId, rng: &mut dyn RngCore| {
        rng.gen::<f64>() + if label(a) != label(b) { 10.0 } else { 0.0 }
    });

    let mut rng = StdRng::seed_from_u64(4);
    for _ in 0..50 {
        let edges = canonical(&sampler.sample(&ind, &mut rng).unwrap());
        let crossings = edges
            .iter()
            .filter(|&&(a, b)| label(NodeId(a)) != label(NodeId(b)))
            .count();
        assert_eq!(crossings, 1, "expected one crossing, got {crossings}");
    }

    // Without the extra cost it crosses more often, so the test above is not
    // measuring something that would happen anyway.
    let plain = MinWeightTree::random();
    let mut rng = StdRng::seed_from_u64(4);
    let plain_crossings: usize = (0..50)
        .map(|_| {
            canonical(&plain.sample(&ind, &mut rng).unwrap())
                .iter()
                .filter(|&&(a, b)| label(NodeId(a)) != label(NodeId(b)))
                .count()
        })
        .sum();
    assert!(
        plain_crossings > 50,
        "unsurcharged trees crossed only {plain_crossings} times in 50"
    );
}

#[test]
fn the_weight_function_sees_parent_graph_nodes() {
    // Even when sampling a subset, the endpoints handed to the weight function
    // are nodes of the whole graph, so a caller's own tables line up.
    let g = grid(4, 4);
    let subset = NodeSet::from_nodes(16, [5, 6, 9, 10].map(NodeId));
    let ind = Induced::new(&g, &subset);
    let seen = std::cell::RefCell::new(HashSet::new());
    let sampler = MinWeightTree::new(|a: NodeId, b: NodeId, _: &mut dyn RngCore| {
        seen.borrow_mut().insert((a.0.min(b.0), a.0.max(b.0)));
        1.0
    });
    sampler.sample(&ind, &mut StdRng::seed_from_u64(5)).unwrap();

    let got = seen.into_inner();
    assert_eq!(got, HashSet::from([(5, 6), (5, 9), (6, 10), (9, 10)]));
}

#[test]
fn a_disconnected_subset_is_refused() {
    let g = grid(3, 3);
    let ind = Induced::new(&g, &NodeSet::from_nodes(9, [0, 8].map(NodeId)));
    let err = MinWeightTree::random()
        .sample(&ind, &mut StdRng::seed_from_u64(6))
        .unwrap_err();
    assert_eq!(err, Error::Disconnected);
}

#[test]
fn a_lone_node_gives_a_tree_with_no_edges() {
    let g = grid(3, 3);
    let ind = Induced::new(&g, &NodeSet::from_nodes(9, [4].map(NodeId)));
    let t = min_weight_spanning_tree(&ind, &RandomWeights, &mut StdRng::seed_from_u64(7)).unwrap();
    assert_eq!(t.len(), 1);
    assert_eq!(t.edges().count(), 0);
    assert_eq!(t.global(t.root()), NodeId(4));
}

#[test]
fn the_partitioner_works_with_either_sampler() {
    let g = grid(6, 6);
    let weights = vec![1.0; 36];
    let base = Partitioner::new(&g, &weights).unwrap();
    let with_mst = Partitioner::new(&g, &weights)
        .unwrap()
        .with_sampler(MinWeightTree::random());
    let opts = SearchOptions::default();

    for seed in 0..5u64 {
        for (name, plan) in [
            (
                "uniform",
                base.search(
                    6,
                    within_epsilon(6.0, 0.0),
                    &opts,
                    &mut StdRng::seed_from_u64(seed),
                ),
            ),
            (
                "mst",
                with_mst.search(
                    6,
                    within_epsilon(6.0, 0.0),
                    &opts,
                    &mut StdRng::seed_from_u64(seed),
                ),
            ),
        ] {
            let plan = plan.unwrap_or_else(|e| panic!("{name} sampler failed: {e}"));
            assert_eq!(plan.district_weights(&weights), vec![6.0; 6], "{name}");
            for d in 0..6u32 {
                let nodes: Vec<u32> = plan.district(d).map(|v| v.0).collect();
                assert!(
                    is_contiguous(&g, &nodes),
                    "{name}: district {d} is not contiguous"
                );
            }
        }
    }
}

#[test]
fn the_two_samplers_produce_different_plans() {
    // Same graph, same seed: if swapping the sampler changed nothing, it would
    // not be wired in.
    let g = grid(6, 6);
    let weights = vec![1.0; 36];
    let opts = SearchOptions::default();
    let uniform = Partitioner::new(&g, &weights)
        .unwrap()
        .search(
            6,
            within_epsilon(6.0, 0.0),
            &opts,
            &mut StdRng::seed_from_u64(1),
        )
        .unwrap();
    let mst = Partitioner::new(&g, &weights)
        .unwrap()
        .with_sampler(MinWeightTree::random())
        .search(
            6,
            within_epsilon(6.0, 0.0),
            &opts,
            &mut StdRng::seed_from_u64(1),
        )
        .unwrap();
    assert_ne!(uniform.assignments(), mst.assignments());
}

#[test]
fn the_default_sampler_is_wilsons() {
    // `Partitioner::new` and an explicit `Uniform` must agree exactly.
    let g = grid(5, 5);
    let weights = vec![1.0; 25];
    let opts = SearchOptions::default();
    let implicit = Partitioner::new(&g, &weights)
        .unwrap()
        .search(
            5,
            within_epsilon(5.0, 0.0),
            &opts,
            &mut StdRng::seed_from_u64(8),
        )
        .unwrap();
    let explicit = Partitioner::new(&g, &weights)
        .unwrap()
        .with_sampler(Uniform)
        .search(
            5,
            within_epsilon(5.0, 0.0),
            &opts,
            &mut StdRng::seed_from_u64(8),
        )
        .unwrap();
    assert_eq!(implicit.assignments(), explicit.assignments());
}

#[test]
fn both_samplers_reach_many_trees() {
    let g = grid(2, 3);
    let subset = NodeSet::all(&g);
    let mut rng = StdRng::seed_from_u64(9);
    for (name, count) in [
        (
            "uniform",
            (0..3000)
                .map(|_| draw(&Uniform, &g, &subset, &mut rng))
                .collect::<HashSet<_>>()
                .len(),
        ),
        (
            "mst",
            (0..3000)
                .map(|_| draw(&MinWeightTree::random(), &g, &subset, &mut rng))
                .collect::<HashSet<_>>()
                .len(),
        ),
    ] {
        assert_eq!(count, 15, "{name} reached {count} of the 15 spanning trees");
    }
}
