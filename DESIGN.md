# partigraph-rust: design

A Rust port of [partigraph](https://github.com/proebsting/partigraph), graph
partitioning into weight-balanced districts via uniform random spanning trees.

## Scope

**This round: the library only. Done — see [README.md](README.md).**

The library is general-purpose: it partitions weighted graphs. Redistricting is
the motivating application and something like GerryChain is an anticipated
client, but neither may be baked into the design. Domain knowledge belongs to
the caller, or to the I/O layer when it exists. Where GerryChain is cited below
it is as evidence for a decision or as a note about a file format, never as a
requirement on the library. A CLI will follow, but it is a new thing that
does not exist in the Python, so it gets designed once there is a library to put
behind it.

### On "stuck with the networkx file format"

Investigated, and it turns out not to bind. partigraph does no file I/O at all.
Its entire networkx surface, across all 615 lines, is:

```
Graph(), .add_node(), .add_edges_from()    building a tree
.nodes(), .neighbors(), .nodes[x][field]   reading
.subgraph()                                induced subgraph
dfs_edges(), edge_bfs()                    two traversals
```

Nine things, none of them serialization. networkx is an in-memory data structure
here, not a format, so the port is free to choose its own representation. The
findings on GerryChain's on-disk format are real and are kept in an appendix for
when the CLI needs them, but nothing in the library depends on them.

## Decisions taken

| | |
| --- | --- |
| GerryChain compatibility | **a CLI concern only.** The library stays domain-neutral: no redistricting vocabulary, no awareness of any particular client. When the CLI is built it will read GerryChain's graph files, write files it can read, and emit plans it can consume, all at the I/O boundary. No Python bindings. |
| Spanning tree sampling | Wilson's by default; a pluggable sampler, with a min-weight tree over a caller-supplied edge weight function as the alternative |
| Node identity | `NodeId(u32)` in the core; original ids live at the I/O boundary |
| Correctness bar | property tests and statistical tests; no Python at test time |
| Layout | one crate now; I/O and CLI crates when the CLI is designed |

## Layout

```
crates/
  partigraph/
    graph.rs       CSR graph, NodeId, node sets, induced subgraphs
    ust.rs         Wilson's algorithm
    tree.rs        parent-array tree, aggregate, closure
    partition.rs   Partitioner, search
```

A cargo workspace with one member, so `partigraph-io` and `partigraph-cli` can be
added later without moving anything. `rand` is the only dependency.

## The core representation

The Python works over `Hashable` nodes, `set[Node]`, and `dict[Edge, V]`. Ported
literally that becomes `HashSet<N>` and `HashMap<(N, N), V>` with a hash on every
neighbour lookup. Instead the core speaks dense indices:

```rust
pub struct NodeId(pub u32);

/// Undirected simple graph in compressed sparse row form.
pub struct Graph {
    offsets: Vec<u32>,    // len n + 1
    neighbors: Vec<u32>,  // len 2m
}
```

Sets of nodes become bitsets, and the `dict[Edge, V]` that `aggregate` returns
becomes a flat `Vec<V>`. GerryChain's own RustworkX backend renumbers to a
contiguous integer range for the same reason, so this is not an exotic choice.

### Trees

Wilson's algorithm already produces parent pointers, so a tree is a parent array
over subset-local indices, plus a preorder and subtree sizes:

```rust
pub struct Tree {
    nodes: Vec<NodeId>,      // local -> global
    parent: Vec<u32>,        // local -> local; the root is its own parent
    preorder: Vec<u32>,
    subtree_size: Vec<u32>,
    root: u32,
}
```

Two consequences worth stating, because they are why this shape was chosen.

**Every tree edge is named by its child.** A tree on *k* nodes has *k-1* edges and
*k-1* non-root nodes, and the map between them is a bijection. So the Python's
`dict[Edge, V]` keyed by *directed* edge becomes two flat arrays indexed by child:

```rust
pub struct EdgeValues<V> {
    down: Vec<V>,  // the child side of edge (parent, child)
    up:   Vec<V>,  // the parent side of the same edge
}
```

**`closure` becomes a slice.** The subtree of a node is a contiguous run in the
preorder array, so the "inside" of a downward cut is `&preorder[i..i + size]` —
no traversal at all. The upward direction is its complement.

### Aggregation

```rust
pub fn aggregate<V: Clone>(
    tree: &Tree,
    get: impl Fn(NodeId) -> V,
    identity: V,
    combine: impl Fn(&V, &V) -> V,
) -> EdgeValues<V>;
```

with `sums()` and `any()` wrappers standing in for `compute_sum` and `compute_or`.

The Python's second pass is `O(Σ deg²)` — for each of the *k-1* edges it rescans
`neighbors(src)`, and a hub node is the `src` of many edges, so a star costs
`O(k²)`. A prefix/suffix scan over each node's children computes "all siblings
except this one" in `O(deg)`, making the whole thing `O(k)`. Results are identical
for any commutative monoid, which is what the Python already implicitly assumed,
since it folds neighbours in arbitrary dict order.

### Randomness

`&mut impl Rng` threaded through explicitly, rather than Python's global `random`.
GerryChain 1.0 reached the same conclusion — every one of its tree functions now
takes `rng: random.Random | int | None`. Seeded runs are reproducible.

## The partitioner

```rust
pub struct Partitioner<'a> { graph: &'a Graph, weights: &'a [f64] }

pub struct SearchOptions {
    pub improve_limit: usize,  // consecutive failed transfers before giving up (10)
    pub backtrack: usize,      // re-divisions attempted per recursion level (3)
    pub max_attempts: usize,   // bound on the outer retry loop
}

pub struct Plan { district_of: Vec<u32>, n_districts: usize }

impl<'a> Partitioner<'a> {
    pub fn new(graph: &'a Graph, weights: &'a [f64]) -> Result<Self, PartitionError>;

    pub fn search(
        &self,
        n_districts: usize,
        is_valid: impl Fn(f64) -> bool,
        opts: &SearchOptions,
        rng: &mut impl Rng,
    ) -> Result<Plan, PartitionError>;
}
```

Three changes from the Python signature:

- **Weights are a slice, not a callable.** `get_weight` and `weight_field` collapse
  into `&[f64]` indexed by `NodeId`. Reading the attribute is the I/O crate's job.
- **Districts are `0..n`, not a generic `[T]`.** The Python threads a type parameter
  through five methods to return `dict[T, set[Node]]`. Returning an assignment
  vector indexed by node is cheaper and is also the shape GerryChain's
  `Assignment.from_dict` wants. Callers map indices to their own labels.
- **`is_valid` stays a predicate**, which is strictly more general than GerryChain's
  `pop_target` + `epsilon`. `within_epsilon(target, eps)` is provided for the
  common case.

## Behaviour changes from the Python

These are deliberate. Each is a bug or a hazard in the original.

1. **`search_for_partition` cannot loop forever.** The Python is
   `while True: try: ... except ValueError: continue`, which never returns if the
   constraint is unsatisfiable. Bounded by `max_attempts`, returning
   `Err(Exhausted)`. GerryChain hit this too and carries `max_attempts=100000`.
2. **Backtracking is not an error type.** The Python raises `ValueError` both for
   "this division failed, try another" and for "src and dst are not adjacent", so a
   real bug is silently swallowed by the backtrack handler. These become distinct
   types, and only the former drives retry.
3. **A single-node partition works.** `closest_random_division` calls `min()` on an
   empty edge map and raises `ValueError` from outside the `try`, which escapes into
   the retry loop and spins forever. Handled as a base case.
4. **A disconnected graph errors instead of hanging.** `ust.py`'s docstring claims it
   "will silently produce a spanning tree for one connected component"; in fact the
   `while u not in tree` walk from an unreachable node never terminates.
   Connectivity is checked up front. This also catches GerryChain "islands"
   (degree-0 nodes), which its own loader only warns about.
5. **`aggregate` is linear**, per above.

Not changed: `repeatedly_improve_division`'s loop guard is `division[0][1] != target`
on floats, which is essentially never true, so `improve_limit` consecutive failures
is the real termination condition. The Rust says so outright but behaves the same.

## Testing

No Python at test time. Built as designed, plus one addition: a test that the
chi-square bar can actually fail, using a random-weight minimum spanning tree as
a known-non-uniform sampler. Without it the uniformity tests could pass
vacuously. 38 tests; see the README for the measured margins.

## Appendix: the GerryChain on-disk format

Not needed by the library. Recorded here so the investigation is not lost when the
CLI gets designed.

`Graph.from_json` calls `networkx.readwrite.json_graph.adjacency_graph`; `to_json`
calls `adjacency_data`. It is *not* the node-link format.

```json
{"directed": false, "multigraph": false, "graph": [],
 "nodes": [{"TOTPOP": 1, "county": "1", "id": 0},
           {"TOTPOP": 1, "county": "1", "id": 1}],
 "adjacency": [[{"id": 8}, {"id": 1}],
               [{"id": 0}, {"id": 9}, {"id": 2}]]}
```

Four behaviours, each verified against networkx 3.7 and GerryChain 1.0.0:

- **`adjacency[i]` belongs to `nodes[i]` by position, but its entries name neighbours
  by id value.** The id-to-index map has to be built before adjacency can be resolved.
- **A neighbour id absent from `nodes` is silently created as a new node.** Given
  `nodes: [{id:20},{id:10}]` and `adjacency: [[{id:99}],[]]`, networkx yields the edge
  `(20, 99)`. A reader should probably reject this as malformed.
- **`"graph"` may be `[]`, `{}`, or absent.** GerryChain writes `[]`; networkx's
  node-link writer writes `{}` for the same field.
- **Edge attributes are written twice, once per direction, and on read the second
  occurrence wins.** A writer must duplicate them or GerryChain reads back a
  different graph.

Node ids are ints or strings. Separately, networkx changed the node-link edge key
from `"links"` to `"edges"` in 3.4, and 3.7 rejects `"links"` outright — irrelevant
to GerryChain, which uses neither.
