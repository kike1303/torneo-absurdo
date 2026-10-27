---
id: "005"
title: "Timeline v2: fines sin KO"
type: plan
status: approved
risk: medium
author: frontend-engineer
reviewers: [architect]
approvals: [{by: architect, date: 2026-10-27}]
linear: TA-19
depends_on: ["002", "003"]
created: 2026-10-27
updated: 2026-10-27
---

# Plan: Timeline v2, fines sin KO (desde spec.md 2026-10-27)

> [intent](./intent.md) · [spec](./spec.md) · [plan-c de 002](../002-spike-renderer/plan-c.md) · [spec de 003](../003-motor-de-pelea/spec.md) · ADRs [0004](../../decisions/0004-personajes-homenaje.md), [0007](../../decisions/0007-i18n-desde-cero.md), [0009](../../decisions/0009-ui-design-system.md), [0012](../../decisions/0012-logs-y-cabeceras-de-seguridad.md)

## Para el architect: lo que tienes que decidir
1. **RF17 con `pace = 0.8` no se puede cumplir con los dorados tal como están.** En `log-decision-a`, `log-decision-b` y `log-draw`, la separación mínima entre impactos es de 700 ms. 700 × 0,8 = 560, por debajo de `strikeGapMs` (600), así que `validateTimeline` da `timeline.strike_gap` sin importar el cierre. `log-ko-comeback` tiene el mismo problema hoy, y es el comportamiento documentado de `retime.ts` ("a factor that breaks a rule ends in `timeline_invalid`, never forced"). Propongo que el test de RF17 compruebe **(a)** con 1.5 y con 0.86 (el menor factor de dos decimales que deja 700 ms en ≥ 600), que la timeline pasa, y **(b)** con 0.8, que el **único** código es `timeline.strike_gap`: así se demuestra que `time_up`, `win` y `draw` se reubican bien. Si lo apruebas, lo anoto como desviación de RF17 en "Desviaciones" y en la descripción del PR. No se toca el motor ni `PACE_MIN`.
2. **Timelines de los dorados para los tests del renderer** (decisión 4). RF5, RF7 y RF10 piden `sceneAt`, `snapshot()` y movimiento reducido "con las timelines de los dorados". El renderer no puede importar el adaptador (vive en `apps/web`, y la frontera de 002 no se mueve). Propongo commitear las tres timelines **generadas** por el adaptador en `packages/renderer/fixtures/golden/`, con un test de frescura en `apps/web` (`toMatchFileSnapshot`). Alternativa: armarlas en el renderer con `TimelineBuilder` imitando solo el tramo final. Es más barata, pero ya no son "las timelines de los dorados".

## Decisiones de implementación
1. **Primero, una foto de la v1.** Antes de tocar `validate.ts` se commitea un test de caracterización: el resultado completo de `validateTimeline` (con `ok`, `errors`, rutas y `data`) para cada fixture de `test/fixtures/timeline/`, el guion `spike.timeline.json` y dos entradas v1 con `time_up` y `draw`, guardado con `toMatchFileSnapshot` en `test/__snapshots__/timeline-v1.snap.json`. Después de la refactorización, el snapshot no cambia. Así RF2 ("idéntico al de hoy para cualquier entrada") se prueba contra el código de antes y no contra lo que yo recuerde de él. El único cambio es la entrada de `unsupported_version.json` (pasa de `2` a `3`), y el resultado sigue igual (mismo código y ruta).
2. **Despacho por versión** según "Implementación del despacho" de la spec. `run` acepta `1 | 2` (`TIMELINE_VERSIONS.includes`), y si no, da `unsupported_version`. Pasa `version` a `parseCue` (`CUE_TYPES_V1` o `CUE_TYPES`) y a `checkCues`. Del bucle común salen las reglas de cierre:
   - `checkEndingV1` recibe por cue lo que hoy hace el bucle con `ko`/`win` (las líneas `cueAfterWin`/`cueAfterKo`, las ramas `ko` y `win` y los `koCount`/`winCount` "por ausencia"), movido **sin cambios**.
   - `checkEndingV2` es una pequeña máquina de estado por cue (`ko`, `timeUp`, `win`, `draw` con su índice) y una función de cierre al final del recorrido para `ending_missing`, `ending_conflict`, `closing_missing`, `closing_during_knockdown` y `duration_too_short`. `knockdowns` ya se acumula en el bucle común y se le pasa.
   - Una sola pasada y un solo `ErrorSink`. El `Timeline` devuelto conserva la versión de la entrada (`timeline_version: version`).
