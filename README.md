# Rehearsa

> **Backups are hope. Rehearsa is proof.**

Rehearsa is a deterministic restore contract engine for Docker-based self-hosted infrastructure.

It doesn't check whether your backup ran. It proves whether your infrastructure would actually recover — automatically, continuously, and with tamper-evident evidence.

![Rehearsa Status](docs/status.png)

---

## Why Rehearsa Exists

Most self-hosters run automated backups. Almost nobody tests restores.

When disaster strikes, backups fail silently in ways nobody anticipated — volumes missing, databases refusing to start, images pulling a different version, services booting in the wrong order, environment variables absent on the restore host.

A backup succeeding does not mean a restore will succeed.

Rehearsa exists to close that gap — not as a one-time check, but as a continuously enforced contract.

---

## Quick Start

```bash
# Test a stack
rehearsa stack test /path/to/docker-compose.yml

# Pin a restore contract
rehearsa baseline set /path/to/docker-compose.yml

# Generate a compliance report
rehearsa report --stack mystack --format pdf

# View fleet status
rehearsa status
```

---

## What Rehearsa Does

Rehearsa performs a controlled restore simulation from your Compose file:

- Parses the Compose file and resolves service dependency order
- Runs preflight checks — bind mounts, image tags, environment variables
- Creates an isolated temporary Docker network
- Boots services in dependency order
- Scores each service against healthcheck and running state
- Calculates stack confidence, risk band, and stability
- Compares the result against a declared baseline contract
- Records a tamper-evident run history
- Cleans up everything — containers and network

**No changes are made to your live stack.**

---

## The Contract Model

Rehearsa is built around a declared restore contract. You run a rehearsal, review the result, and pin it as the baseline — the standard this stack must meet on every future run.

```bash
rehearsa baseline set /path/to/docker-compose.yml
```

From that point, every rehearsal produces one of two verdicts:

```
CONTRACT HONOURED
DRIFT DETECTED
```

If confidence drops, readiness falls, services disappear, or duration spikes beyond tolerance — the contract is broken and Rehearsa tells you before a real restore does.

---

## Scoring Model

| State | Score |
|---|---|
| HEALTHY (healthcheck passed) | 100 |
| RUNNING (no healthcheck) | 85 |
| UNHEALTHY | 40 |
| EXITED / failed | 0 |

Stack confidence is the average of all service scores, banded into risk:

| Confidence | Risk |
|---|---|
| 90–100% | LOW |
| 70–89% | MODERATE |
| 40–69% | HIGH |
| 0–39% | CRITICAL |

Rehearsa also tracks regression trends (UP / DOWN / SAME), rolling stability across the last 5 runs, duration spikes, and policy violations.

---

## Preflight Checks

Before any simulation runs, Rehearsa scores the stack's restore readiness on a fresh host:

- **BindMountRule** — flags bind mount paths that must exist before the stack can start
- **ImagePullRule** — flags `:latest` tags that may pull a different image on restore
- **EnvVarRule** — detects bare environment variable references missing from the restore host

Every finding is attributed to its source rule with severity and score impact.

---

## Policy Engine

Enforce restore standards per stack:

```bash
rehearsa policy set mystack \
  --min-confidence 80 \
  --min-readiness 90 \
  --block-on-regression true \
  --fail-on-duration-spike true \
  --duration-spike-percent 40
```

Policy violations produce non-zero exit codes — making Rehearsa CI/CD compatible.

---

## Daemon Mode

Rehearsa runs as a systemd service, watching your Compose files and rehearsing on a schedule:

```bash
# Install the daemon
rehearsa daemon install

# Watch a stack with a nightly schedule
rehearsa daemon watch /path/to/docker-compose.yml --schedule "0 3 * * *"

# Check daemon status
rehearsa daemon status
```

Rehearsals fire automatically when a Compose file changes, or on schedule — whichever comes first.

---

## Backup Provider Integration

Attach a named backup provider to a stack so Rehearsa verifies a real snapshot exists before each rehearsal:

```bash
# Register a Restic repository
rehearsa provider add prod-restic \
  --kind restic \
  --repo /mnt/nas/backups/restic \
  --password-env RESTIC_PASSWORD

# Verify it
rehearsa provider verify prod-restic
```

Restic and Borg are supported. If the provider cannot be reached or has no recent snapshot, the rehearsal is blocked with a clear log message.

---

## Notifications

Rehearsa notifies you when something changes:

```bash
rehearsa notify add alerts \
  --url https://ntfy.sh/myserver \
  --secret mysecret
```

Five event types: rehearsal fatal error, provider verification failed, policy violation, baseline drift, and rehearsal recovered. Webhook and email transports supported simultaneously on a single channel.

---

## Compliance Reports

Generate a tamper-evident compliance report from on-disk state — no Docker calls required:

```bash
rehearsa report --stack mystack --format both --output ./reports/
```

JSON and PDF output. The PDF includes a verdict banner (PASS / WARN / FAIL), service score bars, history trend, baseline contract status, preflight findings, and a unique tamper-evident report ID. Single-stack or fleet-wide.

---

## Tamper-Evident History

Every run is SHA-256 hashed and chained. In strict mode, any tampered or corrupted history file blocks execution:

```bash
rehearsa --strict-integrity stack test docker-compose.yml
```

---

## Compose Compatibility

Rehearsa is designed to work against real-world Compose files — not idealised ones. It handles YAML anchor and merge key patterns (`<<:`), string and sequence forms of `command` and `entrypoint`, mixed environment block styles, and both versioned and unversioned Compose formats.

If Docker can run it, Rehearsa can read it.

---

## What Rehearsa Is Not

- Not a backup tool
- Not a monitoring tool
- Not a container orchestrator
- Not a restore tool

It does one thing: **prove whether your infrastructure would recover to a declared standard.**

---

## Design Goals

- Agentless — Docker socket only
- Fully isolated network simulation
- No modification of live containers
- Deterministic cleanup
- Clear scoring and trend visibility
- CI-friendly exit codes
- Single static binary, no runtime dependencies
- Written in Rust

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

Rehearsa is actively evolving.
If you would like to contribute, open an issue to discuss scope and architectural alignment first.

---

## License

MIT — see LICENSE

---

*If you're not rehearsing your restores, you don't have backups. You have hopes.*
