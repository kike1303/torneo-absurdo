---
title: Arquitectura general
type: architecture
status: draft
author: architect
created: 2026-10-20
updated: 2026-10-20
---

# Arquitectura general

> Vista de alto nivel. Las decisiones concretas viven en los ADRs de `docs/decisions/`, y el detalle de cada pieza en la spec de su feature.

## Componentes

```
          navegador (móvil / desktop)
  ┌──────────────────────────────────────────┐
  │ SvelteKit UI ── renderer (PixiJS)        │
  │      │              ▲                    │
  │      │ HTTP         │ FightEvent (WS)    │
  │      │              │ engine-wasm (verif)│
  └──────┼──────────────┼────────────────────┘
         ▼              │
   Traefik (Coolify) ── mismo dominio: /  → web,  /api /ws → server
         │              │
   ┌─────▼─────┐  ┌─────┴──────────────────────────────┐
   │ web (node)│  │ server (Rust/axum)                 │
   │ SSR + i18n│─▶│ auth · apuestas · ledger · rankings│
   └───────────┘  │ scheduler ─▶ engine (crate)        │
                  └─────┬──────────────────────────────┘
                        ▼
                    Postgres
```

## Principios
1. **El servidor Rust es la fuente de verdad** de usuarios, sesiones, monedas, apuestas, peleas y rankings. SvelteKit solo presenta: hace SSR llamando al API y no tiene base de datos propia.
2. **Motor puro**: `engine::simulate(seed, fighter_a, fighter_b, ruleset) -> FightLog`. Sin I/O, sin reloj y determinista en todas las plataformas (nativo y WASM).
3. **Sin video**: el cliente recibe eventos y anima. Se sincroniza con el reloj del servidor (`server_time` y offset).
4. **Sin texto en el dominio**: el motor y el protocolo emiten códigos; la UI y el narrador los traducen (ADR-0007).
5. **Contenido como datos**: personajes y frases en `content/`, versionados y validados en CI.

## Ciclo de vida de una pelea

```
Scheduled ─▶ BettingOpen ─▶ BettingClosed ─▶ Live ─▶ Settled
              │ publica       │ se calcula     │ emite    │ revela la semilla,
              │ SHA-256(seed) │ el FightLog    │ eventos  │ liquida apuestas
              │               │ (no sale aún)  │ según el │ (transacción),
              │               │                │ tiempo   │ actualiza rating
```
- La semilla se genera con un CSPRNG al abrir las apuestas y vive solo en el servidor hasta `Settled`.
- Durante `Live`, cada evento se envía cuando le toca (más un pequeño buffer). Quien entra tarde recibe los eventos ya ocurridos y sigue en vivo.
- Las repeticiones usan el log guardado, y cualquiera puede re-simularlo en el navegador con `engine-wasm` para verificarlo.

## Datos (borrador)
- `users`, `sessions`, `oauth_accounts`
- `ledger_entries` (asientos inmutables), con saldos derivados o materializados con verificación
- `fights` (personajes, ruleset_version, seed_commit, seed —oculta hasta Settled—, resultado, log)
- `bets`, `pools`
- `tournaments`, `tournament_rounds`
- `character_ratings`, `seasons`, `season_stats`

## Tipos compartidos
`crates/protocol` define los mensajes WS y los DTOs del API con serde, y genera TypeScript en `packages/protocol-ts` (con `ts-rs` o similar, se decide en 001).

## Despliegue
Un `infra/docker-compose.yml` con los servicios `web`, `server` y `postgres`, desplegado por Coolify en el VPS. Las imágenes se construyen fuera del VPS (ADR-0006).
