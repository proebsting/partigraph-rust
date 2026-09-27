//! Reading and writing graphs for [`partigraph`], in the networkx "adjacency"
//! JSON format that GerryChain uses.
//!
//! Two layers, deliberately separated:
//!
//! - [`adjacency`] is the file format and nothing else. It knows about nodes,
//!   edges and arbitrary JSON attributes.
//! - [`seedmap`] is where domain conventions live: population as a node
//!   attribute, a relative tolerance, parts numbered from zero.
//!
//! The `partigraph` core below both knows about neither.
//!
//! If you already hold a graph and a weight per node, you do not need this
//! crate: call [`partigraph::partition_balanced`], which does the same search
//! without going near JSON.
//!
//! ```no_run
//! use partigraph_io::{build_seed_map, AdjacencyGraph, SeedMapOptions};
//!
//! let graph = AdjacencyGraph::read("NC.json")?;
//! let opts = SeedMapOptions {
//!     parts: 13,
//!     population_attr: "TOTPOP".into(),
//!     epsilon: 0.01,
//!     assignment_attr: "district".into(),
//!     seed: 42,
//!     ..Default::default()
//! };
//!
//! let (seeded, report) = build_seed_map(&graph, &opts)?;
//! seeded.write("NC_seed.json")?;
//! println!("worst deviation {:.4}", report.worst_deviation());
//! # Ok::<(), partigraph_io::Error>(())
//! ```

pub mod adjacency;
pub mod seedmap;

pub use adjacency::{AdjacencyGraph, NodeLabel};
pub use seedmap::{
    build_seed_map, seed_map, seed_partition, write_assignment, Sampler, SeedMap, SeedMapOptions,
};

/// Re-exported so a caller can build [`SeedMapOptions::params`] without
/// naming the core crate.
pub use partigraph::BalanceParams;

use std::path::PathBuf;

/// What can go wrong reading, partitioning, or writing.
#[derive(Debug)]
pub enum Error {
    ReadingFile(PathBuf, std::io::Error),
    WritingFile(PathBuf, std::io::Error),
    Json(serde_json::Error),
    /// `directed` or `multigraph` was true.
    UnsupportedGraph(&'static str),
    /// `nodes` and `adjacency` are meant to be parallel.
    AdjacencyLengthMismatch {
        nodes: usize,
        adjacency: usize,
    },
    NodeWithoutId {
        position: usize,
    },
    NeighbourWithoutId {
        node: String,
    },
    /// networkx would invent a node here; we refuse.
    UnknownNeighbour {
        node: String,
        neighbour: String,
    },
    DuplicateNodeId {
        label: String,
        first: usize,
        second: usize,
    },
    MissingNodeAttr {
        attr: String,
        node: String,
        available: String,
    },
    NonNumericNodeAttr {
        attr: String,
        node: String,
        found: String,
    },
    /// Nodes with no neighbours, which make the graph disconnected.
    Islands {
        count: usize,
        examples: String,
    },
    BadEpsilon(f64),
    Partitioning(partigraph::Error),
    BuildingGraph(partigraph::GraphError),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::ReadingFile(p, e) => write!(f, "reading {}: {e}", p.display()),
            Error::WritingFile(p, e) => write!(f, "writing {}: {e}", p.display()),
            Error::Json(e) => write!(f, "not valid adjacency JSON: {e}"),
            Error::UnsupportedGraph(what) => {
                write!(f, "the graph is marked {what}, which is not supported")
            }
            Error::AdjacencyLengthMismatch { nodes, adjacency } => write!(
                f,
                "the file has {nodes} nodes but {adjacency} adjacency lists; \
                 they are matched by position and must be the same length"
            ),
            Error::NodeWithoutId { position } => {
                write!(f, "the node at position {position} has no \"id\"")
            }
            Error::NeighbourWithoutId { node } => {
                write!(f, "a neighbour of node {node} has no \"id\"")
            }
            Error::UnknownNeighbour { node, neighbour } => write!(
                f,
                "node {node} lists neighbour {neighbour}, which is not in \"nodes\" \
                 (networkx would silently invent it)"
            ),
            Error::DuplicateNodeId {
                label,
                first,
                second,
            } => write!(
                f,
                "node id {label} appears at positions {first} and {second}"
            ),
            Error::MissingNodeAttr {
                attr,
                node,
                available,
            } => write!(
                f,
                "node {node} has no attribute \"{attr}\"; the nodes carry: {available}"
            ),
            Error::NonNumericNodeAttr { attr, node, found } => write!(
                f,
                "node {node} has \"{attr}\" = {found}, which is not a number"
            ),
            Error::Islands { count, examples } => write!(
                f,
                "{count} node(s) have no neighbours, so the graph is not connected \
                 and cannot be partitioned (for example: {examples})"
            ),
            Error::BadEpsilon(e) => write!(f, "epsilon must be finite and non-negative, got {e}"),
            Error::Partitioning(e) => write!(f, "{e}"),
            Error::BuildingGraph(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Error {
        Error::Json(e)
    }
}

impl From<partigraph::Error> for Error {
    fn from(e: partigraph::Error) -> Error {
        match e {
            // This crate has its own variant for it, from before the check
            // moved into the core.
            partigraph::Error::BadEpsilon(v) => Error::BadEpsilon(v),
            other => Error::Partitioning(other),
        }
    }
}

impl From<partigraph::GraphError> for Error {
    fn from(e: partigraph::GraphError) -> Error {
        Error::BuildingGraph(e)
    }
}