3. **Orden de los códigos de la v2 dentro de un cue** (para que cada fixture dé un código determinado): `cue_after_win` → `cue_after_draw` → `cue_after_ko` (cualquier cue que no sea el `win` del KO, incluido `draw`) → `cue_after_time_up` (cualquier cue que no sea `win` o `draw`). Después, las reglas propias del tipo: `time_up_count`, `ending_conflict` (un `time_up` con `ko` previo, o un `ko` con `time_up` previo, o un segundo cierre de otro tipo), `draw_without_time_up`, `draw_count`, `draw_gap`/`win_gap`. Así un `win` y un `draw` después de un `time_up` dan `ending_conflict` y no `cue_after_win` + `cue_after_time_up`, como pide RF3.3. Si al escribir los fixtures sale un caso con dos códigos, el fixture comprueba el que pide la spec (`toContainEqual`, igual que hoy).
4. **Timelines generadas de los dorados** (punto 2 de arriba): `packages/renderer/fixtures/golden/{decision-a,decision-b,draw}.timeline.json` las escribe `apps/web/tests/golden-timelines.test.ts` con `expect(json).toMatchFileSnapshot(path)` a partir de `timelineFromFightLog(readGoldenLog(...), { a: dummy_a, b: dummy_b })`. Se regeneran con `vitest -u`. En CI (`CI=true`), Vitest no escribe y falla si difieren. Se suman a `.prettierignore` como contenido generado (igual que los `manifest.json` de los placeholders), para que Prettier no pelee con `JSON.stringify(…, null, 2)`. El renderer las lee con un import relativo desde `test/support/scenes.ts` (`goldenScene('draw')`). La frontera de 002 queda igual: el renderer sigue sin conocer el `FightLog`.
5. **Escena.** `PreparedScene` suma `timeUp: { at } | null` y `draw: { at } | null`. `phaseAt` queda `finished` > `draw` > `win` > `time_up` > `ko` > `fight` > `intro`. `ending` se calcula en `sceneAt` a partir del cierre (`win ?? draw`): `'ko'` si hay `ko`, `'decision'` si hay `timeUp` y `win`, `'draw'` si hay `draw`. `choreography.ts` agrega `case 'time_up': case 'draw': break;` en el `switch` de eventos, y la rama final pasa de `else` a `else if (cue.type === 'ko' || cue.type === 'win')`, porque con la unión nueva `cue.fighter` no compila. `effectsAt` ya ignora todo lo que no sea `strike` o `ko`: los finales nuevos no tocan el PRNG ni los `hashSeed`. Además, `time_up` y `draw` van **al final** de la lista de cues, así que no cambian los índices (ni las semillas) de los cues anteriores, y un KO sigue dando la misma escena bit a bit.
6. **Banner `time`.** `bannersAt` empuja `{ code: 'time', ...tweenIn(t - timeUp.at) }` desde el `time_up`. `stage/banners.ts` suma `'time'` a `CODES` y en `paint` le da `colors.text`, porque no es un golpe (P1: sin destello). El color `crit` queda para `ko`. Solo hay roles del tema, ningún color suelto.
7. **Player.** La ventana de `recorder.record` pasa a `t <= (p.win ?? p.draw)?.at`. `snapshot()` suma `ending: scene.ending`.
8. **Adaptador.** `VERSION = 2`. `time_up`: `unresolved()`, `timeUps.push({ t })` y el cue. `end`: se valida por `reason` con la tabla de RF12 y RF13. Un `reason` fuera de `'ko' | 'decision' | 'draw'` (se lee como `string` para escapar del `never` de TypeScript) da `unsupported_ending` con `{ reason }`. Cualquier incoherencia da `outcome_mismatch` con `{ kos, time_ups }`. `CLOSING_DELAY_MS = 1500` se exporta, y un test lo compara con `TIMELINE_LIMITS.winGapMs`, igual que `WIN_DELAY_MS`. El comentario de cabecera pasa de "timeline v1" a "timeline v2".
9. **`retime.ts`.** El "ancla" de cierre es el último `ko` o `time_up` (se escala con `scale`). `win` y `draw` se desplazan con el ancla, conservando su separación, y la cola conserva su largo (`end = closing ?? anchor`). Para un KO el resultado es el mismo que hoy, y lo comprueba el test actual de `retime`, que no cambia.
10. **HUD.** En `FightStage.svelte`, un `$derived` de `snapshot?.ending` elige entre `arena_winner`, `arena_winner_decision` (los dos con `data-testid="fight-winner"`) y `arena_draw` (`data-testid="fight-draw"`), dentro de la misma región `aria-live`. El botón "repetir" sale en las tres. `data-ending={debug ? snapshot?.ending : undefined}`. `labels.banners.time = m.arena_banner_time()`. No hay componentes nuevos, así que `/dev/components` no cambia.
11. **WebKit.** Hay un spec nuevo, `e2e/arena-webkit.spec.ts`, con un único test (`?log=draw&t=99999999999` → `finished`, o `data-error="webgl_unavailable"` con la anotación `limited`, copiando el patrón de `csp.spec.ts`). Se suma al `testMatch` del proyecto `webkit` y se excluye de `chromium` con `testIgnore`, porque Chromium ya lo cubre en `arena.spec.ts`. Si el resultado es `limited`, la prueba manual en Safari (macOS, `just dev`, `/es/dev/arena?log=draw` y `?log=decision-a` hasta el final) va en la descripción del PR (nota 2 de product-owner).

