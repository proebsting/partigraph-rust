//! The networkx "adjacency" JSON graph format.
//!
//! This is what `networkx.readwrite.json_graph.adjacency_data` writes and
//! `adjacency_graph` reads, and therefore what GerryChain's `Graph.to_json` and
//! `Graph.from_json` use. A file looks like:
//!
//! ```json
//! {"directed": false, "multigraph": false, "graph": [],
//!  "nodes": [{"TOTPOP": 1, "id": 0}, {"TOTPOP": 4, "id": 1}],
//!  "adjacency": [[{"id": 1}], [{"id": 0}]]}
//! ```
//!
//! Four things about the format are worth stating, because each is a way to get
//! it wrong. All were checked against networkx 3.7 and GerryChain 1.0.0.
//!
//! - `adjacency[i]` belongs to `nodes[i]` **by position**, but the entries
//!   inside it name their neighbour **by id value**. The id-to-index map has to
//!   exist before adjacency can be resolved.
//! - A neighbour id that is absent from `nodes` is *silently created as a new
//!   node* by networkx. [`AdjacencyGraph::from_json`] rejects it instead.
//! - `"graph"` may be a list, an object, or absent. GerryChain writes `[]`;
//!   networkx's node-link writer writes `{}` for the same field. Whatever came
//!   in is written back out.
//! - Edge attributes appear **twice**, once in each direction. On read the
//!   second occurrence wins, which is what networkx does; on write both copies
//!   are emitted, or the file would not read back the same.
//!
//! Nothing here knows about populations, districts or any other domain: it is a
//! graph with arbitrary JSON attributes hanging off the nodes and edges.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

use partigraph::{Graph, NodeId};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::Error;

/// A node's id as it appears in the file.
///
/// networkx allows any hashable; in practice these are integers or strings.
/// Anything else is kept as its JSON text so that it round-trips unchanged.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum NodeLabel {
    Int(i64),
    Str(String),
    Other(String),
}

impl NodeLabel {
    fn from_value(v: &Value) -> NodeLabel {
        match v {
            Value::Number(n) if n.is_i64() => NodeLabel::Int(n.as_i64().unwrap()),
            Value::String(s) => NodeLabel::Str(s.clone()),
            other => NodeLabel::Other(other.to_string()),
        }
    }

    fn to_value(&self) -> Value {
        match self {
            NodeLabel::Int(i) => Value::from(*i),
            NodeLabel::Str(s) => Value::from(s.clone()),
            NodeLabel::Other(text) => {
                serde_json::from_str(text).unwrap_or_else(|_| Value::from(text.clone()))
            }
        }
    }
}

impl std::fmt::Display for NodeLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeLabel::Int(i) => write!(f, "{i}"),
            NodeLabel::Str(s) => write!(f, "{s}"),
            NodeLabel::Other(t) => write!(f, "{t}"),
        }
    }
}

#[derive(Deserialize)]
struct Raw {
    #[serde(default)]
    directed: bool,
    #[serde(default)]
    multigraph: bool,
    #[serde(default)]
    graph: Option<Value>,
    nodes: Vec<Map<String, Value>>,
    adjacency: Vec<Vec<Map<String, Value>>>,
}

/// A graph read from, or destined for, an adjacency JSON file.
///
/// Node and edge attributes are held as arbitrary JSON and written back
/// unchanged, so reading a file and writing it again preserves everything this
/// type does not itself understand.
#[derive(Clone, Debug)]
pub struct AdjacencyGraph {
    graph_attrs: Value,
    labels: Vec<NodeLabel>,
    node_attrs: Vec<Map<String, Value>>,
    /// Each undirected edge once, as `(u, v, attrs)` with `u < v`.
    edges: Vec<(u32, u32, Map<String, Value>)>,
    index_of: HashMap<NodeLabel, u32>,
}

impl AdjacencyGraph {
    pub fn from_json(text: &str) -> Result<AdjacencyGraph, Error> {
        let raw: Raw = serde_json::from_str(text)?;
        AdjacencyGraph::from_raw(raw)
    }

