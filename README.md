# partigraph-rust

A Rust implementation of [partigraph](https://github.com/proebsting/partigraph),
which partitions a graph into districts of approximately equal weight using
uniform random spanning trees.

The goal is a library you can call from another Rust program -- hand it a graph
and a weight per node, get an assignment back, no files in the middle -- with a
command-line interface to follow.

**Status: the library is ported.** All of `ust.py`, `spanning_tree.py`, and
`partition.py`, with the behaviour changes listed below. The CLI is not written
yet; it is a new thing that does not exist in the Python, so it is being designed
separately. See [DESIGN.md](DESIGN.md).

## Using it

```rust
use partigraph::{Graph, Partitioner, SearchOptions, within_epsilon};
use rand::{rngs::StdRng, SeedableRng};

let graph = Graph::from_edges(node_count, edges)?;
let weights: Vec<f64> = /* one per node, indexed by NodeId */;

let target = weights.iter().sum::<f64>() / 13.0;
let mut rng = StdRng::seed_from_u64(42);

let plan = Partitioner::new(&graph, &weights)?
    .search(13, within_epsilon(target, 0.01), &SearchOptions::default(), &mut rng)?;

for (node, district) in plan.assignments().iter().enumerate() {
    println!("{node} -> {district}");
}
```

`is_valid` is any `Fn(f64) -> bool`, so a tolerance is only the common case;
`within_epsilon` is a convenience over it. The rng is threaded through
explicitly, so a seeded run reproduces exactly.

## Layout

```
crates/partigraph/
  graph.rs       CSR graph, NodeId, node sets, induced subgraphs
  ust.rs         Wilson's algorithm
  tree.rs        parent-array trees, aggregate, cuts
  partition.rs   Partitioner, search
```

`rand` is the only dependency.

## How it differs from the Python

The algorithm is the same. The representation is not: the Python works over
`Hashable` nodes, `set[Node]` and `dict[Edge, V]`, which ported literally would
hash on every neighbour lookup. Here nodes are `u32` indices into a CSR graph,
node sets are bitsets, and a spanning tree is the parent array Wilson's algorithm
already produces.

Two things follow from that. A tree on *k* nodes has *k-1* edges and *k-1*
non-root nodes, and the map between them is a bijection, so `dict[Edge, V]`
becomes two flat `Vec<V>`s indexed by child. And a subtree is a contiguous run of
the preorder array, so `closure` is a slice rather than a traversal.

`aggregate`'s second pass is also asymptotically faster. The Python rescans
`neighbors(src)` once per edge, and a hub node is the `src` of many edges, so a
star costs `O(k²)`. A prefix/suffix scan over each node's children gives "all
siblings but this one" in `O(deg)`, making the pass `O(k)`. Results are identical
for any commutative monoid, which the Python already assumed.

### Deliberate behaviour changes

Each of these is a bug or a hazard in the original.

1. **The search terminates.** `search_for_partition` is
   `while True: try: ... except ValueError: continue`, which never returns if the
   constraint cannot be met. Bounded by `SearchOptions::max_attempts`.
2. **Backtracking is not an error type.** The Python raises `ValueError` both for
   "this division failed, try another" and for "these two partitions do not
   touch", so a real bug is swallowed by the backtrack handler. Separate types
   now, and only the former retries.
3. **A single-node partition works.** `closest_random_division` calls `min()` on
   an empty edge map and raises from outside the enclosing `try`, which escapes
   into the retry loop and spins forever.
4. **A disconnected graph is refused.** `ust.py` claims it "will silently produce
   a spanning tree for one connected component"; in fact the `while u not in tree`
   walk from an unreachable node never terminates.
5. **Bad weights are rejected.** A negative weight is never taken as a move,
   because of the `0 < wt` guard, so it fails silently in the Python. Non-finite
   and negative weights are now an error naming the node.

Unchanged: `repeatedly_improve_division`'s loop guard compares floats for
equality with the target and so is essentially never true; the real termination
condition is `improve_limit` consecutive failures. The Rust says so outright but
behaves the same way.

## How correctness is established

No Python at test time. 38 tests, in four groups.

**Uniformity.** Wilson's sampling uniformly over spanning trees is the property
the library exists to provide, and the one most easily broken in silence -- a
biased sampler still returns perfectly valid trees. On graphs small enough to
enumerate every spanning tree by brute force, many trees are drawn and the counts
checked against uniform with chi-square.

| | spanning trees | draws | Wilson's chi-square | bar (99.9%) |
| --- | --- | --- | --- | --- |
| 5-cycle | 5 | 40,000 | 1.58 | 18.47 |
| 2x3 grid | 15 | 60,000 | 13.83 | 36.12 |
| K4 | 16 | 64,000 | 21.45 | 37.70 |
| K5 | 125 | 125,000 | 107.70 | 187.50 |

**And that bar has teeth**, which is checked separately, because a chi-square
test with too few draws passes for any sampler and would make the table above
vacuous. The same machinery, at the same draw count, seed and threshold, must
*reject* a sampler known not to be uniform. The stand-in is a random-weight
minimum spanning tree -- which is what GerryChain's `random_spanning_tree` does.
On K4 at 64,000 draws it scores **142.3** against the same 37.70 bar that
Wilson's clears at 21.45.

**Differential.** `aggregate` and the cut machinery are reimplemented the slow,
obvious way -- flood fill over an adjacency list rebuilt from the parent
pointers -- and compared on many random trees, for sum, for or, and for a third
monoid to check nothing is special-cased.

**Invariants.** Every district is non-empty and contiguous in the graph, checked
by an independent flood fill; districts partition the node set exactly; weights
land inside the requested tolerance; a seed reproduces a plan.

```bash
cargo test
```

## Performance

Partitioning a grid into equal districts to a 1% tolerance, one core:

| | partigraph | partigraph-rust | |
| --- | --- | --- | --- |
| 2,500 nodes, 10 districts | 138 ms/plan | 6.2 ms/plan | 22x |
| 10,000 nodes, 20 districts | 812 ms/plan | 28.4 ms/plan | 29x |

Both produce exactly equal districts on these inputs, so the comparison is like
for like. Reproduce the Rust side with:

```bash
cargo run --release --example bench -- 100 20 10
```

## License

MIT, as the original.
