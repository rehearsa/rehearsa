# Changelog

All notable changes to Rehearsa are documented here.

---

## [0.7.0] — Notifications

The daemon can now tell you what happened.

Rehearsa 0.7.0 introduces a webhook notification system. When a rehearsal fails, a policy is violated, a baseline drifts, or a provider is unreachable — you find out. When things recover, you find out that too. A silent background daemon you can't trust is not a daemon worth running.

### Added
- Webhook notification channel registry (`/etc/rehearsa/notify.json`)
- `rehearsa notify add` — register named channels (Slack, Discord, ntfy, Gotify, any HTTP endpoint)
- `rehearsa notify default` — set a global default channel for all stacks
- `rehearsa notify test` — send a test payload to verify delivery before relying on it
- `rehearsa notify list / show / delete` — full channel management
- `--notify` flag on `rehearsa daemon watch` — per-stack channel override; global default is the fallback
- Five notification events with explicit severity levels:
  - 🔴 `RehearsalFatalError` — critical
  - 🔴 `ProviderVerificationFailed` — critical
  - 🟡 `PolicyViolation` — warning
  - 🟡 `BaselineDrift` — warning
  - 🟢 `RehearsalRecovered` — recovery (back to passing)
- JSON webhook payload: `source`, `severity`, `event`, `stack`, `message`, `timestamp`
- Optional `X-Rehearsa-Secret` header for receiver-side validation
- `StackRunSummary` now exposes `policy_violated` and `baseline_drift` — daemon reads these to fire the correct event
- `test_stack` no longer calls `process::exit()` — returns `Ok(summary)` in all non-fatal cases so callers can act on the result

### Changed
- CLI `stack test` derives exit code from summary fields directly, preserving the identical exit code contract
- Daemon trigger reads summary fields to dispatch the correct notification event rather than inferring from exit codes

### Philosophy
> "Prove it. Then tell someone."

---

## [0.6.0] — Backup Provider Hooks + Persistent Scheduler

The rehearsal is now connected to the backup.

Rehearsa 0.6.0 introduces backup provider integration and fixes a fundamental gap in the scheduler: last-run state now survives daemon restarts. These two features together mean Rehearsa can be trusted to run continuously and autonomously — verifying that a real backup exists before proving a stack can restore from it.

### Added
- Backup provider registry (`/etc/rehearsa/providers.json`)
- `rehearsa provider add` — register named Restic repositories with credential config
- `rehearsa provider show / list / delete` — full provider management
- `rehearsa provider verify` — checks repo reachability and snapshot presence via `restic snapshots --json`; reports latest snapshot timestamp
- `--provider` flag on `rehearsa daemon watch` — attach a named provider to a stack
- Provider verification runs before each rehearsal; a failing provider blocks the run with a clear log message
- Model B scaffold — `VerifyOptions` struct with `max_snapshot_age_hours` and `test_restore` fields (enforcement deferred)
- Scheduler state persisted to `/etc/rehearsa/scheduler_state.json`
- Scheduler loads persisted state on startup — last-run tracking survives daemon restarts
- `catch_up` now functions correctly — missed scheduled windows trigger an immediate rehearsal on restart
- `rehearsa daemon list` table gains Provider column

### Changed
- `verify_provider` returns `Err` instead of calling `process::exit()` — safe to call from daemon context without killing the process
- `add_watch` gains a `provider` parameter

### Philosophy
> "Not just: can this stack restore? But: can it restore from a backup that actually exists?"

---

## [0.5.0] — Scheduled Rehearsals

Rehearsa stopped waiting to be told.

Version 0.5.0 introduced a cron scheduler running as an independent task alongside the file watcher. The two triggers are fully orthogonal — a file change fires a rehearsal immediately, a schedule fires one at the declared time. The registry is re-read on every tick, so schedules added while the daemon is running take effect without a restart.

### Added
- Per-stack cron expressions (5-field standard cron, e.g. `"0 3 * * *"`)
- Cron expressions validated at registration time
- Scheduler runs as an independent tokio task alongside the file watcher
- Registry re-read on every tick — live schedule changes take effect immediately
- In-memory last-run tracking per stack
- `catch_up` flag per watch entry — fires a missed rehearsal on daemon start (in-memory only; persisted in 0.6.0)
- `--schedule` and `--catch-up` flags on `rehearsa daemon watch`

### Philosophy
> "Restores should be rehearsed on a schedule, not just when you remember."

---

## [0.4.0] — Daemon Mode + File Watching

Rehearsa became a service.

Version 0.4.0 introduced the daemon — a systemd-managed background process that watches Compose files for changes and triggers rehearsals automatically. No manual intervention required. When your stack definition changes, the rehearsal runs.

### Added
- Systemd service generation and installation (`rehearsa daemon install / uninstall`)
- File watching via `notify` — automatic rehearsal on Compose file change
- Watch registry at `/etc/rehearsa/watches.json`
- Heartbeat logging (60s interval)
- Correct user detection via `SUDO_USER` for systemd unit generation
- `rehearsa daemon watch / unwatch / list / status / run` commands
- `rehearsa daemon install` prints management commands on completion

### Philosophy
> "A rehearsal you have to remember to run is a rehearsal you'll forget."

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
