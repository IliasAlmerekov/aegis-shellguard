use super::*;

#[test]
fn assess_h3_followup_storage_sync_delete_rules() {
    let cases: &[(&str, RiskLevel, &str)] = &[
        (
            "gcloud storage rm -r gs://my-bucket/data",
            RiskLevel::Danger,
            "CL-014",
        ),
        (
            "gcloud storage rm --recursive gs://my-bucket",
            RiskLevel::Danger,
            "CL-014",
        ),
        (
            "gcloud --project prod storage rm gs://my-bucket --recursive",
            RiskLevel::Danger,
            "CL-014",
        ),
        (
            "rsync -av --delete ./dist/ deploy:/srv/site/",
            RiskLevel::Warn,
            "FS-015",
        ),
        (
            "rsync --dry-run --delete-after ./dist/ deploy:/srv/site/",
            RiskLevel::Warn,
            "FS-015",
        ),
        (
            "rsync -n --delete-excluded ./dist/ deploy:/srv/site/",
            RiskLevel::Warn,
            "FS-015",
        ),
    ];
    for (cmd, risk, id) in cases {
        assert_assessment_matches_pattern(cmd, *risk, id);
    }
}

#[test]
fn assess_h3_followup_device_wiper_rules() {
    let cases: &[(&str, RiskLevel, &str)] = &[
        ("blkdiscard /dev/sda", RiskLevel::Block, "FS-016"),
        (
            "sudo blkdiscard -f /dev/nvme0n1",
            RiskLevel::Block,
            "FS-016",
        ),
        ("/usr/sbin/blkdiscard /dev/sdb", RiskLevel::Block, "FS-016"),
        ("sgdisk --zap-all /dev/sda", RiskLevel::Danger, "FS-017"),
        ("sgdisk -Z /dev/sda", RiskLevel::Danger, "FS-017"),
        ("parted /dev/sda mklabel gpt", RiskLevel::Danger, "FS-018"),
        (
            "parted -s /dev/sda mklabel msdos",
            RiskLevel::Danger,
            "FS-018",
        ),
        ("parted /dev/sda rm 1", RiskLevel::Danger, "FS-018"),
    ];
    for (cmd, risk, id) in cases {
        assert_assessment_matches_pattern(cmd, *risk, id);
    }
}

#[test]
fn test_fs015_rsync_delete_missing_args_fires() {
    assert_assessment_matches_pattern(
        "rsync --delete-missing-args --files-from=list ./ dest/",
        RiskLevel::Warn,
        "FS-015",
    );
}

#[test]
fn assess_h3_followup_redis_cli_flush_rules() {
    let cases: &[(&str, RiskLevel, &str)] = &[
        ("redis-cli FLUSHALL", RiskLevel::Danger, "DB-006"),
        (
            "redis-cli -h cache.local -n 0 FLUSHDB",
            RiskLevel::Danger,
            "DB-006",
        ),
        (
            "redis-cli --raw FLUSHALL ASYNC",
            RiskLevel::Danger,
            "DB-006",
        ),
        ("sudo redis-cli FLUSHDB", RiskLevel::Danger, "DB-006"),
    ];
    for (cmd, risk, id) in cases {
        assert_assessment_matches_pattern(cmd, *risk, id);
    }
}
