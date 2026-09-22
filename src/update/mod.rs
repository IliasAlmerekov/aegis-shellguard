//! Opt-in npm update notice (ADR-038).
//!
//! Consent and cache state live under `~/.aegis/`, global and never in a
//! project `.aegis.toml` — a project must not be able to turn on an outbound
//! request on a developer's machine. The shell wrapper is the only automatic
//! trigger: on an interactive TTY, text output, with a stale cache, it spawns
//! one detached background child (this same binary, re-invoked with
//! [`INTERNAL_UPDATE_CHECK_FLAG`]) that performs the registry check and exits
//! without the parent ever waiting on it. Aegis never self-updates and never
//! executes npm; it only prints the version and the command to run.

mod error;
mod registry;
mod state;
mod version;

use std::io::Write;
use std::path::Path;
use std::time::Duration;

use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

pub use error::UpdateError;
pub use state::{Channel, UpdateState};

type Result<T> = std::result::Result<T, UpdateError>;

/// Argv flag that re-invokes this binary as the internal background
/// update-check child. Recognized by `main.rs` before clap parsing and Tokio
/// runtime construction, mirroring `analysis::INTERNAL_LANGUAGE_WORKER_FLAG`.
pub const INTERNAL_UPDATE_CHECK_FLAG: &str = "--internal-update-check";

const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const NOTICE_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
/// A lock older than this is assumed to belong to a crashed or killed child
/// and is reclaimed, so a single stuck lock cannot permanently silence
/// background checks.
const LOCK_STALE_AFTER: Duration = Duration::from_secs(120);

/// Result of a live registry check.
pub enum CheckOutcome {
    /// The registry answered and the cache was updated.
    Fetched {
        /// The version string the registry reported as `latest`.
        latest: String,
    },
    /// The network was unreachable or the response was malformed. The
    /// previous cache is left untouched.
    Failed {
        /// Why the check failed.
        error: UpdateError,
    },
}

/// `aegis update enable --channel npm`: opt in and select a channel.
pub fn enable(channel: Channel) -> Result<UpdateState> {
    enable_at(&state::state_path()?, channel)
}

fn enable_at(path: &Path, channel: Channel) -> Result<UpdateState> {
    let mut current = state::load(path)?;
    current.channel = Some(channel);
    current.consent = true;
    state::save(path, &current)?;
    Ok(current)
}

/// `aegis update disable`: opt out and clear the selected channel. The last
/// known cache is left in place — it is inert without consent.
pub fn disable() -> Result<UpdateState> {
    disable_at(&state::state_path()?)
}

fn disable_at(path: &Path) -> Result<UpdateState> {
    let mut current = state::load(path)?;
    current.consent = false;
    current.channel = None;
    state::save(path, &current)?;
    Ok(current)
}

/// `aegis update status`: read consent, channel, and cache state.
pub fn status() -> Result<UpdateState> {
    state::load(&state::state_path()?)
}

/// Run a live registry check now and persist the result on success. Used by
/// the explicit `aegis update check` command and by the background child the
/// shell wrapper spawns. Runs regardless of stored consent: an explicit
/// `check` invocation — interactive or backgrounded on the wrapper's own
/// schedule — is itself the authorization for that one request; `consent`
/// only gates whether the wrapper spawns that background invocation
/// automatically and whether a notice ever prints.
///
/// A failed check returns `Ok(CheckOutcome::Failed { .. })`, not `Err` — the
/// state file is simply left untouched, so a flaky network never corrupts or
/// clears the last valid cache record.
pub fn check_now(channel: Channel) -> Result<CheckOutcome> {
    check_now_with(
        &state::state_path()?,
        channel,
        registry::fetch_latest_version,
    )
}

fn check_now_with(
    path: &Path,
    channel: Channel,
    fetch: impl FnOnce(Channel) -> std::result::Result<String, UpdateError>,
) -> Result<CheckOutcome> {
    let mut current = state::load(path)?;

    let latest = match fetch(channel) {
        Ok(latest) => latest,
        Err(error) => return Ok(CheckOutcome::Failed { error }),
    };
    if let Err(error) = version::parse_strict(&latest) {
        return Ok(CheckOutcome::Failed { error });
    }

    current.latest_known_version = Some(latest.clone());
    current.last_success_check_at = Some(now_rfc3339()?);
    state::save(path, &current)?;
    Ok(CheckOutcome::Fetched { latest })
}

