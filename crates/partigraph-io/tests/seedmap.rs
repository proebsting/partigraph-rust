//! Building seed maps.

use std::collections::{HashMap, HashSet};

use partigraph_io::{
    build_seed_map, seed_map, write_assignment, AdjacencyGraph, Error, NodeLabel, Sampler,
    SeedMapOptions,
};
use serde_json::Value;

const FIXTURE: &str = include_str!("fixtures/gerrymandria.json");

fn fixture() -> AdjacencyGraph {
    AdjacencyGraph::from_json(FIXTURE).unwrap()
}

fn opts(parts: usize, epsilon: f64) -> SeedMapOptions {
    SeedMapOptions {
        parts,
        population_attr: "TOTPOP".into(),
        epsilon,
        assignment_attr: "district".into(),
        seed: 42,
        ..Default::default()
    }
}

/// Is every part connected in the graph? Checked here by flood fill over the
/// written file, not by asking the library that produced it.
fn parts_are_contiguous(written: &Value, attr: &str) -> bool {
    let nodes = written["nodes"].as_array().unwrap();
    let adjacency = written["adjacency"].as_array().unwrap();
    let index: HashMap<String, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n["id"].to_string(), i))
        .collect();
    let part: Vec<String> = nodes.iter().map(|n| n[attr].to_string()).collect();

    let neighbours: Vec<Vec<usize>> = adjacency
        .iter()
        .map(|side| {
            side.as_array()
                .unwrap()
                .iter()
                .map(|e| index[&e["id"].to_string()])
                .collect()
        })
        .collect();

    for p in part.iter().collect::<HashSet<_>>() {
        let members: Vec<usize> = (0..nodes.len()).filter(|&i| &part[i] == p).collect();
        let want: HashSet<usize> = members.iter().copied().collect();
        let mut seen = HashSet::from([members[0]]);
        let mut stack = vec![members[0]];
        while let Some(v) = stack.pop() {
            for &w in &neighbours[v] {
                if want.contains(&w) && seen.insert(w) {
                    stack.push(w);
                }
            }
        }
        if seen.len() != members.len() {
            return false;
        }
    }
    true
}

#[test]
fn produces_balanced_contiguous_parts() {
    // 64 nodes of population 1 into 4 parts: exactly 16 each is achievable.
    let graph = fixture();
    let (seeded, report) = build_seed_map(&graph, &opts(4, 0.0)).unwrap();

    assert_eq!(report.ideal, 16.0);
    assert_eq!(report.populations, vec![16.0; 4]);
    assert_eq!(report.sizes(), vec![16; 4]);
    assert_eq!(report.worst_deviation(), 0.0);

    let written = seeded.to_json_value();
    assert!(parts_are_contiguous(&written, "district"));
}

#[test]
fn honours_the_tolerance() {
    let graph = fixture();
    for (parts, epsilon) in [(3usize, 0.1f64), (5, 0.25), (7, 0.2)] {
        let report = seed_map(&graph, &opts(parts, epsilon)).unwrap();
        assert_eq!(report.populations.len(), parts);
        assert!(
            report.worst_deviation() <= epsilon + 1e-12,
            "{parts} parts at epsilon {epsilon}: worst deviation {}",
            report.worst_deviation()
        );
        assert_eq!(report.populations.iter().sum::<f64>(), 64.0);
    }
}

#[test]
fn parts_are_zero_indexed_integers() {
    // Matching GerryChain's Partition.from_random_assignment, so the labels
    // are what a GerryChain user expects to see.
    let graph = fixture();
    let (seeded, _) = build_seed_map(&graph, &opts(4, 0.0)).unwrap();
    let written = seeded.to_json_value();

    let mut seen: Vec<i64> = written["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| {
            let v = &n["district"];
            assert!(v.is_i64(), "part label {v} is not an integer");
            v.as_i64().unwrap()
        })
        .collect();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen, vec![0, 1, 2, 3]);
}

#[test]
fn the_assignment_attribute_is_named_by_the_caller() {
    let graph = fixture();
    let mut options = opts(2, 0.0);
    options.assignment_attr = "seed_part".into();
    let (seeded, _) = build_seed_map(&graph, &options).unwrap();
    let written = seeded.to_json_value();
    assert!(written["nodes"][0]["seed_part"].is_i64());
    // The fixture's own "district" attribute is untouched.
    assert_eq!(written["nodes"][0]["district"], Value::from("1"));
}

#[test]
fn an_existing_attribute_of_that_name_is_replaced() {
    // The fixture already carries a string-valued "district".
    let graph = fixture();
    assert_eq!(
        graph.node_attr(partigraph::NodeId(0), "district"),
        Some(&Value::from("1"))
    );
    let (seeded, _) = build_seed_map(&graph, &opts(4, 0.0)).unwrap();
    assert!(seeded.to_json_value()["nodes"][0]["district"].is_i64());
}

