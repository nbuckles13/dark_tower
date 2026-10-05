# ========================================
# Migrations image (ADR-0038 §2 step 3)
# ========================================
#
# sqlx-cli plus the tree's `migrations/`, run in-cluster as the `db-migrate`
# Job (infra/services/db-migrate/) BEFORE the environment root is applied.
# Built by infra/kind/scripts/deploy.sh, which tags it by its own image ID
# (content_tag), like every first-party image.
#
# Build context: the repo root (the service images' context and .dockerignore).
# Build args:
#   SQLX_CLI_VERSION  REQUIRED. Derived by deploy.sh from Cargo.lock's `sqlx`
#                     (infra/lib/cargo-lock-version.sh) so the CLI writing
#                     `_sqlx_migrations` matches the library reading it.
#   RUST_VERSION      REQUIRED (no default). The repo's ONE Rust version, derived by
#                     deploy.sh from rust-toolchain.toml (infra/lib/rust-toolchain.sh).
#                     Unset yields `rust:-slim-bookworm`, an invalid reference.
#
# Layer order is load-bearing: `migrations/` is the LAST layer, so a changed
# migration rebuilds only that layer (sqlx-cli stays cached) and changes the
# image ID — hence the tag, hence the Job name — while an unchanged set keeps all
# three, and the Job apply is a no-op.

# Builder and runtime are the SAME Debian release (bookworm = debian12): the
# binary links the builder's glibc, and a newer builder than runtime fails at
# exec with "GLIBC_2.xx not found" (seen at ADR-0038 devloop 2 Gate 2, when the
# floating `-slim` tag had moved to trixie). setup.test.sh fails on any drift.
ARG RUST_VERSION
FROM docker.io/library/rust:${RUST_VERSION}-slim-bookworm AS builder

ARG SQLX_CLI_VERSION
RUN test -n "${SQLX_CLI_VERSION}" || { echo "SQLX_CLI_VERSION build-arg is required (derived from Cargo.lock by infra/kind/scripts/deploy.sh)" >&2; exit 1; }

WORKDIR /build
# The cargo parallelism cap (build.jobs) lives in .cargo/config.toml; copy it in
# before the compile so `cargo install` (run from this directory) honours it.
COPY .cargo/ .cargo/

# `postgres` only: the in-cluster Postgres is plaintext, and no TLS backend
# means no libssl dependency in the distroless runtime.
RUN cargo install sqlx-cli --locked --no-default-features --features postgres \
        --version "=${SQLX_CLI_VERSION}" --root /out

# ========================================
# Runtime
# ========================================
# Tag, not digest: the image's tag is its own ID, so an upstream change to this
# base produces a new tag and a new Job name — a visible, pending-only re-run,
# never a silent substitution.
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime

LABEL org.opencontainers.image.title="Dark Tower DB migrations" \
      org.opencontainers.image.description="sqlx-cli + migrations/, run as the db-migrate Job"

COPY --from=builder /out/bin/sqlx /usr/local/bin/sqlx
# --chown: the runtime user must be able to read the files whatever their mode on the
# build host (git does not track read bits, so a host checkout can hold 0600 files that
# a fresh clone writes 0644). Same pattern as the service images' /migrations copy.
COPY --chown=nonroot:nonroot migrations/ /migrations/

# Plain `migrate run`: pending-only via `_sqlx_migrations`, and it FAILS on a
# changed applied migration, a dirty row, or an applied version missing from
# /migrations. No --ignore-missing / --dry-run / --target-version, ever.
# Connection: DATABASE_URL (host/port only) + PGUSER/PGPASSWORD/PGDATABASE from
# the Job's env (see infra/services/db-migrate/job.yaml).
ENTRYPOINT ["/usr/local/bin/sqlx", "migrate", "run", "--source", "/migrations", "--connect-timeout", "30"]
