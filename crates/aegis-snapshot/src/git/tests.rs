use super::*;
use std::fs;
use tempfile::TempDir;

/// Stand-in for the guarded command. `snapshot()` never reads it.
const DANGEROUS_CMD: &str = "rm -rf .";

/// Initialise a bare git repo with an empty initial commit so stash works.
async fn init_repo(dir: &std::path::Path) {
    git_command(dir).args(["init"]).output().await.unwrap();
    // Set local identity so stash commits don't depend on global git config.
    for (key, val) in [
        ("user.email", "test@aegis.dev"),
        ("user.name", "Aegis Test"),
        // A host with core.autocrlf=true would rewrite line endings whenever
        // git touches a file, so byte-exact content assertions would depend on
        // the developer's global git config.
        ("core.autocrlf", "false"),
    ] {
        git_command(dir)
            .args(["config", key, val])
            .output()
            .await
            .unwrap();
    }
    // Stash requires at least one commit; create an empty one.
    git_command(dir)
        .args(["commit", "--allow-empty", "-m", "init"])
        .output()
        .await
        .unwrap();
}

/// Write `content` to `name`, stage it, and commit it.
async fn commit_file(dir: &std::path::Path, name: &str, content: &str) {
    fs::write(dir.join(name), content).unwrap();
    git_command(dir).args(["add", name]).output().await.unwrap();
    git_command(dir)
        .args([
            "-c",
            "user.email=test@aegis.dev",
            "-c",
            "user.name=Aegis Test",
            "commit",
            "-m",
            &format!("add {name}"),
        ])
        .output()
        .await
        .unwrap();
}

#[tokio::test]
async fn is_applicable_outside_repo() {
    let dir = TempDir::new().unwrap();
    assert!(!GitPlugin.is_applicable(dir.path()).await);
}

/// A `cwd` that does not exist makes `git`'s own `chdir` fail before exec,
/// so `Command::status()` returns `Err` rather than an exit status. This
/// is the spawn-failure branch, distinct from "ran and said not a repo".
#[tokio::test]
async fn is_applicable_assumes_applicable_when_git_cannot_be_spawned() {
    let parent = TempDir::new().unwrap();
    let missing_cwd = parent.path().join("does-not-exist");

    assert!(GitPlugin.is_applicable(&missing_cwd).await);
}

#[test]
fn is_applicable_logs_spawn_failure_via_tracing() {
    use std::sync::{Arc, Mutex};
    use tracing::field::{Field, Visit};

    #[derive(Default)]
    struct CapturedFields {
        message: Option<String>,
        cwd: Option<String>,
    }

    #[derive(Default)]
    struct FieldVisitor(CapturedFields);

    impl Visit for FieldVisitor {
        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            self.record_str(field, &format!("{value:?}"));
        }

        fn record_str(&mut self, field: &Field, value: &str) {
            match field.name() {
                "message" => self.0.message = Some(value.trim_matches('"').to_string()),
                "cwd" => self.0.cwd = Some(value.trim_matches('"').to_string()),
                _ => {}
            }
        }
    }

    struct CollectingSubscriber(Arc<Mutex<Option<CapturedFields>>>);

    impl tracing::Subscriber for CollectingSubscriber {
        fn enabled(&self, _metadata: &tracing::Metadata<'_>) -> bool {
            true
        }
        fn new_span(&self, _span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
            tracing::span::Id::from_u64(1)
        }
        fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}
        fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}
        fn event(&self, event: &tracing::Event<'_>) {
            let mut visitor = FieldVisitor::default();
            event.record(&mut visitor);
            *self.0.lock().unwrap() = Some(visitor.0);
        }
        fn enter(&self, _span: &tracing::span::Id) {}
        fn exit(&self, _span: &tracing::span::Id) {}
    }

    let parent = TempDir::new().unwrap();
    let missing_cwd = parent.path().join("does-not-exist");

    let captured: Arc<Mutex<Option<CapturedFields>>> = Arc::new(Mutex::new(None));
    let subscriber = CollectingSubscriber(Arc::clone(&captured));

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let applicable = tracing::subscriber::with_default(subscriber, || {
        runtime.block_on(GitPlugin.is_applicable(&missing_cwd))
    });
    assert!(applicable);

    let captured = captured
        .lock()
        .unwrap()
        .take()
        .expect("a spawn failure must emit a tracing event");
    assert_eq!(
        captured.message.as_deref(),
        Some("failed to spawn git while checking applicability, assuming applicable")
    );
    assert_eq!(
        captured.cwd.as_deref(),
        Some(missing_cwd.display().to_string()).as_deref()
    );
}

