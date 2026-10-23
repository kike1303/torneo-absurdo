# Torneo Absurdo — guía para Claude

Webapp donde personajes "homenaje" (arquetipos inspirados en personajes icónicos, nunca copias) pelean en batallas simuladas por nuestro propio motor determinista, en vivo y sincronizadas para todos los espectadores, mientras los usuarios apuestan monedas ficticias. Visión completa: [docs/vision.md](docs/vision.md).

## Idioma y convenciones
- **Docs en español.** Código, identificadores, commits, ramas y comentarios de código en inglés.
- **i18n desde cero (en, es):** nunca strings literales visibles en la UI. Ver [ADR-0007](docs/decisions/0007-i18n-desde-cero.md).
- El motor y el protocolo **no contienen texto para humanos**, solo códigos/llaves.
- Commits: Conventional Commits con ID de Linear → `feat(engine): add crit roll (TA-12)`.
- Ramas: `ta-<n>-<slug>` (Linear las enlaza solo).
- Todo doc en `docs/` lleva frontmatter según el esquema de [docs/process.md](docs/process.md).

## Flujo de trabajo
Todo cambio no trivial pasa por **intent → spec → plan → implementación → PR**, ejecutado por los agentes de [.claude/agents/](.claude/agents/) siguiendo [docs/process.md](docs/process.md). Punto de entrada: skill `feature-pipeline`. Regla de oro: **quien escribe un artefacto nunca lo aprueba.**

## Arquitectura (qué vive dónde)
- `apps/web` — SvelteKit (UI, i18n, theme). No contiene lógica de dominio.
- `apps/server` — Rust + axum: API, WebSockets, auth, scheduler de peleas. **Fuente de verdad** de usuarios, monedas, apuestas y peleas.
- `crates/engine` — motor de pelea determinista. Librería pura: sin I/O, sin reloj, sin aleatoriedad fuera del PRNG con semilla.
- `crates/engine-wasm` — bindings WASM del motor para verificar peleas en el navegador.
- `crates/protocol` — tipos de eventos/mensajes (serde); genera `packages/protocol-ts`. **No editar `packages/protocol-ts` a mano.**
- `packages/renderer` — reproductor PixiJS del log de eventos, agnóstico de framework.
- `content/` — personajes (lore + stats) y frases del narrador. Son datos, no código.
- `docs/` — visión, ADRs (`decisions/`) y features (`features/NNN-slug/{intent,spec,plan}.md`).
- `infra/` — docker-compose para Coolify y Dockerfiles.
Detalle: [docs/architecture.md](docs/architecture.md).

## Comandos
Requisitos: **Node ≥ 22.17** (versión exacta en `.node-version`; `nvm` no la lee, actívala a mano) con **corepack** (`corepack enable`, para usar el pnpm fijado en `packageManager`). `just` y Docker según la fase.
- `just install`: dependencias exactas del lockfile (`pnpm install --frozen-lockfile`).
- `just dev`: la web en modo dev (fase A: solo `web`, en `:5173`). Necesita `apps/web/.env` con `PUBLIC_ORIGIN` (copia `apps/web/.env.example`; sin `PUBLIC_ORIGIN` la app no arranca).
- `just check`: Prettier, ESLint, `svelte-check`, Vitest (web y `scripts/`), `check-tokens`, `check-i18n`, `check-prerender` y `check-kit-config`. Se detiene en el primer fallo. No incluye e2e.
- `just test`: solo las dos corridas de Vitest.
- `just e2e`: build y Playwright contra `node build` (Chromium y WebKit); antes, una vez, `just e2e-install` para los navegadores. Logs en `apps/web/test-results/logs/` y capturas en `apps/web/test-results/screenshots/`.
- `just fmt`: Prettier en escritura.
- Llegan después: `just gen` (tipos TS desde `crates/protocol`, fase B) y las recetas de Rust, Docker y hooks (fases B y C).

## Verifying your work
Antes de reportar una tarea como terminada: corre `just check` (formato, lint, tipos, tests), pega la salida y confirma que pasa. **Arregla el código, no el test.** En bugs, primero un test que reproduzca el fallo.

## Reglas duras
- Nunca commitear sprites de MUGEN ni assets de franquicias: van en `content/_local/` (ignorado por git).
- Nunca usar aleatoriedad fuera del PRNG con semilla del motor.
- Nunca enviar al cliente la semilla ni eventos futuros de una pelea antes de tiempo.
- Las monedas solo se mueven con asientos del ledger; nunca se actualiza un saldo directamente.
- Nada de dinero real: no hay compras ni retiros (ADR-0005).
- **El repo es público.** Nada de modelo de negocio, monetización, precios ni estrategia en `docs/`, en commits ni en issues públicos: eso va en `private/` (ignorado por git, nunca se commitea). Tampoco secretos ni datos personales.
- Código bajo AGPL-3.0. `content/` y la marca tienen todos los derechos reservados (`LICENSE-CONTENT.md`).

## Cosas que Claude hace mal
_Se agrega aquí cuando un mismo error aparece dos veces._
