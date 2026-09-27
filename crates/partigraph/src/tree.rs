//! Spanning trees as parent arrays, and aggregation over their edges.

use crate::graph::{Induced, NodeId};

/// Which side of a cut tree edge is the one being named.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    /// The subtree hanging below the child endpoint.
    Subtree,
    /// Everything else: the component containing the root.
    Complement,
}

/// A directed edge of a tree, i.e. a way of splitting it in two.
///
/// A tree on `k` nodes has `k - 1` edges and `k - 1` non-root nodes, and the map
/// between them is a bijection: every edge is `{parent(child), child}`. Naming an
/// edge by its child and a [`Side`] gives the same `2(k - 1)` directed edges the
/// Python keys its dictionaries by, without hashing a node pair.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cut {
    pub child: u32,
    pub side: Side,
}

/// A spanning tree of an [`Induced`] subgraph, in local indices.
#[derive(Clone, Debug)]
pub struct Tree {
    nodes: Vec<NodeId>,
    parent: Vec<u32>,
    preorder: Vec<u32>,
    pos: Vec<u32>,
    subtree_size: Vec<u32>,
    child_offsets: Vec<u32>,
    child_list: Vec<u32>,
    root: u32,
}

impl Tree {
    /// Build from a parent array in local indices. The root must be its own parent.
    pub(crate) fn from_parents(nodes: Vec<NodeId>, parent: Vec<u32>, root: u32) -> Tree {
        let k = parent.len();
        debug_assert_eq!(nodes.len(), k);
        debug_assert_eq!(parent[root as usize], root);

        let mut child_offsets = vec![0u32; k + 1];
        for (v, &p) in parent.iter().enumerate() {
            if v as u32 != root {
                child_offsets[p as usize + 1] += 1;
            }
        }
        for i in 0..k {
            child_offsets[i + 1] += child_offsets[i];
        }
        let mut cursor = child_offsets.clone();
        let mut child_list = vec![0u32; k.saturating_sub(1)];
        for (v, &p) in parent.iter().enumerate() {
            if v as u32 != root {
                child_list[cursor[p as usize] as usize] = v as u32;
                cursor[p as usize] += 1;
            }
        }

        // Depth-first preorder. Pushing all of a node's children at once still
        // finishes one child's subtree before starting the next, so every
        // subtree is a contiguous run.
        let mut preorder = Vec::with_capacity(k);
        let mut pos = vec![0u32; k];
        let mut stack = vec![root];
        while let Some(v) = stack.pop() {
            pos[v as usize] = preorder.len() as u32;
            preorder.push(v);
            let (s, e) = (
                child_offsets[v as usize] as usize,
                child_offsets[v as usize + 1] as usize,
            );
            stack.extend_from_slice(&child_list[s..e]);
        }
        debug_assert_eq!(preorder.len(), k, "parent array is not a tree");

        // Reverse preorder visits every descendant before its parent.
        let mut subtree_size = vec![1u32; k];
        for &v in preorder.iter().rev() {
            if v != root {
                subtree_size[parent[v as usize] as usize] += subtree_size[v as usize];
            }
        }

        Tree {
            nodes,
            parent,
            preorder,
            pos,
            subtree_size,
            child_offsets,
            child_list,
            root,
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    #[inline]
    pub fn root(&self) -> u32 {
        self.root
    }

    /// Local index -> the node it stands for in the parent graph.
    #[inline]
    pub fn global(&self, local: u32) -> NodeId {
        self.nodes[local as usize]
    }

    #[inline]
    pub fn parent(&self, v: u32) -> Option<u32> {
        if v == self.root {
            None
        } else {
            Some(self.parent[v as usize])
        }
    }

    #[inline]
    pub fn children(&self, v: u32) -> &[u32] {
        let (s, e) = (
            self.child_offsets[v as usize] as usize,
            self.child_offsets[v as usize + 1] as usize,
        );
        &self.child_list[s..e]
    }

    #[inline]
    pub fn preorder(&self) -> &[u32] {
        &self.preorder
    }

    /// The edges, each named by its child endpoint.
    pub fn edges(&self) -> impl Iterator<Item = u32> + '_ {
        let root = self.root;
        (0..self.len() as u32).filter(move |&v| v != root)
    }

    /// Every way of splitting the tree: both directions of every edge.
    pub fn cuts(&self) -> impl Iterator<Item = Cut> + '_ {
        self.edges().flat_map(|child| {
            [
                Cut {
                    child,
                    side: Side::Subtree,
                },
                Cut {
                    child,
                    side: Side::Complement,
                },
            ]
        })
    }

    /// How many nodes are on the named side of `cut`.
    #[inline]
    pub fn cut_size(&self, cut: Cut) -> usize {
        let sub = self.subtree_size[cut.child as usize] as usize;
        match cut.side {
            Side::Subtree => sub,
            Side::Complement => self.len() - sub,
        }
    }

    /// The nodes on the named side of `cut`, as local indices.
    ///
    /// Returned as up to two slices of the preorder array: a subtree is one
    /// contiguous run, and its complement is the run before it plus the run
    /// after. Nothing is traversed or allocated.
    pub fn cut_locals(&self, cut: Cut) -> (&[u32], &[u32]) {
        let i = self.pos[cut.child as usize] as usize;
        let n = self.subtree_size[cut.child as usize] as usize;
        match cut.side {
            Side::Subtree => (&self.preorder[i..i + n], &[]),
            Side::Complement => (&self.preorder[..i], &self.preorder[i + n..]),
        }
    }

