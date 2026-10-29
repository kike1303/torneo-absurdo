---
title: Arquitectura general
type: architecture
status: draft
author: architect
created: 2026-10-20
updated: 2026-10-28
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
2. **Motor puro**: `engine::simulate(seed, a, b, ruleset) -> Result<FightLog, SpecError>`. Sin I/O, sin reloj y determinista en todas las plataformas (nativo y WASM). Devuelve `Result` porque valida los personajes, que en `engine-wasm` llegan del cliente; con entradas válidas la simulación no falla (refinamiento de la firma de ADR-0001, preocupación 1 de la [spec de 003](features/003-motor-de-pelea/spec.md)).
3. **Sin video**: el cliente recibe eventos y anima. Se sincroniza con el reloj del servidor (`server_time` y offset).
4. **Sin texto en el dominio**: el motor y el protocolo emiten códigos; la UI y el narrador los traducen (ADR-0007).
5. **Contenido como datos**: personajes y frases en `content/`, versionados y validados en CI.

## Seguridad y logs
- **Logs**: un objeto JSON por línea, con una allowlist cerrada de campos por app y sin datos del cliente (IP, cookies, headers, URL, cuerpos ni valores de persona). `web` escribe solo por `apps/web/src/lib/server/log.ts` y registra los errores por código, nunca por su mensaje; el server, por el formateador de `telemetry.rs`; Postgres, con parámetros que no escriben filas ni sentencias. Contrato completo, niveles, plazo y cómo agregar un campo o un evento: [ADR-0012](decisions/0012-logs-y-cabeceras-de-seguridad.md), parte A.
- **CSP y cabeceras**: la CSP base de `web`, las cabeceras de `web`, del server y de Traefik, lo prohibido y las ampliaciones permitidas (renderer, `engine-wasm`, WebSocket): [ADR-0012](decisions/0012-logs-y-cabeceras-de-seguridad.md), parte B.
- Auth, sesiones, `Origin`/CSRF y límites por IP: [ADR-0008](decisions/0008-autenticacion.md).

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
`crates/protocol` define los mensajes WS, los DTOs del API y el log de pelea (`FightLog`, `FightEvent` y sus enums, feature 003) con serde. Es una librería pura: sin I/O, sin `tokio`, sin reloj y sin texto para humanos (ADR-0007).

- **Generación:** `ts-rs`, opcional detrás de la feature `ts` para que el server no lo compile. `just gen` vacía `packages/protocol-ts/src/generated/`, exporta un `.ts` por tipo (imports con `.js`), les antepone la cabecera de "generado, no editar", escribe el barril `src/index.ts` (`export type *`) y formatea con Prettier. Los archivos generados se commitean, y `just check` falla si están desactualizados. Nunca se editan a mano.
- **Paquete:** `@torneo/protocol` (`packages/protocol-ts`) exporta el TS fuente, sin paso de build. `apps/web` lo consume con `import type`.
- **Convenciones (RF-B4):**
  - Enums con datos: `#[serde(tag = "type", rename_all = "snake_case")]` (o el campo de etiqueta que corresponda, como `status` en `HealthResponse`).
  - Códigos de error: `dominio.motivo` (`ws.origin_forbidden`), como enums con `#[serde(rename = "dominio.motivo")]` en cada variante, nunca `String` libre. Cada enum de códigos es dueño de un solo prefijo (`ApiErrorCode` → `http.*`, `WsErrorCode` → `ws.*`) y expone `ALL` y `as_str()`.
  - `ErrorCode` es la unión sin etiquetar de los enums de códigos y `ApiError { error: ErrorCode }` es el cuerpo de toda respuesta HTTP de error. Un dominio nuevo (por ejemplo `auth.*`) agrega su enum y su variante en `ErrorCode`; los conjuntos de cadenas no se solapan (lo comprueba un test).
  - Nada de enteros de 64 bits o más (`u64`, `i64`, `u128`, `i128`, `usize`) en tipos exportados: `just gen` falla si aparece `bigint`. Si hace falta, se serializan como cadena.

## Motor de pelea

> Detalle: [spec de 003](features/003-motor-de-pelea/spec.md), [plan-a](features/003-motor-de-pelea/plan-a.md) y [plan-b](features/003-motor-de-pelea/plan-b.md). Si algo no coincide, manda la spec.

