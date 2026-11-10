use super::tests::{DANGEROUS_CMD, commit_file, init_repo};
use super::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn build_snapshot_id_writes_v3_layout() {
    assert_eq!(
        build_snapshot_id(
            Path::new("/tmp/r"),
            "0123456789abcdef0123456789abcdef01234567"
        ),
        "v3:2f746d702f72:0123456789abcdef0123456789abcdef01234567"
    );
}

#[test]
fn parse_snapshot_id_reads_v3_and_legacy_to_the_same_target() {
    let hash = "0123456789abcdef0123456789abcdef01234567";
    for id in [
        "v3:2f746d702f72:0123456789abcdef0123456789abcdef01234567",
        "/tmp/r\t0123456789abcdef0123456789abcdef01234567",
    ] {
        assert_eq!(
            parse_snapshot_id(id).unwrap(),
            (PathBuf::from("/tmp/r"), hash),
            "{id}"
        );
    }
}

#[test]
fn parse_snapshot_id_keeps_colon_and_tab_in_a_v3_cwd() {
    let id = build_snapshot_id(Path::new("/tmp/a:b\tc"), "abc");

    assert_eq!(
        parse_snapshot_id(&id).unwrap(),
        (PathBuf::from("/tmp/a:b\tc"), "abc")
    );
}

#[test]
fn parse_snapshot_id_rejects_malformed_v3_without_falling_back_to_legacy() {
    for id in [
        "v3:2f746d702f72",
        "v3:2f:abc:def",
        "v3:zz:abc",
        "v3:abc:abc",
    ] {
        let error = parse_snapshot_id(id).unwrap_err().to_string();
        assert!(error.contains("malformed snapshot_id"), "{id}: {error}");
    }
}

#[tokio::test]
async fn rollback_accepts_a_legacy_tab_snapshot_id() {
    let dir = TempDir::new().unwrap();
    init_repo(dir.path()).await;
    commit_file(dir.path(), "tracked.txt", "base\n").await;
    fs::write(dir.path().join("tracked.txt"), "modified\n").unwrap();
    let snapshot_id = GitPlugin.snapshot(dir.path(), DANGEROUS_CMD).await.unwrap();
    let (cwd, hash) = parse_snapshot_id(&snapshot_id).unwrap();
    let legacy_id = format!("{}\t{hash}", cwd.display());
    fs::remove_file(dir.path().join("tracked.txt")).unwrap();

    GitPlugin.rollback(&legacy_id).await.unwrap();

    assert_eq!(
        fs::read_to_string(dir.path().join("tracked.txt")).unwrap(),
        "modified\n"
    );
}
