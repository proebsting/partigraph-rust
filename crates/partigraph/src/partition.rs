//! Recursive bisection of a graph into weight-balanced districts.

use std::ops::Range;

use rand::Rng;

use crate::graph::{Graph, Induced, NodeId, NodeSet};
use crate::tree::{self, Tree};
use crate::ust::wilson;
use crate::Error;

#[inline]
fn dist(target: f64, value: f64) -> f64 {
    (target - value).abs()
}

/// Either "this attempt failed, try another" or "stop".
///
/// The Python raises `ValueError` for both, so a genuine bug — source and
/// destination not adjacent — is silently swallowed by the backtrack handler.
enum Abort {
    Retry,
    Fatal(Error),
}

impl From<Error> for Abort {
    fn from(e: Error) -> Abort {
        Abort::Fatal(e)
    }
}

/// A set of nodes split in two, with each side's total weight.
#[derive(Clone, PartialEq, Debug)]
pub struct Division {
    pub a: NodeSet,
    pub a_weight: f64,
    pub b: NodeSet,
    pub b_weight: f64,
}

/// Knobs on the search.
#[derive(Clone, Debug)]
pub struct SearchOptions {
    /// Consecutive transfers that fail to change a division before giving up on it.
    pub improve_limit: usize,
    /// Divisions tried at each level of the recursion before reporting failure upward.
    pub backtrack: usize,
    /// Whole-graph attempts before [`Error::Exhausted`]. The Python is unbounded
    /// and hangs forever on an unsatisfiable constraint.
    pub max_attempts: usize,
}

impl Default for SearchOptions {
    fn default() -> Self {
        SearchOptions {
            improve_limit: 10,
            backtrack: 3,
            max_attempts: 1000,
        }
    }
}

/// An assignment of every node to a district in `0..district_count`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Plan {
    district_of: Vec<u32>,
    district_count: usize,
}

impl Plan {
    #[inline]
    pub fn district_count(&self) -> usize {
        self.district_count
    }

    #[inline]
    pub fn district_of(&self, v: NodeId) -> u32 {
        self.district_of[v.index()]
    }

    /// District index per node, indexed by [`NodeId`].
    #[inline]
    pub fn assignments(&self) -> &[u32] {
        &self.district_of
    }

    /// The nodes of one district.
    pub fn district(&self, d: u32) -> impl Iterator<Item = NodeId> + '_ {
        self.district_of
            .iter()
            .enumerate()
            .filter(move |&(_, &x)| x == d)
            .map(|(i, _)| NodeId(i as u32))
    }

    /// Total weight of each district, indexed by district.
    pub fn district_weights(&self, weights: &[f64]) -> Vec<f64> {
        let mut totals = vec![0.0; self.district_count];
        for (i, &d) in self.district_of.iter().enumerate() {
            totals[d as usize] += weights[i];
        }
        totals
    }
}

/// A predicate accepting weights within a relative `epsilon` of `target`.
pub fn within_epsilon(target: f64, epsilon: f64) -> impl Fn(f64) -> bool {
    move |w| (w - target).abs() <= epsilon * target.abs()
}

/// Partitions a graph into districts of approximately equal weight.
#[derive(Clone, Debug)]
pub struct Partitioner<'a> {
    graph: &'a Graph,
    weights: &'a [f64],
}