/// Print an update notice to stderr and record it as shown, if one is due
/// for `installed_version`. Callers must already have confirmed an
/// interactive TTY and non-CI, non-JSON, non-Watch, non-hook context — this
/// function does not re-check the environment, only consent and cache state.
/// Best-effort: any failure to read or write state is swallowed by the
/// caller, never surfaced to the wrapped command's exit code.
pub fn maybe_render_notice(installed_version: &str) -> Result<()> {
    maybe_render_notice_at(&state::state_path()?, installed_version)
}

fn maybe_render_notice_at(path: &Path, installed_version: &str) -> Result<()> {
    let mut current = state::load(path)?;
    if !current.consent {
        return Ok(());
    }
    let Some(latest) = current.latest_known_version.clone() else {
        return Ok(());
    };
    let Some(newer) = version::newer_version(installed_version, &latest) else {
        return Ok(());
    };
    if !notice_is_due(&current, &latest) {
        return Ok(());
    }

    let mut stderr = std::io::stderr();
    let _ = writeln!(
        stderr,
        "Aegis {newer} is available (installed: {installed_version})."
    );
    let _ = writeln!(stderr, "Update: npm i -g @iliasalmerekov/aegis@latest");

    current.last_notice_version = Some(latest);
    current.last_notice_at = Some(now_rfc3339()?);
    state::save(path, &current)?;
    Ok(())
}

fn notice_is_due(state: &UpdateState, latest: &str) -> bool {
    match (&state.last_notice_version, &state.last_notice_at) {
        (Some(version), Some(at)) if version == latest => match OffsetDateTime::parse(at, &Rfc3339)
        {
            Ok(at) => OffsetDateTime::now_utc() - at >= NOTICE_INTERVAL,
            Err(_) => true,
        },
        _ => true,
    }
}

fn cache_is_stale(state: &UpdateState) -> bool {
    match &state.last_success_check_at {
        Some(at) => match OffsetDateTime::parse(at, &Rfc3339) {
            Ok(at) => OffsetDateTime::now_utc() - at >= CHECK_INTERVAL,
            Err(_) => true,
        },
        None => true,
    }
}

/// The channel to background-check now, when consent is on, a channel is
/// selected, and the cache is stale — `None` otherwise.
fn spawn_channel_if_due(state: &UpdateState) -> Option<Channel> {
    if !state.consent {
        return None;
    }
    let channel = state.channel?;
    cache_is_stale(state).then_some(channel)
}

/// Spawn the detached background update-check child if consent is on, a
/// channel is selected, and the cache is stale. Best-effort and silent on
/// every failure path (missing `HOME`, corrupt state, lock contention,
/// `current_exe` unavailable) — never affects the wrapped command's exit
/// code, and never blocks waiting on the child.
pub fn maybe_spawn_background_check() {
    let Ok(state_path) = state::state_path() else {
        return;
    };
    let Ok(current) = state::load(&state_path) else {
        return;
    };
    let Some(channel) = spawn_channel_if_due(&current) else {
        return;
    };
    if !acquire_lock().unwrap_or(false) {
        return;
    }
    let Ok(exe) = std::env::current_exe() else {
        let _ = release_lock();
        return;
    };

    let spawned = std::process::Command::new(exe)
        .arg(INTERNAL_UPDATE_CHECK_FLAG)
        .arg(channel.as_arg())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();

    // The child releases the lock itself on exit (see
    // `run_internal_update_check`). If the spawn itself failed, release it
    // here so a single failed spawn does not permanently wedge future checks
    // for up to `LOCK_STALE_AFTER`.
    if spawned.is_err() {
        let _ = release_lock();
    }
}

fn acquire_lock() -> Result<bool> {
    acquire_lock_at(&state::lock_path()?)
}

fn acquire_lock_at(path: &Path) -> Result<bool> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // At most one reclaim attempt: try to create the lock, and if it already
    // exists and is stale, remove it and try exactly once more. A second
    // contender losing that retry backs off rather than looping.
    for _ in 0..2 {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
        {
            Ok(file) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
                }
                let mut file = file;
                let _ = writeln!(file, "pid={}", std::process::id());
                return Ok(true);
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                if !lock_is_stale(path) {
                    return Ok(false);
                }
                let _ = std::fs::remove_file(path);
            }
            Err(err) => return Err(err.into()),
        }
    }
    Ok(false)
}

