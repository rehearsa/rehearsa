# ── Build stage ──────────────────────────────────────────
FROM rust:1.75-slim AS builder

WORKDIR /build

# Install musl tools for a fully static binary
RUN apt-get update && apt-get install -y musl-tools && rm -rf /var/lib/apt/lists/*

# Cache dependencies before copying source
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs
RUN cargo build --release 2>/dev/null || true
RUN rm -f src/main.rs

# Build the real binary
COPY src ./src
RUN touch src/main.rs && cargo build --release

# ── Final stage ───────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/rehearsa /usr/local/bin/rehearsa

# Config and state directories
VOLUME ["/etc/rehearsa", "/root/.rehearsa"]

# Docker socket must be mounted at runtime:
# -v /var/run/docker.sock:/var/run/docker.sock

ENTRYPOINT ["rehearsa"]
CMD ["daemon", "run"]