#[tokio::test]
async fn is_applicable_at_repo_root() {
    let dir = TempDir::new().unwrap();
    init_repo(dir.path()).await;
    assert!(GitPlugin.is_applicable(dir.path()).await);
}

#[tokio::test]
async fn is_applicable_in_repo_subdirectory() {
    let dir = TempDir::new().unwrap();
    init_repo(dir.path()).await;
    let sub = dir.path().join("deep/nested/dir");
    fs::create_dir_all(&sub).unwrap();
    // Should detect the repo even though there is no .git in this subdirectory.
    assert!(GitPlugin.is_applicable(&sub).await);
}

#[tokio::test]
async fn snapshot_clean_tree_returns_sentinel() {
    let dir = TempDir::new().unwrap();
    init_repo(dir.path()).await;

    let id = GitPlugin.snapshot(dir.path(), "rm -rf .").await.unwrap();
    assert_eq!(id, CLEAN_SENTINEL);
}

#[tokio::test]
async fn snapshot_and_rollback_restores_changes() {
    let dir = TempDir::new().unwrap();
    init_repo(dir.path()).await;
    commit_file(dir.path(), "hello.txt", "original\n").await;

    // Introduce an uncommitted change.
    fs::write(dir.path().join("hello.txt"), "modified\n").unwrap();

    let snapshot_id = GitPlugin.snapshot(dir.path(), "rm -rf .").await.unwrap();
    assert_ne!(snapshot_id, CLEAN_SENTINEL, "expected a real stash");

    // The snapshot is not observable in the working tree.
    assert_eq!(
        fs::read_to_string(dir.path().join("hello.txt"))
            .unwrap()
            .trim(),
        "modified"
    );

    // The guarded command overwrites the file.
    fs::write(dir.path().join("hello.txt"), "clobbered\n").unwrap();

    GitPlugin.rollback(&snapshot_id).await.unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("hello.txt"))
            .unwrap()
            .trim(),
        "modified"
    );
}

#[tokio::test]
async fn rollback_clean_sentinel_is_noop() {
    // Rolling back a "clean" snapshot must succeed without touching git.
    GitPlugin.rollback(CLEAN_SENTINEL).await.unwrap();
}

// ── untracked files ──────────────────────────────────────────────────────

#[tokio::test]
async fn snapshot_includes_untracked_files() {
    let dir = TempDir::new().unwrap();
    init_repo(dir.path()).await;

    // New file, never `git add`'ed.
    let new_file = dir.path().join("untracked.txt");
    fs::write(&new_file, "brand new\n").unwrap();

    let snapshot_id = GitPlugin.snapshot(dir.path(), "rm -rf .").await.unwrap();
    assert_ne!(snapshot_id, CLEAN_SENTINEL);

    // The file is captured in the stash and still on disk.
    assert_eq!(fs::read_to_string(&new_file).unwrap().trim(), "brand new");

    // The guarded command deletes it.
    fs::remove_file(&new_file).unwrap();

    GitPlugin.rollback(&snapshot_id).await.unwrap();
    assert_eq!(fs::read_to_string(&new_file).unwrap().trim(), "brand new");
}

// ── staged + unstaged changes ────────────────────────────────────────────

#[tokio::test]
async fn snapshot_and_rollback_preserves_index() {
    let dir = TempDir::new().unwrap();
    init_repo(dir.path()).await;
    commit_file(dir.path(), "staged.txt", "base\n").await;
    commit_file(dir.path(), "unstaged.txt", "base\n").await;

    // Stage a change to staged.txt.
    fs::write(dir.path().join("staged.txt"), "staged-change\n").unwrap();
    git_command(dir.path())
        .args(["add", "staged.txt"])
        .output()
        .await
        .unwrap();

    // Leave a change to unstaged.txt outside the index.
    fs::write(dir.path().join("unstaged.txt"), "unstaged-change\n").unwrap();

    let snapshot_id = GitPlugin.snapshot(dir.path(), "rm -rf .").await.unwrap();
    assert_ne!(snapshot_id, CLEAN_SENTINEL);

    // Both files keep their uncommitted content after the stash.
    assert_eq!(
        fs::read_to_string(dir.path().join("staged.txt"))
            .unwrap()
            .trim(),
        "staged-change"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("unstaged.txt"))
            .unwrap()
            .trim(),
        "unstaged-change"
    );

    // The guarded command deletes both files.
    fs::remove_file(dir.path().join("staged.txt")).unwrap();
    fs::remove_file(dir.path().join("unstaged.txt")).unwrap();

    GitPlugin.rollback(&snapshot_id).await.unwrap();

    // Both changes are restored.
    assert_eq!(
        fs::read_to_string(dir.path().join("staged.txt"))
            .unwrap()
            .trim(),
        "staged-change"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("unstaged.txt"))
            .unwrap()
            .trim(),
        "unstaged-change"
    );

    // staged.txt should be in the index after rollback (--index flag).
    let status = git_command(dir.path())
        .args(["diff", "--cached", "--name-only"])
        .output()
        .await
        .unwrap();
    let staged_files = String::from_utf8_lossy(&status.stdout);
    assert!(
        staged_files.contains("staged.txt"),
        "staged.txt should still be staged after rollback"
    );
}

