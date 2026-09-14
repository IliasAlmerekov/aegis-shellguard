# ADR-013: Project config uses a security ratchet

## Status

Accepted. Annotated 2026-08-20 — see [Annotation](#annotation--2026-08-20-two-ratcheted-fields-leave-the-set).
Annotated 2026-09-14 — see [Annotation](#annotation--2026-09-14-a-normative-direction-table-for-every-field).

## Context

Aegis loads built-in defaults, then global user config, then project-local
`.aegis.toml`. The project layer is untrusted input when an AI agent enters a
repository. Pure last-layer-wins semantics allowed a repository to set
`mode = "Audit"`, `allowlist_override_level = "Danger"`, and
`snapshot_policy = "None"`, weakening Aegis to audit-only behavior for
non-`Block` commands. An initial ratchet covered only a few scalar fields, but
sibling fields stayed last-wins: a project could disable `sandbox.enabled`, set
`auto_snapshot_* = false`, enable `sandbox.allow_network`, or expand
`sandbox.allow_write`, each silently defeating a stricter global base.

## Decision

Project-local config may only tighten security-critical fields, never weaken
them. The ratcheted set is: `mode`, `allowlist_override_level`, `ci_policy`,
`snapshot_policy`, `sandbox.enabled`, `sandbox.required`,
`sandbox.allow_network`, `sandbox.allow_write`, the `auto_snapshot_*` flags
(`git`, `docker`, `postgres`, `mysql`, `supabase`, `sqlite`), the Snapshot
target of each database provider — `sqlite_snapshot_path`; `postgres_snapshot`
and `mysql_snapshot`'s `database`, `host`, `port`, and `user`; and
`supabase_snapshot`'s `project_ref` and `db.database`/`db.host`/`db.port`/
`db.user` — plus `docker_scope`, `audit.integrity_mode`, and project-layer
`[[rules]]` `decision = "Allow"`. `supabase_snapshot.require_config_target_match_on_rollback`
is ratcheted too, but on its own axis: it always tightens (never weakens), even
for a Supabase target the project is otherwise free to configure (#269).
Audit rotation follows the
same rule: a project may disable rotation, raise
`audit.max_file_size_bytes`, or raise `audit.retention_files`, but it cannot
enable disabled rotation or lower either retention limit. Snapshot prune
retention follows it too: a project may disable prune or raise
`prune.max_count_per_provider` and `prune.max_age_days`, but it cannot enable
disabled prune, lower either limit, or set a limit the base leaves unset. Global config remains
the user's trusted policy layer. When a project config attempts to weaken one
of these fields, Aegis keeps the more restrictive value and
`aegis config validate` reports a warning.

Directionality is field-specific. For booleans where `true` is the stricter
value (`sandbox.enabled`, `sandbox.required`, all `auto_snapshot_*`), the
Project layer keeps `base || requested` (the stricter of base/requested wins).
For `sandbox.allow_network`, where `true` is the weaker value (it grants
network access), the Project layer keeps `base && requested`. For
`audit.rotation_enabled`, where `true` causes old audit entries to be removed,
the Project layer also keeps `base && requested`. For audit retention limits,
where larger values retain more history, it keeps `max(base, requested)`.
`prune.enabled` keeps `base && requested`. For prune retention limits it keeps
`max(base, requested)` when the base sets the limit and leaves the limit unset
otherwise, because with both limits unset prune deletes nothing. For
`sandbox.allow_write` (a `Vec<PathBuf>` where more entries is weaker), the
Project layer keeps the trusted base set and ignores the project value
entirely. Global always stays last-layer-wins for every field.

For a database provider's Snapshot target, directionality is a single
all-or-nothing rule rather than per-field tightening: once the trusted base
enables the provider (its `auto_snapshot_*` flag, or `snapshot_policy = "Full"`)
with a non-empty target, the Project layer keeps every target field exactly as
the base set it and ignores every field the project requested for that target,
because a project that could repoint even one field (say, only `host`) could
still redirect a later Rollback at a database it controls. When the base
leaves the provider off or its target empty, there is nothing to protect and
the project may enable and configure its own target in full (#269).
`supabase_snapshot.require_config_target_match_on_rollback` is the one
exception to "nothing to protect": it keeps `base || requested` regardless of
whether the target itself is protected, because the check exists to catch a
target drifting out from under a Snapshot after the fact, not only a target
the base itself locked down.

`auto_snapshot_*` and `sandbox.enabled` close the bypass where a project could
otherwise disable snapshots or the sandbox despite a stricter `snapshot_policy`
or `sandbox.required` inherited from defaults or global config.

## Consequences

Repository-local config can still add patterns, scoped `[[allow]]`/`[[block]]`
rules (capped by `allowlist_override_level`), and tighter project policy. It
can no longer silently disable prompts, snapshots, CI blocking, the sandbox
itself, required sandbox behavior, or audit integrity inherited from defaults
or global config. Users who intentionally want a weaker posture (e.g. opting
out of git snapshots, granting network access, or disabling the audit
integrity chain) must set it in their global config rather than letting a
repository impose it. The merge path and the warning collector share the same
ratchet helpers, so the reported `kept` value always matches the effective
merged value.

Two ratchets deserve explicit call-out, because each closes a bypass with the
same shape as the original C3 attack — a repository silently auto-approving or
de-safeguarding a `Warn`/`Danger` command:

- **Project-layer `[[rules]]` `decision = "Allow"`** is dropped (and warned),
  not honored. A `[[rules]]` `Allow` auto-approves a command *before* `Mode`
  and with *no* `allowlist_override_level` ceiling (unlike an `[[allow]]`
  entry, which is capped). Without this ratchet, a repository could add a
  `[[rules]]` entry matching e.g. `git reset --hard` with `decision = "Allow"`
  and auto-approve a `Danger` command with no prompt, defeating the ratchet on
  `allowlist_override_level`. Project `[[rules]]` may still tighten
  (`Prompt`/`Block`); only `Allow` is untrusted. A project that needs an
  auto-approve must declare the rule in global config.
- **`audit.integrity_mode`** is ratcheted so a project cannot weaken it to
  `Off`. (The chain is an integrity/corruption check, not adversarial
  tamper-evidence — see TASKS.md H5 — but silently disabling even that from an
  untrusted repo is the same weakening shape and is closed here.)
- **Audit rotation retention** is ratcheted so a project cannot shrink global
  audit history by forcing frequent rotation or fewer archives. A project can
  retain more history or disable rotation. (#267)
- **Snapshot prune retention** is ratcheted so a project cannot set
  `max_age_days = 0` and `max_count_per_provider = 0` and have the next
  `aegis snapshot prune --yes` delete every Snapshot, including the recovery
  material for that repository. A project can keep more Snapshots or disable
  prune. (#268)
- **A database provider's Snapshot target is ratcheted once the base enables
  it**, and `supabase_snapshot.require_config_target_match_on_rollback`
  ratchets unconditionally. Before this, a project could repoint an already-
  enabled Postgres, MySQL, or Supabase target at a database it controls, or
  switch off the Supabase rollback target-match check, and later have
  Rollback restore into that decoy — silently producing a Snapshot that looks
  successful but recovers nothing real. A project can still configure a
  target the base never enabled. (#269)

## Annotation — 2026-08-20: two ratcheted fields leave the set

Decided in [#240](https://github.com/IliasAlmerekov/aegis-shellguard/issues/240),
recorded as an amendment to
[ADR-029](adr-029-the-sandbox-is-a-mandatory-1-0-layer.md). The ratchet stands;
what changes is which fields it covers and how one of them merges.

- **`sandbox.enabled` and `sandbox.required` are no longer ratcheted fields.**
  Both leave the 1.0 configuration contract entirely: they are accepted by exact
  name, ignored at any value, and warned about with `deprecated_sandbox_field`.
  There is nothing left for a project layer to weaken, because there is no
  effective flag. The bypass this ADR closed is closed more strongly than a
  ratchet could: the Sandbox is a mandatory layer, so it applies whatever any
  layer says. The Decision text above that names them — the ratcheted set, the
  `base || requested` directionality, and the "`auto_snapshot_*` and
  `sandbox.enabled` close the bypass" paragraph — is superseded for those two
  fields only; every other field in the set is untouched.
- **`sandbox.allow_write` becomes a semantic tree intersection.** "The Project
  layer keeps the trusted base set and ignores the project value entirely" is
  replaced by a component-wise intersection of path trees, computed at merge with
  no filesystem access. A project layer may therefore narrow the ceiling, which
  the old rule made impossible. An attempted widening still keeps the base and
  still emits `project_security_ratchet` — the project asked for more than it
  received, so the warning is owed. A malformed individual entry gets its own
  outcome, `trusted_ceiling_path_omitted`, rather than being reported as a
  merge-time weakening.
- **The Consequences paragraph is narrowed.** "It can no longer silently disable
  … the sandbox itself, required sandbox behavior" now describes the mandatory
  layer rather than this ratchet. The remaining list — prompts, snapshots, CI
  blocking, audit integrity — is unchanged.

`PRD.md` §5.5 is the normative statement of the resulting `[sandbox]` semantics.

## Annotation — 2026-09-14: a normative direction table for every field

Decided in [#270](https://github.com/IliasAlmerekov/aegis-shellguard/issues/270).
The ratchet stands unchanged; what changes is how it's expressed and enforced.
Before this, the ratchet lived as thirteen `ratchet_*` helper functions and
about thirty call sites, with no mechanism stopping a new field from bypassing
all of it (which is how the audit and prune gaps in this ADR's own
Consequences section got in, and how a repository could still widen every
other field the Decision text doesn't name). `merge_layer` now destructures
`AegisConfig` and every nested config struct exhaustively, so a field with no
direction is a compile error, and a test diffs the ratcheted field set against
the config's JSON schema so an omission fails a build even where the compiler
can't catch it (a `Custom` group's internal leaves).

Five directions cover every field (CONTEXT.md "Ratchet direction"):

- **Tighten**: the Project layer keeps the stricter of the trusted base and
  the requested value. Global always wins outright.
- **Global-only**: the project value is ignored regardless of layer content.
- **Append**: project entries are added after trusted ones. Nothing to
  ratchet, since concatenation can't remove a trusted entry.
- **Unratcheted**: the last layer wins, by recorded decision rather than
  oversight.
- **Custom**: a named rule for the handful of fields where "stricter" needs
  its own definition.

The table below supersedes the prose enumerations earlier in this ADR. Where
they conflict, the table is authoritative. "Tighten (ceiling)" means the value
additionally clamps to a hard ceiling at every layer including Global
(ADR-022 §6).

| Field | Direction | Rule |
| --- | --- | --- |
| `config_version` | Unratcheted | Schema version, not a security posture. |
| `mode` | Tighten | Stricter of `Audit` < `Protect` < `Strict`. |
| `custom_patterns` | Append | Concatenated, global first. |
| `allow` | Append | Concatenated, global first; capped elsewhere by `allowlist_override_level`. |
| `block` | Append | Concatenated, global first; blocklist always wins over allowlist. |
| `allowlist_override_level` | Tighten | Stricter of `Danger` < `Warn` < `Never`. |
| `snapshot_policy` | Tighten | Stricter of `None` < `Selective` < `Full`. |
| `auto_snapshot_git` | Tighten | `true` is stricter (`base \|\| requested`). |
| `auto_snapshot_docker` | Tighten | `true` is stricter. |
| `auto_snapshot_postgres` | Tighten | `true` is stricter. |
| `postgres_snapshot.{database,host,port,user}` | Custom | All-or-nothing: once the base enables Postgres with a non-empty `database`, every field stays pinned to the base; a project can still configure a target the base left off (#269). |
| `auto_snapshot_mysql` | Tighten | `true` is stricter. |
| `mysql_snapshot.{database,host,port,user}` | Custom | Same all-or-nothing rule as `postgres_snapshot` (#269). |
| `auto_snapshot_supabase` | Tighten | `true` is stricter. |
| `supabase_snapshot.{project_ref,db.database,db.host,db.port,db.user}` | Custom | Same all-or-nothing rule, once `db.database` is non-empty (#269). |
| `supabase_snapshot.require_config_target_match_on_rollback` | Tighten | `true` is stricter, on its own axis: ratcheted even when the target itself isn't protected. |
| `auto_snapshot_sqlite` | Tighten | `true` is stricter. |
| `sqlite_snapshot_path` | Custom | All-or-nothing once the base path is non-empty (#269). |
| `docker_scope.{mode,label,name_patterns}` | Custom | Once the docker provider is enabled and the base scope isn't a no-op, only a keep-or-broaden move is honored (`All` broadest; same-label `Labeled`; superset `Names`). |
| `ci_policy` | Tighten | Stricter of `Allow` < `Block`. |
| `audit.rotation_enabled` | Tighten | `false` is stricter (`base && requested`). Disabling rotation is available only globally. |
| `audit.max_file_size_bytes` | Tighten | Larger retains more history (`max(base, requested)`). |
| `audit.retention_files` | Tighten | Larger retains more history (`max(base, requested)`). |
| `audit.compress_rotated` | Unratcheted | Storage format, not audit coverage. |
| `audit.integrity_mode` | Tighten | `ChainSha256` is stricter than `Off`. |
| `rules` (`[[rules]]`) | Custom | Every field tightens (`Prompt`/`Block`); a project-layer entry whose `decision` or `when.then` is `Allow` is dropped and warned about, not honored. |
| `sandbox.enabled` | Tighten | `true` is stricter (current behaviour; [#229](https://github.com/IliasAlmerekov/aegis-shellguard/issues/229) tracks folding the Sandbox's mandatory posture in here directly). |
| `sandbox.required` | Tighten | `true` is stricter (same #229 note). |
| `sandbox.allow_write` | Custom | Project keeps the intersection of its requested set with the trusted base (narrow-only): see the 2026-08-20 annotation above. The annotation's tree-intersection semantics remain the target; this implementation is still the literal-set filter it describes as the interim state, pending [#229](https://github.com/IliasAlmerekov/aegis-shellguard/issues/229). |
| `sandbox.allow_network` | Tighten | `false` is stricter (`base && requested`). Network access is available only globally. |
| `prune.enabled` | Tighten | `false` is stricter (`base && requested`). |
| `prune.max_count_per_provider` | Custom | Larger retains more Snapshots when the base already sets a limit; an unset base limit stays unset rather than adopting the project's. |
| `prune.max_age_days` | Custom | Same rule as `prune.max_count_per_provider`. |
| `language_analysis.inline_source_limit_bytes` | Tighten (ceiling) | Smaller is stricter; ceiling `LANGUAGE_ANALYSIS_INLINE_SOURCE_MAX_BYTES`. |
| `language_analysis.script_file_limit_bytes` | Tighten (ceiling) | Smaller is stricter; ceiling `LANGUAGE_ANALYSIS_SCRIPT_FILE_HARD_CEILING_BYTES` (ADR-022 §6). |
| `language_analysis.max_script_files` | Tighten (ceiling) | Smaller is stricter; ceiling `LANGUAGE_ANALYSIS_MAX_SCRIPT_FILES`. |
| `language_analysis.max_depth` | Tighten (ceiling) | Smaller is stricter; ceiling `LANGUAGE_ANALYSIS_MAX_DEPTH`. |
| `language_analysis.max_targets` | Tighten (ceiling) | Smaller is stricter; ceiling `LANGUAGE_ANALYSIS_MAX_TARGETS`. |
| `language_analysis.max_aggregate_bytes` | Tighten (ceiling) | Smaller is stricter; ceiling `LANGUAGE_ANALYSIS_MAX_AGGREGATE_BYTES`. |
| `language_analysis.timeout_ms` | Tighten (ceiling) | Smaller is stricter; ceiling `LANGUAGE_ANALYSIS_TIMEOUT_MS`. |
| `language_analysis.trusted_aliases` | Global-only | "Trusted global aliases only" (ADR-022 §6). A project-layer entry is dropped, not merged. |

`sandbox.allow_write` is the one field where this table and the current
implementation still disagree with the 2026-08-20 annotation's stated target.
That annotation calls for a tree intersection over path prefixes: this
implementation still filters on literal path equality, the same "keep the
trusted base set" behavior in effect since before that annotation. Both are
narrow-only, so nothing here weakens the field; closing the gap between
literal filter and tree intersection is tracked in
[#229](https://github.com/IliasAlmerekov/aegis-shellguard/issues/229).
