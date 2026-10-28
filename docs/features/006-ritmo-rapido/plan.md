---
id: "006"
title: Ritmo rápido del motor
type: plan
status: approved
risk: high
author: backend-engineer
reviewers: [architect]
approvals: [{by: architect, date: 2026-10-27}]
linear: TA-21
depends_on: ["002", "003", "005"]
created: 2026-10-27
updated: 2026-10-27
---

# Plan: ritmo rápido del motor (desde spec.md 2026-10-27)

> [intent](./intent.md) · [spec](./spec.md) · [spec de 003](../003-motor-de-pelea/spec.md) · [spec de 005](../005-timeline-v2-fines-sin-ko/spec.md) · [spec de 002](../002-spike-renderer/spec.md) · ADRs [0001](../../decisions/0001-motor-propio-determinista.md), [0002](../../decisions/0002-commit-reveal-semilla.md), [0007](../../decisions/0007-i18n-desde-cero.md)

Un solo PR, en la rama `ta-21-faster-pacing`. Lo implementan dos ingenieros, en el orden de "Orden de trabajo". **B** es backend-engineer (motor, dorados y simulador) y **F** es frontend-engineer (arena web). Algunos pasos de documentación los firman otros autores (**A**, architect; **G**, game-designer), porque son dueños de esos textos.

## Para el architect: lo que tienes que decidir
1. **`paceMin` y `resolvePace` reciben el validador como parámetro** (desviación de firma de RF15 y RF16). La spec dice `paceMin(timeline)`, con `validateTimeline` llamado dentro de `retime.ts`. Así no se puede: `apps/web/eslint.config.js` (`rendererRestriction`, ADR-0009 D4) solo deja importar valores del renderer en `FightStage.svelte`, y `scripts/check-renderer-split.mjs` (002 RF-A17) falla si la ruta `dev/arena` llega a `packages/renderer/` por un import estático. Por eso propongo:
   - `paceMin(timeline: Timeline, isValid: (t: Timeline) => boolean): number | null` y `resolvePace(requested: number, timeline: Timeline, isValid: …): PaceResolution`. Siguen siendo funciones puras, y `retime.ts` mantiene solo `import type` del renderer.
   - La prop `retime` de `FightStage` pasa de `(timeline) => Timeline` a `(timeline, isValid) => Timeline`. `FightStage` ya tiene el módulo cargado con `import()` y le pasa `(t) => rendererModule.validateTimeline(t).ok`, sin manifests, como pide RF15.
   - Los Vitest le pasan `(t) => validateTimeline(t).ok`, porque los tests sí pueden importar valores.

   La regla sigue teniendo una sola fuente (`TIMELINE_LIMITS` y el validador). Alternativa descartada: calcular el mínimo dentro de `FightStage`. Metería lógica de `?pace` en un componente genérico del catálogo. Si lo apruebas, lo anoto en "Desviaciones" y en el PR.
2. **Denominador de la no vacuidad de RF7.** "Las peleas que no terminan antes de `breather_after_resolutions` resoluciones": para que un respiro se pueda **observar** hace falta una resolución después de la que lo abre. Propongo contar las peleas con **más de** `breather_after_resolutions` resoluciones (≥ 10 en v1). Si con los fixtures de `tests/fixtures/` (que no son el `content/` de la calibración) la proporción sale por debajo del 90 %, no cambio ni el umbral ni el denominador: lo escalo con la cifra medida.
3. **Coste del test de propiedad nuevo.** "Sobre los mismos 10 000 logs" significa simular otra vez los 10 000 pares (seed, par), una sola vez cada uno y no dos. Mido el tiempo en *debug* de `cargo test -p engine --test properties` antes y después. Si crece más de un 20 %, paso el cálculo al bucle de `every_log_is_deterministic_and_respects_rf2` mediante un helper, conservo el nombre `breathers_happen_and_keep_the_contract` en un test que solo cubre los extremos, y lo registro como desviación.
4. **Semillas raíz de los espejos (RF13).** Propongo `DEFAULT_SEED` (`746f726e…`, la de `just sim`), `S2 = 00…01` (63 ceros y un `1`) y `S3 = a5` × 32. Son fijas y van tal cual en el PR, para que cualquiera repita la medición.

## Decisiones de implementación
1. **Primero el contrato, sin tocar bytes.** `TIMELINE_CONTRACT` y los literales de `assert_invariants` entran en un commit aparte, **antes** del respiro. En ese commit los 7 `log-*.json` y el digest no cambian (solo aparece `timeline-contract.json`), y eso demuestra que el contrato no es una entrada de la simulación (RF5, preocupación 1).
2. **Campos del ruleset sin uso, después el respiro.** Los cuatro campos y los dos derivados (RF1) entran con el test de cotas (RF6) en su propio commit, todavía sin bytes nuevos. El commit siguiente es el único que cambia la simulación y regenera todo.
3. **El respiro en `Fight`.** Hay un campo nuevo, `since_breather: u32`, y un método `fn breather<R: Rolls>(&mut self, rolls: &mut R, knockdown: bool)`. Se llama al final de `action`, después de `self.last = Some(side)` y solo en la rama sin KO, cuando los préstamos `atk`/`def` ya terminaron. La lógica es la de RF2:
   - con knockdown, el contador vuelve a 0 y no se sortea nada;
   - si no, `+= 1`, y con `>= breather_after_resolutions()` se hace `roll(PERMILLE)`;
   - si el resultado es `< breather_chance_permille()`, se calcula `d = breather_min_ms() + roll(bound(breather_span_ms().saturating_add(1)))`, se aplica `saturating_add(d)` a los dos `next_at` y el contador vuelve a 0.

   No hay ningún evento ni ningún cambio en `next_actor`.
