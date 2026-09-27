//! Building a seed map: a starting partition, carried on the graph itself.
//!
//! A *seed map* is the input graph written back out with one extra node
//! attribute holding each node's part. GerryChain consumes it directly, because
//! `Partition`'s `assignment` argument accepts the name of a node attribute:
//!
//! ```python
//! graph = Graph.from_json("seed.json")
//! partition = Partition(graph, "district", updaters)
//! ```
//!
//! This module is where the conventions live — population as a node attribute,
//! a relative tolerance on part population, parts numbered from zero. The
//! [`crate::adjacency`] module below it knows none of that, and neither does
//! the `partigraph` core.

use partigraph::{
    within_epsilon, MinWeightTree, NodeId, Partition, Partitioner, SearchOptions, Uniform,
};
use rand::{rngs::StdRng, SeedableRng};
use serde_json::Value;

use crate::{AdjacencyGraph, Error};

/// Which spanning-tree sampler to partition with.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Sampler {
    /// Wilson's algorithm: uniform over spanning trees.
    #[default]
    Uniform,
    /// Random-weight minimum spanning tree. Faster, not uniform. This is what
    /// GerryChain's `random_spanning_tree` does.
    MinWeightTree,
}

/// What to ask of a seed map.
#[derive(Clone, Debug)]
pub struct SeedMapOptions {
    /// How many parts to divide the graph into.
    pub parts: usize,
    /// Node attribute holding each node's population.
    pub population_attr: String,
    /// Allowed relative deviation from the ideal part population, so a part is
    /// acceptable when `|pop - ideal| <= epsilon * ideal`. This is the same
    /// meaning GerryChain gives `epsilon`.
    pub epsilon: f64,
    /// Node attribute to write the resulting part into.
    pub assignment_attr: String,
    /// Seed for the random number generator, so a run reproduces.
    pub seed: u64,
    pub sampler: Sampler,
    pub search: SearchOptions,
}

impl Default for SeedMapOptions {
    fn default() -> Self {
        SeedMapOptions {
            parts: 2,
            population_attr: "TOTPOP".into(),
            epsilon: 0.01,
            assignment_attr: "district".into(),
            seed: 0,
            sampler: Sampler::default(),
            search: SearchOptions::default(),
        }
    }
}

/// A seed map, and what it came out to.
#[derive(Clone, Debug)]
pub struct SeedMap {
    pub partition: Partition,
    /// Total population of each part, indexed by part.
    pub populations: Vec<f64>,
    /// Total population divided by the number of parts.
    pub ideal: f64,
}

impl SeedMap {
    /// The largest relative deviation from ideal across the parts.
    pub fn worst_deviation(&self) -> f64 {
        if self.ideal == 0.0 {
            return 0.0;
        }
        self.populations
            .iter()
            .map(|p| (p - self.ideal).abs() / self.ideal)
            .fold(0.0, f64::max)
    }

    /// Node counts per part, indexed by part.
    pub fn sizes(&self) -> Vec<usize> {
        (0..self.partition.part_count())
            .map(|p| self.partition.part(p as u32).count())
            .collect()
    }
}

/// Partition `graph` into population-balanced parts.
///
/// Reads the population attribute, runs the search, and reports what it found.
/// The graph is not modified; use [`write_assignment`] to put the result onto
/// it, or take `seed.partition` and do something else with it.
pub fn seed_map(graph: &AdjacencyGraph, opts: &SeedMapOptions) -> Result<SeedMap, Error> {
    if opts.parts == 0 {
        return Err(Error::Partitioning(partigraph::Error::NoParts));
    }
    if !(opts.epsilon.is_finite() && opts.epsilon >= 0.0) {
        return Err(Error::BadEpsilon(opts.epsilon));
    }

    // An island makes the graph disconnected, which the partitioner refuses.
    // Say so in the terms the file is written in, rather than as a bare
    // "not connected".
    let islands = graph.islands();
    if !islands.is_empty() {
        return Err(Error::Islands {
            count: islands.len(),
            examples: islands
                .iter()
                .take(5)
                .map(|l| l.to_string())
                .collect::<Vec<_>>()
                .join(", "),
        });
    }

    let core = graph.to_graph()?;
    let populations = graph.numeric_node_attr(&opts.population_attr)?;
    let total: f64 = populations.iter().sum();
    let ideal = total / opts.parts as f64;

    let partitioner = Partitioner::new(&core, &populations)?;
    let mut rng = StdRng::seed_from_u64(opts.seed);
    let valid = within_epsilon(ideal, opts.epsilon);

    let partition = match opts.sampler {
        Sampler::Uniform => {
            partitioner
                .with_sampler(Uniform)
                .search(opts.parts, valid, &opts.search, &mut rng)
        }
        Sampler::MinWeightTree => partitioner.with_sampler(MinWeightTree::random()).search(
            opts.parts,
            valid,
            &opts.search,
            &mut rng,
        ),
    }?;

    let part_populations = partition.part_weights(&populations);
    Ok(SeedMap {
        partition,
        populations: part_populations,
        ideal,
    })
}

/// Write a partition onto a graph as a node attribute, ready to be saved.
///
/// Parts are written as integers numbered from zero, which is what GerryChain's
/// own `Partition.from_random_assignment` produces, so the labels line up with
/// what a GerryChain user expects.
pub fn write_assignment(graph: &mut AdjacencyGraph, attr: &str, partition: &Partition) {
    let values = (0..graph.node_count()).map(|i| Value::from(partition.part_of(NodeId(i as u32))));
    graph.set_node_attr(attr, values);
}

/// Read a graph, partition it, and return the graph with the assignment on it.
///
/// The one-call version, for when nothing in between is wanted.
pub fn build_seed_map(
    graph: &AdjacencyGraph,
    opts: &SeedMapOptions,
) -> Result<(AdjacencyGraph, SeedMap), Error> {
    let seed = seed_map(graph, opts)?;
    let mut out = graph.clone();
    write_assignment(&mut out, &opts.assignment_attr, &seed.partition);
    Ok((out, seed))
}
