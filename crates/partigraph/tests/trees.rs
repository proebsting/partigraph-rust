//! `aggregate` and the cut machinery, against brute force.
//!
//! Both are reimplemented here the slow, obvious way — flood fill over an
//! adjacency list rebuilt from the parent pointers — and compared against the
//! library's preorder arithmetic on many random trees.

mod common;

use common::{complete, grid};
use partigraph::{random_spanning_tree, tree, Cut, NodeSet, Side, Tree};
use rand::{rngs::StdRng, Rng, SeedableRng};

fn tree_adjacency(t: &Tree) -> Vec<Vec<u32>> {
    let mut adj = vec![Vec::new(); t.len()];
    for child in t.edges() {
        let p = t.parent(child).unwrap();
        adj[child as usize].push(p);
        adj[p as usize].push(child);
    }
    adj
}

/// Nodes reachable from `start` without crossing the edge `{a, b}`.
fn reachable_avoiding(adj: &[Vec<u32>], start: u32, a: u32, b: u32) -> Vec<u32> {
    let mut seen = vec![false; adj.len()];
    let mut stack = vec![start];
    seen[start as usize] = true;
    let mut out = Vec::new();
    while let Some(v) = stack.pop() {
        out.push(v);
        for &w in &adj[v as usize] {
            let crosses = (v == a && w == b) || (v == b && w == a);
            if !crosses && !seen[w as usize] {
                seen[w as usize] = true;
                stack.push(w);
            }
        }
    }
    out.sort_unstable();
    out
}

/// The nodes a cut names, computed independently of the library.
fn brute_force_side(t: &Tree, cut: Cut) -> Vec<u32> {
    let adj = tree_adjacency(t);
    let parent = t.parent(cut.child).unwrap();
    let start = match cut.side {
        Side::Subtree => cut.child,
        Side::Complement => parent,
    };
    reachable_avoiding(&adj, start, parent, cut.child)
}

fn sorted_locals(t: &Tree, cut: Cut) -> Vec<u32> {
    let (a, b) = t.cut_locals(cut);
    let mut v: Vec<u32> = a.iter().chain(b).copied().collect();
    v.sort_unstable();
    v
}

#[test]
fn cuts_match_flood_fill() {
    let graphs = [grid(3, 4), grid(2, 6), complete(5)];
    for g in &graphs {
        let subset = NodeSet::all(g);
        let mut rng = StdRng::seed_from_u64(31);
        for _ in 0..50 {
            let t = random_spanning_tree(g, &subset, &mut rng).unwrap();
            for cut in t.cuts() {
                assert_eq!(
                    sorted_locals(&t, cut),
                    brute_force_side(&t, cut),
                    "cut {cut:?}"
                );
                assert_eq!(t.cut_size(cut), brute_force_side(&t, cut).len());
            }
        }
    }
}

#[test]
fn the_two_sides_of_an_edge_partition_the_tree() {
    let g = grid(4, 4);
    let subset = NodeSet::all(&g);
    let mut rng = StdRng::seed_from_u64(1);
    for _ in 0..50 {
        let t = random_spanning_tree(&g, &subset, &mut rng).unwrap();
        for child in t.edges() {
            let down = sorted_locals(
                &t,
                Cut {
                    child,
                    side: Side::Subtree,
                },
            );
            let up = sorted_locals(
                &t,
                Cut {
                    child,
                    side: Side::Complement,
                },
            );
            assert_eq!(down.len() + up.len(), t.len());
            let mut both: Vec<u32> = down.iter().chain(&up).copied().collect();
            both.sort_unstable();
            assert_eq!(both, (0..t.len() as u32).collect::<Vec<_>>());
            // Neither side is ever empty: removing one tree edge gives two
            // non-empty components, so every move leaves something behind.
            assert!(!down.is_empty() && !up.is_empty());
        }
    }
}

