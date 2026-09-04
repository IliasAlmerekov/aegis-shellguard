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

/// bubblewrap is the second sanctioned native-C build input (ADR-029 §3–§4),
/// vendored under `crates/aegis-sandbox/vendor/bubblewrap/`. Unlike the
/// Tree-sitter crates it is not a cargo dependency, so `Cargo.lock` cannot pin
/// it; the version is asserted against the `VERSION` marker in the vendored
/// tree, and the LGPL notice is asserted against the distributed notices.
const BUBBLEWRAP_VERSION: &str = "0.11.2";
const BUBBLEWRAP_VENDOR_DIR: &str = "crates/aegis-sandbox/vendor/bubblewrap";

fn repo_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn read_repo_file(relative: &str) -> String {
    let path = repo_path(relative);
    fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("{} must be readable: {error}", path.display());
    })
}

/// The component names from attribution-table rows in the notices: the first
/// cell of every table row, skipping separator and header rows. Structured row
/// parsing rather than a substring search, so a coincidental prose mention of
/// a vendored name cannot satisfy the notice contract.
fn attribution_row_components(notices: &str) -> Vec<String> {
    notices
        .lines()
        .filter(|line| line.starts_with('|'))
        .filter(|line| {
            // Skip separator rows (`|---|---:|…`) and the header row, whose
            // second cell is the literal column title.
            let first = line.split('|').nth(1).unwrap_or_default().trim();
            let second = line.split('|').nth(2).unwrap_or_default().trim();
            !first.starts_with('-') && second != "Version"
        })
        .filter_map(|line| line.split('|').nth(1))
        .map(|cell| cell.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect()
}

/// Every vendored third-party source tree under `crates/*/vendor/`, as
/// `(crate, tree_name, path)`. Scanned generically rather than from a hardcoded
/// list so a newly vendored dependency is caught by the notice contract instead
/// of silently shipping.
fn vendored_trees() -> Vec<(String, String, PathBuf)> {
    let crates_root = repo_path("crates");
    let mut trees = Vec::new();
    for crate_dir in fs::read_dir(&crates_root).expect("crates/ must be readable") {
        let crate_dir = crate_dir.expect("crates/ entry must be readable").path();
        let vendor_dir = crate_dir.join("vendor");
        if !vendor_dir.is_dir() {
            continue;
        }
        let crate_name = crate_dir
            .file_name()
            .expect("crate dir must have a name")
            .to_string_lossy()
            .into_owned();
        for tree in fs::read_dir(&vendor_dir).expect("vendor/ must be readable") {
            let tree = tree.expect("vendor/ entry must be readable").path();
            if !tree.is_dir() {
                continue;
            }
            let tree_name = tree
                .file_name()
                .expect("vendored tree must have a name")
                .to_string_lossy()
                .into_owned();
            trees.push((crate_name.clone(), tree_name, tree));
        }
    }
    trees.sort_unstable();
    trees
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
        "| Per-target release-binary size | native `target/release/aegis` | 9.7 MiB | Local native size is recorded for drift detection. Grew from 9.5 MiB when the embedded bubblewrap fallback (ADR-029 §3) was added; the embedded `bwrap` is ~116 KiB. Exact sizes for Linux musl x86_64/aarch64 and macOS x86_64/aarch64 must come from the required CI contexts; local cross-target sizes are not substituted for release artifacts. |",
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

#[test]
fn each_foundation_adapter_has_a_checked_in_qualification_record() {
    let record = read_repo_file("docs/language-qualification.md");

    for (language, corpus, fuzz_target, parse_row) in [
        (
            "Python",
            "crates/aegis-language/tests/python_corpus.rs",
            "language_python",
            "parse_latency_per_grammar/parse/python",
        ),
        (
            "JavaScript",
            "crates/aegis-language/tests/javascript_corpus.rs",
            "language_javascript",
            "parse_latency_per_grammar/parse/javascript",
        ),
        (
            "TypeScript",
            "crates/aegis-language/tests/typescript_corpus.rs",
            "language_typescript",
            "parse_latency_per_grammar/parse/typescript",
        ),
        (
            "Shell/Bash",
            "crates/aegis-language/tests/bash_corpus.rs",
            "language_bash",
            "parse_latency_per_grammar/parse/bash",
        ),
    ] {
        for evidence in [language, corpus, fuzz_target, parse_row] {
            assert!(
                record.contains(evidence),
                "qualification record must retain {language}'s evidence `{evidence}`"
            );
        }
    }

    assert!(
        record.contains("30907622035"),
        "qualification record must link the all-four-target CI evidence"
    );
}

#[test]
fn bubblewrap_vendored_sources_and_lgpl_notice_are_pinned() {
    // Line endings are normalized so the contract does not depend on the
    // checkout's `core.autocrlf` setting (CI checks out LF; some local
    // checkouts are CRLF).
    let notices = read_repo_file("THIRD_PARTY_NOTICES.md").replace("\r\n", "\n");

    // The version marker in the vendored tree is the source of truth; there is
    // no `Cargo.lock` entry to assert against because vendored C is not a cargo
    // dependency.
    let version_marker = read_repo_file(&format!("{BUBBLEWRAP_VENDOR_DIR}/VERSION"));
    assert!(
        version_marker.contains(BUBBLEWRAP_VERSION),
        "vendored bubblewrap VERSION must pin {BUBBLEWRAP_VERSION}, got: {version_marker}"
    );

    // Every file in the vendored tree must exist and be non-empty. Scanned
    // generically rather than from a hardcoded list so a newly added source file
    // is covered by the same contract.
    let tree = repo_path(BUBBLEWRAP_VENDOR_DIR);
    let mut empty: Vec<String> = Vec::new();
    for entry in fs::read_dir(&tree).expect("vendored bubblewrap tree must be readable") {
        let entry = entry
            .expect("vendored bubblewrap entry must be readable")
            .path();
        if entry.is_dir() {
            continue;
        }
        let content = fs::read_to_string(&entry).unwrap_or_else(|error| {
            panic!("{} must be readable: {error}", entry.display());
        });
        if content.trim().is_empty() {
            empty.push(
                entry
                    .file_name()
                    .expect("vendored file must have a name")
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
    assert!(
        empty.is_empty(),
        "vendored bubblewrap source file(s) {empty:?} must be non-empty"
    );

    // The distributed notice must carry the bubblewrap row and the LGPL notice.
    let row = format!(
        "| bubblewrap | `{BUBBLEWRAP_VERSION}` | <https://github.com/containers/bubblewrap> | LGPL-2.0-or-later |"
    );
    assert!(
        notices.contains(&row),
        "third-party notices must attribute bubblewrap at {BUBBLEWRAP_VERSION}: expected row `{row}`"
    );
    // The copyright cell must restate the vendored sources' own header line,
    // derived here from bubblewrap.c rather than restated by hand: the notices
    // describe what this repo actually ships.
    let bubblewrap_c = read_repo_file(&format!("{BUBBLEWRAP_VENDOR_DIR}/bubblewrap.c"));
    let expected_copyright = bubblewrap_c
        .lines()
        .find(|line| line.contains("Copyright (C)"))
        .expect("vendored bubblewrap.c must carry a Copyright (C) header line")
        .trim()
        .trim_start_matches("* ")
        .trim()
        .to_string();
    assert!(
        notices.contains(&format!(
            "| bubblewrap | `{BUBBLEWRAP_VERSION}` | <https://github.com/containers/bubblewrap> | LGPL-2.0-or-later | {expected_copyright} |"
        )),
        "third-party notices must carry bubblewrap's own copyright line \
         `{expected_copyright}` in the attribution row, not a restated range"
    );
    assert!(
        notices.contains("GNU Library General Public License"),
        "third-party notices must include the LGPL notice for bubblewrap"
    );
    assert!(
        notices.contains("LGPL-2.0-or-later"),
        "third-party notices must name the LGPL-2.0-or-later SPDX expression for bubblewrap"
    );
    // The LGPL obligation must be stated deliberately (issue #231): the
    // recipient can replace the covered component, and the PATH preference is
    // the mechanism that makes that real without relinking Aegis. The notices
    // are hard-wrapped, so phrase assertions run against a newline-flattened
    // copy; the verbatim COPYING assertion below stays on the raw text.
    let unwrapped = notices.replace('\n', " ");
    assert!(
        unwrapped.contains("replace the covered component"),
        "third-party notices must state the LGPL relink/replace obligation deliberately"
    );
    assert!(
        unwrapped.contains("prefers any usable `bwrap` found on `PATH`"),
        "third-party notices must frame the PATH preference as the LGPL replace mechanism"
    );
    // LGPL-2.0 §4: the licence text must accompany the binary. The notices are
    // the distribution artifact (staged byte-identical into the npm package
    // and the installed doc dir), so the full text is reproduced from the
    // vendored COPYING rather than referenced by a repo path those channels
    // do not carry.
    let copying = read_repo_file(&format!("{BUBBLEWRAP_VENDOR_DIR}/COPYING")).replace("\r\n", "\n");
    assert!(
        notices.contains(&copying),
        "third-party notices must reproduce the vendored LGPL-2.0 COPYING verbatim \
         so every distribution channel carries the licence with the binary"
    );
}

#[test]
fn every_vendored_third_party_tree_has_a_notice_entry() {
    let notices = read_repo_file("THIRD_PARTY_NOTICES.md");

    // Each vendored third-party source tree must have a matching attribution
    // ROW in THIRD_PARTY_NOTICES.md; an unlisted tree fails CI. The match is
    // against parsed table rows, not a substring search, so a coincidental
    // prose mention cannot pass. This is generic so a future third vendored
    // dependency is caught rather than silently shipped.
    let listed_components = attribution_row_components(&notices);
    let mut unlisted: Vec<String> = vendored_trees()
        .iter()
        .filter(|(_, tree_name, _)| {
            !listed_components
                .iter()
                .any(|component| component == tree_name)
        })
        .map(|(_, tree_name, _)| tree_name.clone())
        .collect();
    unlisted.sort_unstable();
    unlisted.dedup();
    assert!(
        unlisted.is_empty(),
        "vendored third-party source tree(s) {unlisted:?} have no attribution row in \
         THIRD_PARTY_NOTICES.md; add a row and a notice section for each before merging"
    );
}

#[test]
fn attribution_row_components_ignores_separator_header_and_prose_mentions() {
    let notices = "\
# Notices

Prose mentions bubblewrap without a row: bubblewrap is vendored, and this
sentence alone must not satisfy the notice contract.

| Component | Version | Upstream | SPDX license | Copyright notice |
|---|---:|---|---|---|
| bubblewrap | `0.11.2` | <https://github.com/containers/bubblewrap> | LGPL-2.0-or-later | Copyright (C) Example |
";
    let components = attribution_row_components(notices);
    assert_eq!(
        components,
        ["bubblewrap"],
        "only table-row first cells count; prose and header/separator rows do not"
    );
}

#[test]
fn deny_toml_keeps_the_cargo_graph_strict_and_declares_the_vendored_lgpl_exception() {
    let deny = read_repo_file("deny.toml").replace("\r\n", "\n");

    // The `[licenses]` allow list is the cargo-graph gate and must stay
    // permissive-only (CONVENTION.md §6): LGPL-2.0-or-later enters this repo
    // only as vendored C, which cargo-deny cannot see, so listing it here would
    // silently admit a future LGPL *cargo* dependency instead of failing the
    // check. The graph stays strict; the vendored exception is declared in the
    // file's comments and enforced by the vendored-trees contract test.
    // Anchor on the standalone `[licenses]` header line: the word also appears
    // in the file's leading comment, so a bare substring split would grab the
    // wrong section.
    let licenses_section = deny
        .split("\n[licenses]\n")
        .nth(1)
        .expect("deny.toml must define a [licenses] section");
    let allow_list = licenses_section
        .split("allow = [")
        .nth(1)
        .and_then(|rest| rest.split_once(']'))
        .map(|(entries, _)| entries)
        .expect("deny.toml [licenses] must define an allow list");
    assert!(
        !allow_list.contains("LGPL-2.0-or-later"),
        "deny.toml [licenses] allow must stay permissive-only: cargo-deny cannot scope an \
         exception to vendored C, so an allow entry would silently admit a future LGPL cargo \
         dependency instead of triaging it"
    );
    assert!(
        deny.contains("LGPL-2.0-or-later"),
        "deny.toml must still declare the vendored bubblewrap LGPL-2.0-or-later exception \
         (ADR-029 §3–§4) in its comments"
    );
}

#[test]
fn cross_musl_dockerfile_verifies_the_libcap_tarball_checksum() {
    let dockerfile = read_repo_file("docker/cross-musl/Dockerfile");

    // This image compiles libcap into every musl release binary, so the
    // fetched tarball is a build input of shipped artifacts: it must be
    // verified against a pinned SHA256 before the build uses it, not fetched
    // blind (CONVENTION.md §6 supply-chain rule).
    let verifying = dockerfile
        .lines()
        .find(|line| line.contains("sha256sum -c"))
        .expect(
            "the cross-musl Dockerfile must verify the libcap tarball against a \
             pinned SHA256 before building with it",
        );
    assert!(
        verifying
            .split_whitespace()
            .any(|token| { token.len() == 64 && token.chars().all(|c| c.is_ascii_hexdigit()) }),
        "the libcap verification must pin a full 64-hex SHA256 digest, got: {verifying}"
    );
}

#[test]
fn convention_documents_lgpl_exception_and_sandbox_build_dependencies() {
    let convention = read_repo_file("CONVENTION.md");

    // §6 Dependency Rules must name bubblewrap as the second sanctioned native-C
    // build input and record its LGPL-2.0-or-later exception.
    assert!(
        convention.contains("second sanctioned native-C build input"),
        "CONVENTION.md §6 must name bubblewrap as the second sanctioned native-C build input"
    );
    assert!(
        convention.contains("bubblewrap") && convention.contains("LGPL-2.0-or-later"),
        "CONVENTION.md §6 must document bubblewrap's LGPL-2.0-or-later exception (ADR-029 §3–§4)"
    );

    // The `cc` and `pkg-config` build-dependencies are scoped to aegis-sandbox
    // and must be recorded in the approved dependency list.
    assert!(
        convention.contains("`cc`") && convention.contains("`pkg-config`"),
        "CONVENTION.md §6 must list `cc` and `pkg-config` as approved build-dependencies"
    );
    assert!(
        convention.contains("aegis-sandbox"),
        "CONVENTION.md §6 must scope the `cc`/`pkg-config` build-dependencies to aegis-sandbox"
    );
}

#[test]
fn claude_md_approves_the_sandbox_build_dependencies() {
    let claude = read_repo_file("CLAUDE.md");

    // The `cc` and `pkg-config` build-dependencies in aegis-sandbox's Cargo.toml
    // must be recorded in the approved-dependencies table so the hard Standards
    // violation (a build-dependency outside the approved set) is closed.
    assert!(
        claude.contains("`cc`") && claude.contains("`pkg-config`"),
        "CLAUDE.md approved-dependencies table must list `cc` and `pkg-config` as \
         build-dependencies for aegis-sandbox"
    );
    assert!(
        claude.contains("aegis-sandbox"),
        "CLAUDE.md must scope the `cc`/`pkg-config` build-dependencies to aegis-sandbox"
    );
}
