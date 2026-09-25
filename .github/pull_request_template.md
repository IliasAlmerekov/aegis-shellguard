Closes #
<!-- No issue? Write "No issue: <reason>" instead, e.g. "No issue: maintainer request". -->

<!-- One or two sentences: what changed and why. The diff shows the files. -->

Evidence:
- `cargo test --workspace`: N passed, 0 failed
- `scripts/lint.sh`: clean
- `cargo audit`: no new advisories
- `cargo deny check`: ok

Confidence: high | medium | low. <!-- One sentence: what would prove this wrong, or what is left unchecked. -->
