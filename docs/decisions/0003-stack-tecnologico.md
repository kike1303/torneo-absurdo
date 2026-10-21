---
id: "0003"
title: Stack tecnológico y monorepo
type: adr
status: approved
risk: medium
author: owner
approvals:
  - by: owner
    date: 2026-10-20
created: 2026-10-20
updated: 2026-10-20
---

# ADR-0003: Stack tecnológico y monorepo

## Decisión
| Pieza | Elección |
|---|---|
| Motor | Rust (`crates/engine`), con WASM (`crates/engine-wasm`) para verificar en el cliente |
| Servidor | Rust + axum + tokio (HTTP, WebSockets, scheduler) |
| Base de datos | Postgres con `sqlx` y migraciones versionadas |
| Tipos compartidos | `crates/protocol` (serde), que genera TS en `packages/protocol-ts` |
| Frontend | SvelteKit (adapter-node, SSR) con TypeScript estricto |
| Renderizado | PixiJS en `packages/renderer`, agnóstico de framework |
| Monorepo | Cargo workspace + pnpm workspaces, con `just` como task runner único |
| Calidad | rustfmt, clippy, cargo test (+ proptest en el motor); ESLint, Prettier, svelte-check, Vitest, Playwright |

## Por qué
- Un solo lenguaje en el backend (el motor tiene que ser Rust por determinismo y WASM).
- Rust en runtime consume muy poca memoria, y el VPS tiene 4 GB compartidos con Coolify.
- PixiJS solo dibuja: no necesitamos la física, las escenas ni el input de Phaser, porque reproducimos un log.
- SvelteKit en vez de Astro: la app es muy interactiva y en tiempo real.

## Alternativas descartadas
- NestJS en el backend: habría dos lenguajes en el backend.
- Phaser: es más pesado de lo necesario.