4. **El roller guionado de `sim.rs`** suma `Step::Breather` y `Step::BreatherSpan`. Después del jitter, el paso siguiente se decide por la cota pedida: `n == 1000` es el paso 6 y cualquier otra es la elección. Los luchadores de esos tests tienen pesos totales de 150 a 280 y nunca llegan a 1000; un `assert_ne!` en la elección lo deja explícito. `Plan` suma `breather: bool`, que responde 0 o 999 en el paso 6 (por defecto `false`, así que los tests actuales no cambian), y `breather_ms: u32`, que es la respuesta a `roll(801)` y comprueba `n == 801`. El roller también anota en qué acciones se pidió el paso 6 (`breather_asks: Vec<usize>`), y así los tests comprueban cuándo se sortea y cuándo no.
5. **`rolls_follow_the_rf7_order_in_a_real_fight`** pasa a construir lo esperado **por acción**: la resolución, más el `knockdown` o el `ko` que la siguen en el log. El `Recorder` guarda también los valores devueltos, y el test replica el contador. Tras cada acción sin KO ni knockdown espera `1000` si el contador llegó a 9, y además `801` si el valor devuelto fue `< 200`.
6. **Dorados.** Se regeneran con `UPDATE_GOLDEN=1` (nativo, nunca en CI; `check_golden` lo impide con `CI`). `search_golden_seeds` imprime, junto a cada candidato, cuántas separaciones ≥ `breather_gap_min()` tiene. De cada objetivo elijo la semilla más baja que cumpla las reglas de siempre: pares variados, ningún vector que sea otro con los lados cambiados y el empate como espejo de `turtle`. Entre las de KO, prefiero las que tienen ≥ 2 pausas. El dorado de KO con más pausas es **el dorado de RF20**, y el PR lo nombra.
7. **Simulador.** `Facts` (por pelea) suma `min_gap_ms`, `burst_gaps` y `pause_gaps`, y `Tally` suma `pauses: Vec<u32>` (pausas por pelea, que se ordena en `finish` como `resolutions`) y `fights_with_pause`.
   - `Density.min_gap_ms` es 0 si no hay separaciones, y en ese caso no se evalúa `StrikeGap` (la media ya falla).
   - Las bandas se comparan con productos cruzados en `u64`: `gap_total_ms >= mean_gap_min_ms * gaps`, `gap_total_ms <= mean_gap_max_ms * gaps`, y `at_least` para la ráfaga y las peleas con pausa.
   - `Failure` suma `MeanGap { ms }`, `BurstShare { permille }`, `PausesMedian { count }`, `PausedFights { permille }` y `StrikeGap { ms }`, cada uno con su `describe_failure` en inglés.
8. **Arena.**
   - La página guarda `paceInfo = $state<PaceResolution | null>(null)`. El callback `retime` calcula `resolvePace` y lo guarda en `paceInfo`. Si `min === null`, devuelve la `Timeline` sin tocar; si no, `retimeTimeline(t, effective)`.
   - `paceMin` calcula el factor como `k / 100` (`86 / 100 === 0.86` en IEEE 754) y nunca como `k * 0.01`, para que la igualdad de `clamped` sea exacta.
   - `ArenaStats` recibe una prop opcional `pace`. Con ella suma la fila `pace`, con `data-testid="arena-stats-pace"`, `data-clamped` y la nota, y mete en el JSON `pace_requested`, `pace_effective` y `pace_min`.
   - La sección del catálogo (`ArenaSection.svelte`) muestra un caso con clamp, para que la fila nueva quede a la vista en `/dev/components`.
9. **Copias de la arena.** `ENGINE_LOGS` queda con este orden de llaves: `ko-comeback`, `ko-clutch`, `ko-decisive-crit`, `ko-knockdown`, `decision-a`, `decision-b`, `draw`. Las primeras cuatro existentes se mantienen y las nuevas van detrás de `ko-comeback`, agrupadas por final. Los tests que hoy fijan cifras de un dorado concreto pasan a leerlas del JSON importado: `max_hp` 700/860 y ganador `b` en `engine-logs.test.ts`, y "b gana con a en 0 HP" en `arena.spec.ts` (preocupación 5).

