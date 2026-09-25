# ADR-022: Language-aware analysis is an additive isolated stage

## Status

Accepted

## Context

Aegis currently classifies the shell command text that an agent asks it to
execute. The shell tokenizer, Aho-Corasick Quick scan, regex Patterns, and
Token-prefix rules cover visible command shapes well, but they cannot reliably
describe operations hidden inside source-language syntax. Examples include
`python -c "os.remove(path)"`, `node -e "fs.rmSync(path)"`, interpreter stdin,
heredocs, and named script files.

Replacing the shell scanner with a language parser would be the wrong trade-off.
The existing scanner owns shell boundaries, launcher normalization, fast common
cases, and the sub-2-ms safe-command target. A language parser also cannot prove
what arbitrary code will do: imports, dynamic dispatch, runtime values, generated
code, native extensions, and time-of-check/time-of-use changes remain outside a
bounded static assessment.

Tree-sitter provides production-quality concrete syntax trees and error-tolerant
parsing for many languages, but its runtime and generated grammars introduce
native C build inputs, binary-size cost, grammar supply-chain risk, and parser
isolation requirements. The project therefore needs an explicit boundary rather
than treating Tree-sitter as a drop-in scanner replacement.

The target user population runs more than Python and JavaScript. The production
language set is Shell/Bash, Python, JavaScript, TypeScript, PowerShell, PHP, Ruby,
Go, Perl, and Lua. Shipping every adapter in one unqualified release would make
correctness, platform parity, and review unmanageable.

## Decision

### 1. Preserve shell detection and add a bounded slow path

Language-aware analysis complements the existing Scanner. It does not replace
the shell tokenizer, Quick scan, regex Patterns, or Token-prefix rules. Existing
Matches are monotonic: later analysis may add Matches, raise RiskLevel, or record
Analysis degradation; it may never remove a Match or lower RiskLevel.

The no-source safe-command path remains synchronous and must stay below 2 ms.
`Scanner::assess` remains pure and performs no filesystem access. Language-aware
analysis runs only when command-visible source or a supported script invocation
creates an analysis target.

Script source inspection is an asynchronous stage after the baseline Assessment
and before Policy evaluation. The parent process performs bounded source routing
and reads using Tokio, merges all analysis results into one Assessment, and only
then asks the PolicyEngine for a Decision.

### 2. Isolate parsing in an ephemeral worker

Tree-sitter parsing and language adapters run in a self-spawned, ephemeral worker
process. One worker session may accept a bounded sequence of analysis requests for
one intercepted command and exits when that Assessment is complete. There is no
daemon, runtime plugin loader, or network service.

The parent and worker communicate over a versioned pipe protocol. Source bytes are
sent through the protocol; temporary source files are forbidden. The worker parses
only the supplied bytes and may not read the filesystem or execute subprocesses.
The parent owns recursive routing, limits, result merging, and policy integration.

A worker crash, timeout, incompatible protocol response, or invalid response is an
Analysis degradation. Results already produced by the shell Scanner or an earlier
analysis target are retained.

Language adapters and Tree-sitter dependencies live behind one focused workspace
library boundary, provisionally `aegis-language`. Root-binary code owns async
orchestration and delegates worker analysis to that library; `main.rs` remains
CLI parsing and orchestration only.

### 3. Normalize syntax into Detected operations

Each adapter uses Tree-sitter queries for structural capture and typed Rust code
for semantic interpretation. It emits language-neutral Detected operations rather
than assigning risk directly from an API spelling. A shared classifier maps:

- operation kind and effect scope;
- modifiers such as recursive, forced, or destructive mode; and
- Operand certainty (`Known`, `Partial`, or `Dynamic`)

into the existing Category, RiskLevel, explanation, safer alternative, and Match
vocabulary.

Bounded symbol resolution follows only direct imports, aliases, adjacent literals,
literal concatenation, simple constant bindings, and language escapes inside one
source target. It excludes type inference, interprocedural analysis, dependency or
import traversal, arbitrary data-flow analysis, and intent inference.