    /// The nodes on the named side of `cut`, as nodes of the parent graph.
    pub fn cut_nodes(&self, cut: Cut) -> impl Iterator<Item = NodeId> + '_ {
        let (a, b) = self.cut_locals(cut);
        a.iter().chain(b).map(|&l| self.nodes[l as usize])
    }

    /// Collect the parent-graph node indices on the named side of `cut`.
    pub fn cut_globals(&self, cut: Cut, out: &mut Vec<u32>) {
        out.clear();
        let (a, b) = self.cut_locals(cut);
        out.reserve(a.len() + b.len());
        for &l in a.iter().chain(b) {
            out.push(self.nodes[l as usize].0);
        }
    }

    /// Rebuild the induced subgraph this tree spans, for tests and callers that
    /// want to re-cut it.
    pub fn spans(&self, ind: &Induced) -> bool {
        self.len() == ind.len()
            && (0..self.len() as u32).all(|v| ind.local(self.global(v)) == Some(v))
    }
}

/// Aggregated values for both directions of every tree edge.
///
/// Indexed by child endpoint; see [`Cut`]. The entry for the root is the fold
/// over the whole tree (`down`) and the identity (`up`), neither of which names
/// an edge.
#[derive(Clone, Debug)]
pub struct EdgeValues<V> {
    down: Vec<V>,
    up: Vec<V>,
}

impl<V> EdgeValues<V> {
    /// The fold over the subtree at `child`.
    #[inline]
    pub fn down(&self, child: u32) -> &V {
        &self.down[child as usize]
    }

    /// The fold over everything outside the subtree at `child`.
    #[inline]
    pub fn up(&self, child: u32) -> &V {
        &self.up[child as usize]
    }

    /// The fold over the side of `cut` that it names.
    #[inline]
    pub fn get(&self, cut: Cut) -> &V {
        match cut.side {
            Side::Subtree => self.down(cut.child),
            Side::Complement => self.up(cut.child),
        }
    }

    /// The fold over the whole tree.
    #[inline]
    pub fn total(&self, tree: &Tree) -> &V {
        &self.down[tree.root() as usize]
    }
}

/// For every directed tree edge, fold the values of all nodes on its far side.
///
/// Two linear passes: subtree folds bottom-up, then complements top-down using a
/// prefix/suffix scan over each node's children to get "all siblings but this
/// one" in time proportional to the node's degree.
///
/// `combine` must be associative and commutative, with `identity` its unit. The
/// Python requires the same, since it folds neighbours in arbitrary dict order.
pub fn aggregate<V: Clone>(
    tree: &Tree,
    get: impl Fn(u32) -> V,
    identity: V,
    combine: impl Fn(&V, &V) -> V,
) -> EdgeValues<V> {
    let k = tree.len();
    let own: Vec<V> = (0..k as u32).map(&get).collect();

    let mut down = own.clone();
    for &v in tree.preorder.iter().rev() {
        if v != tree.root {
            let p = tree.parent[v as usize] as usize;
            let combined = combine(&down[p], &down[v as usize]);
            down[p] = combined;
        }
    }

    let mut up = vec![identity.clone(); k];
    let mut prefix: Vec<V> = Vec::new();
    let mut suffix: Vec<V> = Vec::new();
    // Preorder guarantees `up[p]` is already final when `p` is reached.
    for idx in 0..k {
        let p = tree.preorder[idx];
        let ch = tree.children(p);
        if ch.is_empty() {
            continue;
        }
        let base = combine(&own[p as usize], &up[p as usize]);

        let m = ch.len();
        prefix.clear();
        prefix.push(identity.clone());
        for i in 0..m {
            let next = combine(&prefix[i], &down[ch[i] as usize]);
            prefix.push(next);
        }
        suffix.clear();
        suffix.resize(m + 1, identity.clone());
        for i in (0..m).rev() {
            suffix[i] = combine(&suffix[i + 1], &down[ch[i] as usize]);
        }
        for i in 0..m {
            let siblings = combine(&prefix[i], &suffix[i + 1]);
            up[ch[i] as usize] = combine(&base, &siblings);
        }
    }

    EdgeValues { down, up }
}

/// Sum a value over each side of every tree edge.
pub fn sums(tree: &Tree, get: impl Fn(u32) -> f64) -> EdgeValues<f64> {
    aggregate(tree, get, 0.0, |a, b| a + b)
}

/// Whether any node on each side of every tree edge satisfies a predicate.
pub fn any(tree: &Tree, get: impl Fn(u32) -> bool) -> EdgeValues<bool> {
    aggregate(tree, get, false, |a, b| *a || *b)
}

impl Cut {
    /// The same tree edge, naming the other side.
    #[inline]
    pub fn flip(self) -> Cut {
        Cut {
            child: self.child,
            side: match self.side {
                Side::Subtree => Side::Complement,
                Side::Complement => Side::Subtree,
            },
        }
    }
}
