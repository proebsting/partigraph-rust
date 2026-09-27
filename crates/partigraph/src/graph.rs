//! Graphs, node sets, and induced subgraphs.

/// Index of a node in a [`Graph`].
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct NodeId(pub u32);

impl NodeId {
    #[inline]
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// Sentinel in a global-to-local table for "not in this subset".
pub(crate) const NOT_MEMBER: u32 = u32::MAX;

/// Something wrong with a graph's construction.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum GraphError {
    /// An edge named a node at or beyond `node_count`.
    NodeOutOfRange { node: u32, node_count: usize },
    /// More nodes than a `u32` index can address.
    TooManyNodes { node_count: usize },
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GraphError::NodeOutOfRange { node, node_count } => {
                write!(
                    f,
                    "edge names node {node}, but the graph has {node_count} nodes"
                )
            }
            GraphError::TooManyNodes { node_count } => {
                write!(f, "{node_count} nodes exceeds the u32 index space")
            }
        }
    }
}

impl std::error::Error for GraphError {}

/// An undirected simple graph, stored in compressed sparse row form.
///
/// Nodes are `0..node_count`. Each undirected edge appears once in each
/// endpoint's adjacency list, so `neighbors` has length `2 * edge_count`.
#[derive(Clone, Debug)]
pub struct Graph {
    offsets: Vec<u32>,
    neighbors: Vec<u32>,
}

impl Graph {
    /// Build a graph on `node_count` nodes from an edge list.
    ///
    /// Parallel edges are collapsed and self-loops are dropped: neither carries
    /// adjacency information, and a self-loop would put a node in its own
    /// neighbour list, which breaks the spanning-tree invariants downstream.
    pub fn from_edges(
        node_count: usize,
        edges: impl IntoIterator<Item = (u32, u32)>,
    ) -> Result<Graph, GraphError> {
        let limit: u32 = node_count
            .try_into()
            .map_err(|_| GraphError::TooManyNodes { node_count })?;

        let mut ends: Vec<(u32, u32)> = Vec::new();
        for (a, b) in edges {
            for v in [a, b] {
                if v >= limit {
                    return Err(GraphError::NodeOutOfRange {
                        node: v,
                        node_count,
                    });
                }
            }
            if a == b {
                continue;
            }
            ends.push((a, b));
            ends.push((b, a));
        }

        // Counting sort the half-edges by source.
        let mut offsets = vec![0u32; node_count + 1];
        for &(a, _) in &ends {
            offsets[a as usize + 1] += 1;
        }
        for i in 0..node_count {
            offsets[i + 1] += offsets[i];
        }
        let mut cursor = offsets.clone();
        let mut neighbors = vec![0u32; ends.len()];
        for &(a, b) in &ends {
            let slot = &mut cursor[a as usize];
            neighbors[*slot as usize] = b;
            *slot += 1;
        }

        // Sort and dedupe each adjacency list, compacting toward the front.
        // The write cursor never passes the read cursor, since after finishing
        // node v we have written at most `offsets[v + 1]` entries.
        let mut write = 0usize;
        let mut compact = vec![0u32; node_count + 1];
        for v in 0..node_count {
            let (start, end) = (offsets[v] as usize, offsets[v + 1] as usize);
            neighbors[start..end].sort_unstable();
            let mut last: Option<u32> = None;
            for i in start..end {
                let x = neighbors[i];
                if last != Some(x) {
                    neighbors[write] = x;
                    write += 1;
                    last = Some(x);
                }
            }
            compact[v + 1] = write as u32;
        }
        neighbors.truncate(write);

        Ok(Graph {
            offsets: compact,
            neighbors,
        })
    }

    #[inline]
    pub fn node_count(&self) -> usize {
        self.offsets.len() - 1
    }

    #[inline]
    pub fn edge_count(&self) -> usize {
        self.neighbors.len() / 2
    }

    #[inline]
    pub fn neighbors(&self, v: NodeId) -> &[u32] {
        let (s, e) = (
            self.offsets[v.index()] as usize,
            self.offsets[v.index() + 1] as usize,
        );
        &self.neighbors[s..e]
    }

    #[inline]
    pub fn degree(&self, v: NodeId) -> usize {
        self.neighbors(v).len()
    }

    pub fn nodes(&self) -> impl Iterator<Item = NodeId> {
        (0..self.node_count() as u32).map(NodeId)
    }

    /// Number of connected components. An empty graph has none.
    pub fn component_count(&self) -> usize {
        let n = self.node_count();
        let mut seen = vec![false; n];
        let mut stack = Vec::new();
        let mut components = 0;
        for start in 0..n {
            if seen[start] {
                continue;
            }
            components += 1;
            seen[start] = true;
            stack.push(start as u32);
            while let Some(v) = stack.pop() {
                for &w in self.neighbors(NodeId(v)) {
                    if !seen[w as usize] {
                        seen[w as usize] = true;
                        stack.push(w);
                    }
                }
            }
        }
        components
    }

    pub fn is_connected(&self) -> bool {
        self.component_count() == 1
    }
}

/// A subset of a graph's nodes, held as both a bitset and a member list.
///
/// The bitset answers membership in O(1) and the member list gives a compact
/// iteration order; the partitioner needs both on every move.
#[derive(Clone, Debug)]
pub struct NodeSet {
    bits: Vec<u64>,
    members: Vec<u32>,
    universe: usize,
}

