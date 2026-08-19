# ADR-003 — Aegis is a heuristic guardrail, not a sandbox

## Status

Superseded by [ADR-029](adr-029-the-sandbox-is-a-mandatory-1-0-layer.md), which
makes the Sandbox a mandatory 1.0 layer. The honesty constraint below survives in
narrower form: Aegis is not a confidentiality boundary and not a privilege
boundary, and no document may promise that file reads or secrets are hidden from
a command.

## Decision

Aegis intentionally operates on raw command text and policy decisions before
the real shell runs.

## Why

- it is meant to reduce accidental damage, not provide OS isolation
- approved commands still run with the operator's normal privileges

## Implication

- docs must not describe Aegis as a hard security boundary
- limitations around encoded input, deferred execution, and shell/runtime
  expansion remain explicit non-goals
