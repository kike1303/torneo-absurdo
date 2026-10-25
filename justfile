# Torneo Absurdo — task runner. Requires Node >= 22.17 with corepack enabled (`corepack enable`),
# the Rust toolchain of rust-toolchain.toml (rustup installs it) and, since phase B, Docker.

set shell := ["bash", "-euo", "pipefail", "-c"]

pg_port := env("DEV_PG_PORT", "5432")
# Only the test recipes use this default; it is never exported globally (plan-b, devops note 5).
test_db_url := env("DATABASE_URL", "postgres://torneo:torneo_dev@127.0.0.1:" + pg_port + "/torneo")

# List the available recipes
default:
    @just --list

# Install JS dependencies exactly as locked
install:
    pnpm install --frozen-lockfile

# Ctrl-C stops the server and the web; Postgres stays up (`just db-down`).
# Needs apps/server/.env and apps/web/.env (copy the .env.example files).
# Postgres, the server and the web in dev mode
dev: _web-prepare db-up
    #!/usr/bin/env bash
    # bash 3.2 compatible (macOS): no `wait -n` (plan-b decision 24).
    set -euo pipefail
    for file in apps/server/.env apps/web/.env; do
        if [ ! -f "$file" ]; then
            echo "Missing $file: copy $file.example" >&2
            exit 1
        fi
    done
    trap 'trap - INT TERM; kill 0' INT TERM
    # The server .env is loaded only in this subshell: DATABASE_URL is never exported globally.
    (set -a; . apps/server/.env; set +a; exec cargo run -p server --locked) &
    pnpm --filter web dev &
    wait

# Production build of the web app
build: _web-prepare
    pnpm --filter web build

# Format everything Prettier owns (and Rust)
fmt:
    pnpm exec prettier --write .
    cargo fmt --all

# Needs Docker since phase B (the Rust tests use the dev Postgres).
# Format, lint, types, unit and integration tests and repo checks; stops at the first failure
check: check-web check-rust

# The web half of `check` (no Docker needed)
check-web: _web-prepare
    pnpm exec prettier --check .
    pnpm --filter web exec eslint .
    pnpm --filter web exec svelte-check --fail-on-warnings
    pnpm --filter web exec vitest run
    pnpm exec vitest run --config scripts/vitest.config.mjs
    node scripts/check-tokens.mjs
    node scripts/check-i18n.mjs
    node scripts/check-prerender.mjs
    node scripts/check-kit-config.mjs
    node scripts/check-compose.mjs

# The Rust half of `check`: fmt, clippy, tests (with Postgres), freshness of `gen`, ts-rs graph
check-rust:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets --locked -- -D warnings
    cargo clippy -p protocol --features ts --all-targets --locked -- -D warnings
    {{just_executable()}} db-up
    DATABASE_URL='{{test_db_url}}' cargo test --workspace --locked
    {{just_executable()}} _check-gen
    {{just_executable()}} _check-ts-rs-graph

# Unit and integration tests (web, repo scripts and Rust)
test: _web-prepare
    pnpm --filter web exec vitest run
    pnpm exec vitest run --config scripts/vitest.config.mjs
    {{just_executable()}} db-up
    DATABASE_URL='{{test_db_url}}' cargo test --workspace --locked

# End-to-end tests against `node build` (Chromium and WebKit)
e2e: _web-prepare
    pnpm --filter web build
    pnpm --filter web exec playwright test

# Install the Playwright browsers used by `just e2e`
e2e-install:
    pnpm --filter web exec playwright install chromium webkit

# Generated code the tools need (Kit types, compiled Paraglide messages). Does not touch versioned files.
_web-prepare:
    pnpm --filter web run sync

# Uses the network (one request per second), so it is not part of `check`.
# Run it on every Cargo.lock change, before building (plan-b decision 16).
# Every Cargo.lock version must be at least 3 days old on crates.io
crate-age:
    node scripts/check-crate-age.mjs

# cargo deny check bans sources (RF-B2). Always in CI; locally only if cargo-deny is installed.
deny:
    #!/usr/bin/env bash
    set -euo pipefail
    if command -v cargo-deny >/dev/null 2>&1; then
        cargo deny check bans sources
    elif [ -n "${CI:-}" ]; then
        echo "cargo-deny is required in CI" >&2
        exit 1
    else
        echo "cargo-deny is not installed, skipping (cargo install --locked cargo-deny@0.20.2)"
    fi

