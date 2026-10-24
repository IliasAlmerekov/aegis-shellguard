use super::*;

/// Mock docker whose `image inspect <name>` succeeds only for `name == live`.
fn mock_with_live_image(dir: &std::path::Path, live: &str) {
    write_mock_docker(
        dir,
        &format!(
            r#"if [ "$1 $2" = "image inspect" ] && [ "$3" = "{live}" ]; then exit 0; fi
printf "Error: No such image: $3\n" >&2
exit 1"#
        ),
    );
}

#[tokio::test]
async fn liveness_is_present_when_every_recorded_image_exists() {
    let dir = TempDir::new().unwrap();
    mock_with_live_image(dir.path(), "snap-a");
    let id = minimal_record("a", "snap-a");

    let outcome = plugin(&dir.path().join("docker"))
        .liveness(&id)
        .await
        .unwrap();

    assert_eq!(outcome, Liveness::Present);
}

#[tokio::test]
async fn liveness_is_absent_when_any_recorded_image_is_missing() {
    let dir = TempDir::new().unwrap();
    mock_with_live_image(dir.path(), "snap-a");
    let id = format!(
        "{}\n{}",
        minimal_record("a", "snap-a"),
        minimal_record("b", "snap-b")
    );

    let outcome = plugin(&dir.path().join("docker"))
        .liveness(&id)
        .await
        .unwrap();

    assert!(matches!(outcome, Liveness::Absent { .. }), "{outcome:?}");
}

#[tokio::test]
async fn liveness_is_inconclusive_when_the_daemon_does_not_answer() {
    let dir = TempDir::new().unwrap();
    write_mock_docker(
        dir.path(),
        r#"printf "Cannot connect to the Docker daemon at unix:///var/run/docker.sock\n" >&2; exit 1"#,
    );
    let id = minimal_record("a", "snap-a");

    let outcome = plugin(&dir.path().join("docker"))
        .liveness(&id)
        .await
        .unwrap();

    assert!(
        matches!(outcome, Liveness::Inconclusive { .. }),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn liveness_is_inconclusive_when_the_docker_binary_cannot_run() {
    let dir = TempDir::new().unwrap();
    let id = minimal_record("a", "snap-a");

    let outcome = plugin(&dir.path().join("missing-docker"))
        .liveness(&id)
        .await
        .unwrap();

    assert!(
        matches!(outcome, Liveness::Inconclusive { .. }),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn liveness_is_nothing_captured_for_the_none_sentinel() {
    let outcome = DockerPlugin::new().liveness("none").await.unwrap();

    assert_eq!(outcome, Liveness::NothingCaptured);
}

#[tokio::test]
async fn liveness_rejects_a_malformed_record() {
    assert!(DockerPlugin::new().liveness("not json").await.is_err());
}