## Archivos que cambian
| Archivo | Quién | Cambio |
|---|---|---|
| `crates/engine/src/contract.rs` | B | nuevo: `TimelineContract` (`u32`, `Debug, Clone, Copy, PartialEq, Eq, Serialize`) y `pub const TIMELINE_CONTRACT` con 2000/600/800/3000/1500/2000 (RF5) |
| `crates/engine/src/lib.rs` | B | `mod contract; pub use contract::{TIMELINE_CONTRACT, TimelineContract};` y una línea en el doc del crate (RF5) |
| `crates/engine/src/ruleset.rs` | B | los 4 campos `breather_*` con su doc en la macro, sus valores en `v1()`, `strike_gap_min()` y `breather_gap_min()` como `pub const fn`; `v1_respects_the_bounds` según RF6 (RF1, RF6) |
| `crates/engine/src/sim.rs` | B | `since_breather` y `Fight::breather` (RF2, RF3); el doc del módulo cita los pasos 6–7; el roller guionado, `rolls_follow_the_rf7_order_in_a_real_fight` y los unitarios nuevos (decisiones 3 a 5) |
| `crates/engine/tests/common/mod.rs` | B | `assert_invariants` sin literales: `TIMELINE_CONTRACT.{intro_gap_ms, strike_gap_ms, knockdown_min_ms..=knockdown_max_ms, win_gap_ms + win_pose_ms}` (RF5) |
| `crates/engine/tests/properties.rs` | B | `breathers_happen_and_keep_the_contract` (RF7) |
| `crates/engine/tests/golden.rs` | B | vector (d), `timeline_contract_matches_its_golden`; `VECTORS` con semillas nuevas; dos condiciones en `vectors_cover_every_event_and_moment`; `search_golden_seeds` imprime las pausas (RF9) |
| `crates/engine/tests/golden/log-*.json` (7), `digest-1000.txt` | B | regenerados (RF9) |
| `crates/engine/tests/golden/timeline-contract.json` | B | nuevo, regenerado con `UPDATE_GOLDEN=1` (RF5) |
| `crates/sim/src/report.rs` | B | `BURST_GAP_MAX_MS = 900` y `PAUSE_GAP_MIN_MS = 2500` junto a `DURATION_BAND_MS`; `Density`, `Facts` y `Tally` con las métricas de RF11; `Criteria` con 5 campos nuevos e `in_band_min_permille: 900`; 5 `Failure` nuevos; la línea `pacing: …` del informe de texto (RF11, RF12) |
| `crates/sim/src/report/tests.rs` | B | bordes 900/901 y 2500/2499; percentiles de pausas; cada `Failure` nuevo con un log construido; `the_criteria_are_the_ones_of_rf17`; los 3 tests de atadura con `Ruleset::v1()` y `TIMELINE_CONTRACT` (RF11, RF12) |
| `crates/sim/tests/cli.rs` | B | las llaves nuevas de `density` en `--json` (RF11) |
| `crates/sim/tests/fixtures/balanced/twin-{a,b}/character.json` | B | `speed` más bajo, el mismo cambio en los dos (decisión del paso B5) (RF13) |
| `crates/sim/tests/balance.rs` | B | sin cambios de código; pasa con `Criteria::RF17` nuevo (RF13) |
| `apps/web/src/lib/features/fight/fixtures/log-*.json` | B (las 4 existentes) y F (las 3 nuevas) | `cp` byte a byte de los 7 dorados (RF10) |
| `packages/renderer/test/fixtures/golden/{decision-a,decision-b,draw}.timeline.json` | B | regenerados con `vitest -u` (RF10) |
| `apps/web/src/lib/features/fight/engine-logs.ts` | F | 7 llaves (decisión 9) y el comentario de cabecera (RF10) |
| `apps/web/src/lib/features/arena/retime.ts` | F | `parsePace` solo valida la sintaxis; se van `PACE_MIN` y su comentario; entran `PACE_FLOOR = 0.5`, `paceMin`, `resolvePace` y `PaceResolution`; `PACE_MAX` se queda (RF14 a RF16, decisión 1 del architect) |
| `apps/web/src/lib/components/fight/FightStage.svelte` | F | firma de `retime` con `isValid` (decisión 1 del architect) |
| `apps/web/src/routes/dev/arena/+page.svelte` | F | `resolvePace` dentro del callback, `paceInfo` y la prop `pace` de `ArenaStats` (RF16, RF17) |
| `apps/web/src/lib/components/fight/ArenaStats.svelte` | F | fila `pace`, nota de clamp, `data-clamped` y las llaves del JSON (RF17) |
| `apps/web/src/routes/dev/components/_sections/ArenaSection.svelte` | F | ejemplo con `pace` y clamp (decisión 8) |
| `apps/web/messages/es.json`, `apps/web/messages/en.json` | F | `arena_stats_pace` ("Ritmo (factor)" / "Pace (factor)") y `arena_stats_pace_clamped` ("ajustado desde {requested}" / "clamped from {requested}") (RF17) |
| `apps/web/tests/timeline-contract.test.ts` | F | nuevo: lee `timeline-contract.json` con `GOLDEN_DIR` y lo compara llave a llave con `TIMELINE_LIMITS`, con el conjunto exacto de las 6 llaves (RF5) |
| `apps/web/tests/retime.test.ts` | F | `parsePace` (solo sintaxis); `paceMin`/`resolvePace` sobre los 7 dorados adaptados y el guion; quita los casos fijos de 0.8/0.86 de 005 RF17, que pasan a ser los de RF16 y RF17 (RF14 a RF16) |
| `apps/web/tests/engine-logs.test.ts` | F | 7 llaves; cifras de `ko-comeback` leídas de la copia (decisión 9) |
| `apps/web/tests/fight-log-goldens.test.ts` | F | sin cambios de lógica (ya es genérico); se revisa que cubra las 7 copias (RF10) |
| `apps/web/e2e/arena.spec.ts` | F | los 7 `?log=` con (a), (b) y (c); copiar JSON con `ko-knockdown&pace=0.8`; ganador y HP leídos del JSON; test del knockdown sobre `ko-knockdown` (Estrategia de pruebas) |
| `apps/web/e2e/arena-webkit.spec.ts` | F | los 7 logs con `pace=0`, `finished` o `limited` (Estrategia de pruebas) |
| `.claude/skills/write-character/SKILL.md` | G | nota de `speed` y la media de ritmo; criterios de ritmo y 90 % en la lista de "Balance" (preocupación 3) |
| `docs/features/003-motor-de-pelea/spec.md` | A | enmiendas de RF18 (RF4, RF7, tabla del ruleset, cotas, nota de RF17, cifras medidas) |
| `docs/features/005-timeline-v2-fines-sin-ko/spec.md` | A | enmiendas de RF18 (RF15 con 7 copias, RF17) |
| `docs/features/002-spike-renderer/spec.md` | A | enmiendas de RF18 ("Datos para 003", RF-B9) |
| `docs/architecture.md` | A | "Motor de pelea" (respiro, `TIMELINE_CONTRACT`) y "Renderer de peleas" (los 7 `?log=`) (RF18) |
| `docs/features/006-ritmo-rapido/spec.md` | owner y A | nota del owner de RF20 en `## Revisión`; cifras reales de RF13 si difieren de la calibración |
| `docs/features/006-ritmo-rapido/plan.md` | B y F | "Desviaciones" |

