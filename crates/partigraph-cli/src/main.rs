//! An example client for [`partigraph`]: generate a GerryChain seed map.
//!
//! Deliberately narrow. It exists to show what using the library looks like
//! end to end, and to be useful for the one job it does. Everything it does is
//! in `partigraph-io`, so the same thing can be done from Rust without going
//! through a file or this binary; see `partigraph_io::build_seed_map`.

use std::process::ExitCode;

use clap::{Parser, ValueEnum};
use partigraph::SearchOptions;
use partigraph_io::{build_seed_map, AdjacencyGraph, Sampler, SeedMapOptions};

/// Generate a seed map: partition a graph into population-balanced parts and
/// write it back out with the assignment on the nodes.
///
/// Reads a graph in the networkx "adjacency" JSON format, which is what
/// GerryChain's Graph.to_json writes, divides it into parts of approximately
/// equal population, and writes the same graph back out with one extra node
/// attribute giving each node's part. The result is ready to use as a starting
/// partition:
///
///     graph = Graph.from_json("out.json")
///     partition = Partition(graph, "district", updaters)
///
/// Parts are numbered from zero, matching GerryChain's own
/// Partition.from_random_assignment.
#[derive(Parser, Debug)]
#[command(
    name = "partigraph-seedmap",
    version,
    about = "Generate a GerryChain seed map from a graph",
    verbatim_doc_comment
)]
struct Args {
    /// Graph to read, in networkx adjacency JSON format
    #[arg(short = 'g', long, value_name = "FILE")]
    graph: std::path::PathBuf,

    /// Where to write the seeded graph
    #[arg(short = 'o', long, value_name = "FILE")]
    output: std::path::PathBuf,

    /// Number of parts to divide the graph into
    #[arg(short = 'n', long, value_name = "N")]
    parts: usize,

    /// Node attribute holding each node's population
    #[arg(short = 'p', long, value_name = "ATTR")]
    pop_col: String,

    /// Allowed relative deviation from the ideal part population, so a part
    /// passes when |pop - ideal| <= epsilon * ideal. Same meaning as
    /// GerryChain's epsilon: 0.01 is one percent.
    #[arg(short = 'e', long, value_name = "FRACTION")]
    epsilon: f64,

    /// Node attribute to write each node's part into
    #[arg(short = 'a', long, value_name = "ATTR", default_value = "district")]
    assignment: String,

    /// Seed for the random number generator; the same seed reproduces the same
    /// seed map
    #[arg(short = 's', long, value_name = "N", default_value_t = 0)]
    seed: u64,

    /// Spanning-tree sampler to partition with
    #[arg(long, value_enum, default_value_t = SamplerArg::Uniform)]
    sampler: SamplerArg,

    /// Whole-graph attempts before giving up
    #[arg(long, value_name = "N", default_value_t = 1000)]
    max_attempts: usize,

    /// Transfers that fail to improve a division before moving on
    #[arg(long, value_name = "N", default_value_t = 10)]
    improve_limit: usize,

    /// Divisions tried at each level of the recursion
    #[arg(long, value_name = "N", default_value_t = 3)]
    backtrack: usize,

    /// Print only errors
    #[arg(short = 'q', long)]
    quiet: bool,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, ValueEnum)]
enum SamplerArg {
    /// Wilson's algorithm: uniform over spanning trees
    Uniform,
    /// Random-weight minimum spanning tree: faster, not uniform
    Mst,
}

impl From<SamplerArg> for Sampler {
    fn from(a: SamplerArg) -> Sampler {
        match a {
            SamplerArg::Uniform => Sampler::Uniform,
            SamplerArg::Mst => Sampler::MinWeightTree,
        }
    }
}

fn main() -> ExitCode {
    let args = Args::parse();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("partigraph-seedmap: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &Args) -> Result<(), partigraph_io::Error> {
    let graph = AdjacencyGraph::read(&args.graph)?;
    let say = |line: &str| {
        if !args.quiet {
            eprintln!("{line}");
        }
    };
    say(&format!(
        "read {}: {} nodes, {} edges",
        args.graph.display(),
        graph.node_count(),
        graph.edge_count()
    ));

    let opts = SeedMapOptions {
        parts: args.parts,
        population_attr: args.pop_col.clone(),
        epsilon: args.epsilon,
        assignment_attr: args.assignment.clone(),
        seed: args.seed,
        sampler: args.sampler.into(),
        search: SearchOptions {
            improve_limit: args.improve_limit,
            backtrack: args.backtrack,
            max_attempts: args.max_attempts,
        },
    };

    let (seeded, report) = build_seed_map(&graph, &opts)?;
    seeded.write(&args.output)?;

    if !args.quiet {
        let sizes = report.sizes();
        say(&format!(
            "ideal population {:.2} across {} parts, worst deviation {:.4} (limit {:.4})",
            report.ideal,
            args.parts,
            report.worst_deviation(),
            args.epsilon
        ));
        for (part, (pop, size)) in report.populations.iter().zip(&sizes).enumerate() {
            let deviation = if report.ideal == 0.0 {
                0.0
            } else {
                (pop - report.ideal) / report.ideal
            };
            say(&format!(
                "  part {part:>3}  population {pop:>12.2}  {deviation:>+8.4}  {size} nodes"
            ));
        }
        say(&format!(
            "wrote {} with \"{}\" on each node",
            args.output.display(),
            args.assignment
        ));
    }
    Ok(())
}
