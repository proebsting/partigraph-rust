//! Time repeated partitions of a grid. `cargo run --release --example bench`.

use partigraph::{within_epsilon, Graph, MinWeightTree, Partitioner, SearchOptions};
use rand::{rngs::StdRng, SeedableRng};
use std::time::Instant;

fn grid(rows: u32, cols: u32) -> Graph {
    let mut edges = Vec::new();
    for r in 0..rows {
        for c in 0..cols {
            let v = r * cols + c;
            if c + 1 < cols {
                edges.push((v, v + 1));
            }
            if r + 1 < rows {
                edges.push((v, v + cols));
            }
        }
    }
    Graph::from_edges((rows * cols) as usize, edges).unwrap()
}

fn main() {
    let side: u32 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(50);
    let districts: usize = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);
    let runs: usize = std::env::args()
        .nth(3)
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);

    let g = grid(side, side);
    let n = g.node_count();
    let weights = vec![1.0; n];
    let target = n as f64 / districts as f64;
    let p = Partitioner::new(&g, &weights).unwrap();
    let opts = SearchOptions::default();

    let mst = Partitioner::new(&g, &weights)
        .unwrap()
        .with_sampler(MinWeightTree::random());

    println!("{n} nodes, {districts} districts, {runs} plans each");
    for name in ["uniform (Wilson's)", "min-weight tree"] {
        let mut rng = StdRng::seed_from_u64(1);
        let mut last = None;
        let start = Instant::now();
        for _ in 0..runs {
            let valid = within_epsilon(target, 0.01);
            let plan = if name.starts_with("uniform") {
                p.search(districts, valid, &opts, &mut rng)
            } else {
                mst.search(districts, valid, &opts, &mut rng)
            };
            last = Some(plan.unwrap());
        }
        let elapsed = start.elapsed();
        let mut sizes: Vec<usize> = (0..districts as u32)
            .map(|d| last.as_ref().unwrap().district(d).count())
            .collect();
        sizes.sort_unstable();
        println!(
            "  {name:20} {:.3}s total, {:6.1} ms/plan   (last plan sizes {sizes:?})",
            elapsed.as_secs_f64(),
            elapsed.as_secs_f64() * 1000.0 / runs as f64
        );
    }
}