No se tocan `crates/protocol`, `packages/protocol-ts`, `crates/engine-wasm/src`, `crates/content`, `content/`, `packages/renderer/src`, `apps/server`, `Cargo.lock` ni los `package.json` (RF19, sin dependencias nuevas).

## Orden de trabajo
Entorno: el shell puede traer Node 18. Antes de cada comando de JS va `export PATH="$HOME/.nvm/versions/node/v22.23.4/bin:$PATH"`, y las llamadas directas usan `corepack pnpm …`. La máquina está cargada: durante el trabajo solo se corren los tests del área tocada, y `just check` y `just e2e` completos se reservan para los puntos marcados.

**B0 (B, antes de tocar código).**
1. Linear: TA-21 queda en *In Progress* al aprobarse este plan.
2. Se crean las relaciones "TA-21 **bloquea** TA-13" y "TA-21 bloquea" el issue del scheduler (se busca en Linear; si no existe, se anota en el comentario de TA-21). Es el pendiente del product-owner sobre la preocupación 4.
3. Se comprueba que en `main` siga sin fusionarse nada que guarde peleas: `git log origin/main` y `apps/server/Cargo.toml` sin `engine`.

**B1 (B). Contrato sin cambio de bytes.**
1. `contract.rs`, el `pub use` y `assert_invariants` con `TIMELINE_CONTRACT`.
2. El vector (d) en `golden.rs`, más `UPDATE_GOLDEN=1 cargo test -p engine --test golden` para crear `timeline-contract.json`.
3. Prueba: `git status` muestra **solo** `timeline-contract.json` como archivo nuevo en `tests/golden/`, sin ningún `log-*.json` ni `digest-1000.txt` modificado, y `cargo test -p engine` pasa.
4. Commit `feat(engine): timeline contract mirror (TA-21)`.

**B2 (B). Campos del ruleset y cotas.**
1. Los 4 campos, los 2 derivados y `v1_respects_the_bounds` según RF6. `max_events_bound()` sigue en 418.
2. Prueba: `cargo test -p engine` pasa sin bytes nuevos.
3. Commit `feat(engine): breather fields in ruleset v1 (TA-21)`.

**B3 (B). El respiro y los dorados.** Es el paso más riesgoso.
1. `Fight::breather` (decisión 3), el roller guionado y los unitarios (decisiones 4 y 5). Primero corren `cargo test -p engine --lib`.
2. `cargo test -p engine --test golden -- --ignored --nocapture search_golden_seeds`, para elegir las semillas nuevas de `VECTORS` (decisión 6).
3. `UPDATE_GOLDEN=1 cargo test -p engine --test golden`, para regenerar los 7 logs y el digest. Después, la misma orden **sin** `UPDATE_GOLDEN`, que tiene que pasar. Se comprueba que los `spec-*.json` no cambiaron (`git diff --exit-code crates/engine/tests/golden/spec-*.json`); si cambiaron, es un bug.
4. Las dos condiciones nuevas de `vectors_cover_every_event_and_moment` (RF9).
5. `breathers_happen_and_keep_the_contract` en `properties.rs` (RF7), con el tiempo de *debug* antes y después anotado (decisión 3 del architect).
6. `just _check-wasm`, para pasar los mismos dorados en WASI.
7. Regeneración mecánica del lado web, para que `check-web` no quede con copias viejas:
   - `for f in ko-comeback decision-a decision-b draw; do cp crates/engine/tests/golden/log-$f.json apps/web/src/lib/features/fight/fixtures/; done`
   - `corepack pnpm --filter web exec vitest run -u tests/golden-timelines.test.ts`, que reescribe las 3 timelines del renderer;
   - `corepack pnpm --filter web exec vitest run tests/fight-log-goldens.test.ts tests/golden-timelines.test.ts`, que tienen que pasar;
   - `corepack pnpm --filter @torneo/renderer exec vitest run`, para los tests del renderer que leen esas timelines.
