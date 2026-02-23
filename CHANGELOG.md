# Changelog

All notable changes to Rehearsa are documented here.

---

## [0.9.1] — Coverage + Schema Hardening

The final release before 1.0.

0.9.1 closes the feature story with one new command and hardens the on-disk schema in preparation for the 1.0 stability guarantee. After this release, the CLI surface is declared stable — no breaking changes will be made without a major version increment.

### Added

**Restore Contract Coverage**
- `rehearsa coverage` — fleet-wide contract coverage across all watched stacks
- Coverage bar, per-stack status table, and rollup counters in a single command
- Five status states: `CONTRACT HONOURED`, `DRIFT DETECTED`, `NO_BASELINE`, `NO_RUNS`, `UNWATCHED`
- Exits 0 only when all watched stacks are honouring their contracts — usable as a CI gate
- `--json` flag for machine-readable output and pipeline integration
- Pure on-disk read — no Docker calls required

**Schema Versioning**
- `schema_version` field added to `RunRecord` and `StackBaseline`
- `CURRENT_SCHEMA_VERSION = 1` — the single source of truth for the on-disk format
- Pre-1.0 records on disk deserialise cleanly as version 0 — no migration required
- Hash computation excludes `schema_version` — existing tamper-evident records remain valid
- All record construction sites stamp the current version — every new record is versioned from this release forward
- Breaking schema changes after 1.0 will be detectable on load

### Philosophy
> "Prove recoverability. Know your coverage. Ship with confidence."

---

## [0.9.0] — Full Compatibility + Contract Hardening

Rehearsa now works against real infrastructure, not just ideal infrastructure.

0.9.0 is the result of running Rehearsa against a real 21-stack production self-hosted fleet and fixing everything that stood between the tool and honest results. The parser was rebuilt from the ground up. The contract model grew smarter — external networks, oneshot services, and snapshot age enforcement all land in this release. The first-deployment experience is solved with a single command.

### Added

**Two-layer Compose Parser**
- Complete rewrite of the Compose deserialisation layer
- Raw YAML parsed first into `serde_yaml::Value` — no field-level fatal errors possible
- Fields extracted with explicit, tolerant extractors covering all known real-world variations
- Handles YAML anchor and merge key patterns (`<<:`) in environment blocks and service definitions
- Handles string and sequence forms of `command` and `entrypoint`
- Handles map-form `depends_on` with `condition: service_healthy` and similar
- Handles object-form volumes `{source, target}` and ports `{published, target}`
- Handles disabled healthchecks (`disable: true`)
- Labels extracted and exposed for rule and scoring use
- Validated against 21 production stacks — zero fatal errors

**ExternalNetworkRule**
- New preflight rule detecting external networks declared in the Compose file
- Networks that don't exist on the current host: Critical finding, −25 score penalty
- Networks that exist but are external: Info advisory — must also be created on any restore host
- Catches the most common silent restore failure in multi-stack homelab environments

**Oneshot Container Scoring**
- Services labelled `com.rehearsa.oneshot: true` are scored correctly
- A labelled service that exits with code 0 scores 100 — it finished, it didn't fail
- Fixes false-zero scoring for Recyclarr, migration runners, config appliers, and any init container pattern
- Exit code verified against Docker inspect — only genuinely clean exits score 100

**Model B Enforcement — Snapshot Age**
- `max_snapshot_age_hours` now enforced for both Restic and Borg providers
- `rehearsa provider verify` reports snapshot age and fails if it exceeds the declared maximum
- Restic: timestamp parsed from `restic snapshots --json` output
- Borg: timestamp parsed from `borg list --json` archive metadata
- `rehearsa provider verify-set <name> --max-age-hours <n>` — configure enforcement per provider
- The contract now answers: "does a backup exist that is recent enough to restore from?"

**Sendgrid Delivery**
- Sendgrid email transport fully implemented via HTTP API
- Sends to Sendgrid v3 `/mail/send` endpoint using `curl` — zero new dependencies
- API key via literal value or environment variable
- Multiple recipients supported
- All five notification events delivered over Sendgrid identically to SMTP

**Baseline Auto-Init**
- `rehearsa baseline auto-init` — first-deployment contract bootstrapping
- Rehearses every watched stack and pins the result as an initial baseline
- Clear output: confidence and readiness per stack, pass/fail summary
- Baselines marked as initial — output reminds operator to review before enforcing policy
- Next steps printed on completion: status, show, policy set

**Stack Name Collision Fix**
- Stack identity now derived from the parent directory name, not the filename
- Two stacks both named `docker-compose.yml` in different directories no longer collide in history, baseline, or policy
- Falls back to file stem only when no meaningful parent directory is available

### Fixed

**Lock Contention False Alarms**
- When the scheduler and file watcher both fire for the same stack simultaneously, the second attempt is now logged as a deliberate skip rather than a failure
- No `RehearsalFatalError` notification fires for lock contention
- Log message distinguishes: "already in progress (lock held)" vs genuine failure

### Philosophy
> "If Docker can run it, Rehearsa can read it. If a contract is declared, Rehearsa enforces it."

---

## [0.8.1] — Compose Compatibility

Rehearsa now works against real-world infrastructure.

This patch release was driven by testing against production Docker Compose stacks on live self-hosted infrastructure. Two parser bugs were found and fixed — both caused fatal errors that blocked rehearsals entirely on stacks that Docker itself runs without complaint.