// ── snapshot from a repo subdirectory ────────────────────────────────────

#[tokio::test]
async fn snapshot_and_rollback_from_subdirectory() {
    let dir = TempDir::new().unwrap();
    init_repo(dir.path()).await;
    commit_file(dir.path(), "file.txt", "original\n").await;

    let sub = dir.path().join("subdir");
    fs::create_dir_all(&sub).unwrap();

    // Modify the file from the repo root, but run snapshot from a subdir.
    fs::write(dir.path().join("file.txt"), "modified\n").unwrap();

    let snapshot_id = GitPlugin.snapshot(&sub, "rm -rf .").await.unwrap();
    assert_ne!(snapshot_id, CLEAN_SENTINEL);

    assert_eq!(
        fs::read_to_string(dir.path().join("file.txt"))
            .unwrap()
            .trim(),
        "modified"
    );

    // The guarded command deletes the file.
    fs::remove_file(dir.path().join("file.txt")).unwrap();

    GitPlugin.rollback(&snapshot_id).await.unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("file.txt"))
            .unwrap()
            .trim(),
        "modified"
    );
}

// ── worktree ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn is_applicable_in_worktree() {
    let main_dir = TempDir::new().unwrap();
    init_repo(main_dir.path()).await;
    // A worktree needs a branch name; HEAD is fine for detection.
    let wt_dir = TempDir::new().unwrap();
    let out = git_command(main_dir.path())
        .args([
            "worktree",
            "add",
            wt_dir.path().to_str().unwrap(),
            "HEAD",
            "--detach",
        ])
        .output()
        .await
        .unwrap();
    // Skip if git worktree is unavailable in this environment.
    if !out.status.success() {
        return;
    }
    assert!(
        GitPlugin.is_applicable(wt_dir.path()).await,
        "worktree should be detected as a git repo"
    );
}

#[tokio::test]
async fn snapshot_and_rollback_in_worktree() {
    let main_dir = TempDir::new().unwrap();
    init_repo(main_dir.path()).await;
    commit_file(main_dir.path(), "file.txt", "original\n").await;

    let wt_dir = TempDir::new().unwrap();
    let out = git_command(main_dir.path())
        .args([
            "worktree",
            "add",
            wt_dir.path().to_str().unwrap(),
            "HEAD",
            "--detach",
        ])
        .output()
        .await
        .unwrap();
    if !out.status.success() {
        return;
    }

    // Modify the file inside the worktree.
    fs::write(wt_dir.path().join("file.txt"), "modified\n").unwrap();

    let snapshot_id = GitPlugin.snapshot(wt_dir.path(), "rm -rf .").await.unwrap();
    assert_ne!(snapshot_id, CLEAN_SENTINEL);

    assert_eq!(
        fs::read_to_string(wt_dir.path().join("file.txt"))
            .unwrap()
            .trim(),
        "modified"
    );

    // The guarded command deletes the file.
    fs::remove_file(wt_dir.path().join("file.txt")).unwrap();

    GitPlugin.rollback(&snapshot_id).await.unwrap();
    assert_eq!(
        fs::read_to_string(wt_dir.path().join("file.txt"))
            .unwrap()
            .trim(),
        "modified"
    );
}

// ── rollback conflict ────────────────────────────────────────────────────

