# Torneo Absurdo — task runner. Requires Node >= 22.17 with corepack enabled (`corepack enable`).

set shell := ["bash", "-euo", "pipefail", "-c"]

# List the available recipes
default:
    @just --list

# Install JS dependencies exactly as locked
install:
    pnpm install --frozen-lockfile

# Run the web app in dev mode (phase A: web only). Needs apps/web/.env (see .env.example)
dev: _web-prepare
    pnpm --filter web dev

# Production build of the web app
build: _web-prepare
    pnpm --filter web build

# Format everything Prettier owns
fmt:
    pnpm exec prettier --write .

# Format, lint, types, unit tests and repo checks; stops at the first failure
check: _check-web

_check-web: _web-prepare
    pnpm exec prettier --check .
    pnpm --filter web exec eslint .
    pnpm --filter web exec svelte-check --fail-on-warnings
    pnpm --filter web exec vitest run
    pnpm exec vitest run --config scripts/vitest.config.mjs
    node scripts/check-tokens.mjs
    node scripts/check-i18n.mjs
    node scripts/check-prerender.mjs
    node scripts/check-kit-config.mjs

# Unit tests (web and repo scripts)
test: _web-prepare
    pnpm --filter web exec vitest run
    pnpm exec vitest run --config scripts/vitest.config.mjs

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

# Every Cargo.lock version must be >= 3 days old on crates.io (plan-b decision 16). Uses the
# network (one request per second), so it is not part of `check`. Run it on every lock change.
crate-age:
    node scripts/check-crate-age.mjs

# Generate the TypeScript types of crates/protocol into packages/protocol-ts (RF-B6).
# Empties generated/ first: ts-rs never deletes the file of a removed type.
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