The goal is simple: if Docker can run it, Rehearsa can read it.

### Fixed

**YAML merge key support in environment blocks**
- Compose files using `<<: *anchor` inside environment blocks caused a fatal parse error
- This pattern is common in real-world stacks that share environment templates across services
- Merge keys are now skipped cleanly during environment deserialisation
- Affected stacks using patterns like `<<: *common-env` alongside additional env vars now parse correctly

**String `command` field support**
- Docker Compose allows `command` as either a string (`command: "--force"`) or a sequence (`command: ["--force"]`)
- Rehearsa only accepted the sequence form — a plain string caused a fatal parse error
- Both forms are now accepted and normalised to a string list internally

### Compatibility
Both fixes were validated against production stacks using real infrastructure before release. The regression test covered four affected stacks across both bug patterns with no regressions.

### Philosophy
> "If Docker can run it, Rehearsa can read it."

---

## [0.8.0] — Compliance, Contracts, and Coverage

Rehearsa grew up.

Version 0.8.0 is the MSP release. It closes the loop between running rehearsals and proving they happened — with exportable compliance reports, a full baseline audit trail, email notifications, Borg backup support, and a materially smarter preflight. Every feature in this release exists because someone needed to hand evidence to a client, an auditor, or a regulator and say: this infrastructure can recover.

### Added

**Compliance Reports**
- `rehearsa report` — generate a full compliance report from on-disk rehearsal state
- JSON output: machine-readable, pipeable, archivable
- PDF output: paginated document with verdict banner, per-section tables, service score bars, and tamper-evident report ID
- Report sections: latest rehearsal, history and trend, baseline contract status, policy compliance, preflight findings, provider status
- `--stack` flag for single-stack reports; omit for fleet-wide (one JSON array, one PDF per stack)
- `--window` flag to control how many historical runs appear in the trend section
- `--provider` flag to include named provider status in the report
- Verdict: `PASS` / `WARN` / `FAIL` derived from confidence, policy, and drift state

**Baseline Promote + History**
- `rehearsa baseline promote <stack>` — pin any historical run as the new baseline without needing the compose file path
- `--timestamp` flag for targeted promotion; defaults to latest run; partial timestamp matching supported
- Baseline history log at `~/.rehearsa/baseline-history/<stack>/` — every `baseline set` and `baseline promote` appends a timestamped snapshot automatically
- `rehearsa baseline history` — fleet-wide table showing current pinned baseline, drift status, and version count per stack
- `rehearsa baseline history --stack <stack>` — per-version chronological diff: confidence delta, readiness delta, duration delta, services added/removed between each consecutive version
- `StackBaseline` gains `pinned_at` (run timestamp) and `promoted_at` (wall clock) fields; fully backward-compatible with existing baseline files

**Email Notifications**
- Email transport added to the notify channel system — channels now support webhook, email, or both simultaneously
- `rehearsa notify add-email` — register or update the email transport on a named channel
- SMTP delivery via `lettre` 0.11 with STARTTLS and rustls — no system dependencies, proper TLS certificate validation
- Password supplied via literal value or environment variable — credential never required in the registry file
- Sendgrid scaffolded — API key config stored and validated; delivery deferred
- All five existing notification events fire over email using the same severity model as webhooks
- `rehearsa notify show` updated to display full email config alongside webhook config
- `rehearsa notify list` gains a Transport column: `webhook`, `email (smtp)`, or `webhook + email (smtp)`
- `rehearsa notify test` fires all configured transports and reports each independently

**Borg Backup Provider**
- `--kind borg` now accepted by `rehearsa provider add`
- Supports local paths and SSH remotes (`user@host:path`) — Borg handles SSH natively in the repository string
- Passphrase via env var (`BORG_PASSPHRASE`) or file (`BORG_PASSCOMMAND=cat <file>`)
- `rehearsa provider verify` for Borg runs `borg info --json` (reachability) then `borg list --json --last 1` (archive presence)
- Reports archive count, latest archive name, and timestamp — mirrors the Restic verify output format
- Model B scaffold (max snapshot age, test restore) carried forward — same pattern as Restic

**Preflight — Environment Variable Rule**
- New `EnvVarRule` checks every bare-key env entry (entries without `=`) across all services
- `Critical` finding (−20 points) when a required variable is absent from the restore host
- `Info` advisory when a variable is present on this host but must also exist on any future restore host
- `ctx.environment` (host env snapshot) now actively used — was populated but unread in prior versions
- `Severity::Info` now emitted — was defined but never constructed in prior versions
- `finding.rule` now printed in preflight output — every finding is attributed to its source rule
- Bind mount existing-path finding downgraded from `Warning` (−5) to `Info` (−0) — presence on this host is not a problem, portability is the concern

### Changed
- `NotifyChannel.url` is now `Option<String>` — channels can be email-only, webhook-only, or both; existing webhook-only channels on disk deserialise correctly
- `StackRunSummary` trimmed to the four fields actually consumed by callers (`readiness`, `confidence`, `policy_violated`, `baseline_drift`); unused fields removed
- `rehearsa notify list` empty-state message updated to mention both `add` and `add-email`

### Dependencies
- `printpdf = "0.7"` — PDF generation, pure Rust
- `lettre = { version = "0.11", default-features = false, features = ["smtp-transport", "rustls-tls", "builder"] }` — SMTP delivery

### Philosophy
> "Prove it. Record it. Hand it over."

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