8. Commit `feat(engine): breather pause and regenerated goldens (TA-21)`. A partir de aquí F puede trabajar sobre los dorados finales.

**B4 (B). Métricas y criterios del simulador.**
1. Se implementa `report.rs` (decisión 7) junto con sus tests y `cli.rs`, y corre `cargo test -p sim`.
2. Se corre `just sim` y se guarda el informe de texto completo. Si **cualquier** criterio de RF17 o de ritmo falla con `content/` real, me detengo y lo escalo al architect y al game-designer con las cifras, sin mover valores.
3. Si las cifras reales difieren de la calibración de RF13, se anotan en la spec (A) y en el PR.

**B5 (B). Fixture `balanced`.**
1. Se mide su media con `just sim --content crates/sim/tests/fixtures/balanced --fights 400 --check`.
2. Se baja `speed` en los dos twins por igual (primer intento: 95 → 85, que sube el intervalo 100 ms), hasta que la media quede dentro de 1100–1300 y pase todo `--check` con 400 peleas.
3. El valor elegido y su informe van a "Desviaciones". El fixture `lopsided` tiene que seguir fallando por `win_rate` (`a_lopsided_roster_fails_the_check_only_with_check`).
4. Commit `feat(sim): pacing metrics and criteria (TA-21)`.

**B6 (B). Espejos con 3 semillas raíz (RF13).** Es una medición, no un cambio de código.
1. Con los valores elegidos (9/200/800), para cada semilla de la decisión 4 del architect, se corre `just sim --json --seed <hex>` y se anota `mirrors`. El lado `a` es `(wins_a + draws/2) / fights`.
2. Para 8/150/1000 se editan **temporalmente** los tres valores en `ruleset.rs` (con B3 ya commiteado), se repiten las 3 corridas y se restaura con `git checkout -- crates/engine/src/ruleset.rs`. La restauración se comprueba con `git diff --exit-code crates/engine/src/ruleset.rs`. Nada de eso se commitea.
3. Se pegan en el PR las 6 cifras y el informe completo de `just sim` con la semilla por defecto, para que lo firme el game-designer (RF13).
4. **Si alguna sale fuera de 47–53 %, o las 3 de una configuración quedan del mismo lado de 50 %**, se trata como un bug estructural. La investigación sigue este orden:
   - (a) `breather_chance_permille = 0` en local, que tiene que reproducir el 50,3 % de "sin respiro";
   - (b) el lado `a` partido por quién ganó la iniciativa;
   - (c) los empates de `next_at` después de un respiro (`next_actor` con `last`);
   - (d) el `saturating_add` y los respiros que cruzan `time_limit_ms`.

   Los valores no se mueven hasta encontrar la causa, y el hallazgo va a "Desviaciones".

**F1 (F, después de B3; el Vitest de RF5 puede empezar después de B1). Copias y contrato.**
1. `cp` de `log-ko-clutch`, `log-ko-decisive-crit` y `log-ko-knockdown`, y `ENGINE_LOGS` con 7 llaves (decisión 9).
2. `engine-logs.test.ts` y `timeline-contract.test.ts`.
3. Corre `corepack pnpm --filter web exec vitest run`.

**F2 (F, paralelizable con B1 a B4). `retime.ts`.**
1. `parsePace`, `paceMin`, `resolvePace` (decisión 1 del architect) y `retime.test.ts`.
2. Se puede escribir contra los dorados actuales, cuya separación mínima ya es de 700 ms, así que el mínimo ya da 0.86. Se vuelve a correr después de B3.

**F3 (F, después de F2). Arena.**
1. `FightStage`, la página, `ArenaStats`, `ArenaSection` y los mensajes es/en.
2. Corren `just check-web` (Prettier, ESLint, `svelte-check`, `check-i18n` y `check-renderer-split` en e2e) y una revisión manual con `just dev` en `/es/dev/arena?log=ko-knockdown&pace=0.8` y `/en/…`, a 375 px y en escritorio.
3. Commit `feat(web): dynamic pace minimum and seven engine logs in the arena (TA-21)`.

**F4 (F). e2e.**
1. `arena.spec.ts` y `arena-webkit.spec.ts`.
2. Durante el desarrollo solo se corre el spec tocado: `corepack pnpm --filter web exec playwright test arena.spec.ts --project chromium -g "log="`, después de un `build`. El `just e2e` completo se corre **una sola vez**, en la prueba final.
3. Commit `test(web): arena e2e over the seven goldens (TA-21)`.