impl<'a> Partitioner<'a> {
    /// `weights` is indexed by [`NodeId`] and must be finite and non-negative.
    ///
    /// The Python takes a `get_weight` callable or a node-attribute name. A slice
    /// covers both and keeps attribute lookup out of the algorithm.
    pub fn new(graph: &'a Graph, weights: &'a [f64]) -> Result<Partitioner<'a>, Error> {
        if graph.node_count() == 0 {
            return Err(Error::EmptyGraph);
        }
        if weights.len() != graph.node_count() {
            return Err(Error::WeightCountMismatch {
                nodes: graph.node_count(),
                weights: weights.len(),
            });
        }
        // A negative weight is never taken as a move, because of the `0 < wt`
        // guard in `improve_division`, so it fails quietly in the Python.
        if let Some((i, &w)) = weights
            .iter()
            .enumerate()
            .find(|(_, &w)| !w.is_finite() || w < 0.0)
        {
            return Err(Error::BadWeight {
                node: NodeId(i as u32),
                weight: w,
            });
        }
        Ok(Partitioner { graph, weights })
    }

    pub fn graph(&self) -> &Graph {
        self.graph
    }

    pub fn weights(&self) -> &[f64] {
        self.weights
    }

    pub fn total_weight(&self, set: &NodeSet) -> f64 {
        set.members()
            .iter()
            .map(|&v| self.weights[v as usize])
            .sum()
    }

    fn tree_weights(&self, tree: &Tree) -> tree::EdgeValues<f64> {
        tree::sums(tree, |l| self.weights[tree.global(l).index()])
    }

    /// Which nodes of the induced subgraph have a neighbour in `other`.
    fn border_flags(&self, ind: &Induced, other: &NodeSet) -> (Vec<bool>, usize) {
        let mut flags = vec![false; ind.len()];
        let mut count = 0;
        for l in 0..ind.len() as u32 {
            if self
                .graph
                .neighbors(ind.global(l))
                .iter()
                .any(|&x| other.contains(NodeId(x)))
            {
                flags[l as usize] = true;
                count += 1;
            }
        }
        (flags, count)
    }

    /// Split `current` at the tree edge whose cut comes closest to `target`.
    pub fn closest_random_division(
        &self,
        current: &NodeSet,
        target: f64,
        rng: &mut impl Rng,
    ) -> Result<Division, Error> {
        self.divide(current, target, rng).map_err(|a| match a {
            Abort::Fatal(e) => e,
            Abort::Retry => Error::Indivisible,
        })
    }

    fn divide(
        &self,
        current: &NodeSet,
        target: f64,
        rng: &mut impl Rng,
    ) -> Result<Division, Abort> {
        let ind = Induced::new(self.graph, current);
        // A single node has no edges to cut. The Python calls `min()` on the
        // empty edge map here and raises out of the enclosing `try`.
        if ind.len() < 2 {
            return Err(Abort::Retry);
        }
        let tree = wilson(&ind, rng)?;
        let w = self.tree_weights(&tree);
        let best = tree
            .cuts()
            .min_by(|&x, &y| dist(target, *w.get(x)).total_cmp(&dist(target, *w.get(y))))
            .ok_or(Abort::Retry)?;

        let mut moved = Vec::new();
        tree.cut_globals(best, &mut moved);
        let mut a = NodeSet::empty(current.universe());
        a.insert_all(&moved);
        let mut b = current.clone();
        b.remove_all(&moved);
        Ok(Division {
            a,
            a_weight: *w.get(best),
            b,
            b_weight: *w.get(best.flip()),
        })
    }

    /// The transfer from `src` to `dst` whose weight is closest to `target`.
    ///
    /// Legal transfers are subtrees of a fresh random spanning tree of `src` that
    /// touch the border with `dst`, so the moved nodes stay connected to `dst` and
    /// what remains of `src` stays connected too. If `keep_adjacent` is given and
    /// non-empty, transfers that would take away every node bordering it are
    /// excluded.
    ///
    /// `Ok(None)` means no legal transfer exists.
    pub fn best_move(
        &self,
        src: &NodeSet,
        dst: &NodeSet,
        target: f64,
        keep_adjacent: Option<&NodeSet>,
        rng: &mut impl Rng,
    ) -> Result<Option<(Vec<u32>, f64)>, Error> {
        let ind = Induced::new(self.graph, src);
        let (on_border, border_count) = self.border_flags(&ind, dst);
        if border_count == 0 {
            return Err(Error::NotAdjacent);
        }

        let tree = wilson(&ind, rng)?;
        let w = self.tree_weights(&tree);
        let reaches = tree::any(&tree, |l| on_border[l as usize]);
        let keep = match keep_adjacent {
            Some(k) if !k.is_empty() => {
                let (flags, n) = self.border_flags(&ind, k);
                let counts = tree::sums(&tree, |l| if flags[l as usize] { 1.0 } else { 0.0 });
                Some((counts, n as f64))
            }
            _ => None,
        };

        let best = tree
            .cuts()
            .filter(|&c| *reaches.get(c))
            .filter(|&c| {
                keep.as_ref()
                    .map_or(true, |(counts, n)| *counts.get(c) < *n)
            })
            .min_by(|&x, &y| dist(target, *w.get(x)).total_cmp(&dist(target, *w.get(y))));

        Ok(best.map(|c| {
            let mut moved = Vec::new();
            tree.cut_globals(c, &mut moved);
            (moved, *w.get(c))
        }))
    }

    /// Move nodes across a division if doing so brings side `a` nearer `target`.
    pub fn improve_division(
        &self,
        division: Division,
        target: f64,
        keep_adjacent: (Option<&NodeSet>, Option<&NodeSet>),
        rng: &mut impl Rng,
    ) -> Result<Division, Error> {
        let Division {
            mut a,
            a_weight,
            mut b,
            b_weight,
        } = division;
        let delta = (a_weight - target).abs();
        let from_a = a_weight > target;

        let found = {
            let (src, dst, keep) = if from_a {
                (&a, &b, keep_adjacent.0)
            } else {
                (&b, &a, keep_adjacent.1)
            };
            self.best_move(src, dst, delta, keep, rng)?
        };

        // Only take a move that lands nearer the target than staying put.
        match found {
            Some((moved, wt)) if wt > 0.0 && wt < 2.0 * delta => {
                if from_a {
                    a.remove_all(&moved);
                    b.insert_all(&moved);
                    Ok(Division {
                        a,
                        a_weight: a_weight - wt,
                        b,
                        b_weight: b_weight + wt,
                    })
                } else {
                    a.insert_all(&moved);
                    b.remove_all(&moved);
                    Ok(Division {
                        a,
                        a_weight: a_weight + wt,
                        b,
                        b_weight: b_weight - wt,
                    })
                }
            }
            _ => Ok(Division {
                a,
                a_weight,
                b,
                b_weight,
            }),
        }
    }

    /// Improve until `improve_limit` consecutive transfers change nothing.
    ///
    /// The Python's loop also stops when side `a` hits `target` exactly, which on
    /// floats essentially never happens; `limit` is what actually ends the loop.
    pub fn repeatedly_improve_division(
        &self,
        mut division: Division,
        target: f64,
        limit: usize,
        keep_adjacent: (Option<&NodeSet>, Option<&NodeSet>),
        rng: &mut impl Rng,
    ) -> Result<Division, Error> {
        let mut failed = 0;
        while failed < limit && division.a_weight != target {
            let before = division.a_weight;
            division = self.improve_division(division, target, keep_adjacent, rng)?;
            if division.a_weight == before {
                failed += 1;
            } else {
                failed = 0;
            }
        }
        Ok(division)
    }

    #[allow(clippy::too_many_arguments)]
    fn partition_equally(
        &self,
        nodes: &NodeSet,
        weight: f64,
        districts: Range<u32>,
        opts: &SearchOptions,
        is_valid: &dyn Fn(f64) -> bool,
        rng: &mut impl Rng,
        out: &mut [u32],
    ) -> Result<(), Abort> {
        let count = districts.len();
        if count == 1 {
            if !is_valid(weight) {
                return Err(Abort::Retry);
            }
            for &v in nodes.members() {
                out[v as usize] = districts.start;
            }
            return Ok(());
        }
        if nodes.len() < count {
            return Err(Abort::Retry);
        }

        let mid = districts.start + (count / 2) as u32;
        let (first, second) = (districts.start..mid, mid..districts.end);
        let scaled = weight / count as f64 * first.len() as f64;

        for _ in 0..opts.backtrack.max(1) {
            let division = match self.divide(nodes, scaled, rng) {
                Ok(d) => d,
                Err(Abort::Retry) => continue,
                Err(fatal) => return Err(fatal),
            };
            let division = self.repeatedly_improve_division(
                division,
                scaled,
                opts.improve_limit,
                (None, None),
                rng,
            )?;

            let halves = self
                .partition_equally(
                    &division.a,
                    division.a_weight,
                    first.clone(),
                    opts,
                    is_valid,
                    rng,
                    out,
                )
                .and_then(|()| {
                    self.partition_equally(
                        &division.b,
                        division.b_weight,
                        second.clone(),
                        opts,
                        is_valid,
                        rng,
                        out,
                    )
                });
            match halves {
                Ok(()) => return Ok(()),
                Err(Abort::Retry) => continue,
                Err(fatal) => return Err(fatal),
            }
        }
        Err(Abort::Retry)
    }

    /// Partition the whole graph into `district_count` districts, every one of
    /// which satisfies `is_valid`.
    pub fn search(
        &self,
        district_count: usize,
        is_valid: impl Fn(f64) -> bool,
        opts: &SearchOptions,
        rng: &mut impl Rng,
    ) -> Result<Plan, Error> {
        let n = self.graph.node_count();
        if district_count == 0 {
            return Err(Error::NoDistricts);
        }
        if district_count > n {
            return Err(Error::TooFewNodes {
                nodes: n,
                districts: district_count,
            });
        }
        if !self.graph.is_connected() {
            return Err(Error::Disconnected);
        }

        let all = NodeSet::all(self.graph);
        let total = self.total_weight(&all);
        let mut out = vec![u32::MAX; n];
        let attempts = opts.max_attempts.max(1);
        let check: &dyn Fn(f64) -> bool = &is_valid;

        for _ in 0..attempts {
            match self.partition_equally(
                &all,
                total,
                0..district_count as u32,
                opts,
                check,
                rng,
                &mut out,
            ) {
                Ok(()) => {
                    return Ok(Plan {
                        district_of: out,
                        district_count,
                    })
                }
                Err(Abort::Retry) => continue,
                Err(Abort::Fatal(e)) => return Err(e),
            }
        }
        Err(Error::Exhausted { attempts })
    }
}
