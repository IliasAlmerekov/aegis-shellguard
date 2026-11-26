pub const NO_SOURCE: &[&str] = &[
    "ls -la /home/user",
    "echo hello world",
    "cat /etc/hostname",
    "grep -r TODO src/",
    "git status",
    "git log --oneline -20",
    "cargo build --release",
    "npm run test",
    "docker ps -a",
    "kubectl get pods -n production",
];
