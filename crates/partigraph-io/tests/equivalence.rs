//! The two routes to a seed map must agree.
//!
//! A caller that already holds an adjacency list and a weight vector can call
//! [`partigraph::partition_balanced`] (or [`seed_partition`]) directly, without
//! serialising a graph to JSON so that this crate will accept it. That is only
//! worth anything if it gives the same answer as going through the file, which
//! is what these check.

use partigraph::{partition_balanced, BalanceParams, Graph, NodeId, SamplerChoice};
use partigraph_io::{seed_map, seed_partition, AdjacencyGraph, SeedMapOptions};
use serde_json::{json, Value};

const FIXTURE: &str = include_str!("fixtures/gerrymandria.json");

/// A grid, described once and built two ways.
struct Grid {
    rows: u32,
    cols: u32,
    populations: Vec<f64>,
}

impl Grid {
    fn new(rows: u32, cols: u32, populations: Vec<f64>) -> Grid {
        assert_eq!(populations.len(), (rows * cols) as usize);
        Grid {
            rows,
            cols,
            populations,
        }
    }

    fn edges(&self) -> Vec<(u32, u32)> {
        let mut edges = Vec::new();
        for r in 0..self.rows {
            for c in 0..self.cols {
                let v = r * self.cols + c;
                if c + 1 < self.cols {
                    edges.push((v, v + 1));
                }
                if r + 1 < self.rows {
                    edges.push((v, v + self.cols));
                }
            }
        }
        edges
    }

    /// The route the caller wants: straight into the core.
    fn graph(&self) -> Graph {
        Graph::from_edges(self.populations.len(), self.edges()).unwrap()
    }

    /// The same graph as a networkx adjacency file.
    fn json(&self) -> String {
        let n = self.populations.len();
        let mut adjacency: Vec<Vec<Value>> = vec![Vec::new(); n];
        for (u, v) in self.edges() {
            adjacency[u as usize].push(json!({"id": v}));
            adjacency[v as usize].push(json!({"id": u}));
        }
        let nodes: Vec<Value> = (0..n)
            .map(|i| json!({"pop": self.populations[i], "id": i}))
            .collect();
        json!({
            "directed": false, "multigraph": false, "graph": [],
            "nodes": nodes, "adjacency": adjacency
        })
        .to_string()
    }
}

fn options(parts: usize, epsilon: f64, seed: u64, sampler: SamplerChoice) -> SeedMapOptions {
    SeedMapOptions {
        parts,
        population_attr: "pop".into(),
        epsilon,
        assignment_attr: "district".into(),
        seed,
        sampler,
        ..Default::default()
    }
}

#[test]
fn a_graph_built_directly_gives_the_same_seed_map_as_the_file() {
    let grid = Grid::new(6, 6, vec![1.0; 36]);
    let json = grid.json();
    let graph = grid.graph();

    for sampler in [SamplerChoice::Uniform, SamplerChoice::MinWeightTree] {
        for seed in 0..6u64 {
            let opts = options(4, 0.0, seed, sampler);

            let via_file = seed_map(&AdjacencyGraph::from_json(&json).unwrap(), &opts).unwrap();
            let direct = seed_partition(&graph, &grid.populations, &opts.params()).unwrap();

            assert_eq!(via_file, direct, "sampler {sampler:?}, seed {seed}");
        }
    }
}

#[test]
fn the_core_function_agrees_too() {
    // partition_balanced is the same thing without this crate involved at all.
    let grid = Grid::new(5, 5, (0..25).map(|i| (i % 4 + 1) as f64).collect());
    let opts = options(3, 0.2, 99, SamplerChoice::Uniform);

    let via_file = seed_map(&AdjacencyGraph::from_json(&grid.json()).unwrap(), &opts).unwrap();
    let core = partition_balanced(&grid.graph(), &grid.populations, &opts.params()).unwrap();

    assert_eq!(via_file.partition, core.partition);
    assert_eq!(via_file.populations, core.part_weights);
    assert_eq!(via_file.ideal, core.ideal);
    assert_eq!(via_file.worst_deviation(), core.worst_deviation());
    assert_eq!(via_file.sizes(), core.sizes());
}