**D (paralelizable con F, después de B4, que da las cifras).**
- A redacta las enmiendas de RF18, cada una con su línea "Enmienda (006, TA-21)" y el enlace a la spec 006. En 003: RF4, RF7 (pasos 6–7), la fila "Respiro" de la tabla del ruleset, la lista de cotas, la nota de RF17 (criterios de ritmo y 900 ‰) y el bloque de cifras medidas. En 005: RF15 y RF17. En 002: "Datos para 003" y RF-B9. En `docs/architecture.md`, "Motor de pelea" y "Renderer de peleas".
- G agrega a `write-character` la nota de que `speed` mueve la media de ritmo, y de que un personaje nuevo tiene que pasar los criterios de ritmo de `just sim --check`. También actualiza la lista de criterios: 90 % en 45–90 s, media de 1100–1300 ms, ≥ 25 % de ráfaga, mediana de ≥ 2 pausas, ≥ 90 % de peleas con pausa y separación mínima ≥ 600.
- Commit `docs(006): amendments to 002, 003, 005 and architecture (TA-21)`.

**P (B y F). Prueba final y RF20.**
1. `just check` completo, con la salida pegada en el PR.
2. `just e2e` completo, una vez.
3. Se abre el PR. Su descripción declara que **no existe ninguna pelea de producción** (RF8), pega el informe de `just sim` y las 6 cifras de espejos (RF13), nombra el dorado de RF20 e incluye el resultado de WebKit (`finished` o `limited`, con la prueba en Safari si hace falta).
4. **RF20 con el owner:** con `just dev`, el owner mira `/es/dev/arena?log=<dorado de RF20>` y `?log=ko-knockdown`, cada uno con `pace=1` y con `pace=0` (que se lleva al mínimo), y deja su nota en `## Revisión` de la spec. "A medias" cuenta como "no", salvo que el owner diga explícitamente otra cosa.
5. Si la respuesta es "no", en el mismo PR **solo se cambian valores**: primero `breather_span_ms` 800 → 600 si la pausa se siente muerta, y si no, `breather_*` o `interval_*`. Después se repiten B3 desde el paso 2 (semillas, regeneración y copias), F1 (las 3 copias nuevas), B4, B6 y la prueba final, y se vuelve a pedir la nota al owner.

## Riesgos
- **Las cifras reales no reproducen la calibración del prototipo** (media 1226, 989 ‰ con pausa, espejos 49,3 %). → B4 se detiene ante cualquier criterio que falle, sin mover valores, y lo escala. Si todo pasa con cifras distintas, se registran en la spec y en el PR.
- **Espejos frágiles (±3,5σ con 3332 espejos).** → B6 con 3 semillas y las dos configuraciones, y el protocolo de bug estructural de RF13. La banda no se toca.
- **Fixture `balanced` con 400 peleas**, donde los espejos tienen σ ≈ 2,5 %. Un cambio de `speed` puede hacerlo fallar por el sesgo y no por la media. → Es determinista, así que se ve enseguida. Se prueba primero con 85, y lo que se elija se registra. Es un fixture de test, no contenido, así que ajustarlo no viola "no mover valores hasta que pase" del motor.
- **El roller guionado confunde el paso 6 con la elección.** → Se distinguen por la cota, con un `assert_ne!(total, 1000)` en la elección y `breather_asks` para comprobarlo (decisión 4).
- **Pasos 6–7 en un orden distinto del de RF3**, que es un cambio silencioso de determinismo. → `rolls_follow_the_rf7_order_in_a_real_fight` con el contador replicado (decisión 5), más los dorados en nativo y WASI.
- **Desbordamiento cerca del tope.** → `saturating_add` en los dos relojes, la cota de RF6 con todos los sumandos y la propiedad sobre los extremos (`valid_extremes_respect_rf2`).
- **Tests web y e2e atados a cifras de un dorado viejo** (`max_hp` 700/860, ganador `b`, knockdown en `ko-comeback`). → Decisión 9: se leen del JSON importado.
- **Tests del renderer sobre las timelines regeneradas** (`player.test.ts` cuenta frames desde el primer golpe). → Sus cotas dependen del reloj, no del contenido. Igual se corren en B3 (paso 7) y en la prueba final.
- **`check-renderer-split` o ESLint, si `retime.ts` importara valores del renderer.** → Decisión 1 del architect: el validador se inyecta.
- **El PR llega tarde y TA-13 o el scheduler se fusionan antes**, y la reescritura de v1 deja de valer. → B0 (bloqueo en Linear) y la comprobación de RF8 en el PR. Si pasa, se para y se replantea como `ruleset_version` 2.
- **La máquina cargada hace lento el ciclo e2e.** → Specs focalizados con `-g` durante F4, y un solo `just e2e` completo.

## Alternativas descartadas
- **Importar `validateTimeline` en `retime.ts`.** Lo prohíben la regla `rendererRestriction` de ESLint y `check-renderer-split` (decisión 1 del architect).
- **Calcular el mínimo en `FightStage`.** Mete lógica de una herramienta de `/dev/**` en un componente del catálogo.
- **Distinguir el paso 6 replicando en el roller guionado el estado del motor** (KO y knockdown). Duplica la simulación en el test. La cota más la anotación de `breather_asks` es más simple, y sigue siendo comprobable.
- **Un solo commit para contrato, campos y respiro.** Pierde la prueba de que el contrato y los campos sin uso no cambian bytes (decisiones 1 y 2).
- **Medir 8/150/1000 con un `Ruleset` paramétrico expuesto a `sim`.** Abriría una API de construcción de rulesets fuera de `for_version`, contra RF4 de 003. Basta con la edición local que no se commitea (B6).
- **`proptest`.** La spec la descarta: es una dependencia nueva y 003 fijó semillas derivadas.

