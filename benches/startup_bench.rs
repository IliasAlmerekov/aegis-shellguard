use std::hint::black_box;
use std::process::Command;
use std::time::Duration;

use aegis::runtime::context::RuntimeContext;
use aegis_config::{AegisConfig, UserPattern};
use aegis_scanner::PatternSet;
use aegis_scanner::Scanner;
use aegis_types::{Category, RiskLevel};
use criterion::{Criterion, criterion_group, criterion_main};
use tempfile::TempDir;
use tokio::runtime::Runtime;

fn bench_scanner_construction(c: &mut Criterion) {
    // Models the one-per-process `BUILTIN_SCANNER` build cost (static cannot re-init in a loop).
    c.bench_function("scanner_construction", |b| {
        b.iter(|| {
            let patterns = PatternSet::load().expect("patterns.toml must load");
            black_box(Scanner::try_new(patterns).expect("built-in patterns compile"))
        })
    });
}

fn bench_runtime_context_construction(c: &mut Criterion) {
    let runtime = Runtime::new().expect("tokio runtime must start");
    let handle = runtime.handle().clone();

    // Default config clones the warm `BUILTIN_SCANNER` Arc, disjoint from `scanner_construction`.
    c.bench_function("runtime_context_construction", |b| {
        b.iter(|| {
            black_box(
                RuntimeContext::new(AegisConfig::default(), handle.clone())
                    .expect("runtime context must build from a default config"),
            )
        })
    });
}

fn bench_runtime_context_custom_pattern_construction(c: &mut Criterion) {
    let runtime = Runtime::new().expect("tokio runtime must start");
    let handle = runtime.handle().clone();

    // One custom pattern triggers a full scanner rebuild merging built-ins and custom patterns.
    let mut config = AegisConfig::default();
    config.custom_patterns = vec![UserPattern {
        id: "USR-BENCH-001".to_string(),
        category: Category::Cloud,
        risk: RiskLevel::Warn,
        pattern: "internal-teardown".to_string(),
        description: "custom warning".to_string(),
        safe_alt: None,
        justification: None,
    }];

    c.bench_function("runtime_context_custom_pattern_construction", |b| {
        b.iter(|| {
            black_box(
                RuntimeContext::new(config.clone(), handle.clone())
                    .expect("runtime context must build from a config with one custom pattern"),
            )
        })
    });
}

// The Sandbox-enabled hot path: the `.git` search plus the ceiling
// computation, from a directory nested six levels below the worktree root.
fn bench_sandbox_ceiling_nested_worktree(c: &mut Criterion) {
    let repo = TempDir::new().expect("temp repo must be created");
    std::fs::create_dir(repo.path().join(".git")).expect(".git must be created");
    let cwd = repo.path().join("a/b/c/d/e/f");
    std::fs::create_dir_all(&cwd).expect("nested dir must be created");
    let mut config = AegisConfig::default();
    config.sandbox.enabled = true;

    c.bench_function("sandbox_ceiling_nested_worktree", |b| {
        b.iter(|| black_box(aegis::runtime::sandbox_ceiling(&config, black_box(&cwd))))
    });
}

criterion_group! {
    name = construction_benches;
    config = Criterion::default();
    targets = bench_scanner_construction, bench_runtime_context_construction, bench_runtime_context_custom_pattern_construction, bench_sandbox_ceiling_nested_worktree
}

// Full process spawn per iteration; shorter sample/measurement window keeps bench fast.

fn bench_startup_safe_command(c: &mut Criterion) {
    let home = TempDir::new().expect("temp HOME must be created");

    c.bench_function("startup_safe_command", |b| {
        b.iter(|| {
            let output = Command::new(env!("CARGO_BIN_EXE_aegis"))
                .args(["-c", "ls -la", "--output", "json"])
                .env("HOME", home.path())
                .current_dir(home.path())
                // `AEGIS_CI=0` short-circuits `is_ci_environment` without clearing every CI variable.
                .env("AEGIS_CI", "0")
                .output()
                .expect("aegis must spawn and run to completion");

            if !output.status.success() {
                panic!(
                    "aegis exited with {:?}; stderr:\n{}",
                    output.status.code(),
                    String::from_utf8_lossy(&output.stderr)
                );
            }

            black_box(output);
        })
    });
}

criterion_group! {
    name = process_benches;
    config = Criterion::default()
        .sample_size(25)
        .measurement_time(Duration::from_secs(5))
        .warm_up_time(Duration::from_secs(1));
    targets = bench_startup_safe_command
}

criterion_main!(construction_benches, process_benches);
