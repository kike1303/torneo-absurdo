---
id: "011"
title: Repeticiones y verificación en el navegador
type: plan
status: approved
risk: high
author: frontend-engineer
reviewers: [architect]
approvals:
  - by: architect
    date: 2026-11-02
linear: TA-44
depends_on: ["010", "003", "002"]
created: 2026-11-02
updated: 2026-11-02
---

# Plan: Repeticiones y verificación en el navegador (desde spec.md 2026-11-02)

> [intent](./intent.md) · [spec](./spec.md) · ADRs [0002](../../decisions/0002-commit-reveal-semilla.md), [0007](../../decisions/0007-i18n-desde-cero.md), [0009](../../decisions/0009-ui-design-system.md), [0012](../../decisions/0012-logs-y-cabeceras-de-seguridad.md) B4.1 y B6

Escrito en solo lectura sobre `ta-44-repeticiones` (`ab72c22`): leí el código y solo corrí `cargo tree` y `gh run view`. No compilé nada ni corrí pruebas.

**Antes de implementar:** el gate de `risk: high` del owner (la spec lo pide, porque el PR relaja la CSP) y este plan aprobado.

**Un solo PR**, por commits. Cada paso lleva su dueño:
- **[Rust]**: `crates/*`, `apps/server`, `scripts/check-engine.mjs`. Lo puede tomar backend-engineer.
- **[CI]**: `justfile`, `ci.yml`, `release.yml`, `check-workflows`, `compare-trees`. Lo implementa frontend-engineer y lo revisa devops-engineer.
- **[Web]**: `apps/web` y `check-renderer-split`. Lo implementa frontend-engineer.

**No se toca:** `crates/engine`, `packages/renderer`, `packages/protocol-ts` a mano, `apps/server/src/fights/hook.rs`, `apps/server/src/fights/bets_contract.rs`, el ledger, el ciclo de vida, `apps/web/Dockerfile` ni `.dockerignore` (ninguna de sus reglas alcanza a `src/lib/features/replay/`). Sin arte nuevo. `Cargo.lock` no debe cambiar: `ruleset_bounds` no suma dependencias, y `cargo install` del CLI no toca el lock del workspace. Si cambiara, `just crate-age` y `just deny` antes de compilar.

**Entorno de los agentes:** `PATH="$HOME/.nvm/versions/node/v22.23.4/bin:$PATH"` y `corepack pnpm`. El Docker es Colima.

## Archivos que cambian

### [Rust]
| Archivo | Cambio |
|---|---|
| `crates/engine-wasm/src/lib.rs` | `pub fn bounds(version) -> Result<String, WasmError>` pura, con `format!`, y `#[wasm_bindgen] ruleset_bounds` encima. Actualiza el comentario "One exported function" (RF4) |
| `crates/engine-wasm/tests/codes.rs` | `bounds` da el JSON exacto, `ruleset.unknown` con 0 y 2, igualdad con `Ruleset::for_version(1)` |
| `crates/protocol/src/fights.rs`, `src/lib.rs` | `FightsPage`, con su ida y vuelta serde (RF10) |
| `packages/protocol-ts/src/**` | Generado con `just gen` |
| `scripts/check-engine.mjs`, `scripts/__tests__/check-engine.test.mjs` | `checkWasmSources` falla con `\bextern\b` sobre `stripComments`. `checkWasmGraph` falla con `js-sys` o `web-sys` en `cargoTree(['engine-wasm'])` (RF3). Hoy no están: lo comprobé |
| `apps/server/src/fights/store.rs` | Macro `summary_columns!()`, en el orden de `columns!()`, con `NULL::bytea` en las posiciones 26 y 27 y `''::bytea` en la 6 y la 7. `settled_page` (RF8) |
| `apps/server/src/fights/api.rs` | Handler `list`, con `Result<Query<_>, QueryRejection>`: un `before` inválido o repetido da 404 `fight.not_found`, por `parse_id` |
| `apps/server/src/http/routes.rs` | `Handler::FightsList`, `GET /api/fights`, `Effect::Read`, con la misma capa `fight_read_ip` y `no-store` del detalle, antes de `/current` y `/{id}` |
| `apps/server/tests/fights_api.rs` | Orden, páginas, final de la lista, `before` inválido, cabeceras, 503, 429 compartido con el detalle, fila en cada estado, `the_routes_only_read` |
| `apps/server/tests/fights_no_leaks.rs` | `FightsList` en `request_of`, `expected_status` sin comodín (cada `Handler` nombrado, con sus `cfg`), negativo antes de `Settled`, positivo en `Settled`, barrido con `?before=<id+1>` (RF9) |
| `apps/server/tests/fights_ws_parity.rs` | RF41: `GET /api/fights` también da el mismo cuerpo hasta `live_started_at` |
| `apps/server/tests/fights_source_guard.rs` | Prueba de fuente de `summary_columns!`: no nombra `revealed_seed`, `log`, `spec_a` ni `spec_b` como columnas leídas |
| `apps/server/tests/support/public_verifier.rs` | `check_commit` exige 64 hexadecimales en minúscula (desviación D2) |
| `apps/server/tests/fights_public_verify.rs`, `tests/golden/fight-verify-vectors.json` (nuevo) | Escribe con `UPDATE_GOLDEN=1` y comprueba los vectores (RF14, desviación D5) |
| `apps/server/README.md` | `GET /api/fights` |

