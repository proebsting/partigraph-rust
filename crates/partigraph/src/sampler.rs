//! Choosing a spanning tree at random.
//!
//! The partitioner draws a fresh spanning tree for every division and every
//! transfer, and which trees it tends to draw is what shapes the districts it
//! produces. Two samplers ship:
//!
//! - [`Uniform`], Wilson's algorithm, which draws uniformly from all spanning
//!   trees of the subgraph. This is what partigraph does and the default here.
//! - [`MinWeightTree`], which weights each edge and takes the minimum spanning
//!   tree. With independent random weights this is what GerryChain does. It is
//!   *not* uniform — on K4 it over-produces stars — but it is cheap, and the
//!   weight function is a place to express preferences about which edges a tree
//!   should avoid.

use rand::{Rng, RngCore};

use crate::graph::{Induced, NodeId};
use crate::tree::Tree;
use crate::Error;

/// A way of drawing a spanning tree of an induced subgraph.
pub trait TreeSampler {
    fn sample(&self, subgraph: &Induced, rng: &mut dyn RngCore) -> Result<Tree, Error>;
}

/// Wilson's algorithm: uniform over the spanning trees of the subgraph.
#[derive(Clone, Copy, Debug, Default)]
pub struct Uniform;

impl TreeSampler for Uniform {
    fn sample(&self, subgraph: &Induced, rng: &mut dyn RngCore) -> Result<Tree, Error> {
        crate::ust::wilson(subgraph, rng)
    }
}

/// A cost for each edge of the graph, used to pick a minimum spanning tree.
///
/// Implemented for any `Fn(NodeId, NodeId, &mut dyn RngCore) -> f64`, so a
/// closure will do. The endpoints are nodes of the parent graph, not of the
/// induced subgraph, so a weight function can consult whatever the caller knows
/// about them.
pub trait EdgeWeight {
    fn weight(&self, a: NodeId, b: NodeId, rng: &mut dyn RngCore) -> f64;
}

impl<F: Fn(NodeId, NodeId, &mut dyn RngCore) -> f64> EdgeWeight for F {
    fn weight(&self, a: NodeId, b: NodeId, rng: &mut dyn RngCore) -> f64 {
        self(a, b, rng)
    }
}

/// Independent uniform weights in `[0, 1)`, ignoring the endpoints.
#[derive(Clone, Copy, Debug, Default)]
pub struct RandomWeights;

impl EdgeWeight for RandomWeights {
    fn weight(&self, _: NodeId, _: NodeId, rng: &mut dyn RngCore) -> f64 {
        rng.gen()
    }
}

/// Weight every edge, then take the minimum spanning tree.
///
/// ```
/// use partigraph::{MinWeightTree, NodeId};
/// use rand::{Rng, RngCore};
///
/// // GerryChain's sampler: independent random weights.
/// let plain = MinWeightTree::random();
///
/// // Or anything else. This one keeps nodes of the same county together, by
/// // making edges that cross a county line expensive and so unlikely to
/// // survive into the tree.
/// let county: Vec<u32> = vec![0, 0, 1, 1];
/// let whole_counties = MinWeightTree::new(
///     move |a: NodeId, b: NodeId, rng: &mut dyn RngCore| {
///         let crosses = county[a.index()] != county[b.index()];
///         rng.gen::<f64>() + if crosses { 1.0 } else { 0.0 }
///     },
/// );
/// # let _ = (plain, whole_counties);
/// ```
#[derive(Clone, Copy, Debug, Default)]
pub struct MinWeightTree<W> {
    pub weights: W,
}

impl MinWeightTree<RandomWeights> {
    /// Independent random weights: the sampler GerryChain uses.
    pub fn random() -> MinWeightTree<RandomWeights> {
        MinWeightTree {
            weights: RandomWeights,
        }
    }
}

impl<W: EdgeWeight> MinWeightTree<W> {
    pub fn new(weights: W) -> MinWeightTree<W> {
        MinWeightTree { weights }
    }
}

impl<W: EdgeWeight> TreeSampler for MinWeightTree<W> {
    fn sample(&self, subgraph: &Induced, rng: &mut dyn RngCore) -> Result<Tree, Error> {
        crate::mst::min_weight_spanning_tree(subgraph, &self.weights, rng)
    }
}

/// Lets `&S` stand in wherever a sampler is wanted.
impl<S: TreeSampler + ?Sized> TreeSampler for &S {
    fn sample(&self, subgraph: &Induced, rng: &mut dyn RngCore) -> Result<Tree, Error> {
        (**self).sample(subgraph, rng)
    }
}
