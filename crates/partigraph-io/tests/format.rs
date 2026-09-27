//! Reading and writing the adjacency format, including the ways it can be malformed.

use partigraph_io::{AdjacencyGraph, Error, NodeLabel};
use serde_json::{json, Value};

const FIXTURE: &str = include_str!("fixtures/gerrymandria.json");

fn two_nodes(graph_field: Option<Value>) -> String {
    let mut root = json!({
        "directed": false, "multigraph": false,
        "nodes": [{"pop": 5, "id": 10}, {"pop": 3, "id": 20}],
        "adjacency": [[{"id": 20}], [{"id": 10}]]
    });
    if let Some(g) = graph_field {
        root["graph"] = g;
    }
    root.to_string()
}

#[test]
fn reads_a_graph_gerrychain_wrote() {
    let g = AdjacencyGraph::from_json(FIXTURE).unwrap();
    assert_eq!(g.node_count(), 64);
    assert_eq!(g.edge_count(), 112); // 8x8 grid: 2 * 8 * 7
    assert!(g.islands().is_empty());
    assert!(g.to_graph().unwrap().is_connected());

    let pop = g.numeric_node_attr("TOTPOP").unwrap();
    assert_eq!(pop.len(), 64);
    assert_eq!(pop.iter().sum::<f64>(), 64.0);
    assert!(g.node_attr_names().contains(&"county"));
}

