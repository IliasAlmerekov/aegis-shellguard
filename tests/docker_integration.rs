#![expect(clippy::expect_used, reason = "test helpers fail loudly")]
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use aegis_config::{DockerScope, DockerScopeMode};
use aegis_snapshot::{DockerPlugin, SnapshotPlugin};

fn docker_available() -> bool {
    if std::env::var("AEGIS_DOCKER_TESTS").is_err() {
        return false;
    }
    std::process::Command::new("docker")
        .args(["ps", "-q"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

macro_rules! require_docker {
    () => {
        if !docker_available() {
            eprintln!(
                "skipping: set AEGIS_DOCKER_TESTS=1 and start the Docker daemon to run this test"
            );
            return;
        }
    };
}

fn unique_name(label: &str) -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("aegis-itest-{label}-{ts}")
}

struct ContainerGuard(String);

impl Drop for ContainerGuard {
    fn drop(&mut self) {
        let _ = std::process::Command::new("docker")
            .args(["stop", "-t", "1", &self.0])
            .output();
        let _ = std::process::Command::new("docker")
            .args(["rm", "-f", &self.0])
            .output();
    }
}

async fn start_alpine(name: &str, extra_args: &[&str]) -> String {
    let mut args = vec!["run", "-d", "--name", name];
    args.extend_from_slice(extra_args);
    args.extend(["alpine", "sleep", "3600"]);

    let out = tokio::process::Command::new("docker")
        .args(&args)
        .output()
        .await
        .expect("failed to run docker run");

    assert!(
        out.status.success(),
        "docker run failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

async fn exec_in(container: &str, cmd: &str) -> bool {
    tokio::process::Command::new("docker")
        .args(["exec", container, "sh", "-c", cmd])
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

async fn inspect_field(container: &str, template: &str) -> String {
    let out = tokio::process::Command::new("docker")
        .args(["inspect", "--format", template, container])
        .output()
        .await
        .expect("docker inspect failed");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[tokio::test]
async fn is_applicable_true_when_container_running() {
    require_docker!();

    let name = unique_name("applicable");
    let _guard = ContainerGuard(name.clone());
    start_alpine(&name, &[]).await;

    assert!(
        DockerPlugin::new().is_applicable(Path::new("/")).await,
        "is_applicable must return true when a container is running"
    );
}

#[tokio::test]
async fn snapshot_parses_real_inspect_output() {
    require_docker!();

    let name = unique_name("inspect");
    let _guard = ContainerGuard(name.clone());
    start_alpine(&name, &[]).await;

    let plugin = DockerPlugin::new();
    let snapshot_id = plugin
        .snapshot(Path::new("/"), "docker rm -f test")
        .await
        .expect("snapshot must succeed against a real container");

    assert_ne!(snapshot_id, "none", "snapshot_id must not be the sentinel");

    for line in snapshot_id.lines() {
        let v: serde_json::Value =
            serde_json::from_str(line).expect("each snapshot_id line must be valid JSON");
        assert!(
            v["container_id"].is_string(),
            "record must have container_id"
        );
        assert!(v["image"].is_string(), "record must have image");
        assert!(v["config"].is_object(), "record must have config object");
        assert!(
            v["config"]["network_mode"].is_string(),
            "config must have network_mode"
        );
        assert!(
            v["config"]["restart_policy"].is_string(),
            "config must have restart_policy"
        );
    }
}

#[tokio::test]
async fn snapshot_rollback_reverts_filesystem_change() {
    require_docker!();

    let name = unique_name("lifecycle");
    let _guard = ContainerGuard(name.clone());

    start_alpine(&name, &["--label", "aegis.snapshot=true"]).await;

    let plugin = DockerPlugin::new();

    let snapshot_id = plugin
        .snapshot(Path::new("/"), "docker rm -f test")
        .await
        .expect("snapshot must succeed");

    assert!(
        exec_in(&name, "echo aegis_marker > /aegis_marker_file").await,
        "docker exec must succeed"
    );
    assert!(
        exec_in(&name, "test -f /aegis_marker_file").await,
        "marker must exist before rollback"
    );

    plugin
        .rollback(&snapshot_id)
        .await
        .expect("rollback must succeed");

    assert!(
        !exec_in(&name, "test -f /aegis_marker_file").await,
        "marker file must NOT exist in the rolled-back container"
    );
}

#[tokio::test]
async fn snapshot_rollback_preserves_container_name() {
    require_docker!();

    let name = unique_name("named");
    let _guard = ContainerGuard(name.clone());

    start_alpine(&name, &[]).await;

    let plugin = DockerPlugin::new();
    let snapshot_id = plugin
        .snapshot(Path::new("/"), "docker stop test")
        .await
        .expect("snapshot must succeed");

    plugin
        .rollback(&snapshot_id)
        .await
        .expect("rollback must succeed");

    let actual_name = inspect_field(&name, "{{.Name}}").await;
    assert_eq!(
        actual_name,
        format!("/{name}"),
        "rolled-back container must have the original name"
    );
}

#[tokio::test]
async fn snapshot_rollback_preserves_labels() {
    require_docker!();

    let name = unique_name("labeled");
    let _guard = ContainerGuard(name.clone());

    start_alpine(&name, &["--label", "aegis-test=true", "--label", "env=ci"]).await;

    let plugin = DockerPlugin::new();
    let snapshot_id = plugin
        .snapshot(Path::new("/"), "docker stop test")
        .await
        .expect("snapshot must succeed");

    plugin
        .rollback(&snapshot_id)
        .await
        .expect("rollback must succeed");

    let label_val = inspect_field(&name, "{{index .Config.Labels \"aegis-test\"}}").await;
    assert_eq!(
        label_val, "true",
        "rolled-back container must carry the original label"
    );

    let env_label = inspect_field(&name, "{{index .Config.Labels \"env\"}}").await;
    assert_eq!(
        env_label, "ci",
        "rolled-back container must carry all original labels"
    );
}

#[tokio::test]
async fn snapshot_rollback_preserves_restart_policy() {
    require_docker!();

    let name = unique_name("restart");
    let _guard = ContainerGuard(name.clone());

    start_alpine(&name, &["--restart", "on-failure"]).await;

    let plugin = DockerPlugin::new();
    let snapshot_id = plugin
        .snapshot(Path::new("/"), "docker stop test")
        .await
        .expect("snapshot must succeed");

    plugin
        .rollback(&snapshot_id)
        .await
        .expect("rollback must succeed");

    let policy = inspect_field(&name, "{{.HostConfig.RestartPolicy.Name}}").await;
    assert_eq!(
        policy, "on-failure",
        "rolled-back container must have the original restart policy"
    );
}

#[tokio::test]
async fn rollback_succeeds_when_original_container_already_removed() {
    require_docker!();

    let name = unique_name("preremoved");
    let _guard = ContainerGuard(name.clone());

    let container_id = start_alpine(&name, &[]).await;

    let plugin = DockerPlugin::new();
    let snapshot_id = plugin
        .snapshot(Path::new("/"), "docker rm -f test")
        .await
        .expect("snapshot must succeed");

    let rm_out = tokio::process::Command::new("docker")
        .args(["rm", "-f", &container_id])
        .output()
        .await
        .unwrap();
    assert!(rm_out.status.success(), "pre-removal must succeed");

    plugin
        .rollback(&snapshot_id)
        .await
        .expect("rollback must succeed even when original container was already removed");

    let running = inspect_field(&name, "{{.State.Running}}").await;
    assert_eq!(running, "true", "rolled-back container must be running");
}

#[tokio::test]
async fn snapshot_rollback_preserves_port_binding() {
    require_docker!();

    let name = unique_name("ported");
    let _guard = ContainerGuard(name.clone());

    start_alpine(&name, &["-p", "127.0.0.1:41980:80"]).await;

    let plugin = DockerPlugin::new();
    let snapshot_id = plugin
        .snapshot(Path::new("/"), "docker stop test")
        .await
        .expect("snapshot must succeed");

    plugin
        .rollback(&snapshot_id)
        .await
        .expect("rollback must succeed");

    let host_port = inspect_field(
        &name,
        "{{(index (index .HostConfig.PortBindings \"80/tcp\") 0).HostPort}}",
    )
    .await;
    assert_eq!(
        host_port, "41980",
        "rolled-back container must have the original port binding"
    );
}

#[tokio::test]
async fn snapshot_default_labeled_scope_selects_only_opted_in_containers() {
    require_docker!();

    let labeled_name = unique_name("labeled");
    let unlabeled_name = unique_name("unlabeled");
    let _labeled_guard = ContainerGuard(labeled_name.clone());
    let _unlabeled_guard = ContainerGuard(unlabeled_name.clone());

    start_alpine(&labeled_name, &["--label", "aegis.snapshot=true"]).await;
    start_alpine(&unlabeled_name, &[]).await;

    let plugin = DockerPlugin::new();
    let snapshot_id = plugin
        .snapshot(Path::new("/"), "docker stop test")
        .await
        .expect("snapshot must succeed");

    let records: Vec<serde_json::Value> = snapshot_id
        .lines()
        .map(|line| serde_json::from_str(line).expect("snapshot line must be valid json"))
        .collect();

    assert_eq!(
        records.len(),
        1,
        "only labeled container should be snapshotted"
    );
    assert_eq!(
        records[0]["config"]["name"].as_str(),
        Some(labeled_name.as_str())
    );
}

#[tokio::test]
async fn snapshot_name_scope_selects_only_matching_container_names() {
    require_docker!();

    let selected_name = unique_name("selected");
    let skipped_name = unique_name("skipped");
    let _selected_guard = ContainerGuard(selected_name.clone());
    let _skipped_guard = ContainerGuard(skipped_name.clone());

    start_alpine(&selected_name, &[]).await;
    start_alpine(&skipped_name, &[]).await;

    let plugin = DockerPlugin::new().with_scope(DockerScope {
        mode: DockerScopeMode::Names,
        label: "aegis.snapshot".to_string(),
        name_patterns: vec![selected_name.clone()],
    });
    let snapshot_id = plugin
        .snapshot(Path::new("/"), "docker stop test")
        .await
        .expect("snapshot must succeed");

    let records: Vec<serde_json::Value> = snapshot_id
        .lines()
        .map(|line| serde_json::from_str(line).expect("snapshot line must be valid json"))
        .collect();

    assert_eq!(records.len(), 1, "only matching name should be snapshotted");
    assert_eq!(
        records[0]["config"]["name"].as_str(),
        Some(selected_name.as_str())
    );
}