#[test]
fn sums_match_brute_force() {
    let g = grid(3, 5);
    let subset = NodeSet::all(&g);
    let mut rng = StdRng::seed_from_u64(2024);
    for _ in 0..40 {
        let weights: Vec<f64> = (0..g.node_count())
            .map(|_| rng.gen_range(0.0..100.0))
            .collect();
        let t = random_spanning_tree(&g, &subset, &mut rng).unwrap();
        let got = tree::sums(&t, |l| weights[t.global(l).index()]);

        for cut in t.cuts() {
            let want: f64 = brute_force_side(&t, cut)
                .iter()
                .map(|&l| weights[t.global(l).index()])
                .sum();
            let diff = (got.get(cut) - want).abs();
            assert!(diff < 1e-9, "cut {cut:?}: got {} want {want}", got.get(cut));
        }
        let total: f64 = weights.iter().sum();
        assert!((got.total(&t) - total).abs() < 1e-9);
    }
}

#[test]
fn any_matches_brute_force() {
    let g = grid(3, 4);
    let subset = NodeSet::all(&g);
    let mut rng = StdRng::seed_from_u64(55);
    for _ in 0..40 {
        let flagged: Vec<bool> = (0..g.node_count()).map(|_| rng.gen_bool(0.2)).collect();
        let t = random_spanning_tree(&g, &subset, &mut rng).unwrap();
        let got = tree::any(&t, |l| flagged[t.global(l).index()]);
        for cut in t.cuts() {
            let want = brute_force_side(&t, cut)
                .iter()
                .any(|&l| flagged[t.global(l).index()]);
            assert_eq!(*got.get(cut), want, "cut {cut:?}");
        }
    }
}

#[test]
fn aggregate_handles_a_star_without_quadratic_blowup() {
    // A star is the tree shape that makes the Python's second pass O(k^2): the
    // hub is the source of every edge, so it rescans its whole neighbour list
    // once per edge. Correctness check on the shape that provoked the rewrite.
    let n = 400u32;
    let g = partigraph::Graph::from_edges(n as usize, (1..n).map(|v| (0, v))).unwrap();
    let subset = NodeSet::all(&g);
    let mut rng = StdRng::seed_from_u64(3);
    let t = random_spanning_tree(&g, &subset, &mut rng).unwrap();
    let weights: Vec<f64> = (0..n).map(|v| v as f64).collect();
    let got = tree::sums(&t, |l| weights[t.global(l).index()]);
    let total: f64 = weights.iter().sum();

    for cut in t.cuts() {
        let want: f64 = brute_force_side(&t, cut)
            .iter()
            .map(|&l| weights[t.global(l).index()])
            .sum();
        assert!((got.get(cut) - want).abs() < 1e-9);
        // The two directions of an edge must account for everything.
        assert!((got.get(cut) + got.get(cut.flip()) - total).abs() < 1e-9);
    }
}

#[test]
fn aggregate_works_for_a_monoid_other_than_sum_and_or() {
    let g = grid(3, 3);
    let subset = NodeSet::all(&g);
    let mut rng = StdRng::seed_from_u64(4);
    let t = random_spanning_tree(&g, &subset, &mut rng).unwrap();
    let value = |l: u32| t.global(l).0 as i64;
    let got = tree::aggregate(&t, value, i64::MIN, |a, b| *a.max(b));
    for cut in t.cuts() {
        let want = brute_force_side(&t, cut)
            .iter()
            .map(|&l| value(l))
            .max()
            .unwrap();
        assert_eq!(*got.get(cut), want);
    }
}

#[test]
fn a_single_node_tree_has_no_edges() {
    let g = grid(3, 3);
    let subset = NodeSet::from_nodes(9, [partigraph::NodeId(4)]);
    let mut rng = StdRng::seed_from_u64(5);
    let t = random_spanning_tree(&g, &subset, &mut rng).unwrap();
    assert_eq!(t.len(), 1);
    assert_eq!(t.edges().count(), 0);
    assert_eq!(t.cuts().count(), 0);
    assert_eq!(t.global(t.root()), partigraph::NodeId(4));
}
