# Rehearsa

> **Backups are hope. Rehearsa is proof.**

Rehearsa is a restore contract engine for Docker-based self-hosted environments.
It validates that your docker-compose stacks can actually restore and boot correctly — before disaster happens.

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

Rehearsa exists to close that gap — and prove you can recover.

---

## Quick Start

```bash
# Test a stack
rehearsa --timeout 120 stack test docker-compose.yml

# View fleet status
rehearsa status

# Pin a restore contract baseline
rehearsa baseline set mystack

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
- Enforces policy and exits deterministically
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
rehearsa baseline set mystack
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
| 1 | Confidence below threshold |
| 2 | Regression detected |
| 3 | Critical failure |
| 4 | Baseline drift detected |

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

## Design Goals

- Agentless — Docker socket only
- Fully isolated network simulation
- No modification of live containers
- Deterministic cleanup and exit codes
- Explicit baseline contracts — no silent mutation
- Tamper-evident audit history
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

---

## Roadmap

- [ ] `baseline diff` — show exact contract drift
- [ ] `baseline promote` — promote latest successful run
- [ ] `baseline history` — contract change audit trail
- [ ] `baseline lock` — read-only contract protection
- [ ] Exportable compliance reports
- [ ] Backup provider hooks (Restic, Borg)
- [ ] Scheduled automated rehearsals
- [ ] Cross-host restore simulation

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
