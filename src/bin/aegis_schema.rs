#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use std::fs;

use anyhow::Context as _;
use schemars::schema_for;

fn main() -> anyhow::Result<()> {
    let schema = schema_for!(aegis_config::AegisConfig);
    let json = serde_json::to_string_pretty(&schema).context("failed to serialize schema")?;
    fs::write("aegis-schema.json", json).context("failed to write aegis-schema.json")?;
    println!("Schema written to aegis-schema.json");
    Ok(())
}