A recognized process, shell, or eval sink always emits a CodeExecution Match. A
literal payload also becomes a bounded recursive analysis target. A dynamic payload
records Analysis degradation in addition to the CodeExecution Match; uncertainty
about the payload never hides the visible execution sink.

The initial operation scope is destructive effects plus code-execution sinks:
filesystem deletion and overwrite/truncation, dangerous permission or ownership
changes, device or critical-path writes, destructive database operations, literal
shell/process/eval payloads, and a deliberately selected set of destructive cloud,
container, and package APIs. Generic HTTP/exfiltration detection, malware or
vulnerability scanning, taint analysis, and general program comprehension are
non-goals.

### 4. Use a common Detection rule and evidence model

Before 1.0, the scanner model will expose a common Detection rule contract with
three concrete mechanisms: regex Pattern, Token-prefix rule, and Language-aware
rule. Language-aware rules are built in. Project configuration cannot define
custom Tree-sitter queries, disable a built-in semantic rule, or lower its
classification.

Every Match carries typed evidence. Language-aware evidence records a Detected
operation, Operand certainty, and Analysis provenance. The current singular
`DecisionSource` concept is replaced by Assessment basis: all decisive Match IDs
at the Assessment's maximum RiskLevel, or `Fallback` when nothing matched. Each
Match also identifies its detection mechanism and whether it is built in or
custom.

Analysis status is typed as `NotApplicable`, `Complete`, or `Degraded`, with
per-target results and typed degradation reasons. Reasons include unsupported or
unavailable grammar, incomplete syntax, unsafe or unavailable source, unsupported
encoding, size/count/depth/timeout limits, dynamic source or cwd, and worker or
protocol failure.

### 5. Fail closed without inventing a synthetic RiskLevel

Analysis degradation is orthogonal to RiskLevel; it is not a synthetic `Warn`.
It may coexist with `Safe`, but it never authorizes safe auto-execution.

In enforcing `Protect`, a degraded Assessment requires explicit one-time approval.
`Strict` continues to block non-safe and Indirect execution by default, but this ADR
adds one narrow exception: an interactive, non-persistable Analysis override may
approve a non-`Block` language-aware Match or degradation. It does not authorize an
unrelated Strict denial. Non-interactive degradation is denied. Trusted global
`Audit` mode remains observe-only and records the degradation. An intrinsic `Block`
remains unbypassable in every posture.

Completed language-aware `Warn` and `Danger` results use the existing CI policy.
In interactive Protect/Strict flows, a language-aware Match or degradation cannot
be auto-approved by an allowlist or a policy-rule `Allow`; only a one-time approval
or Analysis override, respectively, is valid. Neither authorization is persistable.
Audit mode and the global Toggle remain explicit trusted posture controls.

The TUI presents one consolidated Assessment confirmation containing the decisive
Matches and any degradation, with other Matches available as detail. It does not
prompt once per operation. A Required recovery degradation may still need its
separate recovery prompt because it approves a different failure boundary.

### 6. Treat script-file reads as catch-only evidence

Script source inspection reads only local regular files through the caller's
permissions and within explicit budgets. Symlinks, FIFOs, sockets, devices,
directories, unavailable paths, unsafe metadata, and unsupported encoding produce
Analysis degradation rather than a read. Absolute paths are allowed. The parent
records the inspected bytes' hash and descriptor metadata without claiming that
the interpreter will later execute those same bytes.

Successful inspection may add Matches and raise RiskLevel, but it does not remove
Effect-opaque execution or waive Required recovery. This preserves the backstop for
TOCTOU changes, imports, generated code, and other runtime effects outside the read
source.

Routing precedence is explicit interpreter, then verified shebang for a directly
executed file, then target extension for a file created by a visible heredoc.
Explicit interpreter wins over extension. Routing uses a built-in canonical
interpreter/runner registry, basename and versioned-name normalization, existing
Launcher-prefix logic, and trusted global aliases only. It performs no `PATH`,
`--version`, or content-guessing probes. ADR-040 extends the built-in registry
to command runners whose visible argv names an interpreter or script. Resolving
package manifests or expanding build tasks such as `npm run`, `go generate`,
Make, Composer, Bundler, and framework CLIs remains a v1 non-goal.

