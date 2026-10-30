//! Project automation. Run with `cargo xtask <command>`.
//!
//! `cargo xtask ci` is the quality gate shared by local runs and GitHub
//! Actions: format check, clippy (warnings are errors), tests, and the
//! crate boundary check.

use std::collections::{BTreeSet, HashSet, VecDeque};
use std::env;
use std::process::{Command, ExitCode};

use cargo_metadata::{DependencyKind, Metadata, MetadataCommand, PackageId};

/// Workspace crates each library crate may depend on directly.
/// The CLI binary (`bitturbulent`) and `xtask` are unrestricted.
const ALLOWED_INTERNAL_DEPS: &[(&str, &[&str])] = &[
    ("bt-core", &[]),
    ("bt-metainfo", &["bt-core"]),
    ("bt-engine", &["bt-core", "bt-metainfo"]),
    ("bt-session", &["bt-core", "bt-metainfo", "bt-engine"]),
];

/// Crates whose non-dev dependency tree must stay free of async runtimes.
const RUNTIME_FREE: &[&str] = &["bt-core", "bt-metainfo"];

/// Async runtimes that must not appear in a `RUNTIME_FREE` crate's tree.
const ASYNC_RUNTIMES: &[&str] = &["tokio", "async-std", "smol"];

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("ci") => ci(),
        _ => {
            eprintln!("usage: cargo xtask ci");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("xtask: {msg}");
            ExitCode::FAILURE
        }
    }
}

fn ci() -> Result<(), String> {
    cargo(&["fmt", "--all", "--check"])?;
    cargo(&[
        "clippy",
        "--workspace",
        "--all-targets",
        "--",
        "-D",
        "warnings",
    ])?;
    cargo(&["test", "--workspace"])?;
    step("boundaries");
    check_boundaries()?;
    eprintln!("xtask: all checks passed");
    Ok(())
}

fn step(name: &str) {
    eprintln!("\n==> {name}");
}

fn cargo(args: &[&str]) -> Result<(), String> {
    step(&format!("cargo {}", args.join(" ")));
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = Command::new(&cargo)
        .args(args)
        .status()
        .map_err(|e| format!("failed to run {cargo}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`cargo {}` failed ({status})", args.join(" ")))
    }
}

fn check_boundaries() -> Result<(), String> {
    let metadata = MetadataCommand::new()
        .exec()
        .map_err(|e| format!("cargo metadata failed: {e}"))?;
    let mut errors = Vec::new();

    for &(krate, allowed) in ALLOWED_INTERNAL_DEPS {
        let id = package_id(&metadata, krate)?;
        for dep in internal_deps(&metadata, id) {
            if !allowed.contains(&dep.as_str()) {
                errors.push(format!("{krate} must not depend on {dep}"));
            }
        }
    }

    for &krate in RUNTIME_FREE {
        let id = package_id(&metadata, krate)?;
        for name in non_dev_closure(&metadata, id) {
            if ASYNC_RUNTIMES.contains(&name.as_str()) {
                errors.push(format!("{krate} must not depend on async runtime `{name}`"));
            }
        }
    }

    if errors.is_empty() {
        eprintln!("crate boundaries ok");
        Ok(())
    } else {
        for e in &errors {
            eprintln!("  {e}");
        }
        Err(format!("{} boundary violation(s)", errors.len()))
    }
}

fn package_id<'a>(metadata: &'a Metadata, name: &str) -> Result<&'a PackageId, String> {
    metadata
        .workspace_packages()
        .into_iter()
        .find(|p| p.name.as_str() == name)
        .map(|p| &p.id)
        .ok_or_else(|| format!("workspace package `{name}` not found"))
}

fn resolve_node<'a>(metadata: &'a Metadata, id: &PackageId) -> &'a cargo_metadata::Node {
    metadata
        .resolve
        .as_ref()
        .expect("cargo metadata always resolves dependencies by default")
        .nodes
        .iter()
        .find(|n| &n.id == id)
        .expect("every package has a resolve node")
}

/// Non-dev dependencies of `id` that are workspace members.
fn internal_deps(metadata: &Metadata, id: &PackageId) -> BTreeSet<String> {
    let members: HashSet<&PackageId> = metadata.workspace_members.iter().collect();
    resolve_node(metadata, id)
        .deps
        .iter()
        .filter(|d| is_non_dev(d) && members.contains(&d.pkg))
        .map(|d| metadata[&d.pkg].name.as_str().to_owned())
        .collect()
}

/// Names of every package reachable from `id` through normal or build deps.
fn non_dev_closure(metadata: &Metadata, id: &PackageId) -> BTreeSet<String> {
    let mut seen = HashSet::new();
    let mut names = BTreeSet::new();
    let mut queue = VecDeque::from([id.clone()]);
    while let Some(current) = queue.pop_front() {
        for dep in resolve_node(metadata, &current)
            .deps
            .iter()
            .filter(|d| is_non_dev(d))
        {
            if seen.insert(dep.pkg.clone()) {
                names.insert(metadata[&dep.pkg].name.as_str().to_owned());
                queue.push_back(dep.pkg.clone());
            }
        }
    }
    names
}

fn is_non_dev(dep: &cargo_metadata::NodeDep) -> bool {
    dep.dep_kinds
        .iter()
        .any(|k| k.kind != DependencyKind::Development)
}