## Archivos que cambian
| Archivo | Cambio |
|---|---|
| `packages/renderer/src/timeline/types.ts` | `TIMELINE_VERSIONS`, `TimelineVersion`, `TIMELINE_VERSION = 2`, `CUE_TYPES_V1`, `CUE_TYPES` con `time_up`/`draw`, `TimeUpCue`, `DrawCue`, `ENDINGS`/`Ending`, `Timeline.timeline_version: TimelineVersion` (RF1) |
| `packages/renderer/src/timeline/codes.ts` | 10 códigos nuevos (tabla de la spec) |
| `packages/renderer/src/timeline/validate.ts` | despacho por versión, `checkEndingV1` (movido sin cambios), `checkEndingV2`, `CUE_FIELDS` de `time_up`/`draw` (RF2, RF3) |
| `packages/renderer/src/timeline/index.ts` | comentario "v1 y v2" |
| `packages/renderer/src/scene/types.ts` | `BannerCode` + `time`, `Phase` nueva, `Scene.ending` (RF6, RF7) |
| `packages/renderer/src/scene/prepare.ts` | `timeUp` y `draw` en `PreparedScene`; comentario de `K` (máximo 11) (RF9) |
| `packages/renderer/src/scene/choreography.ts` | `time_up`/`draw` sin eventos y la rama `ko`/`win` explícita (RF5) |
| `packages/renderer/src/scene/banners.ts` | banner `time` (RF6) |
| `packages/renderer/src/scene/scene-at.ts` | `phaseAt` y `ending` (RF7) |
| `packages/renderer/src/player/types.ts` | `RendererSnapshot.ending`; `labels.banners` exige `time` por `BannerCode` (RF6, RF7) |
| `packages/renderer/src/player/player.ts` | ventana de stats hasta el cierre y `snapshot().ending` (RF7, RF8) |
| `packages/renderer/src/stage/banners.ts` | `CODES` + `time` y su color en `paint` (RF6) |
| `packages/renderer/fixtures/golden/{decision-a,decision-b,draw}.timeline.json` | nuevos, generados (decisión 4) |
| `packages/renderer/test/fixtures/timeline/unsupported_version.json` | `value: 3` ("Áreas de preocupación" 4) |
| `packages/renderer/test/fixtures/timeline-v2/` | nuevo: `_valid.json` (decisión), `_valid.draw.json`, `_valid.ko.json` (variantes válidas como `ops`) y un fixture por código nuevo (RF3) |
| `packages/renderer/test/__snapshots__/timeline-v1.snap.json` | nuevo: caracterización de la v1 (decisión 1) |
| `packages/renderer/test/support/patch.ts` | `loadFixtures` devuelve también las variantes `_valid.*.json` |
| `packages/renderer/test/support/timeline-builder.ts` | opción `version` y `finishByTime(timeUpAt, winner: Side \| null)` |
| `packages/renderer/test/support/scenes.ts` | `goldenTimeline(name)` y `goldenScene(name)` sobre `fixtures/golden/` |
| `packages/renderer/test/support/fake-stage.ts` | `banners.time` en las labels |
| `packages/renderer/test/timeline.test.ts` | cobertura de códigos = unión de `timeline/` y `timeline-v2/` |
| `packages/renderer/test/timeline-v1.test.ts` | nuevo: caracterización (decisión 1, RF2) |
| `packages/renderer/test/timeline-v2.test.ts` | nuevo: fixtures v2, variantes válidas, superconjunto con el guion y `_valid.json` (RF3, RF4) |
| `packages/renderer/test/scene.test.ts` | fases, `ending`, banner `time`; cota de `cueWindow` ≤ 11 con `time_up` y cierre (RF5 a RF7, RF9) |
| `packages/renderer/test/choreography.test.ts` | "antes del cierre, igual que sin el `time_up`"; ganador en `win`, perdedor y empate en `idle` (RF5) |
| `packages/renderer/test/player.test.ts` | `snapshot()` en `finished` con el empate; ventana de stats hasta `draw` con reloj falso (RF7, RF8) |
| `packages/renderer/test/reduced-motion.test.ts` | los mismos casos con `log-draw` y `log-decision-a` (RF10) |
| `apps/web/src/lib/features/fight/timeline-from-fight-log.ts` | v2, `time_up`, `decision`, `draw`, `CLOSING_DELAY_MS`, errores (RF11 a RF13) |
| `apps/web/src/lib/features/fight/engine-logs.ts` | `decision-a`, `decision-b`, `draw` en `ENGINE_LOGS` (RF15) |
| `apps/web/src/lib/features/fight/fixtures/log-{decision-a,decision-b,draw}.json` | nuevos, `cp` de los dorados (RF15) |
| `apps/web/src/lib/features/arena/retime.ts` | ancla `ko`/`time_up`, cierre `win`/`draw` (RF17) |
| `apps/web/src/lib/components/fight/FightStage.svelte` | resultado por `ending`, `data-ending`, `banners.time` (RF16) |
| `apps/web/messages/es.json`, `apps/web/messages/en.json` | `arena_banner_time`, `arena_winner_decision`, `arena_draw` |
| `apps/web/tests/support/golden-logs.ts` | `UNSUPPORTED_LOGS` → `TIME_LOGS` y su comentario (RF14) |
| `apps/web/tests/timeline-from-fight-log.test.ts` | `EXPECTED.timeline_version: 2`, decisión y empate sintéticos, cada incoherencia de RF13, `reason` desconocido, `CLOSING_DELAY_MS` (RF11 a RF13) |
| `apps/web/tests/fight-log-goldens.test.ts` | `TIME_LOGS` válidos con ganador correcto y sin `win` en el empate; los siete validan con v1 y v2 cuando aplica (RF4, RF14); copias byte a byte y conjunto exacto (RF15 a, b) |
| `apps/web/tests/golden-timelines.test.ts` | nuevo: frescura de `packages/renderer/fixtures/golden/` (decisión 4) |
| `apps/web/tests/engine-logs.test.ts` | las cuatro llaves (RF15 c) y la carga de `draw` |
| `apps/web/tests/retime.test.ts` | dorados por tiempo con 1.5, 0.86 y 0.8 (RF17, ver la decisión 1 del architect) |
| `apps/web/e2e/arena.spec.ts` | RF18 (es y en, los tres logs: `finished`, `data-ending`, ganador o empate, `data-last-cue` del `time_up`, consola limpia) |
| `apps/web/e2e/arena-webkit.spec.ts` | nuevo (decisión 11) |
| `apps/web/e2e/screenshots.spec.ts` | `arena-time` de los tres logs en `t = 88500`, a 375×812 y 1280×800, en es y en, dark y light |
| `apps/web/playwright.config.ts` | `testMatch` de `webkit` + `arena-webkit.spec.ts`; `testIgnore` del mismo en `chromium` |
| `.prettierignore` | `packages/renderer/fixtures/golden/`; comentario de `fixtures/` del arena ("las copias", no "la copia") |
| `.gitattributes` | solo el comentario de `fixtures/`, si lo tiene (RF15) |
| `docs/architecture.md` | viñeta "Timeline propia" y trabajo derivado 6 resuelto por TA-19 (RF20) |
| `docs/glossary.md` | entrada *Cue* con `time_up` y `draw` (RF20) |
| `docs/features/005-timeline-v2-fines-sin-ko/spec.md` | solo la errata de la nota 1 de product-owner ("cuatro textos"/"4 mensajes" → tres). Lo marco en el PR para que lo vea el architect, autor de la spec |