- **Crates.** `crates/engine` (librería pura: `simulate`, `Seed`, `Ruleset`, `CharacterSpec`, `SpecError` y la serialización canónica) y `crates/engine-wasm` (un `cdylib` con `wasm-bindgen` que exporta solo `simulate` y devuelve códigos de error `dominio.motivo`). `crates/content` lee `content/characters/` con `load_roster(root, &Ruleset) -> Result<Vec<Character>, ContentError>` (solo I/O y conversión: la validación es `CharacterSpec::validate` e `is_valid_slug` del motor, así que el navegador valida igual) y `crates/sim` juega peleas masivas con el motor real y emite el informe de balance (`just sim [--fights N] [--seed HEX] [--content DIR] [--json] [--check]`, en release). Sus umbrales viven en `sim::Criteria::RF17` y el test `crates/sim/tests/balance.rs` los aplica al contenido real dentro de `just check`. `sim` usa su propia semilla raíz y nunca recibe la de una pelea real (ADR-0002).
- **Dirección de dependencias:** `protocol ← engine ← {content, engine-wasm, sim}` y `content ← sim` (más adelante, el server dependerá de `engine` y `content`). Nunca al revés: el motor no conoce a sus consumidores. El motor depende de `protocol` sin la feature `ts`: los tipos del log viven en `protocol` y el motor no compila `ts-rs` (lo comprueba `_check-ts-rs-graph`).
- **Pureza:** lints `deny` en `lib.rs`, `crates/engine/clippy.toml` (sin `HashMap`/`HashSet`, reloj, semillas del sistema ni I/O de archivos, red, procesos, hilos o entorno) y `scripts/check-engine.mjs` (grafo y features de `engine`, y también del grafo conjunto de sus consumidores, que salen de `cargo metadata` (todo miembro del workspace que dependa de `engine`, salvo el server), para que un consumidor no active en un build conjunto features que el motor hereda; que el server no dependa de `sim`, cuyas semillas son públicas (ADR-0002); en `engine-wasm`, ni el token `unsafe` ni atributos que relajen `unsafe_code`, ni `include!` ni `#[path]`), todo dentro de `just check`. `engine` y `engine-wasm` heredan `unsafe_code = "forbid"` del workspace, sin excepciones.
- **Serialización canónica:** `canonical_json(&FightLog)` y `canonical_spec_json(&CharacterSpec)` son `serde_json::to_vec` compacto, con los campos en orden de declaración y sin mapas ni floats. "Idéntico byte a byte" significa que devuelven los mismos bytes. Los dorados de `crates/engine/tests/golden/` lo comprueban en nativo y en `wasm32-wasip1` (`just _check-wasm`), y los `log-*.json` son también los fixtures del adaptador de 002, que los lee en su lugar (`apps/web/tests/support/golden-logs.ts`). Las únicas copias son los 7 `log-*.json` de `apps/web/src/lib/features/fight/fixtures/`, que el arena empaqueta porque el contexto de Docker de la web no incluye `crates/`. Un test las compara byte a byte con su dorado y otro exige que el conjunto de `log-*.json` sea exactamente el de `ENGINE_LOGS` (enmienda (006, TA-21), ver la [spec de 006](features/006-ritmo-rapido/spec.md)).
- **Respiro (006):** tras una racha de resoluciones sin KO ni knockdown, un sorteo del PRNG (pasos 6 y 7 de RF7 de 003) desplaza por igual los dos relojes entre 1,8 y 2,6 s, y deja un hueco de ≥ 2,5 s sin impactos. No es un evento ni cambia quién actúa: solo ausencia de impactos. Sus entradas son los campos `breather_*` de `Ruleset`, y `sim` exige el ritmo resultante (media de 1100–1300 ms, ráfagas y pausas) en `Criteria::RF17`. Enmienda (006, TA-21): ver la [spec de 006](features/006-ritmo-rapido/spec.md).
- **Contrato de la `Timeline` (006):** `engine::TIMELINE_CONTRACT` (`crates/engine/src/contract.rs`) es un espejo de `TIMELINE_LIMITS` del renderer (2000 ms de intro, 600 ms entre impactos, knockdown de 800 a 3000 ms, 1500 ms de KO a victoria y 2000 ms de pose). No es una constante de juego ni un campo de `Ruleset`: la simulación nunca lo lee. Lo usan el test de cotas del ruleset, las invariantes de los tests y `sim`. El dorado `tests/golden/timeline-contract.json` y `apps/web/tests/timeline-contract.test.ts` lo atan a `TIMELINE_LIMITS`, así que cambiar un lado solo falla `just check`. Enmienda (006, TA-21): ver la [spec de 006](features/006-ritmo-rapido/spec.md).
- **Versionado:** `Ruleset::for_version(n)`; antes de la primera pelea de producción, la versión 1 puede cambiar regenerando los dorados en el mismo PR, y después todo cambio es una versión nueva (ADR-0001, RF10 de 003).

