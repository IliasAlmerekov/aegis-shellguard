// Shared no-source command corpus for `tests/no_source.rs` and `benches/no_source_bench.rs` (ADR-022 §1).
// Lives in `tests/common/` so Cargo does not compile it as a standalone test target.

/// Commands that expose no analyzable source.
pub const NO_SOURCE: &[&str] = &[
    "ls -la /home/user",
    "echo hello world",
    "cat /etc/hostname",
    "grep -r TODO src/",
    "git status",
    "git log --oneline -20",
    "cargo build --release",
    "npm run test",
    "docker ps -a",
    "kubectl get pods -n production",
];
