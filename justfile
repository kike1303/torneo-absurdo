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
    (set -a; . apps/server/.env; set +a; exec cargo run -p server --features dev-auth --locked) &
    pnpm --filter web dev &
    wait

# Applies to every worktree of the clone. The hook checks the working tree and needs Docker.
# Point git at scripts/git-hooks: the pre-push hook runs `just check` (RF-C17)
hooks:
    git config core.hooksPath scripts/git-hooks

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
    pnpm --filter @torneo/renderer exec tsc -p tsconfig.json --noEmit
    pnpm --filter @torneo/renderer exec tsc -p tsconfig.node.json --noEmit
    pnpm --filter @torneo/renderer exec eslint .
    pnpm --filter @torneo/renderer exec vitest run
    pnpm --filter @torneo/sff-converter exec tsc -p tsconfig.json --noEmit
    pnpm --filter @torneo/sff-converter exec eslint .
    pnpm --filter @torneo/sff-converter exec vitest run
    pnpm --filter @torneo/sprite-importer exec tsc -p tsconfig.json --noEmit
    pnpm --filter @torneo/sprite-importer exec eslint .
    pnpm --filter @torneo/sprite-importer exec vitest run
    pnpm -s --filter @torneo/sprite-importer check
    pnpm exec vitest run --config scripts/vitest.config.mjs
    node scripts/check-tokens.mjs
    node scripts/check-i18n.mjs
    node scripts/check-prerender.mjs
    node scripts/check-kit-config.mjs
    node scripts/check-compose.mjs
    node scripts/check-workflows.mjs

# The Rust half of `check`: fmt, clippy, engine purity and WASM (no Docker), tests (with
# Postgres), freshness of `gen`, ts-rs graph
check-rust:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets --locked -- -D warnings
    cargo clippy -p protocol --features ts --all-targets --locked -- -D warnings
    cargo clippy -p server --features dev-auth --all-targets --locked -- -D warnings
    {{just_executable()}} _check-engine
    {{just_executable()}} _check-wasm
    {{just_executable()}} db-up
    {{just_executable()}} _test-rust
    {{just_executable()}} _check-gen
    {{just_executable()}} _check-ts-rs-graph

# Unit and integration tests (web, renderer, SFF converter, sprite importer, repo scripts and Rust)
test: _web-prepare
    pnpm --filter web exec vitest run
    pnpm --filter @torneo/renderer exec vitest run
    pnpm --filter @torneo/sff-converter exec vitest run
    pnpm --filter @torneo/sprite-importer exec vitest run
    pnpm -s --filter @torneo/sprite-importer check
    pnpm exec vitest run --config scripts/vitest.config.mjs
    {{just_executable()}} db-up
    {{just_executable()}} _test-rust

# The Rust tests with Postgres (008 plan-b decision 15): the workspace without `dev-auth` (the
# production cookie policy and routes), the server with it, then the A9 check of the Postgres
# log of the run.
_test-rust:
    #!/usr/bin/env bash
    set -euo pipefail
    # Stale canaries of earlier runs must not count: each run of tests/pg_log.rs writes its own.
    rm -f target/tmp/pglog-canary-*.txt
    # The clock of the Postgres container, not the host's: `logs --since` filters by the daemon's
    # clock, which drifts inside a VM (Colima, Docker Desktop).
    since="$(docker compose -f infra/compose.dev.yml exec -T postgres date -u +%Y-%m-%dT%H:%M:%SZ)"
    DATABASE_URL='{{test_db_url}}' cargo test --workspace --locked
    DATABASE_URL='{{test_db_url}}' cargo test -p server --features dev-auth --locked
    {{just_executable()}} _check-pg-log "$since"

