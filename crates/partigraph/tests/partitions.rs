//! End-to-end partitioning: the invariants a plan must satisfy, and the error
//! paths the Python either hangs on or reports as a bare `ValueError`.

mod common;

use common::{grid, is_contiguous};
use partigraph::{within_epsilon, Error, Graph, NodeId, NodeSet, Partitioner, Plan, SearchOptions};
use rand::{rngs::StdRng, Rng, SeedableRng};

/// Every node assigned exactly once, every district non-empty and contiguous.
fn check_plan(graph: &Graph, plan: &Plan) {
    assert_eq!(plan.assignments().len(), graph.node_count());
    for (i, &d) in plan.assignments().iter().enumerate() {
        assert!(
            (d as usize) < plan.district_count(),
            "node {i} is assigned to district {d}, outside 0..{}",
            plan.district_count()
        );
    }
    for d in 0..plan.district_count() as u32 {
        let nodes: Vec<u32> = plan.district(d).map(|v| v.0).collect();
        assert!(!nodes.is_empty(), "district {d} is empty");
        assert!(
            is_contiguous(graph, &nodes),
            "district {d} is not contiguous"
        );
    }
}

#[test]
fn splits_a_grid_into_exactly_equal_districts() {
    let g = grid(4, 4);
    let weights = vec![1.0; 16];
    let p = Partitioner::new(&g, &weights).unwrap();
    let opts = SearchOptions::default();

    for seed in 0..20u64 {
        let mut rng = StdRng::seed_from_u64(seed);
        let plan = p
            .search(4, within_epsilon(4.0, 0.0), &opts, &mut rng)
            .unwrap();
        check_plan(&g, &plan);
        assert_eq!(plan.district_weights(&weights), vec![4.0; 4]);
    }
}

#[test]
fn splits_a_larger_grid_into_exactly_equal_districts() {
    let g = grid(6, 6);
    let weights = vec![1.0; 36];
    let p = Partitioner::new(&g, &weights).unwrap();
    let opts = SearchOptions::default();

    for seed in 0..5u64 {
        let mut rng = StdRng::seed_from_u64(seed);
        let plan = p
            .search(6, within_epsilon(6.0, 0.0), &opts, &mut rng)
            .unwrap();
        check_plan(&g, &plan);
        assert_eq!(plan.district_weights(&weights), vec![6.0; 6]);
    }
}

#[test]
fn honours_a_tolerance_on_uneven_weights() {
    let g = grid(8, 8);
    let mut rng = StdRng::seed_from_u64(77);
    let weights: Vec<f64> = (0..64).map(|_| rng.gen_range(1.0..20.0)).collect();
    let total: f64 = weights.iter().sum();
    let districts = 4;
    let target = total / districts as f64;
    let epsilon = 0.05;

    let p = Partitioner::new(&g, &weights).unwrap();
    let plan = p
        .search(
            districts,
            within_epsilon(target, epsilon),
            &SearchOptions::default(),
            &mut rng,
        )
        .unwrap();

    check_plan(&g, &plan);
    for (d, w) in plan.district_weights(&weights).iter().enumerate() {
        let deviation = (w - target).abs() / target;
        assert!(
            deviation <= epsilon,
            "district {d} deviates by {deviation}, over {epsilon}"
        );
    }
}

#[test]
fn a_district_per_node_is_allowed() {
    let g = grid(2, 2);
    let weights = vec![1.0; 4];
    let p = Partitioner::new(&g, &weights).unwrap();
    let mut rng = StdRng::seed_from_u64(9);
    let plan = p
        .search(
            4,
            within_epsilon(1.0, 0.0),
            &SearchOptions::default(),
            &mut rng,
        )
        .unwrap();
    check_plan(&g, &plan);
    let mut assigned = plan.assignments().to_vec();
    assigned.sort_unstable();
    assert_eq!(assigned, vec![0, 1, 2, 3]);
}