#[tokio::test]
async fn rollback_returns_conflict_error_with_recovery_hint() {
    let dir = TempDir::new().unwrap();
    init_repo(dir.path()).await;
    commit_file(dir.path(), "file.txt", "original\n").await;

    // Stash a diverging change.
    fs::write(dir.path().join("file.txt"), "stashed-content\n").unwrap();
    let snapshot_id = GitPlugin.snapshot(dir.path(), "rm -rf .").await.unwrap();
    assert_ne!(snapshot_id, CLEAN_SENTINEL);

    // Commit a different version of the same file. The stash entry now carries
    // a change against a parent that no longer matches HEAD, so applying it
    // cannot auto-merge.
    fs::write(dir.path().join("file.txt"), "conflicting-content\n").unwrap();
    commit_file(dir.path(), "file.txt", "conflicting-content\n").await;

    let err = GitPlugin
        .rollback(&snapshot_id)
        .await
        .expect_err("expected a conflict error");

    match err {
        SnapshotError::RollbackConflict {
            ref stash_ref,
            ref cwd,
            ..
        } => {
            // The stash ref should be a positional ref (stash@{N}).
            assert!(
                stash_ref.starts_with("stash@{"),
                "stash_ref should be a positional ref, got: {stash_ref}"
            );
            // The cwd must be present so the user knows where to recover.
            assert!(!cwd.is_empty(), "cwd must be non-empty");
            // The error message must contain recovery instructions.
            let msg = err.to_string();
            assert!(
                msg.contains("git stash drop"),
                "message should include drop command: {msg}"
            );
            assert!(
                msg.contains("git diff"),
                "message should include diff command: {msg}"
            );
        }
        other => panic!("expected RollbackConflict, got: {other:?}"),
    }
}

#[tokio::test]
async fn rollback_rejects_malformed_snapshot_id() {
    let err = GitPlugin
        .rollback("not-a-valid-snapshot-id")
        .await
        .expect_err("malformed snapshot id should fail");

    match err {
        SnapshotError::Snapshot(msg) => assert!(msg.contains("malformed snapshot_id")),
        other => panic!("expected snapshot error, got {other:?}"),
    }
}

// ── environment isolation (issue #317) ───────────────────────────────────

#[test]
fn git_command_removes_every_ambient_git_env_var() {
    let dir = TempDir::new().unwrap();
    let cmd = git_command(dir.path());
    let std_cmd = cmd.as_std();
    for var in GIT_ENV_VARS_TO_CLEAR {
        let entry = std_cmd
            .get_envs()
            .find(|(k, _)| *k == std::ffi::OsStr::new(var));
        assert_eq!(
            entry.map(|(_, v)| v),
            Some(None),
            "{var} should be marked removed on the spawned git command"
        );
    }
}

/// Exit code a child test process uses to report "checked, isolation
/// held": the child ran its check and the ambient `GIT_DIR` did not leak
/// in. Not `0`, so a renamed target (which makes `--exact` match nothing
/// and libtest exit `0` having run no test) cannot be mistaken for this.
const CHILD_ISOLATION_HELD: i32 = 42;

/// Exit code a child test process uses to report "checked, isolation
/// failed": the child ran its check and the ambient `GIT_DIR` leaked in.
const CHILD_ISOLATION_FAILED: i32 = 43;

/// Re-executes this test binary as a child process running only the
/// single test named `fn_name`, with a marker env var and a foreign
/// `GIT_DIR` set only on the child `Command` — never via
/// `std::env::set_var` in this process, which would leak into every
/// other test sharing this address space (tests run concurrently by
/// default). `fn_name` must be a `#[tokio::test]` in this module whose
/// body checks `AEGIS_GIT_ENV_CHILD` and exits with
/// [`CHILD_ISOLATION_HELD`] or [`CHILD_ISOLATION_FAILED`].
///
/// The `--exact` filter is built from `module_path!()` (this module's
/// own path, so a module rename or move breaks the build or the filter
/// automatically) plus `fn_name`. A function rename isn't caught at
/// compile time — stable Rust has no way to ask "what is my own name" —
/// but is still caught here: a stale `fn_name` makes `--exact` match no
/// test, libtest exits `0` having run nothing, and that `0` is treated
/// as failure below rather than silently accepted as success.
async fn assert_child_reports_isolation_held(fn_name: &str, env: &[(&str, &std::ffi::OsStr)]) {
    let module_path = module_path!()
        .split_once("::")
        .map_or(module_path!(), |(_crate_name, rest)| rest);
    let test_name = format!("{module_path}::{fn_name}");

    let exe = std::env::current_exe().unwrap();
    let mut child = std::process::Command::new(exe);
    child.args(["--exact", &test_name, "--nocapture"]);
    child.env("AEGIS_GIT_ENV_CHILD", "1");
    for (key, value) in env {
        child.env(key, value);
    }
    let status = child.status().expect("failed to re-exec test binary");

    match status.code() {
        Some(code) if code == CHILD_ISOLATION_HELD => {}
        Some(0) => panic!(
            "child ran no test for --exact {test_name:?}; libtest exits 0 when a \
             filter matches nothing, so this would silently pass if not checked — \
             the test this filter targets was likely renamed or moved"
        ),
        Some(code) if code == CHILD_ISOLATION_FAILED => {
            panic!("child at {test_name} reported isolation failed: ambient GIT_DIR leaked in")
        }
        other => panic!(
            "child process for {test_name} exited abnormally (code {other:?}); \
             treating as a crash, not a test result"
        ),
    }
}

