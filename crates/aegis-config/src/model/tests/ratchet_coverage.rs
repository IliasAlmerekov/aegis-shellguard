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
            if sub_schema.get("deprecated").and_then(Value::as_bool) == Some(true) {
                continue;
            }
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