## Pruebas (cómo se demuestra que funciona)
- [ ] `just check` en verde, con la salida pegada en el PR. Incluye `_check-engine`, `_check-wasm` (dorados en WASI) y la frescura de `just gen` sin diferencias (RF19).
- [ ] `just e2e` en verde (Chromium y WebKit), una vez, con el resultado de WebKit (`finished` o `limited`) en el PR.
- [ ] B1: `timeline-contract.json` nuevo sin cambios en `log-*.json` ni en el digest (RF5, preocupación 1).
- [ ] `v1_respects_the_bounds` cubre RF6: el contrato, `strike_gap_min() >= 600`, las cotas de `breather_*`, el desbordamiento y los 418 eventos.
- [ ] Unitarios de `sim.rs` (RF2, RF3):
  - el roller comprueba `roll(1000)` y `roll(801)`;
  - con el paso 6 forzado a 0, la resolución siguiente llega a ≥ `t_res + 2500` y la secuencia de atacantes coincide con la del mismo guion sin respiro;
  - con un sorteo ≥ `chance` no hay desplazamiento;
  - ni el knockdown ni el KO consumen los pasos 6–7, y el contador se reinicia tras un knockdown y tras un respiro;
  - antes de 9 resoluciones no se sortea nada.
- [ ] `rolls_follow_the_rf7_order_in_a_real_fight` con los pasos 6–7 (RF3).
- [ ] `properties.rs`: los 10 000 logs y los extremos con `assert_invariants` sin literales, `breathers_happen_and_keep_the_contract` con separación mínima ≥ `strike_gap_min()` y no vacuidad de ≥ 90 % (RF4, RF7), y el tiempo de *debug* anotado.
- [ ] `golden.rs` en nativo y WASI: 7 logs, digest, `timeline-contract.json` y cobertura. Al menos un KO tiene ≥ 2 pausas y `log-ko-knockdown` conserva su `knockdown` (RF9).
- [ ] `crates/sim`:
  - `report/tests.rs`, con los bordes de RF11 y cada `Failure` nuevo;
  - los 3 tests de atadura (RF12);
  - `cli.rs`, con las llaves de `--json` y `balanced` pasando;
  - `balance.rs` con `Criteria::RF17` completo (RF12, RF13).
- [ ] Informe completo de `just sim` y las 6 mediciones de espejos en el PR, firmados por el game-designer (RF13).
- [ ] Vitest:
  - `timeline-contract.test.ts` (RF5);
  - `fight-log-goldens.test.ts`, con 7 copias byte a byte y el conjunto exacto (RF10);
  - `golden-timelines.test.ts` con las timelines frescas (RF10);
  - `retime.test.ts` (RF14 a RF16):
    - `parsePace` solo valida la sintaxis;
    - el mínimo es 0.86 en los 7 dorados;
    - cada centésima de `[mínimo, 1.5]` pasa con los manifests de los placeholders, también en el guion;
    - si el mínimo es mayor que 0.5, `mínimo − 0.01` falla solo con `timeline.strike_gap` o `timeline.strike_during_knockdown`;
    - clamp por arriba y por abajo, y `clamped: false` dentro del rango;
    - una `Timeline` inválida con factor 1 da `null` y `effective: 1` sin lanzar;
  - `meanStrikeGapMs` sin cambios.
- [ ] e2e Chromium, para los 7 `?log=` en `es`: (a) `t=99999999999` llega a `finished` con su `data-ending`; (b) `pace=0` llega a `finished` y la fila de ritmo muestra el mínimo con `data-clamped="true"`; (c) `pace=1.5` llega a `finished`. Además, copiar JSON con `?log=ko-knockdown&pace=0.8` da `0.8`/`0.86`/`0.86`, no hay errores de consola y el test del knockdown corre sobre `ko-knockdown` (RF16, RF17).
- [ ] e2e WebKit: los 7 logs con `pace=0` llegan a `finished` o quedan en `limited`.
- [ ] Capturas de `ArenaStats` con la fila de ritmo y la nota de clamp, a 375 px y en escritorio, en `es` y en `en` (RF17, ADR-0007).
- [ ] RF20: la nota del owner en `## Revisión` de la spec antes del merge.
- [ ] Diff del PR sin `crates/protocol`, `packages/protocol-ts`, `packages/renderer/src` ni `content/` (RF19).

## Estimación de CI
Los workflows no cambian. El job `rust` suma unos segundos: el test de propiedad nuevo y las métricas de `sim`, compensados en parte porque las peleas tienen menos acciones. El job `e2e` suma unos 21 casos en Chromium y 6 en WebKit, alrededor de 1 a 2 minutos por corrida.