#[tokio::test]
async fn is_applicable_ignores_ambient_git_dir_pointing_elsewhere() {
    if std::env::var_os("AEGIS_GIT_ENV_CHILD").is_some() {
        let non_repo_cwd =
            std::env::var("AEGIS_TEST_CWD").expect("parent must pass AEGIS_TEST_CWD");
        let applicable = GitPlugin.is_applicable(Path::new(&non_repo_cwd)).await;
        std::process::exit(if applicable {
            CHILD_ISOLATION_FAILED
        } else {
            CHILD_ISOLATION_HELD
        });
    }

    let foreign_repo = TempDir::new().unwrap();
    init_repo(foreign_repo.path()).await;
    let foreign_git_dir = foreign_repo.path().join(".git");
    let non_repo_cwd = TempDir::new().unwrap();

    assert_child_reports_isolation_held(
        "is_applicable_ignores_ambient_git_dir_pointing_elsewhere",
        &[
            ("AEGIS_TEST_CWD", non_repo_cwd.path().as_os_str()),
            ("GIT_DIR", foreign_git_dir.as_os_str()),
        ],
    )
    .await;
}

/// Same re-exec pattern as the `is_applicable` test above: the child runs
/// `snapshot()` on a non-repo tempdir under a foreign `GIT_DIR` and
/// reports what happened through its exit code, so the parent never has
/// to touch process-global environment state itself.
#[tokio::test]
async fn snapshot_does_not_stash_into_ambient_git_dir() {
    if std::env::var_os("AEGIS_GIT_ENV_CHILD").is_some() {
        let non_repo_cwd =
            std::env::var("AEGIS_TEST_CWD").expect("parent must pass AEGIS_TEST_CWD");
        let cwd = Path::new(&non_repo_cwd);
        // Isolated from the foreign GIT_DIR, `cwd` is not a real
        // repository, so `snapshot()` is expected to error out on its
        // own `git status` call — that is fine. What must never happen
        // is the file getting swept away into the foreign repo's stash.
        let _ = GitPlugin.snapshot(cwd, "rm -rf .").await;
        let file_survived = cwd.join("untracked.txt").exists();
        std::process::exit(if file_survived {
            CHILD_ISOLATION_HELD
        } else {
            CHILD_ISOLATION_FAILED
        });
    }

    let foreign_repo = TempDir::new().unwrap();
    init_repo(foreign_repo.path()).await;
    let foreign_git_dir = foreign_repo.path().join(".git");

    let non_repo_cwd = TempDir::new().unwrap();
    fs::write(
        non_repo_cwd.path().join("untracked.txt"),
        "do not steal me\n",
    )
    .unwrap();

    assert_child_reports_isolation_held(
        "snapshot_does_not_stash_into_ambient_git_dir",
        &[
            ("AEGIS_TEST_CWD", non_repo_cwd.path().as_os_str()),
            ("GIT_DIR", foreign_git_dir.as_os_str()),
        ],
    )
    .await;

    // The foreign repo's stash must still be empty: nothing was swept
    // into it on the child's behalf.
    let stash_list = git_command(foreign_repo.path())
        .args(["stash", "list"])
        .output()
        .await
        .unwrap();
    assert!(
        String::from_utf8_lossy(&stash_list.stdout)
            .trim()
            .is_empty(),
        "foreign repo's stash should still be empty"
    );
}

#[tokio::test]
async fn rollback_errors_when_stash_entry_not_found() {
    let dir = TempDir::new().unwrap();
    init_repo(dir.path()).await;

    let snapshot_id = format!(
        "{}\t0000000000000000000000000000000000000000",
        dir.path().display()
    );
    let err = GitPlugin
        .rollback(&snapshot_id)
        .await
        .expect_err("missing stash hash should fail");

    match err {
        SnapshotError::Snapshot(msg) => assert!(msg.contains("stash entry not found")),
        other => panic!("expected snapshot error, got {other:?}"),
    }
}

