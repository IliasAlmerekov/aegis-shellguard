#![expect(clippy::panic, reason = "test helpers fail loudly")]
use aegis_language::manifest::{
    GrammarEntry, ManifestError, RELEASE_GRAMMARS, SUPPORTED_LANGUAGE_ABI, validate_entry,
};

fn well_formed(language: &'static str) -> GrammarEntry {
    GrammarEntry {
        language,
        crate_name: "tree-sitter-fixture",
        version: "0.1.0",
        upstream: "https://example.invalid/tree-sitter-fixture",
        license_spdx: "MIT",
        abi: SUPPORTED_LANGUAGE_ABI,
    }
}

#[test]
fn rejects_an_unpinned_grammar_with_empty_version() {
    let mut entry = well_formed("python");
    entry.version = "";
    assert_eq!(
        validate_entry(&entry, RELEASE_GRAMMARS),
        Err(ManifestError::Unpinned { language: "python" })
    );
}

#[test]
fn rejects_a_wildcard_version_as_unpinned() {
    let mut entry = well_formed("python");
    entry.version = "*";
    assert_eq!(
        validate_entry(&entry, RELEASE_GRAMMARS),
        Err(ManifestError::Unpinned { language: "python" })
    );
}

#[test]
fn rejects_a_grammar_missing_license_evidence() {
    let mut entry = well_formed("python");
    entry.license_spdx = "";
    assert_eq!(
        validate_entry(&entry, RELEASE_GRAMMARS),
        Err(ManifestError::MissingLicense { language: "python" })
    );
}

#[test]
fn rejects_a_grammar_with_unsupported_tree_sitter_abi() {
    let mut entry = well_formed("python");
    entry.abi = 99;
    assert_eq!(
        validate_entry(&entry, RELEASE_GRAMMARS),
        Err(ManifestError::UnsupportedAbi {
            language: "python",
            abi: 99
        })
    );
}

#[test]
fn accepts_a_grammar_at_a_backwards_compatible_abi() {
    use aegis_language::manifest::MIN_SUPPORTED_LANGUAGE_ABI;
    let mut entry = well_formed("python");
    entry.abi = MIN_SUPPORTED_LANGUAGE_ABI + 1;
    assert_eq!(
        validate_entry(&entry, RELEASE_GRAMMARS),
        Ok(()),
        "a grammar at a backwards-compatible ABI must pass the contract"
    );
}

#[test]
fn rejects_a_grammar_below_the_minimum_compatible_abi() {
    use aegis_language::manifest::MIN_SUPPORTED_LANGUAGE_ABI;
    let mut entry = well_formed("python");
    entry.abi = MIN_SUPPORTED_LANGUAGE_ABI - 1;
    assert_eq!(
        validate_entry(&entry, RELEASE_GRAMMARS),
        Err(ManifestError::UnsupportedAbi {
            language: "python",
            abi: MIN_SUPPORTED_LANGUAGE_ABI - 1
        })
    );
}

#[test]
fn rejects_a_grammar_absent_from_the_release_manifest() {
    let entry = well_formed("ruby");
    assert_eq!(
        validate_entry(&entry, RELEASE_GRAMMARS),
        Err(ManifestError::AbsentFromRelease { language: "ruby" })
    );
}

#[test]
fn accepts_every_well_formed_release_grammar() {
    for &language in RELEASE_GRAMMARS {
        let entry = well_formed(language);
        assert_eq!(
            validate_entry(&entry, RELEASE_GRAMMARS),
            Ok(()),
            "release grammar `{language}` should pass the qualification contract"
        );
    }
}

#[test]
fn release_grammar_set_matches_adr022_l1_foundation() {
    assert_eq!(
        RELEASE_GRAMMARS,
        &["python", "javascript", "typescript", "bash"]
    );
}

#[test]
fn release_target_set_matches_adr022_release_matrix() {
    use aegis_language::manifest::RELEASE_TARGETS;
    assert_eq!(
        RELEASE_TARGETS,
        &[
            "x86_64-unknown-linux-musl",
            "aarch64-unknown-linux-musl",
            "x86_64-apple-darwin",
            "aarch64-apple-darwin",
        ]
    );
}

#[cfg(feature = "grammars")]
use aegis_language::{SourceLanguage, parse};

#[cfg(feature = "grammars")]
#[test]
fn each_release_grammar_parses_a_trivial_snippet_on_host() {
    let cases = [
        (SourceLanguage::Python, "x = 1\n", "module"),
        (SourceLanguage::JavaScript, "let x = 1;\n", "program"),
        (
            SourceLanguage::TypeScript,
            "let x: number = 1;\n",
            "program",
        ),
        (SourceLanguage::Bash, "echo hi\n", "program"),
    ];
    for (lang, source, expected_root) in cases {
        let tree = parse(lang, source)
            .unwrap_or_else(|err| panic!("parse({lang:?}, {source:?}) failed: {err}"));
        let root = tree.root_node();
        assert!(
            !root.has_error(),
            "release grammar `{lang:?}` produced an error tree for {source:?}"
        );
        assert_eq!(
            root.kind(),
            expected_root,
            "release grammar `{lang:?}` root kind for {source:?}"
        );
    }
}

