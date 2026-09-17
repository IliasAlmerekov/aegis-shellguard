//! Git snapshot provider — creates stashes before dangerous commands.

use std::path::Path;

use async_trait::async_trait;
use tokio::process::Command;

use crate::SnapshotPlugin;
use crate::error::SnapshotError;

type Result<T> = std::result::Result<T, SnapshotError>;

/// Sentinel value stored when the working tree had no changes to stash.
/// A real stash hash is always a 40-char hex string, so this cannot collide.
const CLEAN_SENTINEL: &str = "clean";

/// Separator between the encoded `cwd` and the stash hash in a `snapshot_id`.
/// Tab is not a valid path component on Unix or Windows, so it is safe to use
/// as an unambiguous delimiter.
const SEP: char = '\t';

/// Environment variables git reads to locate a repository instead of walking
/// up from `cwd`. This is the exact output of `git rev-parse --local-env-vars`
/// (git 2.43) — the list git itself clears when it enters another repository
/// (for example via `-C` or a submodule), so clearing it here on every spawn
/// gives `GitPlugin` the same isolation git already relies on internally.
///
/// A caller's ambient `GIT_DIR` (set, for instance, by git in a `pre-push`
/// hook running in a linked worktree) would otherwise override `current_dir`
/// and point every spawn at the wrong repository (issue #317).
const GIT_ENV_VARS_TO_CLEAR: &[&str] = &[
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CONFIG",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
    "GIT_OBJECT_DIRECTORY",
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_GRAFT_FILE",
    "GIT_INDEX_FILE",
    "GIT_NO_REPLACE_OBJECTS",
    "GIT_REPLACE_REF_BASE",
    "GIT_PREFIX",
    "GIT_SHALLOW_FILE",
    "GIT_COMMON_DIR",
];

/// Build a `git` command scoped to `cwd`, isolated from ambient git-location
/// environment variables. All nine `git` spawns in `GitPlugin` must go
/// through this instead of `Command::new("git")` directly — see
/// `GIT_ENV_VARS_TO_CLEAR`.
fn git_command(cwd: impl AsRef<Path>) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(cwd);
    for var in GIT_ENV_VARS_TO_CLEAR {
        cmd.env_remove(var);
    }
    cmd
}

/// Built-in Git snapshot provider (creates stashes before dangerous commands).
pub struct GitPlugin;

