//! Graph construction, node sets, and induced subgraphs.

mod common;

use common::{complete, cycle, grid};
use partigraph::{Graph, GraphError, Induced, NodeId, NodeSet};

#[test]
fn parallel_edges_collapse_and_self_loops_vanish() {
    let g = Graph::from_edges(3, [(0, 1), (1, 0), (0, 1), (1, 1), (1, 2)]).unwrap();
    assert_eq!(g.edge_count(), 2);
    assert_eq!(g.neighbors(NodeId(0)), &[1]);
    assert_eq!(g.neighbors(NodeId(1)), &[0, 2]);
    assert_eq!(g.neighbors(NodeId(2)), &[1]);
}

#[test]
fn adjacency_lists_are_sorted() {
    let g = Graph::from_edges(4, [(0, 3), (0, 1), (0, 2)]).unwrap();
    assert_eq!(g.neighbors(NodeId(0)), &[1, 2, 3]);
}

#[test]
fn isolated_nodes_are_allowed_but_disconnect_the_graph() {
    let g = Graph::from_edges(3, [(0, 1)]).unwrap();
    assert_eq!(g.degree(NodeId(2)), 0);
    assert_eq!(g.component_count(), 2);
    assert!(!g.is_connected());
}

#[test]
fn an_edge_out_of_range_is_refused() {
    let err = Graph::from_edges(3, [(0, 7)]).unwrap_err();
    assert_eq!(
        err,
        GraphError::NodeOutOfRange {
            node: 7,
            node_count: 3
        }
    );
}

#[test]
fn counts_are_right_on_known_graphs() {
    assert_eq!(complete(5).edge_count(), 10);
    assert_eq!(cycle(7).edge_count(), 7);
    let g = grid(4, 5);
    assert_eq!(g.node_count(), 20);
    assert_eq!(g.edge_count(), 4 * 4 + 5 * 3); // horizontal + vertical
    assert!(g.is_connected());
}

#[test]
fn node_sets_track_membership_and_members_together() {
    let mut s = NodeSet::empty(10);
    assert!(s.is_empty());
    for v in [3u32, 1, 4, 1, 5] {
        s.insert(NodeId(v));
    }
    assert_eq!(s.len(), 4, "1 was inserted twice");
    assert!(s.contains(NodeId(4)));
    assert!(!s.contains(NodeId(2)));

    s.remove_all(&[1, 9]);
    assert_eq!(s.len(), 3);
    assert!(!s.contains(NodeId(1)));
    let mut got: Vec<u32> = s.members().to_vec();
    got.sort_unstable();
    assert_eq!(got, vec![3, 4, 5]);

    s.insert_all(&[4, 7]);
    assert_eq!(s.len(), 4, "4 was already present");
    assert!(s.contains(NodeId(7)));
}

#[test]
fn an_induced_subgraph_keeps_only_internal_edges() {
    let g = grid(3, 3);
    // The middle column: 1, 4, 7. In the grid they form a path.
    let subset = NodeSet::from_nodes(9, [1, 4, 7].map(NodeId));
    let ind = Induced::new(&g, &subset);

    assert_eq!(ind.len(), 3);
    assert!(ind.is_connected());
    assert_eq!(ind.local(NodeId(4)), Some(1));
    assert_eq!(ind.local(NodeId(0)), None);
    for l in 0..3u32 {
        assert_eq!(ind.local(ind.global(l)), Some(l));
    }
    // Node 4 neighbours 1 and 7 inside the subset; 3 and 5 are outside.
    let mut mid: Vec<u32> = ind.neighbors(ind.local(NodeId(4)).unwrap()).to_vec();
    mid.sort_unstable();
    assert_eq!(
        mid.iter().map(|&l| ind.global(l).0).collect::<Vec<_>>(),
        vec![1, 7]
    );
}

#[test]
fn a_disconnected_subset_is_reported_as_such() {
    let g = grid(3, 3);
    // Opposite corners, with nothing between them.
    let subset = NodeSet::from_nodes(9, [0, 8].map(NodeId));
    assert!(!Induced::new(&g, &subset).is_connected());
}

#[test]
fn an_empty_subset_induces_nothing() {
    let g = grid(3, 3);
    let ind = Induced::new(&g, &NodeSet::empty(9));
    assert!(ind.is_empty());
    assert!(!ind.is_connected());
}
