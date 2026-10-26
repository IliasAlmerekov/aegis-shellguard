# How Aegis works

This page holds the detail the README leaves out: how a command is judged, what
happens to each risk level, and what the Sandbox and Snapshots do and do not
promise. Terms follow [`CONTEXT.md`](../CONTEXT.md).

## Command flow

```
AI agent command
      │
      ▼
 Aegis parses and classifies it
      │
      ├──▶ Safe   ──▶ run immediately
      ├──▶ Warn   ──▶ ask first
      ├──▶ Danger ──▶ ask first, then snapshot if approved/configured
      └──▶ Block  ──▶ refuse
                          │
                          ▼
               real shell executes only
               what you approved
```

![Aegis command flow](../src/assets/howitwork.png)

Aegis decides by heuristics, not by understanding your command. It is not a
privilege boundary: it does not stop code that already runs as you. See the
[threat model](threat-model.md) for the full security model.

## Risk levels

| Level | What happens |
|-------|-------------|
| Safe | Runs immediately unless bounded effect-opaque Required recovery degrades |
| Warn | Pauses and asks for your approval |
| Danger | Asks first, then attempts configured Snapshots before execution |
| Block | Refused outright, with no prompt |

## Effect opacity and Required recovery

Bounded **Effect-opaque execution** such as `sh ./cleanup.sh` stays on its normal
`RiskLevel`, but Protect/Strict requires at least one configured Snapshot before
it runs. This is Required recovery. If no Snapshot is created, non-interactive
execution denies, and an interactive user sees only **Run once without recovery**
or **Deny**. Aegis does not inspect the referenced script, and a successful
Snapshot is still not a complete backup or universal undo.

Effect opacity is a separate axis from the risk level: a bounded effect-opaque
command that would otherwise execute must create a required Snapshot, receive a
one-time Recovery override, or deny.

## Snapshots and rollback

Where a Snapshot plugin applies (git, Docker, and the database plugins), Aegis
captures state before a Danger command runs. `aegis rollback --last` restores the
Snapshots of the latest command recorded at the current location. When no plugin
applies, Aegis says so instead of implying an undo it cannot perform.

## The Sandbox

The **Sandbox** is a write/network guardrail.
It is not a confidentiality boundary and not a privilege boundary:
it does not promise to hide readable files or secrets, and it does not stop a
command from doing anything your own user could do within the paths it is allowed
to write.

In Aegis 1.0 this layer is mandatory. Confinement is attempted for every
executed command, and failure to establish it blocks the command instead of
running it unconfined
([ADR-029](adr/adr-029-the-sandbox-is-a-mandatory-1-0-layer.md), `PRD.md`
§5.5). `sandbox.enabled` and `sandbox.required` leave the contract there: they
stay accepted by exact name and ignored at any value, each reported as a
`deprecated_sandbox_field`, and Aegis never rewrites your config to remove them.

**Current pre-1.0 implementation (0.x):** the shipped binary still implements
the optional model. If confinement is unavailable it records
`sandbox_status = "unavailable"` and warns on the active Shell or Watch channel
before running unconfined, and `sandbox.required = true` is what turns that into a
block. Closing that gap is
[#229](https://github.com/IliasAlmerekov/aegis-shellguard/issues/229) and
[#230](https://github.com/IliasAlmerekov/aegis-shellguard/issues/230).
