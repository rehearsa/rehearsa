# Rehearsa

> Backups are hope. Rehearsa is proof.

Rehearsa is a restore rehearsal engine for Docker-based self-hosted
environments.

It validates that your docker-compose stacks can actually boot ---
before disaster happens.

------------------------------------------------------------------------

## Why Rehearsa Exists

Most self-hosters run automated backups.

Almost nobody tests restores.

When disaster strikes:

-   Volumes are missing\
-   Databases fail to start\
-   Permissions are wrong\
-   Images are unavailable\
-   Dependencies boot in the wrong order

A backup succeeding does **not** mean a restore will succeed.

Rehearsa exists to close that gap.

------------------------------------------------------------------------

## What Rehearsa Does (v0.1.0)

Rehearsa performs a controlled stack simulation based on a
docker-compose file:

1.  Parses the compose file
2.  Resolves service dependency order
3.  Creates an isolated temporary Docker network
4.  Pulls images (based on policy)
5.  Boots services in dependency order
6.  Waits for container health / running state
7.  Scores each service
8.  Calculates stack confidence, risk, and stability
9.  Cleans up everything (containers + network)

No changes are made to your live stack.

------------------------------------------------------------------------

## Core Principle

Rehearsa is **not** a backup tool.\
It is a **restore validation engine**.

It answers one critical question:

> If I had to rebuild this stack on a fresh host --- would it boot?

------------------------------------------------------------------------

## Example Usage

``` bash
rehearsa --timeout 120 stack test arrstack.yml
```

Then view results:

``` bash
rehearsa status
```

Example output:

    Rehearsa Status
    ────────────────────────────────────────────────────────

    Stack                Confidence   Risk        Stability    Trend
    ────────────────────────────────────────────────────────
    arrstack             81%          MODERATE    81%          →
    heavy-stack          95%          LOW         95%          →
    unhealthy-stack      40%          HIGH        40%          →

------------------------------------------------------------------------

## Scoring Model

Each service receives a score based on runtime state:

-   100 --- Running and healthy\
-   85 --- Running (no healthcheck)\
-   40 --- Running but unhealthy\
-   0 --- Exited or failed

Stack confidence is the average of all service scores.

Rehearsa also tracks:

-   Regression trends (UP / DOWN / SAME)
-   Stability (rolling average of last 5 runs)
-   Duration spikes
-   Policy violations

------------------------------------------------------------------------

## Policy Engine

Rehearsa supports optional enforcement policies per stack:

-   Minimum confidence threshold
-   Block on regression
-   Fail on duration spike
-   Custom duration spike percentage

Example:

``` bash
rehearsa policy set arrstack \
  --min-confidence 80 \
  --block-on-regression true \
  --fail-on-duration-spike true \
  --duration-spike-percent 40
```

If a policy is violated, Rehearsa exits with a non-zero code --- making
it CI/CD compatible.

------------------------------------------------------------------------

## Design Goals

-   Agentless (Docker socket only)
-   Fully isolated network simulation
-   No modification of live containers
-   Deterministic cleanup
-   Clear scoring and trend visibility
-   CI-friendly exit codes
-   Rust-first implementation

------------------------------------------------------------------------

## What Rehearsa Is Not

-   Not a backup tool
-   Not a monitoring tool
-   Not a container orchestrator
-   Not a live migration system

It does one thing:

Validate whether your stack would successfully start.

------------------------------------------------------------------------

## Current Status

Stable v0.1.0 release.

Core features:

-   Stack simulation
-   Dependency ordering
-   Health-aware scoring
-   Stability engine
-   Regression detection
-   Policy enforcement
-   Clean CLI status dashboard

Planned future direction:

-   Volume validation
-   Mount inspection
-   Network mapping verification
-   Cross-host restore simulation
-   Weighted reliability scoring

------------------------------------------------------------------------

## Contributing

Rehearsa is actively evolving.

If you would like to contribute, open an issue to discuss scope and
architectural alignment first.

------------------------------------------------------------------------

## License

MIT