### [CI]
| Archivo | Cambio |
|---|---|
| `justfile` | `gen-wasm`, `_wasm-cli`, `_wasm-generate <dir>`, `_check-wasm-artifact`, en `check-rust` después de `_check-wasm`. `_e2e-fights-run` suma la corrida sembrada |
| `scripts/compare-trees.mjs` y su test | `--name` y `--command`; sin ellos, el mensaje de hoy para `_check-gen`. Con ellos: "`<name>` is out of date: run `<command>`, review and commit", sin decir que se regeneró, porque aquí el árbol commiteado no se toca |
| `.gitattributes` | `apps/web/src/lib/features/replay/engine-wasm/* -text` y `apps/server/tests/golden/fight-verify-vectors.json -text` |
| `.prettierignore` | Los mismos dos, comentados |
| `.github/workflows/ci.yml` | Filtro `rust` + `apps/web/src/lib/features/replay/engine-wasm/**`; filtro `web` + `apps/server/tests/golden/fight-verify-vectors.json`; `b1=`; `timeout-minutes` de `rust` a 25 (desviación D6) |
| `.github/workflows/release.yml` | Job `wasm-artifact`; `needs: wasm-artifact` en `build`; `b1=`; comentario de cabecera |
| `scripts/check-workflows.mjs`, su test y `scripts/__fixtures__/check-workflows/valid/.github/workflows/{ci,release}.yml` | `JOB_PERMISSIONS['release.yml']['wasm-artifact']`. Función nueva, un caso negativo por regla de RF2: permisos, `if` de job en `wasm-artifact` o `build`, falta `needs`, `_check-wasm-artifact` ausente, con otra condición o con `continue-on-error`, un `rust-cache`, otra caché o `docker/login-action`, y el filtro perdido |

### [Web]
| Archivo | Cambio |
|---|---|
| `apps/web/src/lib/features/replay/engine-wasm/*` | Los cuatro generados por `just gen-wasm` |
| `apps/web/eslint.config.js` y `tests/eslint/*` | `globalIgnores` del directorio. `engineWasmRestriction` en todos los grupos, y un bloque final para `engine-loader.ts` sin ella (RF5). `tests/**` no la lleva: RF3 carga el glue |
| `apps/web/tsconfig.json` | `exclude` de `src/lib/features/replay/engine-wasm/engine_wasm.js`, porque el proyecto tiene `checkJs: true` y `svelte-check` lo revisaría. El import resuelve al `.d.ts` |
| `scripts/check-tokens.mjs` | `EXEMPT` suma ese directorio: escanea `.ts`, y los `.d.ts` son generados |
| `apps/web/src/lib/features/replay/fight-id.ts` (nuevo) | `parseFightId(text): number \| null`, el decimal canónico de RF34. Lo usan el matcher y `before` |
| `apps/web/src/params/fight_id.ts` (nuevo) | `match` sobre `parseFightId` |
| `apps/web/src/lib/features/replay/load-detail.ts` (nuevo) | `parseFightDetail(unknown): FightDetail \| null` (forma de RF13) y `loadDetail(id, fetch)`, que da `ok`, `malformed` (JSON sin la forma), `unavailable` (red, plazo, 4xx, 5xx o un 200 que no es JSON) |
| `apps/web/src/lib/features/replay/verify.ts` (nuevo) | `verifyFight(detail: unknown, expectedId, deps): Promise<Verdict>` y `hashSnapshot`. Sin DOM ni Svelte. Nunca rechaza |
| `apps/web/src/lib/features/replay/engine-loader.ts` (nuevo) | Único que importa `engine-wasm/`, solo con `import()`. `import('./engine-wasm/engine_wasm_bg.wasm?url')`, `init({ module_or_path })`, plazo de 30 s, sin cachear un rechazo. `realDeps()` con `crypto.subtle` |
| `apps/web/src/lib/features/replay/replay-store.svelte.ts` (nuevo) | Estado de la página: detalle, veredicto, timeline, resultado visto, avisos. Recibe `fetch` y `deps` |
| `apps/web/src/lib/features/replay/list-date.ts` (nuevo) | `live_started_at ?? live_at` a `Date` |
| `apps/web/src/lib/features/fight/live/view-state.ts` | Exporta `resultOfFinal`, sin cambiarla |
| `apps/web/src/lib/server/api/reads.ts` | `getFightsPage(api, before: number \| null)` y `getFightDetail(api, id: number)` |
| `apps/web/src/routes/fights/+page.server.ts`, `+page.svelte` (nuevos) | Lista (RF17, RF26) |
| `apps/web/src/routes/fights/[id=fight_id]/+page.server.ts`, `+page.svelte` (nuevos) | Página (RF18, RF23, RF26) |
| `apps/web/src/lib/components/replay/` (nuevo) | `FightList`, `ReplayControls`, `ReplayVerdict`, `ReplayFinalNotice`, `ReplayView`. Datos por props; de features, solo `import type` (ADR-0009 D4) |
| `apps/web/src/lib/components/fight/FightStage.svelte` | RF19 y la desviación D1 |
| `apps/web/src/lib/components/fight/FightHome.svelte`, `FightResultCard.svelte`, `FightNotice.svelte` | RF26. `FightNotice` no está en la tabla de la spec: necesita un enlace opcional para el aviso de RF57 |
| `apps/web/messages/{es,en}.json` | `replay_*` y `replay_verify_*`, con los textos de RF16 y RF22 tal cual |
| `apps/web/vite.config.ts` | `'script-src': ['self', 'wasm-unsafe-eval']` |
| `apps/web/src/routes/dev/components/_sections/ReplaySection.svelte` (nuevo), `dev/components/+page.svelte` | Catálogo de los componentes nuevos, en cada estado |
| `scripts/check-renderer-split.mjs`, su test y fixtures nuevos | `checkEngineSplit` y `checkWasmFile` aparte, para no tocar los cuatro fixtures de hoy (RF5). Imprime el tamaño |

