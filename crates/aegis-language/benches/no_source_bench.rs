use aegis_language::worker::{Outcome, analyze};
use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;

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
                     the no-source path must not spawn \
                     a worker or perform filesystem metadata calls"
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
