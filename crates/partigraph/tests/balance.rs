//! `partition_balanced`: the whole job in one call, for a caller that already
//! holds a graph and a weight per node.

mod common;

use common::{grid, is_contiguous};
use partigraph::{
    partition_balanced, BalanceParams, Error, Graph, Induced, NodeSet, SamplerChoice, TreeSampler,
};
use rand::{rngs::StdRng, SeedableRng};

fn params(parts: usize, epsilon: f64, seed: u64) -> BalanceParams {
    BalanceParams {
        parts,
        epsilon,
        seed,
        ..Default::default()
    }
}

#[test]
fn divides_a_grid_into_equal_parts() {
    let g = grid(6, 6);
    let weights = vec![1.0; 36];
    let result = partition_balanced(&g, &weights, &params(6, 0.0, 1)).unwrap();

    assert_eq!(result.ideal, 6.0);
    assert_eq!(result.part_weights, vec![6.0; 6]);
    assert_eq!(result.sizes(), vec![6; 6]);
    assert_eq!(result.worst_deviation(), 0.0);
    assert_eq!(result.partition.part_count(), 6);
    for p in 0..6u32 {
        let nodes: Vec<u32> = result.partition.part(p).map(|v| v.0).collect();
        assert!(is_contiguous(&g, &nodes), "part {p} is not contiguous");
    }
}

#[test]
fn honours_the_tolerance_on_uneven_weights() {
    let g = grid(8, 8);
    let weights: Vec<f64> = (0..64).map(|i| (i % 7 + 1) as f64).collect();
    let total: f64 = weights.iter().sum();

    for (parts, epsilon) in [(2usize, 0.02f64), (4, 0.05), (5, 0.1)] {
        let result = partition_balanced(&g, &weights, &params(parts, epsilon, 3)).unwrap();
        assert_eq!(result.ideal, total / parts as f64);
        assert_eq!(result.part_weights.len(), parts);
        assert!(
            result.worst_deviation() <= epsilon + 1e-12,
            "{parts} parts at {epsilon}: worst {}",
            result.worst_deviation()
        );
        assert!((result.part_weights.iter().sum::<f64>() - total).abs() < 1e-9);
    }
}

#[test]
fn the_same_seed_reproduces_the_result() {
    let g = grid(5, 5);
    let weights = vec![1.0; 25];
    let run = |seed| partition_balanced(&g, &weights, &params(5, 0.0, seed)).unwrap();
    assert_eq!(run(42), run(42));
    assert!(
        (0..5).any(|s| run(s) != run(s + 50)),
        "the seed is not being used"
    );
}

#[test]
fn either_sampler_can_be_named_at_runtime() {
    let g = grid(6, 6);
    let weights = vec![1.0; 36];
    for sampler in [SamplerChoice::Uniform, SamplerChoice::MinWeightTree] {
        let p = BalanceParams {
            sampler,
            ..params(6, 0.0, 2)
        };
        let result = partition_balanced(&g, &weights, &p).unwrap();
        assert_eq!(result.part_weights, vec![6.0; 6], "{sampler:?}");
    }

    // And choosing differently actually changes the result.
    let uniform = partition_balanced(
        &g,
        &weights,
        &BalanceParams {
            sampler: SamplerChoice::Uniform,
            ..params(6, 0.0, 1)
        },
    );
    let mst = partition_balanced(
        &g,
        &weights,
        &BalanceParams {
            sampler: SamplerChoice::MinWeightTree,
            ..params(6, 0.0, 1)
        },
    );
    assert_ne!(uniform.unwrap().partition, mst.unwrap().partition);
}

#[test]
fn sampler_choice_is_itself_a_sampler() {
    // So it can be handed to `with_sampler` directly, not only through params.
    let g = grid(4, 4);
    let ind = Induced::new(&g, &NodeSet::all(&g));
    for choice in [SamplerChoice::Uniform, SamplerChoice::MinWeightTree] {
        let tree = choice.sample(&ind, &mut StdRng::seed_from_u64(1)).unwrap();
        assert_eq!(tree.len(), 16);
        assert_eq!(tree.edges().count(), 15);
    }
    assert_eq!(SamplerChoice::default(), SamplerChoice::Uniform);
}

#[test]
fn the_default_parameters_are_usable() {
    let g = grid(4, 4);
    let weights = vec![1.0; 16];
    let result = partition_balanced(&g, &weights, &BalanceParams::default()).unwrap();
    assert_eq!(result.partition.part_count(), 2);
    assert_eq!(result.ideal, 8.0);
}

#[test]
fn a_single_part_takes_everything() {
    let g = grid(3, 3);
    let weights = vec![2.0; 9];
    let result = partition_balanced(&g, &weights, &params(1, 0.0, 0)).unwrap();
    assert_eq!(result.ideal, 18.0);
    assert_eq!(result.part_weights, vec![18.0]);
    assert_eq!(result.partition.assignments(), vec![0; 9]);
}

#[test]
fn bad_requests_are_named() {
    let g = grid(3, 3);
    let weights = vec![1.0; 9];

    assert_eq!(
        partition_balanced(&g, &weights, &params(0, 0.1, 0)).unwrap_err(),
        Error::NoParts
    );
    assert_eq!(
        partition_balanced(&g, &weights, &params(99, 0.1, 0)).unwrap_err(),
        Error::TooFewNodes {
            nodes: 9,
            parts: 99
        }
    );
    assert_eq!(
        partition_balanced(&g, &weights, &params(2, -0.5, 0)).unwrap_err(),
        Error::BadEpsilon(-0.5)
    );
    assert_eq!(
        partition_balanced(&g, &weights, &params(2, f64::INFINITY, 0)).unwrap_err(),
        Error::BadEpsilon(f64::INFINITY)
    );
    assert!(matches!(
        partition_balanced(&g, &weights, &params(2, f64::NAN, 0)).unwrap_err(),
        Error::BadEpsilon(e) if e.is_nan()
    ));
    assert_eq!(
        partition_balanced(&g, &[1.0; 3], &params(2, 0.1, 0)).unwrap_err(),
        Error::WeightCountMismatch {
            nodes: 9,
            weights: 3
        }
    );

    // Two triangles with nothing between them.
    let split = Graph::from_edges(6, [(0, 1), (1, 2), (2, 0), (3, 4), (4, 5), (5, 3)]).unwrap();
    assert_eq!(
        partition_balanced(&split, &[1.0; 6], &params(2, 0.5, 0)).unwrap_err(),
        Error::Disconnected
    );
}

#[test]
fn an_unreachable_tolerance_gives_up() {
    // Nine unit weights cannot make two exactly equal parts.
    let g = grid(3, 3);
    let mut p = params(2, 0.0, 0);
    p.search.max_attempts = 25;
    assert_eq!(
        partition_balanced(&g, &[1.0; 9], &p).unwrap_err(),
        Error::Exhausted { attempts: 25 }
    );
}

#[test]
fn zero_total_weight_does_not_divide_by_it() {
    let g = grid(3, 3);
    let result = partition_balanced(&g, &[0.0; 9], &params(3, 0.0, 0)).unwrap();
    assert_eq!(result.ideal, 0.0);
    assert_eq!(result.part_weights, vec![0.0; 3]);
    assert_eq!(
        result.worst_deviation(),
        0.0,
        "must not be NaN from dividing by zero"
    );
}