# ADR-0012 A9: the canary of tests/pg_log.rs never reaches the Postgres log, and the log of the
# run has at least one ERROR line (positive control, so an empty log never passes)
_check-pg-log since:
    #!/usr/bin/env bash
    set -euo pipefail
    shopt -s nullglob
    files=(target/tmp/pglog-canary-*.txt)
    if [ "${#files[@]}" -eq 0 ]; then
        echo "check-pg-log: no canary (did apps/server/tests/pg_log.rs run?)" >&2
        exit 1
    fi
    logs="$(docker compose -f infra/compose.dev.yml logs --no-log-prefix --since "{{since}}" postgres 2>&1)"
    for file in "${files[@]}"; do
        canary="$(cat "$file")"
        if [ -z "$canary" ]; then
            echo "check-pg-log: an empty canary file" >&2
            exit 1
        fi
        if grep -qF "$canary" <<<"$logs"; then
            echo "check-pg-log: a value of a failed statement reached the Postgres log" >&2
            exit 1
        fi
    done
    if ! grep -q ' ERROR: ' <<<"$logs"; then
        echo "check-pg-log: no ERROR line in the Postgres log of the run (positive control)" >&2
        exit 1
    fi
    echo "check-pg-log: ok"

# Needs Docker (Postgres) and compiles the server with dev-auth (008 RF-D7, plan-d decision 10).
# Everything listens on 127.0.0.1 and the browsers go out only through the egress proxy.
# End-to-end tests against `node build`, the server and an edge proxy (Chromium and WebKit)
e2e: _web-prepare db-up
    cargo build -p server --features dev-auth --locked
    E2E_DATABASE_URL='postgres://torneo:torneo_dev@127.0.0.1:{{pg_port}}/torneo_e2e' \
        E2E_SERVER_BIN='{{justfile_directory()}}/target/debug/server' \
        {{just_executable()}} _e2e-run