No cambian: `crates/**`, `packages/protocol-ts/**`, `apps/server/**`, `infra/**`, `content/**`, la CSP de `apps/web/vite.config.ts`, el formato de assets, los placeholders ni `packages/renderer/fixtures/spike.timeline.json`.

## Orden de trabajo
Cada paso termina con sus tests en verde y un commit (`feat(renderer): … (TA-19)`, `feat(web): … (TA-19)`, `test(…)`, `docs: … (TA-19)`).

0. **Entorno y línea base.** `export PATH="$HOME/.nvm/versions/node/v22.23.4/bin:$PATH"` en cada shell y `node -v` (≥ 22.17). Comprobar que `pnpm` resuelve vía corepack (si no, usar `corepack pnpm`). Correr `corepack pnpm --filter @torneo/renderer test` y `corepack pnpm --filter web exec vitest run` para tener la línea base en verde antes de tocar nada.
1. **Caracterización de la v1** (decisión 1): `timeline-v1.test.ts` y su snapshot, generado con el código **actual**. Commit aparte, antes de cualquier cambio en `src/`.
2. **Formato y validador** (RF1 a RF4): `types.ts`, `codes.ts`, `validate.ts`; `unsupported_version.json` → 3; `patch.ts` con variantes; `timeline-v2/` y `timeline-v2.test.ts`; cobertura por unión. Correr `tsc` del renderer ya en este paso: la unión `Cue` nueva rompe la compilación en `choreography.ts` y en el adaptador, y conviene ver la lista completa. El arreglo mínimo de `choreography.ts` (la rama `ko`/`win` explícita) entra aquí para que compile. El snapshot del paso 1 no cambia.
3. **Escena, player y stage** (RF5 a RF10, con timelines del `TimelineBuilder`): `prepare.ts`, `scene-at.ts`, `banners.ts` (los dos), `types.ts`, `player.ts`, comentario de `K`; tests de fases, `ending`, banner, cota de `cueWindow` y stats; `fake-stage.ts`.
4. **Adaptador y dorados en `apps/web`** (RF11 a RF14): adaptador, `golden-logs.ts`, tests del adaptador y de los dorados. Después, `golden-timelines.test.ts` con `vitest -u` para escribir `packages/renderer/fixtures/golden/`, y `.prettierignore`.
5. **Tests del renderer con los dorados** (RF5, RF7, RF10): `scenes.ts` (`goldenScene`), `choreography.test.ts`, `player.test.ts` y `reduced-motion.test.ts` con `decision-a`, `decision-b` y `draw`. Depende del paso 4.
6. **Arena y HUD** (RF15 a RF17):
   - 6a, **paralelizable desde el paso 0:** `cp crates/engine/tests/golden/log-{decision-a,decision-b,draw}.json apps/web/src/lib/features/fight/fixtures/`, los mensajes en `es.json`/`en.json` y los comentarios de `.prettierignore`/`.gitattributes`;
   - 6b, después del paso 4: `engine-logs.ts`, `retime.ts`, `FightStage.svelte` y sus tests. `corepack pnpm --filter web run sync` para compilar los mensajes antes de `svelte-check`.
