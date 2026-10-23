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
    pnpm --filter web exec node --input-type=module -e "import { compile } from '@inlang/paraglide-js'; import options from './paraglide.options.js'; await compile(options);"
    pnpm --filter web exec svelte-kit sync
