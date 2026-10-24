---
title: Arquitectura general
type: architecture
status: draft
author: architect
created: 2026-10-20
updated: 2026-10-24
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
`crates/protocol` define los mensajes WS y los DTOs del API con serde. Es una librería pura: sin I/O, sin `tokio`, sin reloj y sin texto para humanos (ADR-0007).

- **Generación:** `ts-rs`, opcional detrás de la feature `ts` para que el server no lo compile. `just gen` vacía `packages/protocol-ts/src/generated/`, exporta un `.ts` por tipo (imports con `.js`), les antepone la cabecera de "generado, no editar", escribe el barril `src/index.ts` (`export type *`) y formatea con Prettier. Los archivos generados se commitean, y `just check` falla si están desactualizados. Nunca se editan a mano.
- **Paquete:** `@torneo/protocol` (`packages/protocol-ts`) exporta el TS fuente, sin paso de build. `apps/web` lo consume con `import type`.
- **Convenciones (RF-B4):**
  - Enums con datos: `#[serde(tag = "type", rename_all = "snake_case")]` (o el campo de etiqueta que corresponda, como `status` en `HealthResponse`).
  - Códigos de error: `dominio.motivo` (`ws.origin_forbidden`), como enums con `#[serde(rename = "dominio.motivo")]` en cada variante, nunca `String` libre. Cada enum de códigos es dueño de un solo prefijo (`ApiErrorCode` → `http.*`, `WsErrorCode` → `ws.*`) y expone `ALL` y `as_str()`.
  - `ErrorCode` es la unión sin etiquetar de los enums de códigos y `ApiError { error: ErrorCode }` es el cuerpo de toda respuesta HTTP de error. Un dominio nuevo (por ejemplo `auth.*`) agrega su enum y su variante en `ErrorCode`; los conjuntos de cadenas no se solapan (lo comprueba un test).
  - Nada de enteros de 64 bits o más (`u64`, `i64`, `u128`, `i128`, `usize`) en tipos exportados: `just gen` falla si aparece `bigint`. Si hace falta, se serializan como cadena.

## Despliegue
Un `infra/docker-compose.yml` con los servicios `web`, `server` y `postgres`, desplegado por Coolify en el VPS. Las imágenes se construyen fuera del VPS (ADR-0006).
