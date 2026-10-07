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
`/aegis-rollback` command. A root `.claude-plugin/marketplace.json` lists it as
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
- The Claude Code plugin counts as enabled when the effective `enabledPlugins`
  value of `aegis@claude-plugins-community` or `aegis@aegis-shellguard` is
  `true`. Which scopes count depends on what the hooks cover. A global install
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
  `settings.json` hooks off. `installed_plugins.json` is not read: only an
  enabled plugin registers hooks.
- While the plugin is enabled, `aegis install-hooks` with Claude selected writes
  no shims and no settings entries and prints `Claude Code: skipped (Claude
  Code plugin aegis is enabled)`. It does not remove existing aegis-managed
  entries.
- `aegis status` prints `claude code hooks: plugin|settings|none|duplicate`.
  `duplicate` names the settings file that still holds the aegis-managed
  PreToolUse entry and asks the user to remove it by hand.
- `/aegis-rollback` runs `aegis snapshot list` and `aegis rollback <id>` through
  the Bash tool after the user confirms. It carries no `` !` `` shell
  injection, because those lines run outside the Bash tool and so bypass
  PreToolUse and Aegis.
- `plugin.json` carries the `aegis` crate version. A test enforces it, and
  `scripts/update-npm-package.sh` bumps it with the npm package version.

## Consequences

- A user with the Claude Code plugin enabled and an older `settings.json`
  install runs the Hook twice until they remove the entries. `aegis status`
  reports this; nothing removes them automatically.
- If user settings enable the Claude Code plugin and a project disables it, a
  global install skips, so that project has no Hook and `aegis status` there
  prints `none`. The remedy is a `--local` install in that project.
- Every change to a Claude hook template must also update the plugin copy. The
  drift test catches a missed copy.
- The `aegis-hook-version` header of `claude-session-start.sh` moves to 2. The
  installer now renders the binary path into the SessionStart hook too.
- `aegis rollback` is reserved for the human operator by the PreToolUse hook, so
  inside a guarded session `/aegis-rollback` may end by handing the user the
  exact command to run in their own terminal.
- The Codex SessionStart hook gets no missing-binary notice yet; that is a
  follow-up.
- Reverting this decision means deleting `plugins/aegis/` and the marketplace
  file and the install skip. Existing `settings.json` installs are never
  modified, so a revert restores the old behaviour for every user.
