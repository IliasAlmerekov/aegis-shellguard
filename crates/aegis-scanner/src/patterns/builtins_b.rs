use std::borrow::Cow;

use aegis_types::RiskLevel;

use super::{Category, PatternSource, PatternToken, PrefixRule, a, any_star, s};
pub(super) fn rules() -> Vec<PrefixRule> {
    vec![
        // ── Docker ────────────────────────────────────────────────────────────
        PrefixRule {
            id: Cow::Borrowed("DK-001"),
            category: Category::Docker,
            pattern: vec![s("docker"), s("system"), s("prune")],
            risk: RiskLevel::Warn,
            description: Cow::Borrowed(
                "docker system prune — removes all stopped containers, dangling images, unused networks, and build cache",
            ),
            safe_alt: Some(Cow::Borrowed(
                "Use '--filter until=24h' to limit pruning to older resources only",
            )),
            justification: Some(Cow::Borrowed(
                "Removes stopped containers, dangling images, networks, and build cache. Some of these may be needed for rollback or debugging.",
            )),
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &["docker system prune -f"],
            not_match_examples: &["docker system info"],
        },
        PrefixRule {
            id: Cow::Borrowed("DK-002"),
            category: Category::Docker,
            pattern: vec![s("docker"), s("volume"), s("prune")],
            risk: RiskLevel::Warn,
            description: Cow::Borrowed(
                "docker volume prune — removes all unused Docker volumes including any persistent data they hold",
            ),
            safe_alt: Some(Cow::Borrowed(
                "List volumes first: 'docker volume ls' and back up data before pruning",
            )),
            justification: Some(Cow::Borrowed(
                "Deletes all unused volumes. If a volume is unmounted but contains important data, it will be lost permanently.",
            )),
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &["docker volume prune -f"],
            not_match_examples: &["docker volume ls"],
        },
        PrefixRule {
            id: Cow::Borrowed("DK-003"),
            category: Category::Docker,
            pattern: vec![
                s("docker-compose"),
                s("down"),
                PatternToken::AnyStar,
                s("-v"),
            ],
            risk: RiskLevel::Warn,
            description: Cow::Borrowed(
                "docker-compose down -v — stops services and removes named volumes, deleting persistent data",
            ),
            safe_alt: Some(Cow::Borrowed(
                "Omit '-v' to keep volumes: 'docker-compose down' preserves volume data",
            )),
            justification: Some(Cow::Borrowed(
                "The -v flag removes named volumes, deleting persistent data that would otherwise survive container restarts.",
            )),
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &["docker-compose down -v"],
            not_match_examples: &["docker-compose up"],
        },
        PrefixRule {
            id: Cow::Borrowed("DK-003"),
            category: Category::Docker,
            pattern: vec![
                s("docker"),
                s("compose"),
                s("down"),
                PatternToken::AnyStar,
                s("-v"),
            ],
            risk: RiskLevel::Warn,
            description: Cow::Borrowed(
                "docker compose down -v — stops services and removes named volumes, deleting persistent data",
            ),
            safe_alt: Some(Cow::Borrowed(
                "Omit '-v' to keep volumes: 'docker compose down' preserves volume data",
            )),
            justification: Some(Cow::Borrowed(
                "The -v flag removes named volumes, deleting persistent data that would otherwise survive container restarts.",
            )),
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &["docker compose down -v"],
            not_match_examples: &["docker compose up"],
        },
        PrefixRule {
            id: Cow::Borrowed("DK-004"),
            category: Category::Docker,
            pattern: vec![s("docker"), s("rmi")],
            risk: RiskLevel::Warn,
            description: Cow::Borrowed(
                "docker rmi — removes Docker images; rebuild time is lost if image is deleted unintentionally",
            ),
            safe_alt: Some(Cow::Borrowed(
                "Tag images you want to keep before running bulk rmi commands",
            )),
            justification: Some(Cow::Borrowed(
                "Deleting images forces rebuilds and removes layers that other images may depend on.",
            )),
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &["docker rmi my-image:latest"],
            not_match_examples: &["docker image ls"],
        },
        PrefixRule {
            id: Cow::Borrowed("DK-005"),
            category: Category::Docker,
            pattern: vec![s("docker"), s("container"), s("prune")],
            risk: RiskLevel::Warn,
            description: Cow::Borrowed(
                "docker container prune — removes all stopped containers, including those with useful logs or data",
            ),
            safe_alt: Some(Cow::Borrowed(
                "Inspect stopped containers first: 'docker ps -a' before pruning",
            )),
            justification: Some(Cow::Borrowed(
                "Removes all stopped containers, including those with useful logs or forensic evidence.",
            )),
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &["docker container prune -f"],
            not_match_examples: &["docker container ls"],
        },
        PrefixRule {
            id: Cow::Borrowed("DK-006"),
            category: Category::Docker,
            pattern: vec![s("docker"), s("network"), s("prune")],
            risk: RiskLevel::Warn,
            description: Cow::Borrowed(
                "docker network prune — removes all unused Docker networks; can break containers that reconnect",
            ),
            safe_alt: Some(Cow::Borrowed(
                "List networks in use: 'docker network ls' before pruning",
            )),
            justification: Some(Cow::Borrowed(
                "Removes networks that disconnected containers may still reference, causing reconnection failures.",
            )),
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &["docker network prune -f"],
            not_match_examples: &["docker network ls"],
        },
        PrefixRule {
            id: Cow::Borrowed("DK-007"),
            category: Category::Docker,
            pattern: vec![s("docker"), s("volume"), s("rm")],
            risk: RiskLevel::Danger,
            description: Cow::Borrowed(
                "docker volume rm — deletes a named Docker volume, usually the only copy of a database's data",
            ),
            safe_alt: Some(Cow::Borrowed(
                "Back up the volume first: 'docker run --rm -v pgdata:/data -v $PWD:/backup alpine tar czf /backup/pgdata.tgz -C /data .'",
            )),
            justification: Some(Cow::Borrowed(
                "Danger, deliberately breaking parity with the six other DK-* rules (all Warn): prune collects garbage, rm <name> destroys the volume the user named. Equating them would understate the second. docker rm -v (removing anonymous volumes alongside a container) is not folded in — a different argument model and false-positive profile.",
            )),
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &["docker volume rm pgdata", "docker volume rm -f pgdata"],
            not_match_examples: &["docker volume ls", "docker volume inspect pgdata"],
        },
        // ── Process ───────────────────────────────────────────────────────────
        PrefixRule {
            id: Cow::Borrowed("PS-001"),
            category: Category::Process,
            pattern: vec![
                s("kill"),
                PatternToken::Alts(vec![
                    Cow::Borrowed("-9"),
                    Cow::Borrowed("-KILL"),
                    Cow::Borrowed("-SIGKILL"),
                ]),
                s("1"),
            ],
            risk: RiskLevel::Block,
            description: Cow::Borrowed(
                "kill -9 1 — sends SIGKILL to PID 1 (init/systemd), immediately crashing the entire system",
            ),
            safe_alt: Some(Cow::Borrowed(
                "Use 'systemctl stop <service>' to stop individual services safely",
            )),
            justification: Some(Cow::Borrowed(
                "SIGKILL to PID 1 crashes the entire system immediately. There is no graceful shutdown of services or sync of data.",
            )),
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &["kill -9 1"],
            not_match_examples: &["kill -15 1"],
        },
        PrefixRule {
            id: Cow::Borrowed("PS-002"),
            category: Category::Process,
            pattern: vec![s("pkill"), PatternToken::AnyStar, s("-9")],
            risk: RiskLevel::Warn,
            description: Cow::Borrowed(
                "pkill -9 — sends SIGKILL to all matching processes with no chance for cleanup or graceful shutdown",
            ),
            safe_alt: Some(Cow::Borrowed(
                "Use 'pkill -15' (SIGTERM) first to allow graceful shutdown before escalating",
            )),
            justification: Some(Cow::Borrowed(
                "SIGKILL prevents cleanup. Databases, editors, and services may leave corrupted files or lose unsaved work.",
            )),
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &["pkill -9 nginx"],
            not_match_examples: &["pkill -15 nginx"],
        },
        // PS-008 is to PS-006 what FS-020 is to FS-001: it finds a recursive
        // delete of `/` in any flag order (GHSA-7gcj-4f7x-7fxj). It also takes
        // `//`, `/.`, and `/..`, which PS-006 misses. The pattern only feeds
        // the program index; `matches_tokens` decides the match.
        PrefixRule {
            id: Cow::Borrowed("PS-008"),
            category: Category::Process,
            pattern: vec![s("rm"), any_star(), s("/")],
            risk: RiskLevel::Block,
            description: Cow::Borrowed(
                "rm -r / — recursively deletes the root filesystem regardless of flag order; unrecoverable system destruction",
            ),
            safe_alt: Some(Cow::Borrowed(
                "There is no safe alternative; this command must not be run",
            )),
            justification: Some(Cow::Borrowed(
                "This recursively deletes everything on the root filesystem. Flag order and -f do not change the result: the machine stops working and the data is gone.",
            )),
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &[
                "rm -v -rf /",
                "rm / -rf",
                "rm -r /",
                "rm --no-preserve-root -rf /",
                "rm -v -rf --no-preserve-root /",
                "rm -r -- /",
                "rm -rf //",
                "rm -rf /.",
                "rm -r /..",
            ],
            not_match_examples: &[
                "rm file",
                "rm -r /home",
                "rm -rf /*",
                "rm -- -r /",
                "rm /",
                "rm -rf //home",
                "rm -rf /.git",
            ],
        },
        PrefixRule {
            id: Cow::Borrowed("PS-005"),
            category: Category::Filesystem,
            pattern: vec![s("chmod"), PatternToken::AnyStar, s("777"), s("/")],
            risk: RiskLevel::Danger,
            description: Cow::Borrowed(
                "chmod 777 / — makes the root filesystem world-writable, creating a severe security vulnerability",
            ),
            safe_alt: Some(Cow::Borrowed(
                "Apply permissions only to the specific directory that needs them",
            )),
            justification: Some(Cow::Borrowed(
                "World-writable root allows any user to modify system binaries and config. This is a critical security vulnerability.",
            )),
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &["chmod 777 /"],
            not_match_examples: &["chmod 755 /"],
        },
        // FS-019 keys on the recursive target rather than the mode: a
        // recursive chmod of a system root can break the machine even when the
        // requested mode is not 000 or 777. Trailing-slash spellings remain
        // explicit to this rule rather than changing shared token equality.
        PrefixRule {
            id: Cow::Borrowed("FS-019"),
            category: Category::Filesystem,
            pattern: vec![
                s("chmod"),
                PatternToken::AnyStar,
                PatternToken::ShortFlag {
                    short: 'R',
                    long: &["--recursive"],
                },
                PatternToken::AnyStar,
                PatternToken::Alts(vec![
                    Cow::Borrowed("/"),
                    Cow::Borrowed("/usr"),
                    Cow::Borrowed("/usr/"),
                    Cow::Borrowed("/etc"),
                    Cow::Borrowed("/etc/"),
                    Cow::Borrowed("/bin"),
                    Cow::Borrowed("/bin/"),
                    Cow::Borrowed("/sbin"),
                    Cow::Borrowed("/sbin/"),
                    Cow::Borrowed("/lib"),
                    Cow::Borrowed("/lib/"),
                    Cow::Borrowed("/var"),
                    Cow::Borrowed("/var/"),
                    Cow::Borrowed("/boot"),
                    Cow::Borrowed("/boot/"),
                ]),
            ],
            risk: RiskLevel::Danger,
            description: Cow::Borrowed(
                "chmod -R on a system root — recursively rewrites permissions across critical system files",
            ),
            safe_alt: Some(Cow::Borrowed(
                "Apply permissions only to a specific application-owned directory",
            )),
            justification: Some(Cow::Borrowed(
                "Recursive chmod over /, /usr, /etc, /bin, /sbin, /lib, /var, or /boot can break boot, service, authentication, and package-managed files regardless of the requested mode. Danger rather than Block because a throwaway container can have a deliberate use.",
            )),
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &[
                "chmod -R 000 /",
                "chmod -R 755 /usr",
                "chmod -R 700 /etc",
                "chmod -Rf 000 /bin",
                "chmod --recursive 000 /sbin",
                "chmod -R 000 /lib",
                "chmod -R 000 /var",
                "chmod -R 000 /boot",
                "chmod -R 000 /usr/",
            ],
            not_match_examples: &[
                "chmod -r 000 /",
                "chmod -R 000 ./build",
                "chmod 000 /etc/passwd",
                "chmod -R 000 /*",
                "cd /usr && chmod -R 000 .",
            ],
        },
        // ── Package ───────────────────────────────────────────────────────────
        PrefixRule {
            id: Cow::Borrowed("PKG-005"),
            category: Category::Package,
            pattern: vec![
                s("pip"),
                s("install"),
                PatternToken::AnyStar,
                s("--trusted-host"),
            ],
            risk: RiskLevel::Warn,
            description: Cow::Borrowed(
                "pip install --trusted-host — disables SSL verification for the specified host",
            ),
            safe_alt: Some(Cow::Borrowed(
                "Fix the SSL/TLS issue instead of bypassing verification; use a proper certificate",
            )),
            justification: Some(Cow::Borrowed(
                "Disables TLS certificate validation. An attacker on the network can inject malicious packages during install.",
            )),
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &["pip install requests --trusted-host pypi.org"],
            not_match_examples: &["pip install requests"],
        },
        // ── Outbound irreversible actions ────────────────────────────────────
        // PKG-006/007 guard outbound irreversible actions: they destroy nothing
        // locally but cannot be undone once directed outward. PKG-006 carries
        // the negative condition --dry-run so the rehearsal an agent runs first
        // stays Safe (issue #194, ADR-025's sibling for the outbound class).
        PrefixRule {
            id: Cow::Borrowed("PKG-006"),
            category: Category::Package,
            pattern: vec![s("npm"), s("publish")],
            risk: RiskLevel::Warn,
            description: Cow::Borrowed(
                "npm publish — an outbound irreversible action: republishing a version is forbidden, so an unattended publish cannot be undone",
            ),
            safe_alt: Some(Cow::Borrowed(
                "Run 'npm publish --dry-run' to rehearse; keep the package private and publish only with explicit human approval",
            )),
            justification: Some(Cow::Borrowed(
                "Warn rather than Danger: publishing is a normal intended act that must not happen unattended, but it is not destruction. The negative condition --dry-run keeps the rehearsal silent — a rule that shouts at a dry run is disabled on day one.",
            )),
            source: PatternSource::Builtin,
            suppressed_by: &["--dry-run"],
            match_examples: &["npm publish", "npm publish --access public"],
            not_match_examples: &["npm publish --dry-run"],
        },
        PrefixRule {
            id: Cow::Borrowed("PKG-007"),
            category: Category::Package,
            pattern: vec![s("npm"), s("unpublish")],
            risk: RiskLevel::Danger,
            description: Cow::Borrowed(
                "npm unpublish — an outbound irreversible action that breaks every consumer depending on the published version",
            ),
            safe_alt: Some(Cow::Borrowed(
                "Deprecate the package ('npm deprecate') instead of unpublishing, and coordinate the removal with its consumers",
            )),
            justification: Some(Cow::Borrowed(
                "Danger, deliberately above its publish sibling: unpublishing breaks consumers already depending on the version, which is strictly worse than publishing.",
            )),
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &["npm unpublish", "npm unpublish --force pkg"],
            not_match_examples: &["npm publish"],
        },
        // ── Aegis self-management (ADR-035) ───────────────────────────────────
        //
        // The agent hook denies a command whose first word is literally
        // `aegis`, which leaves `/usr/bin/aegis off`, `env aegis off` and
        // `echo hi && aegis off` to the scanner. These rules classify the
        // command itself, so launcher and absolute-path normalization
        // (ADR-014) covers every spelling in every position.
        //
        // Commands that only tighten enforcement (`aegis on`) or only read
        // state (`status`, `audit`, `snapshot list`, `config show`,
        // `config validate`) carry no rule and stay Safe.
        PrefixRule {
            id: Cow::Borrowed("AEG-001"),
            category: Category::Aegis,
            pattern: vec![s("aegis"), s("off")],
            risk: RiskLevel::Danger,
            description: Cow::Borrowed(
                "aegis off — turns off the guardrail, so every later command runs unchecked",
            ),
            safe_alt: Some(Cow::Borrowed(
                "Leave the toggle on and approve the one command you need, or run 'aegis off' yourself in a terminal Aegis does not proxy",
            )),
            justification: Some(Cow::Borrowed(
                "Turning the toggle off removes classification, confirmation, and snapshots from every command that follows. Whether Aegis guards this machine is the operator's decision, not the agent's.",
            )),
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &["aegis off"],
            not_match_examples: &["aegis on", "aegis status"],
        },
        PrefixRule {
            id: Cow::Borrowed("AEG-002"),
            category: Category::Aegis,
            pattern: vec![s("aegis"), s("rollback")],
            risk: RiskLevel::Danger,
            description: Cow::Borrowed(
                "aegis rollback — restores a recorded snapshot over the current working tree",
            ),
            safe_alt: Some(Cow::Borrowed(
                "Inspect the snapshot first with 'aegis snapshot list' and commit or stash current work before restoring",
            )),
            justification: Some(Cow::Borrowed(
                "Restoring a snapshot overwrites the files it covers. Uncommitted work made after the snapshot was taken is lost.",
            )),
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &["aegis rollback snap-123"],
            not_match_examples: &["aegis snapshot list"],
        },
        PrefixRule {
            id: Cow::Borrowed("AEG-003"),
            category: Category::Aegis,
            pattern: vec![
                s("aegis"),
                s("snapshot"),
                s("prune"),
                any_star(),
                s("--yes"),
            ],
            risk: RiskLevel::Danger,
            description: Cow::Borrowed(
                "aegis snapshot prune --yes — deletes snapshot artifacts, removing the recovery path for earlier commands",
            ),
            safe_alt: Some(Cow::Borrowed(
                "Preview the deletion first: 'aegis snapshot prune --dry-run'",
            )),
            justification: Some(Cow::Borrowed(
                "Pruned snapshot artifacts cannot be restored. Without them 'aegis rollback' has nothing to roll back to.",
            )),
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &["aegis snapshot prune --yes"],
            not_match_examples: &["aegis snapshot prune --dry-run", "aegis snapshot prune"],
        },
        PrefixRule {
            id: Cow::Borrowed("AEG-004"),
            category: Category::Aegis,
            pattern: vec![s("aegis"), s("config"), s("init")],
            risk: RiskLevel::Warn,
            description: Cow::Borrowed(
                "aegis config init — writes a project-local .aegis.toml that changes how commands in this directory are judged",
            ),
            safe_alt: Some(Cow::Borrowed(
                "Read the active configuration first: 'aegis config show'",
            )),
            justification: Some(Cow::Borrowed(
                "A project config overwrites an existing .aegis.toml and then takes part in every later assessment. The project layer can only tighten security fields (ADR-013), so this is Warn rather than Danger.",
            )),
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &["aegis config init"],
            not_match_examples: &["aegis config show", "aegis config validate"],
        },
        PrefixRule {
            id: Cow::Borrowed("AEG-005"),
            category: Category::Aegis,
            pattern: vec![s("aegis"), a(&["install-hooks", "install"])],
            risk: RiskLevel::Warn,
            description: Cow::Borrowed(
                "aegis install-hooks — rewrites the agent settings file that decides which commands reach Aegis",
            ),
            safe_alt: Some(Cow::Borrowed(
                "Install the hooks yourself in a terminal Aegis does not proxy, then let the agent confirm with 'aegis status'",
            )),
            justification: Some(Cow::Borrowed(
                "The hook entry in the agent's settings file is where interception starts. Rewriting it changes which commands Aegis ever sees.",
            )),
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &["aegis install-hooks --all", "aegis install --claude-code"],
            not_match_examples: &["aegis status"],
        },
        PrefixRule {
            id: Cow::Borrowed("AEG-006"),
            category: Category::Aegis,
            pattern: vec![s("aegis"), s("setup-shell")],
            risk: RiskLevel::Warn,
            description: Cow::Borrowed(
                "aegis setup-shell — edits a shell startup file and the SHELL it points at for new terminal sessions",
            ),
            safe_alt: Some(Cow::Borrowed(
                "Edit the startup file yourself so you can review the managed block before it takes effect",
            )),
            justification: Some(Cow::Borrowed(
                "This writes to ~/.zshrc or ~/.bashrc and decides whether new sessions run through Aegis. '--remove' takes the proxy away entirely.",
            )),
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &["aegis setup-shell --remove", "aegis setup-shell"],
            not_match_examples: &["aegis status"],
        },
    ]
}
