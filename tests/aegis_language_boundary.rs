mod support;
use support::assert_no_dep;

const OTHER_WORKSPACE_CRATES: &[&str] = &[
    "aegis-types",
    "aegis-parser",
    "aegis-scanner",
    "aegis-policy",
    "aegis-config",
    "aegis-explanation",
    "aegis-tui",
    "aegis-audit",
    "aegis-snapshot",
    "aegis-sandbox",
];

#[test]
fn no_workspace_crate_depends_on_aegis_language() {
    for &crate_name in OTHER_WORKSPACE_CRATES {
        assert_no_dep(crate_name, "aegis-language");
    }
}

#[test]
fn aegis_language_does_not_depend_on_any_workspace_crate() {
    for &forbidden in OTHER_WORKSPACE_CRATES {
        assert_no_dep("aegis-language", forbidden);
    }
}