## Renderer de peleas

> Detalle normativo en la [spec de 002](features/002-spike-renderer/spec.md) ("Formato de assets v1", "Timeline v1" y "Coreografía"). La fase A1 entregó el núcleo puro y A2 la capa Pixi, el reproductor, la ruta de prueba y la carga diferida.

- **Paquete:** `@torneo/renderer` (`packages/renderer`) exporta el TS fuente, sin paso de build, con los subpaths `.`, `./manifest`, `./timeline`, `./guide`, `./scene`, `./placeholders/*` y `./fixtures/*`. `./guide` y `./scene` son puros (sin Pixi ni DOM); `apps/web` no los importa (su ESLint prohíbe `@torneo/renderer/*` como valor), así que no cambian `check-renderer-split`. Los imports relativos llevan extensión `.ts` para que lo carguen Vite y Node con *type stripping*.
- **Timeline propia:** el renderer no conoce el `FightLog`. Reproduce una `Timeline` de *cues* (`intro`, `strike`, `knockdown`, `ko`, `win` y, desde la `timeline_version` 2, `time_up` y `draw`) validada con `validateTimeline`, que despacha por versión: la v1 sigue igual y la v2 es un superconjunto que suma los fines por tiempo (decisión y empate, feature 005). El único archivo que conoce las dos formas es el adaptador del lado del consumidor, `apps/web/src/lib/features/fight/timeline-from-fight-log.ts` (`timelineFromFightLog`, fase C de 002), y lo comprueba `apps/web/tests/fight-boundary.test.ts`. Es puro, solo importa tipos y no corrige nada: lo que la v2 puede revisar lo revisa `validateTimeline` en `load`. Emite siempre la v2 y traduce los tres finales del motor (`ko`, `decision` y `draw`); ningún log válido da `adapter.unsupported_ending`, que queda solo para un `reason` desconocido. `/[lang]/dev/arena?log=ko-comeback|ko-clutch|ko-decisive-crit|ko-knockdown|decision-a|decision-b|draw` reproduce los 7 dorados del motor, y `?pace=<factor>` reescala el combate entre un mínimo dinámico (el menor factor que sigue pasando `validateTimeline`, con piso 0.5) y 1.5, con clamp al límite válido. Enmienda (006, TA-21): ver la [spec de 006](features/006-ritmo-rapido/spec.md). Así un cambio del protocolo cae en el adaptador o en una `timeline_version` nueva, no en el renderer.
- **Formato de assets v1:** cada personaje es una carpeta con `manifest.json` y hojas `sheet-<n>.png`, con las nueve animaciones obligatorias y tres opcionales con fallback fijo. `validateManifest` lo valida como entrada no confiable y devuelve errores como códigos con JSON Pointer. Los placeholders `dummy_a` y `dummy_b` los genera un script determinista.
- **Escena pura:** `prepareScene(timeline, manifests)` resuelve una vez por carga la coreografía (segmentos por luchador con las reglas de precedencia), el HP por luchador y las semillas cosméticas, y lo congela. `sceneAt(prepared, t, { reducedMotion })` es función pura de `t`: búsquedas binarias y una ventana acotada de cues (`cueWindow`, hasta 3.3 s hacia atrás y 400 ms hacia adelante por el windup). `seek`, las pestañas en segundo plano y las caídas de frames no desincronizan, porque no hay estado por frame.
- **Azar y reloj:** los efectos usan un PRNG propio (`mulberry32`) sembrado con el índice del cue y un canal, nunca la semilla de la pelea (ADR-0002). El tiempo entra por el reloj que inyecta el host. ESLint prohíbe `Math.random`, `Date`, `performance` y `crypto` en todo el paquete, y Pixi y el DOM en el núcleo (`src/{manifest,timeline,scene,fx}`).
- **Sin texto ni colores:** la escena describe colores por rol (`hp`, `hpLost`, `crit`, `text`) que la capa Pixi traduce con `RendererTheme` (ADR-0009 D7), y los nombres y banners llegan traducidos por el host (ADR-0007). El HUD se maqueta en px CSS del contenedor (`layoutHud`), fuera de la cámara lógica de 640×360.
- **Tres capas:** `src/player/` (puro) implementa el `RendererHandle` sobre una interfaz `Stage`: reloj de reproducción anclado al reloj del host (`t = anclaT + (clock() − anclaReloj)`, nunca frames contados), despacho de cues con un cursor (un `seek` no dispara lo que salta; un salto del reloj dispara todo el intervalo en orden), `onState`, `snapshot()` y `RendererError` con códigos. `src/stage/` (Pixi) traduce cada `Scene` a objetos reutilizables (pools de partículas y números, barras como sprites) sin guardar estado entre frames. `createRenderer(container, options)` une las dos. Las estadísticas de frames (`src/stats/`, puro) usan `elapsedMS` del ticker, así que el paquete no lee `performance.now()`.
- **CSP sin cambios** ([ADR-0012](decisions/0012-logs-y-cabeceras-de-seguridad.md) B4.2): la entrada del paquete importa primero `pixi.js/unsafe-eval` (shaders sin `new Function`), `Assets.setPreferences({ preferWorkers: false })` evita los workers `blob:` y ESLint prohíbe los submódulos de Pixi con workers o texturas comprimidas (`ktx2`, `basis`, `ktx`, `dds`, `webworker`, `gif`). Solo WebGL: sin WebGL, `load` rechaza con `webgl_unavailable`. `csp.spec.ts` lo prueba en Chromium y WebKit sobre `/es/dev/arena`.
- **Carga diferida en `apps/web`:** solo `lib/components/fight/FightStage.svelte` importa valores del renderer, y lo hace con `import()` en `onMount` (ESLint cubre también los `import()` fuera de ese archivo). `scripts/check-renderer-split.mjs` corre en `just e2e` sobre el build: con el manifest de Vite y el mapa chunk → módulos que escribe `apps/web/vite/chunk-modules.ts` (en `.vite/`, que no se publica), falla si alguna entrada o nodo de ruta, también el de `dev/arena`, alcanza `pixi.js` o `packages/renderer` por imports estáticos, y exige que `dev/arena` alcance Pixi por un import dinámico. `Assets` de Pixi es global, así que las hojas pasan por una caché compartida con conteo de referencias (`packages/renderer/src/stage/shared-cache.ts`): cada URL se carga una vez, `destroy` solo libera las referencias de su renderer y una hoja se descarga cuando la suelta su último dueño. Dos renderers que se solapan (un cambio rápido de luchador o un reintento) no se destruyen las texturas entre sí.
- **Convertidor local de MUGEN (fase B de 002):** `tools/sff-converter` (`@torneo/sff-converter`) convierte SFF v1/v2 y AIR al formato de assets v1. Es una herramienta de desarrollo: corre con *type stripping* de Node, solo importa `@torneo/renderer/manifest`, no es dependencia de `apps/web` y `tools/` está fuera del contexto de Docker (`.dockerignore`, comprobado por `check-compose` y `check-dockerignore`). Trata su entrada como no confiable (lector acotado, límites, sin recursión) y solo escribe archivos que `git check-ignore` confirma ignorados, por defecto en `content/_local/sprites/<id>/`. Uso en [su README](../tools/sff-converter/README.md).
- **Capa de guía (007):** `@torneo/renderer/guide` valida un set propio sobre lo que ya valida `validateManifest`: `validateGuide(manifest, hojas RGBA, rulesets)` devuelve códigos `guide.*` con JSON Pointer. Sus rangos viven en un único `GUIDE_LIMITS` y los tiempos se derivan de `TIMELINE_LIMITS`, `REACTION_WINDOW_MS`, `CONTACT_PX` y del ruleset del motor, que llega por el dorado `crates/content/tests/golden/ruleset-timing.json` (lo escribe un test de `crates/content` con la API pública de `Ruleset`). El runtime del renderer nunca la importa (`test/guide-imports.test.ts`).
- **Sets propios e importador (007):** los sets viven en `content/sprites/<id>/` (`manifest.json`, hojas y `provenance.json` sin datos personales) y un personaje los enlaza con `"sprites": "<id>"`, que `crates/content` expone como `Character::sprites()` sin tocar `CharacterSpec`. `tools/sprite-importer` (`@torneo/sprite-importer`, misma forma que el convertidor: *type stripping*, fuera de Docker) convierte un export abierto (un PNG por frame en una celda fija y un `export.json`) en hojas y manifest, solo hacia `content/sprites/<id>` o `content/_local/sprites/<id>`, y su `check`, dentro de `just check`, valida cada set, su procedencia y los vínculos. Las tablas de la [guía de arte](guides/sprites.md) se generan del código con prueba de frescura.
- **Servido de `content/_local/sprites/` solo en `vite dev`:** `apps/web/vite/local-sprites.ts` es un plugin `apply: 'serve'` (sin `configurePreviewServer`) que responde `/__local-sprites/index.json` y `/__local-sprites/<id>/{manifest.json,sheet-<n>.png}` con una lista blanca exacta de nombres, sin decodificar, con contención por `realpath` y `nosniff`. Los middlewares de Vite que validan `Host` y CORS corren antes. El cliente pide el índice detrás de `import.meta.env.DEV`, así que el build no lo contiene; `scripts/check-no-local-sprites.mjs` lo comprueba en `just e2e`. Los personajes locales nunca llegan al build, a la imagen ni al repo (ADR-0004).
- **Medición:** `just arena-bench` sirve el build de producción en `127.0.0.1:3000` con el catálogo de desarrollo; el método (375 px, calentamiento y tres corridas, "copiar JSON") está en el README del paquete.

