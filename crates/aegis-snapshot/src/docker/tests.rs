use super::*;
use std::fs;
use std::process::Command as StdCommand;
use tempfile::TempDir;

/// Write a shell script to `dir/docker` and make it executable.
fn write_mock_docker(dir: &std::path::Path, script: &str) -> std::path::PathBuf {
    let path = dir.join("docker");
    crate::test_support::write_executable(&path, &format!("#!/bin/sh\n{script}"));
    path
}

#[test]
fn write_mock_docker_rewrite_keeps_executable_and_updates_contents() {
    let dir = TempDir::new().unwrap();
    let path = write_mock_docker(dir.path(), "sleep 1\n");

    write_mock_docker(dir.path(), "printf 'updated\\n'\n");

    let output = StdCommand::new(&path).output().unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "updated\n");
}

fn single_quote_for_shell(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\'', r"'\''")
}

/// Helper that creates a plugin with `All` scope — no filtering.
/// Use `plugin_with_scope` to test specific scope behaviour.
fn plugin(bin: &std::path::Path) -> DockerPlugin {
    DockerPlugin {
        docker_bin: bin.to_string_lossy().into_owned(),
        scope: DockerScope {
            mode: DockerScopeMode::All,
            ..DockerScope::default()
        },
    }
}

fn plugin_with_scope(bin: &std::path::Path, scope: DockerScope) -> DockerPlugin {
    DockerPlugin {
        docker_bin: bin.to_string_lossy().into_owned(),
        scope,
    }
}

/// Minimal `docker inspect` JSON for a container with no special config.
const MINIMAL_INSPECT: &str = r#"[{"Name":"/test-ctr","Config":{"Labels":{}},"HostConfig":{"Binds":null,"PortBindings":{},"NetworkMode":"bridge","RestartPolicy":{"Name":"no"}}}]"#;

/// Richer inspect JSON covering ports, binds, network, restart, and labels.
const RICH_INSPECT: &str = r#"[{"Name":"/web","Config":{"Labels":{"app":"frontend"}},"HostConfig":{"Binds":["/data:/app/data"],"PortBindings":{"80/tcp":[{"HostIp":"","HostPort":"8080"}]},"NetworkMode":"my-net","RestartPolicy":{"Name":"always"}}}]"#;

/// Build a minimal ContainerRecord JSON string suitable for use as a snapshot_id line.
fn minimal_record(container_id: &str, image: &str) -> String {
    serde_json::to_string(&ContainerRecord {
        container_id: container_id.to_string(),
        image: image.to_string(),
        config: ContainerConfig {
            name: String::new(),
            binds: vec![],
            port_bindings: vec![],
            labels: HashMap::new(),
            network_mode: "bridge".to_string(),
            restart_policy: "no".to_string(),
        },
    })
    .unwrap()
}

// ── is_applicable ──────────────────────────────────────────────────────────

mod delete_tests;
mod rollback_tests;
mod snapshot_tests;
