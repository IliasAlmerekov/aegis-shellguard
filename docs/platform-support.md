# Platform support

## Support matrix

| Platform | Status | Shell / process model | Notes |
| --- | --- | --- | --- |
| Linux | Supported | POSIX-style shell execution via `bash` / `zsh` / `/bin/sh` fallback | Primary target for install, shell wrapping, and test coverage. |
| macOS | Supported | POSIX-style shell execution via `bash` / `zsh` / `/bin/sh` fallback | Supported with the same Unix-like shell assumptions as Linux. |
| Windows host via WSL2 terminal | Best-effort / not separately validated | Linux userspace and POSIX-style shell execution inside WSL2 | Treated as a Linux environment for terminal usage, but not yet backed by dedicated WSL CI/smoke validation. |
| Windows | Not supported | `PowerShell` and `cmd.exe` are out of scope | Deferred until Aegis has a dedicated Windows interception design. |

## Current strategy

Aegis officially supports **Unix-like systems only** today.

That includes Linux and macOS directly, and can include **WSL2 terminal usage**
when Aegis runs inside the Linux environment provided by WSL2, but that path
is not separately validated yet.

That means the supported runtime boundary is:

- POSIX-style shell invocation
- `SHELL`-based wrapper setup
- `AEGIS_REAL_SHELL` recursion protection
- Unix-like path and process semantics

## Sandbox confinement

The Sandbox limits what an executed command may write and whether it can reach
the network. It is not a confidentiality boundary and not a privilege boundary:
a confined command can still read every file its user can read, and the Sandbox
does not stop anything else the user could do within the paths it may write
([ADR-029](adr/adr-029-the-sandbox-is-a-mandatory-1-0-layer.md)). The mechanism
and its strength differ by platform.

| Platform | Mechanism | Known gaps |
| --- | --- | --- |
| Linux | bubblewrap, plus Landlock write restrictions on the Shell path | Needs unprivileged user namespaces. Landlock is applied only when `allow_write` is not empty, and only inside the Shell exec path. Watch uses bubblewrap alone. A kernel without Landlock support rejects a non-empty `allow_write` instead of running unconfined. A hardened kernel that forbids executing memfd files loses the embedded bubblewrap fallback. |
| macOS | Seatbelt through `/usr/bin/sandbox-exec` | Confinement is one Seatbelt profile with no process namespace isolation and no second layer. The profile allows all file reads. It cannot be applied when the shell already runs under an outer Seatbelt profile (see below). |
| Windows host via WSL2 terminal | bubblewrap and Landlock, as on Linux | Same gaps as Linux. WSL2 is not separately validated. WSL1 cannot create the user namespaces bubblewrap needs, so the Sandbox is `Unavailable` there. |
| Windows | None | There is no Sandbox implementation for native Windows. The Sandbox crate compiles a stub that always reports `Unavailable`. |

On Linux, Aegis prefers a `bwrap` found on `PATH` and skips one that lacks an
option Aegis needs. Otherwise it uses the embedded bubblewrap, built from
vendored C sources and run from an in-memory file. Linux mounts the whole
filesystem read-only, unshares every namespace, and binds each `allow_write`
path writable. It shares the network namespace only when `allow_network` is set.

On macOS, the Seatbelt profile denies by default and allows file reads and
process execution. It allows writes under each `allow_write` path, and network
access only when `allow_network` is set.

When the Sandbox is `Unavailable`, the 1.0 contract blocks the command. The
0.x binary still implements the optional model: the Sandbox is off unless
`sandbox.enabled` is set, and an unavailable Sandbox runs the command
unconfined with a warning unless `sandbox.required = true`. The README describes
the difference.

### Nested Seatbelt on macOS

When the shell Aegis runs as is already confined by an outer Seatbelt profile,
macOS refuses Aegis' own `sandbox_apply` call. The Sandbox then reports
`Unavailable`, and with confinement required Aegis blocks every command. This is
always the case under Codex on macOS. Under Claude Code it happens only once
`/sandbox` is enabled. Aegis does not treat the outer profile as a substitute for
its own, because the two layers protect different things
([ADR-029](adr/adr-029-the-sandbox-is-a-mandatory-1-0-layer.md), amendment of
2026-08-28).

The block carries its own diagnostic, code
`sandbox_required_nested_unavailable`, which names the outer sandbox as the
cause. Under Claude Code the remedy is to disable `/sandbox` and retry. Under
Codex on macOS there is no bypass today. Linux is not affected: a nested
bubblewrap can only tighten the outer confinement.

## Build prerequisites

| Platform | Needed to build from source | Needed to run |
| --- | --- | --- |
| Linux | A C compiler, `pkg-config`, and the `libcap` headers (`libcap-dev` on Debian and Ubuntu). The build compiles the vendored bubblewrap and fails without `libcap`. | Unprivileged user namespaces. A `bwrap` on `PATH` is optional, since the binary embeds one. |
| macOS | Nothing beyond the Rust toolchain. The build script does no C step. | `/usr/bin/sandbox-exec`, which ships with the OS. |
| Windows host via WSL2 terminal | The Linux prerequisites, inside the WSL2 distribution. | The Linux requirements. |
| Windows | Not supported. | Not supported. |

The Linux build fails by design with `failed to compile bubblewrap for Linux
target: libcap not available via pkg-config` when the headers are missing.
`AEGIS_SKIP_BWRAP_BUILD=1` skips the C build for local work, but the binary then
has no embedded fallback and needs a usable system `bwrap`.
[Troubleshooting](troubleshooting.md) covers the overrides, and
[CONTRIBUTING.md](../CONTRIBUTING.md) lists the contributor setup.

## WSL2 guidance

If you use Windows, the best-effort path is to run Aegis **inside a WSL2 Linux
terminal**, where it uses the same Unix-like shell and process assumptions as
on Linux.

Current WSL2 position:

- Windows host via WSL2 terminal: best-effort Linux-like environment
- native Windows shells (`PowerShell`, `cmd.exe`): unsupported
- WSL2 support is not yet backed by dedicated CI or explicit smoke coverage

## Unsupported Windows strategy

Windows is intentionally out of scope for the current release line.

The project does **not** currently support:

- `PowerShell` command semantics
- `cmd.exe` quoting / escaping semantics
- Windows-specific path handling
- Windows process / shell-wrapper behavior

The installer rejects Windows explicitly instead of pretending support exists.

## Why Windows is deferred

A safe Windows implementation needs a separate design for:

- `PowerShell` parsing and execution semantics
- `cmd.exe` process model and quoting rules
- path normalization across drive letters and backslashes
- recursion-safe shell-wrapper installation on Windows

Until that exists, the support policy remains explicit Unix-only.
