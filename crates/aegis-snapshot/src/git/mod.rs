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

/// Message prefix of the stash entry `rollback` creates for the state it
/// replaces. Tests and recovery instructions both look for it.
const PRE_ROLLBACK_PREFIX: &str = "aegis-pre-rollback-";

/// Seconds since the Unix epoch, used to label stash entries.
fn unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Report whether `cwd`'s working tree holds anything a stash would capture:
/// staged changes, unstaged changes, or untracked files.
async fn working_tree_is_dirty(cwd: impl AsRef<Path>) -> Result<bool> {
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

    Ok(!status_out.stdout.iter().all(|b| b.is_ascii_whitespace()))
}

/// Find the positional `stash@{N}` reference of the entry whose commit is
/// `hash`, or `None` when the entry is gone.
async fn find_stash_ref(cwd: impl AsRef<Path>, hash: &str) -> Result<Option<String>> {
    let list_out = git_command(cwd)
        .args(["stash", "list", "--format=%H %gd"])
        .output()
        .await
        .map_err(|e| SnapshotError::Snapshot(format!("git stash list failed: {e}")))?;

    if !list_out.status.success() {
        return Err(SnapshotError::Snapshot("git stash list failed".to_string()));
    }

    let list_stdout = String::from_utf8_lossy(&list_out.stdout);
    Ok(list_stdout.lines().find_map(|line| {
        let (h, r) = line.split_once(' ')?;
        (h == hash).then(|| r.to_string())
    }))
}

/// Apply the stash entry `hash` onto `cwd`, index included. Returns `None`
/// when git restored the state, or the joined git output when it refused, so
/// each caller can wrap the same failure in the error its own path needs.
async fn apply_stash(cwd: impl AsRef<Path>, hash: &str) -> Result<Option<String>> {
    let apply_out = git_command(cwd)
        .args(["stash", "apply", "--index", hash])
        .output()
        .await
        .map_err(|e| SnapshotError::Snapshot(format!("git stash apply failed: {e}")))?;

    if apply_out.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&apply_out.stdout);
    let stderr = String::from_utf8_lossy(&apply_out.stderr);
    Ok(Some(format!("{stdout}{stderr}").trim().to_string()))
}

/// Move the current working-tree state into a stash entry of its own, so the
/// tree is back at HEAD and a snapshot entry can be applied onto it. Returns
/// the entry's commit hash, or `None` when the tree was already clean.
async fn park_current_work(cwd: &str) -> Result<Option<String>> {
    if !working_tree_is_dirty(cwd).await? {
        tracing::debug!(
            cwd,
            "working tree is clean before rollback, nothing to park"
        );
        return Ok(None);
    }

    let message = format!("{PRE_ROLLBACK_PREFIX}{}", unix_timestamp());

    let push_out = git_command(cwd)
        .args(["stash", "push", "--include-untracked", "-m", &message])
        .output()
        .await
        .map_err(|e| SnapshotError::Snapshot(format!("failed to run git stash push: {e}")))?;

    if !push_out.status.success() {
        let stderr = String::from_utf8_lossy(&push_out.stderr);
        return Err(SnapshotError::Snapshot(format!(
            "could not park the current working tree before rollback: {stderr}"
        )));
    }

    let rev_out = git_command(cwd)
        .args(["rev-parse", "stash@{0}"])
        .output()
        .await;

    let rev_out = match rev_out {
        Ok(output) => output,
        Err(error) => {
            return Err(SnapshotError::SnapshotNotRestoredUnknown {
                cwd: cwd.to_string(),
                details: format!("git rev-parse failed after parking current work: {error}"),
            });
        }
    };

    if !rev_out.status.success() {
        return Err(SnapshotError::SnapshotNotRestoredUnknown {
            cwd: cwd.to_string(),
            details: "could not resolve the stash entry that parked the working tree".to_string(),
        });
    }

    let hash = String::from_utf8_lossy(&rev_out.stdout).trim().to_string();
    tracing::info!(
        cwd,
        %message,
        stash_hash = %hash,
        "parked the current working tree in a stash entry before rollback"
    );
    Ok(Some(hash))
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
        let message = format!("aegis-snap-{}", unix_timestamp());

        if !working_tree_is_dirty(cwd).await? {
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
            .await;

        let rev_out = match rev_out {
            Ok(output) => output,
            Err(error) => {
                return Err(SnapshotError::SnapshotNotRestoredUnknown {
                    cwd: cwd.display().to_string(),
                    details: format!("git rev-parse failed: {error}"),
                });
            }
        };

        if !rev_out.status.success() {
            return Err(SnapshotError::SnapshotNotRestoredUnknown {
                cwd: cwd.display().to_string(),
                details: "could not resolve stash ref after push".to_string(),
            });
        }

        let hash = String::from_utf8_lossy(&rev_out.stdout).trim().to_string();

        // `git stash push` writes the entry *and* resets the working tree to
        // HEAD. Put the captured work straight back: a snapshot records state,
        // it must never be observable in the tree (issue #356). The tree is at
        // HEAD here, so this apply has nothing to merge against and the entry
        // stays in the stash list for a later rollback.
        let restored = match apply_stash(cwd, &hash).await {
            Ok(restored) => restored,
            Err(error) => {
                return Err(SnapshotError::SnapshotNotRestored {
                    stash_hash: hash,
                    cwd: cwd.display().to_string(),
                    details: error.to_string(),
                });
            }
        };
        if let Some(details) = restored {
            tracing::error!(
                stash_hash = %hash,
                details = %details,
                "working tree could not be restored after the snapshot stash"
            );

            return Err(SnapshotError::SnapshotNotRestored {
                stash_hash: hash,
                cwd: cwd.display().to_string(),
                details,
            });
        }

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

        // Resolve the entry before anything is written: a rollback that cannot
        // find its snapshot must fail without touching the working tree.
        find_stash_ref(cwd_str, hash).await?.ok_or_else(|| {
            SnapshotError::Snapshot(format!("stash entry not found for hash {hash}"))
        })?;

        // The snapshot left the working tree as it found it, so whatever is on
        // disk now would make `git stash apply` refuse to overwrite it. Park
        // that state in its own stash entry first: a rollback replaces the
        // current state with the captured one, it never destroys it.
        park_current_work(cwd_str).await?;

        // Parking shifts every `stash@{N}`, so resolve the positional ref again.
        let stash_ref = find_stash_ref(cwd_str, hash).await?.ok_or_else(|| {
            SnapshotError::Snapshot(format!(
                "stash entry disappeared while parking current work; parked work was retained and rollback stopped for hash {hash}"
            ))
        })?;

        if let Some(details) = apply_stash(cwd_str, hash).await? {
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
