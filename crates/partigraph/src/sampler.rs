//! Choosing a spanning tree at random.
//!
//! The partitioner draws a fresh spanning tree for every division and every
//! transfer, and which trees it tends to draw is what shapes the parts it
//! produces. Two samplers ship:
//!
//! - [`Uniform`], Wilson's algorithm, which draws uniformly from all spanning
//!   trees of the subgraph. This is what partigraph does, and the default here.
//! - [`MinWeightTree`], which costs every edge and takes the minimum spanning
//!   tree. With independent random costs it is cheaper than Wilson's, but it is
//!   *not* uniform: on K4 it over-produces stars. Its cost function is where a
//!   caller says which edges a tree should prefer to avoid.

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
/// // Independent random costs.
/// let plain = MinWeightTree::random();
///
/// // Or anything a cost per edge can express. Given any labelling of the
/// // nodes, charging extra for the edges that run between labels yields trees
/// // that prefer to stay inside one label, and so cuts that tend to fall on
/// // the boundaries.
/// let label: Vec<u32> = vec![0, 0, 1, 1];
/// let respect_labels = MinWeightTree::new(
///     move |a: NodeId, b: NodeId, rng: &mut dyn RngCore| {
///         let crosses = label[a.index()] != label[b.index()];
///         rng.gen::<f64>() + if crosses { 1.0 } else { 0.0 }
///     },
/// );
/// # let _ = (plain, respect_labels);
/// ```
#[derive(Clone, Copy, Debug, Default)]
pub struct MinWeightTree<W> {
    pub weights: W,
}

impl MinWeightTree<RandomWeights> {
    /// Independent random costs in `[0, 1)`, which is the cheapest way to get
    /// a varied tree.
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
