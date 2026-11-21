//! Benchmark harness for the no-source contract (ADR-022 §1).
//!
//! Measures [`aegis_language::worker::analyze`] over `NO_SOURCE` and asserts
//! [`Outcome::NotStarted`] on every entry. The recorded mean is gated by the
//! `no_source_does_not_start_worker` row in `perf/scanner_bench_baseline.toml`;
//! adding `NO_SOURCE` entries raises the mean, so rebaseline that row in the
//! same change. See `docs/performance-baseline.md`.

use aegis_language::worker::{Outcome, analyze};
use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;

// The no-source corpus is shared verbatim with `tests/no_source.rs` (a
// `tests/common/` subdirectory file, not its own test target) so the bench and
// the contract test cannot drift. `include!` inlines the `pub const NO_SOURCE`.
include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/common/no_source_corpus.rs"
));

fn no_source_does_not_start_worker(c: &mut Criterion) {
    c.bench_function("no_source_does_not_start_worker", |b| {
        b.iter(|| {
            for cmd in NO_SOURCE {
                let outcome = analyze(black_box(*cmd));
                assert_eq!(
                    outcome,
                    Outcome::NotStarted,
                    "no-source command `{cmd}` started the language worker — \
                     regression of ADR-022 Iteration 0 RED #3 (the no-source \
                     path must not spawn a worker or perform filesystem metadata \
                     calls)"
                );
            }
        })
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().measurement_time(Duration::from_secs(3));
    targets = no_source_does_not_start_worker
}
criterion_main!(benches);
