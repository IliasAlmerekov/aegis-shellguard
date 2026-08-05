# L1 language-adapter qualification record

This is the checked-in qualification record required by the Iteration 10 plan
and ADR-022 §11. It records the four L1 adapters separately, but it is **not**
release enablement: an adapter remains unsupported until a release candidate
containing this exact evidence passes the required CI contexts.

## Shared release evidence

- Grammar pins, upstream provenance, licenses, ABI compatibility, native build
  inputs, and the Tree-sitter runtime are recorded in
  [`language-grammar-manifest.md`](language-grammar-manifest.md).
- The [CI run 30907622035](https://github.com/IliasAlmerekov/aegis-shellguard/actions/runs/30907622035)
  passed the four musl/macOS release builds, both release-host builds, the
  performance policy, live installers, and all nine fuzz targets. Its four
  `Cross build` jobs prove that the same statically linked grammar set builds
  for both Linux musl and both macOS targets.
- The measured worker lifecycle, no-source, aggregate-timeout, and native-size
  evidence is recorded in [`performance-baseline.md`](performance-baseline.md).
  Criterion policy rows are deliberately named below so each adapter can be
  traced independently.

## Per-adapter evidence

| Adapter | Corpus and supported-operation characterization | Fuzz target and seed corpus | Latency policy row | Release-build evidence |
|---|---|---|---|---|
| Python | `crates/aegis-language/tests/python_corpus.rs` | `language_python` with `fuzz/corpus/language_python/` | `parse_latency_per_grammar/parse/python` | CI run 30907622035, four-target matrix |
| JavaScript | `crates/aegis-language/tests/javascript_corpus.rs` | `language_javascript` with `fuzz/corpus/language_javascript/` | `parse_latency_per_grammar/parse/javascript` | CI run 30907622035, four-target matrix |
| TypeScript | `crates/aegis-language/tests/typescript_corpus.rs` | `language_typescript` with `fuzz/corpus/language_typescript/` | `parse_latency_per_grammar/parse/typescript` | CI run 30907622035, four-target matrix |
| Shell/Bash | `crates/aegis-language/tests/bash_corpus.rs` | `language_bash` with `fuzz/corpus/language_bash/` | `parse_latency_per_grammar/parse/bash` | CI run 30907622035, four-target matrix |

The four corpora exercise positive destructive operations, narrowness cases,
literal and dynamic operands, nested payloads, malformed syntax, and each
grammar's supported current syntax. Shared protocol, router, heredoc, worker
failure, timeout, privacy, Audit v1/v2, and Shell/Watch/Hook/CI interface
contracts are exercised by the workspace suites listed in the plan's
Iteration 10 RED gate. The all-four-target CI run executes the adapter fuzz
targets for 100,000 iterations each with their checked-in seed corpora.

## Remaining release decision

This record documents qualification evidence only. Before changing the L1
release-readiness checkboxes or treating any adapter as default-on, repeat the
required CI matrix on the release candidate and complete the Iteration 10
review and re-review gates.
