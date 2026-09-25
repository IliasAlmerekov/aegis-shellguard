use super::*;

// Issue #431: `git push` forms that delete refs on the remote ran as Safe,
// while deleting a local branch with `git branch -D` gets a Warn (GIT-006).
fn ids(assessment: &Assessment) -> Vec<&str> {
    assessment
        .matched
        .iter()
        .map(|m| m.pattern.id.as_ref())
        .collect()
}

#[test]
fn git_push_forms_that_delete_remote_refs_warn() {
    let cases = [
        "git push origin --delete old-branch",
        "git push origin --delete a b c",
        "git push --delete origin old-branch",
        "git push origin -d old-branch",
        "git push -d origin old-branch",
        "git push origin :old-branch",
        "git push origin :refs/tags/v1.0",
        "git push origin main :old-branch",
        "git push --prune origin",
        "git push origin --prune refs/heads/*:refs/heads/*",
        "git push --mirror backup",
        // Git accepts unambiguous long-option prefixes and short-flag bundles.
        "git push origin --del old-branch",
        "git push --pru origin",
        "git push --mir backup",
        "git push -ud origin old-branch",
        "git push origin +:old-branch",
        // A later `--no-dry-run` turns the push back into a real one.
        "git push --dry-run --no-dry-run origin --delete old-branch",
        "git push -n --no-dry origin :old-branch",
        // Only the exact dry-run spellings suppress the rule, so an
        // abbreviation git would also accept still warns.
        "git push --dry origin --delete old-branch",
        // A push-option value is consumed, and the flag after it is real.
        "git push -o ci.skip -d origin old-branch",
        "git push --push-option ci.skip --delete origin old-branch",
        "git push --push-option=ci.skip -d origin old-branch",
    ];

    let s = scanner();
    let missed: Vec<&str> = cases
        .into_iter()
        .filter(|cmd| s.assess(cmd).risk < RiskLevel::Warn)
        .collect();
    assert!(missed.is_empty(), "expected at least Warn for: {missed:#?}");
}

#[test]
fn git_push_forms_that_do_not_delete_stay_safe() {
    let cases = [
        "git push",
        "git push origin main",
        "git push -u origin feature",
        "git push origin feature:feature",
        "git push origin HEAD:refs/heads/topic",
        "git push origin :",
        "git push --tags origin",
        "git push --dry-run origin main",
        "git push -oci.skip-deploy origin main",
        "git push --push-option=deploy origin main",
        // A dry run deletes nothing on the remote.
        "git push --dry-run origin --delete old-branch",
        "git push -n origin :old-branch",
        "git push -nd origin old-branch",
        "git push --delete --dry-run origin old-branch",
        // The value of `-o` / `--push-option` is data, not a flag.
        "git push -o -d origin main",
        "git push --push-option -d origin main",
        "git push -uo -d origin main",
    ];

    let s = scanner();
    for cmd in cases {
        let assessment = s.assess(cmd);
        assert_eq!(
            assessment.risk,
            RiskLevel::Safe,
            "command {cmd:?}: expected Safe, got {:?} ({:?})",
            assessment.risk,
            ids(&assessment),
        );
    }
}
