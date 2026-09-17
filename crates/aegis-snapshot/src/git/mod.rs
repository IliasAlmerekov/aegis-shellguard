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
/// environment variables. Every git spawn in this module and its tests goes
/// through this instead of spawning `git` directly. See
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
mod tests;