### e2e
| Archivo | Cambio |
|---|---|
| `apps/web/e2e/support/fight-script.mjs` | `REPLAY_VECTORS` aparte, con `decision`. **No** se agrega a `VECTORS`: `fightScript()` lo recorre entero y `fights.spec.ts` dejaría de ver sus peleas |
| `apps/web/e2e/support/db.mjs`, `tests/e2e-db.test.ts` | `seedReplays(url)`, tras `assertE2eTarget`, una transacción por fila |
| `apps/web/e2e/support/seed-replays.ts` (nuevo) | `globalSetup` de la configuración sembrada |
| `apps/web/playwright.replays.config.ts` (nuevo) | `workers: 1`, 4 proyectos, `grep: /@replays/`, `trace: 'off'`, `outputDir: './test-results/fights-artifacts/replays'`, para que el artifact actual de `e2e-fights` recoja sus PNG sin cambiar `ci.yml`. `stackServers({ CONTENT_DIR: NO_ROSTER })` |
| `apps/web/playwright.config.ts` | `grepInvert: /@replays/`; `testIgnore` suma `replays.spec.ts`; WebKit suma las pruebas generales nuevas |
| `apps/web/e2e/replays.spec.ts` (nuevo) | Sembradas y manipulación |
| `apps/web/e2e/fights-list.spec.ts` (nuevo) | General: estado vacío, 404, matcher, carga diferida |
| `apps/web/e2e/fights.spec.ts`, `home.spec.ts`, `csp.spec.ts`, `a11y.spec.ts`, `hydration.spec.ts`, `security.spec.ts` | Ver paso 13 y `CSP_B1` |

### Docs (cierre)
`docs/architecture.md`, `docs/glossary.md`, `docs/features/003-motor-de-pelea/spec.md` (enmienda de RF19), `CLAUDE.md`, como pide "Trabajo de cierre" de la spec.

## Orden de trabajo

### 1. [Rust] Lo que entra al `.wasm`, y los tipos
`ruleset_bounds`, sus tests, `check-engine` y `FightsPage` con `just gen`.

**Va antes del artefacto a propósito:** `engine` depende de `protocol`, así que `FightsPage` cambia la entrada del `.wasm`, y sumar un tipo puede mover los bytes aunque nadie lo use.

Prueba: `cargo test -p engine-wasm -p protocol`, `just _check-engine`, `just _check-gen`, `git diff --exit-code Cargo.lock`.

### 2. [CI] Generar y comprobar el artefacto (RF1, RF2)
```
_wasm-cli         lee la versión de wasm-bindgen en Cargo.lock; si falta target/wasm-tools/<v>/bin/wasm-bindgen:
                  cargo install wasm-bindgen-cli --version =<v> --locked --root target/wasm-tools/<v>
                  (sin el RUSTFLAGS de abajo)
_wasm-generate o  RUSTFLAGS="--remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}/registry/src=/cargo --remap-path-prefix=$PWD=/src"
                  CARGO_TARGET_DIR=target/wasm-artifact cargo build -p engine-wasm --target wasm32-unknown-unknown --release --locked
                  wasm-bindgen --target web --omit-default-module-path --out-dir o .../release/engine_wasm.wasm
gen-wasm          vacía el directorio y llama a _wasm-generate
_check-wasm-artifact  host de `rustc -vV`: x86_64-unknown-linux-gnu o aarch64-apple-darwin sigue; otro, con CI falla y sin CI avisa y sale con 0.
                  genera en mktemp -d, con trap, y corre compare-trees --name engine-wasm --command "just gen-wasm"
```
Luego `just gen-wasm`, commit de los cuatro archivos y los ignores (`.gitattributes`, `.prettierignore`, ESLint, `tsconfig`, `check-tokens`).

Prueba: `just _check-wasm-artifact`; tocar un byte en una copia del árbol falla nombrando `just gen-wasm`; `just check-web` sigue verde con el artefacto dentro.

### 3. [CI] Filtros, release y sus reglas (RF2). **Paralelizable con 4 y 5**
`ci.yml`, `release.yml`, `check-workflows`, fixtures. Forma de `wasm-artifact`: `runs-on: ubuntu-24.04`, `timeout-minutes: 20`, `permissions: { contents: read }`, sin `if` de job. Pasos, todos con `if: github.event_name == 'push'`: `actions/checkout` con el SHA de hoy y `persist-credentials: false`; `./.github/actions/setup-just`; `rustup toolchain install --no-self-update`; `run: just _check-wasm-artifact`. No usa `setup-js`, porque `compare-trees` solo usa built-ins de Node y el runner ya lo trae, igual que `check-release-binary` en ese workflow. `deploy` y `verify` no cambian.

Prueba: `node scripts/check-workflows.mjs` y `pnpm exec vitest run --config scripts/vitest.policy.config.mjs`.