7. **e2e** (RF18, RF19): `arena.spec.ts`, `arena-webkit.spec.ts`, `screenshots.spec.ts` y `playwright.config.ts`. Con la máquina cargada, primero un build y una corrida acotada (ver "Pruebas"). Revisar las capturas a 375 y 1280 en es y en, y en dark y light (las capturas ya iteran temas), y ajustar si "¡Tiempo!" o "Gana … por decisión" desbordan.
8. **Docs** (RF20), **paralelizable** con los pasos 3 a 7: `architecture.md`, `glossary.md` y la errata de la spec.
9. **Cierre:** `just check` completo y `just e2e` una vez. Si WebKit queda `limited`, prueba manual en Safari. Descripción del PR con la lista de fixtures que cambian ("Áreas de preocupación" 4), el resultado de Safari y la desviación de RF17 si se aprueba.

## Riesgos
- **Regresión silenciosa de la v1 al mover las reglas de cierre** → la caracterización del paso 1 se commitea antes y es la red. Si el snapshot cambia, se arregla el código, no el snapshot.
- **La unión `Cue` nueva rompe `switch` y ramas `else` que asumían `fighter`** (choreography, retime, adaptador y tests) → `tsc` en el paso 2, antes de seguir. ESLint del renderer ya exige exhaustividad donde la había.
- **RF17 con 0.8 imposible con los dorados** → decisión 1 del architect. No se afloja `strikeGapMs` ni `PACE_MIN`.
- **Fixtures generados que derivan** (decisión 4) → `toMatchFileSnapshot` falla en CI. Si el motor regenera un dorado, fallan la copia byte a byte (RF15 a) y la frescura, y los dos se arreglan con `cp` y `vitest -u`.
- **Un `knockdown` después del `time_up`** daría `cue_after_time_up`. Lo leí en `crates/engine/src/sim.rs`: el motor emite `time_up` y `end` solo entre acciones atómicas, así que todo `knockdown` sale antes. No hace falta tolerarlo.
- **`closing_during_knockdown` con un `ruleset` futuro** ("Áreas de preocupación" 3 y nota 1 de game-designer) → hoy no pasa. Queda documentado en la spec, y este plan no lo cambia.
- **WebKit sin WebGL** → `limited` y prueba manual en Safari, con el resultado en el PR (nota 2 de product-owner).
- **Máquina cargada: e2e lentos o con timeouts** → corridas acotadas con `-g` y `--project`, y `just e2e` completo una sola vez al final. Un timeout no se "arregla" subiendo el límite sin entender la causa.
- **`just check` necesita Docker** (`check-rust` hace `db-up`) aunque este cambio no toque Rust → si Docker no responde, se reporta el fallo en `db-up` con la salida, sin darlo por bueno.
- **Pureza del renderer** → no entra `Math.random`, `Date`, `performance` ni `crypto`. La regla de ESLint de `packages/renderer` lo bloquea, y los finales nuevos no usan el PRNG (decisión 5).
- **Peso del bundle** → tres JSON de ~14 KB, cada uno en su chunk diferido y solo en `/dev/arena`. `check-renderer-split` no cambia.

