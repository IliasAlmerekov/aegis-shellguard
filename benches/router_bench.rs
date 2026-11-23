use std::hint::black_box;
use std::time::Duration;

use aegis::analysis::router::route;
use aegis::planning::{CwdState, PlanningOutcome, PlanningRequest, PreparedPlanner};
use aegis::runtime::RuntimeContext;
use aegis_config::AegisConfig;
use aegis_policy::ExecutionTransport;
use criterion::{Criterion, criterion_group, criterion_main};
use tempfile::TempDir;
use tokio::runtime::Runtime;

// ADR-022 §0: `PreparedPlanner::plan` spawns no worker when `route` yields no target.
// Group 4 routes to targets, so only `route` is timed there.

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

// Build `RuntimeContext` directly to time per-command planning without config disk I/O.

fn make_prepared_planner() -> (Runtime, PreparedPlanner, TempDir) {
    let runtime = Runtime::new().expect("tokio runtime must start");
    let handle = runtime.handle().clone();
    let context = RuntimeContext::new(AegisConfig::default(), handle)
        .expect("runtime context must build from a default config");
    let cwd = TempDir::new().expect("temp cwd must be created");
    (runtime, PreparedPlanner::Ready(Box::new(context)), cwd)
}

fn planning_request<'a>(cwd_state: &CwdState, cmd: &'a str) -> PlanningRequest<'a> {
    PlanningRequest {
        command: black_box(cmd),
        cwd_state: cwd_state.clone(),
        transport: ExecutionTransport::Evaluation,
        ci_detected: false,
    }
}

fn assert_worker_free_plan(prepared: &PreparedPlanner, cwd_state: &CwdState, cmd: &str) {
    let PlanningOutcome::Planned(plan) = prepared.plan(planning_request(cwd_state, cmd)) else {
        panic!("router benchmark command did not produce a plan: {cmd}");
    };
    assert!(
        plan.assessment().analysis.is_none(),
        "router benchmark command unexpectedly started language analysis: {cmd}"
    );
}

fn evaluate(prepared: &PreparedPlanner, cwd_state: &CwdState, cmd: &str) {
    black_box(prepared.plan(planning_request(cwd_state, cmd)));
}

fn bench_evaluate_safe_single(c: &mut Criterion) {
    let (_runtime, prepared, cwd) = make_prepared_planner();
    let cwd_state = CwdState::Resolved(cwd.path().to_path_buf());

    for (name, cmd) in SAFE_SINGLE {
        assert_worker_free_plan(&prepared, &cwd_state, cmd);
        c.bench_function(&format!("evaluate_safe_single_{name}"), |b| {
            b.iter(|| evaluate(&prepared, &cwd_state, cmd))
        });
    }
}

fn bench_evaluate_safe_compound(c: &mut Criterion) {
    let (_runtime, prepared, cwd) = make_prepared_planner();
    let cwd_state = CwdState::Resolved(cwd.path().to_path_buf());

    for (name, cmd) in safe_compound_commands() {
        assert_worker_free_plan(&prepared, &cwd_state, cmd.as_str());
        c.bench_function(&format!("evaluate_safe_compound_{name}"), |b| {
            b.iter(|| evaluate(&prepared, &cwd_state, cmd.as_str()))
        });
    }
}

fn bench_evaluate_long_safe(c: &mut Criterion) {
    let (_runtime, prepared, cwd) = make_prepared_planner();
    let cwd_state = CwdState::Resolved(cwd.path().to_path_buf());

    let chain = long_chain_command();
    assert_worker_free_plan(&prepared, &cwd_state, chain.as_str());
    c.bench_function("evaluate_long_chain_50_echo", |b| {
        b.iter(|| evaluate(&prepared, &cwd_state, chain.as_str()))
    });

    let single = long_single_command();
    assert_worker_free_plan(&prepared, &cwd_state, single.as_str());
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
