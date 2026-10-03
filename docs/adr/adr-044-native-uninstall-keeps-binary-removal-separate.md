# ADR-044: Native uninstall keeps binary removal separate

## Status

Accepted (#375).

## Context

Aegis supports curl, npm, Homebrew, and Cargo installations. The shell fallback
only removes the curl binary, but integrations belong to Aegis regardless of
installation channel. Data in `~/.aegis` includes audit logs and snapshots that
an operator may need after removal.

## Decision

Add `aegis uninstall` to remove managed shell blocks, hook registrations, and
hook payloads across channels. Reuse the installer's managed-command predicates
so unrelated hooks stay intact. Keep user configuration and data by default.
Delete `~/.aegis` only with `--purge-data`; print the data decision either way.
Allow an absolute custom startup file through `--rc-file`. Resolve the
symlinks in the directories above it first, so a path through a linked
directory such as macOS `/var` or `/tmp` names the real file; the file itself
is not resolved and stays subject to the symlink check below.

Do not execute package managers or delete the running binary. Infer a channel
from known global npm, Homebrew, Cargo, and curl paths, then print its separate
removal command. The inferred or explicit channel is the Removal channel. Match
against the canonical path of the running binary (`fs::canonicalize`, falling
back to the invoked path), so a Homebrew symlink in the curl bindir is detected
as Homebrew. The printed curl `rm --` target is that canonical path. Label
inference as a hint, not proof of package ownership.
Unknown paths require an explicit `--channel` choice before giving channel
advice. Local npm paths must not yield a global npm removal command.

Preflight settings and managed shell blocks before mutation. Reject a symlinked
target or ancestor only for a file uninstall would change: agent settings, hook
payloads, and a startup file that holds a managed block. A settings file is
checked only when it holds an Aegis registration, and a hook payload only when
it exists. A symlinked startup file without a block, or a symlinked `~/.claude`
or `~/.codex` with nothing of Aegis inside, is left alone; dotfile managers
create both. A symlinked
`~/.aegis` is kept and not followed, including the toggle-state payload inside
it; `--purge-data` refuses it. Each refusal names the file and the step
that finishes its cleanup by hand. Replace changed configuration files atomically,
keep their permissions and the line endings of kept lines. Data purge does not
follow descendant symlinks.

Report which startup files lost a block. When none did and `$SHELL` resolves to
the running binary, warn that a block may live in a file set up with
`--rc-file` and name the flag that cleans it.
The scanner classifies uninstall as a Danger Self-management command (ADR-035).

Clean only the global integrations. Project-local hooks written by
`aegis install-hooks --local` are reported in the success output, not removed;
the operator cleans them by hand.

Leave the Codex `features.hooks` flag that `install-hooks` turns on in
`~/.codex/config.toml`. Other Codex hooks may need it. The success output
says the flag stays enabled when it is on.

Keep `scripts/uninstall.sh` as the curl-specific fallback. It may probe package
managers for advice but never invokes their uninstall commands. It resolves the
bindir target through symlinks with a portable `readlink` loop and leaves a link
that resolves into `/Cellar/aegis/` alone, advising `brew uninstall aegis`.

## Consequences

- Operators run integration cleanup before their channel's binary removal.
- Channel hints can be wrong for manually copied binaries. Explicit selection
  lets the operator resolve ambiguity without automatic destructive guesses.
- Project-local hooks outlive the binary and the shim then denies every Bash
  command in those projects until the operator removes them.
- Malformed settings, shell blocks, or symlinked integration targets stop native
  cleanup before writes. Repair those files before retrying.
- Cleanup is not transactional across files. A runtime I/O failure can leave
  partial cleanup; the command returns an error rather than claiming completion.
- Preflight checks are not a security boundary against concurrent filesystem
  replacement by another process running with the operator's permissions.