## Despliegue

> Fuente de las decisiones: [ADR-0006](decisions/0006-despliegue-y-ci.md). Detalle de la fase C: [spec de 001](features/001-fundaciones/spec.md) (RF-C1 a RF-C19) y [plan-c](features/001-fundaciones/plan-c.md). Esta sección es el resumen operativo; si algo no coincide, mandan el ADR y la spec.

### Piezas
- **Postgres es un recurso standalone de Coolify** (`postgres:17-alpine`, 512 MB, `shared_buffers=128MB`, 20 conexiones), **fuera del compose** y sin puerto público. Sus parámetros de log no escriben datos de las filas ni sentencias ([ADR-0012](decisions/0012-logs-y-cabeceras-de-seguridad.md) A9; paso 6 de "Preparación del VPS"). Sus backups son los programados de Coolify (diarios a las 09:00 UTC, 7 días en el VPS y 30 días en Cloudflare R2; ADR-0006 D9).
- **`infra/docker-compose.yml` tiene solo `web` y `server`.** Los dos usan `image: ghcr.io/kike1303/torneo-absurdo/{server,web}:${IMAGE_TAG:?}` y ninguno tiene `build:` ni `ports:`. Coolify lee el compose de `main`, con el auto-deploy apagado, y le pone `IMAGE_TAG`, `PUBLIC_HOST` y `DATABASE_URL` como variables de entorno. Esas variables viven **solo en Coolify**: no hay `infra/.env` con valores reales. `scripts/check-compose.mjs` exige la forma exacta del archivo:
  - `mem_limit` = `memswap_limit` (256 MB `server` y 384 MB `web`, con `NODE_OPTIONS=--max-old-space-size=256`);
  - healthchecks en forma exec (15 s / 3 s / 3 / 30 s), y `web` espera a que `server` esté `healthy`;
  - `read_only`, `cap_drop: [ALL]`, `no-new-privileges`, `platform: linux/amd64` y `restart: unless-stopped`;
  - logs `json-file` de 3 × 10 MB, en los que se apoya el plazo del aviso de privacidad;
  - `stop_grace_period: 15s` en `server`, con cierre `1012` de los WebSockets (ADR-0006 D10);
  - `RUST_LOG=info` exacto.