#[async_trait]
impl SnapshotPlugin for GitPlugin {
    fn name(&self) -> &'static str {
        "git"
    }

    async fn is_applicable(&self, cwd: &Path) -> bool {
        match git_command(cwd)
            .args(["rev-parse", "--git-dir"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
        {
            // git ran and told us definitively whether `cwd` is a repo.
            Ok(status) => {
                let applicable = status.success();
                tracing::debug!(
                    applicable,
                    cwd = %cwd.display(),
                    "git rev-parse --git-dir answered applicability check"
                );
                applicable
            }
            // We could not even spawn git, so we have no signal either way.
            // Fail open rather than silently reporting "not a repo": the
            // caller still attempts `snapshot()`, and if that also cannot
            // spawn git, the failure surfaces through the existing
            // `SNAPSHOT_FAILED_CONTINUING` warning instead of vanishing here.
            Err(error) => {
                tracing::error!(
                    %error,
                    cwd = %cwd.display(),
                    "failed to spawn git while checking applicability, assuming applicable"
                );
                true
            }
        }
    }

    async fn snapshot(&self, cwd: &Path, _cmd: &str) -> Result<String> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let message = format!("aegis-snap-{timestamp}");

        let status_out = git_command(cwd)
            .args(["status", "--porcelain"])
            .output()
            .await
            .map_err(|e| SnapshotError::Snapshot(format!("failed to run git status: {e}")))?;

        if !status_out.status.success() {
            return Err(SnapshotError::Snapshot(
                "git status --porcelain failed".to_string(),
            ));
        }

        if status_out.stdout.iter().all(|b| b.is_ascii_whitespace()) {
            tracing::info!("git working tree is clean, nothing to stash");
            return Ok(CLEAN_SENTINEL.to_string());
        }

        let stash_out = git_command(cwd)
            .args(["stash", "push", "--include-untracked", "-m", &message])
            .output()
            .await
            .map_err(|e| SnapshotError::Snapshot(format!("failed to run git stash: {e}")))?;

        if !stash_out.status.success() {
            let stderr = String::from_utf8_lossy(&stash_out.stderr);
            return Err(SnapshotError::Snapshot(format!(
                "git stash push failed: {stderr}"
            )));
        }

        let rev_out = git_command(cwd)
            .args(["rev-parse", "stash@{0}"])
            .output()
            .await
            .map_err(|e| SnapshotError::Snapshot(format!("git rev-parse failed: {e}")))?;

        if !rev_out.status.success() {
            return Err(SnapshotError::Snapshot(
                "could not resolve stash ref after push".to_string(),
            ));
        }

        let hash = String::from_utf8_lossy(&rev_out.stdout).trim().to_string();

        let snapshot_id = format!("{}{SEP}{hash}", cwd.display());
        tracing::info!(%snapshot_id, "git snapshot created");
        Ok(snapshot_id)
    }

    async fn rollback(&self, snapshot_id: &str) -> Result<()> {
        if snapshot_id == CLEAN_SENTINEL {
            tracing::info!("git snapshot was clean, nothing to roll back");
            return Ok(());
        }

        let (cwd_str, hash) = snapshot_id.split_once(SEP).ok_or_else(|| {
            SnapshotError::Snapshot(format!("malformed snapshot_id: {snapshot_id:?}"))
        })?;

        let list_out = git_command(cwd_str)
            .args(["stash", "list", "--format=%H %gd"])
            .output()
            .await
            .map_err(|e| SnapshotError::Snapshot(format!("git stash list failed: {e}")))?;

        if !list_out.status.success() {
            return Err(SnapshotError::Snapshot("git stash list failed".to_string()));
        }

        let list_stdout = String::from_utf8_lossy(&list_out.stdout);
        let stash_ref = list_stdout
            .lines()
            .find_map(|line| {
                let (h, r) = line.split_once(' ')?;
                (h == hash).then(|| r.to_string())
            })
            .ok_or_else(|| {
                SnapshotError::Snapshot(format!("stash entry not found for hash {hash}"))
            })?;

        let apply_out = git_command(cwd_str)
            .args(["stash", "apply", "--index", hash])
            .output()
            .await
            .map_err(|e| SnapshotError::Snapshot(format!("git stash apply failed: {e}")))?;

        if !apply_out.status.success() {
            let stderr = String::from_utf8_lossy(&apply_out.stderr);
            let stdout = String::from_utf8_lossy(&apply_out.stdout);
            let details = format!("{stdout}{stderr}").trim().to_string();

            tracing::error!(
                stash_ref = %stash_ref,
                details = %details,
                "git stash apply conflicted, stash entry is preserved for manual recovery"
            );

            return Err(SnapshotError::RollbackConflict {
                stash_ref,
                cwd: cwd_str.to_string(),
                details,
            });
        }

        let drop_out = git_command(cwd_str)
            .args(["stash", "drop", &stash_ref])
            .output()
            .await;
        if !drop_out.map(|o| o.status.success()).unwrap_or(false) {
            tracing::warn!(stash_ref = %stash_ref, "git stash drop failed after successful apply");
        }

        tracing::info!(stash_ref = %stash_ref, "git snapshot rolled back");
        Ok(())
    }

    async fn delete(&self, snapshot_id: &str) -> Result<()> {
        if snapshot_id == CLEAN_SENTINEL {
            tracing::info!("git snapshot was clean, nothing to delete");
            return Ok(());
        }

        let (cwd_str, hash) = snapshot_id.split_once(SEP).ok_or_else(|| {
            SnapshotError::Snapshot(format!("malformed snapshot_id: {snapshot_id:?}"))
        })?;

        let cwd_path = Path::new(cwd_str);
        if !cwd_path.exists() {
            tracing::info!(%snapshot_id, "git repository no longer exists, nothing to delete");
            return Ok(());
        }

        let list_out = git_command(cwd_path)
            .args(["stash", "list", "--format=%H %gd"])
            .output()
            .await
            .map_err(|e| SnapshotError::Snapshot(format!("git stash list failed: {e}")))?;

        if !list_out.status.success() {
            let stderr = String::from_utf8_lossy(&list_out.stderr);
            if stderr.to_lowercase().contains("not a git repository") {
                tracing::info!(%snapshot_id, "git repository no longer exists, nothing to delete");
                return Ok(());
            }
            return Err(SnapshotError::Snapshot(format!(
                "git stash list failed: {stderr}"
            )));
        }

        let list_stdout = String::from_utf8_lossy(&list_out.stdout);
        let stash_ref = list_stdout.lines().find_map(|line| {
            let (h, r) = line.split_once(' ')?;
            (h == hash).then(|| r.to_string())
        });

        let Some(stash_ref) = stash_ref else {
            tracing::info!(%snapshot_id, "git stash entry already removed");
            return Ok(());
        };

        let drop_out = git_command(cwd_path)
            .args(["stash", "drop", &stash_ref])
            .output()
            .await;

        match drop_out {
            Ok(output) if output.status.success() => {
                tracing::info!(%stash_ref, "git snapshot deleted");
                Ok(())
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if stderr
                    .to_lowercase()
                    .contains("is not a valid stash reference")
                {
                    tracing::info!(%stash_ref, "git stash entry already removed");
                    return Ok(());
                }
                Err(SnapshotError::DeleteFailed {
                    plugin: "git".to_string(),
                    snapshot_id: snapshot_id.to_string(),
                    source: format!("git stash drop failed: {stderr}"),
                })
            }
            Err(error) => Err(SnapshotError::DeleteFailed {
                plugin: "git".to_string(),
                snapshot_id: snapshot_id.to_string(),
                source: format!("git stash drop failed: {error}"),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Initialise a bare git repo with an empty initial commit so stash works.
    async fn init_repo(dir: &std::path::Path) {
        Command::new("git")
            .args(["init"])
            .current_dir(dir)
            .output()
            .await
            .unwrap();
        // Set local identity so stash commits don't depend on global git config.
        for (key, val) in [
            ("user.email", "test@aegis.dev"),
            ("user.name", "Aegis Test"),
        ] {
            Command::new("git")
                .args(["config", key, val])
                .current_dir(dir)
                .output()
                .await
                .unwrap();
        }
        // Stash requires at least one commit; create an empty one.
        Command::new("git")
            .args(["commit", "--allow-empty", "-m", "init"])
            .current_dir(dir)
            .output()
            .await
            .unwrap();
    }

    /// Write `content` to `name`, stage it, and commit it.
    async fn commit_file(dir: &std::path::Path, name: &str, content: &str) {
        fs::write(dir.join(name), content).unwrap();
        Command::new("git")
            .args(["add", name])
            .current_dir(dir)
            .output()
            .await
            .unwrap();
        Command::new("git")
            .args([
                "-c",
                "user.email=test@aegis.dev",
                "-c",
                "user.name=Aegis Test",
                "commit",
                "-m",
                &format!("add {name}"),
            ])
            .current_dir(dir)
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
            fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {
            }
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

        // File should be back to the committed version.
        assert_eq!(
            fs::read_to_string(dir.path().join("hello.txt"))
                .unwrap()
                .trim(),
            "original"
        );

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

        // File should have been swept into the stash.
        assert!(!new_file.exists(), "untracked file should be stashed");

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
        Command::new("git")
            .args(["add", "staged.txt"])
            .current_dir(dir.path())
            .output()
            .await
            .unwrap();

        // Leave a change to unstaged.txt outside the index.
        fs::write(dir.path().join("unstaged.txt"), "unstaged-change\n").unwrap();

        let snapshot_id = GitPlugin.snapshot(dir.path(), "rm -rf .").await.unwrap();
        assert_ne!(snapshot_id, CLEAN_SENTINEL);

        // Both files are back to committed state after the stash.
        assert_eq!(
            fs::read_to_string(dir.path().join("staged.txt"))
                .unwrap()
                .trim(),
            "base"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("unstaged.txt"))
                .unwrap()
                .trim(),
            "base"
        );

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
        let status = Command::new("git")
            .args(["diff", "--cached", "--name-only"])
            .current_dir(dir.path())
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
            "original"
        );

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
        let out = Command::new("git")
            .args([
                "worktree",
                "add",
                wt_dir.path().to_str().unwrap(),
                "HEAD",
                "--detach",
            ])
            .current_dir(main_dir.path())
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
        let out = Command::new("git")
            .args([
                "worktree",
                "add",
                wt_dir.path().to_str().unwrap(),
                "HEAD",
                "--detach",
            ])
            .current_dir(main_dir.path())
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
            "original"
        );

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

        // Introduce a conflicting change so stash pop cannot auto-merge.
        fs::write(dir.path().join("file.txt"), "conflicting-content\n").unwrap();

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

    /// Re-executes this test binary as a child process with a marker env var
    /// so the child branch runs under a foreign `GIT_DIR` while the parent
    /// branch observes the outcome through the child's exit code. This is the
    /// only way to test env isolation honestly: setting `GIT_DIR` via
    /// `std::env::set_var` in the test process itself would leak into every
    /// other test running in the same process (tests share one address
    /// space and `cargo test` runs them concurrently by default).
    #[tokio::test]
    async fn is_applicable_ignores_ambient_git_dir_pointing_elsewhere() {
        if std::env::var_os("AEGIS_GIT_ENV_CHILD").is_some() {
            let non_repo_cwd =
                std::env::var("AEGIS_TEST_CWD").expect("parent must pass AEGIS_TEST_CWD");
            let applicable = GitPlugin.is_applicable(Path::new(&non_repo_cwd)).await;
            std::process::exit(if applicable { 1 } else { 0 });
        }

        let foreign_repo = TempDir::new().unwrap();
        init_repo(foreign_repo.path()).await;
        let foreign_git_dir = foreign_repo.path().join(".git");

        let non_repo_cwd = TempDir::new().unwrap();

        let exe = std::env::current_exe().unwrap();
        let status = std::process::Command::new(exe)
            .args([
                "--exact",
                "git::tests::is_applicable_ignores_ambient_git_dir_pointing_elsewhere",
                "--nocapture",
            ])
            .env("AEGIS_GIT_ENV_CHILD", "1")
            .env("AEGIS_TEST_CWD", non_repo_cwd.path())
            .env("GIT_DIR", &foreign_git_dir)
            .status()
            .expect("failed to re-exec test binary");

        assert!(
            status.success(),
            "GitPlugin::is_applicable reported applicable=true for a non-repo cwd \
             under a foreign GIT_DIR (child exit code {:?}); ambient GIT_DIR leaked in",
            status.code()
        );
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
            std::process::exit(if file_survived { 0 } else { 1 });
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

        let exe = std::env::current_exe().unwrap();
        let status = std::process::Command::new(exe)
            .args([
                "--exact",
                "git::tests::snapshot_does_not_stash_into_ambient_git_dir",
                "--nocapture",
            ])
            .env("AEGIS_GIT_ENV_CHILD", "1")
            .env("AEGIS_TEST_CWD", non_repo_cwd.path())
            .env("GIT_DIR", &foreign_git_dir)
            .status()
            .expect("failed to re-exec test binary");

        assert!(
            status.success(),
            "GitPlugin::snapshot stashed the tempdir's file into a foreign GIT_DIR \
             instead of leaving it alone (child exit code {:?})",
            status.code()
        );

        // The foreign repo's stash must still be empty: nothing was swept
        // into it on the child's behalf.
        let stash_list = Command::new("git")
            .args(["stash", "list"])
            .current_dir(foreign_repo.path())
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
}
