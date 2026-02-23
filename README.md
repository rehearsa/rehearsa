# Rehearsa

> **Backups are hope. Rehearsa is proof.**

Rehearsa is a deterministic restore contract engine for Docker-based self-hosted infrastructure.
It proves your stacks can actually recover to a declared standard — automatically, continuously, and with tamper-evident evidence.

![Rehearsa Status](docs/status.png)

---

## Why Rehearsa Exists

Most self-hosters run automated backups. Almost nobody tests restores.

When disaster strikes:
- Volumes are missing
- Databases fail to start
- Permissions are wrong
- Images are unavailable
- Dependencies boot in the wrong order

A backup succeeding does not mean a restore will succeed.

Rehearsa exists to close that gap — not just once, but continuously, with a pinned contract you can prove hasn't drifted.

---

## Quick Start

```bash
# Test a stack
rehearsa --timeout 120 stack test docker-compose.yml

# View fleet status
rehearsa status

# Pin a restore contract baseline
rehearsa baseline set docker-compose.yml

# Check for drift against your contract
rehearsa baseline diff mystack

# Test with failure injection
rehearsa --inject-failure myservice stack test docker-compose.yml
```

---

## What Rehearsa Does

Rehearsa performs a controlled restore simulation based on a docker-compose file:

- Runs preflight readiness checks before simulation
- Parses the Compose file and resolves service dependency order
- Creates an isolated temporary Docker network
- Pulls images based on policy
- Boots services in dependency order
- Waits for container health and runtime state
- Scores each service and calculates confidence, risk, and stability
- Detects drift against a declared restore contract (baseline)
- Verifies backup provider health before rehearsing
- Enforces policy and exits deterministically
- Fires webhook notifications on violations, drift, or recovery
- Cleans up everything — containers and network

**No changes are made to your live stack.**

---

## Core Principle

Rehearsa is not a backup tool. It is a restore contract engine.

It answers one critical question:

> If I had to rebuild this stack on a fresh host right now — would it restore to the declared standard?

---

## Preflight Readiness

Before simulation begins, Rehearsa analyses the stack for restore risk:

```
Preflight: Fresh Host Readiness
--------------------------------
⚠ Service 'navidrome' uses bind mount: /mnt/nas/data/media/music
⚠ Service 'navidrome' uses bind mount: /mnt/nvme/docker/navidrome
⚠ Service 'navidrome' uses :latest tag (non-deterministic restore)
Restore Readiness Score: 85%
```

Readiness warnings surface external dependencies and non-deterministic images that would silently break a real restore.

---

## Scoring Model

| State | Score |
|---|---|
| HEALTHY (healthcheck passed) | 100 |
| RUNNING (no healthcheck) | 85 |
| UNHEALTHY | 40 |
| EXITED / failed | 0 |

Stack confidence is the average of all service scores, aggregated into a risk band:

| Confidence | Risk |
|---|---|
| 90–100% | LOW |
| 70–89% | MODERATE |
| 40–69% | HIGH |
| 0–39% | CRITICAL |

---

## Baseline Contract System

Pin a restore contract for any stack:

```bash
rehearsa baseline set docker-compose.yml
```

Future runs are measured against this contract. If the stack drifts — new services, missing services, confidence drop, readiness drop, duration spike — Rehearsa detects and reports it.

```
BASELINE DRIFT DETECTED
-----------------------
Confidence delta: -15%

POLICY VIOLATION: baseline drift detected
```

Contracts are explicit. Baselines never update silently.

---

## Policy Engine

Enforce restore standards per stack:

```bash
rehearsa policy set mystack \
  --min-confidence 80 \
  --min-readiness 80 \
  --block-on-regression true \
  --fail-on-baseline-drift true \
  --fail-on-duration-spike true \
  --duration-spike-percent 40
```

Policy violations produce deterministic exit codes — making Rehearsa fully CI/CD compatible.

---

## Exit Codes

| Code | Meaning |
|---|---|
| 0 | Pass |
| 1 | Fatal error |
| 2 | Moderate confidence (70–89%) |
| 3 | Critical confidence (<40%) |
| 4 | Policy violation |
| 5 | Baseline drift |

---

## CI Integration

```yaml
name: Restore Rehearsal
on:
  schedule:
    - cron: '0 3 * * *'
jobs:
  rehearse:
    runs-on: self-hosted
    steps:
      - uses: actions/checkout@v3
      - name: Run restore rehearsal
        run: rehearsa --fail-below 80 --fail-on-regression stack test docker-compose.yml
```

---

## Daemon Mode

Run Rehearsa as a systemd service for continuous, automated rehearsals:

```bash
# Install and start the daemon
sudo rehearsa daemon install

# Watch a stack — rehearse on Compose file change
rehearsa daemon watch mystack docker-compose.yml

# Watch with a cron schedule
rehearsa daemon watch mystack docker-compose.yml --schedule "0 3 * * *"

# Watch with a backup provider and notification channel
rehearsa daemon watch mystack docker-compose.yml \
  --schedule "0 3 * * *" \
  --provider restic-main \
  --notify slack-ops

# List watched stacks
rehearsa daemon list

# View logs
journalctl -u rehearsa -f
```

Scheduled run history persists across daemon restarts. Stacks registered with `--catch-up` will fire immediately on restart if a scheduled window was missed.

---

## Backup Provider Hooks

Verify your backup repository is healthy before each rehearsal:

```bash
# Register a Restic repository
rehearsa provider add restic-main \
  --kind restic \
  --repo /mnt/backups/restic \
  --password-env RESTIC_PASSWORD

# Verify it's reachable and has snapshots
rehearsa provider verify restic-main

# List all providers
rehearsa provider list
```