### 4. [Web] Artefacto y verificación pura (RF3, RF11 a RF13). **Paralelizable con 3 y 5**
Depende de 2. En `apps/web/tests/`:
- `replay-artifact.test.ts` (RF3): `initSync` con el `.wasm` commiteado, los 7 dorados byte a byte, `ruleset.unknown`, sin `/Users/` ni `/home/`, y superficie con `WebAssembly.Module.exports` e `imports`.
- `replay-verify.test.ts` (RF11 a RF13, RF24): una fila por caso, con el dato alterado y nulo, incluidos `final: null` y `state: "live"`.

Reglas de `verify.ts`:
- Un `try/catch` externo da `cannot_check(engine_failed)`.
- `engine()` se llama solo en `decided`, tras el commit y RF2.
- Sin `crypto.subtle`, `crypto_unavailable`.
- Código de `simulate`: `seed.*`, `spec.*` e `input.too_large` dan `mismatch(log)`; `ruleset.unknown` da `cannot_check(ruleset_unknown)`; lo demás, `engine_failed`.
- Una `reason` o `kind` desconocidos dan `unknown_final` antes de mirar la semilla (RF22), y una `voided` conocida con `seed` nula da `mismatch(commit)`.
- `switch` exhaustivo con `never`, sobre `kind` y sobre cada `reason`, también la de `decided`.

Prueba: Vitest de `apps/web`, sin server ni navegador. Regla de ESLint y su test en `tests/eslint/`.

### 5. [Rust] Ruta de lista (RF7 a RF9). **Paralelizable con 3 y 4**
`settled_page(pool, before: Option<i32>, lock_timeout)`: `WHERE state = 'settled' AND id < $1::bigint ORDER BY id DESC LIMIT 21`. `$1` es `before` o, sin él, `i64::from(i32::MAX) + 1`: así entra también `i32::MAX`, con una sola consulta. Lee en una transacción con `set_lock_timeout`, como `by_id_bounded`, y el handler pone el mismo `timeout` con `ACQUIRE_TIMEOUT.saturating_mul(3)`.

`next_before` sale de las filas leídas: si llegaron 21, el `id` de la número 20. Una función pura, `summaries_of`, decodifica fila por fila, omite la que no decodifica o no da `summary()` y registra `fights.list_row_invalid` en `warn`, solo con `event` y `error_kind`: `ALLOWED_FIELDS` no tiene `fight_id` y no se le agrega. Unitaria sobre esa función.

Prueba: `cargo test -p server --test fights_api --test fights_no_leaks --test fights_ws_parity --test fights_source_guard` con `just db-up`.

### 6. [Rust] Vectores (RF14)
Depende de 1. `check_commit` más estricto (D2). `fight-verify-vectors.json`: `[{ name, detail, native: "ok" | "err" | "undeserializable", browser: "match:fight" | "mismatch:log" | ... }]`, sin Postgres (D5).

Base: KO, decisión, empate, `voided(engine_error)`, `voided(snapshot_invalid)`, `cancelled(missed_window)` y `cancelled(fight_unrunnable)`. Alteraciones: `log`, `seed`, `seed_commit`, `final`, `summary`, cotas, `final: null`, `state: "live"`, semilla en mayúsculas. Excepciones marcadas: versión desconocida con `snapshot_sha256` recalculado, `final.kind` y `reason` desconocidos.

Prueba: `cargo test -p server --test fights_public_verify`.

### 7. [Web] La misma tabla en el navegador (RF14)
Depende de 4 y 6. `replay-vectors.test.ts` carga el JSON con el `.wasm` real: `ok` da `match` o `not_applicable`, `err` da `mismatch`, y las excepciones dan `cannot_check`. También comprueba que el archivo tenga los nombres esperados, para que nunca pase en vacío.

### 8. [Web + CI] CSP (RF6)
En un commit propio, con las cuatro copias: `vite.config.ts`, `CSP_B1` de `security.spec.ts` y `b1=` de `ci.yml` y `release.yml`. `check-kit-config` no cambia: `CSP_EXTENSIONS.extraSources['script-src']` ya lo admite y su test ya lo cubre.

B6.3 y B6.4 sobre cabeceras no aplican: `security-headers.ts` no lleva la CSP. Lo que falta de B6, `csp.spec.ts` y la justificación en el PR (B6.5), se resuelve en el paso 13 y al abrir el PR.

Prueba: `node scripts/check-kit-config.mjs`, `node scripts/check-workflows.mjs`.

### 9. [Web] Lecturas, matcher y lista (RF10, RF17, RF18 parcial)
`reads.ts`, `fight-id.ts`, `params/fight_id.ts`, `/fights` y `FightList`. Paginación con enlace `?before=`, sin JavaScript. Fecha con `formatDate` en `timeZone: 'UTC'` y `timeZoneName: 'short'`, y local en `onMount`. Un fallo del API da 200 con estado de error.

Prueba: `tests/routes/fights.test.ts`, al estilo de `home.test.ts`, y test del matcher con `../me`, `12?x`, `01` y `2147483648`.

### 10. [Web] Página y reproductor (RF15, RF16, RF18 a RF25)
**`+page.server.ts`:**
- Liquidada: `{ id, a, b, date }`.
- Sin liquidar: `{ pending: true, a, b }`.
- 404 del API: `error(404)`.
- 503, 429 o caída: estado con reintento, con 200.
- Siempre `no-store`, y `liveNotice` desde `getFightsCurrent`.
- Test: su salida no tiene `final`, `seed`, `log` ni `spec_*`, en los cuatro estados.