#[test]
fn one_district_takes_everything() {
    let g = grid(3, 3);
    let weights = vec![1.0; 9];
    let p = Partitioner::new(&g, &weights).unwrap();
    let mut rng = StdRng::seed_from_u64(11);
    let plan = p
        .search(1, |_| true, &SearchOptions::default(), &mut rng)
        .unwrap();
    assert_eq!(plan.assignments(), vec![0; 9]);
}

#[test]
fn the_same_seed_gives_the_same_plan() {
    let g = grid(5, 5);
    let weights = vec![1.0; 25];
    let p = Partitioner::new(&g, &weights).unwrap();
    let opts = SearchOptions::default();
    let run = |seed| {
        let mut rng = StdRng::seed_from_u64(seed);
        p.search(5, within_epsilon(5.0, 0.0), &opts, &mut rng)
            .unwrap()
            .assignments()
            .to_vec()
    };
    assert_eq!(run(42), run(42));
    // Different seeds should explore; if this ever collides it is chance, but
    // across five pairs it would mean the rng is not being threaded through.
    assert!((0..5).any(|s| run(s) != run(s + 100)));
}

#[test]
fn an_unsatisfiable_constraint_gives_up_instead_of_hanging() {
    // Three unit nodes cannot be split into two districts of 1.5 each. The
    // Python loops forever here.
    let g = grid(1, 3);
    let weights = vec![1.0; 3];
    let p = Partitioner::new(&g, &weights).unwrap();
    let opts = SearchOptions {
        max_attempts: 50,
        ..SearchOptions::default()
    };
    let mut rng = StdRng::seed_from_u64(1);
    assert_eq!(
        p.search(2, within_epsilon(1.5, 0.0), &opts, &mut rng),
        Err(Error::Exhausted { attempts: 50 })
    );
}

#[test]
fn a_disconnected_graph_is_refused_rather_than_hung_on() {
    // Two triangles with nothing between them. Wilson's walk from the far
    // component never reaches the tree.
    let g = Graph::from_edges(6, [(0, 1), (1, 2), (2, 0), (3, 4), (4, 5), (5, 3)]).unwrap();
    let weights = vec![1.0; 6];
    let p = Partitioner::new(&g, &weights).unwrap();
    let mut rng = StdRng::seed_from_u64(1);
    assert_eq!(
        p.search(2, |_| true, &SearchOptions::default(), &mut rng),
        Err(Error::Disconnected)
    );
}

#[test]
fn bad_inputs_are_named() {
    let g = grid(2, 2);
    assert_eq!(
        Partitioner::new(&g, &[1.0, 1.0]).unwrap_err(),
        Error::WeightCountMismatch {
            nodes: 4,
            weights: 2
        }
    );
    assert_eq!(
        Partitioner::new(&g, &[1.0, -1.0, 1.0, 1.0]).unwrap_err(),
        Error::BadWeight {
            node: NodeId(1),
            weight: -1.0
        }
    );
    // NaN never equals itself, so match the shape rather than the value.
    assert!(matches!(
        Partitioner::new(&g, &[1.0, f64::NAN, 1.0, 1.0]).unwrap_err(),
        Error::BadWeight { node: NodeId(1), weight } if weight.is_nan()
    ));
    assert_eq!(
        Partitioner::new(&g, &[1.0, 1.0, f64::INFINITY, 1.0]).unwrap_err(),
        Error::BadWeight {
            node: NodeId(2),
            weight: f64::INFINITY
        }
    );

    let weights = vec![1.0; 4];
    let p = Partitioner::new(&g, &weights).unwrap();
    let mut rng = StdRng::seed_from_u64(1);
    let opts = SearchOptions::default();
    assert_eq!(
        p.search(0, |_| true, &opts, &mut rng),
        Err(Error::NoDistricts)
    );
    assert_eq!(
        p.search(9, |_| true, &opts, &mut rng),
        Err(Error::TooFewNodes {
            nodes: 4,
            districts: 9
        })
    );
}

