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
- `crates/content` — carga y valida `content/characters/` para el motor (`load_roster`). Hace solo I/O y conversión; la validación es del motor.
- `crates/sim` — simulador de balance (`just sim`). Herramienta de desarrollo: el server nunca depende de él.
- `crates/protocol` — tipos de eventos/mensajes (serde); genera `packages/protocol-ts`. **No editar `packages/protocol-ts` a mano.**
- `packages/renderer` — reproductor PixiJS del log de eventos, agnóstico de framework.
- `content/` — personajes (lore + stats) y frases del narrador. Son datos, no código.
- `docs/` — visión, ADRs (`decisions/`) y features (`features/NNN-slug/{intent,spec,plan}.md`).
- `infra/` — docker-compose para Coolify y Dockerfiles.
Detalle: [docs/architecture.md](docs/architecture.md).

## Comandos
Requisitos: **Node ≥ 22.17** (versión exacta en `.node-version`; `nvm` no la lee, actívala a mano) con **corepack** (`corepack enable`, para usar el pnpm fijado en `packageManager`), **rustup** (instala solo la toolchain de `rust-toolchain.toml`, con los targets `wasm32-unknown-unknown` y `wasm32-wasip1`), `just` y **Docker** (desde la fase B, `just check` y `just test` levantan Postgres).
- `just install`: dependencias JS exactas del lockfile (`pnpm install --frozen-lockfile`).
- `just dev`: Postgres (`db-up`), el server (`cargo run -p server`, en `127.0.0.1:8080`) y la web (`:5173`, con proxy de `/api` y `/ws` al server). Necesita `apps/server/.env` y `apps/web/.env` (copia los `.env.example`). Ctrl-C detiene el server y la web; Postgres sigue arriba.
- `just check`: `check-web` (Prettier, ESLint, `svelte-check`, Vitest de web, `tsc`, ESLint y Vitest de `packages/renderer`, de `tools/sff-converter` y de `tools/sprite-importer`, el `check` de sets de sprites, Vitest de `scripts/`, `check-tokens`, `check-i18n`, `check-prerender`, `check-kit-config`, `check-compose`, `check-workflows`) y después `check-rust` (`cargo fmt --check`, clippy con `-D warnings` dos veces, `_check-engine` y `_check-wasm` (sin Docker), `db-up`, `cargo test --workspace`, frescura de `just gen` y `ts-rs` fuera del grafo del server). Se detiene en el primer fallo. No incluye e2e. Si Docker no responde, falla en `db-up`.
- `just test`: Vitest (web, `packages/renderer`, `tools/sff-converter`, `tools/sprite-importer` y `scripts/`), el `check` de sets de sprites y `cargo test --workspace` con Postgres.
- `pnpm --filter @torneo/renderer gen:placeholders`: regenera los placeholders `dummy_a` y `dummy_b` (`packages/renderer/placeholders/`) con Node y `pngjs`. La salida se commitea; un test de Vitest falla si los PNG o los `manifest.json` no coinciden con lo que dibuja el script.
- `pnpm -s --filter @torneo/sff-converter inspect|convert -- --sff … --air … --map …`: convertidor **solo local** de MUGEN (002 fase B). `inspect` lee cabeceras sin decodificar; `convert` escribe solo en `content/_local/` (se niega fuera de lo que ignora git). Usa `-s` para que pnpm no imprima rutas locales. Detalle en [tools/sff-converter/README.md](tools/sff-converter/README.md).
- `pnpm -s --filter @torneo/sprite-importer run import -- --in … --out …` | `check` | `gen:guide`: importador de sets propios (007 fase B). `run import` convierte un export abierto (`torneo.sprites.export` v1) en sheet + `manifest.json` y lo valida con la capa de guía del renderer (va `run` porque `pnpm import` es un comando de pnpm). `check` valida todo set de `content/sprites/`, su `provenance.json` y los vínculos `sprites` de `character.json` (corre en `just check`). `gen:guide` regenera las tablas de `docs/guides/sprites.md` desde `GUIDE_LIMITS`; un test falla si quedaron viejas. Detalle en [docs/guides/sprites.md](docs/guides/sprites.md).
- `just e2e`: build, `check-renderer-split` (Pixi y el renderer solo por `import()`, 002 RF-A17) y Playwright contra `node build` (Chromium y WebKit); antes, una vez, `just e2e-install` para los navegadores. Logs en `apps/web/test-results/logs/` y capturas en `apps/web/test-results/screenshots/`. El e2e opcional de `/dev/status` corre contra `just dev`: `pnpm --filter web exec playwright test -c playwright.dev.config.ts`.
- `just arena-bench`: build de producción de `/dev/arena` en `127.0.0.1:3000` (fijo, nunca `0.0.0.0`) para medir fps; método en `packages/renderer/README.md`. Falla si el 3000 está ocupado.
- `just gen`: regenera `packages/protocol-ts` desde `crates/protocol` (`ts-rs`). Hay que correrlo y commitear el resultado cada vez que cambia un tipo de `protocol`; `just check` falla si quedó desactualizado.
- `just sim [--fights N] [--seed HEX] [--content DIR] [--json] [--check]`: simula peleas en masa con el motor real (release) sobre `content/characters/` y emite el informe de balance (RF15 y RF17 de 003). Por defecto, 10 000 peleas con semilla fija; `--check` sale con 1 si falla un criterio y con 2 ante un error. El mismo chequeo corre en `just check` (`crates/sim/tests/balance.rs`). Córrelo tras crear o editar un personaje. Nunca le pases la semilla de una pelea real.
- `just db-up` / `just db-down` / `just db-reset`: Postgres de desarrollo (`infra/compose.dev.yml`, `127.0.0.1:${DEV_PG_PORT:-5432}`). `db-reset` borra el volumen y las bases `torneo_test_*` que hayan quedado.
- `just crate-age`: comprueba en crates.io que toda versión de `Cargo.lock` tenga al menos 3 días. Usa la red; córrelo **antes de compilar** cada vez que cambie el lock y baja lo marcado con `cargo update -p <crate>@<versión> --precise <anterior>`.
- `just deny`: `cargo deny check bans sources` (sin OpenSSL ni aws-lc, una sola versión de `tungstenite` y solo crates.io como fuente); en local solo si `cargo-deny` está instalado.
- `just fmt`: Prettier y `cargo fmt` en escritura.
- `just prod-local [tag]`: las imágenes de producción con `infra/docker-compose.yml` más `infra/compose.local.yml` (Postgres local), en `127.0.0.1:3000` (web) y `127.0.0.1:8080` (server). Sin `tag` construye `ghcr.io/kike1303/torneo-absurdo/{server,web}:local` para `linux/amd64`; con `vX.Y.Z` o `sha-<7>` las baja de GHCR. Falla si el 3000 o el 8080 están ocupados (`just dev` usa el 8080). Crea las redes `torneo-edge` y `torneo-db` con la etiqueta `torneo.local=1` (`scripts/local-networks.sh`). Para ejecutar imágenes amd64 en una Mac ARM hace falta emulación: en Colima, `colima start --vz-rosetta` (o `rosetta: true` en `colima.yaml`); en Docker Desktop, "Use Rosetta for x86_64/amd64 emulation". El build no emula: compila en cruzado.
- `just prod-local-down`: baja el stack (conserva el volumen de Postgres) y borra **solo** las redes con la etiqueta `torneo.local=1`.
- `just check-dockerignore`: prueba con Docker que `.dockerignore` deja fuera los `.env`, `.git` y los directorios de build. No abre ni toca un `.env` existente.
- `just hooks`: instala el hook `pre-push` (`git config core.hooksPath scripts/git-hooks`, vale para todos los worktrees del clon). El hook corre `just check` y bloquea el push si falla. Revisa el **árbol de trabajo**, no los commits que se empujan, y necesita Docker encendido. `git push --no-verify` es la salida excepcional. Un worktree de una rama sin `scripts/git-hooks/` empuja sin chequeo.
- **CI** (`.github/workflows/ci.yml`): `ci-ok` es el único check requerido. `policy` corre siempre (`scripts/check-workflows.mjs`, sus tests con `scripts/vitest.policy.config.mjs` y `scripts/check-hooks.sh`), y `rust`, `web`, `e2e` e `images` corren según los archivos que toca el PR. Un workflow o una action nueva tiene que pasar `node scripts/check-workflows.mjs`. Por ejemplo, un job nuevo se agrega a su tabla de permisos en el mismo PR, las actions van fijadas por SHA y ningún `${{ }}` entra en un `run:` (va por `env:`). El release (`release.yml`) solo lo dispara un tag `v*` del owner, y `deploy` espera su aprobación en el environment `production`.
- **Dependabot** abre PRs semanales con 3 días de cooldown y nunca se fusionan solos. Un bump de `rust:` en `apps/server/Dockerfile` sin `rust-toolchain.toml`, o de `node:` sin `.node-version`, falla en `check-compose.mjs` a propósito: se suben los dos en el mismo PR.

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
