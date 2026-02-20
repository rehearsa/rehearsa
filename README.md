# Rehearsa

> Backups are hope. Rehearsa is proof.

Rehearsa is a restore rehearsal engine for Docker-based self-hosted environments.

It validates that your services can actually boot from restored data — before disaster happens.

---

## Why Rehearsa Exists

Most self-hosters run automated backups.

Almost nobody tests restores.

When disaster strikes:

- Volumes are missing  
- Databases fail to start  
- Permissions are wrong  
- Secrets weren’t backed up  
- Restore order is incorrect  

A backup succeeding does not mean a restore will succeed.

Rehearsa exists to close that gap.

---

## What Rehearsa Does

Rehearsa performs a controlled restore simulation:

1. Extracts container configuration (image, env, volumes, healthchecks)
2. Clones or restores data into temporary volumes
3. Creates an isolated Docker network
4. Boots the service in a sandbox
5. Validates that it starts successfully
6. Reports the result
7. Cleans up everything

No changes are made to your live stack.

---

## Core Principle

Rehearsa is **not** a backup tool.

It is a **restore validation engine**.

It answers one critical question:

> Can this service actually boot from backup data?

---

## Planned Development Phases

### Phase C — Simulated Restore (v0.1)

- Clone live volumes
- Boot in isolated sandbox
- Validate container state and healthcheck
- Deterministic teardown

### Phase A — Volume Restore

- Restore from tar / snapshot
- Validate restored state

### Phase B — Backup Integration

- Restic support
- Borg support
- ZFS snapshot support
- Snapshot validation scoring

---

## Example (Planned CLI)

```bash
rehearsa test jellyfin
```

Example output:

```
✔ Cloning volume...
✔ Creating sandbox network...
✔ Booting container...
✔ Healthcheck passed

Restore Simulation: SUCCESS
Time: 18.4s
```

---

## Design Goals

- Agentless (Docker socket only)
- Fully isolated sandbox execution
- Zero impact on live services
- Deterministic cleanup
- Minimal dependencies
- Rust-first implementation
- Clear, structured architecture

---

## Status

Early development.

v0.1 will validate boot capability using cloned volumes.

---

## Contributing

Rehearsa is in early architectural development.

If you would like to contribute, please open an issue first to discuss scope and approach.

---

## License

MIT (to be finalized)
