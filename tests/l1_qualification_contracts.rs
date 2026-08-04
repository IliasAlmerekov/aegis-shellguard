//! Iteration 10 qualification contracts for the supported grammar set.
//!
//! These tests exercise the release-facing seams: Cargo dependency pins,
//! distributed third-party notices, and the public analysis budget. They use
//! ADR-022's four-language L1 set as independent expectations rather than
//! deriving assertions from the production manifest.

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use aegis::analysis::OrchestrationBudget;
use aegis::analysis::queue::QueueBudget;
use aegis::config::AegisConfig;
use aegis::runtime::RuntimeConfig;
use aegis_language::manifest::BUILTIN_MANIFEST;

const QUALIFIED_GRAMMARS: &[(&str, &str, &str)] = &[
    ("python", "tree-sitter-python", "0.25.0"),
    ("javascript", "tree-sitter-javascript", "0.25.0"),
    ("typescript", "tree-sitter-typescript", "0.23.2"),
    ("bash", "tree-sitter-bash", "0.25.1"),
];

/// Every statically linked Tree-sitter component the notice must attribute, as
/// `(crate, upstream, SPDX expression, copyright notice)`. Versions are
/// deliberately absent: they are read from `Cargo.lock` so that bumping a
/// component and forgetting to refresh the shipped notice fails instead of
/// silently publishing stale attribution.
///
/// Upstream URLs, SPDX expressions, and copyright lines are independent legal
/// facts and stay hardcoded. Five of the six copyright lines are transcribed from
/// the vendored crate's own `LICENSE`; the published `tree-sitter-typescript`
/// crate ships no `LICENSE`, so its line comes from the upstream tag.
///
/// The `tree-sitter` runtime's SPDX expression is *not* the crate's declared
/// `MIT`: it vendors an ICU subset under `src/unicode/` that is compiled into
/// every release binary, so the shipped notice must also carry the Unicode
/// license. `cargo deny` reads only the declared `MIT` and cannot catch that.
const NOTICE_COMPONENTS: &[(&str, &str, &str, &str)] = &[
    (
        "tree-sitter",
        "https://github.com/tree-sitter/tree-sitter",
        "MIT AND Unicode-DFS-2016",
        "Copyright (c) 2018 Max Brunsfeld",
    ),
    (
        "tree-sitter-python",
        "https://github.com/tree-sitter/tree-sitter-python",
        "MIT",
        "Copyright (c) 2016 Max Brunsfeld",
    ),
    (
        "tree-sitter-javascript",
        "https://github.com/tree-sitter/tree-sitter-javascript",
        "MIT",
        "Copyright (c) 2014 Max Brunsfeld",
    ),
    (
        "tree-sitter-typescript",
        "https://github.com/tree-sitter/tree-sitter-typescript",
        "MIT",
        "Copyright (c) 2017 Max Brunsfeld",
    ),
    (
        "tree-sitter-bash",
        "https://github.com/tree-sitter/tree-sitter-bash",
        "MIT",
        "Copyright (c) 2017 Max Brunsfeld",
    ),
    (
        "tree-sitter-language",
        "https://github.com/tree-sitter/tree-sitter",
        "MIT",
        "Copyright (c) 2018 Max Brunsfeld",
    ),
];

/// Tree-sitter crates that appear in `Cargo.lock` but are *not* linked into the
/// release binary (dev- or build-dependencies only), and therefore must not be
/// claimed as attributed components. Empty today; the tripwire below explains
/// when to extend it.
const NOT_LINKED_TREE_SITTER_CRATES: &[&str] = &[];

fn repo_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn read_repo_file(relative: &str) -> String {
    let path = repo_path(relative);
    fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("{} must be readable: {error}", path.display());
    })
}

/// Resolve the version `Cargo.lock` records for `package`.
///
/// `Cargo.lock` lists one `[[package]]` table per resolved crate, with `name`
/// immediately followed by `version`. `deny.toml` only *warns* on duplicate
/// versions, so two entries for one name are possible; picking either silently
/// would ship an attribution row for a version that may not be the linked one,
/// so this panics instead and forces a human decision.
fn locked_version(lockfile: &str, package: &str) -> String {
    let name_line = format!("name = \"{package}\"");
    let mut lines = lockfile.lines().peekable();
    let mut versions = Vec::new();

    while let Some(line) = lines.next() {
        if line.trim() != name_line {
            continue;
        }
        let version = lines
            .peek()
            .and_then(|next| next.trim().strip_prefix("version = \""))
            .and_then(|rest| rest.strip_suffix('"'))
            .unwrap_or_else(|| {
                panic!("Cargo.lock entry for `{package}` must be followed by its version")
            });
        versions.push(version.to_owned());
    }

    match versions.as_slice() {
        [version] => version.clone(),
        [] => panic!("Cargo.lock must resolve a version for `{package}`"),
        many => panic!(
            "Cargo.lock resolves {} versions of `{package}` ({}); attribution must name the \
             version actually linked, so deduplicate the graph or pin the row explicitly",
            many.len(),
            many.join(", ")
        ),
    }
}