// ── the snapshot is not observable in the working tree (issue #356) ───────

/// Read `git status --porcelain` for `dir`.
async fn porcelain_status(dir: &std::path::Path) -> String {
    let out = git_command(dir)
        .args(["status", "--porcelain"])
        .output()
        .await
        .unwrap();
    String::from_utf8(out.stdout).unwrap()
}

/// Report whether `hash` is still listed as a stash entry in `dir`.
async fn stash_list_contains(dir: &std::path::Path, hash: &str) -> bool {
    let out = git_command(dir)
        .args(["stash", "list", "--format=%H"])
        .output()
        .await
        .unwrap();
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|line| line == hash)
}

#[tokio::test]
async fn snapshot_leaves_working_tree_unchanged() {
    let dir = TempDir::new().unwrap();
    init_repo(dir.path()).await;
    commit_file(dir.path(), "tracked.txt", "base\n").await;
    commit_file(dir.path(), "staged.txt", "base\n").await;

    fs::write(dir.path().join("tracked.txt"), "modified\n").unwrap();
    fs::write(dir.path().join("staged.txt"), "staged-change\n").unwrap();
    git_command(dir.path())
        .args(["add", "staged.txt"])
        .output()
        .await
        .unwrap();
    fs::write(dir.path().join("untracked.txt"), "scratch\n").unwrap();

    let before = porcelain_status(dir.path()).await;

    let snapshot_id = GitPlugin.snapshot(dir.path(), DANGEROUS_CMD).await.unwrap();
    assert_ne!(snapshot_id, CLEAN_SENTINEL, "expected a real stash");

    assert_eq!(
        porcelain_status(dir.path()).await,
        before,
        "git status must be byte-identical before and after a snapshot"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("tracked.txt")).unwrap(),
        "modified\n"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("staged.txt")).unwrap(),
        "staged-change\n"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("untracked.txt")).unwrap(),
        "scratch\n"
    );

    let (_, hash) = snapshot_id.split_once(SEP).unwrap();
    assert!(
        stash_list_contains(dir.path(), hash).await,
        "the stash entry must stay available for rollback"
    );
}

#[tokio::test]
async fn rollback_restores_state_after_a_command_deleted_files() {
    let dir = TempDir::new().unwrap();
    init_repo(dir.path()).await;
    commit_file(dir.path(), "tracked.txt", "base\n").await;

    fs::write(dir.path().join("tracked.txt"), "modified\n").unwrap();
    fs::write(dir.path().join("untracked.txt"), "scratch\n").unwrap();

    let snapshot_id = GitPlugin.snapshot(dir.path(), DANGEROUS_CMD).await.unwrap();

    // The dangerous command runs and deletes both files.
    fs::remove_file(dir.path().join("tracked.txt")).unwrap();
    fs::remove_file(dir.path().join("untracked.txt")).unwrap();

    GitPlugin.rollback(&snapshot_id).await.unwrap();

    assert_eq!(
        fs::read_to_string(dir.path().join("tracked.txt")).unwrap(),
        "modified\n"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("untracked.txt")).unwrap(),
        "scratch\n"
    );
}

#[tokio::test]
async fn rollback_parks_current_work_before_restoring() {
    let dir = TempDir::new().unwrap();
    init_repo(dir.path()).await;
    commit_file(dir.path(), "file.txt", "original\n").await;

    fs::write(dir.path().join("file.txt"), "snapshotted\n").unwrap();
    let snapshot_id = GitPlugin.snapshot(dir.path(), DANGEROUS_CMD).await.unwrap();

    // Work done after the snapshot, on top of the snapshotted content.
    fs::write(dir.path().join("file.txt"), "later-edit\n").unwrap();

    GitPlugin.rollback(&snapshot_id).await.unwrap();

    assert_eq!(
        fs::read_to_string(dir.path().join("file.txt")).unwrap(),
        "snapshotted\n",
        "rollback must restore the snapshotted content"
    );

    let list = git_command(dir.path())
        .args(["stash", "list", "--format=%gs"])
        .output()
        .await
        .unwrap();
    let list = String::from_utf8_lossy(&list.stdout);
    assert!(
        list.contains(PRE_ROLLBACK_PREFIX),
        "the work rollback replaced must be parked in a stash entry: {list}"
    );
}