## Alternativas descartadas
- **Recalcular el resultado de la decisión en el adaptador:** descartada en la spec ("Áreas de preocupación" 2).
- **Exportar `./scene` desde `@torneo/renderer` y probar los dorados desde `apps/web`:** abre una API pública nueva solo para tests, y `snapshot()` necesita además el `FakeStage`, que vive en `test/support` del renderer.
- **Adaptador duplicado en un helper del renderer:** dos implementaciones que pueden divergir. Rompe la idea de "un solo archivo conoce las dos formas".
- **Probar RF2 solo con los `expect` actuales de los fixtures** (`toContainEqual`): no detecta errores extra ni rutas distintas. Por eso, la caracterización completa.
- **Correr todo `arena.spec.ts` en WebKit:** duplica el tiempo de e2e y falla en Linux sin WebGL. Basta con un test acotado.

## Pruebas (cómo se demuestra que funciona)
Todos los comandos, con `PATH="$HOME/.nvm/versions/node/v22.23.4/bin:$PATH"` y desde la raíz del worktree.

- [ ] `just check` en verde, con la salida pegada en el reporte.
- [ ] RF2: `corepack pnpm --filter @torneo/renderer exec vitest run test/timeline-v1.test.ts test/timeline.test.ts test/spike-timeline.test.ts`, con el snapshot de v1 sin cambios (`git diff --exit-code packages/renderer/test/__snapshots__/`).
- [ ] RF3 y RF4: `corepack pnpm --filter @torneo/renderer exec vitest run test/timeline-v2.test.ts`. Un fixture por código nuevo, las variantes de empate y de KO válidas, el superconjunto con el guion y `_valid.json`.
- [ ] RF5 a RF10: `corepack pnpm --filter @torneo/renderer exec vitest run test/scene.test.ts test/choreography.test.ts test/player.test.ts test/reduced-motion.test.ts`, con la cota de `cueWindow` ≤ 11 y `snapshot()` del empate `{ phase: 'finished', winner: null, ending: 'draw' }`.
- [ ] RF10 (pureza): `corepack pnpm --filter @torneo/renderer lint`.
- [ ] RF11 a RF17: `corepack pnpm --filter web exec vitest run tests/timeline-from-fight-log.test.ts tests/fight-log-goldens.test.ts tests/golden-timelines.test.ts tests/engine-logs.test.ts tests/retime.test.ts tests/fight-boundary.test.ts`.
- [ ] i18n y tipos: `corepack pnpm --filter web run sync`, después `corepack pnpm --filter web exec svelte-check` y `node scripts/check-i18n.mjs` (los dos también corren en `just check`).
- [ ] RF18 y RF19 acotados, mientras se itera: `corepack pnpm --filter web build && corepack pnpm --filter web exec playwright test e2e/arena.spec.ts --project=chromium -g "decision|draw|ko-comeback"`, y `… e2e/arena-webkit.spec.ts --project=webkit`.
- [ ] Capturas: `corepack pnpm --filter web exec playwright test e2e/screenshots.spec.ts --project=chromium -g "arena-time"`. Reviso a ojo `apps/web/test-results/screenshots/` a 375×812 y 1280×800, en es y en, dark y light: banner "¡Tiempo!"/"Time!" y pose en la decisión, dos `idle` en el empate, sin scroll horizontal.
- [ ] `just e2e` completo una vez al final (Chromium y WebKit, con `check-renderer-split`).
- [ ] WebKit: si el test sale `limited`, prueba manual en Safari con `just dev` (`/es/dev/arena?log=draw` y `/en/dev/arena?log=decision-a` hasta `finished`) y el resultado en la descripción del PR.
- [ ] Restricciones: `git diff --stat main -- crates packages/protocol-ts apps/server infra content apps/web/vite.config.ts` vacío.

