# ADR-050: The git Snapshot follows `cd` steps earlier in the command line; the router's cwd tracking stays stricter

## Status

Accepted. Extends ADR-048.

## Context

ADR-048 made the git Snapshot follow the Git target for `git -C`,
`--work-tree` and `--git-dir`, and listed "targeting after `cd`" as out of
scope. `cd ../other && rm -rf x` still prompted on FS-001 (Danger) but took its
Snapshot of the directory Aegis started in. The directory the command changes
got no Snapshot, and nothing disclosed the gap (#525).

The router already tracks `cd` for script routing (`CwdState`, ADR-022 section
6). It trusts only `cd -- <literal> &&`, because routing decides what to
auto-approve: a wrong cwd there could read the wrong script and pass a
dangerous command.

## Decision

The **Git target** (see `CONTEXT.md`) is now the working tree a Danger stage
changes once `cd` steps and git options are applied, for git and non-git stages
alike. Planning computes, for each Danger stage, the directory it runs in: the
process cwd, then the `cd` steps before it, then (for a git stage) the ADR-048
option resolution starting from that directory.

The scanner walks the command's list structure only when a recovery backstop
applies and the command holds a `cd`, `pushd` or `popd`. A command with no such
word produces the same plan as before. The walk reports each Danger stage's
`cd` steps (`StageDir`) or why they cannot be trusted. Planning resolves them
against the filesystem.

Which `cd` forms resolve:

- `cd <path>` and `cd -- <path>` with a literal path resolve relative to the
  directory in effect before the `cd`. The chain is followed lexically, as the
  shell does (`cd link && cd ..` is the parent of `link`), then canonicalized.
- Unresolved: `cd` with no argument, `cd -`, any option, extra operands,
  `pushd`, `popd`, a path with `$`, a glob, a brace expansion, a backslash or
  `~`, and any relative `cd` while `CDPATH` is set in Aegis's own environment
  or by an earlier stage of the command line. A stage that names `CDPATH`
  anywhere (`export CDPATH=/tmp`, `CDPATH=x`, `declare -x CDPATH=x`) makes
  every later relative `cd` unresolved, because Aegis does not track the value.
  Bash does not search `CDPATH` for an absolute operand or one that starts with
  `.`, `..`, `./` or `../`, so those stay resolved.
  Redirections on the `cd` stage (`cd x 2>/dev/null`) are syntax, not
  operands, and do not change the form. A quoted word is always an operand.
  Quoting counts: single quotes make every character literal, so `cd 'x$y'`
  resolves to the directory named `x$y`. Inside double quotes only `$`, a
  backtick and a backslash still expand, so `cd "~/x"` resolves to a directory
  named `~` and `cd "x$y"` stays unresolved. The `-C` values of ADR-048 reach
  planning with their quotes already removed, so they stay conservative.
- A literal absolute `cd` (`cd /known/repo`, `cd -- /known/repo`) reached
  through `&&` after an unresolved `cd` recovers a known directory: it runs
  only if the earlier steps succeeded and lands in the same place whatever the
  directory was. Behind `;`, `||` or `&`, or with a dynamic or relative path,
  the earlier unresolved state stands.
- A `cd` that hides behind `eval`, `source`, `.`, a launcher or a nested shell
  (`bash -c 'cd x'`) leaves its own stage and every later stage unresolved.
  `eval`, `source` and `.` count whether or not a `cd` word appears beside
  them, because the string or file they run can hold one. The walk itself
  starts only when the command holds a `cd`, `pushd` or `popd`, so a command
  with `source` but no `cd` keeps its old plan. A stage that is itself `pushd`
  or `popd` reports the unsupported form, whatever it holds.

Which separators carry the directory forward:

- Only `&&` carries a resolved `cd` to the next stage.
- A `cd` followed by `;`, a newline, `||` or `&`, or reached through `||`,
  makes every later stage unresolved: if the `cd` fails, the later stage runs
  in the old directory, and Aegis does not snapshot both (ADR-048).
- A `cd` that is a pipeline stage runs in a subshell and changes nothing after
  it.
- A `cd` inside a `( ... )` group applies inside the group only.

Several directories: every resolved directory gets its own git Snapshot,
deduplicated by repository top level as in ADR-048. A stage in the process cwd
still takes the cwd pass, and a moved stage whose repository is the process
cwd's repository adds no second Snapshot of it when the cwd pass runs.

Directories that are not a repository, or do not exist:

- A non-git Danger stage in an existing directory outside any git working tree
  behaves like the same stage run directly there: no git Snapshot, no
  disclosure, no halt. Its directory is an **opportunistic directory**: it gets
  a git Snapshot only if it lies inside a repository the git provider covers.
- A git stage in such a directory is an unresolved Git target, as `git -C` is
  under ADR-048.
- A non-git stage that a `cd` moved and that sits behind a launcher which
  changes the directory again (`env -C`, `env --chdir`, `sudo -D`,
  `sudo --chdir`), or behind one Aegis cannot map, is an unresolved target, as
  such a git stage is under ADR-048. Without a `cd`, such a stage keeps its
  pre-#525 plan.
- A git stage after an unresolved `cd` whose options are all absolute
  (`cd "$X" && git -C /known/repo ...`) names its tree whatever the directory
  was, so it resolves from the options alone. A relative option keeps the
  `cd` reason.
- A directory that does not exist at planning time (`mkdir n && cd n && ...`) is
  unresolved for any stage.

Unresolved directories follow ADR-048 disclosure: no substitute cwd Snapshot,
the dialog and audit entry name the reason, an unattended auto-approved run
outside `Mode::Audit` halts with `RecoveryDegradation::UnresolvedGitTarget`,
and `Mode::Audit` takes no Snapshot and only explains. The reasons are new
`GitTargetUnresolved` values; no new degradation variant is needed.

Only the git provider follows the directory. Docker, Postgres, MySQL, Supabase
and SQLite keep using the process cwd.

### Why router `CwdState` stays stricter

The two readers answer different questions. The router decides whether a
command may be auto-approved, so a trusted-but-wrong cwd weakens a security
decision. Snapshot planning decides where a recovery point goes, and an error
there is already disclosed: an unresolved directory is shown and halts an
unattended run. Planning can therefore trust more forms than the router, and
the router keeps its narrow rule, including its degrade-on-first-doubt
behaviour. This ADR changes no auto-approve decision, risk level or router
state.

## Consequences

`cd ../other && rm -rf x` restores `other`'s uncommitted work and leaves the
cwd repository untouched. `cd a && rm -rf x && cd ../c && rm -rf y` takes two
Snapshots.

A command that is effect-opaque still gets the ADR-016 cwd Snapshot for the
opacity, as in ADR-048. `cd "$D" && rm -rf y` is both: the cwd Snapshot covers
the opacity and the `cd` target is disclosed as uncovered.

The walk is an approximation of shell grammar built on the parser's list
segmentation. Constructs it cannot place (`{ ...; }` groups, `if`/`for` bodies)
pass through their separators, so a `cd` inside one degrades to unresolved
rather than guessing. That can add a halt for a harmless command, never a wrong
Snapshot silently.

Out of scope: changing router `CwdState`, making SQLite or any other non-git
provider follow the directory (a separate issue tracks the SQLite relative-path
case), and resolving `pushd`, `popd`, `CDPATH`, `cd -` or dynamic paths.
