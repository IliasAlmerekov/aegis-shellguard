# Issue tracker: GitHub

## This repo

- Pass `-R IliasAlmerekov/aegis-shellguard` to every `gh issue` and `gh pr` call. The clone also has a `ghsa` remote (a private advisory fork), so `gh` can resolve the wrong repo from `git remote`.
- Write issue titles, bodies, and comments in English.
- A command that gets past Aegis analysis (a bypass) is a security report. File it as a private GitHub Security Advisory, never as a public issue.
- The `1.0` milestone is the only release gate. Put an issue in it only when it falsifies a promise in `PRD.md`.
- **Milestone status**: one call lists the open `1.0` issues with labels, open blockers, and linked PRs:

  ```sh
  gh api graphql -f query='{repository(owner:"IliasAlmerekov",name:"aegis-shellguard"){milestone(number:1){issues(states:OPEN,first:100){nodes{number title labels(first:10){nodes{name}} blockedBy(first:20){nodes{number state}} closedByPullRequestsReferences(first:5,includeClosedPrs:true){nodes{number state}}}}}}}' --jq '.data.repository.milestone.issues.nodes[] | "#\(.number) [\([.labels.nodes[].name] | join(","))] blocked-by:\([.blockedBy.nodes[] | select(.state == "OPEN") | .number] | join(",")) prs:\([.closedByPullRequestsReferences.nodes[] | "\(.number)/\(.state)"] | join(",")) \(.title)"'
  ```

Issues and specs for this repo live as GitHub issues. Use the `gh` CLI for all operations.

## Conventions

- **Create an issue**: `gh issue create --title "..." --body-file <file>`. Write a multi-line body to a file first and pass it with `--body-file` (or `-F` for comments); a Bash call that nests heredocs can be blocked whole.
- **Read an issue**: `gh issue view <number> --json number,title,body,labels,comments` (always pass `--json`; bare `gh issue view` fails on the deprecated `projectCards` GraphQL field).
- **List issues**: `gh issue list --state open --json number,title,body,labels,comments --jq '[.[] | {number, title, body, labels: [.labels[].name], comments: [.comments[].body]}]'` with appropriate `--label` and `--state` filters.
- **Make an issue a sub-issue of a parent**: `gh issue create --parent <parent> ...`, or `gh issue edit <parent> --add-sub-issue <child>` afterwards (`gh` 2.94+). Older `gh`: `gh api --method POST repos/<owner>/<repo>/issues/<parent>/sub_issues -F sub_issue_id=<child-db-id>` (database id, as in **Blocking** below). Without sub-issues, put `Part of #<parent>` at the top of the child body.
- **Comment on an issue**: `gh issue comment <number> --body "..."`
- **Apply / remove labels**: `gh issue edit <number> --add-label "..."` / `--remove-label "..."`
- **Close**: `gh issue close <number> --comment "..."`


## Pull requests as a triage surface

**PRs as a request surface: no.** _(Set to `yes` if this repo treats external PRs as feature requests; `/triage` reads this flag.)_

When set to `yes`, PRs run through the same labels and states as issues, using the `gh pr` equivalents:

- **Read a PR**: `gh pr view <number> --comments` and `gh pr diff <number>` for the diff.
- **List external PRs for triage**: `gh api --paginate 'repos/{owner}/{repo}/pulls?state=open' --jq '.[] | select(.author_association | IN("OWNER","MEMBER","COLLABORATOR") | not) | {number, title, author: .user.login, author_association, labels: [.labels[].name]}'`.
- **Comment / label / close**: `gh pr comment`, `gh pr edit --add-label`/`--remove-label`, `gh pr close`.

GitHub shares one number space across issues and PRs, so a bare `#42` may be either: resolve with `gh pr view 42` and fall back to `gh issue view 42`.

## When a skill says "publish to the issue tracker"

Create a GitHub issue.

## When a skill says "fetch the relevant ticket"

Read it as in **Read an issue** above.

## Wayfinding operations

Used by `/wayfinder`. The **map** is a single issue with **child** issues as tickets.

- **Map**: a single issue labelled `wayfinder:map`, holding the Notes / Decisions-so-far / Fog body. `gh issue create --label wayfinder:map`.
- **Child ticket**: an issue linked to the map as a GitHub sub-issue (see **Make an issue a sub-issue of a parent**). Where sub-issues aren't enabled, add the child to a task list in the map body and put `Part of #<map>` at the top of the child body. Labels: `wayfinder:<type>` (`research`/`prototype`/`grilling`/`task`). Once claimed, the ticket is assigned to the driving dev.
- **Blocking**: GitHub's **native issue dependencies**, the canonical, UI-visible representation. Add an edge with `gh api --method POST repos/<owner>/<repo>/issues/<child>/dependencies/blocked_by -F issue_id=<blocker-db-id>`, where `<blocker-db-id>` is the blocker's numeric **database id** (`gh api repos/<owner>/<repo>/issues/<n> --jq .id`, _not_ the `#number` or `node_id`). GitHub reports `issue_dependencies_summary.blocked_by` (open blockers only, the live gate). Where dependencies aren't available, fall back to a `Blocked by: #<n>, #<n>` line at the top of the child body. A ticket is unblocked when every blocker is closed.
- **Frontier query**: list the map's open children (`gh issue list --state open`, scoped to the map's sub-issues / task list), drop any with an open blocker (`issue_dependencies_summary.blocked_by > 0`, or an open issue in the `Blocked by` line) or an assignee; first in map order wins.
- **Claim**: `gh issue edit <n> --add-assignee @me`, the session's first write.
- **Resolve**: `gh issue comment <n> --body "<answer>"`, then `gh issue close <n>`, then append a context pointer (gist + link) to the map's Decisions-so-far.
