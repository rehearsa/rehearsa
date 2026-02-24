# Getting Started with Rehearsa

You back up your Docker stacks. But when did you last prove the backup would actually restore?

Rehearsa answers that question — automatically, on a schedule, against a declared standard. This guide takes you from zero to a monitored, contracted fleet in about 15 minutes.

---

## What Rehearsa does

Rehearsa runs a controlled restore simulation from your existing Compose file. It boots your services in an isolated network, scores each one, and compares the result against a baseline contract you declare. Every run produces a tamper-evident record. If something changes — confidence drops, a service disappears, an image can no longer be pulled — Rehearsa tells you before a real disaster does.

**It does not touch your live stack.** No containers are stopped or modified. The simulation runs in a separate temporary network and cleans up after itself.

---

## Requirements

- Linux host running Docker
- Docker socket accessible (default: `/var/run/docker.sock`)
- `sudo` access for daemon installation
- `restic` or `borg` installed on the host if using provider verification (see [Attach a backup provider](#attach-a-backup-provider))

---

## Install

**x86_64**
```bash
curl -L https://github.com/rehearsa/rehearsa/releases/latest/download/rehearsa-x86_64 -o rehearsa
chmod +x rehearsa
sudo mv rehearsa /usr/local/bin/
```

**aarch64 (Raspberry Pi, ARM servers)**
```bash
curl -L https://github.com/rehearsa/rehearsa/releases/latest/download/rehearsa-aarch64 -o rehearsa
chmod +x rehearsa
sudo mv rehearsa /usr/local/bin/
```

Verify:
```bash
rehearsa version
```

---

## Your first rehearsal

Pick any Compose file. Run a rehearsal against it:

```bash
rehearsa stack test /path/to/docker-compose.yml
```

Rehearsa will:
1. Run preflight checks — bind mounts, image tags, environment variables, external networks
2. Pull any missing images
3. Boot services in dependency order in an isolated network
4. Score each service based on healthcheck and running state
5. Print the result and clean everything up

A typical output looks like this:

```
Preflight: Fresh Host Readiness
--------------------------------
ℹ  [BindMountRule] Service 'app' uses bind mount '/data/app' — ensure data is restored before rehearsal
Restore Readiness Score: 95%

Starting restore simulation for 'myapp' (3 services)...

✓ postgres     — HEALTHY   100%
✓ redis        — RUNNING    85%
✓ app          — HEALTHY   100%

Confidence: 95%   Risk: LOW   Stability: 100%   Duration: 14s

CONTRACT HONOURED
```

The **confidence score** is the average of all service scores. The **readiness score** reflects how prepared a fresh host would be to run this stack. Both matter.

---

## Scoring reference

| Service state | Score |
|---|---|
| Healthcheck passing | 100% |
| Running, no healthcheck | 85% |
| Unhealthy | 40% |
| Exited or failed | 0% |
| Exited (oneshot) | 100% |

| Confidence | Risk band |
|---|---|
| 90–100% | LOW |
| 70–89% | MODERATE |
| 40–69% | HIGH |
| 0–39% | CRITICAL |

---

## Pin a baseline contract

A rehearsal without a baseline is just an observation. A baseline is a declared standard — the result this stack must meet on every future run.

Once you're happy with a rehearsal result, pin it:

```bash
rehearsa baseline set /path/to/docker-compose.yml
```

From this point, every rehearsal produces one of two verdicts:

```
CONTRACT HONOURED
DRIFT DETECTED
```

If confidence drops, readiness falls, a service disappears, or duration spikes beyond tolerance — the contract is broken.

To inspect your current contract:
```bash
rehearsa baseline show myapp
```

To see what changed since the last run:
```bash
rehearsa baseline diff myapp
```

---

## Watch a stack and run on a schedule

Manual rehearsals are useful. Automated ones are the point.

First, install the daemon:
```bash
sudo rehearsa daemon install
sudo systemctl start rehearsa
```

Then register your stack with a nightly schedule:
```bash
sudo rehearsa daemon watch myapp /path/to/docker-compose.yml \
  --schedule "0 3 * * *"
```

The daemon now:
- Rehearses the stack every night at 03:00
- Also rehearses automatically whenever the Compose file changes
- Records every run to tamper-evident history

List what's being watched:
```bash
rehearsa daemon list
```

---

## First deployment — contract the whole fleet at once

If you have multiple stacks already being watched, you don't need to pin baselines one by one. Run:

```bash
rehearsa baseline auto-init
```

This rehearses every watched stack and pins the result as an initial baseline in one command. Output tells you the confidence and readiness for each stack. Review the scores — if anything looks wrong, fix the stack and run `baseline set` manually for that one.

---

## Check fleet coverage

Once your stacks are watched and contracted:

```bash
rehearsa coverage
```

```
Restore Contract Coverage
────────────────────────────────────────────────────────────
Coverage  [████████████████████]  100%

  4  watched
  4  with baseline contract
  4  honouring contract  ✓

Stack                  Status               Confidence  Readiness
──────────────────────────────────────────────────────────────────
immich                 ✓  CONTRACT HONOURED        94%        88%
paperless              ✓  CONTRACT HONOURED        91%       100%
vaultwarden            ✓  CONTRACT HONOURED       100%       100%
gitea                  ✓  CONTRACT HONOURED        87%        95%

  All contracts are honoured.
```

`rehearsa coverage` exits non-zero if any contract is not honoured — use it as a CI gate or a health check in your monitoring.

---

## View fleet status

```bash
rehearsa status
```

Shows confidence, readiness, risk band, stability, and trend arrows for every stack with history.

---

## Set a restore policy

Policies turn observations into enforcement. If a stack falls below your declared standard, the rehearsal exits with a non-zero code.

```bash
rehearsa policy set myapp \
  --min-confidence 80 \
  --min-readiness 90 \
  --block-on-regression true \
  --fail-on-baseline-drift true
```

Useful flags:
- `--min-confidence` — minimum acceptable confidence score
- `--min-readiness` — minimum acceptable preflight readiness
- `--block-on-regression` — fail if confidence drops compared to the previous run
- `--fail-on-baseline-drift` — fail if the result no longer matches the pinned contract
- `--fail-on-duration-spike true --duration-spike-percent 40` — fail if the rehearsal takes significantly longer than expected

---

## Attach a backup provider

Rehearsa can verify that a real backup snapshot exists — and is recent enough — before each rehearsal. This closes the loop: not just "can this stack restore?" but "can it restore from a backup that actually exists right now?"

> **Prerequisite:** Provider verification calls the `restic` or `borg` binary directly on the host. If your backup tool only runs inside a Docker container, install it on the host too:
> ```bash
> sudo apt install restic      # Debian/Ubuntu
> sudo apt install borgbackup  # for Borg
> ```

```bash
# Register a Restic repository
rehearsa provider add prod-restic \
  --kind restic \
  --repo /mnt/nas/backups/restic \
  --password-env RESTIC_PASSWORD

# Enforce maximum snapshot age
rehearsa provider verify-set prod-restic --max-age-hours 25

# Attach the provider to the watch entry
sudo rehearsa daemon watch myapp /path/to/docker-compose.yml \
  --schedule "0 3 * * *" \
  --provider prod-restic
```

If the provider is unreachable, has no snapshots, or the latest snapshot is too old, the rehearsal is blocked with a clear message. Borg is also supported (`--kind borg`).

---

## Set up notifications

Get notified when something changes — a contract breaks, a provider fails, a stack recovers.

**Webhook (Slack, Discord, ntfy, Gotify, any HTTP endpoint)**
```bash
rehearsa notify add alerts \
  --url https://ntfy.sh/myserver-rehearsa \
  --secret mysecret

rehearsa notify default alerts
```

**Email via SMTP**
```bash
rehearsa notify add-email alerts \
  --from "Rehearsa <alerts@example.com>" \
  --to you@example.com \
  --smtp-host smtp.example.com \
  --smtp-username alerts@example.com \
  --smtp-password-env SMTP_PASSWORD
```

Test the channel before relying on it:
```bash
rehearsa notify test alerts
```

Five event types are delivered: rehearsal fatal error, provider verification failed, policy violation, baseline drift, and rehearsal recovered.

---

## Generate a compliance report

```bash
rehearsa report --stack myapp --format both --output ./reports/
```

Produces a JSON record and a PDF with a verdict banner (PASS / WARN / FAIL), per-service score bars, history trend, baseline contract status, preflight findings, and provider status. The report ID is tamper-evident — it can be handed to an auditor or a client as evidence of tested recoverability.

For a fleet-wide report covering all stacks:
```bash
rehearsa report --format both --output ./reports/
```

---

## Concurrency and CPU usage

By default Rehearsa runs one rehearsal at a time. This is intentional — it keeps CPU usage predictable on low-power hardware (Raspberry Pi, older i3/i5 machines, ARM single-board computers).

On a large fleet with a shared nightly schedule, rehearsals queue and run one after another rather than all firing simultaneously.

If your hardware can handle more, increase the limit:

**Via CLI (bare-metal installs)**
```bash
sudo rehearsa daemon set-concurrency 2
sudo systemctl restart rehearsa
```

**Via environment variable (Docker Compose deploys)**
```yaml
environment:
  - REHEARSA_MAX_CONCURRENT=2
```

**Check what's currently configured**
```bash
rehearsa daemon config
```

Start at 1. If rehearsals are completing without CPU issues, try 2. Each rehearsal can boot 10–15 containers simultaneously, so the real limit is your Docker host's available memory and I/O, not just CPU.

---

## Oneshot services

Migration runners, config appliers, backup scripts, and tools like Recyclarr exit by design. Without a label, Rehearsa scores an exited container as 0. Add this label to tell Rehearsa the exit was intentional:

```yaml
labels:
  com.rehearsa.oneshot: "true"
```

A labelled service scores 100 on any exit — the contract is that it started and ran, not that it succeeded at its task in a simulation environment.

> **Note for Portainer users:** Portainer may strip labels when re-serialising your stack on deploy. If a oneshot service is still scoring 0 after adding the label, verify the label is present in the file on disk with `grep -A3 labels /path/to/docker-compose.yml`.

---

## Typical workflow summary

```
Install Rehearsa
      ↓
rehearsa stack test <compose-file>     # run your first rehearsal
      ↓
rehearsa baseline set <compose-file>   # declare the contract
      ↓
sudo rehearsa daemon install           # install the daemon
sudo rehearsa daemon watch ...         # watch and schedule the stack
      ↓
rehearsa coverage                      # verify the fleet is contracted
      ↓
rehearsa policy set ...                # enforce your standards
```

After that, Rehearsa runs silently in the background. You'll only hear from it when something changes.

---

## Common commands reference

| Command | What it does |
|---|---|
| `rehearsa stack test <file>` | Run a rehearsal |
| `rehearsa baseline set <file>` | Pin the current result as the contract |
| `rehearsa baseline show <stack>` | Inspect the current contract |
| `rehearsa baseline diff <stack>` | Compare latest run to contract |
| `rehearsa baseline auto-init` | Rehearse and contract all watched stacks |
| `rehearsa coverage` | Fleet-wide contract coverage |
| `rehearsa status` | Fleet status table |
| `rehearsa daemon install` | Install the systemd service |
| `rehearsa daemon watch <stack> <file>` | Watch a stack |
| `rehearsa daemon list` | List watched stacks |
| `rehearsa daemon set-concurrency <n>` | Set max simultaneous rehearsals |
| `rehearsa daemon config` | Show daemon configuration |
| `rehearsa policy set <stack>` | Set restore policy |
| `rehearsa provider add <n>` | Register a backup provider |
| `rehearsa provider verify <n>` | Verify provider health |
| `rehearsa notify add <n>` | Register a notification channel |
| `rehearsa report` | Generate a compliance report |
| `rehearsa history show <stack>` | View run history |
| `rehearsa cleanup` | Remove orphaned rehearsal containers |

---

## Getting help

- GitHub: [github.com/rehearsa/rehearsa](https://github.com/rehearsa/rehearsa)
- Issues: [github.com/rehearsa/rehearsa/issues](https://github.com/rehearsa/rehearsa/issues)
- `rehearsa --help` and `rehearsa <command> --help` for full flag reference