# Without Docker (the CI job `e2e` runs it with its own Postgres service and the server binary
# of `e2e-server`). Needs E2E_DATABASE_URL and an absolute E2E_SERVER_BIN built with dev-auth;
# E2E_APPLY_A9=1 applies the A9 parameters first (CI only, never on loopback).
# The e2e with an existing server binary and Postgres: build, checks, torneo_e2e and Playwright
_e2e-run: _web-prepare
    #!/usr/bin/env bash
    set -euo pipefail
    : "${E2E_DATABASE_URL:?E2E_DATABASE_URL is required}"
    : "${E2E_SERVER_BIN:?E2E_SERVER_BIN is required}"
    case "$E2E_SERVER_BIN" in
        /*) ;;
        *) echo "E2E_SERVER_BIN must be an absolute path (Playwright runs in apps/web)" >&2; exit 1 ;;
    esac
    # Fails fast if the binary is not the dev-auth one (008 RF-D9).
    node scripts/check-release-binary.mjs --expect-dev "$E2E_SERVER_BIN"
    # Once per run, here and never in playwright.config.ts (evaluated again in every worker).
    # node:crypto, because the Playwright image does not promise openssl.
    secret() { node -e "process.stdout.write(require('node:crypto').randomBytes(32).toString('base64url'))"; }
    E2E_INTERNAL_API_SECRET="$(secret)"
    E2E_IP_HASH_KEY="$(secret)"
    export E2E_INTERNAL_API_SECRET E2E_IP_HASH_KEY E2E_DATABASE_URL E2E_SERVER_BIN
    pnpm --filter web build
    node scripts/check-renderer-split.mjs
    node scripts/check-no-local-sprites.mjs
    node apps/web/e2e/support/db.mjs reset
    if [ "${E2E_APPLY_A9:-}" = 1 ]; then node apps/web/e2e/support/db.mjs a9 --apply; fi
    node apps/web/e2e/support/db.mjs a9 --check
    # A leftover of an earlier local run must not declare hosts or canaries for this one, and an
    # old egress.log must not stand in for a proxy that did not start.
    node apps/web/e2e/support/reset-run.mjs
    pnpm --filter web exec playwright test

# HOST is fixed to 127.0.0.1 (never read from the environment): adapter-node listens on 0.0.0.0 by
# default, and SOCKET_PATH and LISTEN_FDS, which it prefers over HOST, are cleared. On Android,
# forward the port with chrome://inspect. Method: packages/renderer/README.md.
# The production build of /dev/arena on 127.0.0.1:3000, to measure fps (002 plan-a2 decision 12)
arena-bench: build
    #!/usr/bin/env bash
    set -euo pipefail
    if (exec 3<>/dev/tcp/127.0.0.1/3000) 2>/dev/null; then
        echo "127.0.0.1:3000 is in use: stop whatever listens there first" >&2
        exit 1
    fi
    echo "arena-bench: http://localhost:3000/es/dev/arena (Ctrl-C to stop)"
    exec env -u HOST -u SOCKET_PATH -u LISTEN_FDS HOST=127.0.0.1 PORT=3000 \
        PUBLIC_ORIGIN=http://localhost:3000 ENABLE_DEV_CATALOG=1 node apps/web/build

# Install the Playwright browsers used by `just e2e`
e2e-install:
    pnpm --filter web exec playwright install chromium webkit

# Never pass the seed of a real fight. Defaults: 10000 fights, fixed seed, content/characters.
# Mass fight simulation and balance report (003 RF15): just sim [--fights N] [--seed HEX] [--content DIR] [--json] [--check]
[positional-arguments]
sim *args:
    cargo run -p sim --release --locked -- "$@"

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

# RF5, RF19, RF20: engine graph and features (alone and as built by every workspace member that
# depends on it, except the server), server without sim (ADR-0002), engine-wasm unsafe and lints
_check-engine:
    node scripts/check-engine.mjs

# RF18: engine and bindings compile for the browser; the same goldens pass in WASI.
# --nocapture: under panic = abort a failure is a trap, so the panic message must not be captured.
_check-wasm:
    #!/usr/bin/env bash
    set -euo pipefail
    for target in wasm32-unknown-unknown wasm32-wasip1; do
        if ! rustup target list --installed | grep -qx "$target"; then
            echo "missing target $target: run \`rustup toolchain install\` in the repo" >&2
            exit 1
        fi
    done
    cargo clippy -p engine -p engine-wasm --target wasm32-unknown-unknown --locked -- -D warnings
    CARGO_TARGET_WASM32_WASIP1_RUNNER="node --disable-warning=ExperimentalWarning {{justfile_directory()}}/scripts/wasi-runner.mjs" \
        cargo test -p engine --test golden --target wasm32-wasip1 --locked -- --nocapture

# RF-B3: ts-rs never in the server's normal graph; protocol without features has no ts-rs nor tokio.
# 003: neither in engine, engine-wasm, content nor sim (they depend on protocol without `ts`).
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
    for crate in engine engine-wasm content sim; do
        if cargo tree -p "$crate" -e normal --locked --prefix none | grep -qE '^(ts-rs|tokio) '; then
            echo "$crate pulls ts-rs or tokio (003 RF5)" >&2
            exit 1
        fi
    done
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

# DATABASE_URL, PUBLIC_HOST and the secrets of the shell are dropped: only infra/local.env and the
# generated infra/local.secrets.env feed the compose.
# Networks and `compose up`, without the preflight (shared by prod-local and _prod-local-up)
_prod-local-start:
    #!/usr/bin/env bash
    set -euo pipefail
    : "${IMAGE_TAG:?IMAGE_TAG is required}"
    bash scripts/local-networks.sh up
    # The two secrets of the server and the four OAuth fillers, generated once into a file git
    # ignores (008 plan-b decision 6, plan-c decision 3).
    bash scripts/local-secrets.sh
    env -u DATABASE_URL -u PUBLIC_HOST -u INTERNAL_API_SECRET -u IP_HASH_KEY \
        -u GOOGLE_CLIENT_ID -u GOOGLE_CLIENT_SECRET -u DISCORD_CLIENT_ID -u DISCORD_CLIENT_SECRET \
        IMAGE_TAG="$IMAGE_TAG" \
        docker compose -p torneo-local \
        --env-file infra/local.env --env-file infra/local.secrets.env \
        -f infra/docker-compose.yml -f infra/compose.local.yml \
        up -d --wait --wait-timeout 180
    docker compose -p torneo-local ps

# Keeps the Postgres volume. Never removes a network without the torneo.local=1 label.
# Stop `prod-local` and remove the networks it created
prod-local-down:
    env -u DATABASE_URL -u PUBLIC_HOST IMAGE_TAG=unused INTERNAL_API_SECRET=unused IP_HASH_KEY=unused \
        GOOGLE_CLIENT_ID=unused GOOGLE_CLIENT_SECRET=unused \
        DISCORD_CLIENT_ID=unused DISCORD_CLIENT_SECRET=unused \
        docker compose -p torneo-local \
        --env-file infra/local.env -f infra/docker-compose.yml -f infra/compose.local.yml down
    bash scripts/local-networks.sh down