#[test]
fn everything_else_in_the_file_is_left_alone() {
    let graph = fixture();
    let (seeded, _) = build_seed_map(&graph, &opts(4, 0.0)).unwrap();
    let before: Value = serde_json::from_str(FIXTURE).unwrap();
    let after = seeded.to_json_value();

    assert_eq!(before["graph"], after["graph"]);
    assert_eq!(before["adjacency"], after["adjacency"]);
    for (a, b) in before["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .zip(after["nodes"].as_array().unwrap())
    {
        for (k, v) in a.as_object().unwrap() {
            if k != "district" {
                assert_eq!(b.get(k), Some(v), "attribute {k} changed");
            }
        }
    }
}

#[test]
fn the_same_seed_gives_the_same_map() {
    let graph = fixture();
    let run = |seed| {
        let mut o = opts(4, 0.0);
        o.seed = seed;
        build_seed_map(&graph, &o).unwrap().0.to_json_value()
    };
    assert_eq!(run(7), run(7));
    assert!(
        (0..4).any(|s| run(s) != run(s + 100)),
        "the seed is not being used"
    );
}

#[test]
fn either_sampler_can_be_used() {
    let graph = fixture();
    for sampler in [Sampler::Uniform, Sampler::MinWeightTree] {
        let mut o = opts(4, 0.0);
        o.sampler = sampler;
        let (seeded, report) = build_seed_map(&graph, &o).unwrap();
        assert_eq!(report.populations, vec![16.0; 4], "{sampler:?}");
        assert!(
            parts_are_contiguous(&seeded.to_json_value(), "district"),
            "{sampler:?}"
        );
    }
}

#[test]
fn write_assignment_can_be_used_on_its_own() {
    // The pieces are available separately, for a caller that wants the
    // partition without the file, or the file without recomputing.
    let graph = fixture();
    let report = seed_map(&graph, &opts(2, 0.0)).unwrap();
    assert_eq!(report.partition.part_count(), 2);

    let mut copy = fixture();
    write_assignment(&mut copy, "mine", &report.partition);
    let written = copy.to_json_value();
    assert_eq!(
        written["nodes"][0]["mine"],
        Value::from(report.partition.part_of(partigraph::NodeId(0)))
    );
}

#[test]
fn impossible_requests_are_reported_not_hung_on() {
    let graph = fixture();

    // 64 unit-population nodes into 5 exactly equal parts is arithmetically
    // impossible; ideal is 12.8. GerryChain raises RuntimeError here.
    let mut tight = opts(5, 0.0);
    tight.search.max_attempts = 20;
    assert!(matches!(
        seed_map(&graph, &tight).unwrap_err(),
        Error::Partitioning(partigraph::Error::Exhausted { .. })
    ));

    // More parts than nodes.
    assert!(matches!(
        seed_map(&graph, &opts(100, 0.5)).unwrap_err(),
        Error::Partitioning(partigraph::Error::TooFewNodes {
            nodes: 64,
            parts: 100
        })
    ));

    // No parts at all.
    assert!(matches!(
        seed_map(&graph, &opts(0, 0.5)).unwrap_err(),
        Error::Partitioning(partigraph::Error::NoParts)
    ));

    // A nonsense tolerance.
    let mut bad = opts(4, -1.0);
    bad.epsilon = f64::NAN;
    assert!(matches!(
        seed_map(&graph, &bad).unwrap_err(),
        Error::BadEpsilon(_)
    ));
}

#[test]
fn an_island_is_explained_in_the_files_own_terms() {
    // A disconnected graph cannot be partitioned. Saying "node 99 has no
    // neighbours" is more use than "the graph is not connected".
    let text = serde_json::json!({
        "directed": false, "multigraph": false, "graph": [],
        "nodes": [{"pop": 1, "id": 0}, {"pop": 1, "id": 1}, {"pop": 1, "id": 99}],
        "adjacency": [[{"id": 1}], [{"id": 0}], []]
    })
    .to_string();
    let graph = AdjacencyGraph::from_json(&text).unwrap();
    let mut o = opts(2, 0.5);
    o.population_attr = "pop".into();
    let err = seed_map(&graph, &o).unwrap_err();
    assert!(matches!(err, Error::Islands { count: 1, .. }), "{err:?}");
    assert!(err.to_string().contains("99"), "{err}");
}

#[test]
fn a_missing_population_column_is_caught_before_partitioning() {
    let graph = fixture();
    let mut o = opts(4, 0.0);
    o.population_attr = "NOT_A_COLUMN".into();
    let err = seed_map(&graph, &o).unwrap_err();
    assert!(matches!(err, Error::MissingNodeAttr { .. }), "{err:?}");
}

#[test]
fn node_ids_are_not_assumed_to_be_indices() {
    // A file whose ids are strings, out of order, must still round-trip and
    // get a correct assignment.
    let text = serde_json::json!({
        "directed": false, "multigraph": false, "graph": [],
        "nodes": [{"pop": 1, "id": "c"}, {"pop": 1, "id": "a"},
                  {"pop": 1, "id": "d"}, {"pop": 1, "id": "b"}],
        "adjacency": [[{"id": "a"}], [{"id": "c"}, {"id": "d"}],
                      [{"id": "a"}, {"id": "b"}], [{"id": "d"}]]
    })
    .to_string();
    let graph = AdjacencyGraph::from_json(&text).unwrap();
    let mut o = opts(2, 0.0);
    o.population_attr = "pop".into();
    let (seeded, report) = build_seed_map(&graph, &o).unwrap();

    assert_eq!(report.populations, vec![2.0, 2.0]);
    let written = seeded.to_json_value();
    let ids: Vec<&str> = written["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        vec!["c", "a", "d", "b"],
        "node order must be preserved"
    );
    assert!(parts_are_contiguous(&written, "district"));
    assert_eq!(graph.index_of(&NodeLabel::Str("d".into())).unwrap().0, 2);
}