**`ReplayStore`:**
- En `onMount`, una sola lectura.
- Con `ok`: `JSON.parse` y `timelineFromFightLog` en un `try` (si falla, `replay_notice_playback_error`, sin reproductor), y `verifyFight` una vez sobre ese mismo objeto.
- Reintentar verificación solo con `engine_unavailable`, recargar con `ruleset_unknown`, `unknown_final` y `engine_failed`, y nada con `crypto_unavailable`. Una función pura lo decide, con su test.
- **No importa tipos del renderer.** `tests/fight-boundary.test.ts` fija la lista exacta de archivos que usan `StageSnapshot`. El store declara un tipo estructural propio, `{ t, phase, ending }`, y el `onfailure` recibe `string`.

**`FightStage`:** ver D1.
- `seekBy(ms)`: lee `snapshot().t`. Si `phase === 'finished'`, llama antes a `renderer.pause()`, porque al llegar al final natural `playback.playing` sigue en `true`, y sin esto el salto reanudaría en vez de dejar en pausa como pide RF19. Después, `seek` y `refresh`.
- `seekToEnd()`: `seek(loadedTimeline.duration_ms)`. `seek` conserva `playing`, y `settledState` da `finished`.
- `toggle()` y `restart()` ya hacen lo que pide RF19.

**`ReplayView`:**
- `FightStage` con `showResult={false}`, `source`, `fighters={{ a: 'dummy_a', b: 'dummy_b' }}` y `displayNames`.
- `ReplayControls`: 5 botones `size="icon"` (`size-11`, 44 px) en `grid-cols-5` y `aria-label` de Paraglide. Caben en 343 px útiles a 375 px.
- `RoundClock` con `roundClockMs(time_limit_ms, log.events, snapshot.t)`.
- El resultado, en su región `aria-live`, con `resultOfFinal`, cuando `ending` o `finished`, o con el botón. Una vez visto queda visto.

**`ReplayVerdict`:** RF16 completo, con `aria-live="polite"`, el límite fuera de `<details>`, datos copiables con `copy-text.ts`, omisiones de `cancelled` y enlace a `/api/fights/{id}`.

Prueba: Vitest del store con `fetch` y `deps` simulados, `replay-copy.test.ts` (RF16: palabras prohibidas en en y es, y la frase del límite junto a `match`), y `check-i18n`.

### 11. [Web] Cómo se llega, y catálogo (RF26)
`FightHome` con su enlace, y prop `replayHref` en `FightResultCard`, solo con `fight.state === 'settled'`. `FightNotice` con enlace opcional. El aviso "hay una pelea en vivo" en las dos páginas. `ReplaySection` en `/dev/components`.

### 12. [Web] Carga diferida (RF5)
Depende de 10. `checkEngineSplit` sigue el patrón de `checkRendererSplit`: ningún entry alcanza `/replay\/engine-wasm\//` por imports estáticos, y el nodo que contiene `src/routes/fights/[id=fight_id]/+page.svelte` lo alcanza por uno dinámico. `checkWasmFile`: exactamente un `.wasm` en `.svelte-kit/output/client`, igual byte a byte al commiteado, sin `data:application/wasm` en ningún chunk, e imprime el tamaño.

Prueba: `pnpm --filter web build && node scripts/check-renderer-split.mjs`, y tests con fixtures nuevos.

### 13. [Web + CI] e2e (la estrategia de pruebas de la spec)
**Siembra.** Ya comprobé contra `20261031000000_fights.sql` que el trayecto es legal:
- `INSERT` en `scheduled`.
- `fight_secrets`.
- `betting_open` con `seed_commit` y `opened_at`.
- `betting_closed`.
- `live`.
- `settled` con `revealed_seed` y `log`.
- `DELETE` del secreto.

`cancelled` va de `scheduled` a `settled`. `voided` va por `betting_closed>settled` o `live>settled`, con semilla y sin log. `opens_at` debe ser único. Cotas: 86000 y 90000; si estuvieran mal, el e2e da `mismatch(bounds)`.

**Orden en `_e2e-fights-run`,** tras la corrida actual: `db.mjs reset`, `a9 --check`, `reset-run.mjs` y `playwright test -c playwright.replays.config.ts`. **Confirmado en Playwright 1.64.1** (`createGlobalSetupTasks`): los plugins, y con ellos `webServer`, arrancan antes de `globalSetup`, así que el server ya migró. El `teardown` de la corrida anterior detiene los procesos por su pid antes del `reset`.

**`replays.spec.ts`, describe con `{ tag: '@replays' }`:**
- KO, decisión y empate: +10 s hasta `finished`, resultado, −10 s en pausa, reproducir hasta el final natural (unos 10 s), reiniciar, y estado de cada control.
- Nada de ganador antes de reproducir (RF20), personajes sin lore (RF25).
- `voided`, `cancelled`, `scheduled` da "todavía no".
- Sin WebGL (`getContext`, como `home.spec.ts`), `.wasm` bloqueado, `webglcontextlost` (D1).
- Manipulación: se lee el detalle una vez en la prueba y se responde con `route.fulfill`, para no gastar `FIGHT_READ_IP`. Log, semilla, commit, `final: null` y `state: "live"` dan "no coincide"; la versión desconocida, con `snapshot_sha256` recalculado, y `final` desconocido dan "no se pudo comprobar".
- Canarios: `addDynamicCanary` con cada semilla y un fragmento de cada log. `global-teardown` los busca en todos los logs.
- Capturas a 375 y 1280 px, es y en, dark y light, en `test-results/screenshots/`.