- **Imágenes.** Se construyen en GitHub Actions (amd64 nativo) y **nunca en el VPS**. Se publican en GHCR público, así que el VPS hace pull sin `docker login`.
  - `server`: binario estático musl (`cargo-chef` y `cargo-zigbuild`) sobre **`gcr.io/distroless/static-debian13:nonroot`**.
  - `web`: `build/` de adapter-node y `package.json`, sin `node_modules`, sobre **`gcr.io/distroless/nodejs22-debian13:nonroot`** (Node igual a `.node-version`).
  - Las dos bases van por digest y corren como uid `65532`. Las `-debian12` están deprecadas (enmienda del 2026-10-24 en la spec y nota en ADR-0006).
  - El server sirve HTTP/1.1 con su propio bucle de hyper `http1` (`header_read_timeout`, upgrades para WS), no con `axum::serve` (desviación 8 de [plan-b](features/001-fundaciones/plan-b.md)).
- **Redes** (ADR-0006 D1). "Connect to Predefined Network" está apagado: nada nuestro vive en la red compartida `coolify`.
  - **`torneo-edge`**: bridge externa y dedicada, `10.250.0.0/28`. Solo la usan `coolify-proxy` (IP fija `10.250.0.2`), `web` y `server`, que lleva el alias `torneo-server`. Es la única subred que el server acepta en `TRUSTED_PROXIES` (`10.250.0.2/32`).
  - **`torneo-db`**: bridge externa, que solo comparten `server` y el Postgres standalone. `web` no la ve.
