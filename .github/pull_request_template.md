Closes #
<!-- No issue? Write "No issue: <reason>" instead, e.g. "No issue: maintainer request". -->

<!-- 2-3 bullets: what changed, why, and the issue it closes. The commits carry the detail. -->

-
-

Evidence:
- `cargo test --workspace`: N passed, 0 failed
- `scripts/lint.sh`: clean
- `cargo audit`: no new advisories
- `cargo deny check`: ok

Confidence: high | medium | low. <!-- One sentence: what would prove this wrong, or what is left unchecked. -->