The verified-shebang check is discovery, not committed analysis: routing does
not yet know whether a directly executed file is a script at all. A read that
exceeds the script-file budget, or whose content is not valid UTF-8, both
conclusively rule out a shebang (a short ASCII prefix), so it resolves
identically to a read that found no `#!` line — `NotApplicable`, not
degradation. This is narrower than the general "unsupported encoding produces
degradation" rule above, which governs an `Inline`/`ScriptFile` target: there,
an interpreter has already committed to treating the argument as source, so an
equivalent read failure leaves Aegis blind to bytes it knows will execute, and
stays a genuine `Degraded`. A directly executed file that turns out not to be
a shebang script at all (e.g. a compiled binary) carries no such certainty of
imminent script execution to begin with.

The parent tracks only a literal top-level `cd -- <path> &&` cwd change. Dynamic
`cd`, `pushd`, `popd`, `source`/`.`, `eval`, substitutions, or otherwise unresolved
cwd cause degradation. `eval` can run a `cd` the parent cannot see, as `source`/`.`
can, so a relative target after it never resolves against the cwd known before it
ran (GHSA-xj54).

A relative `Direct exec` target under a dynamic cwd records `Dynamic source`
without attempting a read. Its language is not known until a shebang can be read,
so the routing result carries degradation without presupposing a language. `Script
file` targets under the same dynamic cwd, and both target shapes under an
unavailable runtime cwd, likewise record `Dynamic source`. This preserves honest
degradation while avoiding evidence from a possibly wrong working directory.

Quoted heredocs provide exact source. Expanding heredocs are analyzed as the visible
template, but expansions, substitutions, or relevant escapes also record
degradation; Aegis never evaluates them. An in-memory heredoc body may be linked to
a later invocation of the created file in the same shell command.

Interpreter stdin is analyzed only when source is statically recoverable: a quoted
heredoc, literal here-string, or a narrowly proven literal-only producer such as
`printf '%s'`. Dynamic pipelines remain Effect-opaque and degrade honestly.

#### Amendment (2026-09-23, extended 2026-09-24)