- **Traefik** (el proxy de Coolify; ADR-0006 D2 y D11). Usa etiquetas propias con prefijo `torneo-*` y sin strip de prefijo:
  - `torneo-api` (`/api` y `/ws`, prioridad 100) va a `server:8080`;
  - `torneo-web` (todo lo demás, prioridad 1) va a `web:3000`;
  - `torneo-*-http` redirigen a HTTPS con `torneo-https-redirect`;
  - los cuatro routers llevan `observability.accesslogs=false`.

  El router `torneo-web` lleva además el middleware `torneo-web-headers` (`nosniff`, `Referrer-Policy: strict-origin-when-cross-origin` y HSTS `max-age=31536000` **sin** `includeSubDomains` ni `preload`). Así cubre las respuestas que no pasan por el `handle` de SvelteKit: los estáticos y la 308 de `/healthz/`. axum ya pone las suyas.

### Release con gate manual (ADR-0006 D5 y D6)
1. El owner empuja un tag `vX.Y.Z` (un ruleset de tags se lo reserva). `release.yml/build` valida el tag con `^v[0-9]+\.[0-9]+\.[0-9]+$` antes de construir y comprueba que ni `vX.Y.Z` ni `sha-<7>` existen ya, porque una etiqueta publicada nunca se mueve. Después publica `server` y `web` con las dos etiquetas sobre el mismo digest, sin `latest`, y con un manifiesto de imagen por etiqueta (`provenance` y `sbom` apagados, comprobado con `imagetools inspect --raw`). Al final aplica la retención: conserva las 10 versiones más recientes de cada paquete.
2. `deploy` usa el environment `production`, que exige la **aprobación del owner**. Lo que hace depende de la variable `DEPLOY_MODE`:
   - **`manual`** (por defecto, también si la variable no existe): el owner fija `IMAGE_TAG` en Coolify, despliega a mano y aprueba el environment. El job solo registra el tag aprobado.
   - **`api`**: no está implementado y el job **falla**. Implementarlo requiere un nuevo OK del owner, un token de Coolify de alcance de equipo y un secreto del environment (riesgo descrito en ADR-0006 D6).
3. `verify` consulta `https://$PUBLIC_HOST/api/health` (variable de repositorio `PUBLIC_HOST`) cada 10 s durante 5 min, y falla si `version` no es el tag.
4. **Rollback:** `workflow_dispatch` de `release.yml` desde `main` con el tag anterior. No reconstruye (solo comprueba que el tag existe en GHCR) y pasa por la misma aprobación.

Antes de aprobar un release que traiga migraciones se hace un "Backup now" de Postgres en Coolify, y nunca se despliega durante un main event (ADR-0006 D9 y D10).

### Preparación del VPS (una vez, ADR-0006 D1, D4 y D11)
Son los cambios en el proxy compartido que el owner aprobó el 2026-10-21. Se hacen por SSH y en la UI de Coolify.

1. **Red `torneo-edge`.** Primero se comprueba que `10.250.0.0/28` no choca con ninguna red existente, y después se crea con un `--ip-range` que deja la `.2` reservada al proxy:
   ```sh
   docker network inspect $(docker network ls -q) --format '{{.Name}} {{range .IPAM.Config}}{{.Subnet}} {{end}}'
   docker network create --driver bridge --subnet 10.250.0.0/28 --gateway 10.250.0.1 \
     --ip-range 10.250.0.8/29 torneo-edge
   docker network create --driver bridge torneo-db
   ```
   El Postgres standalone se conecta a `torneo-db` desde su configuración en Coolify.
2. **`coolify-proxy` en `torneo-edge` con IP fija.** En Coolify, Servers → Proxy → Configuration, se agrega la red externa a la lista de redes del servicio del proxy, **sin quitar `coolify`**, y se reinicia el proxy:
   ```yaml
   networks:
     torneo-edge:
       external: true
   services:
     traefik:
       networks:
         torneo-edge:
           ipv4_address: 10.250.0.2
   ```
   → `docker inspect coolify-proxy --format '{{json .NetworkSettings.Networks}}'` muestra `torneo-edge` con `10.250.0.2`.
3. **Swap de 2 GB con `swappiness` 10**, solo si `swapon --show` no muestra ninguno:
   ```sh
   sudo fallocate -l 2G /swapfile && sudo chmod 600 /swapfile
   sudo mkswap /swapfile && sudo swapon /swapfile
   echo '/swapfile none swap sw 0 0' | sudo tee -a /etc/fstab
   echo 'vm.swappiness=10' | sudo tee /etc/sysctl.d/99-swappiness.conf && sudo sysctl --system
   ```