**`fights-list.spec.ts` (general):** vacío de `/fights`, 404 de `..%2Fme`, `12%3Fx`, `01` y `2147483648`, sin líneas `route` de `/api/fights` nuevas en `SERVER_LOG` (`http.request` lleva `route`), y cero peticiones de `.wasm` y de Pixi en `/` y `/fights`, con la lista de chunks de `.vite/chunk-modules.json`.

**Existentes:**
- `fights.spec.ts`: "todavía no" en `live`; tras liquidar, KO y empate dan "coincide" y resultado publicado.
- `home.spec.ts`: enlaces con `fake-ws`.
- `csp.spec.ts`, `a11y.spec.ts` y `hydration.spec.ts`: casos con `@replays`.

Prueba: `just e2e` y cronómetro de la corrida nueva, objetivo 3 min.

### 14. Cierre
Docs. Revisión visual contra la spec, móvil y escritorio, dark y light, es y en. `just check` con la salida pegada, `just e2e`. PR con la justificación de `'wasm-unsafe-eval'` (B6.5). Revisan security-reviewer, architect, qa-engineer, devops-engineer (`justfile`, `ci.yml`, `check-workflows`) y backend-engineer (la ruta).

### Commits
| Commit | Pasos |
|---|---|
| `feat(engine-wasm): add ruleset_bounds and the fights page type (TA-44)` | 1 |
| `build(web): commit the engine wasm with a byte-exact freshness check (TA-44)` | 2 |
| `ci: check the wasm artifact on release and in the path filters (TA-44)` | 3 |
| `feat(web): verify a fight in the browser (TA-44)` | 4 |
| `feat(server): add GET /api/fights (TA-44)` | 5 |
| `test(replay): share verification vectors with the native verifier (TA-44)` | 6, 7 |
| `feat(web): allow wasm-unsafe-eval in the CSP (TA-44)` | 8 |
| `feat(web): add the fights list and the replay page (TA-44)` | 9, 10, 11 |
| `test(web): check the lazy loading and the replays end to end (TA-44)` | 12, 13 |
| `docs(replay): close the feature (TA-44)` | 14 |

**Si se separa Rust:** backend-engineer entrega 1, 5 y 6. La web solo espera a 1 (el artefacto y los tipos) y a 6 (los vectores). `settled_page` no bloquea hasta el e2e.

## Riesgos
- **El paso de más riesgo: un binario sin leer en el diff.** → RF2 en `check-rust` y en `wasm-artifact`, y RF5 sobre lo que emite Vite.
- **TA-27 cambia `crates/protocol/src/fights.rs`, y eso mueve el `.wasm`.** Las dos ramas pueden pasar por separado y dejar `main` con un binario viejo. → Quien fusione segundo corre `just gen-wasm` y actualiza contra `main` antes de fusionar. En un conflicto de binario no se fusiona nada: se regenera. **Hay que avisar a esa sesión.** También chocan `routes.rs`, `fights_no_leaks.rs` (`expected_status` sin comodín deja de compilar con un `Handler` nuevo, a propósito) y los mensajes.
- **Mac y Linux pueden dar bytes distintos tras `ruleset_bounds`:** la igualdad se midió antes de ese cambio. → El job `rust` lo dirá en el primer push. Plan B de la preocupación 2: generar en el contenedor `rust` con `--platform linux/amd64` (Colima con `--vz-rosetta`) y quitar el Mac de la allowlist.
- **`rust` queda justo de tiempo.** Una corrida de `main` (2026-11-01) tardó 14 min 10 s, con límite de 20. Suma instalar el CLI (51 s en un Mac) y compilar el `.wasm` en release. No verifiqué si `rust-cache` conserva `target/wasm-tools`; si no, se instala en cada corrida. → D6 sube el límite a 25, y el PR anota el tiempo medido.
- **`checkJs` y el glue.** → `exclude` en `tsconfig`. Si `svelte-check` aún lo revisa, se ve en el paso 2, no al final.
- **`FIGHT_READ_IP` en la corrida sembrada:** 120/min y ráfaga de 30, y todo sale de una IP. → `workers: 1` y `route.fulfill`. Si da 429, menos visitas; nunca subir el límite.
- **Tiempo de `e2e-fights`:** hoy, 5 min 19 s, con límite de 15. → Objetivo de 3 min más. Si se pasa, Chromium y WebKit móviles, sin bajar `csp.spec.ts`.
- **`serve-with-log.mjs` vacía los logs al arrancar:** la corrida sembrada pisa los de la anterior. Solo se pierden si la primera pasó, porque la receta se detiene en el primer fallo. Se acepta.
- **La fila `scheduled` sembrada con un runner sin roster.** `Roster::load` da `None` y el runner sigue vivo, pero no anuncia. → `opens_at` a 7 días, y las pruebas no dependen de `/api/fights/current`.
- **Hidratación de fechas:** el ICU de Node y el del navegador pueden diferir en detalles, y en producción Svelte no avisa. → `<time datetime>` estable, y la prueba mira el texto UTC del SSR y el local tras `onMount`.
- **`fight_notice_degraded` con enlace:** `home.spec.ts` no debe romperse por el texto. → Enlace aparte, sin cambiar la llave.