## Desviaciones durante la implementación
- **B3, dorados (decisión 6).** Semillas elegidas: `ko-decisive-crit` trickster/tank 1 (3 pausas), `ko-knockdown` striker/trickster 0 (2), `ko-comeback` trickster/trickster 3 (3), `ko-clutch` striker/tank 1 (5), `decision-a` tank/turtle 0 (5), `decision-b` tank/tank 0 (5; `turtle/tank 0` es el espejo de `decision-a`) y `draw` turtle/turtle 118 (4). En `ko-clutch`, entre los candidatos de la semilla más baja (1) se desempató por más pausas. **Dorado de RF20: `log-ko-clutch`** (5 pausas).
- **B3, tiempo de *debug* de `--test properties` (decisión 3 del architect).** 11,05 s antes y entre 10,64 y 11,07 s después, con `breathers_happen_and_keep_the_contract` como test propio sobre los 10 000 logs. No crece más de un 20 %, así que no se movió al bucle existente.
- **B3, helper compartido.** `resolution_times` vive en `tests/common/mod.rs` y lo usan `properties.rs` y el conteo de pausas de `golden.rs`.
- **B5, fixture `balanced`.** `speed` 95 → 85 en los dos twins (primer intento del plan). Media de 1098 a 1114 ms. Con 400 peleas pasa todo `--check`: espejos 50,0 %, 99,0 % en 45–90 s, ráfaga 44,2 %, pausas p50 3 y 99,0 % con pausa. `lopsided` sigue fallando solo por `win_rate`.
- **B6, espejos (RF13), escalado.** Con 9/200/800, el lado `a` da 49,40 / 50,26 / 50,14 % con `DEFAULT_SEED`, `00…01` y `a5`×32. Con 8/150/1000 da **46,94** / 49,76 / 49,23 %: uno fuera de 47–53 y los tres por debajo de 50. Es el caso de bug estructural del paso 4. Me detuve sin mover valores ni investigar, y lo escalé al orquestador con las cifras. `ruleset.rs` se restauró (`git diff --exit-code`).

## Revisión
### architect — 2026-10-27 — APPROVE
- Verifiqué: cada RF de la spec tiene archivo, paso y prueba (RF1–RF20); el orden B1 → B2 → B3 demuestra que el contrato y los campos sin uso no cambian bytes (ADR-0001, ADR-0002: el respiro solo consume `Rolls::roll`, en los pasos 6–7 de RF3, comprobados por el roller guionado y `rolls_follow_the_rf7_order_in_a_real_fight` con el contador replicado, en nativo y WASI); sin cambios en `crates/protocol`, `packages/protocol-ts`, `packages/renderer/src` ni `Cargo.lock` (RF19); i18n con dos mensajes es/en y sin literales (ADR-0007); nada toca monedas (ADR-0005). Confirmé en el código la restricción que motiva la decisión 1: `rendererRestriction` en `apps/web/eslint.config.js` (solo `FightStage.svelte` importa valores del renderer), `scripts/check-renderer-split.mjs` (ningún import estático de `packages/renderer/` desde `dev/arena`), `FightStage` ya carga `import('@torneo/renderer')`, cuya raíz reexporta `validateTimeline(json, manifests?)`, y la prop `retime` tiene un único llamador (`routes/dev/arena/+page.svelte`). Lo puede implementar alguien que no vio la conversación.
- Decisiones:
  1. **Validador inyectado: aceptado.** `paceMin(timeline, isValid): number | null`, `resolvePace(requested, timeline, isValid): PaceResolution` y la prop `retime: (timeline, isValid) => Timeline`, con `FightStage` pasando `(t) => rendererModule.validateTimeline(t).ok` (sin manifests). La regla sigue con una sola fuente (`TIMELINE_LIMITS` y el validador) y `retime.ts` queda con `import type`. Es una enmienda técnica de la spec (RF15, RF16), registrada en su `## Revisión`; no va a "Desviaciones". El Vitest de `retime.test.ts` usa siempre el validador real, nunca un doble.
  2. **Denominador de RF7: aceptado.** Peleas con **más de** `breather_after_resolutions` resoluciones (≥ 10 en v1), que es la lectura correcta de "no terminan antes": hace falta una resolución posterior para observar el hueco. El umbral (90 %) y el denominador no se mueven; si con `tests/fixtures/` sale por debajo, se escala con la cifra.
  3. **Coste del test de propiedad: aceptado, con una condición.** Re-simular los 10 000 pares una vez; si el tiempo de *debug* de `--test properties` crece > 20 %, se pasa el cálculo al bucle existente con un helper. En ese caso la separación mínima ≥ `strike_gap_min()` y la no vacuidad ≥ 90 % se siguen exigiendo **sobre los 10 000 logs** (en el test existente), y el test con el nombre de RF7 que cubre solo los extremos no puede ser el único que lo afirme. Se anota en "Desviaciones" con los dos tiempos.
  4. **Semillas raíz de RF13: aceptadas.** `DEFAULT_SEED` (`746f726e656f2d6162737572646f2d73696d2d64656661756c742d7631212121`), `00…01` (63 ceros y un `1`) y `a5` × 32, las tres de 64 dígitos hex, iguales para 9/200/800 y 8/150/1000. Leer "todas del mismo lado de 50" por configuración es más estricto que la spec, y vale.
- Aclaración (no bloquea): en el paso D, "A" redacta las enmiendas de RF18 durante la implementación, en el mismo PR, después de B4; el architect no las escribe antes. Las reviso en el PR junto con el cumplimiento de RF18.

