# Changelog

All notable changes to Rehearsa are documented here.

---

## [0.3.0] — Restore Contract Engine

The philosophical shift from validation to contract.

Rehearsa 0.3.0 introduces the baseline system — a manually declared restore contract that future runs are measured against. This moves Rehearsa beyond reporting into enforcement. A stack either honours its declared contract or it doesn't. There is no grey area.

### Added
- Manual baseline pinning (`rehearsa baseline set`)
- Baseline drift detection — service topology, confidence, readiness, duration
- `fail_on_baseline_drift` policy flag
- Baseline drift reported in structured output and CI exit codes
- `--inject-failure` flag for controlled chaos testing
- Baseline drift exit code (4) distinct from other policy violations

### Philosophy
> Simulation → Validation → Contract

---

## [0.2.0] — Restore Validation + Policy Enforcement

The moment Rehearsa became enforceable.

Version 0.2.0 introduced restore readiness as a first-class concept alongside a full policy engine. Rehearsa stopped being observational and started being opinionated. A stack either meets the declared standard or the pipeline fails.

### Added
- Preflight restore readiness scoring
- Bind mount warnings (external dependency detection)
- `:latest` tag warnings (non-deterministic restore detection)
- Restore Readiness Score (0–100%)
- Policy engine (`rehearsa policy set`)
  - `--min-confidence`
  - `--min-readiness`
  - `--block-on-regression`
  - `--fail-on-duration-spike`
  - `--duration-spike-percent`
- Structured CI exit codes
- Readiness regression tracking
- Readiness column in status overview

### Philosophy
> "Not just simulation, but enforceable restore validation."

---

## [0.1.0] — Restore Simulation Engine

The foundation.

Rehearsa began as a single focused question: if this stack had to restore from scratch right now, would it actually boot? Everything in 0.1.0 exists to answer that question deterministically.

### Added
- Docker Compose parsing and dependency graph resolution
- Isolated restore simulation (temporary network, no live stack modification)
- Healthcheck-aware service scoring
- Confidence scoring and risk banding (LOW / MODERATE / HIGH / CRITICAL)
- Stability tracking (rolling average across last 5 runs)
- Regression detection (confidence delta, trend arrows)
- Tamper-evident run history (SHA-256 signed JSON records)
- Strict integrity mode
- CI-compatible deterministic exit codes
- Fleet status overview (`rehearsa status`)
- JSON output mode (`--json`)
- Single static Rust binary, no runtime dependencies

### Philosophy
> "Can this stack restore?"

---