    pub fn read(path: impl AsRef<Path>) -> Result<AdjacencyGraph, Error> {
        let path = path.as_ref();
        let mut text = String::new();
        BufReader::new(File::open(path).map_err(|e| Error::ReadingFile(path.into(), e))?)
            .read_to_string(&mut text)
            .map_err(|e| Error::ReadingFile(path.into(), e))?;
        AdjacencyGraph::from_json(&text)
    }

    fn from_raw(raw: Raw) -> Result<AdjacencyGraph, Error> {
        if raw.directed {
            return Err(Error::UnsupportedGraph("directed"));
        }
        if raw.multigraph {
            return Err(Error::UnsupportedGraph("multigraph"));
        }
        if raw.nodes.len() != raw.adjacency.len() {
            return Err(Error::AdjacencyLengthMismatch {
                nodes: raw.nodes.len(),
                adjacency: raw.adjacency.len(),
            });
        }

        let mut labels = Vec::with_capacity(raw.nodes.len());
        let mut node_attrs = Vec::with_capacity(raw.nodes.len());
        let mut index_of = HashMap::with_capacity(raw.nodes.len());
        for (i, mut attrs) in raw.nodes.into_iter().enumerate() {
            let id = attrs
                .remove("id")
                .ok_or(Error::NodeWithoutId { position: i })?;
            let label = NodeLabel::from_value(&id);
            if let Some(first) = index_of.insert(label.clone(), i as u32) {
                return Err(Error::DuplicateNodeId {
                    label: label.to_string(),
                    first: first as usize,
                    second: i,
                });
            }
            labels.push(label);
            node_attrs.push(attrs);
        }

        // Collect each undirected edge once. The later of the two directions
        // wins on attributes, matching networkx.
        let mut seen: HashMap<(u32, u32), usize> = HashMap::new();
        let mut edges: Vec<(u32, u32, Map<String, Value>)> = Vec::new();
        for (source, neighbours) in raw.adjacency.into_iter().enumerate() {
            let source = source as u32;
            for mut entry in neighbours {
                let id = entry.remove("id").ok_or(Error::NeighbourWithoutId {
                    node: labels[source as usize].to_string(),
                })?;
                let label = NodeLabel::from_value(&id);
                let target = *index_of
                    .get(&label)
                    .ok_or_else(|| Error::UnknownNeighbour {
                        node: labels[source as usize].to_string(),
                        neighbour: label.to_string(),
                    })?;
                if source == target {
                    continue; // a self-loop carries no adjacency
                }
                let key = (source.min(target), source.max(target));
                match seen.get(&key) {
                    Some(&at) => edges[at].2 = entry,
                    None => {
                        seen.insert(key, edges.len());
                        edges.push((key.0, key.1, entry));
                    }
                }
            }
        }

        Ok(AdjacencyGraph {
            graph_attrs: raw.graph.unwrap_or_else(|| Value::Array(Vec::new())),
            labels,
            node_attrs,
            edges,
            index_of,
        })
    }

    pub fn to_json_value(&self) -> Value {
        let nodes: Vec<Value> = self
            .labels
            .iter()
            .zip(&self.node_attrs)
            .map(|(label, attrs)| {
                let mut o = attrs.clone();
                // networkx writes `id` last, after the attributes.
                o.insert("id".into(), label.to_value());
                Value::Object(o)
            })
            .collect();

        // Both directions of every edge, each carrying the same attributes.
        let mut adjacency: Vec<Vec<Value>> = vec![Vec::new(); self.labels.len()];
        for (u, v, attrs) in &self.edges {
            for (from, to) in [(*u, *v), (*v, *u)] {
                let mut o = attrs.clone();
                o.insert("id".into(), self.labels[to as usize].to_value());
                adjacency[from as usize].push(Value::Object(o));
            }
        }

        let mut root = Map::new();
        root.insert("directed".into(), Value::Bool(false));
        root.insert("multigraph".into(), Value::Bool(false));
        root.insert("graph".into(), self.graph_attrs.clone());
        root.insert("nodes".into(), Value::Array(nodes));
        root.insert(
            "adjacency".into(),
            Value::Array(adjacency.into_iter().map(Value::Array).collect()),
        );
        Value::Object(root)
    }