Routing's launcher-prefix list (`launcher_prefix_lengths`) only recognizes a
closed set of wrapper words — `sudo`, `env`, `timeout`, `nice`, `command`,
and a handful more. A wrapper outside that list (`setsid`, `stdbuf`,
`strace`, `ionice`, `taskset`, `exec -a`, `doas`, `time -p`, `find -exec`,
and any word not yet added) hid the interpreter behind it from routing
entirely: routing found no target, so the language-aware stage never ran,
and the command auto-approved as if it carried no source at all. Adding each
wrapper word by name to the launcher list only closes the specific gap
reported that day; the next wrapper word leaks the same way (issue
#384/#430).

Routing now closes this class of gap once, structurally: when a command or
pipeline stage reaches the end of routing having produced no target by any
other path, and a token past its own program names a known registry
interpreter (after unquoting, basename, and the same versioned-name
normalization routing already applies), routing degrades that stage to
`Unresolved`/`Dynamic source` instead of leaving it silent. This does not
depend on recognizing the wrapper word itself — an interpreter name later in
the command is enough on its own. The interpreter check also reads the
opening word of a token that still carries embedded whitespace after
unquoting (`script -c "python3 ./evil.py"`), since a plain basename lookup
on that whole token would instead find the trailing path segment of its
*last* word.

A program word routing can only read through shell expansion it does not
evaluate (`$VAR`, `${X:-python3}`, a backtick or `$(...)` substitution, a
brace list such as `{a,b}`) degrades the same way, but only once the stage
also carries an operand for it to act on: a bare `$EDITOR`/`$SHELL` with
nothing to act on is an everyday interactive launch and stays untouched. A
program word that instead names a shell `alias` defined earlier in the same
command (`alias runpy=python3`) resolves only as opaquely as its own
replacement text: standing in for a known interpreter, for a dynamic word,
or for nothing parseable at all degrades the same way an unenumerated
wrapper word does; an alias for anything else (`alias ll='ls -l'`) is left
to the rest of routing (issue #384/#430).

When no token names a known interpreter, routing instead emits a `Launcher
operand` candidate for every distinct path-like operand of the stage (past
any leading flags), not only the first, rather than leaving the stage
silent: `setsid ./pyx`, `strace setsid ./pyx`, and `setsid -u ./x ./pyx` (a
path-like flag value) all produce one candidate per path-like operand. This
costs nothing beyond what `Direct exec` already does for a bare path-like
program: `resolve` still reads the file and only treats it as a target with
a verified shebang, so a non-script operand (`setsid ./notes.txt`) stays
unclaimed. Unlike `Direct exec`, a missing path or a literal directory
resolves to no target and no degradation rather than prompting, since the
net picked this operand out of an ordinary command's own arguments rather
than the command naming it as the thing to run, and a missing or directory
argument (`vim ./new.txt`, `du -sh ./srcdir`) is routine there (issue
#384/#430).

This walk also reaches a `for`/`select` loop header's own list (`for f in
dir/*.py; do ...; done`): each path-like list item is an operand of a stage
whose program names no interpreter, so it routes the same way `setsid`'s
operand does, unconditionally, whatever the loop body does with the
variable. This includes a list-only loop that never runs the variable
(`for f in dir/*; do echo "$f"; done`), an accepted false positive: no
static rule can prove the loop variable never executes (issue #384/#430,
GHSA-xj54).

For this candidate only, `~/rest` expands to an absolute path with a home
directory supplied by the caller. Routing never reads the process environment
for that value. Without a supplied home, with `~user/rest`, or when `rest`
is dynamic, the stage degrades to `Unresolved`/`Dynamic source`. The same
degradation applies when an earlier stage on the same command line writes a
shell variable — bare or `export`/`declare`/`typeset`/`readonly`/`local`
assignment (with or without a redirection attached), `read`, `printf -v`,
`mapfile`, `readarray`, `getopts`, `unset`, `let`, `eval`, `source`/`.`, or a
`for`/`select` loop header — resolved through the same effective-program
stripping as everywhere else, so `builtin eval`, `command source`, and `time
export ...` all count. Bash re-reads its own current `HOME` for every `~`
expansion, so a stale caller-supplied value is no longer trustworthy once
any of those runs first, whatever variable it names: routing cannot
statically tell an unrelated write from one that reaches `HOME` (issue
#384/#430, GHSA-xj54). This is a wider net than "did this stage touch
`HOME`": `export PATH=/x; setsid ~/pyx` now prompts too, an accepted false
positive. A path-like operand that contains shell expansion syntax or glob
syntax also degrades; routing neither evaluates nor expands it. Thus no
path-like operand is silently dropped. Operands without `/` remain out of
scope: `setsid $SCRIPT` stays unclaimed, because treating it as dynamic
would also catch routine argument values such as `kill $PID` and `make
$TARGET`.

Only a `/` written outside a command substitution makes an operand path-like.
The text inside `$(...)` or backticks belongs to the substituted command, and
routing handles that command on its own. So `--body "$(cat <<'EOF' ... EOF)"`
stays unclaimed even when the body mentions `docs/x.md`, while `"$(pwd)/pyx"`
keeps its outer `/` and degrades. Routing finds substitutions in the raw
stage text, before the quotes are removed, so quoted or escaped substitution
syntax such as `'$(x/pyx)'` stays a literal path. An operand whose only `/`
sits inside a substitution, such as `setsid "$(echo ./pyx)"`, falls into the
same gap as `setsid $SCRIPT`. When a substitution never closes, the unmasked
tokens decide, so a quote the scan cannot pair degrades the stage instead of
hiding a `/`. Extra slashes after the tilde (`~//pyx`) stay under the home.

The tokenizer does not understand ANSI-C quoting (`$'...'`) either, and can
glue a real operand into the garbage token that results (`strace -o
$'\'$(echo x' ./pyx ')'` tokenizes to nonsense that still happens to contain
a `/`). An unclaimed stage whose raw text carries `$'` outside single and
double quotes degrades to `Unresolved`/`Dynamic source` rather than trusting
whatever the tokenizer made of it. The rule does not wait for a surviving
operand, because the same mis-tokenization can swallow the operand into a
dash-prefixed token (`strace -o$'\'' ./pyx #'`). The check runs
after the exclusion-list exemption above, so `printf $'a\n'`, `echo $'x'`,
and `grep $'\t' file` are unaffected. `sed $'s/\t/ /' file` now prompts too,
an accepted false positive (issue #384/#430, GHSA-xj54).

A fixed exclusion list holds the programs that legitimately name a command,
or a filesystem path, as data rather than run it: `echo`, `printf`, `which`,
`type`, `whereis`, `man`, `info`, `help`, `apropos`, `grep`, `egrep`,
`fgrep`, `rg`, `ag`, `ls`, `cat`, `head`, `tail`, `less`, `more`, `file`,
`stat`, `wc`, `diff`, `apt`, `apt-get`, `apt-cache`, `dnf`, `yum`, `brew`,
`pacman`, `git`, `update-alternatives`, `dpkg`, `mkdir`, `rmdir`, `touch`,
`rm`, `cp`, `mv`, `ln`, `chmod`, `chown`, `chgrp`, `basename`, `dirname`,
`realpath`, and `readlink`, plus `command -v`/`command -V` and `type`
lookups. A stage whose own program is on that list stays unclaimed even when
a later word spells an interpreter name (`echo python3`, `grep -r node
src`, `apt install python3`, `mkdir python3`, `rm -f node`).

A generic interpreter check also examines the value after the first `=` when
the non-empty prefix contains no whitespace. It keeps its whole-token check,
then applies the same basename and command-string rules to the value. This
covers assignment, long-option, short-option, and dotted-key forms without
assuming which program consumes them. When that value names no interpreter
but is itself path-like, it becomes its own `LauncherOperand` candidate — the
same shebang check `setsid ./pyx` gets — instead of the whole `prefix=value`
token being read as one: `--use-compress-program=./evil.py` and
`SHELL=./evil.sh` both candidate on `./evil.py`/`./evil.sh`, not the flag or
the assignment as a whole. The check keeps unwrapping while the value it
just peeled off still splits the same way into a non-empty, whitespace-free
name and a value, so a nested value (`tar --checkpoint-action=exec=./pyx`)
reads `./pyx`, not the intermediate `exec=./pyx`, which carries no `/` of
its own and would otherwise stay unread (issue #384/#430, GHSA-xj54).

A path-like candidate this walk finds through any of the D4, glued
short-flag, or plain-operand channels reduces to its own leading word
before it is read as a path (GHSA-xj54 follow-up). Without this, a quoted
multi-word value such as `tar -I'./pyx -d' -cf out.tar dir` or `rsync -e
'./pyx -p 22' a h:b` was read whole, spaces included, as one literal path
that almost never exists on disk, so the stage silently auto-approved
instead of candidating on `./pyx`. This is the same leading-word reduction
a dispatched executor value already gets, described below. The reduction
only runs once the whole, unreduced candidate is confirmed to carry a `/`:
a candidate whose only slash sits past its first word, such as a quoted,
literal `$(...)` string that reached here as data rather than a real
substitution, still reaches the literal-path check on its full text
instead of losing that word, and the danger it carries, to the reduction.

A `-`-prefixed token with no `=` is no longer silently skipped. Nothing
marks where a short-flag bundle's own letters end and a glued value begins
(`tar -I./pyx`, `rsync -e./pyx`), so routing tries every split of the
token's leading letter run as a possible glued value: `-cI./pyx` splits
into `I./pyx` and `./pyx`. A split that names a known interpreter degrades
the stage at once; any other split becomes its own path-like
`LauncherOperand` candidate, the same as an ordinary operand. This
interpreter check only counts a split whose value carries more than one
word: a linker or include flag's own single-word argument (`-lpython3.12`,
`-lnode`, `-Ipython3`) is not a match, since it never runs anything, but a
quoted multi-word value glued the same way (`-I'python3 ./evil.py'`) still
degrades, since its first word is a command line the flag hands to a real
interpreter, not an inert compiler flag. A spurious earlier split from an
ambiguous bundle costs nothing beyond an extra candidate that resolves to
no target. `man`'s own `-P`/`--pager=` dispatch gets a matching glued form
(`man -P./pyx ls`), since `man` sits on the exclusion list above and never
reaches this generic walk. A glued value that carries `$` or glob syntax
(`-I$HOME/inc`) still fails the literal-path check and degrades instead of
resolving to a candidate, the same accepted false positive an ordinary
operand gets (issue #384/#430, GHSA-xj54).

A handful of option values and environment variables run a command instead
of naming one as data, and degrade even for a program on the exclusion list
above: git's `-c`/`--config-env` for a command-carrying config key (`core.pager`,
`core.editor`, `core.sshCommand`, `alias.*`, `diff.external`,
`credential.helper`, `sequence.editor`, `gpg.program`, `filter.*.clean`, and a
handful more), man's `-P`/`--pager=`, and ssh-family `-o` options. For `ssh`,
`scp`, and `sftp`, both `-o VALUE` and `-oVALUE` inspect case-insensitive
`ProxyCommand`, `LocalCommand`, and `KnownHostsCommand` values in either
`Key value` or `Key=value` form. Other ssh options remain data. `rg`'s
`--pre` (its own preprocessor command) and `ag`'s `--pager` get the same
dispatch, in a separate-token or glued `=` form only (`rg --pre ./pyx foo
.`, `rg --pre=./pyx foo .`, `ag --pager=./pyx foo`); neither option has a
short or no-`=` glued form to read, and a same-prefixed sibling such as
`--pre-glob` is matched against the whole flag, never mistaken for it
(GHSA-xj54 follow-up). The scan stops at the first bare `--`: both `rg`
and `ag` treat everything past it as a positional pattern or path, not an
option, so `rg -- --pre ./pyx foo .` reads `./pyx` as data, not as the
`--pre` value (GHSA-xj54 follow-up). The same holds for an executor environment
variable's assigned value (`PAGER`, `GIT_PAGER`,
`MANPAGER`, `EDITOR`, `VISUAL`, `GIT_EDITOR`, `GIT_SSH_COMMAND`, `LESSOPEN`,
`LESSCLOSE`, `GIT_ASKPASS`, and a handful more), whichever of three shapes it
appears in on the same line: leading the stage, past a leading `env` launcher,
or set in its own `export`/`declare -x`/bare assignment stage (issue #384/#430).
Every one of these values gets the same interpreter-or-path-like treatment as
the generic D4 check: `ssh -o ProxyCommand=./evil.py host` and `ssh -o
'ProxyCommand ./evil.py %h' host` both resolve to a `LauncherOperand` for
`./evil.py` — for a multi-word value, the candidate is the first word past
any leading prefix words, same as the interpreter-name check reads it — while
a value naming no interpreter and carrying no `/` (`ProxyCommand=none`,
`ProxyCommand='nc %h %p'`) stays untouched, as before.

This trades false positives for closing the false-negative gap: a program
outside both the interpreter registry and the exclusion list that happens to
take an interpreter name as an unrelated argument will now prompt even
though it never runs that interpreter. Glob arguments at such programs also
prompt, including routine uses such as `du -sh ./build/*` and `tar -czf
out.tar ./dist/*`. These are accepted costs, not defects. Routing fails closed
rather than silently trusting an unrecognized shape, and the exclusion list
can grow as legitimate cases turn up. Parsing the full Bash grammar so routing
understands every wrapper's own argument conventions precisely, instead of
scanning tokens after the fact, is out of scope for this net and tracked
separately (issue #434).

Known gaps this amendment deliberately leaves open:

- Exotic `HOME` writers the parent does not track: a redirection whose target
  names `HOME` through brace/indirect expansion (`{HOME}>file`), a `coproc`
  named `HOME`, or a `trap` handler that assigns `HOME` on a signal (`DEBUG`,
  `EXIT`). These reach `HOME` only through bash semantics the parent does
  not model.
- A multi-command `Executor value` (`PAGER='less; ./pyx'`,
  `core.pager='cd /tmp && ./pyx'`). The value scan reads only the opening
  word, so a later command joined by `;`, `&&`, or `|` stays unrouted. The
  same gap exists in 0.6.9 (issue #434).
- An accepted false positive: the `printf` HOME-write check counts any
  `-v`-prefixed token after the program name, so `printf %s -v` withholds
  HOME trust although `-v` after the format string is an argument, not an
  option.

### 7. Bound recursive and encoded analysis

Literal process or eval payloads become new targets in a bounded cross-language
work queue. Targets are deduplicated by language and source hash. Depth, target
count, aggregate bytes, and total time are capped; cycles or exhausted budgets
produce degradation while preserving prior Matches.

Dynamic process, shell, and eval payloads are not enqueued or evaluated. The
visible sink still emits a CodeExecution Match and the unresolved payload produces
Analysis degradation.

The initial recursion-depth ceiling is 8. The implementation plan must derive the
remaining defaults from prototype benchmarks. Initial research values are:

- existing inline-source limit: 16 KiB;
- script-file default: 256 KiB;
- hard per-file ceiling: 1 MiB;
- maximum script files per command: 8;
- maximum aggregate source: 1 MiB; and
- total language-analysis timeout: 100 ms.

Defaults are configurable within non-configurable hard ceilings. Project config may
only tighten them; trusted global config may tune them within the ceilings.

The pre-1.0 encoding contract is UTF-8, with a UTF-8 BOM mapped back to original
byte spans. Source hashes cover the original bytes. Invalid UTF-8 and UTF-16 degrade;
UTF-16 support is deferred with the PowerShell 1.x adapter. Until its pinned native
external scanner is corrected upstream, the Bash adapter additionally degrades
non-ASCII UTF-8 before parsing; it must not invoke native code known to be unsafe
for those code points. This is conservative evidence loss, never authorization.

Base64, hex, gzip, encryption, and custom payload decoding are out of scope. A
decode-to-eval shape still emits a code-execution operation and degradation rather
than pretending the decoded behavior is known.

### 8. Ship qualified grammars in one binary

Official release binaries statically include every production-qualified grammar.
They never download grammars at runtime and never load dynamic grammar libraries.
The qualified language set must be identical across:

- `x86_64-unknown-linux-musl`;
- `aarch64-unknown-linux-musl`;
- x86_64 macOS; and
- aarch64 macOS.

A grammar is eligible only after independent qualification of license, maintenance,
Tree-sitter ABI compatibility, Rust binding, pinned version or commit, `build.rs`
and bundled native source, transitive dependencies, upstream corpus, Aegis security
corpus, fuzzing, and all-target release builds. Official upstream grammars are
preferred; community grammars must pass the same gate. The release contains a
grammar manifest with versions, provenance, and licenses.

This ADR creates a narrow exception to the project's no-C-build preference: only
the pinned Tree-sitter runtime and production-qualified generated grammars may add
native C compilation. It is not permission for general native dependency growth.
Any other C/native dependency still requires a separate ADR and release-matrix
evidence.

### 9. Stage production enablement by language

The pre-1.0 milestone delivers the common foundation plus Python, JavaScript,
TypeScript, and Shell/Bash. Go, PHP, Ruby, PowerShell, Perl, and Lua follow as
independently qualified 1.x adapters.

Qualified adapters are default-on; there is no separate
`language_analysis.enabled = false` escape hatch. Existing Audit and Toggle controls
remain the trusted posture controls. An adapter remains unsupported, and produces
honest degradation when its source is encountered, until every qualification gate
passes. Release enablement is per language rather than a big-bang switch.

### 10. Extend audit without persisting source

Audit schema v2 adds typed Matches, Assessment basis, analysis status and
provenance, and stable detection IDs. Existing `matched_patterns` and `pattern_ids`
remain as compatibility projections. Absence of v2 fields identifies a legacy v1
line; logs are never rewritten, and mixed v1/v2 querying and integrity verification
must work. The hash chain covers the actual serialized form of each entry.

Analysis provenance may persist language, source origin, rule ID, operation, file
path when applicable, source hash, line/column/byte span, Operand certainty, status,
and degradation reason. It must not persist script contents, full snippets, imported
source, variable values, or syntax trees. The TUI may render a short in-memory
snippet, but it does not write that snippet to the Audit log.

L1 Iteration 10 Slice 2 tightens rendered and persisted behavior further:
production-created Language-aware `Match` values carry a stable, source-free label
(`LANGUAGE_AWARE_MATCH_LABEL`) because `aegis_types::language_match` does not accept
matched text. A defense-in-depth projection (`MatchResult::public_matched_text`) and
its `Debug` implementation apply the same label to any hand-built `LanguageRule`
match reaching an outward surface. The detected operation and its metadata-only
provenance describe the finding instead. A future change that wants to restore a
rendered snippet (still disallowed for the Audit log) must update `language_match`
and this ADR together, not just the TUI.

Aegis adds no automatic network telemetry. A local-only aggregation command may
summarize interpreter/language, invocation shape, status, latency, and size buckets;
export is an explicit user action. Real Audit logs are never automatically uploaded
or used as test fixtures.

### 11. Qualify behavior, not only parsers

Each language adapter must pass the same production gate: pinned grammar and license
evidence; all-target artifact parity; positive, negative, alias, literal, malformed,
and new-syntax corpora; worker crash/timeout/protocol tests; resource-limit and
recursive cross-language tests; adapter and protocol fuzzing; Audit v1/v2
compatibility; Shell, Watch, Hook, and CI integration; safe-hot-path benchmarks; and
slow-path latency, memory, and binary-size budgets.

The workspace test, clippy, format, audit, deny, review, re-review, release-build,
performance, live integration, and fuzz gates remain mandatory. A parser that merely
builds is not a supported language.

## Consequences

### Positive

- Dangerous operations visible in supported source syntax can produce the same
  typed Assessment and Policy behavior as shell-level detections.
- The existing fast path and shell semantics remain intact.
- Worker isolation contains parser crashes, separates parser address space, permits
  a parent-enforced timeout, and makes peak-memory budgets measurable without a
  resident daemon. It does not claim a portable hard memory cap.
- Shared Detected operations reduce semantic drift between language adapters.
- Honest degradation makes unsupported or dynamic cases visible instead of silently
  treating them as safe.
- Per-language qualification permits broad eventual coverage without coupling every
  adapter to the 1.0 release.

### Negative

- Official binaries grow because qualified grammars are statically linked.
- Builds gain a narrowly scoped native C toolchain requirement.
- A worker process adds latency to commands that expose analyzable source.
- Audit schema, TUI, config ratchets, CI, release packaging, and policy integration
  all require coordinated migration.
- The source reader handles sensitive local files, so metadata checks, privacy
  limits, and adversarial tests become part of the security-sensitive attack
  surface.
- Strict gains a narrow Analysis override for non-`Block` language-aware Matches
  and degradation; every unrelated Strict denial remains terminal.

### Residual limits

- This remains a heuristic guardrail, not a sandbox or proof of program behavior.
- Dynamic dispatch, runtime imports, generated or encoded payloads, native code,
  unresolved values, and TOCTOU changes can hide effects.
- Successful Script source inspection never makes Script-file execution trusted.
- Unsupported languages and unqualified adapters degrade rather than receiving a
  weaker best-effort parser.
- Package-manifest and build-task expansion, and dependency traversal, require
  later decisions. Explicit child argv behind the runners in ADR-040 is covered.
