# ADR-047: The Claude Code plugin is the primary Claude install path

## Status

Accepted (#500).

## Context

Claude Code users get the Aegis `Hook`s today through `aegis install-hooks
--claude-code`, which npm postinstall also runs. It writes two shims under
`~/.claude/hooks/` and registers them in `settings.json`. Claude Code also
installs plugins from catalogs, and a Claude Code plugin can register hooks and
slash commands without touching `settings.json`. Users find Aegis in the
community catalog more easily than through an npm postinstall step.

A Claude Code plugin cannot render templates at install time, and the plugin
cache may copy only the plugin directory. It cannot ship the aegis binary in a
form that works on every platform either. If the Claude Code plugin and the
`settings.json` entries are both active, `aegis hook` runs twice for every Bash
call.

## Decision

Ship the Claude Code plugin from this repository: `plugins/aegis/` holds
`plugin.json`, `hooks/hooks.json`, the two hook scripts, and the
`commands/rollback.md`, which Claude Code namespaces as `/aegis:rollback`. A root `.claude-plugin/marketplace.json` lists it as
the `aegis-shellguard` marketplace, so it works before community review.

- The plugin hook scripts are committed copies of `scripts/hooks/claude-code.sh`
  and `scripts/hooks/claude-session-start.sh` with `__AEGIS_BIN__` replaced by
  `aegis`, so they find the binary on `PATH`. A test fails when a copy drifts
  from its template byte for byte.
- `hooks.json` double-quotes `${CLAUDE_PLUGIN_ROOT}` in each command. Claude
  Code runs the command through a shell, and an unquoted root under a path with
  a space would split, fail to start the hook, and let Bash run unguarded.
- The plugin does not ship the binary. The PreToolUse hook keeps its fail-closed
  order: with `~/.aegis/disabled` present it exits before the binary check;
  otherwise a missing binary denies every Bash call. The SessionStart hook
  says so and names `npm i -g @iliasalmerekov/aegis`.
- The Claude Code plugin counts as active only when both hold for the same
  key, `aegis@claude-community` or `aegis@aegis-shellguard`: its effective
  `enabledPlugins` value is `true`, and `~/.claude/plugins/installed_plugins.json`
  lists that key under `plugins` with at least one record whose `installPath`
  is an existing directory. A missing, unreadable, or malformed file, a
  missing key, an empty array, or a missing `installPath` directory all mean
  "not installed". `enabledPlugins` alone is not enough: synced dotfiles can
  enable the plugin on a machine that never added the marketplace, and that
  plugin registers no Hook. Skipping the settings hooks there would leave Bash
  unguarded, while installing them next to a working plugin only runs the Hook
  twice, so the check fails closed. Only the two exact keys are read in both
  files, so a foreign plugin named `aegis` cannot pass either check. Which scopes count depends on what the hooks cover. A global install
  writes hooks for every project, so only `~/.claude/settings.json` decides it:
  a repo that enables the plugin must not leave every other project unguarded,
  and a repo that disables it must not make every other project run the Hook
  twice. A `--local` install writes the shared, committed
  `.claude/settings.json`, so it reads that file, then `~/.claude/settings.json`,
  and ignores the personal `.claude/settings.local.json`: one developer's
  local opt-out must not make every teammate run the Hook twice. `aegis status`
  reports what this user's session registers, so it reads Claude Code's full
  order: `.claude/settings.local.json`, `.claude/settings.json`, then
  `~/.claude/settings.json`. Other `aegis@*` keys
  do not count, so a foreign plugin named `aegis` cannot switch the
  `settings.json` hooks off. `installed_plugins.json` is always read from
  `~/.claude`, whatever the scope that enabled the key.
- While the plugin is active, `aegis install-hooks` with Claude selected writes
  no shims and no settings entries and prints `Claude Code: skipped (Claude
  Code plugin aegis is enabled)`. It does not remove existing aegis-managed
  entries. When the plugin is enabled but not installed, the install writes
  the settings hooks as usual and adds the line `Claude Code: plugin aegis is
  enabled but not installed; installed settings hooks instead`.
- `aegis status` prints `claude code hooks: plugin|settings|none|duplicate`.
  `plugin` and `duplicate` need an active plugin. When the plugin is enabled
  but not installed, the value comes from the settings entries alone
  (`settings` or `none`), and a second line, `claude code plugin: aegis is
  enabled but not installed; only settings hooks guard Bash`, names the gap.
  When the plugin is enabled and `installed_plugins.json` has a top-level
  `version` other than 2, or none, `aegis status` prints `claude code hooks:
  unknown (unrecognised installed_plugins.json format)` instead of guessing.
  It scans every settings scope Claude Code merges hooks from:
  `.claude/settings.local.json`, `.claude/settings.json`, and
  `~/.claude/settings.json`. `duplicate` names the settings file that still
  holds the aegis-managed PreToolUse entry and asks the user to remove it by
  hand.
- `/aegis:rollback` runs `aegis snapshot list` and `aegis rollback <id>` through
  the Bash tool after the user confirms. It carries no `` !` `` shell
  injection, because those lines run outside the Bash tool and so bypass
  PreToolUse and Aegis.
- `plugin.json` carries the `aegis` crate version, and
  `claude_plugin_manifests_are_consistent` fails when they differ. The
  version bump commit edits `plugin.json` next to `Cargo.toml`. Release CI
  cannot do it: the marketplace serves `plugin.json` from git, and
  `release.yml` runs after the tag and commits nothing.

## Consequences

- A user with the Claude Code plugin enabled and an older `settings.json`
  install runs the Hook twice until they remove the entries. `aegis status`
  reports this; nothing removes them automatically.
- Aegis now depends on Claude Code's internal `installed_plugins.json`, and
  reads it only when its top-level `version` is 2. A missing or different
  version means an unknown format: the install treats the plugin as not
  installed and writes the settings hooks, and `aegis status` prints
  `unknown`. If Claude Code moves the file, Aegis reads the plugin as not
  installed. Either way a format change costs a duplicate Hook, never a
  missing one.
- A repository can switch Aegis off for itself. A committed
  `.claude/settings.json` with `"aegis@aegis-shellguard": false` (or
  `"aegis@claude-community": false`) overrides the user-scope enable. With the
  plugin active, the global install wrote no settings hooks, so that project
  runs Bash with no Aegis Hook and no SessionStart notice. Only `aegis status`
  shows it, as `none`. A `--local` install in that project restores the Hook.
- This waiver is accepted because it gives a repository no capability it lacks
  today. Project settings can already set `"disableAllHooks": true`, which
  Claude Code applies over user settings and which turns off settings-installed
  Aegis hooks the same way: per the Claude Code hooks documentation, only hooks
  from managed policy settings survive a project-level `disableAllHooks`.
- Users who need protection against repository config should install the Aegis
  hooks through managed settings. A SessionStart sentinel that warns when the
  PreToolUse Hook is missing is tracked in #514.
- Every change to a Claude hook template must also update the plugin copy. The
  drift test catches a missed copy.
- The `aegis-hook-version` header of `claude-session-start.sh` moves to 2. The
  installer now renders the binary path into the SessionStart hook too.
- `aegis rollback` is reserved for the human operator by the PreToolUse hook, so
  inside a guarded session `/aegis:rollback` may end by handing the user the
  exact command to run in their own terminal.
- The Codex SessionStart hook gets no missing-binary notice yet; that is a
  follow-up.
- Reverting this decision means deleting `plugins/aegis/` and the marketplace
  file and the install skip. Existing `settings.json` installs are never
  modified, so a revert restores the old behaviour for every user.