## Estimación de CI
No hay workflows nuevos ni cambiados. `web` suma unos segundos de Vitest. `e2e` suma unos 12 tests de Chromium, 24 capturas (3 logs × 2 tamaños × 2 idiomas × 2 temas) y 1 test de WebKit: estimo de +1 a +2 min por corrida de `e2e`.

## Desviaciones durante la implementación
1. **Timelines dorados en `packages/renderer/test/fixtures/golden/`**, no en `packages/renderer/fixtures/golden/` (decisión 4). El `package.json` del renderer exporta `./fixtures/*`, así que en `fixtures/` quedaban al alcance del `exports` del paquete, y la condición (a) del architect pide que no. Dentro de `test/` solo las lee `test/support/scenes.ts`. El test de frescura (`apps/web/tests/golden-timelines.test.ts`) y `.prettierignore` apuntan ahí. El test falla si un archivo difiere y también si falta (condición b): guarda si el archivo existía antes de correr y solo lo deja pasar con `vitest -u`. Lo comprobé a mano con `CI=true`: un archivo alterado falla, y uno borrado falla y no se escribe.
2. **Orden de los códigos (nota del architect sobre la decisión 3).** Un segundo cierre de otro tipo después de `time_up` (un `draw` tras el `win` de una decisión, o al revés) da **solo** `ending_conflict`, porque esa comprobación va antes que `cue_after_win` y `cue_after_draw`. Lo prueba `timeline-v2.test.ts` con `toEqual` sobre la lista completa de errores. Los demás casos con dos códigos (p. ej. `ko` y `time_up`, que dan `cue_after_ko` y `ending_conflict`) se comprueban con `toContainEqual`, como permite la spec.
3. **Caracterización de la v1 (decisión 1).** Cuando el resultado es `ok`, el snapshot guarda `timeline_version`, `duration_ms` y el número de cues, no la timeline entera (sería del tamaño del guion). La igualdad completa del `Timeline` entre v1 y v2 la prueba RF4 en `timeline-v2.test.ts` y en `fight-log-goldens.test.ts`.
4. **Capturas extra.** Además de las 24 `arena-time-*` en t = 88 500, `screenshots.spec.ts` toma 6 capturas `arena-time-end-*` en `finished`, a 375 px y en es y en. Sirven para ver que el texto del resultado ("Gana Muñeco A por decisión", "Dummy B wins by decision", "Empate") cabe sin scroll horizontal, porque en t = 88 500 todavía no aparece.
5. **Fixtures de v2 adicionales.** Además de un fixture por código nuevo, hay cuatro variantes de códigos reutilizados en v2: `ending_conflict.ko`, `win_gap.time_up`, `duration_too_short.draw` y `cue_after_ko.draw`.