# Empties generated/ first: ts-rs never deletes the file of a removed type.
# Generate the TypeScript types of crates/protocol into packages/protocol-ts (RF-B6)
gen:
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{justfile_directory()}}/packages/protocol-ts/src/generated"
    mkdir -p "$out"
    rm -f "$out"/*.ts
    env -u TS_RS_LARGE_INT TS_RS_EXPORT_DIR="$out" TS_RS_IMPORT_EXTENSION=js \
        cargo test -p protocol --features ts --locked --quiet export_bindings >/dev/null
    node scripts/gen-protocol-ts.mjs packages/protocol-ts/src
    pnpm exec prettier --write --log-level warn packages/protocol-ts/src

# Freshness of `just gen` against the working tree, without touching the git index (plan-b decision 21)
_check-gen:
    #!/usr/bin/env bash
    set -euo pipefail
    src=packages/protocol-ts/src
    backup="$(mktemp -d)"
    trap 'rm -rf "$backup"' EXIT
    cp -R "$src/." "$backup/"
    set +e
    {{just_executable()}} gen
    status=$?
    set -e
    if [ "$status" -ne 0 ]; then
        rm -rf "$src"
        mkdir -p "$src"
        cp -R "$backup/." "$src/"
        node scripts/compare-trees.mjs --gen-failed || true
        exit "$status"
    fi
    node scripts/compare-trees.mjs "$backup" "$src"

# RF-B3: ts-rs never in the server's normal graph; protocol without features has no ts-rs nor tokio
_check-ts-rs-graph:
    #!/usr/bin/env bash
    set -euo pipefail
    if cargo tree -p server -e normal --locked --prefix none | grep -qE '^ts-rs '; then
        echo "ts-rs is in the normal dependency graph of server (RF-B3)" >&2
        exit 1
    fi
    if cargo tree -p protocol -e normal --locked --prefix none | grep -qE '^(ts-rs|tokio) '; then
        echo "protocol pulls ts-rs or tokio without the ts feature (RF-B3)" >&2
        exit 1
    fi
    echo "check-ts-rs-graph: ok"

# Start the development Postgres (infra/compose.dev.yml) and wait until it is healthy. Idempotent.
db-up:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! docker info >/dev/null 2>&1; then
        echo 'Docker is not responding: `just check` needs Docker since phase B' >&2
        exit 1
    fi
    docker compose -f infra/compose.dev.yml up -d --wait

# Stop the development Postgres (keeps the volume)
db-down:
    docker compose -f infra/compose.dev.yml down

# Drop the development Postgres volume (and any leftover torneo_test_* databases) and start again
db-reset:
    docker compose -f infra/compose.dev.yml down -v
    {{just_executable()}} db-up

# Needs Docker. Never opens nor changes an existing .env (plan-c decision 11).
# Prove with Docker that .dockerignore keeps .env files, .git and build outputs out of the context
check-dockerignore:
    bash scripts/check-dockerignore.sh

# Fails early if 127.0.0.1:3000 or :8080 is taken (`just dev` uses 8080) and shows the daemon in use
_prod-local-preflight:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "docker context: $(docker context show)"
    if ! docker info >/dev/null 2>&1; then
        echo "Docker is not responding" >&2
        exit 1
    fi
    for port in 3000 8080; do
        if (exec 3<>"/dev/tcp/127.0.0.1/$port") 2>/dev/null; then
            echo "127.0.0.1:$port is in use: stop \`just dev\` or run \`just prod-local-down\`" >&2
            exit 1
        fi
    done

# Without a tag, builds the images locally as :local; with a tag (vX.Y.Z or sha-<7>), pulls them.
# Needs Docker with amd64 emulation (Colima or Docker Desktop with Rosetta). Stop with prod-local-down.
# The production images with the production compose, plus a local Postgres, on 127.0.0.1
[positional-arguments]
prod-local tag="": _prod-local-preflight
    #!/usr/bin/env bash
    set -euo pipefail
    tag="${1:-}"
    repo=ghcr.io/kike1303/torneo-absurdo
    if [ -z "$tag" ]; then
        tag=local
        for svc in server web; do
            docker buildx build --platform linux/amd64 -f "apps/$svc/Dockerfile" \
                -t "$repo/$svc:local" --load .
        done
    elif [[ "$tag" =~ ^(v[0-9]+\.[0-9]+\.[0-9]+|sha-[0-9a-f]{7})$ ]]; then
        for svc in server web; do
            docker pull --platform linux/amd64 "$repo/$svc:$tag"
        done
    else
        echo "tag must be vX.Y.Z or sha-<7 hex>" >&2
        exit 1
    fi
    # The preflight already ran (before the build), so go straight to the start.
    IMAGE_TAG="$tag" {{just_executable()}} _prod-local-start

# Start the stack with an existing IMAGE_TAG, without building (also used by the CI job `images`)
_prod-local-up: _prod-local-preflight
    {{just_executable()}} _prod-local-start

# DATABASE_URL and PUBLIC_HOST of the shell are dropped: only infra/local.env feeds the compose.
# Networks and `compose up`, without the preflight (shared by prod-local and _prod-local-up)
_prod-local-start:
    #!/usr/bin/env bash
    set -euo pipefail
    : "${IMAGE_TAG:?IMAGE_TAG is required}"
    bash scripts/local-networks.sh up
    env -u DATABASE_URL -u PUBLIC_HOST IMAGE_TAG="$IMAGE_TAG" docker compose -p torneo-local \
        --env-file infra/local.env -f infra/docker-compose.yml -f infra/compose.local.yml \
        up -d --wait --wait-timeout 180
    docker compose -p torneo-local ps

# Keeps the Postgres volume. Never removes a network without the torneo.local=1 label.
# Stop `prod-local` and remove the networks it created
prod-local-down:
    env -u DATABASE_URL -u PUBLIC_HOST IMAGE_TAG=unused docker compose -p torneo-local \
        --env-file infra/local.env -f infra/docker-compose.yml -f infra/compose.local.yml down
    bash scripts/local-networks.sh down
