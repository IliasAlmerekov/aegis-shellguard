---
description: Roll the workspace back to an Aegis Snapshot
argument-hint: "[snapshot-id]"
disable-model-invocation: true
---

Roll the workspace back to an Aegis Snapshot. Snapshot id from the user, if any: $ARGUMENTS

Run every `aegis` command below through the Bash tool, so the Aegis PreToolUse hook sees it. Do not use any other way to run them.

1. If no snapshot id was given, run `aegis snapshot list` and show the user the list. Ask which Snapshot to restore and wait for the answer.
2. Show the chosen Snapshot's entry as `aegis snapshot list` reports it. Tell the user that rolling back overwrites the current state that Snapshot covers.
3. Ask the user to confirm the rollback of that exact id. Stop if they do not confirm.
4. After they confirm, run `aegis rollback <id>` with the confirmed id and report its output.

If Aegis refuses `aegis rollback` because the command is reserved for the human operator, do not retry it in another form. Give the user the exact `aegis rollback <id>` command to run in their own terminal.