Nota sin desviación: `closing_during_knockdown` se aplica también en KO, como pide RF3.5. Comprobé que una v1 válida siempre la cumple, así que RF4 se mantiene. Ningún golpe cae dentro de un `knockdown` (`strike_during_knockdown`), el golpe que deja HP 0 va seguido del `ko` (`hp_zero_without_ko`) y no puede haber un `knockdown` después del `ko` (`cue_after_ko`).

## Revisión
### architect — 2026-10-27 — APPROVE
- Verifiqué: cobertura de RF1 a RF20 en la tabla de archivos y en el orden; la caracterización de la v1 antes de tocar `validate.ts` (red buena para RF2); que el despacho sigue "Implementación del despacho" de la spec; que `time_up`/`draw` al final de la lista de cues no mueven índices ni semillas, y que no usan el PRNG (ADR-0001/RNF de pureza); la frontera de 002 (el renderer no importa el adaptador ni el `FightLog`); que no se tocan `crates/**`, `protocol-ts`, server ni CSP; y medí en los siete dorados que la separación mínima entre `strike` es 700 ms.
- **Decisión 1 (RF17): aceptada.** Se verifica 1.5 y 0.86 (pasan) y 0.8 (único código `timeline.strike_gap`). Ya enmendé RF17 en la spec con nota fechada, así que **no es una desviación**: no hace falta anotarla en "Desviaciones" ni en el PR como tal. Si 0.86 rompe otra regla (p. ej. `strike_during_knockdown`, porque el knockdown conserva su duración mientras los impactos se comprimen), eso sí es un hallazgo: se reporta, no se ajusta el factor a ciegas.
- **Decisión 2 (timelines dorados en el renderer): aceptada** la opción de commitear las generadas en `packages/renderer/fixtures/golden/` con frescura en `apps/web` (`toMatchFileSnapshot`). Condiciones: (a) el renderer solo las lee desde `test/`, nunca desde `src/` ni el `exports` del paquete; (b) el test de frescura falla si falta un archivo, no solo si difiere (en CI Vitest no escribe, confírmalo en el PR).
- La errata de la spec ("cuatro textos" → tres) ya la corrigió el orquestador, y yo corregí "4 mensajes" → "3 mensajes" en "Componentes afectados". La fila de `spec.md` de "Archivos que cambian" y esa parte del paso 8 ya están hechas: **la implementación no toca `spec.md`**.
- Nota, no bloqueante: la decisión 3 pone `cue_after_win` antes que `ending_conflict`, así que un `draw` después del `win` de una decisión emitiría `cue_after_win` primero, no solo `ending_conflict` como dice el párrafo. Basta con que el fixture `ending_conflict` lo contenga (`toContainEqual`, como permite la spec); si se quiere exclusividad, que el cierre de otro tipo tras `time_up` se evalúe antes de `cue_after_win`/`cue_after_draw`. Anotar en "Desviaciones" lo que se elija.

