//! `RuntimeContext::new` built from a config loaded through
//! `AegisConfig::load_for` must attribute a scanner failure to the roots that
//! config was actually resolved against, not the process's real `cwd`/`HOME`
//! (issue #400).

use std::fs;

use tempfile::TempDir;

#[test]
fn invalid_layered_pattern_attributes_the_load_for_roots_not_the_process_env() {
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();

    let global_dir = home.path().join(".config").join("aegis");
    fs::create_dir_all(&global_dir).unwrap();
    fs::write(
        global_dir.join("config.toml"),
        r#"
[[custom_patterns]]
id = "USR-DUP"
category = "Filesystem"
risk = "Warn"
pattern = "custom-alpha-only-pattern"
description = "global entry"
"#,
    )
    .unwrap();

    let project_config_path = workspace.path().join(".aegis.toml");
    fs::write(
        &project_config_path,
        r#"
[[custom_patterns]]
id = "USR-DUP"
category = "Filesystem"
risk = "Warn"
pattern = "custom-beta-only-pattern"
description = "project entry"
"#,
    )
    .unwrap();

    let config = aegis::config::AegisConfig::load_for(workspace.path(), Some(home.path()))
        .expect("layered load with only a duplicate-id conflict must still parse");

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let handle = rt.handle().clone();

    let err = match aegis::runtime::RuntimeContext::new(config, handle) {
        Ok(_) => panic!("a duplicate pattern id across layers must abort construction"),
        Err(err) => err,
    };

    let message = err.to_string();
    assert!(
        message.contains(&project_config_path.display().to_string()),
        "error must attribute the pattern to the project config file this config was \
         actually loaded from ({}), not the process's real cwd/HOME: {message}",
        project_config_path.display()
    );
}