4. **Logs de acceso de Traefik apagados** (D11). El comando del proxy no lleva `--accesslog`, y Traefik es ≥ 3.1 (lo necesita `observability.accesslogs`):
   ```sh
   docker inspect coolify-proxy --format '{{json .Config.Cmd}}' | tr ',' '\n' | grep -i accesslog
   docker exec coolify-proxy traefik version
   ```
   El primero no tiene que imprimir nada.
5. **Limpieza de Docker de Coolify** (Servers → Settings): diaria y además al pasar el 80 % de disco, con **"Delete Unused Volumes" apagado**.
6. **Configuración del Postgres standalone** (ADR-0006 D4 y ADR-0012 A9). En el recurso de Postgres de Coolify, el límite de memoria es 512 MB y la "Custom PostgreSQL Configuration" lleva el tuning de D4 y los parámetros de log de A9. Son los mismos valores que el `command` de `infra/compose.local.yml` y, solo los de log, el de `infra/compose.dev.yml` (`scripts/check-compose.mjs` compara los dos compose; en el VPS lo confirma la consulta de los prerrequisitos):
   ```ini
   shared_buffers = 128MB
   max_connections = 20
   work_mem = 4MB
   log_error_verbosity = terse
   log_min_error_statement = panic
   log_statement = none
   log_min_duration_statement = -1
   log_min_duration_sample = -1
   log_connections = off
   log_disconnections = off
   log_transaction_sample_rate = 0
   log_statement_sample_rate = 1
   log_lock_waits = off
   ```
   - Con los valores por defecto, Postgres escribe en su log el `DETAIL` (`Key (…)=(…)`, `Failing row contains (…)`) y la línea `STATEMENT` de cada error. `terse` y `panic` los quitan, y las líneas `ERROR` quedan solo con el mensaje primario. Los cuatro últimos son los valores por defecto, fijados para que nadie encienda el registro muestreado de sentencias (`log_min_duration_sample`) ni de transacciones completas (`log_transaction_sample_rate`) al diagnosticar latencia.
   - `log_line_prefix` se deja en `%m [%p] ` (sin `%h` ni `%r`), `log_hostname` en `off`, y no se cargan `auto_explain` ni `pg_stat_statements` con salida a log.
   - Un valor inválido hace que Postgres no arranque, y el healthcheck de Coolify lo muestra. Nunca se queda con los valores por defecto.
   - Los `FATAL` de conexión (`no pg_hba.conf entry for host "…"`) llevan la IP en el mensaje primario, y `terse` no la quita. Se acepta **solo mientras Postgres no tenga puerto público** y solo `server` se conecte por `torneo-db` (ADR-0006). Si eso cambia, se revisa A9 antes.
   - Al subir de mayor se revisan los parámetros: en PostgreSQL 18, `log_connections` pasa a ser una lista.
7. **Rotación de los logs de Postgres** (ADR-0012 A7). El recurso standalone no usa nuestro compose, así que su `LogConfig` viene del daemon. Se comprueba con:
   ```sh
   docker inspect <contenedor-postgres> --format '{{json .HostConfig.LogConfig}}'
   ```
   Tiene que mostrar `{"Type":"json-file","Config":{"max-file":"3","max-size":"10m"}}`. Si no, el **plan B** es fijarlo en `/etc/docker/daemon.json`:
   - Los valores de `log-opts` van **como cadenas**. Con `"max-file": 3`, `dockerd` no arranca:
     ```json
     { "log-driver": "json-file", "log-opts": { "max-size": "10m", "max-file": "3" } }
     ```
   - Se **fusiona** con el `daemon.json` que ya exista, sin reemplazarlo. El instalador de Coolify suele escribir uno, a menudo con estos mismos valores.
   - Antes de aplicarlo se valida: `sudo dockerd --validate --config-file /etc/docker/daemon.json`.
   - `log-opts` no se recarga con `SIGHUP`: hay que **reiniciar `dockerd`**, lo que reinicia todos los contenedores del VPS compartido, Coolify incluido. Pide una **ventana y el OK del owner**.
   - Solo vale para los contenedores creados después: tras el reinicio se recrea el recurso de Postgres en Coolify y se repite el `docker inspect`.