#[test]
fn a_transfer_between_strangers_is_an_error_not_a_backtrack() {
    // The Python raises ValueError here, which its own backtrack handler then
    // swallows, so a real bug looks like an unlucky division.
    let g = grid(1, 5);
    let weights = vec![1.0; 5];
    let p = Partitioner::new(&g, &weights).unwrap();
    let left = NodeSet::from_nodes(5, [0].map(NodeId));
    let right = NodeSet::from_nodes(5, [3, 4].map(NodeId));
    let mut rng = StdRng::seed_from_u64(1);
    assert_eq!(
        p.best_move(&left, &right, 1.0, None, &mut rng),
        Err(Error::NotAdjacent)
    );
}

#[test]
fn a_single_node_cannot_be_divided() {
    let g = grid(3, 3);
    let weights = vec![1.0; 9];
    let p = Partitioner::new(&g, &weights).unwrap();
    let lone = NodeSet::from_nodes(9, [4].map(NodeId));
    let mut rng = StdRng::seed_from_u64(1);
    assert_eq!(
        p.closest_random_division(&lone, 0.5, &mut rng),
        Err(Error::Indivisible)
    );
}

#[test]
fn a_division_splits_its_input_and_accounts_for_all_the_weight() {
    let g = grid(4, 4);
    let weights: Vec<f64> = (0..16).map(|v| (v % 5) as f64 + 1.0).collect();
    let total: f64 = weights.iter().sum();
    let p = Partitioner::new(&g, &weights).unwrap();
    let all = NodeSet::all(&g);
    let mut rng = StdRng::seed_from_u64(8);

    for _ in 0..50 {
        let d = p
            .closest_random_division(&all, total / 2.0, &mut rng)
            .unwrap();
        assert_eq!(d.a.len() + d.b.len(), 16);
        assert!(!d.a.is_empty() && !d.b.is_empty());
        for v in d.a.iter() {
            assert!(!d.b.contains(v));
        }
        assert!((d.a_weight + d.b_weight - total).abs() < 1e-9);
        assert!((p.total_weight(&d.a) - d.a_weight).abs() < 1e-9);
        assert!((p.total_weight(&d.b) - d.b_weight).abs() < 1e-9);
        // Both sides come from cutting one tree edge, so both are connected.
        assert!(is_contiguous(&g, d.a.members()));
        assert!(is_contiguous(&g, d.b.members()));
    }
}

#[test]
fn a_transfer_keeps_both_sides_connected() {
    let g = grid(5, 5);
    let weights = vec![1.0; 25];
    let p = Partitioner::new(&g, &weights).unwrap();
    let all = NodeSet::all(&g);
    let mut rng = StdRng::seed_from_u64(12);

    let mut division = p.closest_random_division(&all, 12.0, &mut rng).unwrap();
    for _ in 0..40 {
        division = p
            .improve_division(division, 12.0, (None, None), &mut rng)
            .unwrap();
        assert!((p.total_weight(&division.a) - division.a_weight).abs() < 1e-9);
        assert!((p.total_weight(&division.b) - division.b_weight).abs() < 1e-9);
        assert_eq!(division.a.len() + division.b.len(), 25);
        assert!(is_contiguous(&g, division.a.members()));
        assert!(is_contiguous(&g, division.b.members()));
    }
}

#[test]
fn improvement_moves_toward_the_target() {
    let g = grid(6, 6);
    let weights = vec![1.0; 36];
    let p = Partitioner::new(&g, &weights).unwrap();
    let all = NodeSet::all(&g);
    let target = 18.0;
    let mut rng = StdRng::seed_from_u64(21);

    let start = p.closest_random_division(&all, 1.0, &mut rng).unwrap();
    let before = (start.a_weight - target).abs();
    let after = p
        .repeatedly_improve_division(start, target, 25, (None, None), &mut rng)
        .unwrap();
    assert!(
        (after.a_weight - target).abs() <= before,
        "improvement made the division worse: {before} -> {}",
        (after.a_weight - target).abs()
    );
}
