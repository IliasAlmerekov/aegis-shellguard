# Aegis

A `$SHELL` proxy that intercepts AI-agent commands and requires human confirmation
before destructive operations. This file is the **domain glossary** — the ubiquitous
language for the project. It is the single source of truth that humans, AI agents, and
the code itself (type names, fields, config keys, audit fields) must all use for the
same concept. It is maintained by the `domain-modeling` skill and contains no
implementation details. Keep definitions tight and opinionated.

## Command

**Parsed command**:
The canonical token-level representation the scanner consumes (`ParsedCommand`). Carries
`program`, `argv`, the `normalized` form, extracted `inline_scripts`, and the original
`raw` string. Every scanner stage works on this, never on the raw string.
_Avoid_: tokenized command, command struct

**Normalized command**:
The de-quoted, space-joined token sequence (`ParsedCommand.normalized`) — the scanner's
primary match target, free of shell quoting and escape noise.
_Avoid_: cleaned command, sanitized command

**Inline script**:
A script body extracted from an interpreter invocation (`python -c`, `node -e`),
scanned in its own right so risky code hidden behind an interpreter flag is still caught.
_Avoid_: embedded script, subcommand

**Indirect execution**:
Running commands through an interpreter or another layer (inline scripts, piping into a
shell) rather than invoking the program directly. `Strict` mode blocks it.
_Avoid_: nested execution, eval

**Script-file execution**:
Running an interpreter against a script _file_ named in argv (`sh ./cleanup.sh`,
`python3 ./x.py`, `node ./x.js`, `source ./x`) — the destructive effect lives in the file,
which Aegis does not read at classification time. A sub-case of `Indirect execution` and
the sibling of `Inline script`: an inline body (`-c` / `-e`) is extracted and scanned, but
a referenced file is not.
_Avoid_: external script, script bypass, file exec

**Effect-opaque execution**:
A command shape whose text reveals that another execution layer will decide the eventual
filesystem, database, or network effect, but does not reveal that effect directly.
`Script-file execution` is effect-opaque; an `Inline script` may stop being
effect-opaque once its body is extracted and assessed. Orthogonal to `RiskLevel`.
_Avoid_: hidden effect, opaque command, unknown execution

**Launcher prefix**:
A leading token that launches another program rather than being the target itself
(`sudo`, `env`, `nice`, `timeout`, `command`, the site-specific `rtk`, …). Stripped — with
its options, via a built-in option-arity table — to expose the real program for **detection
matching only** (never for execution). Built-in launchers are trusted and include
the local `rtk` execution wrapper. Distinct from the `Wrapper` (`$SHELL` proxy) and
from a `Hook`.
_Avoid_: wrapper, command wrapper, exec prefix

**Effective program**:
The real program token a scan target resolves to after stripping launcher prefixes and
taking the basename of an absolute path (`/usr/bin/git` → `git`, `sudo rtk git` → `git`).
Computed per scan target and used as the lookup key for `Token-prefix rule`s and the
by-program regex index — so prefixes and absolute paths cannot bypass a rule keyed on the
first token. Distinct from `ParsedCommand.program`, which preserves the raw leading token.
_Avoid_: real program, resolved command, normalized program

**Logical segment**:
A scan-oriented command unit produced by `logical_segments` — the raw string cut at
top-level `Command separator`s and normalized, so each independent command is assessed
on its own. A scan-time boundary, not an execution unit: fork semantics of a background
`&` are ignored; it only marks where one command ends.
_Avoid_: segment, sub-command, command part

**Command separator**:
A top-level shell control operator that ends one `Logical segment` and starts the next:
`;`, `&&`, `||`, `|`, newline, and a standalone background `&`. A `&` that is part of a
redirect (`&>`, `>&`, `2>&1`) is not a separator.
_Avoid_: delimiter, control operator

**Short flag bundle**:
A single shell token that combines multiple one-letter CLI flags (for example `-af` as
`-a` + `-f`). Aegis treats bundle semantics as command-specific unless a rule explicitly
models them; exact flag tokens remain the default for `Token-prefix rule`s.
_Avoid_: combined flags, packed flags

