use std::time::Duration;

use aegis::analysis::router::route;
use aegis::planning::{CwdState, PlanningRequest, PreparedPlanner};
use aegis::runtime::RuntimeContext;
use aegis_config::AegisConfig;
use aegis_policy::ExecutionTransport;
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use tempfile::TempDir;
use tokio::runtime::Runtime;

// This file measures the hot-path cost of `aegis::analysis::router::route`
// (PR #437: route every segment of a compound command, not only the first)
// plus, where the input shape allows it, the full per-command planning
// entry point that `shell_wrapper::run_shell_wrapper` and `--output json -c`
// both call: `PreparedPlanner::plan`. That entry point runs scanner assess,
// routing, and policy evaluation without spawning a worker subprocess or
// executing the command — routing to a `ScriptFile` or interpreter `Inline`
// target is what triggers a worker spawn (`src/analysis/orchestrate.rs`,
// ADR-022 §0: "when route yields no target, no subprocess is spawned").
// None of the safe commands below route to anything, so `plan()` stays on
// that no-spawn path. Group 4 does route to a target, so only `route()`
// itself is timed there; timing `plan()` on it would mostly measure worker
// spawn, not the router.
//
// Many small sub-benchmarks live in this file, so the group config below
// trims `measurement_time` and `warm_up_time` below Criterion's defaults —
// these functions run in low single-digit microseconds, so a shorter window
// still gives a stable median.

// ── Group 1: safe everyday commands, single segment ───────────────────────

const SAFE_SINGLE: &[(&str, &str)] = &[
    ("git_status", "git status"),
    ("ls_la", "ls -la"),
    ("cargo_test", "cargo test --workspace"),
    ("grep", "grep -rn foo src"),
    ("npm_build", "npm run build"),
];

// ── Group 2: safe compound commands ────────────────────────────────────────

const GH_PR_BODY: &str = "\
Summary
This change adds a router benchmark comparing route() before and after PR #437.

Motivation
PR #437 changed how compound commands are routed so every segment gets
evaluated instead of only the first one. That is a hot-path change and
needs a number attached to it, not just a description in the PR body.

What changed
- benches/router_bench.rs times route() against four command shapes
- the same file also times the full per-command planning entry point
- measurements come from two worktrees pinned to the commits before
  and after the merge, same inputs on both

Test plan
- cargo bench --bench router_bench on both worktrees
- compare medians against the 2 ms safe-path budget from AGENTS.md
- flag any group that regresses past 20% or crosses 100 microseconds

Notes
This PR only adds measurement code. It does not change router.rs or
any other production path.
";

fn gh_pr_create_command() -> String {
    format!("gh pr create --body \"$(cat <<'EOF'\n{GH_PR_BODY}EOF\n)\"")
}

fn safe_compound_commands() -> Vec<(&'static str, String)> {
    vec![
        ("cd_ls", "cd src && ls".to_string()),
        (
            "git_add_commit_push",
            r#"git add . && git commit -m "fix: x" && git push"#.to_string(),
        ),
        (
            "cargo_build_tail",
            "cargo build 2>&1 | tail -20".to_string(),
        ),
        (
            "find_xargs_wc",
            "find . -name '*.rs' | xargs wc -l".to_string(),
        ),
        ("gh_pr_create", gh_pr_create_command()),
    ]
}

// ── Group 3: long safe input ───────────────────────────────────────────────

/// 50 `&&`-joined `echo` segments — stresses segment-splitting, not string size.
fn long_chain_command() -> String {
    (0..50)
        .map(|i| format!("echo {i}"))
        .collect::<Vec<_>>()
        .join(" && ")
}

/// A single ~4 KB command — stresses string size, not segment count.
fn long_single_command() -> String {
    format!("echo {}", "x".repeat(4083))
}

// ── Group 4: interpreter shapes (not the fast path, for information) ──────

const INTERPRETER_SHAPES: &[(&str, &str)] = &[
    ("python_inline", "python3 -c 'print(1)'"),
    ("python_script", "true; python3 ./x.py"),
];

// ── route() benches ─────────────────────────────────────────────────────

fn bench_route_safe_single(c: &mut Criterion) {
    for (name, cmd) in SAFE_SINGLE {
        c.bench_function(&format!("route_safe_single_{name}"), |b| {
            b.iter(|| black_box(route(black_box(cmd), black_box(&[]))))
        });
    }
}