impl NodeSet {
    pub fn empty(universe: usize) -> NodeSet {
        NodeSet {
            bits: vec![0; universe.div_ceil(64)],
            members: Vec::new(),
            universe,
        }
    }

    /// Every node of `graph`.
    pub fn all(graph: &Graph) -> NodeSet {
        let mut s = NodeSet::empty(graph.node_count());
        for v in 0..graph.node_count() as u32 {
            s.insert(NodeId(v));
        }
        s
    }

    pub fn from_nodes(universe: usize, nodes: impl IntoIterator<Item = NodeId>) -> NodeSet {
        let mut s = NodeSet::empty(universe);
        for v in nodes {
            s.insert(v);
        }
        s
    }

    #[inline]
    pub fn contains(&self, v: NodeId) -> bool {
        let i = v.index();
        i < self.universe && self.bits[i / 64] >> (i % 64) & 1 == 1
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.members.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    #[inline]
    pub fn universe(&self) -> usize {
        self.universe
    }

    /// The members, in insertion order. Each appears exactly once.
    #[inline]
    pub fn members(&self) -> &[u32] {
        &self.members
    }

    pub fn iter(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.members.iter().copied().map(NodeId)
    }

    /// Returns whether the node was newly added.
    pub fn insert(&mut self, v: NodeId) -> bool {
        let i = v.index();
        let (word, bit) = (i / 64, 1u64 << (i % 64));
        if self.bits[word] & bit != 0 {
            return false;
        }
        self.bits[word] |= bit;
        self.members.push(v.0);
        true
    }

    /// Add every node in `nodes` (given as raw indices).
    pub fn insert_all(&mut self, nodes: &[u32]) {
        for &v in nodes {
            self.insert(NodeId(v));
        }
    }

    /// Remove every node in `nodes` (given as raw indices). O(len).
    pub fn remove_all(&mut self, nodes: &[u32]) {
        let mut removed = 0;
        for &v in nodes {
            let (word, bit) = (v as usize / 64, 1u64 << (v as usize % 64));
            if self.bits[word] & bit != 0 {
                self.bits[word] &= !bit;
                removed += 1;
            }
        }
        if removed > 0 {
            let bits = &self.bits;
            self.members
                .retain(|&v| bits[v as usize / 64] >> (v as usize % 64) & 1 == 1);
        }
    }
}

/// Membership, not insertion order: two sets holding the same nodes are equal
/// however they were built.
impl PartialEq for NodeSet {
    fn eq(&self, other: &NodeSet) -> bool {
        self.universe == other.universe && self.bits == other.bits
    }
}

impl Eq for NodeSet {}

/// The subgraph of `graph` induced by a node subset, renumbered to `0..len`.
///
/// Built once per spanning tree so that Wilson's walk can index a neighbour list
/// directly rather than filtering the parent graph's at every step.
#[derive(Clone, Debug)]
pub struct Induced {
    members: Vec<u32>,
    local_of: Vec<u32>,
    offsets: Vec<u32>,
    neighbors: Vec<u32>,
}

impl Induced {
    pub fn new(graph: &Graph, subset: &NodeSet) -> Induced {
        let members: Vec<u32> = subset.members().to_vec();
        let mut local_of = vec![NOT_MEMBER; graph.node_count()];
        for (i, &g) in members.iter().enumerate() {
            local_of[g as usize] = i as u32;
        }

        let k = members.len();
        let mut offsets = vec![0u32; k + 1];
        for (i, &g) in members.iter().enumerate() {
            let d = graph
                .neighbors(NodeId(g))
                .iter()
                .filter(|&&x| local_of[x as usize] != NOT_MEMBER)
                .count();
            offsets[i + 1] = d as u32;
        }
        for i in 0..k {
            offsets[i + 1] += offsets[i];
        }

        let mut neighbors = vec![0u32; offsets[k] as usize];
        for (i, &g) in members.iter().enumerate() {
            let mut w = offsets[i] as usize;
            for &x in graph.neighbors(NodeId(g)) {
                let l = local_of[x as usize];
                if l != NOT_MEMBER {
                    neighbors[w] = l;
                    w += 1;
                }
            }
        }

        Induced {
            members,
            local_of,
            offsets,
            neighbors,
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.members.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    /// Local index -> the node it stands for in the parent graph.
    #[inline]
    pub fn global(&self, local: u32) -> NodeId {
        NodeId(self.members[local as usize])
    }

    /// Parent-graph node -> its local index, or `None` if outside the subset.
    #[inline]
    pub fn local(&self, v: NodeId) -> Option<u32> {
        match self.local_of[v.index()] {
            NOT_MEMBER => None,
            l => Some(l),
        }
    }

    #[inline]
    pub fn neighbors(&self, local: u32) -> &[u32] {
        let (s, e) = (
            self.offsets[local as usize] as usize,
            self.offsets[local as usize + 1] as usize,
        );
        &self.neighbors[s..e]
    }

    /// Whether the induced subgraph is connected. Wilson's walk does not
    /// terminate on a disconnected input, so callers check this first.
    pub fn is_connected(&self) -> bool {
        let k = self.len();
        if k == 0 {
            return false;
        }
        let mut seen = vec![false; k];
        let mut stack = vec![0u32];
        seen[0] = true;
        let mut count = 1;
        while let Some(v) = stack.pop() {
            for &w in self.neighbors(v) {
                if !seen[w as usize] {
                    seen[w as usize] = true;
                    count += 1;
                    stack.push(w);
                }
            }
        }
        count == k
    }
}