When a provider is attached to a daemon watch, Rehearsa verifies the repository and confirms at least one snapshot exists before running the rehearsal. A failed provider blocks the run and fires a notification.

---

## Notifications

Get alerted when things go wrong — or when they recover:

```bash
# Register a webhook channel (Slack, Discord, ntfy, Gotify, any HTTP endpoint)
rehearsa notify add slack-ops --url https://hooks.slack.com/services/...

# Set as the global default for all stacks
rehearsa notify default slack-ops

# Send a test notification to verify delivery
rehearsa notify test slack-ops

# Per-stack channel override
rehearsa daemon watch mystack docker-compose.yml --notify client-a-slack
```

**Notification events:**

| Severity | Event |
|---|---|
| 🔴 Critical | Rehearsal fatal error |
| 🔴 Critical | Provider verification failed |
| 🟡 Warning | Policy violation |
| 🟡 Warning | Baseline drift detected |
| 🟢 Recovery | Rehearsal recovered (back to passing) |

Payloads are JSON-formatted webhooks. An optional `X-Rehearsa-Secret` header is supported for receiver validation.

---

## Strict Integrity Mode

Rehearsa signs every run record with a SHA-256 hash. In strict mode, any tampered or corrupted history file will block execution — providing a tamper-evident audit trail.

```bash
rehearsa --strict-integrity stack test docker-compose.yml
```

---

## History and Trend Tracking

```bash
rehearsa history show mystack
```

```
Stack: mystack

2026-02-21T12:39:45+00:00 | Confidence: 100%        | Risk: LOW      | Duration: 13s | Exit: 0
2026-02-21T13:15:22+00:00 | Confidence: 100% →   0  | Risk: LOW      | Duration: 13s | Exit: 0
2026-02-21T14:02:11+00:00 | Confidence:  78% ↓ -22  | Risk: MODERATE | Duration: 15s | Exit: 0
```

---

## Full Command Reference

```
rehearsa stack test <compose-file>

rehearsa baseline set <compose-file>
rehearsa baseline show <stack>
rehearsa baseline diff <stack>
rehearsa baseline delete <stack>

rehearsa policy set <stack> [--min-confidence N] [--min-readiness N]
                            [--block-on-regression bool] [--fail-on-baseline-drift bool]
                            [--fail-on-duration-spike bool] [--duration-spike-percent N]
rehearsa policy show <stack>
rehearsa policy delete <stack>

rehearsa history list
rehearsa history show <stack>

rehearsa provider add <n> --kind restic --repo <path> [--password-env VAR | --password-file PATH]
rehearsa provider show <n>
rehearsa provider list
rehearsa provider delete <n>
rehearsa provider verify <n>

rehearsa notify add <n> --url <webhook-url> [--secret KEY]
rehearsa notify show <n>
rehearsa notify list
rehearsa notify delete <n>
rehearsa notify default <n>
rehearsa notify test <n>

rehearsa daemon install
rehearsa daemon uninstall
rehearsa daemon status
rehearsa daemon run
rehearsa daemon watch <stack> <compose-file> [--schedule CRON] [--catch-up]
                                             [--provider NAME] [--notify NAME]
rehearsa daemon unwatch <stack>
rehearsa daemon list

rehearsa status
rehearsa version
```

---

## Design Goals

- Agentless — Docker socket only
- Fully isolated network simulation
- No modification of live containers
- Deterministic cleanup and exit codes
- Explicit baseline contracts — no silent mutation
- Tamper-evident audit history
- Backup provider verification before rehearsal
- Webhook notifications with severity and recovery events
- CI-friendly by default
- Single static binary — no runtime dependencies
- Written in Rust

---

## What Rehearsa Is Not

- Not a backup tool — use Restic, Borg, or similar
- Not a monitoring tool — use Uptime Kuma or similar
- Not a container orchestrator — use Compose or Kubernetes
- Not a live migration system

It does one thing: **prove your infrastructure can recover to a declared standard.**

---

## Versioned Philosophy

| Version | Identity |
|---|---|
| 0.1.0 | Restore Simulation Engine |
| 0.2.0 | Restore Validation + Policy Enforcement |
| 0.3.0 | Restore Contract Engine |
| 0.4.0 | Daemon Mode + File Watching |
| 0.5.0 | Scheduled Rehearsals |
| 0.6.0 | Backup Provider Hooks + Persistent Scheduler |
| 0.7.0 | Notifications |

---

## Roadmap

- [ ] Exportable compliance report (PDF/JSON)
- [ ] `baseline promote` and `baseline history`
- [ ] Email notifications
- [ ] Borg backup provider
- [ ] Broader Compose compatibility testing
- [ ] SaaS layer — central dashboard, multi-host fleet
- [ ] Compliance-grade reporting for MSPs and regulated teams

---

## Installation

### Pre-built binary (x86_64)

```bash
curl -L https://github.com/rehearsa/rehearsa/releases/latest/download/rehearsa-x86_64 -o rehearsa
chmod +x rehearsa
sudo mv rehearsa /usr/local/bin/
```

### ARM (Raspberry Pi and similar)

```bash
curl -L https://github.com/rehearsa/rehearsa/releases/latest/download/rehearsa-aarch64 -o rehearsa
chmod +x rehearsa
sudo mv rehearsa /usr/local/bin/
```

### Build from source

```bash
git clone https://github.com/rehearsa/rehearsa
cd rehearsa
cargo build --release
```

Requires Rust 1.75+ and Docker.

---

## Contributing

Rehearsa is actively evolving. If you would like to contribute, open an issue to discuss scope and architectural alignment first.

---

## License

MIT — see [LICENSE](LICENSE)

---

*If you're not rehearsing your restores, you don't have backups. You have hopes.*