fn bench_route_safe_compound(c: &mut Criterion) {
    for (name, cmd) in safe_compound_commands() {
        c.bench_function(&format!("route_safe_compound_{name}"), |b| {
            b.iter(|| black_box(route(black_box(cmd.as_str()), black_box(&[]))))
        });
    }
}

fn bench_route_long_safe(c: &mut Criterion) {
    let chain = long_chain_command();
    c.bench_function("route_long_chain_50_echo", |b| {
        b.iter(|| black_box(route(black_box(chain.as_str()), black_box(&[]))))
    });

    let single = long_single_command();
    c.bench_function("route_long_single_4kb", |b| {
        b.iter(|| black_box(route(black_box(single.as_str()), black_box(&[]))))
    });
}

fn bench_route_interpreter_shapes(c: &mut Criterion) {
    for (name, cmd) in INTERPRETER_SHAPES {
        c.bench_function(&format!("route_interpreter_{name}"), |b| {
            b.iter(|| black_box(route(black_box(cmd), black_box(&[]))))
        });
    }
}

// ── full evaluation entry point benches ────────────────────────────────────
//
// `PreparedPlanner::plan` is the same call `run_shell_wrapper` makes for
// `aegis --output json -c <cmd>`. Building `RuntimeContext` directly with a
// default config (rather than going through `prepare_planner`, which reads
// config from disk) isolates the per-command cost from one-time process
// setup, the same split `scanner_bench.rs` and `startup_bench.rs` already
// make between construction and per-call cost.

fn make_prepared_planner() -> (Runtime, PreparedPlanner, TempDir) {
    let runtime = Runtime::new().expect("tokio runtime must start");
    let handle = runtime.handle().clone();
    let context = RuntimeContext::new(AegisConfig::default(), handle)
        .expect("runtime context must build from a default config");
    let cwd = TempDir::new().expect("temp cwd must be created");
    (runtime, PreparedPlanner::Ready(Box::new(context)), cwd)
}

fn evaluate(prepared: &PreparedPlanner, cwd_state: &CwdState, cmd: &str) {
    black_box(prepared.plan(PlanningRequest {
        command: black_box(cmd),
        cwd_state: cwd_state.clone(),
        transport: ExecutionTransport::Evaluation,
        ci_detected: false,
    }));
}

fn bench_evaluate_safe_single(c: &mut Criterion) {
    let (_runtime, prepared, cwd) = make_prepared_planner();
    let cwd_state = CwdState::Resolved(cwd.path().to_path_buf());

    for (name, cmd) in SAFE_SINGLE {
        c.bench_function(&format!("evaluate_safe_single_{name}"), |b| {
            b.iter(|| evaluate(&prepared, &cwd_state, cmd))
        });
    }
}

fn bench_evaluate_safe_compound(c: &mut Criterion) {
    let (_runtime, prepared, cwd) = make_prepared_planner();
    let cwd_state = CwdState::Resolved(cwd.path().to_path_buf());

    for (name, cmd) in safe_compound_commands() {
        c.bench_function(&format!("evaluate_safe_compound_{name}"), |b| {
            b.iter(|| evaluate(&prepared, &cwd_state, cmd.as_str()))
        });
    }
}

fn bench_evaluate_long_safe(c: &mut Criterion) {
    let (_runtime, prepared, cwd) = make_prepared_planner();
    let cwd_state = CwdState::Resolved(cwd.path().to_path_buf());

    let chain = long_chain_command();
    c.bench_function("evaluate_long_chain_50_echo", |b| {
        b.iter(|| evaluate(&prepared, &cwd_state, chain.as_str()))
    });

    let single = long_single_command();
    c.bench_function("evaluate_long_single_4kb", |b| {
        b.iter(|| evaluate(&prepared, &cwd_state, single.as_str()))
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(3))
        .warm_up_time(Duration::from_secs(1));
    targets = bench_route_safe_single,
        bench_route_safe_compound,
        bench_route_long_safe,
        bench_route_interpreter_shapes,
        bench_evaluate_safe_single,
        bench_evaluate_safe_compound,
        bench_evaluate_long_safe
}
criterion_main!(benches);
