---
description: Roll the workspace back to an Aegis Snapshot
argument-hint: "[snapshot-id]"
disable-model-invocation: true
---

Help the user roll the workspace back to an Aegis Snapshot. Snapshot id from the user, if any: $ARGUMENTS

Aegis reserves `aegis rollback` for the human operator, and the Aegis PreToolUse hook denies it. Never run it yourself. Hand the user the exact line to run in their own terminal.

Run `aegis snapshot list` through the Bash tool, so the hook sees it. It is the only `aegis` command this command runs.

1. If no snapshot id was given, tell the user to run `aegis rollback --last` in their own terminal. It restores the Snapshots of the latest command recorded for the current repository or directory. It shows its own summary and asks the user to confirm before it restores anything. Do not list Snapshots yet.
2. If the user wants an older Snapshot instead, run `aegis snapshot list` and show them the list. Ask which Snapshot to restore and wait for the answer.
3. If a snapshot id was given, or the user picked one, show that Snapshot's entry as `aegis snapshot list` reports it. Tell the user that rolling back overwrites the current state that Snapshot covers.
4. Give the user the exact `aegis rollback <id>` line, with that id, to run in their own terminal. Stop there.