#[test]
fn a_round_trip_changes_nothing() {
    let once = AdjacencyGraph::from_json(FIXTURE).unwrap();
    let text = once.to_json_value().to_string();
    let twice = AdjacencyGraph::from_json(&text).unwrap();

    assert_eq!(once.node_count(), twice.node_count());
    assert_eq!(once.edge_count(), twice.edge_count());
    assert_eq!(once.labels(), twice.labels());
    assert_eq!(once.to_json_value(), twice.to_json_value());

    // Against the original bytes, not just against ourselves: every node
    // attribute the file carried must still be there, with the same value.
    let original: Value = serde_json::from_str(FIXTURE).unwrap();
    let written = once.to_json_value();
    for (a, b) in original["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .zip(written["nodes"].as_array().unwrap())
    {
        assert_eq!(a, b, "a node attribute changed on the way through");
    }
    assert_eq!(original["graph"], written["graph"]);
}

#[test]
fn edge_attributes_are_written_in_both_directions() {
    // networkx reads only one direction, so a writer that emits an edge's
    // attributes once produces a file that does not read back the same.
    let text = json!({
        "directed": false, "multigraph": false, "graph": [],
        "nodes": [{"id": 0}, {"id": 1}],
        "adjacency": [[{"id": 1, "shared_perim": 2.5}], [{"id": 0, "shared_perim": 2.5}]]
    })
    .to_string();
    let written = AdjacencyGraph::from_json(&text).unwrap().to_json_value();
    let adjacency = written["adjacency"].as_array().unwrap();
    for side in adjacency {
        let entry = &side.as_array().unwrap()[0];
        assert_eq!(entry["shared_perim"], json!(2.5));
    }
}

#[test]
fn the_later_direction_wins_when_they_disagree() {
    // Matching networkx, which overwrites as it reads.
    let text = json!({
        "directed": false, "multigraph": false, "graph": [],
        "nodes": [{"id": 0}, {"id": 1}],
        "adjacency": [[{"id": 1, "w": "from0"}], [{"id": 0, "w": "from1"}]]
    })
    .to_string();
    let written = AdjacencyGraph::from_json(&text).unwrap().to_json_value();
    assert_eq!(written["adjacency"][0][0]["w"], json!("from1"));
}

#[test]
fn the_graph_field_survives_in_any_of_its_shapes() {
    for shape in [
        Some(json!([])),
        Some(json!({})),
        Some(json!([["crs", "EPSG:4269"]])),
        None,
    ] {
        let text = two_nodes(shape.clone());
        let g = AdjacencyGraph::from_json(&text).unwrap();
        let written = g.to_json_value();
        let expected = shape.unwrap_or_else(|| json!([]));
        assert_eq!(
            written["graph"], expected,
            "graph field {expected} did not survive"
        );
    }
}

#[test]
fn string_and_integer_node_ids_both_work() {
    let text = json!({
        "directed": false, "multigraph": false, "graph": [],
        "nodes": [{"id": "37001"}, {"id": "37002"}],
        "adjacency": [[{"id": "37002"}], [{"id": "37001"}]]
    })
    .to_string();
    let g = AdjacencyGraph::from_json(&text).unwrap();
    assert_eq!(g.labels()[0], NodeLabel::Str("37001".into()));
    assert_eq!(g.index_of(&NodeLabel::Str("37002".into())).unwrap().0, 1);
    assert_eq!(g.to_json_value()["nodes"][0]["id"], json!("37001"));

    let g2 = AdjacencyGraph::from_json(&two_nodes(None)).unwrap();
    assert_eq!(g2.labels()[0], NodeLabel::Int(10));
    assert_eq!(g2.to_json_value()["nodes"][0]["id"], json!(10));
}

#[test]
fn a_neighbour_that_is_not_a_node_is_refused() {
    // networkx silently invents node 99 here, yielding a graph the file does
    // not describe.
    let text = json!({
        "directed": false, "multigraph": false, "graph": [],
        "nodes": [{"id": 20}, {"id": 10}],
        "adjacency": [[{"id": 99}], []]
    })
    .to_string();
    let err = AdjacencyGraph::from_json(&text).unwrap_err();
    assert!(
        matches!(&err, Error::UnknownNeighbour { node, neighbour } if node == "20" && neighbour == "99"),
        "got {err:?}"
    );
    assert!(err.to_string().contains("99"));
}

#[test]
fn malformed_files_are_named_precisely() {
    type Check = Box<dyn Fn(&Error) -> bool>;
    let cases: Vec<(&str, Check)> = vec![
        (
            r#"{"directed": true, "multigraph": false, "graph": [], "nodes": [], "adjacency": []}"#,
            Box::new(|e| matches!(e, Error::UnsupportedGraph("directed"))),
        ),
        (
            r#"{"directed": false, "multigraph": true, "graph": [], "nodes": [], "adjacency": []}"#,
            Box::new(|e| matches!(e, Error::UnsupportedGraph("multigraph"))),
        ),
        (
            r#"{"nodes": [{"id": 0}, {"id": 1}], "adjacency": [[]]}"#,
            Box::new(|e| {
                matches!(
                    e,
                    Error::AdjacencyLengthMismatch {
                        nodes: 2,
                        adjacency: 1
                    }
                )
            }),
        ),
        (
            r#"{"nodes": [{"pop": 1}], "adjacency": [[]]}"#,
            Box::new(|e| matches!(e, Error::NodeWithoutId { position: 0 })),
        ),
        (
            r#"{"nodes": [{"id": 7}, {"id": 7}], "adjacency": [[], []]}"#,
            Box::new(|e| {
                matches!(
                    e,
                    Error::DuplicateNodeId {
                        first: 0,
                        second: 1,
                        ..
                    }
                )
            }),
        ),
        (
            r#"{"nodes": [{"id": 0}, {"id": 1}], "adjacency": [[{"weight": 1}], []]}"#,
            Box::new(|e| matches!(e, Error::NeighbourWithoutId { .. })),
        ),
        ("not json at all", Box::new(|e| matches!(e, Error::Json(_)))),
    ];
    for (text, matches) in cases {
        let err = AdjacencyGraph::from_json(text).unwrap_err();
        assert!(matches(&err), "unexpected error for {text}: {err:?}");
        assert!(!err.to_string().is_empty());
    }
}

#[test]
fn a_bad_population_attribute_says_which_node_and_what_is_there() {
    let g = AdjacencyGraph::from_json(FIXTURE).unwrap();

    let err = g.numeric_node_attr("POPULATION").unwrap_err();
    let msg = err.to_string();
    assert!(matches!(err, Error::MissingNodeAttr { .. }));
    assert!(msg.contains("POPULATION"), "{msg}");
    assert!(
        msg.contains("TOTPOP"),
        "the message should list what is available: {msg}"
    );

    // `county` is a string in this file, so asking for it as a number fails
    // rather than silently coercing.
    let err = g.numeric_node_attr("county").unwrap_err();
    assert!(matches!(err, Error::NonNumericNodeAttr { .. }), "{err:?}");
    assert!(err.to_string().contains("not a number"));
}

#[test]
fn islands_are_found() {
    let text = json!({
        "directed": false, "multigraph": false, "graph": [],
        "nodes": [{"id": 0}, {"id": 1}, {"id": "lonely"}],
        "adjacency": [[{"id": 1}], [{"id": 0}], []]
    })
    .to_string();
    let g = AdjacencyGraph::from_json(&text).unwrap();
    assert_eq!(g.islands(), vec![&NodeLabel::Str("lonely".into())]);
}

#[test]
fn self_loops_are_dropped() {
    let text = json!({
        "directed": false, "multigraph": false, "graph": [],
        "nodes": [{"id": 0}, {"id": 1}],
        "adjacency": [[{"id": 0}, {"id": 1}], [{"id": 0}]]
    })
    .to_string();
    let g = AdjacencyGraph::from_json(&text).unwrap();
    assert_eq!(g.edge_count(), 1);
}