## Scanner

**Assessment**:
The result of scanning one command — a `RiskLevel` plus the patterns it matched and
the parsed command.
_Avoid_: result, verdict, scan output

**RiskLevel**:
The severity a command is classified as, ordered by escalation: `Safe`, `Warn`,
`Danger`, `Block`. The order is semantic — never reorder it.
_Avoid_: severity level, threat level

**Intrinsic Block**:
A hard-coded, unbypassable `Block` decision checked before allowlist, rules, and mode.
The product's core guarantee that certain commands never execute.
_Avoid_: hard block, force block

**Pattern**:
A regex-based detection rule (built-in or user-defined) matched against the
normalized command string; matches **anywhere** in the string. Database rules are
regex `Pattern`s (match-anywhere), not `Token-prefix rule`s: SQL verbs (`DROP TABLE`)
arrive embedded in `psql -c` / `mysql -e` / heredoc / stdin, not as a leading program
token (ADR-015).
_Avoid_: rule, signature (reserve "rule" for prefix rules)

**Token-prefix rule**:
A detection rule keyed on a command's `Effective program` token (e.g. `git`, `docker`) and
matched against the token sequence — distinct from a regex `Pattern`. Git, Cloud,
Docker, some Process, and some Filesystem rules (`wipefs`, `unlink` — where the
destructive verb *is* the effective program) are token-prefix rules. A destructive
operation that arrives embedded mid-command instead (SQL verbs, a redirect to a
sensitive path) stays a regex `Pattern` (ADR-014/015).
_Avoid_: prefix pattern, first-token rule

**Quick scan**:
The fast first pass — an Aho-Corasick multi-pattern scan with no allocations, on the
< 2ms hot path. Never uses regex.
_Avoid_: prefilter, fast match

**Full scan**:
The verification pass that runs regex `Pattern`s and token-prefix rules after the
quick scan flags a candidate.
_Avoid_: deep scan, second pass

**Category**:
The domain a detection rule belongs to: `Filesystem`, `Git`, `Database`, `Cloud`,
`Docker`, `Process`, `Package`.

