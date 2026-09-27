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
//!
//! # Getting in without a file
//!
//! The work splits at the point where a file stops being involved. A caller
//! that already holds a graph and a weight per node does not need this module
//! at all, and should call [`partigraph::partition_balanced`] directly — it
//! takes the same parameters, does the same search, and costs no dependency on
//! JSON. [`seed_partition`] is the same thing returning a [`SeedMap`], for
//! callers that want to go on to write a file.

use partigraph::{
    partition_balanced, BalanceParams, BalancedPartition, Graph, NodeId, Partition, SearchOptions,
};
use serde_json::Value;

use crate::{AdjacencyGraph, Error};

pub use partigraph::SamplerChoice as Sampler;

/// What to ask of a seed map.
///
/// Two kinds of thing, deliberately: `parts`, `epsilon`, `seed`, `sampler` and
/// `search` say what to compute, and [`params`](SeedMapOptions::params) hands
/// exactly those to the core. `population_attr` and `assignment_attr` say only
/// what the columns are called in a file, and are of no interest to anyone who
/// does not have one.
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

impl SeedMapOptions {
    /// The subset that says what to compute, with the attribute names dropped.
    pub fn params(&self) -> BalanceParams {
        BalanceParams {
            parts: self.parts,
            epsilon: self.epsilon,
            seed: self.seed,
            sampler: self.sampler,
            search: self.search.clone(),
        }
    }
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
///
/// The same data as [`BalancedPartition`], named for what it means here: these
/// weights are populations. The core does not know that, which is why this is
/// its own type rather than a re-export.
#[derive(Clone, PartialEq, Debug)]
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
        self.as_balanced().worst_deviation()
    }

    /// Node counts per part, indexed by part.
    pub fn sizes(&self) -> Vec<usize> {
        self.as_balanced().sizes()
    }

    fn as_balanced(&self) -> BalancedPartition {
        BalancedPartition {
            partition: self.partition.clone(),
            part_weights: self.populations.clone(),
            ideal: self.ideal,
        }
    }
}

impl From<BalancedPartition> for SeedMap {
    fn from(b: BalancedPartition) -> SeedMap {
        SeedMap {
            partition: b.partition,
            populations: b.part_weights,
            ideal: b.ideal,
        }
    }
}

/// Partition a graph whose populations the caller already holds.
///
/// The half of [`seed_map`] that needs no file. Equivalent to
/// [`partigraph::partition_balanced`], returning a [`SeedMap`].
pub fn seed_partition(
    graph: &Graph,
    populations: &[f64],
    params: &BalanceParams,
) -> Result<SeedMap, Error> {
    Ok(partition_balanced(graph, populations, params)?.into())
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

    // Stays here rather than in the core, because it reports node labels and
    // the core has none. The core refuses a disconnected graph anyway, but
    // "node 37001000001 has no neighbours" beats "the graph is not connected".
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

    seed_partition(
        &graph.to_graph()?,
        &graph.numeric_node_attr(&opts.population_attr)?,
        &opts.params(),
    )
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
