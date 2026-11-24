mod support;

use std::fs;

use support::repo_path;

const PUBLISHED_ADVISORIES: [&str; 7] = [
    "GHSA-mgwj-4828-3mrg",
    "GHSA-7gcj-4f7x-7fxj",
    "GHSA-c5pv-6qf7-w4cc",
    "GHSA-xj54-q8xw-w92r",
    "GHSA-399c-6p33-f6m8",
    "GHSA-7564-rmfj-ch4p",
    "GHSA-xprr-pwj3-gq85",
];

const ADVISORY_INDEX_WHY: &str = "an advisory without a named regression test can be reopened silently when its layer is moved or gated";

/// Distinct `GHSA-xxxx-xxxx-xxxx` ids found in `text`.
fn ghsa_ids(text: &str) -> Vec<String> {
    let mut ids = Vec::new();
    for (start, _) in text.match_indices("GHSA-") {
        let id: String = text[start..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect();
        if id.len() == 19 && !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

#[test]
fn security_advisory_index_names_a_real_regression_test_per_advisory() {
    let security = fs::read_to_string(repo_path("SECURITY.md")).unwrap();
    let index = security
        .split("## Published advisories")
        .nth(1)
        .unwrap_or_else(|| {
            panic!("SECURITY.md needs a `## Published advisories` index: {ADVISORY_INDEX_WHY}")
        });
    let index = index.split("\n## ").next().unwrap();

    let rows: Vec<Vec<&str>> = index
        .lines()
        .filter(|line| line.starts_with("| GHSA-"))
        .map(|line| line.trim_matches('|').split('|').map(str::trim).collect())
        .collect();

    for id in PUBLISHED_ADVISORIES {
        let row = rows.iter().find(|row| row[0] == id).unwrap_or_else(|| {
            panic!("{id} has no row in the SECURITY.md advisory index: {ADVISORY_INDEX_WHY}")
        });
        assert_eq!(
            row.len(),
            5,
            "{id} row needs GHSA, patched, ADR, layer, test"
        );

        for test in row[4].split(", ").map(|cell| cell.trim_matches('`')) {
            let (file, name) = test.split_once("::").unwrap_or_else(|| {
                panic!("{id} test cell must list `path::function` entries, got `{test}`")
            });
            let source = fs::read_to_string(repo_path(file))
                .unwrap_or_else(|_| panic!("{id} names test file `{file}` that does not exist"));
            assert!(
                source.contains(&format!("fn {name}(")),
                "{id} names test function `{name}` that does not exist in `{file}`: {ADVISORY_INDEX_WHY}"
            );
        }

        if row[2].starts_with("ADR-") {
            let prefix = format!("{}-", row[2].to_lowercase());
            let found = fs::read_dir(repo_path("docs/adr")).unwrap().any(|e| {
                e.unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(&prefix)
            });
            assert!(found, "{id} names {} but docs/adr has no such file", row[2]);
        }
    }

    let changelog = fs::read_to_string(repo_path("CHANGELOG.md")).unwrap();
    for id in ghsa_ids(&changelog) {
        assert!(
            rows.iter().any(|row| row[0] == id),
            "CHANGELOG.md links {id} but the SECURITY.md advisory index has no row for it: {ADVISORY_INDEX_WHY}"
        );
    }
}

#[test]
fn changelog_links_every_published_advisory() {
    let changelog = fs::read_to_string(repo_path("CHANGELOG.md")).unwrap();
    for id in PUBLISHED_ADVISORIES {
        let link = format!(
            "(https://github.com/IliasAlmerekov/aegis-shellguard/security/advisories/{id})"
        );
        assert!(changelog.contains(&link), "CHANGELOG.md must link {id}");
    }
}