#[test]
fn builtin_manifest_satisfies_the_release_contract() {
    use aegis_language::manifest::{BUILTIN_MANIFEST, validate_manifest};
    validate_manifest(BUILTIN_MANIFEST, RELEASE_GRAMMARS)
        .expect("the built-in release manifest must satisfy the qualification contract");
}

#[cfg(feature = "grammars")]
#[test]
fn builtin_manifest_abi_matches_the_live_tree_sitter_runtime() {
    use aegis_language::manifest::BUILTIN_MANIFEST;

    for entry in BUILTIN_MANIFEST {
        let lang = SourceLanguage::from_id(entry.language).unwrap_or_else(|| {
            panic!(
                "manifest language `{}` is not a wired SourceLanguage",
                entry.language
            )
        });
        let runtime_abi = lang.tree_sitter_language().abi_version();
        assert_eq!(
            entry.abi, runtime_abi as u32,
            "manifest ABI for `{}` ({}) must match the live grammar ABI ({})",
            entry.language, entry.abi, runtime_abi,
        );
    }
}

use std::fs;
use std::process::Command;

fn cargo_lock() -> String {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock");
    fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()))
}

fn locked_versions(name: &str) -> Vec<String> {
    let mut versions = Vec::new();
    for block in cargo_lock().split("\n[[package]]") {
        let Some(block_name) = block
            .lines()
            .find_map(|l| l.strip_prefix("name = ").map(str::trim))
            .map(|n| n.trim_matches('"'))
        else {
            continue;
        };
        if block_name != name {
            continue;
        }
        if let Some(version_line) = block.lines().find(|l| l.starts_with("version = ")) {
            let v = version_line
                .trim_start_matches("version = ")
                .trim_matches('"')
                .to_owned();
            versions.push(v);
        }
    }
    versions
}

#[test]
fn builtin_manifest_versions_match_cargo_lock_pins() {
    use aegis_language::manifest::BUILTIN_MANIFEST;

    for entry in BUILTIN_MANIFEST {
        let locked = locked_versions(entry.crate_name);
        assert!(
            !locked.is_empty(),
            "Cargo.lock has no `[[package]]` entry for `{}` — the grammar crate is \
             not actually pinned in the lockfile",
            entry.crate_name,
        );
        assert_eq!(
            locked.len(),
            1,
            "Cargo.lock pins `{}` to {} versions ({:?}); cargo-deny should ban the \
             duplicate — the manifest records a single pin",
            entry.crate_name,
            locked.len(),
            locked,
        );
        assert_eq!(
            locked[0], entry.version,
            "manifest version `{}` for `{}` does not match the Cargo.lock pin `{}` — \
             the manifest must reflect the real lockfile pin, not a caret range",
            entry.version, entry.crate_name, locked[0],
        );
    }
}

#[test]
fn builtin_manifest_versions_match_exact_cargo_metadata_requirements() {
    let workspace_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new(env!("CARGO"))
        .current_dir(&workspace_root)
        .args(["metadata", "--locked", "--format-version=1", "--no-deps"])
        .output()
        .unwrap_or_else(|err| panic!("cargo metadata should run: {err}"));
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata = String::from_utf8(output.stdout)
        .unwrap_or_else(|err| panic!("cargo metadata emitted invalid UTF-8: {err}"));
    let package_start = metadata
        .find(r#""name":"aegis-language""#)
        .unwrap_or_else(|| panic!("cargo metadata must contain the aegis-language package"));
    let package = &metadata[package_start..];

    for entry in aegis_language::manifest::BUILTIN_MANIFEST {
        let needle = format!(r#""name":"{}","source":"#, entry.crate_name);
        let dependency_start = package.find(&needle).unwrap_or_else(|| {
            panic!(
                "cargo metadata must list grammar dependency `{}`",
                entry.crate_name
            )
        });
        let dependency = &package[dependency_start..];
        let exact_requirement = format!(r#""req":"={}""#, entry.version);
        assert!(
            dependency.starts_with(&format!(r#""name":"{}","source":"#, entry.crate_name))
                && dependency.contains(&exact_requirement),
            "cargo metadata must require `{}` exactly at {}; dependency: {}",
            entry.crate_name,
            entry.version,
            dependency.lines().next().unwrap_or(dependency),
        );
    }
}

#[test]
fn builtin_manifest_provenance_is_complete() {
    use aegis_language::manifest::BUILTIN_MANIFEST;

    for entry in BUILTIN_MANIFEST {
        assert!(
            !entry.crate_name.is_empty(),
            "grammar `{}` has an empty crate_name",
            entry.language,
        );
        assert!(
            entry.crate_name.starts_with("tree-sitter-"),
            "grammar `{}` crate_name `{}` must be a `tree-sitter-*` grammar crate",
            entry.language,
            entry.crate_name,
        );
        assert!(
            !entry.version.is_empty(),
            "grammar `{}` has an empty version",
            entry.language,
        );
        assert!(
            entry.upstream.starts_with("https://"),
            "grammar `{}` upstream `{}` must be an https URL",
            entry.language,
            entry.upstream,
        );
        assert!(
            !entry.license_spdx.is_empty(),
            "grammar `{}` has no SPDX license evidence",
            entry.language,
        );
        assert!(
            RELEASE_GRAMMARS.contains(&entry.language),
            "grammar `{}` is not in the release set {:?}",
            entry.language,
            RELEASE_GRAMMARS,
        );
    }
}
