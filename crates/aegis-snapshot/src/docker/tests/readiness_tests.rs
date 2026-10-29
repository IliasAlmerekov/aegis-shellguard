use super::*;
use aegis_types::{SnapshotAttemptReadiness as R, StructuralCheck as S, ValidationLevel as V};

async fn triple(plugin: &DockerPlugin, id: &str) -> (R, V, S) {
    let outcome = plugin.readiness(id).await;
    (
        outcome.readiness,
        outcome.validation_level,
        outcome.structural_check,
    )
}

/// Mock docker whose `image inspect <name>` succeeds only for `name == live`.
fn mock_with_live_image(dir: &std::path::Path, live: &str) {
    write_mock_docker(
        dir,
        &format!(
            r#"if [ "$1 $2" = "image inspect" ] && [ "$3" = "{live}" ]; then exit 0; fi
exit 1"#
        ),
    );
}

#[tokio::test]
async fn an_existing_image_is_present_with_no_structural_check() {
    let dir = TempDir::new().unwrap();
    mock_with_live_image(dir.path(), "snap-a");
    let id = minimal_record("a", "snap-a");

    assert_eq!(
        triple(&plugin(&dir.path().join("docker")), &id).await,
        (R::Ready, V::PresenceOnly, S::NotSupported)
    );
}

#[tokio::test]
async fn a_missing_image_is_unavailable() {
    let dir = TempDir::new().unwrap();
    mock_with_live_image(dir.path(), "snap-other");
    let id = minimal_record("a", "snap-a");

    assert_eq!(
        triple(&plugin(&dir.path().join("docker")), &id).await,
        (R::Unavailable, V::PresenceOnly, S::NotRun)
    );
}

#[tokio::test]
async fn an_unrunnable_docker_binary_is_unavailable() {
    let dir = TempDir::new().unwrap();
    let id = minimal_record("a", "snap-a");

    assert_eq!(
        triple(&plugin(&dir.path().join("missing-docker")), &id).await,
        (R::Unavailable, V::PresenceOnly, S::NotRun)
    );
}

#[tokio::test]
async fn the_none_sentinel_met_the_obligation_with_nothing_to_capture() {
    assert_eq!(
        triple(&DockerPlugin::new(), "none").await,
        (R::Ready, V::PresenceOnly, S::NotSupported)
    );
}

#[tokio::test]
async fn a_malformed_record_is_unavailable() {
    assert_eq!(
        triple(&DockerPlugin::new(), "not json").await,
        (R::Unavailable, V::PresenceOnly, S::NotRun)
    );
}
