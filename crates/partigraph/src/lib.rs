//! Graph partitioning into weight-balanced parts using uniform random
//! spanning trees.
//!
//! A Rust port of [partigraph](https://github.com/proebsting/partigraph).
//!
//! The approach is divide and conquer. Draw a uniform random spanning tree of
//! the graph, cut the edge that comes closest to splitting the weight in the
//! wanted proportion, improve that split by transferring subtrees of fresh
//! random trees across the border, then recurse into each side.
//!
//! ```
//! use partigraph::{Graph, Partitioner, SearchOptions, within_epsilon};
//! use rand::{rngs::StdRng, SeedableRng};
//!
//! // A 4x4 grid, one unit of weight per node.
//! let mut edges = Vec::new();
//! for r in 0..4u32 {
//!     for c in 0..4u32 {
//!         let v = r * 4 + c;
//!         if c + 1 < 4 { edges.push((v, v + 1)); }
//!         if r + 1 < 4 { edges.push((v, v + 4)); }
//!     }
//! }
//! let graph = Graph::from_edges(16, edges)?;
//! let weights = vec![1.0; 16];
//!
//! let mut rng = StdRng::seed_from_u64(20260926);
//! let partition = Partitioner::new(&graph, &weights)?
//!     .search(4, within_epsilon(4.0, 0.0), &SearchOptions::default(), &mut rng)?;
//!
//! assert_eq!(partition.part_weights(&weights), vec![4.0; 4]);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub mod graph;
pub mod mst;
pub mod partition;
pub mod sampler;
pub mod tree;
pub mod ust;

pub use graph::{Graph, GraphError, Induced, NodeId, NodeSet};
pub use mst::min_weight_spanning_tree;
pub use partition::{
    partition_balanced, within_epsilon, BalanceParams, BalancedPartition, Division, Partition,
    Partitioner, SearchOptions,
};
pub use sampler::{EdgeWeight, MinWeightTree, RandomWeights, SamplerChoice, TreeSampler, Uniform};
pub use tree::{aggregate, any, sums, Cut, EdgeValues, Side, Tree};
pub use ust::random_spanning_tree;

/// What can go wrong while partitioning.
#[derive(Clone, PartialEq, Debug)]
pub enum Error {
    /// The graph has no nodes.
    EmptyGraph,
    /// A spanning tree was asked for over no nodes.
    EmptySubset,
    /// The graph, or a subgraph of it, is not connected. Wilson's walk does not
    /// terminate on a disconnected input, so this is refused rather than hung on.
    Disconnected,
    /// `weights` did not have one entry per node.
    WeightCountMismatch { nodes: usize, weights: usize },
    /// A weight was negative, infinite, or NaN.
    BadWeight { node: NodeId, weight: f64 },
    /// Zero parts were asked for.
    NoParts,
    /// A tolerance was negative, infinite, or NaN.
    BadEpsilon(f64),
    /// More parts than nodes were asked for.
    TooFewNodes { nodes: usize, parts: usize },
    /// A single node cannot be divided in two.
    Indivisible,
    /// A transfer was asked for between two sets that do not touch.
    NotAdjacent,
    /// No valid partition was found within `SearchOptions::max_attempts`.
    Exhausted { attempts: usize },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::EmptyGraph => write!(f, "the graph has no nodes"),
            Error::EmptySubset => write!(f, "cannot build a spanning tree over no nodes"),
            Error::Disconnected => write!(f, "the graph is not connected"),
            Error::WeightCountMismatch { nodes, weights } => {
                write!(f, "{weights} weights for {nodes} nodes")
            }
            Error::BadWeight { node, weight } => {
                write!(
                    f,
                    "node {} has weight {weight}, which is not finite and non-negative",
                    node.0
                )
            }
            Error::NoParts => write!(f, "at least one part is required"),
            Error::BadEpsilon(e) => {
                write!(f, "the tolerance must be finite and non-negative, got {e}")
            }
            Error::TooFewNodes { nodes, parts } => {
                write!(f, "cannot split {nodes} nodes into {parts} parts")
            }
            Error::Indivisible => write!(f, "a single node cannot be divided"),
            Error::NotAdjacent => write!(f, "the two partitions do not share a border"),
            Error::Exhausted { attempts } => {
                write!(f, "no valid partition found in {attempts} attempts")
            }
        }
    }
}

impl std::error::Error for Error {}

impl From<GraphError> for Error {
    fn from(e: GraphError) -> Error {
        match e {
            GraphError::NodeOutOfRange { .. } | GraphError::TooManyNodes { .. } => {
                Error::EmptyGraph
            }
        }
    }
}