#[test]
fn they_agree_on_a_graph_gerrychain_wrote() {
    // Not just on graphs this test invented: the same check against the real
    // fixture, taking the adjacency and weights out of it once and then never
    // touching JSON again.
    let adjacency = AdjacencyGraph::from_json(FIXTURE).unwrap();
    let graph = adjacency.to_graph().unwrap();
    let populations = adjacency.numeric_node_attr("TOTPOP").unwrap();

    for seed in 0..8u64 {
        let opts = SeedMapOptions {
            seed,
            ..options(4, 0.0, seed, SamplerChoice::Uniform)
        };
        let opts = SeedMapOptions {
            population_attr: "TOTPOP".into(),
            ..opts
        };

        let via_file = seed_map(&adjacency, &opts).unwrap();
        let direct = seed_partition(&graph, &populations, &opts.params()).unwrap();
        assert_eq!(via_file, direct, "seed {seed}");
    }
}

#[test]
fn node_position_is_what_lines_the_two_routes_up() {
    // Node ids need not be 0..n, and need not be in order. What matches a file
    // node to a core NodeId is its position in "nodes", so a caller building a
    // Graph directly has to use the same order — and if it does, the answers
    // agree even when the ids are unordered strings.
    let json = json!({
        "directed": false, "multigraph": false, "graph": [],
        "nodes": [{"pop": 3.0, "id": "zeta"}, {"pop": 1.0, "id": "alpha"},
                  {"pop": 3.0, "id": "mu"},   {"pop": 1.0, "id": "beta"}],
        "adjacency": [[{"id": "alpha"}], [{"id": "zeta"}, {"id": "mu"}],
                      [{"id": "alpha"}, {"id": "beta"}], [{"id": "mu"}]]
    })
    .to_string();

    let adjacency = AdjacencyGraph::from_json(&json).unwrap();
    assert_eq!(adjacency.label(NodeId(0)).to_string(), "zeta");

    // The same path, positionally: zeta - alpha - mu - beta.
    let graph = Graph::from_edges(4, [(0, 1), (1, 2), (2, 3)]).unwrap();
    let populations = vec![3.0, 1.0, 3.0, 1.0];
    assert_eq!(adjacency.numeric_node_attr("pop").unwrap(), populations);

    let opts = options(2, 0.0, 4, SamplerChoice::Uniform);
    let via_file = seed_map(&adjacency, &opts).unwrap();
    let direct = seed_partition(&graph, &populations, &opts.params()).unwrap();
    assert_eq!(via_file, direct);
    assert_eq!(via_file.populations, vec![4.0, 4.0]);
}

#[test]
fn params_carries_everything_that_decides_the_answer_and_nothing_else() {
    let grid = Grid::new(4, 4, vec![1.0; 16]);
    let opts = options(4, 0.0, 11, SamplerChoice::MinWeightTree);
    let params = opts.params();

    assert_eq!(params.parts, opts.parts);
    assert_eq!(params.epsilon, opts.epsilon);
    assert_eq!(params.seed, opts.seed);
    assert_eq!(params.sampler, opts.sampler);

    // Renaming the file columns cannot change the partition, which is the
    // point of them not being in BalanceParams.
    let renamed = SeedMapOptions {
        population_attr: "pop".into(),
        assignment_attr: "something_else".into(),
        ..opts.clone()
    };
    let a = partition_balanced(&grid.graph(), &grid.populations, &opts.params()).unwrap();
    let b = partition_balanced(&grid.graph(), &grid.populations, &renamed.params()).unwrap();
    assert_eq!(a.partition, b.partition);
}

#[test]
fn a_direct_caller_gets_the_same_errors() {
    let grid = Grid::new(3, 3, vec![1.0; 9]);
    let graph = grid.graph();

    let mut opts = options(5, 0.0, 1, SamplerChoice::Uniform);
    opts.search.max_attempts = 10;
    assert!(matches!(
        seed_partition(&graph, &grid.populations, &opts.params()),
        Err(partigraph_io::Error::Partitioning(
            partigraph::Error::Exhausted { .. }
        ))
    ));

    assert!(matches!(
        partition_balanced(
            &graph,
            &grid.populations,
            &BalanceParams {
                parts: 0,
                ..Default::default()
            }
        ),
        Err(partigraph::Error::NoParts)
    ));
    assert!(matches!(
        partition_balanced(
            &graph,
            &grid.populations,
            &BalanceParams {
                epsilon: f64::NAN,
                ..Default::default()
            }
        ),
        Err(partigraph::Error::BadEpsilon(_))
    ));
    // The io wrapper keeps reporting its own variant for this.
    assert!(matches!(
        seed_partition(
            &graph,
            &grid.populations,
            &BalanceParams {
                epsilon: -1.0,
                ..Default::default()
            }
        ),
        Err(partigraph_io::Error::BadEpsilon(_))
    ));
}
