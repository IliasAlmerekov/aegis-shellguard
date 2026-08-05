# Bash Unicode fuzz-crash repair

## Problem

The `language_bash` fuzz target reproduces an AddressSanitizer segmentation
fault in `tree-sitter-bash` 0.25.1's native external scanner when it is handed
non-ASCII source. The grammar is an allowed native dependency under ADR-022,
but it is untrusted-input code and must not crash an in-process adapter.

## Constraints

- Preserve the current pinned grammar and its qualified release metadata.
- Do not silently treat unsupported source as clean: return a typed adapter
  degradation so the parent records `UnsupportedEncoding` and remains
  fail-closed.
- Keep the regression at the public `languages::bash::analyze` seam, using the
  CI artifact's lossy-UTF-8 reproduction.

## Slices

1. Add the crashing reproduction as a unit test; confirm it fails under ASan.
2. Reject non-ASCII Bash adapter input before calling the native scanner,
   returning no operations and a typed encoding degradation.
3. Add the artifact to the fuzz corpus and replay it under the pinned nightly
   ASan target, then run the relevant workspace gates and document the bounded
   input posture.
