//! The binary, driven the way a user drives it.

use std::path::PathBuf;
use std::process::{Command, Output};

const FIXTURE: &str = include_str!("../../partigraph-io/tests/fixtures/gerrymandria.json");
const EXE: &str = env!("CARGO_BIN_EXE_partigraph-seedmap");

fn tmp(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name)
}

fn input() -> PathBuf {
    let path = tmp("gerrymandria.json");
    std::fs::write(&path, FIXTURE).unwrap();
    path
}

fn run(args: &[&str]) -> Output {
    Command::new(EXE)
        .args(args)
        .output()
        .expect("failed to run the binary")
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn writes_a_seed_map() {
    let out_path = tmp("seeded.json");
    let _ = std::fs::remove_file(&out_path);
    let out = run(&[
        "--graph",
        input().to_str().unwrap(),
        "--output",
        out_path.to_str().unwrap(),
        "--parts",
        "4",
        "--pop-col",
        "TOTPOP",
        "--epsilon",
        "0",
        "--seed",
        "42",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));

    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out_path).unwrap()).unwrap();
    let nodes = written["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 64);
    let mut parts: Vec<i64> = nodes
        .iter()
        .map(|n| n["district"].as_i64().unwrap())
        .collect();
    parts.sort_unstable();
    parts.dedup();
    assert_eq!(parts, vec![0, 1, 2, 3]);

    // The report goes to stderr, leaving stdout clean for piping.
    assert!(out.stdout.is_empty(), "stdout should be empty");
    let report = stderr(&out);
    assert!(report.contains("64 nodes"), "{report}");
    assert!(report.contains("ideal population 16.00"), "{report}");
}

#[test]
fn short_flags_work_too() {
    let out_path = tmp("seeded_short.json");
    let out = run(&[
        "-g",
        input().to_str().unwrap(),
        "-o",
        out_path.to_str().unwrap(),
        "-n",
        "2",
        "-p",
        "TOTPOP",
        "-e",
        "0",
        "-s",
        "1",
        "-q",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stderr(&out).is_empty(), "--quiet should silence the report");
    assert!(out_path.exists());
}

#[test]
fn the_assignment_attribute_can_be_renamed() {
    let out_path = tmp("seeded_named.json");
    let out = run(&[
        "-g",
        input().to_str().unwrap(),
        "-o",
        out_path.to_str().unwrap(),
        "-n",
        "2",
        "-p",
        "TOTPOP",
        "-e",
        "0",
        "--assignment",
        "seed_part",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out_path).unwrap()).unwrap();
    assert!(written["nodes"][0]["seed_part"].is_i64());
    assert!(stderr(&out).contains("seed_part"));
}

#[test]
fn the_same_seed_reproduces_the_same_file() {
    let go = |name: &str, seed: &str| {
        let p = tmp(name);
        let out = run(&[
            "-g",
            input().to_str().unwrap(),
            "-o",
            p.to_str().unwrap(),
            "-n",
            "4",
            "-p",
            "TOTPOP",
            "-e",
            "0",
            "-s",
            seed,
            "-q",
        ]);
        assert!(out.status.success(), "{}", stderr(&out));
        std::fs::read_to_string(&p).unwrap()
    };
    assert_eq!(go("rep_a.json", "5"), go("rep_b.json", "5"));
    assert_ne!(go("rep_a.json", "5"), go("rep_c.json", "6"));
}

#[test]
fn both_samplers_are_selectable() {
    for sampler in ["uniform", "mst"] {
        let out_path = tmp(&format!("seeded_{sampler}.json"));
        let out = run(&[
            "-g",
            input().to_str().unwrap(),
            "-o",
            out_path.to_str().unwrap(),
            "-n",
            "4",
            "-p",
            "TOTPOP",
            "-e",
            "0",
            "--sampler",
            sampler,
            "-q",
        ]);
        assert!(out.status.success(), "{sampler}: {}", stderr(&out));
    }
    let bad = run(&[
        "-g",
        input().to_str().unwrap(),
        "-o",
        tmp("x.json").to_str().unwrap(),
        "-n",
        "4",
        "-p",
        "TOTPOP",
        "-e",
        "0",
        "--sampler",
        "nonsense",
    ]);
    assert!(!bad.status.success());
    assert!(
        stderr(&bad).contains("uniform"),
        "the error should list the choices"
    );
}

#[test]
fn failures_exit_nonzero_and_explain_themselves() {
    let cases: Vec<(&str, Vec<&str>, &str)> = vec![
        (
            "missing population column",
            vec!["-n", "4", "-p", "NOPE", "-e", "0"],
            "NOPE",
        ),
        (
            "impossible tolerance",
            vec!["-n", "5", "-p", "TOTPOP", "-e", "0", "--max-attempts", "5"],
            "attempts",
        ),
        (
            "more parts than nodes",
            vec!["-n", "9999", "-p", "TOTPOP", "-e", "0.5"],
            "9999",
        ),
    ];
    let graph = input();
    for (name, extra, expected) in cases {
        let out_path = tmp("should_not_appear.json");
        let _ = std::fs::remove_file(&out_path);
        let mut args = vec![
            "-g",
            graph.to_str().unwrap(),
            "-o",
            out_path.to_str().unwrap(),
        ];
        args.extend(extra);
        let out = run(&args);
        assert!(!out.status.success(), "{name} should have failed");
        let msg = stderr(&out);
        assert!(
            msg.contains(expected),
            "{name}: expected {expected:?} in: {msg}"
        );
        assert!(
            !out_path.exists(),
            "{name} wrote an output file despite failing"
        );
    }
}

#[test]
fn a_missing_input_file_is_reported_clearly() {
    let out = run(&[
        "-g",
        "/no/such/graph.json",
        "-o",
        tmp("y.json").to_str().unwrap(),
        "-n",
        "2",
        "-p",
        "TOTPOP",
        "-e",
        "0",
    ]);
    assert!(!out.status.success());
    let msg = stderr(&out);
    assert!(msg.contains("/no/such/graph.json"), "{msg}");
}

#[test]
fn required_arguments_are_required() {
    let out = run(&["--graph", input().to_str().unwrap()]);
    assert!(!out.status.success());
    let msg = stderr(&out);
    for flag in ["--output", "--parts", "--pop-col", "--epsilon"] {
        assert!(msg.contains(flag), "the error should name {flag}: {msg}");
    }
}

#[test]
fn help_documents_every_parameter() {
    let out = run(&["--help"]);
    assert!(out.status.success());
    let help = String::from_utf8_lossy(&out.stdout);
    for flag in [
        "--graph",
        "--output",
        "--parts",
        "--pop-col",
        "--epsilon",
        "--assignment",
        "--seed",
        "--sampler",
        "--max-attempts",
        "--improve-limit",
        "--backtrack",
        "--quiet",
    ] {
        assert!(help.contains(flag), "--help does not mention {flag}");
    }
    assert!(
        help.contains("GerryChain"),
        "--help should say what the output is for"
    );
    assert!(run(&["--version"]).status.success());
}
