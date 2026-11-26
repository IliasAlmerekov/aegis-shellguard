use super::*;
use crate::busy::is_executable_busy;
use std::fs;
use std::io;
use std::process::Command as StdCommand;
use std::time::Duration;
use tempfile::TempDir;

fn write_mock_docker(dir: &std::path::Path, script: &str) -> std::path::PathBuf {
    let path = dir.join("docker");
    let temp_path = dir.join("docker.tmp");
    fs::write(&temp_path, format!("#!/bin/sh\n{script}")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temp_path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    fs::rename(&temp_path, &path).unwrap();
    path
}

#[test]
fn write_mock_docker_rewrite_keeps_executable_and_updates_contents() {
    let dir = TempDir::new().unwrap();
    let path = write_mock_docker(dir.path(), "sleep 1\n");

    write_mock_docker(dir.path(), "printf 'updated\\n'\n");

    let output = output_with_etxtbsy_retry(&path).unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "updated\n");
}

fn output_with_etxtbsy_retry(path: &std::path::Path) -> io::Result<std::process::Output> {
    const ATTEMPTS: usize = 20;
    const DELAY_MS: u64 = 10;

    for attempt in 0..ATTEMPTS {
        match StdCommand::new(path).output() {
            Ok(output) => return Ok(output),
            Err(error) if is_executable_busy(&error) && attempt + 1 < ATTEMPTS => {
                std::thread::sleep(Duration::from_millis(DELAY_MS));
            }
            Err(error) => return Err(error),
        }
    }

    unreachable!("retry loop must return or error")
}

fn single_quote_for_shell(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\'', r"'\''")
}

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

const MINIMAL_INSPECT: &str = r#"[{"Name":"/test-ctr","Config":{"Labels":{}},"HostConfig":{"Binds":null,"PortBindings":{},"NetworkMode":"bridge","RestartPolicy":{"Name":"no"}}}]"#;

const RICH_INSPECT: &str = r#"[{"Name":"/web","Config":{"Labels":{"app":"frontend"}},"HostConfig":{"Binds":["/data:/app/data"],"PortBindings":{"80/tcp":[{"HostIp":"","HostPort":"8080"}]},"NetworkMode":"my-net","RestartPolicy":{"Name":"always"}}}]"#;

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

mod delete_tests;
mod liveness_tests;
mod readiness_tests;
mod rollback_tests;
mod snapshot_tests;