    pub fn write(&self, path: impl AsRef<Path>) -> Result<(), Error> {
        let path = path.as_ref();
        let file = File::create(path).map_err(|e| Error::WritingFile(path.into(), e))?;
        let mut out = BufWriter::new(file);
        serde_json::to_writer(&mut out, &self.to_json_value())?;
        out.flush()
            .map_err(|e| Error::WritingFile(path.into(), e))?;
        Ok(())
    }

    pub fn node_count(&self) -> usize {
        self.labels.len()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    pub fn labels(&self) -> &[NodeLabel] {
        &self.labels
    }

    pub fn label(&self, v: NodeId) -> &NodeLabel {
        &self.labels[v.index()]
    }

    pub fn index_of(&self, label: &NodeLabel) -> Option<NodeId> {
        self.index_of.get(label).map(|&i| NodeId(i))
    }

    pub fn node_attr(&self, v: NodeId, name: &str) -> Option<&Value> {
        self.node_attrs[v.index()].get(name)
    }

    /// The names of every attribute appearing on any node, sorted.
    pub fn node_attr_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self
            .node_attrs
            .iter()
            .flat_map(|a| a.keys().map(String::as_str))
            .collect();
        names.sort_unstable();
        names.dedup();
        names
    }

    /// Read a numeric node attribute into a slice indexed by [`NodeId`].
    ///
    /// Strict on purpose: a missing or non-numeric value is an error naming the
    /// node, rather than a silent zero or a coerced string, because a mistyped
    /// attribute name would otherwise produce a confidently wrong result.
    pub fn numeric_node_attr(&self, name: &str) -> Result<Vec<f64>, Error> {
        let mut out = Vec::with_capacity(self.node_count());
        for (i, attrs) in self.node_attrs.iter().enumerate() {
            let label = || self.labels[i].to_string();
            match attrs.get(name) {
                None => {
                    return Err(Error::MissingNodeAttr {
                        attr: name.into(),
                        node: label(),
                        available: self.node_attr_names().join(", "),
                    })
                }
                Some(Value::Number(n)) => out.push(n.as_f64().unwrap_or(f64::NAN)),
                Some(other) => {
                    return Err(Error::NonNumericNodeAttr {
                        attr: name.into(),
                        node: label(),
                        found: other.to_string(),
                    })
                }
            }
        }
        Ok(out)
    }

    /// Set a node attribute from a slice indexed by [`NodeId`], replacing any
    /// existing value.
    pub fn set_node_attr(&mut self, name: &str, values: impl IntoIterator<Item = Value>) {
        for (attrs, value) in self.node_attrs.iter_mut().zip(values) {
            attrs.insert(name.to_string(), value);
        }
    }

    /// Build the partigraph [`Graph`] this file describes.
    pub fn to_graph(&self) -> Result<Graph, Error> {
        Ok(Graph::from_edges(
            self.node_count(),
            self.edges.iter().map(|&(u, v, _)| (u, v)),
        )?)
    }

    /// Nodes with no neighbours. GerryChain calls these islands and warns about
    /// them; they make the graph disconnected, so partitioning will refuse it.
    pub fn islands(&self) -> Vec<&NodeLabel> {
        let mut degree = vec![0usize; self.node_count()];
        for &(u, v, _) in &self.edges {
            degree[u as usize] += 1;
            degree[v as usize] += 1;
        }
        degree
            .iter()
            .enumerate()
            .filter(|&(_, &d)| d == 0)
            .map(|(i, _)| &self.labels[i])
            .collect()
    }
}