**Match**:
One pattern hit (`MatchResult`) — the `Pattern` that fired, matched text, and the
highlight span in the original command. For a regex or Token-prefix rule Match the
matched text is the actual text fragment that triggered it; production-created
Language-aware Matches (ADR-022 §10) use only a stable, source-free label
(`public_matched_text`'s `LANGUAGE_AWARE_MATCH_LABEL`) and typically no highlight
span. Any hand-built Language-aware Match receives that same label on rendered,
persisted, machine-readable, and `Debug` surfaces — the detected operation and its
metadata-only provenance describe it instead of the source bytes.
_Avoid_: hit, finding

**Decision source**:
What produced an assessment (`DecisionSource`): `BuiltinPattern`, `CustomPattern`, or
`Fallback` (nothing matched → assessed `Safe`). Distinct from the final `Decision`.
The singular label is retained as a compatibility projection; `Assessment basis` is
the richer successor that retains every decisive `Match` rather than one.
_Avoid_: origin, cause

## Language-aware analysis

**Detection rule**:
The common contract every detection mechanism exposes before 1.0 — a regex
`Pattern`, a `Token-prefix rule`, or a built-in Language-aware rule. Each `Match`
identifies which mechanism fired and whether the rule is built in or custom
(ADR-022 §4).
_Avoid_: detection signature, rule (reserve "rule" for `Token-prefix rule` / `Policy rule`)

**Detection mechanism**:
Which of the three mechanisms produced a `Match` — regex `Pattern`,
`Token-prefix rule`, or Language-aware rule (`DetectionMechanism`).
_Avoid_: match type, rule kind

**Detection source**:
Whether a `Detection rule` is built into Aegis or came from user config
(`DetectionSource`). Distinct from `PatternSource`, which lives on the legacy
`Pattern` struct.
_Avoid_: rule origin

**Match evidence**:
The typed record every `Match` carries — its `Detection mechanism`,
`Detection source`, and (for Language-aware rules) the `Detected operation` and
`Analysis provenance` (`MatchEvidence`).
_Avoid_: match detail, match info

**Detected operation**:
A language-neutral operation a language adapter emits — operation kind (delete,
overwrite, code execution, …), modifiers (recursive / forced / destructive),
and `Operand certainty` — rather than assigning `RiskLevel` directly from an API
spelling. A shared classifier maps it to `Category` / `RiskLevel` (ADR-022 §3).
_Avoid_: detected action, finding

**Operand certainty**:
How completely a `Detected operation`'s operand is statically known: `Known`,
`Partial`, or `Dynamic` (`OperandCertainty`). A `Dynamic` operand is never
evidence of safety — it records `Analysis degradation` in addition to the
visible operation (ADR-022 §3, §7).
_Avoid_: operand confidence, resolution level

**Analysis status**:
The per-target state of language-aware analysis: `NotApplicable`, `Complete`, or
`Degraded` (`AnalysisStatus`). Ordered by increasing degradation, so the worst
target drives the merged `Assessment` (ADR-022 §4). Distinct from an **absent**
`AnalysisSummary`, which means the stage never ran — not built into this binary,
or not reached; `NotApplicable` means it ran and found no analyzable source.
Absence is the ordinary case while the stage is not `Default-on`, and it is never
evidence about the command.
_Avoid_: analysis state, result status

**Analysis degradation**:
Language-aware analysis ran but could not fully resolve a target — typed by a
`Degradation reason` and never authorizing auto-execution. Prior `Match`es and
earlier target results are retained (ADR-022 §4, §5).
_Avoid_: analysis warning, analysis failure

**Degradation reason**:
A typed reason `Analysis degradation` occurred — unsupported or unavailable
grammar, incomplete syntax, unsafe or unavailable source, unsupported
encoding, a size/count/depth/timeout limit, dynamic source or cwd, or a
worker/protocol failure (`DegradationReason`).
_Avoid_: degradation cause

**Analysis provenance**:
The metadata record of where a language-aware result came from — language,
`Source origin`, rule ID, `Detected operation`, file path, source hash, span,
`Operand certainty`, `Analysis status`, `Degradation reason`. Metadata only: it
must never persist source body, full snippet, imported source, variable value,
or syntax tree (ADR-022 §10).
_Avoid_: analysis metadata, evidence origin

**Source origin**:
Where a language-aware analysis target's source came from — `Inline`,
`Heredoc`, `ScriptFile`, `Stdin`, or `Pipe` (`SourceOrigin`).
_Avoid_: source type, input kind

**Target analysis**:
The per-target language-aware result — `Analysis status`, `Degradation
reason`s, and `Analysis provenance` (`TargetAnalysis`).
_Avoid_: target result, per-file analysis

**Assessment basis**:
What produced the final interception decision, expressed as the decisive
`Match`es — every `Match` at the `Assessment`'s maximum `RiskLevel`, or
`Fallback` when nothing matched (`AssessmentBasis`). Replaces the singular
`Decision source` (ADR-022 §4).
_Avoid_: decision basis

**Decisive Match**:
A `Match` whose `RiskLevel` equals the `Assessment`'s maximum; the `Assessment
basis` retains every such `Match`, not just one.
_Avoid_: primary match, winning match

**Trusted global alias**:
A wrapper program name (e.g. a `py` shim) that the trusted global config layer
maps to the canonical registry interpreter it stands in for
(`language_analysis.trusted_aliases`, `TrustedAlias`). Global-layer only — a
project `.aegis.toml` can never add or narrow this list; project-supplied
entries are dropped entirely rather than merged (ADR-022 §6).
_Avoid_: interpreter alias, program alias

**Script-file limit**:
The maximum bytes read from a routed script file
(`language_analysis.script_file_limit_bytes`), bounded by a non-configurable
1 MiB hard ceiling at every config layer. Project config may only lower it,
never raise it (ADR-022 §6, §7).
_Avoid_: script budget, file-size limit

**Language-analysis budget**:
A hard-bounded limit on inline bytes, script-file count/bytes, recursion depth,
target count, aggregate bytes, or total session time. Trusted global config may
tune within the hard ceiling; project config may only tighten it (ADR-022 §7).
_Avoid_: analysis toggle, semantic-rule override

**Analysis confirmation**:
The Protect-mode one-time human decision required for a non-`Block`
Language-aware `Match` or Analysis degradation. It cannot be persisted as an
allowlist or policy-rule grant.
_Avoid_: allowlist approval, permanent approval

**Analysis override**:
The Strict-mode one-time human decision narrowly attached to a non-`Block`
semantic `Match`, or to a Safe assessment with Analysis degradation. It cannot
authorize an unrelated Strict denial and cannot be persisted.
_Avoid_: strict bypass, allowlist override

**Adapter qualification**:
The evidence gate one language adapter must pass before it may be linked and
trusted — its corpus, degradation, ABI, supply-chain, and all-target release
checks (ADR-022 §11, recorded in `docs/language-qualification.md`). It is a
property of the adapter's evidence and says nothing about whether the stage is
switched on for users.
_Avoid_: adapter readiness, certification

**Default-on**:
Whether language-aware analysis runs for a user without opting in. A release
decision, held separately from `Adapter qualification` — a qualified adapter is
not thereby default-on, and per ADR-024 none is in 1.0. The two must never be
stated as one condition.
_Avoid_: enabled, production-enabled, shipped

## Policy

**Mode**:
The top-level posture: `Protect` (default — prompt on `Warn`/`Danger`), `Audit`
(non-blocking, log only), `Strict` (block non-safe and indirect execution).
_Avoid_: level, profile

**Allowlist / Blocklist**:
User-configured exceptions. Blocklist always wins over allowlist; an allowlist entry
only downgrades up to `allowlist_override_level`.
_Avoid_: whitelist, blacklist

**Override level**:
The ceiling an allowlist entry may downgrade to (`AllowlistOverrideLevel`): `Warn`
(default), `Danger`, or `Never`. Above the ceiling, the allowlist does not auto-approve.
_Avoid_: allow ceiling, max downgrade

**Ratchet**:
The rule that a project-local `.aegis.toml` may only **tighten** a security-critical
field, never **weaken** it. Global config stays the user's trusted layer; a project
layer that asks for a weaker value keeps the more restrictive one and `config validate`
warns (ADR-013). "Tighten" and "weaken" are the canonical directions, and the stricter
direction is field-specific — more confinement, more Snapshot coverage, and fewer
write paths are all tightenings regardless of the underlying boolean.
_Avoid_: override, downgrade, merge (reserve "override" for `Override level`)

**Policy rule**:
A typed `[[rules]]` entry in config whose outcome is a `PolicyRuleDecision` — `Allow`,
`Prompt`, or `Block`. A rule `Allow` auto-approves the command ahead of `Mode` and with
no `allowlist_override_level` ceiling — unlike an `[[allow]]` allowlist entry, which is
capped by the override level. Because that makes a project-layer `Allow` a silent
auto-approve of a `Warn`/`Danger` command, project-layer `[[rules]] Allow` is untrusted:
the ratchet drops it and `config validate` warns (ADR-013); a project that needs an
auto-approve must declare it in global config. This `Block` is a *rule outcome*, distinct
from the `Block` `RiskLevel` and from a blocklist entry.
_Avoid_: custom rule (reserve "rule" wording for prefix rules / Patterns)

**CI policy**:
What Aegis does in a detected CI environment (default `Block`, since no TTY exists to
prompt). Distinct from `Mode`.
_Avoid_: CI mode

**Snapshot policy**:
When snapshot plugins run (`SnapshotPolicy`): `None`, `Selective` (honour per-plugin
flags — default), or `Full` (run every registered plugin). Distinct from `Mode` and
`CI policy`.
_Avoid_: backup policy

## Decision & Execution

**Decision**:
The recorded outcome of the interception flow: `Approved`, `Denied`, `AutoApproved`,
`Blocked`, `Pruned`. The final human-or-auto verdict the audit log stores — distinct
from the scanner's `Assessment`.
_Avoid_: result, outcome, verdict (those belong to the scanner stage)

**Toggle**:
The global on/off switch checked at command boundaries; when off, Aegis passes commands
through unguarded (ADR-005).
_Avoid_: enable flag, kill switch

**CI override**:
The rule that a continuous-integration environment keeps enforcement on regardless of a
`Toggle` that is off, so an operator's local escape hatch cannot silently travel into
automation (ADR-006). It only ever tightens: it cannot turn enforcement off.
_Avoid_: CI mode, CI bypass, forced mode

**Effective enforcement state**:
Whether Aegis actually inspects commands right now, after resolving the `Toggle` against
the `CI override` — as opposed to what the `Toggle` alone says. `aegis status` is the
authoritative surface for it; every other surface reports it rather than deciding it.
The three states are enforcing, enforcing by `CI override`, and disabled passthrough.
_Avoid_: toggle state, effective mode, actual state

**Disabled passthrough**:
The `Effective enforcement state` in which Aegis runs commands without inspecting them.
Intentional and operator-chosen — not a failure and not a `Decision`. Nothing is
classified, confirmed, or snapshotted while it holds.
_Avoid_: bypass, off mode, unguarded mode

**Session-start notice**:
The message an agent integration emits when a session begins, reporting the
`Effective enforcement state` so a new session cannot inherit `Disabled passthrough`
unseen. Informational only: it is not a `Decision` and produces no audit entry — only
`Toggle` transitions are auditable. It travels inside the agent's own protocol envelope
and never as loose output that would corrupt it.
_Avoid_: session warning, startup banner, session audit

**Sandbox**:
The OS-level layer that restricts what an executed command may write and whether it
may reach the network. Mandatory on Linux and macOS (ADR-029): confinement is
attempted for every executed command, and unavailability blocks rather than
degrading to an unconfined run. Not a security or confidentiality boundary; it does
not promise that file reads or secrets are hidden from the command.
_Avoid_: jail, container, confinement profile (the profile is the
`Effective confinement profile`; the Sandbox is the layer that applies it)

**Trusted ceiling**:
The upper bound on the authority any command may receive, resolved from
configuration before classification: writable roots plus network. Nothing later in
the flow may exceed it. Its default admits the workspace tree and `/tmp` with the
network off, because a mandatory `Sandbox` whose default forbids all writes cannot
run ordinary work (ADR-030).
_Avoid_: static profile, base profile, session profile

**Confinement restriction**:
An optional declaration on a Rule stating how the `Trusted ceiling` narrows when that
Rule matches. It can only subtract. Targets are located by a named
program-specific extractor, never by argv position and never by `Category`
inference. A Rule that declares none is the identity — the ceiling passes through
unchanged (ADR-030).
_Avoid_: permission grant, capability, sandbox rule

**Effective confinement profile**:
The writable roots and network permission actually applied to one command: the
`Trusted ceiling` intersected with project tightening, with every matched
`Confinement restriction`, and with any outer agent sandbox. Recorded in the
`AuditEntry`, because `Sandbox status` alone no longer says what was confined once
the profile varies per command.
_Avoid_: derived profile, computed profile, final profile

**Confinement degradation**:
The state where a `Confinement restriction` was declared but its extractor resolved
no target, so the command runs under the `Trusted ceiling` instead of a narrowed
profile. Like `Recovery degradation` it must never be silent: it is visible in the
confirmation dialog, not only in the `Audit log`.
_Avoid_: extractor failure, profile fallback

**Sandbox status**:
The confinement path selected during command preparation (`SandboxStatus`),
recorded in every audit entry: `Active`, `Unavailable`, `NotConfigured`, or
`NotAttempted`. `Active` means the confined launch path was prepared, not that a
later OS-level exec or spawn succeeded. `NotConfigured` means Sandbox was
disabled; `NotAttempted` means it was enabled but neither a confined nor
fallback launch path was used, including fail-closed preparation errors.
_Avoid_: sandbox state

**Sandbox bypass**:
Execution through the prepared unconfined fallback after Sandbox infrastructure
was unavailable (`SandboxStatus::Unavailable`). A mandatory layer has no such
path — under ADR-029 unavailability blocks, so this term describes only the
**current pre-1.0 implementation** and retires with it. It never named a
successful escape from an applied profile.
_Avoid_: sandbox failure, escape

## Snapshot & Audit

**Snapshot**:
A best-effort pre-execution capture (e.g. `git stash`) produced by an applicable
`Snapshot plugin`. It preserves only the state that plugin captures at that
moment; it is not a complete backup and does not promise to reverse every later
command effect.
_Avoid_: backup, checkpoint

**Snapshot plugin**:
A per-backend snapshotter (`git`, `docker`, `postgres`, `mysql`, `supabase`, `sqlite`)
that knows how to capture and restore state for its domain. Each successful run yields a
`SnapshotRecord` (`plugin` + opaque `snapshot_id`).
_Avoid_: snapshotter, driver, backend

**Snapshot store**:
The trusted directory a `Snapshot plugin` owns for reading and writing its
artifacts. A filesystem artifact must resolve beneath this directory before a
rollback or deletion may use it.
_Avoid_: snapshots dir, bundle root, snapshot root

**Snapshot artifact**:
The concrete filesystem object in a `Snapshot store` addressed by a
`snapshot_id`.
_Avoid_: dump, blob

**Path containment**:
The invariant that a resolved `Snapshot artifact` is provably beneath its
`Snapshot store`, including after symlink resolution.
_Avoid_: path validation, path sanitization

**Owner-only artifact permissions**:
The Unix invariant that a `Snapshot store` and its directory artifacts use mode
`0700`, while file `Snapshot artifact`s use mode `0600`; an unsafe store leaf
is tightened only when the current owner owns it, otherwise rejected before a
sensitive write.
_Avoid_: private snapshot, chmod security

**Unattended destructive execution**:
The execution of a `Danger` command or an `Effect-opaque execution` for which no
human decision was taken in the current run — auto-approval by an allowlist
override, a `Policy rule`, or a previously persisted always-allow. Defined by the
missing decision, never by a missing terminal: a terminal can be present while
the command is auto-approved, and absent while a persisted approval applies.
_Avoid_: non-interactive execution, CI execution, headless run

**Required recovery**:
The obligation to hold at least one `Snapshot` whose `Snapshot attempt readiness`
is `Ready` before an `Unattended destructive execution` proceeds. The obligation
is independent of whether any `Snapshot plugin` is available or succeeds; a
`Recovery opt-out` means recovery is not required rather than degraded. An
interactive `Danger` command is not subject to it — the human decides in the
moment and is told when no provider applies.
_Avoid_: mandatory backup, available snapshot

**Snapshot attempt readiness**:
The result of the local check of the expected artifact after one `Snapshot
plugin` attempt: `Ready` when the artifact passed the `Validation level`
reached, `Invalid` when it was found but failed a minimal check, `Unavailable`
when the artifact is missing or unreadable or the attempt produced no
`SnapshotRecord` at all. A property of the attempt, not of the artifact — an
attempt can be recorded when no artifact exists. It is not a liveness check and
proves nothing about whether a `Rollback` would succeed.
_Avoid_: snapshot verification, artifact validity, snapshot health

**Validation level**:
The depth of local checking a `Snapshot attempt readiness` result actually
reached: `PresenceOnly` when only existence, readability, and non-emptiness were
established, or `Structural` when a local tool additionally parsed the artifact.
An absent or unresponsive checking tool yields `PresenceOnly`, never `Invalid`.
_Avoid_: verification strength, check depth

**Recovery opt-out**:
The trusted, deliberate declining of `Required recovery`, from either of two
sources: `Mode::Audit`, which declines all enforcement, or a global
`SnapshotPolicy::None`, which declines recovery while enforcement continues.
Neither is a `Recovery degradation` and neither may be recorded as one; a project
config cannot introduce either, and both are visible in the `Audit log`.
_Avoid_: recovery disabled, snapshot off, degraded recovery

**Recovery degradation**:
The state where `Required recovery` applies but no attempt reached `Ready`. It is
distinct from a `Recovery opt-out` and must never silently become permission to
execute. It is never recorded while `Recovery status` is `Ready` — a partial
failure alongside a usable `Snapshot` is a per-attempt fact, not a degradation of
the obligation.
_Avoid_: snapshot warning, best-effort failure

**Recovery status**:
The post-attempt state of `Required recovery`: `Ready` when at least one attempt
reached `Snapshot attempt readiness::Ready`, or `Degraded` when none did.
Execution surfaces derive their deny or `Recovery override` behavior from this
shared fact.
_Avoid_: snapshot result, recovery verdict

**Recovery override**:
A one-time human approval to execute despite a visible `Recovery degradation`.
It cannot be persisted as an allowlist entry because it applies to the observed
failure to create a `Snapshot`, not to the command prefix.
_Avoid_: always allow, recovery bypass

**Rollback**:
Restoring the state captured by a previous `Snapshot`, addressed by its
`snapshot_id`. It restores captured state; it is not a general undo of the
command that ran afterward.
_Avoid_: undo, revert

**Audit log**:
The append-only JSONL record at `~/.aegis/audit.jsonl`. One `AuditEntry` per line;
never rewritten. The format is part of the public contract.
_Avoid_: history, journal

**Audit directory**:
A directory Aegis creates while materializing the configured `Audit log` path.
A pre-existing parent remains a caller-owned container, not an Audit directory.
_Avoid_: audit parent, log folder

**Audit artifact**:
An owner-only filesystem object used by the audit subsystem: the active `Audit
log`, its lock file, a rotated segment, or the managed gzip rotation staging
object.
_Avoid_: audit file, log artifact

**Audit integrity chain**:
The optional unkeyed SHA-256 link between consecutive `AuditEntry` values and
rotated segments (`ChainSha256`). It detects corruption and inconsistent edits,
but has no keyed or external anchor and therefore does not prove adversarial
tamper-evidence against an actor who can rewrite the whole local log.
_Avoid_: tamper-evident log, tamper-proof audit

**AuditEntry**:
One JSONL line in the audit log — the structured record of a single intercepted command,
its `Decision`, and its `Sandbox status`.
_Avoid_: log line, event

## Surfaces

**Wrapper / `$SHELL` proxy**:
The aegis binary acting as the user's `$SHELL`, intercepting commands launched via
`$SHELL -c`. The shell-level surface, distinct from a per-agent `Hook`.
_Avoid_: shim (reserve "shim"/"hook" for per-agent routing)

**Hook**:
A per-agent shim (`claude-code.sh`, the Codex hook) that routes a tool call through
Aegis. Must fail **closed** (deny) on missing dependencies or invalid input. A
panic or abnormal termination of the Hook is contained as a deny — see
**Contained Hook Panic**.
_Avoid_: wrapper, plugin (reserve "wrapper" for the shell `$SHELL` proxy itself)

**Contained Hook Panic**:
A panic or abnormal termination of the `Hook` that is converted into the ordinary
deny response instead of dying silently, so the agent never mistakes a crash for
permission to run the command. Two layers provide it: an in-process unwind guard
at the `Hook` boundary (fixed, detail-free deny reason) and the installed per-agent
`Hook` script, which survives the binary's death and denies on a non-zero exit
status. External SIGKILL, an OOM-kill of the agent process itself, and a corrupted
`Hook` script are not covered (ADR-023).
_Avoid_: crash handling, panic recovery (reserve "recovery" for snapshot rollback)