> **Advertencias.**
> - **Nunca borrar `torneo-edge`.** Si la red no existe, el compose del proxy falla y se caen **todas** las apps del VPS. `just prod-local-down` solo borra redes con la etiqueta `torneo.local=1`, así que no la toca.
> - **"Reset configuration to default" del proxy en Coolify quita `torneo-edge` y la IP `10.250.0.2`.** Después de un reset, de reiniciar el proxy o de actualizar Coolify, se repite el paso 2 y la verificación 15 de ADR-0006.

### Prerrequisitos del primer despliegue a producción (bloquean; RF-C18)
Ninguno se cumple en la feature 001. Los puntos 1 a 4 se siguen en Linear (TA-8, bloqueado por la feature de auth y por TA-6), el 5 en TA-13 y el 6 en la feature 002.

1. **Límites anónimos de ADR-0008 D6 y D7 en `/ws`, implementados y probados:** como máximo 50 sockets simultáneos por `ip_key`, y upgrade a 30/min con ráfaga de 10 por IP, con `TRUSTED_PROXIES=10.250.0.2/32` (ADR-0006 D2).
2. **Las verificaciones de ADR-0006 que no se hicieron en el cierre de 001** (RF-C19), más dos comprobaciones con el stack desplegado:
   - `docker inspect` de `web` y `server`: `ReadonlyRootfs`, `CapDrop`, `SecurityOpt` y `LogConfig`, para confirmar que Coolify respeta el endurecimiento y los logs al reescribir el compose.
   - `docker inspect` del contenedor de Postgres: `LogConfig` es `json-file` con `max-size` `10m` y `max-file` `3`. Si no, plan B de `daemon.json` y recrear el recurso (paso 7 de "Preparación del VPS", ADR-0012 A7).
   - Los parámetros de log de ADR-0012 A9 en el Postgres de producción. Cada fila tiene el valor del paso 6 de "Preparación del VPS", con `source` distinto de `default` en los fijados, y `log_line_prefix` no lleva `%h` ni `%r`:
     ```sql
     SELECT name, setting, source FROM pg_settings WHERE name IN (
       'log_error_verbosity', 'log_min_error_statement', 'log_statement',
       'log_min_duration_statement', 'log_min_duration_sample', 'log_connections', 'log_disconnections',
       'log_transaction_sample_rate', 'log_statement_sample_rate', 'log_lock_waits',
       'log_line_prefix', 'log_hostname') ORDER BY name;
     ```
   - `curl -I` a `https://${PUBLIC_HOST}/es/`, `/_app/version.json` y `/healthz/` (la 308). Las tres respuestas traen `X-Content-Type-Options: nosniff`, `Referrer-Policy: strict-origin-when-cross-origin` y `Strict-Transport-Security: max-age=31536000`, sin `includeSubDomains` ni `preload` y cada una una sola vez (Traefik solo emite HSTS sobre TLS).
3. **La configuración de GitHub de RF-C19**, si no se hizo en el cierre de 001: rulesets de `main` (con `ci-ok` requerido) y de tags `v*`, el environment `production` con el owner como reviewer y `can_admins_bypass: false`, aprobación de workflows para todos los contribuidores externos y las alertas de Dependabot.
4. **Preparación del VPS** (sección anterior), con la verificación 15 de ADR-0006 en verde, y la **prueba de restauración** de backups de ADR-0006 D9.
5. **Peleas re-simulables antes de abrir apuestas (RF22 de 003, TA-13).** Ninguna pelea de producción abre apuestas si no se cumple una de estas dos condiciones: (a) se publican como snapshot inmutable, junto con el commit, los `canonical_spec_json` de los dos personajes y la `ruleset_version`; o (b) el commit se extiende a `SHA-256(seed ‖ canonical_spec_json(a) ‖ canonical_spec_json(b) ‖ ruleset_version)`, lo que enmienda ADR-0002 con el OK del owner. TA-13 también cubre que el `FightLog` completo no salga del server antes de `Settled`, igual que la semilla.
6. **`timeline_version` 2 del renderer (preocupación 7 de la spec de 003). Resuelto por TA-19 (feature 005):** la v2 suma `time_up` y `draw`, y el adaptador reproduce los tres finales. Texto original: ninguna feature publica peleas reales sin una `Timeline` que exprese el fin por tiempo: un cue `time_up`, una victoria por decisión sin `ko` y un cierre sin ganador (empate). Mientras no exista, el adaptador de 002 rechaza esos logs con `adapter.unsupported_ending`. Issue: TA-19 ("Timeline v2: fines sin KO"), con `log-decision-a`, `log-decision-b` y `log-draw` como casos de aceptación.