## Alternativas descartadas
- **`StageSnapshot` en el store y sumar el archivo a la lista de `fight-boundary.test.ts`.** Afloja una frontera para ahorrar un tipo de tres campos.
- **Una lectura real en cada prueba de manipulación con `route.fetch()`.** Gasta cuota por nada: la respuesta es la misma.
- **Agregar `decision` a `VECTORS`.** Cambia el guion de la corrida real.
- **Reutilizar `live-store.svelte.ts`.** Lleva socket y reloj, y aquí no hay ninguno.
- **Barra o reloj propio en `ReplayControls`.** RF19 y RF20 los prohíben.
- **Vectores leídos de peleas reales en Postgres.** Ver D5.

## Pruebas (cómo se demuestra que funciona)
- [ ] `just check` y `just e2e` en verde, con la salida pegada
- [ ] RF1, RF2: tocar un byte falla `_check-wasm-artifact`; `check-workflows` con un negativo por regla, también del filtro
- [ ] RF3: `replay-artifact.test.ts` y `check-engine.test.mjs`
- [ ] RF4: `crates/engine-wasm/tests/codes.rs`
- [ ] RF5: `check-renderer-split` y `fights-list.spec.ts`
- [ ] RF6: `csp.spec.ts` en `/es/fights` y en una pelea con verificación terminada, en Chromium y WebKit; `security.spec.ts`; `check-workflows` con `b1=` idénticos
- [ ] RF7 a RF9: `fights_api.rs`, `fights_no_leaks.rs`, `fights_ws_parity.rs`, `fights_source_guard.rs`
- [ ] RF10: `_check-gen` y `svelte-check`
- [ ] RF11 a RF13, RF24: `replay-verify.test.ts`
- [ ] RF14: `fights_public_verify.rs` y `replay-vectors.test.ts`
- [ ] RF15, RF16: `replay-copy.test.ts` y test del store
- [ ] RF17, RF18, RF23: `tests/routes/fights*.test.ts`, test del matcher, `fights-list.spec.ts`, `fights.spec.ts`, `hydration.spec.ts`
- [ ] RF19 a RF22, RF25: `replays.spec.ts`
- [ ] RF26: `home.spec.ts`
- [ ] Logs (ADR-0012 A4): canarios de `global-teardown`
- [ ] `a11y.spec.ts`: axe en las dos páginas, en dark y light
- [ ] Capturas a 375 y 1280 px, es y en, dark y light, comparadas con la spec
- [ ] `git diff --exit-code Cargo.lock packages/renderer crates/engine`

## Estimación de CI
| Job | Hoy | Con este PR |
|---|---|---|
| `rust` | 14 min 10 s (una corrida de `main`) | Unos 16 a 18 min. Límite: de 20 a 25 (D6). El PR anota lo medido |
| `web` | Sin medir | Unos segundos más (RF3, RF14) |
| `e2e` | Sin medir | Unos segundos más (`fights-list.spec.ts`, split) |
| `e2e-fights` | 5 min 19 s (la misma corrida) | Unos 8 a 9 min. Límite de 15, sin cambios |
| `wasm-artifact` (release) | No existe | Unos 3 a 6 min, solo en un tag, sin caché |

Son estimaciones sin ejecutar nada. No hay jobs nuevos en `ci.yml`.

## Desviaciones de la spec
Se proponen aquí para que architect las apruebe con el plan.

- **D1. RF19 dice que `FightStage` "solo suma dos métodos"; con eso RF21 no se cumple.** Hoy, `watchContext` solo corre con `live`: sin él, nadie atiende una pérdida de contexto y nunca se llega a `onfailure`. Además, `start()` volvería a `initialT`, y `watchSnapshots` se detiene en `finished`, lo que dejaría sin datos al reiniciar. **Alternativa:** tres cambios más, solo si el host pasa `onsnapshot`, cosa que `/dev/arena` y el catálogo no hacen:
  1. El camino sin `live` también llama a `watchSnapshots` y `watchContext`.
  2. `watchSnapshots` solo se detiene en `finished` con `live`.
  3. Al perder el contexto, guarda `snapshot.t` y `start()` vuelve a ese punto, en pausa.

  `packages/renderer` sigue sin cambios.
- **D2. RF14 dice que hay dos excepciones, y hay una tercera sin declarar.** `hex_decode` de `public_verifier.rs` acepta mayúsculas y hasta `+a` (`from_str_radix`), mientras RF13 exige minúsculas: la misma semilla en mayúsculas da `Ok` nativo y `mismatch(commit)` en el navegador. **Alternativa:** `check_commit` exige 64 dígitos en minúscula, y un vector lo fija. Es solo el verificador de pruebas, y el server ya emite minúsculas.
- **D3. RF19 y "Calidad" del intent en WebKit.** WebKit en Linux no tiene WebGL (`arena-webkit.spec.ts`, `csp.spec.ts`), así que en CI los controles no se pueden probar allí. **Alternativa:** igual que esas specs, la prueba ramifica por `data-state`. Con `webgl_unavailable` comprueba el aviso, el botón de resultado y la verificación completa, y se anota `limited`. Chromium cubre los controles, y se repite a mano en Safari antes de cerrar el PR, con el resultado en la descripción.
- **D4. Rollback real de `release.yml`.** security-reviewer dejó "probarlo con el PR", pero no se puede: `workflow_dispatch` solo existe desde la rama por defecto, exige un tag ya publicado en GHCR, y no hay ninguno ni se crean (TA-8 pospuesto). **Alternativa:** `check-workflows` fija la forma en este PR, y se agrega a "Prerrequisitos del primer despliegue" de `docs/architecture.md` que el primer rollback de prueba llegue hasta `verify`.
- **D5. RF14 pide `FightDetail` "reales".** Leerlos de Postgres haría depender el archivo de `id` y de la hora, y el test de escritura necesitaría base de datos. **Alternativa:** se construyen con `Vector::of`, los bytes reales del motor, specs canónicas y horas fijas, y cada `ok` debe pasar `verify`. `every_kind_of_final_verifies_with_public_data` ya cubre la lectura real.
- **D6. Límite de `rust`, que la spec deja en 20 min.** Quedaría con unos 2 min de margen. **Alternativa:** 25, en el mismo PR, con la revisión de devops-engineer. Se baja si lo medido lo permite.

