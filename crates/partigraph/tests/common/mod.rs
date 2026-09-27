//! Shared fixtures and brute-force reference implementations.

#![allow(dead_code)]

use partigraph::{Graph, NodeId, NodeSet};

/// An `rows` x `cols` grid graph, row-major.
pub fn grid(rows: u32, cols: u32) -> Graph {
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

/// The complete graph on `n` nodes.
pub fn complete(n: u32) -> Graph {
    let edges = (0..n).flat_map(|a| (a + 1..n).map(move |b| (a, b)));
    Graph::from_edges(n as usize, edges).unwrap()
}

/// The cycle on `n` nodes.
pub fn cycle(n: u32) -> Graph {
    Graph::from_edges(n as usize, (0..n).map(|a| (a, (a + 1) % n))).unwrap()
}

/// Is the subgraph induced by `nodes` connected? Independent flood fill, used to
/// check district contiguity without going through the library's own machinery.
pub fn is_contiguous(graph: &Graph, nodes: &[u32]) -> bool {
    if nodes.is_empty() {
        return false;
    }
    let set: std::collections::HashSet<u32> = nodes.iter().copied().collect();
    let mut seen = std::collections::HashSet::new();
    let mut stack = vec![nodes[0]];
    seen.insert(nodes[0]);
    while let Some(v) = stack.pop() {
        for &w in graph.neighbors(NodeId(v)) {
            if set.contains(&w) && seen.insert(w) {
                stack.push(w);
            }
        }
    }
    seen.len() == nodes.len()
}

pub fn all_nodes(graph: &Graph) -> NodeSet {
    NodeSet::all(graph)
}

/// Every spanning tree of `graph`, each as a sorted list of `(min, max)` edges.
///
/// Brute force over edge subsets of size `n - 1`; only for graphs small enough
/// that `2^m` is nothing.
pub fn all_spanning_trees(graph: &Graph) -> Vec<Vec<(u32, u32)>> {
    let n = graph.node_count();
    let mut edges = Vec::new();
    for v in 0..n as u32 {
        for &w in graph.neighbors(NodeId(v)) {
            if v < w {
                edges.push((v, w));
            }
        }
    }
    let m = edges.len();
    assert!(
        m < 25,
        "brute force enumeration is exponential in edge count"
    );

    let mut trees = Vec::new();
    for mask in 0u32..(1 << m) {
        if mask.count_ones() as usize != n - 1 {
            continue;
        }
        let chosen: Vec<(u32, u32)> = (0..m)
            .filter(|i| mask >> i & 1 == 1)
            .map(|i| edges[i])
            .collect();

        // n - 1 edges and connected implies acyclic.
        let mut parent: Vec<u32> = (0..n as u32).collect();
        fn find(parent: &mut [u32], mut x: u32) -> u32 {
            while parent[x as usize] != x {
                parent[x as usize] = parent[parent[x as usize] as usize];
                x = parent[x as usize];
            }
            x
        }
        let mut merged = 0;
        for &(a, b) in &chosen {
            let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
            if ra != rb {
                parent[ra as usize] = rb;
                merged += 1;
            }
        }
        if merged == n - 1 {
            trees.push(chosen);
        }
    }
    trees
}

/// Canonical form of a tree: its edges as sorted `(min, max)` pairs.
pub fn canonical(tree: &partigraph::Tree) -> Vec<(u32, u32)> {
    let mut edges: Vec<(u32, u32)> = tree
        .edges()
        .map(|child| {
            let p = tree.global(tree.parent(child).unwrap()).0;
            let c = tree.global(child).0;
            (p.min(c), p.max(c))
        })
        .collect();
    edges.sort_unstable();
    edges
}