fn lock_is_stale(path: &Path) -> bool {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .map(|modified| {
            modified
                .elapsed()
                .map(|age| age >= LOCK_STALE_AFTER)
                .unwrap_or(true)
        })
        .unwrap_or(true)
}

fn release_lock() -> Result<()> {
    release_lock_at(&state::lock_path()?)
}

fn release_lock_at(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

/// Entry point for the `--internal-update-check <channel>` child the shell
/// wrapper spawns. Not a user-facing command. Always releases the lock on
/// exit and always returns a best-effort exit code — nothing waits on it.
pub fn run_internal_update_check(channel: Channel) -> i32 {
    let result = check_now(channel);
    let _ = release_lock();
    match result {
        Ok(CheckOutcome::Fetched { .. } | CheckOutcome::Failed { .. }) => 0,
        Err(_) => 1,
    }
}

/// Parse the channel argument that follows [`INTERNAL_UPDATE_CHECK_FLAG`] in
/// `argv` and run the background check. Returns `0` (nothing to do, silently)
/// when the flag has no recognized channel argument.
pub fn run_internal_update_check_from_env() -> i32 {
    let channel = std::env::args()
        .skip_while(|arg| arg != INTERNAL_UPDATE_CHECK_FLAG)
        .nth(1)
        .and_then(|raw| Channel::from_arg(&raw));

    match channel {
        Some(channel) => run_internal_update_check(channel),
        None => {
            let _ = release_lock();
            0
        }
    }
}

fn now_rfc3339() -> Result<String> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|err| UpdateError::Internal {
            detail: format!("failed to format update timestamp: {err}"),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn cache_is_stale_when_never_checked() {
        assert!(cache_is_stale(&UpdateState::default()));
    }

    #[test]
    fn cache_is_stale_when_timestamp_is_unparseable() {
        let state = UpdateState {
            last_success_check_at: Some("not a timestamp".to_string()),
            ..UpdateState::default()
        };
        assert!(cache_is_stale(&state));
    }

    #[test]
    fn cache_is_fresh_right_after_a_check() {
        let state = UpdateState {
            last_success_check_at: Some(now_rfc3339().unwrap()),
            ..UpdateState::default()
        };
        assert!(!cache_is_stale(&state));
    }

    #[test]
    fn notice_is_due_on_first_sighting_of_a_version() {
        assert!(notice_is_due(&UpdateState::default(), "0.6.8"));
    }

    #[test]
    fn notice_is_not_due_twice_for_the_same_version_within_a_day() {
        let state = UpdateState {
            last_notice_version: Some("0.6.8".to_string()),
            last_notice_at: Some(now_rfc3339().unwrap()),
            ..UpdateState::default()
        };
        assert!(!notice_is_due(&state, "0.6.8"));
    }

    #[test]
    fn notice_is_due_again_for_a_newer_version_even_within_a_day() {
        let state = UpdateState {
            last_notice_version: Some("0.6.8".to_string()),
            last_notice_at: Some(now_rfc3339().unwrap()),
            ..UpdateState::default()
        };
        assert!(notice_is_due(&state, "0.6.9"));
    }

    #[test]
    fn channel_display_matches_the_arg_form() {
        assert_eq!(Channel::Npm.to_string(), "npm");
    }

    #[test]
    fn enable_then_disable_round_trips_consent_and_channel() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("update.json");

        let enabled = enable_at(&path, Channel::Npm).unwrap();
        assert!(enabled.consent);
        assert_eq!(enabled.channel, Some(Channel::Npm));

        let disabled = disable_at(&path).unwrap();
        assert!(!disabled.consent);
        assert_eq!(disabled.channel, None);
    }

    #[test]
    fn disable_preserves_the_last_known_cache() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("update.json");
        state::save(
            &path,
            &UpdateState {
                latest_known_version: Some("0.6.8".to_string()),
                consent: true,
                channel: Some(Channel::Npm),
                ..UpdateState::default()
            },
        )
        .unwrap();

        let disabled = disable_at(&path).unwrap();

        assert_eq!(disabled.latest_known_version.as_deref(), Some("0.6.8"));
    }

    #[test]
    fn check_now_leaves_cache_untouched_on_network_failure() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("update.json");
        state::save(
            &path,
            &UpdateState {
                latest_known_version: Some("0.6.7".to_string()),
                ..UpdateState::default()
            },
        )
        .unwrap();

        let outcome = check_now_with(&path, Channel::Npm, |_| {
            Err(UpdateError::NetworkUnavailable {
                detail: "connection refused".to_string(),
            })
        })
        .unwrap();

        assert!(matches!(outcome, CheckOutcome::Failed { .. }));
        let reloaded = state::load(&path).unwrap();
        assert_eq!(reloaded.latest_known_version.as_deref(), Some("0.6.7"));
    }

    #[test]
    fn check_now_fails_closed_on_a_malformed_registry_version() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("update.json");

        let outcome =
            check_now_with(&path, Channel::Npm, |_| Ok("not-a-version".to_string())).unwrap();

        assert!(matches!(outcome, CheckOutcome::Failed { .. }));
        assert!(!path.exists());
    }

    #[test]
    fn check_now_persists_a_fresh_version_on_success() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("update.json");

        let outcome = check_now_with(&path, Channel::Npm, |_| Ok("0.9.9".to_string())).unwrap();

        assert!(matches!(outcome, CheckOutcome::Fetched { ref latest } if latest == "0.9.9"));
        let reloaded = state::load(&path).unwrap();
        assert_eq!(reloaded.latest_known_version.as_deref(), Some("0.9.9"));
    }

    #[test]
    fn maybe_render_notice_is_a_noop_without_consent() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("update.json");

        maybe_render_notice_at(&path, "0.6.7").unwrap();

        assert!(!path.exists());
    }

    #[test]
    fn maybe_render_notice_is_a_noop_when_installed_is_current() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("update.json");
        state::save(
            &path,
            &UpdateState {
                consent: true,
                channel: Some(Channel::Npm),
                latest_known_version: Some("0.6.7".to_string()),
                ..UpdateState::default()
            },
        )
        .unwrap();

        maybe_render_notice_at(&path, "0.6.7").unwrap();

        let reloaded = state::load(&path).unwrap();
        assert_eq!(reloaded.last_notice_version, None);
    }

    #[test]
    fn maybe_render_notice_records_the_notice_once() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("update.json");
        state::save(
            &path,
            &UpdateState {
                consent: true,
                channel: Some(Channel::Npm),
                latest_known_version: Some("0.6.8".to_string()),
                ..UpdateState::default()
            },
        )
        .unwrap();

        maybe_render_notice_at(&path, "0.6.7").unwrap();

        let reloaded = state::load(&path).unwrap();
        assert_eq!(reloaded.last_notice_version.as_deref(), Some("0.6.8"));
        assert!(reloaded.last_notice_at.is_some());
    }

    #[test]
    fn spawn_channel_if_due_requires_consent_channel_and_staleness() {
        assert_eq!(spawn_channel_if_due(&UpdateState::default()), None);

        let no_consent = UpdateState {
            channel: Some(Channel::Npm),
            ..UpdateState::default()
        };
        assert_eq!(spawn_channel_if_due(&no_consent), None);

        let fresh = UpdateState {
            consent: true,
            channel: Some(Channel::Npm),
            last_success_check_at: Some(now_rfc3339().unwrap()),
            ..UpdateState::default()
        };
        assert_eq!(spawn_channel_if_due(&fresh), None);

        let due = UpdateState {
            consent: true,
            channel: Some(Channel::Npm),
            ..UpdateState::default()
        };
        assert_eq!(spawn_channel_if_due(&due), Some(Channel::Npm));
    }

    #[test]
    fn acquire_lock_then_second_attempt_is_rejected() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("update.lock");

        assert!(acquire_lock_at(&path).unwrap());
        assert!(!acquire_lock_at(&path).unwrap());
    }

    #[test]
    fn release_then_acquire_succeeds_again() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("update.lock");

        assert!(acquire_lock_at(&path).unwrap());
        release_lock_at(&path).unwrap();
        assert!(acquire_lock_at(&path).unwrap());
    }

    #[test]
    fn a_stale_lock_is_reclaimed() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("update.lock");
        std::fs::write(&path, "pid=1\n").unwrap();
        let stale_time = std::time::SystemTime::now() - LOCK_STALE_AFTER - Duration::from_secs(5);
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(stale_time)
            .unwrap();

        assert!(acquire_lock_at(&path).unwrap());
    }

    #[test]
    fn release_lock_on_a_missing_file_is_not_an_error() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("update.lock");

        assert!(release_lock_at(&path).is_ok());
    }
}
