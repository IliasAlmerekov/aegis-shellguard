//! Every field of `AegisConfig` (and its nested config structs) must run
//! through a declared `Ratchet` direction — ADR-013, acceptance criterion
//! "adding a field without a direction fails the build or a test". The
//! exhaustive destructure in `model::merge_layer` gives the compile-error
//! half of that guarantee; this test gives the other half, for the fields
//! whose direction is a `Custom` function rather than a field-by-field
//! destructure (which the compiler already checked).
//!
//! It runs one merge with a recording sink over the default config, collects
//! every field path a `Ratchet` direction touched, and diffs that set
//! against the leaf field paths of `AegisConfig`'s JSON schema. A `Custom`
//! group (`postgres_snapshot`, `mysql_snapshot`, `supabase_snapshot`,
//! `docker_scope`, `rules`) registers ONE path covering every schema leaf
//! under it, rather than one path per leaf — see the doc comment on
//! `custom_postgres_snapshot` and friends for why each of those groups is an
//! all-or-nothing rule, not independent per-field tightening.

use serde_json::Value;

use super::AegisConfig;
use super::partial::PartialConfig;
use crate::allowlist::ConfigSourceLayer;

fn resolve<'a>(node: &'a Value, defs: Option<&'a Value>) -> &'a Value {
    let Some(reference) = node.get("$ref").and_then(Value::as_str) else {
        return node;
    };
    let name = reference.rsplit('/').next().unwrap_or(reference);
    defs.and_then(|defs| defs.get(name)).unwrap_or(node)
}

/// Walk one JSON Schema node, appending a dotted path for every leaf
/// (a field with no `properties` of its own — scalars, enums, and arrays
/// all count as leaves; only nested objects recurse).
fn collect_leaf_paths(node: &Value, defs: Option<&Value>, prefix: &str, out: &mut Vec<String>) {
    let node = resolve(node, defs);

    if let Some([only]) = node
        .get("allOf")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
    {
        collect_leaf_paths(only, defs, prefix, out);
        return;
    }

    if let Some(properties) = node.get("properties").and_then(Value::as_object) {
        for (key, sub_schema) in properties {
            let path = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{prefix}.{key}")
            };
            collect_leaf_paths(sub_schema, defs, &path, out);
        }
        return;
    }

    if !prefix.is_empty() {
        out.push(prefix.to_string());
    }
}

fn schema_leaf_paths() -> Vec<String> {
    let schema = serde_json::to_value(schemars::schema_for!(AegisConfig))
        .expect("schema serializes to JSON");
    let defs = schema.get("$defs").or_else(|| schema.get("definitions"));
    let mut out = Vec::new();
    collect_leaf_paths(&schema, defs, "", &mut out);
    out
}

/// A declared path covers a schema leaf if it names that leaf exactly, or
/// names an ancestor object a `Custom` group merges as a whole (e.g.
/// `postgres_snapshot` covers `postgres_snapshot.database`).
fn covers(declared: &str, leaf: &str) -> bool {
    leaf == declared || leaf.starts_with(&format!("{declared}."))
}

#[test]
fn every_schema_field_has_a_declared_ratchet_direction() {
    let (_config, sink) = AegisConfig::merge_layer_for_coverage_test(
        AegisConfig::defaults(),
        PartialConfig::default(),
        ConfigSourceLayer::Project,
        "ratchet_coverage_test.aegis.toml",
    );

    let leaves = schema_leaf_paths();
    let declared = sink.touched;

    let missing: Vec<&String> = leaves
        .iter()
        .filter(|leaf| !declared.iter().any(|path| covers(path, leaf)))
        .collect();
    assert!(
        missing.is_empty(),
        "AegisConfig schema field(s) with no declared Ratchet direction: {missing:?}"
    );

    let extra: Vec<&&str> = declared
        .iter()
        .filter(|path| !leaves.iter().any(|leaf| covers(path, leaf)))
        .collect();
    assert!(
        extra.is_empty(),
        "declared Ratchet path(s) that don't match any AegisConfig schema field: {extra:?}"
    );
}