#[test]
fn qualified_grammar_metadata_matches_cargo_pins_and_distributed_notices() {
    let cargo_toml = read_repo_file("crates/aegis-language/Cargo.toml");
    let lockfile = read_repo_file("Cargo.lock");
    let notices = read_repo_file("THIRD_PARTY_NOTICES.md");

    assert_eq!(BUILTIN_MANIFEST.len(), QUALIFIED_GRAMMARS.len());
    for &(language, crate_name, version) in QUALIFIED_GRAMMARS {
        let entry = BUILTIN_MANIFEST
            .iter()
            .find(|entry| entry.language == language)
            .unwrap_or_else(|| panic!("manifest must qualify `{language}`"));
        assert_eq!(entry.crate_name, crate_name);
        assert_eq!(entry.version, version);
        assert_eq!(entry.license_spdx, "MIT");

        let cargo_pin = format!(r#"{crate_name} = "={version}""#);
        assert!(
            cargo_toml.contains(&cargo_pin),
            "Cargo must pin `{crate_name}` exactly at {version}"
        );
    }

    // The Tree-sitter runtime is pinned directly, unlike `tree-sitter-language`,
    // which is only reachable transitively through the grammar crates.
    let runtime_version = locked_version(&lockfile, "tree-sitter");
    assert!(
        cargo_toml.contains(&format!(r#"tree-sitter = "={runtime_version}""#)),
        "Cargo must pin the Tree-sitter runtime exactly at the locked {runtime_version}"
    );

    for &(crate_name, upstream, spdx, copyright) in NOTICE_COMPONENTS {
        let version = locked_version(&lockfile, crate_name);
        let row = format!("| `{crate_name}` | `{version}` | <{upstream}> | {spdx} | {copyright} |");
        assert!(
            notices.contains(&row),
            "third-party notices must attribute `{crate_name}` at the locked {version}: \
             expected row `{row}`"
        );
    }
    assert!(
        notices.contains("Permission is hereby granted, free of charge"),
        "third-party notices must include the MIT permission notice"
    );

    // The runtime's vendored ICU headers are compiled into every release binary,
    // so the Unicode notice must ship with it — the crate's declared `MIT` alone
    // understates the obligation and no tooling flags the difference.
    assert!(
        notices.contains("COPYRIGHT AND PERMISSION NOTICE (ICU 58 and later)")
            && notices.contains("Copyright © 1991-2019 Unicode, Inc. All rights reserved."),
        "third-party notices must reproduce the Unicode notice for the ICU subset vendored \
         by the tree-sitter runtime"
    );
    // The vendored headers name IBM alongside Unicode, Inc.; dropping either
    // copyright holder under-attributes code that ships in every binary.
    assert!(
        notices.contains("Copyright (C) 1999-2015, International Business Machines"),
        "third-party notices must name IBM, the second copyright holder carried by the \
         vendored ICU headers"
    );
    for icu_header in ["utf8.h", "utf16.h", "umachine.h"] {
        assert!(
            notices.contains(icu_header),
            "third-party notices must identify the vendored ICU header `{icu_header}`"
        );
    }
    // Pinning the ICU commit makes the provenance a visible, reviewable fact
    // rather than untracked prose: a re-vendored subset must be re-verified, not
    // silently inherited. `src/unicode/ICU_SHA` in the crate is the source.
    assert!(
        notices.contains("552b01f61127d30d6589aa4bf99468224979b661"),
        "third-party notices must pin the vendored ICU commit"
    );

    // Guard the other direction too: a newly resolved Tree-sitter crate must be
    // triaged, not silently left out. This is a tripwire over `Cargo.lock`, which
    // also contains dev- and build-dependencies — so a hit is not proof of
    // release-binary linkage, and the row must not be added blindly.
    let mut untriaged: Vec<&str> = lockfile
        .lines()
        .filter_map(|line| line.trim().strip_prefix("name = \""))
        .filter_map(|rest| rest.strip_suffix('"'))
        .filter(|name| name.starts_with("tree-sitter"))
        .filter(|name| {
            !NOTICE_COMPONENTS
                .iter()
                .any(|&(crate_name, ..)| crate_name == *name)
                && !NOT_LINKED_TREE_SITTER_CRATES.contains(name)
        })
        .collect();
    untriaged.sort_unstable();
    untriaged.dedup();
    assert!(
        untriaged.is_empty(),
        "Cargo.lock resolves untriaged Tree-sitter crate(s) {untriaged:?}. Check whether they \
         reach the release binary (`cargo tree --edges normal -p aegis`): if so, attribute them \
         in THIRD_PARTY_NOTICES.md and in NOTICE_COMPONENTS; if they are only dev/build \
         dependencies, list them in NOT_LINKED_TREE_SITTER_CRATES — do not add an attribution \
         row for a crate the binary does not link"
    );
}

#[test]
fn config_defaults_and_public_budget_constants_agree_with_adr022() {
    let orchestration = OrchestrationBudget::L1_DEFAULT;
    assert_eq!(orchestration.inline_source_limit_bytes, 16 * 1024);
    assert_eq!(orchestration.script_file_limit_bytes, 256 * 1024);
    assert_eq!(orchestration.max_script_files, 8);
    assert_eq!(orchestration.max_aggregate_bytes, 1024 * 1024);
    assert_eq!(orchestration.total_timeout.as_millis(), 100);

    let queue = QueueBudget::L1_DEFAULT;
    assert_eq!(queue.max_depth, 8);
    assert_eq!(queue.max_targets, 16);
    assert_eq!(queue.max_aggregate_bytes, 1024 * 1024);

    // `max_targets` is the one value here with no ADR-022 §7 anchor — its ceiling
    // is set and justified in `src/analysis/queue.rs`. It is pinned anyway,
    // because the drift risk below applies to it equally.
    //
    // The constants above are only meaningful if the budget a real invocation
    // runs under is derived from them. `RuntimeConfig` converts the TOML-facing
    // `[language_analysis]` defaults into an `OrchestrationBudget`, so the two
    // sides can drift independently — assert they land on the same numbers.
    let effective = RuntimeConfig::from(&AegisConfig::default()).language_analysis_budget;
    assert_eq!(
        effective.inline_source_limit_bytes,
        orchestration.inline_source_limit_bytes
    );
    assert_eq!(
        effective.script_file_limit_bytes,
        orchestration.script_file_limit_bytes
    );
    assert_eq!(effective.max_script_files, orchestration.max_script_files);
    assert_eq!(effective.max_depth, queue.max_depth);
    assert_eq!(effective.max_targets, queue.max_targets);
    assert_eq!(
        effective.max_aggregate_bytes,
        orchestration.max_aggregate_bytes
    );
    assert_eq!(effective.total_timeout, Duration::from_millis(100));
}

#[test]
fn ci_keeps_safe_and_slow_path_qualification_benches_on_the_performance_gate() {
    let workflow = read_repo_file(".github/workflows/ci.yml");
    let baseline = read_repo_file("perf/scanner_bench_baseline.toml");

    // Anchored to `run:` rather than a bare substring: each of these commands is
    // also the step's `name:`, so a plain `contains` would stay green if the
    // `run:` line were deleted and the bench silently stopped executing.
    let workflow = workflow.replace("\r\n", "\n");
    for command in [
        "cargo bench --bench scanner_bench",
        "cargo bench --bench no_source_bench -p aegis-language",
        "cargo bench --bench parse_latency_bench -p aegis-language",
    ] {
        assert!(
            workflow.contains(&format!("run: {command}\n")),
            "Performance baseline CI must actually run `{command}`"
        );
    }
    assert!(
        workflow.contains("--baseline perf/scanner_bench_baseline.toml"),
        "Performance baseline CI must evaluate the checked-in benchmark policy"
    );

    // The policy verdict is piped into `tee` for the log artifact. Actions' implicit
    // Linux shell is `bash -e {0}` with no `pipefail`, so without an explicit
    // `shell: bash` + `set -o pipefail` the pipeline reports `tee`'s status and every
    // benchmark FAIL goes green — the gate would exist on paper only.
    let evaluate_step = workflow
        .split("- name: Evaluate benchmark policy")
        .nth(1)
        .expect("Performance baseline CI must keep the benchmark policy step");
    let evaluate_step = evaluate_step
        .split_once("\n      - name:")
        .map_or(evaluate_step, |(step, _)| step);
    // Anchored to whole lines: the step's own explanatory comment mentions
    // `pipefail`, so a bare `contains("pipefail")` would pass with the actual
    // setting deleted.
    let step_lines: Vec<&str> = evaluate_step.lines().map(str::trim).collect();
    assert!(
        step_lines.contains(&"shell: bash"),
        "the benchmark policy step pipes into `tee`, so it must declare `shell: bash`; \
         Actions' implicit shell has no `pipefail` and a failing policy would exit 0"
    );
    assert!(
        step_lines
            .iter()
            .any(|line| line.starts_with("set -") && line.contains("pipefail")),
        "the benchmark policy step must `set -o pipefail` so `tee` cannot mask a failing policy"
    );

    // Criterion carries `new/estimates.json` forward in the cached `target/`, so a
    // bench that stops running would gate against a stale measurement instead of
    // failing as missing.
    assert!(
        workflow.contains("rm -rf target/criterion"),
        "Performance baseline CI must discard cached Criterion results before benching"
    );

    for benchmark in [
        "no_source_does_not_start_worker",
        "parse_latency_per_grammar/parse/python",
        "parse_latency_per_grammar/parse/javascript",
        "parse_latency_per_grammar/parse/typescript",
        "parse_latency_per_grammar/parse/bash",
    ] {
        assert!(
            baseline.contains(&format!("name = \"{benchmark}\"")),
            "benchmark policy must fail slow-path latency regressions for `{benchmark}`"
        );
    }
}

#[test]
fn production_qualification_record_covers_all_remaining_iteration_10_measurements() {
    let performance = read_repo_file("docs/performance-baseline.md");
    let readiness = read_repo_file("docs/release-readiness.md");

    // Plan Iteration 10 GREEN requires measured evidence, not merely the
    // earlier Iteration 0 deferrals. Keep the public qualification record
    // explicit about the worker lifecycle and the bounded aggregate path so a
    // later documentation refresh cannot accidentally present the adapters as
    // qualified with only parse microbenchmarks.
    for row in [
        "| No-source safe path | `cargo bench --bench no_source_bench -p aegis-language` | 1.01 µs for the ten-command corpus (about 101 ns/command) | Worker-free and below the 2.5 µs policy ceiling. |",
        "| Per-grammar parse | `cargo bench --bench parse_latency_bench -p aegis-language` | Python 28.7 µs; JavaScript 21.8 µs; TypeScript 24.5 µs; Bash 14.6 µs | Every row is below its checked-in Criterion ceiling. |",
        "| Worker cold-session latency | `/usr/bin/time` around one framed `--internal-language-worker` Python parse | below the tool's 10 ms display resolution | This is process start + one bounded request on the release binary; use the 100 ms total deadline, not this host measurement, as the enforced bound. |",
        "| Worker warm-session latency | Not applicable to the production orchestration | no reusable session | `orchestrate` deliberately spawns, closes, and reaps one ephemeral worker per queued target. The protocol can carry a bounded sequence, but production does not reuse a warm worker; a future reuse optimization needs its own benchmark and review. |",
        "| Peak worker RSS | five cold worker samples with `/usr/bin/time` | 4.1–4.3 MiB | The direct worker process stayed within this observed host range; it is evidence only, not a cross-platform memory promise. |",
        "| Aggregate-timeout boundary | `tests/analysis_orchestrate_runtime.rs::run_records_target_aggregate_and_total_time_budget_exhaustion` | enforced at the configured total deadline | The default is 100 ms. The regression asserts typed `LimitExceeded` while retaining prior target results; no timer-derived throughput claim is made. |",
        "| Per-target release-binary size | native `target/release/aegis` | 9.5 MiB | Local native size is recorded for drift detection. Exact sizes for Linux musl x86_64/aarch64 and macOS x86_64/aarch64 must come from the required CI contexts; local cross-target sizes are not substituted for release artifacts. |",
    ] {
        assert!(
            performance.contains(row),
            "performance record must retain a complete Iteration 10 evidence row `{row}`"
        );
    }

    for needle in [
        "four foundation adapters under qualification",
        "not a release-enable claim",
        "required CI contexts",
    ] {
        assert!(
            readiness.contains(needle),
            "release readiness must state the Iteration 10 qualification boundary `{needle}`"
        );
    }
}