**Agregados que la spec no nombra**, sin cambiar lo que pide: `tsconfig.json`, `check-tokens.mjs`, `FightNotice.svelte`, `view-state.ts` (un `export`), `fight-id.ts`, `list-date.ts`, `replay-store.svelte.ts`, y `.prettierignore` y `.gitattributes` también para los vectores.

**Confirmado sin desviación:**
- `check-kit-config` ya admite la fuente.
- `js-sys` y `web-sys` no están en el grafo de `engine-wasm`.
- Los triggers dejan el trayecto de la siembra.
- `webServer` arranca antes de `globalSetup`.
- `http.request` lleva `route`, así que RF18 se puede comprobar en el log.

## Desviaciones durante la implementación
- **Paso 2, `check-tokens`.** El directorio generado entra en `SKIP_DIRS`, junto a `paraglide`, y no en `EXEMPT`: esa lista compara el final de la ruta de un archivo, y un directorio no le sirve.
- **Paso 5, `summary_columns!`.** No repite la lista de `columns!`: las dos salen de una macro `columns_with!`, que solo cambia las posiciones de los specs y de la revelación. Así el orden no puede divergir, y `nothing_writes_a_voided_final`, que cuenta dos `void_reason` en `store.rs`, sigue igual. La prueba de fuente revisa `summary_columns!` y `columns_with!`.
- **Paso 5, `settled_page`.** Devuelve `PageRow { id, row }`, con el `id` aparte, para que `next_before` salga de las filas leídas aunque la fila 20 no decodifique. `page_of` arma la página sobre `summaries_of`.
- **Paso 5, prueba nueva.** `tests/fights_list_logs.rs` comprueba el evento `fights.list_row_invalid` con la capa real de logs. Va en su propio binario, por la regla de `tests/test_layout.rs`.
- **Paso 6, forma de los vectores.** Cada línea suma dos campos a los del plan: `expected_id`, el `id` de la URL que recibe `verifyFight`, porque un `detail` sin forma no lo tiene, y `exception`, la marca de RF14, que es `true` solo con `cannot_check`. Un `fight.id` distinto del de la URL no va en el archivo: el verificador nativo no conoce la URL, así que sería una tercera diferencia. Lo prueba `replay-verify.test.ts`.

## Revisión
### frontend-engineer — 2026-11-02 — autor
- Lo escribí contra el código de esta rama, sin implementar nada. No compilé ni corrí pruebas: los tiempos de CI salen de una sola corrida de `main` y las sumas son estimaciones. Pendiente: architect. Yo no lo apruebo.

### architect — 2026-11-02 — APPROVE
- Verifiqué: que cada RF de la spec (RF1 a RF26), los RNF, "Estrategia de pruebas" y "Trabajo de cierre" tienen archivo, paso y prueba; que lo puede implementar alguien sin esta conversación; el orden (lo que mueve el `.wasm` antes del artefacto), y que respeta ADR-0001, 0002, 0007 y 0012 y no toca `hook.rs`, `bets_contract.rs`, `crates/engine` ni `packages/renderer`. Contra el código: las posiciones de `summary_columns!`, `reachedEnd` para `seekToEnd`, `fights_ws_parity.rs` para RF41, y que la comparación de `b1=` ignora el orden de las fuentes. No compilé ni corrí nada.
- **Desviaciones: acepto D1 a D6.** Todas cambian algo de la spec, y las enmendé allí con la marca *Enmienda plan*. Sobre D3: si `just e2e` en un Mac tiene WebGL en WebKit, esa corrida ya es la comprobación fuera de CI; pega sus anotaciones `limited` en el PR, y usa Safari a mano solo si no lo cubre. Los "agregados" no cambian ningún contrato.
- **Condiciones al implementar**, sin reescribir el plan. Son huecos de pruebas o de detalle, y cuentan como parte de él:
  1. Paso 10: `malformed` de `loadDetail` da `mismatch(data)`, sin reproductor, resultado ni botón (RF24 enmendado). Falta en `ReplayStore`, que solo describe `ok`, y `unavailable` da el estado con reintento, sin veredicto.
  2. `replays.spec.ts` suma un caso con `reducedMotion: 'reduce'` (RF21, y lo pide el intent) y comprueba que el `<title>` no lleva el resultado (RF20), que tampoco aparece en el paso 10.
  3. RF22 solo da sus cuatro motivos en es: escribe los de en con el mismo alcance, y `check-i18n` los exige.
  4. `NO_ROSTER` es una constante local de `playwright.config.ts`: la configuración nueva define la misma ruta o la comparten desde `e2e/support/`.
  5. El aviso de pelea en vivo de RF26 sale del SSR, así que `fake-ws` no lo alcanza: lo prueban los tests de `load`, y `fights.spec.ts` lo comprueba mientras la pelea está en `live`.
- Sigue pendiente, antes de implementar: el gate de `risk: high` del owner, y que security-reviewer y product-owner vean sus filas de la enmienda de la spec.
