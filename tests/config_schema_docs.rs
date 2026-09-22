use std::fs;
use std::path::PathBuf;

use aegis_config::PruneConfig;

fn repo_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

/// Field names declared on `PruneConfig`, read from its generated JSON
/// schema rather than hard-coded — a field renamed or added there is
/// renamed or added here too, so it can't silently drop out of the docs.
fn prune_config_field_names() -> Vec<String> {
    let schema =
        serde_json::to_value(schemars::schema_for!(PruneConfig)).expect("schema serializes");
    schema
        .get("properties")
        .and_then(serde_json::Value::as_object)
        .expect("PruneConfig schema has properties")
        .keys()
        .cloned()
        .collect()
}

#[test]
fn config_schema_doc_exists_and_describes_versioning_and_migration() {
    let path = repo_path("docs/config-schema.md");
    assert!(
        path.exists(),
        "docs/config-schema.md must exist to document schema evolution"
    );

    let contents = fs::read_to_string(&path).unwrap_or_default();
    for needle in [
        "config_version",
        "schema evolution",
        "allowlist = [",
        "[[allow]]",
        "cwd or user scope",
        "readable for migration, invalid for runtime",
        "mode semantics",
        "postgres_snapshot",
        "mysql_snapshot",
        "sqlite_snapshot_path",
        "supabase_snapshot",
        "auto_snapshot_supabase",
        "project_ref",
        "require_config_target_match_on_rollback",
        "db-only manifest snapshot",
        "auto_snapshot_postgres",
        "auto_snapshot_mysql",
        "auto_snapshot_sqlite",
        "PGPASSWORD",
        "MYSQL_PWD",
        "deprecated fields",
        "migration",
    ] {
        assert!(
            contents.contains(needle),
            "config schema doc must mention `{needle}`; contents:\n{contents}"
        );
    }
}

#[test]
fn config_schema_doc_documents_every_prune_field() {
    let path = repo_path("docs/config-schema.md");
    let contents = fs::read_to_string(&path).unwrap_or_default();

    for field in prune_config_field_names() {
        let needle = format!("prune.{field}");
        assert!(
            contents.contains(&needle),
            "config schema doc must document `{needle}` (derived from PruneConfig)"
        );
    }
}

#[test]
fn readme_links_to_config_schema_policy() {
    let readme = fs::read_to_string(repo_path("README.md")).unwrap();
    assert!(
        readme.contains("[Config schema](docs/config-schema.md)"),
        "README must link to the explicit config-schema policy"
    );
}
