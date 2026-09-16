use std::process::Command;
use std::time::Duration;

use aegis::config::AegisConfig;
use aegis::interceptor::patterns::PatternSet;
use aegis::interceptor::scanner::Scanner;
use aegis::runtime::context::RuntimeContext;
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use tempfile::TempDir;
use tokio::runtime::Runtime;

// Group 1: sub-benches that each cost a few milliseconds, so the defaults are
// fine — the 8s `measurement_time` in `scanner_bench` exists there only
// because that bench times 1,000 `assess` calls per iteration.

fn bench_scanner_construction(c: &mut Criterion) {
    // Production reaches the scanner through `interceptor::scanner_for` →
    // `builtin_scanner()`, which returns `Arc::clone` of the process-wide
    // `BUILTIN_SCANNER` static (`src/interceptor/mod.rs:64-69`). That static
    // cannot be re-initialised in a loop, so this bench models the
    // one-per-process cost rather than repeating the production call. For a
    // `$SHELL` proxy, one per process is one per command.
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

    // A default config has no `custom_patterns`, so this path gets a cloned
    // `Arc` from the already-warm `BUILTIN_SCANNER` static instead of
    // building a scanner. This row and `scanner_construction` therefore
    // measure disjoint work and can be added together.
    c.bench_function("runtime_context_construction", |b| {
        b.iter(|| {
            black_box(
                RuntimeContext::new(AegisConfig::default(), handle.clone())
                    .expect("runtime context must build from a default config"),
            )
        })
    });
}

criterion_group! {
    name = construction_benches;
    config = Criterion::default();
    targets = bench_scanner_construction, bench_runtime_context_construction
}

// Group 2: one full process invocation per iteration, dominated by process
// spawn and teardown rather than Aegis's own work — a shorter sample count
// and measurement window keeps the bench itself fast.

fn bench_startup_safe_command(c: &mut Criterion) {
    let home = TempDir::new().expect("temp HOME must be created");

    c.bench_function("startup_safe_command", |b| {
        b.iter(|| {
            let output = Command::new(env!("CARGO_BIN_EXE_aegis"))
                .args(["-c", "ls -la", "--output", "json"])
                .env("HOME", home.path())
                .current_dir(home.path())
                // `is_ci_environment` (`src/runtime_gate.rs:19-27`) returns
                // false on `AEGIS_CI=0` before every other check, so this is
                // cheaper and more reliable than clearing seven CI variables.
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
