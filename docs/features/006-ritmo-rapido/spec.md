---
id: "006"
title: Ritmo rápido del motor
type: spec
status: done
risk: high
author: architect
reviewers: [product-owner, security-reviewer, game-designer]
approvals: [{by: product-owner, date: 2026-10-27}, {by: game-designer, date: 2026-10-27}, {by: security-reviewer, date: 2026-10-27}, {by: owner, date: 2026-10-27}]
linear: TA-21
depends_on: ["002", "003", "005"]
created: 2026-10-27
updated: 2026-10-28
---

# Spec: ritmo rápido del motor

> Intent: [intent.md](./intent.md)

## Resumen
El motor gana un **respiro** (*breather*): después de una racha de intercambios, un sorteo del PRNG puede detener los dos relojes de la pelea entre 1.8 y 2.6 s, y eso deja un hueco de ≥ 2.5 s sin impactos. No es un evento nuevo, solo ausencia de impactos. Con cuatro campos nuevos del ruleset v1 (reescrito, sin bump de versión), la separación media pasa de 1096 a ~1226 ms, con ~3 pausas en la pelea mediana y ~28 % de intervalos en ráfaga. `crates/sim --check` mide y exige esas bandas. En `/[lang]/dev/arena`, `?pace` se acota con un mínimo dinámico calculado sobre la `Timeline` ya adaptada, y un valor fuera de rango se lleva al límite válido más cercano. `crates/protocol`, `packages/protocol-ts`, el formato de la `Timeline` y `validateTimeline` **no cambian**.

## Una fuente para cada cifra
Responde al primer pendiente de la revisión del intent. Cada cifra vive en un solo lugar, y las demás la leen o la comprueban con un test que falla si se separan.

| Cifra | Qué es | Fuente única | Quién la comprueba |
|---|---|---|---|
| `breather_*` (9 resoluciones, 200 ‰, 1800 ms, 800 ms) | **Entradas** del ritmo | `Ruleset::v1()` (`crates/engine/src/ruleset.rs`) | test de cotas del ruleset (RF6) y `sim` (RF12) |
| `interval_*`, `jitter_ms`, `windup_*`, `recovery_ms`, `knockdown_ms` | Entradas que ya existían (no cambian) | `Ruleset::v1()` | test de cotas |
| 600 ms entre impactos, 2000 ms de intro, 800–3000 ms de knockdown, 1500 ms de KO a victoria, 2000 ms de pose | **Reglas de la `Timeline`** (002/005) | `TIMELINE_LIMITS` de `packages/renderer/src/timeline/types.ts` | espejo en Rust `engine::TIMELINE_CONTRACT`, atado por un dorado y un Vitest (RF5) |
| 900 ms (umbral de ráfaga) y 2500 ms (umbral de pausa) | **Definiciones** de medida, del go/no-go de 002 ("Datos para 003") | constantes `BURST_GAP_MAX_MS` y `PAUSE_GAP_MIN_MS` de `crates/sim/src/report.rs`, junto a `DURATION_BAND_MS` | test de `sim` contra `Ruleset::breather_gap_min()` (RF12) |
| 1100–1300 ms de media, ≥ 250 ‰ de ráfaga, ≥ 2 pausas en la pelea mediana, ≥ 900 ‰ de peleas con pausa | **Bandas de salida** | `sim::Criteria::RF17` (los campos nuevos) | `just sim --check` y `crates/sim/tests/balance.rs` |
| 45–90 s, ≤ 10 % de `time_up`, < 1 % de empates, win rates, momentos | Criterios que ya existían (RF17 de 003) | `DURATION_BAND_MS` y `Criteria::RF17` | lo mismo |
| `PACE_FLOOR` 0.5, `PACE_MAX` 1.5, paso 0.01 | Red de seguridad de `?pace` | `apps/web/src/lib/features/arena/retime.ts` | Vitest (RF16) |

Ninguna banda de salida es un campo del ruleset, y ninguna regla de la `Timeline` se relaja ni se mueve al motor.

## Requisitos funcionales

### Motor (`crates/engine`)
- **RF1. Campos nuevos del ruleset v1.** `Ruleset` suma cuatro campos con su accessor `const fn` (la macro `ruleset!`) y dos derivados:

  | Campo | Valor v1 | Significado |
  |---|---|---|
  | `breather_after_resolutions: u32` | 9 | Cuántas resoluciones sin KO ni knockdown tienen que pasar, desde el inicio o desde el último respiro o knockdown, antes de que se pueda sortear un respiro |
  | `breather_chance_permille: u32` | 200 | Probabilidad (‰) de que empiece un respiro después de cada resolución elegible |
  | `breather_min_ms: u32` | 1800 | Duración mínima del respiro |
  | `breather_span_ms: u32` | 800 | Duración: `breather_min_ms + roll(breather_span_ms + 1)`, de 1800 a 2600 ms |

  Derivados (`pub const fn`, sin campos nuevos):
  - `strike_gap_min() = min(recovery_ms, interval_min()) + windup_strike_ms`, que en v1 da 300 + 400 = **700 ms**. Es la separación mínima estructural entre dos resoluciones consecutivas: si actúa el defensor, ≥ `recovery_ms` + windup; si repite el atacante, ≥ `interval_min` + windup.
  - `breather_gap_min() = breather_min_ms + strike_gap_min()`, que en v1 da 1800 + 700 = **2500 ms**. Es la separación mínima garantizada entre la resolución que abre un respiro y la siguiente.

  El resto de los campos de v1 no cambia (`interval_base_ms` 1800, `jitter_ms` 200, windups 400/700/1100, `recovery_ms` 300, `knockdown_ms` 1500, `time_limit_ms` 86 000, `end_ms` 4000…). → test de cotas (RF6) y test `only_version_one_exists`.

- **RF2. El respiro.** `Fight` lleva un contador `since_breather: u32` (empieza en 0). Al final de cada acción que **no** termina en KO, después de los momentos:
  1. si la acción produjo un `knockdown`: `since_breather = 0` y no se sortea nada (la caída ya es un tiempo muerto propio, y así no se apilan 1.5 s de caída con 2 s de respiro);
  2. si no: `since_breather += 1`, y si `since_breather >= breather_after_resolutions`, se sortea el paso 6 de RF3. Si sale respiro, se sortea su duración `d` (paso 7) y se hace `a.next_at = a.next_at.saturating_add(d)`, lo mismo para `b.next_at`, y `since_breather = 0`.

  El respiro **desplaza los dos relojes por igual**, así que no cambia quién actúa después, ni el desempate de RF8, ni la recuperación, ni el knockdown. No emite ningún evento y no cambia el `FightLog`, salvo los `t` posteriores. Un respiro que empuja los relojes más allá del tope termina la pelea por `time_up`, con las reglas de RF8 de 003 sin cambios. → tests unitarios de `sim.rs` (Estrategia de pruebas).

- **RF3. Orden de consumo del PRNG (enmienda de RF7 de 003).** Al empezar, `roll(2)` decide la iniciativa. En cada acción, en este orden:
  1. `roll(peso_total)`: elección.
  2. `roll(1000)`: defensa.
  3. Solo en golpe o bloqueo, `roll(31)`: varianza.
  4. Solo en golpe, `roll(1000)`: crítico.
  5. `roll(jitter_ms + 1)` = `roll(201)`: jitter del atacante (siempre).
  6. **Nuevo.** Solo si la acción no terminó en KO, no produjo knockdown y `since_breather >= breather_after_resolutions` (contando ya esta resolución): `roll(1000)`. Hay respiro si el resultado es `< breather_chance_permille`.
  7. **Nuevo.** Solo si hubo respiro en el paso 6: `roll(breather_span_ms + 1)` = `roll(801)`: duración.

  Los pasos 6 y 7 se consumen **después** de aplicar el daño, el knockdown, `low_hp`, los momentos y el KO, porque dependen de que no haya KO ni knockdown. No se consume nada más. → `rolls_follow_the_rf7_order_in_a_real_fight` y el roller guionado de `sim.rs` (la máquina de estados que comprueba cada cota) suman los pasos 6 y 7.

- **RF4. Garantías de ritmo.** Para todo log que produce el motor con un ruleset que pasa el test de cotas:
  - (a) dos resoluciones consecutivas están separadas por ≥ `strike_gap_min()` ≥ `TIMELINE_CONTRACT.strike_gap_ms` (600);
  - (b) la resolución siguiente a un respiro llega a ≥ `t_res + breather_gap_min()`;
  - (c) no empieza ningún respiro en una acción con KO o knockdown;
  - (d) un respiro nunca adelanta un reloj;
  - (e) siguen valiendo todas las invariantes de RF2 de 003.

  (a) es estructural y ya valía (el respiro solo suma tiempo). (b) sale de que, después del desplazamiento, el próximo ataque empieza a ≥ `t_res + d + min(recovery_ms, interval_min)` y resuelve un windup después. → propiedad (RF7) y unitarios.

- **RF5. Contrato de la `Timeline` del lado del motor.** `crates/engine/src/contract.rs` exporta `pub const TIMELINE_CONTRACT: TimelineContract`, con `intro_gap_ms: 2000`, `strike_gap_ms: 600`, `knockdown_min_ms: 800`, `knockdown_max_ms: 3000`, `win_gap_ms: 1500` y `win_pose_ms: 2000` (`u32`, `Serialize`).
  - Es un **espejo** de `TIMELINE_LIMITS` del renderer, cuyo dueño son 002 y 005. **No** es un campo de `Ruleset` ni una regla del juego: la simulación (`sim.rs`) nunca lo lee y no cambia ningún byte del log.
  - Lo usan el test de cotas del ruleset (RF6), `tests/common/mod.rs::assert_invariants`, que reemplaza sus literales `600`, `2000`, `800..=3000` y `3500` (`3500` = `win_gap_ms + win_pose_ms`), y `crates/sim` (RF12).
  - **Atadura:** `golden.rs` suma el vector (d), que escribe `canonical` de `TIMELINE_CONTRACT` en `tests/golden/timeline-contract.json` (se regenera con `UPDATE_GOLDEN=1`, como los demás). Un Vitest nuevo, `apps/web/tests/timeline-contract.test.ts`, lee ese archivo con `support/golden-logs.ts` (`GOLDEN_DIR`) y exige que cada llave sea igual a su par de `TIMELINE_LIMITS` (`intro_gap_ms` ↔ `introGapMs`, etc.) y que el conjunto de llaves sea exacto. Si alguien cambia una regla en un lado sin el otro, falla `just check`.

  → `golden.rs` (d) y el Vitest.

- **RF6. Test de cotas (`v1_respects_the_bounds`).** Reemplaza los literales de ritmo por `TIMELINE_CONTRACT` y suma:
  - `end_ms >= win_gap_ms + win_pose_ms`, `intro_ms + windup_strike_ms >= intro_gap_ms` y `knockdown_ms` en `[knockdown_min_ms, knockdown_max_ms]`;
  - `strike_gap_min() >= strike_gap_ms` (reemplaza las dos aserciones de `recovery` e `interval_min`);
  - `breather_chance_permille <= 1000`, `breather_after_resolutions >= 1` y `breather_span_ms + 1` es una cota válida;
  - el desbordamiento: `time_limit_ms + interval_base_ms + jitter_ms + breather_min_ms + breather_span_ms + knockdown_ms + recovery_ms` cabe en `u32`;
  - `max_events_bound()` sigue en 418: el respiro solo resta acciones, así que la cota analítica sigue siendo una cota superior.

  El umbral de pausa (2500) **no** se comprueba aquí, porque es una definición de `sim` (RF12). → `cargo test -p engine`.

- **RF7. Propiedad de ≥ 600 ms sobre muchas semillas.** Mismo estilo que `properties.rs`: semillas derivadas, sin crates nuevos y sin `proptest` (ver Alternativas).
  - `every_log_is_deterministic_and_respects_rf2` (10 pares × 1000 semillas, dos veces) y `valid_extremes_respect_rf2` siguen como están, y heredan de `assert_invariants` la separación ≥ `TIMELINE_CONTRACT.strike_gap_ms` y el resto del contrato.
  - Test nuevo, `breathers_happen_and_keep_the_contract`. Sobre los mismos 10 000 logs exige que la separación mínima observada sea ≥ `strike_gap_min()` (y por lo tanto ≥ 600), y además pide **no vacuidad**: al menos el 90 % de las peleas que no terminan antes de `breather_after_resolutions` resoluciones tienen alguna separación ≥ `breather_gap_min()`. Así, un respiro roto (que nunca dispara) no pasa en silencio.

  → `cargo test -p engine --test properties`, dentro del tiempo de *debug* actual (las peleas tienen menos acciones, no más).

- **RF8. `ruleset_version` sigue en 1 y se reescribe.** Lo permite RF10 de 003 porque no hay ninguna pelea de producción guardada: `apps/server` no depende de `engine` (su `Cargo.toml` solo tiene `protocol`), no hay scheduler ni tabla de peleas, y `content/characters/*/character.json` sigue con `ruleset_version: 1`. El comentario de cabecera de `ruleset.rs` no cambia. El PR declara en su descripción que no existe ninguna pelea de producción. → revisión del PR (architect).

### Dorados
- **RF9. Regeneración.** Con `UPDATE_GOLDEN=1 cargo test -p engine --test golden`, en nativo:
  - se regeneran los 7 `log-*.json` y `digest-1000.txt`, y se crea `timeline-contract.json` (RF5);
  - los `spec-*.json` **no** cambian, porque los fixtures no cambian. Si cambian, es un error;
  - las semillas de `VECTORS` se vuelven a elegir con `search_golden_seeds`, con las reglas de siempre: pares variados, ningún vector es otro con los lados cambiados y el empate es un espejo de `turtle`. La búsqueda de calibración encontró candidatos para los 7 objetivos en semillas de 0 a 405 (el empate, en `turtle` contra `turtle`, semillas 118, 132, 145…);
  - `vectors_cover_every_event_and_moment` suma dos condiciones: entre los vectores que terminan en KO, al menos uno tiene ≥ 2 separaciones ≥ `breather_gap_min()` (para que el owner vea los dos ritmos, RF20), y `log-ko-knockdown` sigue teniendo su `knockdown`.

  → `cargo test -p engine --test golden`, en nativo y en WASI (`_check-wasm`).
- **RF10. Copias y derivados en `apps/web` y el renderer.**
  - Las copias de la arena pasan de 4 a **los 7 dorados**: se agregan `log-ko-clutch`, `log-ko-decisive-crit` y `log-ko-knockdown`, con `cp` byte a byte a `apps/web/src/lib/features/fight/fixtures/`. `ENGINE_LOGS` suma sus 3 llaves, cada una en su chunk diferido. Así el criterio del intent ("cada dorado se reproduce…") se puede probar en la arena (enmienda de RF15 de 005). Los tests (a) y (b) de esa RF siguen igual y cubren las 7.
  - Se regeneran `packages/renderer/test/fixtures/golden/{decision-a,decision-b,draw}.timeline.json` con `vitest -u` (`golden-timelines.test.ts`).
  - El guion del spike (`spike.timeline.json`) **no** cambia.

  → Vitest (`fight-log-goldens.test.ts`, `golden-timelines.test.ts`).

### Simulador (`crates/sim`)
- **RF11. Métricas de ritmo.** `Density` suma, sobre los cruces:
  - `min_gap_ms`;
  - `burst_gaps`, las separaciones ≤ `BURST_GAP_MAX_MS` (900);
  - `pause_gaps`, las separaciones ≥ `PAUSE_GAP_MIN_MS` (2500);
  - `pauses_p10`, `pauses_p50` y `pauses_p90`: pausas por pelea, por rango más cercano, como `resolutions_p*`;
  - `fights_with_pause`, las peleas con ≥ 1 pausa.

  `mean_gap_ms` no cambia: es la media agregada (`gap_total_ms / gaps`) sobre **todas** las separaciones, pausas incluidas. Una pausa se cuenta por su separación, sea cual sea su causa: un knockdown cuya separación llegue a 2500 también cuenta. El informe de texto suma una línea `pacing: …` en inglés, y `--json` incluye los campos. Las dos constantes nuevas van junto a `DURATION_BAND_MS`, con la misma regla: vienen de 002 y del owner, y el game-designer no las ajusta. → unitarios de `report/tests.rs` con logs construidos (900 cuenta como ráfaga, 901 no; 2500 cuenta como pausa, 2499 no) y `cli.rs` (las llaves de `--json`).
- **RF12. Criterios nuevos de `--check`** (campos de `Criteria::RF17`, comparados con productos cruzados como los demás y sin redondear):

  | Campo | Banda | Medido (calibración) | Failure |
  |---|---|---|---|
  | `mean_gap_min_ms` / `mean_gap_max_ms` | 1100 ≤ media ≤ 1300 ms | 1226 ms | `mean_gap { ms }` |
  | `burst_min_permille` | `burst_gaps` ≥ 250 ‰ de `gaps` | 285 ‰ | `burst_share { permille }` |
  | `pauses_p50_min` | `pauses_p50` ≥ 2 | 3 | `pauses_median { count }` |
  | `paused_fights_min_permille` | `fights_with_pause` ≥ 900 ‰ | 989 ‰ | `paused_fights { permille }` |
  | (sin campo) | `min_gap_ms` ≥ `engine::TIMELINE_CONTRACT.strike_gap_ms` | 700 ms | `strike_gap { ms }` |
  | `in_band_min_permille` (ya existía) | **sube de 800 a 900 ‰**, como pide el intent | 986 ‰ | `duration_in_band` |

  El game-designer puede ajustar `burst_min_permille`, `pauses_p50_min` y `paused_fights_min_permille` (y registrarlo aquí). La banda de la media viene de la cifra del owner (~1.2 s) y cambiarla necesita el OK del product-owner, igual que el tope de `time_up`.

  Tests que atan las cifras, en `crates/sim` (que depende de `engine`, y no al revés):
  - `Ruleset::v1().breather_gap_min() >= PAUSE_GAP_MIN_MS`: cada respiro cuenta como pausa;
  - `strike_gap_min() <= BURST_GAP_MAX_MS`: una ráfaga es alcanzable;
  - `strike_gap_min() >= TIMELINE_CONTRACT.strike_gap_ms`.

  `the_criteria_are_the_ones_of_rf17` se actualiza con los valores nuevos. → `just sim --check`, `balance.rs` y `report/tests.rs` (cada failure nuevo con su log construido).
- **RF13. RF17 entero se vuelve a medir**, no solo el ritmo (segundo pendiente de la revisión del intent). El PR pega el informe completo de `just sim` y el game-designer lo firma. Con los valores de RF1, la calibración (motor prototipo fuera del repo, `content/` real, semilla por defecto):
  - win rates de 48.9 a 51.1 % (cruces de 39.3 a 60.6 %, solo aviso);
  - momentos en el 38.1 % (comeback 13.2, decisive_crit 11.5 y clutch_ko 23.1 %);
  - `end.t` p10/p50/p90 de 50.6/59.4/70.6 s, con el 98.6 % en 45–90 s;
  - KO en el 99.8 %, `time_up` en el 0.1 % y empates en el 0.0 %;
  - espejos con el 49.3 % para el lado `a`.

  Con otras dos semillas raíz, la media sale en 1226 ms y la mediana de pausas en 3, así que es estable.

  > **Medido con el motor real (B4, 2026-10-27).** `just sim` reproduce la calibración cifra por cifra: las de arriba, más 1226 ms de media, 700 ms de mínima, 28,5 % en ráfaga, pausas p10/p50/p90 de 1/3/4 y el 98,9 % de peleas con pausa. El lado `a` de los espejos da 49,40 % (1646/1686/0, que el informe muestra como 49,3 %). Con las otras dos semillas raíz, 50,26 y 50,14 %. Con 8/150/1000 sale 46,94 / 49,76 / 49,23 %, uno fuera de 47–53 %. La investigación de "Desviaciones" del plan concluye que es ruido de muestreo y no un bug: con 333 332 espejos, las dos configuraciones quedan entre 49,98 y 50,10 %. No se cambian código ni valores. Fixture `balanced`: `speed` 95 → 85 en los dos twins, con una media de 1114 ms.

  **Espejos:** el PR pega el % del lado `a` en espejos con los valores elegidos (9/200/800) y con 8/150/1000, cada uno con ≥ 3 semillas raíz. Si alguna medición sale fuera de 47–53 %, o todas quedan del mismo lado de 50 %, se trata como un **bug estructural** que hay que investigar (desempate de RF8, `saturating_add` cerca del tope), no como ruido. Nunca se mueven valores "hasta que pase": los valores no cambian hasta que la causa esté resuelta. **Contenido:** `content/characters/` no se toca. El fixture `crates/sim/tests/fixtures/balanced` mide 1098 ms de media (2 ms por debajo de la banda), así que el plan ajusta sus stats (es un fixture de test, no contenido) para que `a_balanced_roster_passes_the_check` siga probando un roster que pasa.

### Arena de prueba (`apps/web`, `/[lang]/dev/arena`)
- **RF14. Lectura de `?pace`.** `parsePace(raw)` solo valida la sintaxis (`^[0-9](\.[0-9]{1,2})?$`, de 0 a 9.99) y devuelve el número o `null`. Lo malformado o ausente se ignora como hoy (sin retiempo y sin mensaje, igual que `?t` y `?log`). El rango ya no se decide aquí.
- **RF15. Mínimo dinámico.** `paceMin(timeline): number | null`, puro, en `retime.ts`, sobre la `Timeline` que entrega `source` (es decir, **ya adaptada** desde el `FightLog`, o el guion del spike). Recorre `k` de 100 hacia abajo, en enteros, hasta `PACE_FLOOR·100` = 50, y para en el primer `k` con el que `validateTimeline(retimeTimeline(timeline, k / 100))` (sin manifests: el retiempo no toca nada que dependa de ellos) no da `ok`. El mínimo es el último `k / 100` válido.
  - **Si la `Timeline` ya es inválida con factor 1** (no hay ningún `k` válido), `paceMin` devuelve `null` sin lanzar excepción. `resolvePace` con `min: null` no retiempla: devuelve `{ requested, min: null, effective: 1, clamped: requested !== 1 }`, y la página muestra el error de validación que ya daba sin `pace` (RF16).
  - Usar el validador del renderer, y no recalcular `600 / separación mínima`, hace que la regla tenga una sola fuente (`TIMELINE_LIMITS`). Cubre también la regla de no golpear durante un knockdown y el redondeo de `retimeTimeline`.
  - Recorrer de arriba abajo da un mínimo con el que **todo** factor de `[mínimo, 1]` es válido, aunque el redondeo no fuera monótono.
  - Son como mucho 51 validaciones por carga, solo en `/dev/**`.
  - Con los dorados nuevos, el mínimo esperado es **0.86** (600/700 = 0.857; con 0.86, 700 ms pasan a 602), y la media queda en ~1050 ms.
- **RF16. Clamp, no rechazo.** `resolvePace(requested, timeline)` devuelve `{ requested, min, effective, clamped }`, con `effective = min(max(requested, min), PACE_MAX)` y `clamped = effective !== requested`. La página aplica `retimeTimeline(timeline, effective)` dentro del callback `retime` que ya recibe `FightStage`. Así, **ningún `?pace` termina en `timeline_invalid`**. Si la `Timeline` ya era inválida con factor 1, el error sigue siendo el de antes y no lo produce `pace`.
  - Factores mayores que 1: el Vitest de RF17 demuestra que cada centésima de `[1, PACE_MAX]` es válida en los 7 dorados y en el guion.
- **RF17. Informe.**
  - Con `?pace` presente, el JSON de "copiar estadísticas" suma `pace_requested`, `pace_effective` y `pace_min`, junto a `mean_strike_gap_ms`, que ya existía.
  - `ArenaStats` muestra una fila con la etiqueta `m.arena_stats_pace()` y el factor efectivo (`formatDecimal`, 2 decimales). Si hubo clamp, debajo va `m.arena_stats_pace_clamped({ requested })` y la fila lleva `data-clamped="true"`.
  - Los mensajes nuevos se agregan en `messages/es.json` y `messages/en.json`: "Ritmo (factor)" / "Pace (factor)" y "ajustado desde {requested}" / "clamped from {requested}". No hay ningún literal visible (ADR-0007).

  → `check-i18n`, `svelte-check` y e2e.

### Documentación (mismo PR)
- **RF18. Enmiendas.** Cada una con una línea "Enmienda (006, TA-21)" que enlaza esta spec:
  - **003:** RF4 aclara que los contratos del consumidor (`engine::TIMELINE_CONTRACT`) y las definiciones de medida de `sim` (`BURST_GAP_MAX_MS`, `PAUSE_GAP_MIN_MS`, `DURATION_BAND_MS`) **no** son constantes de juego, así que no violan "ninguna constante de juego vive fuera de `Ruleset`"; RF7 con los pasos 6–7; la tabla del ruleset con la fila "Respiro"; la lista de cotas; la nota de RF17 que remite a los criterios de ritmo y a 900 ‰; el bloque de cifras medidas.
  - **005:** RF15 (7 copias) y RF17 (los tests fijos de 0.86/0.8 pasan a ser los de RF16 y RF17 de esta spec).
  - **002:** en "Datos para 003", la media es un criterio de `sim` y sus entradas son los `breather_*` y `interval_*` del ruleset; en RF-B9, "de 0.8 a 1.5" pasa a ser "del mínimo dinámico a 1.5".
  - **`docs/architecture.md`:** en "Motor de pelea", el respiro y `TIMELINE_CONTRACT`; en "Renderer de peleas", la lista de los 7 `?log=`.

  → revisión del PR.

### Alcance y owner
- **RF19. Sin cambios en `crates/protocol`.** `FightLog`, `FightEvent` y los enums siguen iguales, `just gen` no produce diferencias y `packages/protocol-ts` no se toca. Tampoco cambian el formato de la `Timeline`, `TIMELINE_LIMITS` ni `validateTimeline`. → `just check` (frescura de `just gen`) y diff del PR.
- **RF20. Confirmación de los dos ritmos.** Antes del merge, el owner mira en la arena el dorado de KO con ≥ 2 respiros (RF9) y `log-ko-knockdown`, con `pace` en 1 y con el mínimo, y deja su nota en `## Revisión` de esta spec: ¿se notan los dos ritmos? Sí, no o "a medias", con una línea de por qué. Un "a medias" cuenta como "no", salvo que el owner diga explícitamente otra cosa. Si dice que no, en el mismo PR solo se ajustan **valores** (`breather_*` o `interval_*`), se regenera todo lo de RF9 y RF10 y se vuelve a medir RF13. Si el motivo es que la pausa se siente muerta, el primer ajuste es `breather_span_ms` 800 → 600, antes de tocar `breather_after_resolutions` o `breather_chance_permille`.

## Requisitos no funcionales
- **Determinismo (ADR-0001):** el respiro solo consume `Rolls::roll` en el orden de RF3. Ni reloj, ni aleatoriedad fuera del PRNG, ni `HashMap` o floats. Misma semilla, mismos bytes, en nativo y en WASM (`_check-wasm`).
- **Semilla (ADR-0002):** sin cambios. El log sigue sin contener la semilla (aserción de `golden.rs`). El respiro no deja ver nada del futuro: es solo ausencia de eventos, y no justifica que el cliente reciba eventos por adelantado (ver "Modelo de datos").
- **Rendimiento:** 10 000 peleas de `just sim` en ~0.3 s en release (como hoy). El test de propiedades en *debug* no crece. `paceMin` hace ≤ 51 validaciones por carga, solo en `/dev/**`. El VPS no se ve afectado (nada de esto corre en el server).
- **i18n y accesibilidad:** dos mensajes nuevos en es/en, sin literales. La fila nueva de `ArenaStats` sigue el patrón `dl` existente.
- **Seguridad:** `/dev/**` sigue dando 404 en producción. Las 3 copias nuevas son dorados con semillas de test públicas. No se agregan dependencias (ni `proptest`), así que no aplica `crate-age`.

## Diseño

### Componentes afectados
```
crates/engine/src/ruleset.rs        RF1, RF6 (campos, derivados, cotas)
crates/engine/src/sim.rs            RF2, RF3 (respiro, pasos 6–7; tests unitarios y roller guionado)
crates/engine/src/contract.rs       RF5 (nuevo: TIMELINE_CONTRACT)
crates/engine/src/lib.rs            RF5 (pub use)
crates/engine/tests/common/mod.rs   RF5 (assert_invariants sin literales)
crates/engine/tests/properties.rs   RF7
crates/engine/tests/golden.rs       RF9 (semillas, cobertura, vector d)
crates/engine/tests/golden/*        RF9 (regenerados; timeline-contract.json nuevo)
crates/sim/src/report.rs (+tests)   RF11, RF12
crates/sim/tests/{balance,cli}.rs   RF12, RF13 (+ fixture balanced)
apps/web/src/lib/features/arena/retime.ts           RF14–RF16
apps/web/src/lib/features/fight/engine-logs.ts      RF10
apps/web/src/lib/features/fight/fixtures/*          RF10 (7 copias)
apps/web/src/routes/dev/arena/+page.svelte          RF16, RF17
apps/web/src/lib/components/fight/ArenaStats.svelte RF17
apps/web/messages/{es,en}.json                      RF17
apps/web/tests/{retime,fight-log-goldens,timeline-contract}.test.ts  RF5, RF10, RF15–RF17
apps/web/e2e/{arena,arena-webkit}.spec.ts           Estrategia de pruebas
packages/renderer/test/fixtures/golden/*.timeline.json  RF10 (regenerados)
docs/features/{002,003,005}/spec.md, docs/architecture.md  RF18
```
No se tocan `crates/protocol`, `packages/protocol-ts`, `crates/engine-wasm/src`, `crates/content`, `content/`, `packages/renderer/src` ni `apps/server`.

### Modelo de datos / tipos (protocol)
Sin cambios (RF19). El respiro no es un evento. Que no lo sea mantiene el alcance del intent y evita una `timeline_version` nueva. **El cliente nunca conoce eventos futuros (ADR-0002).** En vivo, el narrador (ADR-0010) detecta la pausa por el **tiempo transcurrido sin eventos** desde el último que recibió, o corre en el server, que sí tiene el log. El respiro **no** justifica ningún look-ahead del cliente: ningún evento llega antes de lo que permita el buffer que fije la spec de sincronización en vivo, y esta feature no lo amplía ni lo condiciona. Si en el futuro hace falta nombrarlo (un cue `breather` o un `taunt`), entra con una feature propia: un evento de `protocol`, `timeline_version` 3 y una `ruleset_version` nueva si ya hay peleas en producción.

### API / mensajes WS
Ninguno.

### Flujo de una acción (pasos de PRNG entre corchetes)
```
next_actor ─▶ [1] elección ─▶ attack ─▶ [2] defensa ─▶ [3] varianza? ─▶ [4] crítico?
          ─▶ [5] jitter ─▶ resolución (hit/block/dodge) ─▶ knockdown? ─▶ low_hp ─▶ momentos
          ─▶ KO? ──sí──▶ ko + end (sin pasos 6–7)
              │no
              ▼
          knockdown? ──sí──▶ since_breather = 0
              │no
              ▼
          since_breather += 1 ─▶ ≥ after? ──no──▶ fin de la acción
                                    │sí
                                    ▼
                               [6] roll(1000) < chance? ──no──▶ fin
                                    │sí
                                    ▼
                               [7] d = min + roll(span+1);  a.next_at += d;  b.next_at += d;  since_breather = 0
```

### Derivación de los valores
- **`breather_min_ms = 1800`** = `PAUSE_GAP_MIN_MS` (2500) − `strike_gap_min()` (700). Es el menor valor que garantiza que todo respiro cuenta como pausa, y el test de RF12 lo ata.
- **`breather_span_ms = 800`.** Las pausas resultantes van de 2.5 s a ~4.6 s (la separación más larga por pelea tiene p50 de 3.5 s, p90 de 4.0 s y máximo de 4.6 s). Con 1000 el máximo sube a 4.9 s, y con 600 la media baja apenas 7 ms. 800 deja hueco para una frase del narrador sin que la pausa se sienta muerta con solo `idle`. Es una decisión de experiencia, y el game-designer la puede revisar.
- **`breather_chance_permille = 200` y `breather_after_resolutions = 9`.** El espaciado esperado entre respiros es de 9 + (1/0.2 − 1) = 13 resoluciones. Con 43 resoluciones en la pelea mediana, salen ~3 respiros (2.6 de media, porque los knockdowns reinician el contador). La media esperada es 1096 + 2.6 × ~2.2 s / 42 ≈ 1230 ms, y se midieron 1226. El primer respiro posible llega tras ~13 s de combate, así que la pelea abre con una ráfaga.
- **Los `interval_*`, el jitter y los windups no cambian.** La ráfaga ya existe (30 % de intervalos ≤ 900 ms sin respiro), y el respiro la baja apenas a 28.5 %.

Barrido de calibración (motor prototipo fuera del repo, `content/` real, 10 000 cruces, semilla por defecto; `min` 1800 salvo donde se indica):

| `after` | `chance` ‰ | `span` | media | ráfaga ‰ | pausas p10/p50 | peleas con pausa ‰ | `end.t` p90 | espejos `a` | RF17 |
|---|---|---|---|---|---|---|---|---|---|
| (sin respiro) | — | — | 1096 | 303 | 0/0 | 132 | 63.8 s | 50.3 % | pasa |
| 8 | 150 | 1000 | 1226 | 285 | 1/3 | 981 | 70.8 s | **46.9 %** | **falla** (espejos) |
| 8 | 200 | 800 | 1241 | 282 | 2/3 | 993 | 71.5 s | 47.9 % | pasa |
| **9** | **200** | **800** | **1226** | **285** | **1/3** | **989** | **70.6 s** | **49.3 %** | **pasa** |
| 10 | 200 | 1000 | 1220 | 287 | 1/2 | 987 | 70.3 s | 48.7 % | pasa |
| 6 | 250 | 1000 | 1312 | 274 | 3/4 | 998 | 75.2 s | 51.4 % | pasa (media fuera de banda) |

### UI (arena de prueba)
- **Estados:** iguales a los de hoy (`loading`, `paused`, `playing`, `finished` y `error`). `?pace` ya no puede llevar a `error`.
- **`ArenaStats`:** una fila más ("Ritmo (factor)" con `0,86`) y, si hubo clamp, la nota "ajustado desde 0,80" en `text-muted-foreground`, debajo del valor. En móvil (375 px) entra en la grilla de 2 columnas que ya existe.
- **Copiar JSON:** `pace_requested`, `pace_effective` y `pace_min`, solo con `?pace`.

## Áreas de preocupación
1. **`TIMELINE_CONTRACT` frente a RF4 de 003** ("ninguna constante de juego vive fuera de `Ruleset`"). No es una constante de juego, sino el contrato del consumidor, y la simulación no la lee. Aun así, es una segunda copia de `TIMELINE_LIMITS`. La alternativa, hacerla campo de `Ruleset`, la descarto: volvería versionables por ruleset unas reglas que son del renderer. El riesgo de divergencia lo cierra el dorado más el Vitest de RF5. El security-reviewer y el product-owner deberían confirmar que no ven una regla de juego escondida.
2. **Fragilidad del criterio de espejos (47–53 %).** Con 3332 espejos, la banda es de ±3.5σ, y en el barrido osciló de 46.9 a 51.4 % con parámetros vecinos, que es puro ruido de muestreo (el juego es simétrico). Un ajuste de valores futuro puede fallar por azar. Esta spec no cambia la banda. El 46.9 % está a ~3.6σ, así que no se asume ruido: RF13 fija la medición con ≥ 3 semillas raíz y trata una salida de 47–53 % o un sesgo consistente como bug estructural. Nunca se eligen valores "hasta que pase". Si, descartado el bug, la banda sigue siendo frágil, se amplían los espejos del calendario de `sim` en un cambio aparte (game-designer).
3. **La media depende del roster.** Personajes rápidos bajan la media, y lentos la suben. El fixture `balanced` ya cae en 1098 ms (RF13). Eso es lo que se busca (el ritmo pasa a ser un criterio de contenido), pero limita el diseño de `speed` en personajes futuros. En el mismo PR, el game-designer agrega al skill `write-character` la nota de que `speed` mueve la media de ritmo y de que un personaje nuevo tiene que pasar los criterios de ritmo de `just sim --check`.
4. **Reescribir v1 es la última ventana.** Vale mientras ninguna pelea se guarde en producción (RF8). Si TA-13 o el scheduler se fusionan antes que este PR, esto pasa a ser `ruleset_version` 2 con la v1 conservada. Este PR debe fusionarse antes de cualquier feature que guarde peleas.
5. **Todos los bytes cambian.** Los 7 logs, el digest, las 7 copias y las 3 timelines del renderer. Los e2e que dependen del contenido de un dorado pueden moverse: "ko-comeback: b gana con a en 0 HP", el test del knockdown sobre `ko-comeback`, las capturas en `?t=12500` y los `?t=86500`. El plan los ata a los datos del JSON importado, no a cifras escritas a mano, y el test del knockdown pasa a `ko-knockdown`.
6. **El respiro cerca del tope.** Un respiro tardío puede convertir un KO en `time_up`. Medido: 0.1 % de `time_up`, lejos del 10 %. Las peleas defensivas (`turtle`) siguen dando decisiones y empates, así que los dorados de fin por tiempo existen.
7. **Pausas largas con solo `idle`.** Hasta ~4.6 s sin acción visible, sin `walk` ni `taunt` (fuera de alcance, 004). Si al owner le parece muerta, se baja `breather_span_ms` (RF20). La coreografía de la pausa es del renderer y de 004.
8. **`?pace` del spike.** El mínimo dinámico también se aplica al guion escrito a mano. Su mínimo sale de su propia separación más corta, y el Vitest de RF16 lo incluye.
9. **Enmiendas a specs `done` (002, 003 y 005).** Son editoriales y van en el mismo PR (RF18). Ninguna relaja una regla de la `Timeline`.

## Respuestas a preguntas abiertas del intent
- **P: ¿Un mecanismo de pausa explícito o basta con ampliar el jitter?** → R: un mecanismo explícito, el **respiro** de RF2. El jitter uniforme no da una distribución con dos picos. Sin respiro, solo el 13 % de las peleas tiene alguna separación ≥ 2.5 s, y todas salen de knockdowns.
- **P: ¿Bump de `ruleset_version` o reescribir la v1?** → R: reescribir la v1 (RF8), con la condición de la preocupación 4.
- **P: ¿Cómo se acota `pace`?** → R: con un mínimo dinámico sobre la `Timeline` adaptada, calculado con `validateTimeline` (RF15), un piso de 0.5 y un techo de 1.5. Fuera de rango, **clamp** al límite válido, con el factor efectivo visible e informado (RF16 y RF17). El rechazo con código lo descarto (ver Alternativas).
- **P: ¿Las pausas cuentan en la separación media?** → R: sí. La media es sobre todas las separaciones, pausas incluidas, igual que `mean_gap_ms` y `meanStrikeGapMs` (RF11).
- **P: ¿Qué intervalo cuenta para las reglas? ¿El knockdown es una pausa?** → R: las métricas cuentan por separación, sin mirar la causa. Un knockdown cuenta como pausa solo si su separación llega a 2500, y casi nunca llega (da de 1900 a 2200 ms). En el mecanismo, un knockdown reinicia el contador del respiro y no sortea respiro en esa acción: es un tercer tiempo propio, la "caída". Las reglas de la `Timeline` (≥ 600 ms y nada durante la caída) se aplican igual a todas las separaciones.
- **Pendientes de la revisión del intent:**
  - una fuente por cifra: la tabla del principio;
  - el % mínimo de ráfaga: 250 ‰ (RF12);
  - dónde se consumen los sorteos y RF17 entero: RF3 y RF13;
  - el respiro no es un evento: RF19 y "Modelo de datos";
  - el knockdown: la respuesta anterior;
  - el `pace` sobre la `Timeline` adaptada y el clamp: RF15 y RF16.

## Estrategia de pruebas
- **Unit (`engine`, `sim.rs`):**
  - el roller guionado sigue RF3 con los pasos 6–7 y comprueba sus cotas (`roll(1000)` y `roll(801)`);
  - con el sorteo del paso 6 forzado a 0, la resolución siguiente llega a ≥ `t_res + breather_gap_min()` y el orden de actores no cambia frente al mismo guion sin respiro;
  - con el sorteo ≥ `chance`, no hay desplazamiento;
  - una acción con knockdown o con KO no consume los pasos 6–7, y el contador se reinicia tras un knockdown y tras un respiro;
  - antes de `breather_after_resolutions` resoluciones no se sortea nada;
  - `rolls_follow_the_rf7_order_in_a_real_fight` acepta los pasos nuevos.
- **Cotas (`ruleset.rs`):** RF6.
- **Propiedad (`properties.rs`):** RF7 (10 000 logs y los extremos, más la no vacuidad del respiro).
- **Dorados (`golden.rs`, nativo y WASI):** RF9, incluido el vector (d) de `TIMELINE_CONTRACT`.
- **`sim`:**
  - `report/tests.rs`: las métricas de RF11 en sus bordes y cada `Failure` nuevo;
  - `balance.rs`: `Criteria::RF17` completo sobre `content/`;
  - `cli.rs`: las llaves de `--json` y el fixture `balanced` ajustado;
  - los tres tests de atadura de RF12.
- **Vitest (`apps/web`):**
  - `parsePace`: solo sintaxis, así que `0.79`, `0` y `9.99` se aceptan y `1.255`, `abc` y ` 1` no;
  - `paceMin` y `resolvePace` para los 7 dorados adaptados y el spike: el mínimo es 0.86 en los dorados; cada centésima de `[mínimo, 1.5]` pasa `validateTimeline` con los manifests de los placeholders; si mínimo > 0.5, `mínimo − 0.01` falla **solo** con `timeline.strike_gap` o `timeline.strike_during_knockdown`; un pedido por debajo del mínimo o por encima de 1.5 da clamp, y uno dentro del rango da `clamped: false`; con una `Timeline` construida inválida con factor 1, `paceMin` da `null` sin lanzar y `resolvePace` da `effective: 1`;
  - `fight-log-goldens.test.ts`: las 7 copias, byte a byte y con el conjunto exacto;
  - `golden-timelines.test.ts`: timelines regeneradas;
  - `timeline-contract.test.ts`: RF5;
  - `meanStrikeGapMs` sigue igual.
- **e2e (`just e2e`, Chromium):** para cada uno de los 7 `?log=`, en `es`:
  - (a) `&t=99999999999` llega a `finished` con el final correcto (`data-ending`);
  - (b) `&pace=0&t=99999999999` llega a `finished`, y con las estadísticas abiertas la fila de ritmo muestra el mínimo con `data-clamped="true"`;
  - (c) `&pace=1.5` llega a `finished`.

  `en` se cubre con los tests por idioma que ya existen. Además: "copiar JSON" con `?log=ko-knockdown&pace=0.8` incluye `pace_requested: 0.8`, `pace_effective: 0.86` y `pace_min: 0.86`, y ningún caso deja errores de consola. El test del knockdown pasa a `ko-knockdown`, y los que leen ganador o HP los toman del JSON importado.
- **e2e (WebKit, `arena-webkit.spec.ts`):** cada uno de los 7 logs con `pace=0` (clamp al mínimo) llega a `finished`, o queda en `webgl_unavailable` con la anotación `limited` (preocupación 5 de 005). En ese caso se repite a mano en Safari y el resultado va al PR.
- **Manual:** RF20 (owner) y el informe completo de `just sim` en el PR (RF13), firmado por el game-designer.
- **`just check`** completo, incluidos `_check-wasm` y la frescura de `just gen` (sin diferencias, RF19).

## Alternativas consideradas
- **Ampliar el jitter (o hacerlo bimodal).** Un jitter uniforme más ancho sube la media y convierte ráfagas en intervalos medios, sin dar pausas de ≥ 2.5 s dentro de los 90 s. Uno bimodal es un respiro disfrazado, pero atado al atacante: rompería la simetría del desplazamiento y cambiaría quién actúa. Descartada.
- **Respiro que fija los dos relojes en `t_res + d`** (en vez de desplazarlos). Da pausas más parejas (2.5–3.3 s), pero empata los relojes y el desempate de RF8 le da siempre el turno al defensor, que es un efecto de balance sistemático. Descartada a favor del desplazamiento, que conserva el orden.
- **Disparar el respiro tras un golpe fuerte o un crítico.** Hace que los personajes con mucho `special` provoquen más pausas, otro eje de balance. Se puede sumar en el futuro como un campo nuevo, pero no hace falta para el intent.
- **Un evento `breather` en el log.** Cambia `protocol`, `protocol-ts`, el adaptador y la `timeline_version`, fuera del alcance del intent. Descartada (ver "Modelo de datos").
- **Rechazar `?pace` fuera de rango con un código** (`arena.pace_out_of_range` y un estado de error). Es más estricto, pero deja la arena inutilizable con un enlace viejo (`pace=0.8`) cada vez que cambie el ruleset. El clamp con nota visible da la misma información sin romper la reproducción. Descartada.
- **Mínimo analítico (`ceil(600 / separación mínima)`).** Duplica la regla del validador y no cubre el knockdown ni el redondeo. Descartada a favor de RF15.
- **`proptest`.** Es una dependencia nueva para `engine` (`crate-age`, `check-engine`), y 003 fijó "propiedad sin crates nuevos" con semillas derivadas, que ya dan 10 000 casos deterministas. Descartada.
- **`ruleset_version` 2.** Obliga a conservar para siempre una v1 que ninguna pelea real usó (RF10 de 003). Descartada mientras valga la preocupación 4.

## Revisión
### product-owner — 2026-10-27 — APPROVE
- Verifiqué: resuelve el problema del intent (pausas que hoy no existen, ~1.2 s de media, `?pace` que no rompe con logs reales). Cada criterio de éxito tiene su RF verificable: banda 1100–1300 ms (RF12), ≥ 90 % en 45–90 s y `time_up` ≤ 10 % (RF12/RF13), propiedad ≥ 600 ms (RF7), dorados regenerados (RF9/RF10), los 7 dorados en la arena en Chromium y WebKit (RF10, e2e) y la confirmación del owner (RF20). Respondió las 5 preguntas abiertas y los pendientes del architect. Alcance de MVP: sin `protocol`, sin evento nuevo, sin narrador ni `taunt`, `pace` solo en `/dev/**`. ADR-0004 (sin contenido nuevo ni franquicias), ADR-0005 (no toca monedas) y ADR-0007 (dos mensajes es/en, sin literales); la fila nueva entra en la grilla de 375 px. Que la banda de la media necesite mi OK me parece bien.
- Preocupación 1 (`TIMELINE_CONTRACT` frente a RF4 de 003): confirmo que no veo una regla de juego escondida. La simulación no la lee, no cambia bytes del log y sus valores son del renderer; `BURST_GAP_MAX_MS` y `PAUSE_GAP_MIN_MS` son definiciones de medida de `sim`, y las entradas del respiro sí están en `Ruleset`.
- Arrastrado al plan (no bloquea): (1) sumar a RF18 una enmienda a RF4 de 003 que diga que los contratos del consumidor (`TIMELINE_CONTRACT`) y las definiciones de medida de `sim` no son constantes de juego, para que nadie lo lea como violación; (2) RF20 define qué pasa con "no", pero no con "a medias": que el plan lo trate como "no" salvo que el owner diga lo contrario; (3) preocupación 4: dejar en Linear que TA-21 bloquea a TA-13 y al scheduler, no solo en la spec.

### security-reviewer — 2026-10-27 — REQUEST_CHANGES
- Verifiqué: pasos 6–7 de RF3 solo con `Rolls::roll`, cotas `roll(1000)`/`roll(801)` comprobadas por el roller guionado, consumidos tras KO/knockdown (sin ramas que dependan de algo fuera del estado), `saturating_add` con desbordamiento acotado en RF6; el log sigue sin semilla (`golden.rs`) y el respiro no es evento ni campo, así que en vivo un espectador solo ve el hueco cuando ya pasó, y su ausencia en una acción con KO no revela nada que el `ko` no diga ya. `TIMELINE_CONTRACT` no es regla de juego ni entrada de `sim.rs` (no cambia bytes), y el dorado más el Vitest cierran la divergencia. `?pace`: regex acotada (≤ 4 caracteres, sin `NaN`/`Infinity`/exponentes), clamp a `[min, 1.5]`, ≤ 51 validaciones, solo `/dev/**` (404 en producción); las 3 copias nuevas son dorados con semillas de test públicas. Sin dependencias nuevas.
- Cambios pedidos (Important, contradice ADR-0002): en "Modelo de datos", "El narrador puede detectar el hueco con su look-ahead, porque el próximo evento está a ≥ 2.5 s" presupone que el cliente conoce el próximo evento con ≥ 2.5 s de antelación. Escenario: la spec del narrador o del vivo toma esta frase como requisito y el server adelanta eventos ≥ 2.5 s para que el narrador del cliente "vea" el respiro; cualquiera con devtools o un bot sabe 2.5 s antes el próximo golpe, crítico o el KO. Reescribir: en vivo, el narrador detecta la pausa por **tiempo transcurrido sin eventos** (o corre en el server, ADR-0010); ningún look-ahead del cliente supera el buffer que fije la spec de sincronización, y este respiro no justifica ampliarlo.
- Sugerencia (no bloquea): RF15 no define qué devuelve `paceMin` si el factor 1 ya es inválido (no hay "último `k` válido"); que devuelva 1 o `null` y RF16 lo trate sin excepción.

### game-designer — 2026-10-27 — APPROVE
- Verifiqué: responde a mi intent (respiro explícito, sin evento, media sobre todas las separaciones, v1 reescrita, `pace` dinámico con clamp). El respiro es simétrico: se sortea tras cualquier acción, desplaza los dos relojes por igual y no cambia quién actúa, así que no crea un eje de balance por personaje. Me parece bien que el knockdown reinicie el contador (la "caída" es un tercer tiempo propio y no se apila con la pausa), y que la pelea abra con ~13 s de ráfaga. `breather_min_ms` = 2500 − 700 está bien derivado y atado por test. Los criterios de `--check` tienen margen razonable sobre la calibración: media 1226 en 1100–1300, ráfaga 285 ‰ contra ≥ 250, pausas p50 3 contra ≥ 2, 989 ‰ con pausa contra ≥ 900, banda de duración subida a 900 ‰ con 986. RF17 entero sigue pasando: win rates de 48.9 a 51.1 %, momentos en el 38.1 % con cada `kind` ≥ 11 %, `time_up` en el 0.1 %. Las pausas de hasta ~4.6 s solo con `idle` son aceptables: p90 de la pausa más larga en 4.0 s, y RF20 da la salida.
- Condiciones para el plan y el PR (no bloquean):
  - **Espejos:** el 46.9 % de la fila 8/150/1000 está a ~3.6σ (σ ≈ 0.87 % con 3332 espejos). Es raro para ser puro ruido entre 5 configuraciones. En RF13, el PR pega el % del lado `a` en espejos con los valores elegidos y con 8/150/1000, cada uno con ≥ 3 semillas raíz. Si alguno sale fuera de 47–53 o todos quedan del mismo lado de 50, se trata como un sesgo estructural que hay que investigar (desempate, `saturating_add` cerca del tope), no como ruido, y no se arregla moviendo valores.
  - Si el owner dice "a medias" o "no" en RF20 porque la pausa se siente muerta, el primer ajuste es `breather_span_ms` 800 → 600 (la media baja ~7 ms), antes de tocar `after` o `chance`.
  - Preocupación 3: en el mismo PR agrego a `write-character` la nota de que `speed` mueve la media de ritmo y que un personaje nuevo tiene que pasar los criterios de ritmo de `--check`.

### architect — 2026-10-27 — Respuesta a revisiones
- security-reviewer (bloqueante): reescribí "Modelo de datos". Ya no hay look-ahead: el cliente nunca conoce eventos futuros (ADR-0002); en vivo, el narrador detecta la pausa por el tiempo transcurrido sin eventos, o corre en el server; el respiro no justifica ningún look-ahead del cliente más allá del buffer que fije la spec de sincronización, ni lo amplía. El RNF de semilla remite a ese párrafo.
- security-reviewer (sugerencia): RF15 define que `paceMin` devuelve `null` si la `Timeline` ya es inválida con factor 1, sin lanzar; `resolvePace` queda con `effective: 1` y la página muestra el error de validación previo. Sumé el caso al Vitest de la estrategia de pruebas.
- product-owner: RF18 suma la enmienda a RF4 de 003 (`TIMELINE_CONTRACT` y las definiciones de medida de `sim` no son constantes de juego). RF20: un "a medias" cuenta como "no", salvo que el owner diga explícitamente otra cosa. El bloqueo de TA-13 y el scheduler en Linear queda para el plan.
- game-designer: RF20 fija `breather_span_ms` 800 → 600 como primer ajuste ante una pausa muerta. RF13 exige medir espejos con ≥ 3 semillas raíz con 9/200/800 y 8/150/1000, y trata una salida de 47–53 % o un sesgo consistente como bug estructural, sin mover valores hasta que pase; la preocupación 2 lo refleja. La preocupación 3 pasa a requisito del PR: la nota sobre `speed` en `write-character`.

### security-reviewer — 2026-10-27 — APPROVE
- Verifiqué: "Modelo de datos" ya no presupone look-ahead; el narrador en vivo detecta la pausa por tiempo transcurrido sin eventos o corre en el server, y el respiro no amplía ni condiciona el buffer de la spec de sincronización (ADR-0002). El RNF de semilla remite a ese párrafo y no queda otra mención de eventos por adelantado. RF15/RF16 definen `paceMin` → `null` con la `Timeline` inválida con factor 1, sin excepción y con `effective: 1`, cubierto en el Vitest.
- Nit (no bloquea): la firma de RF15 dice `paceMin(timeline): number`; con el caso nuevo debería ser `number | null`.

**🤖 security-reviewer**

### owner — 2026-10-27 — OK (risk: high)
- OK en chat para seguir con el plan y la implementación.

### architect — 2026-10-27 — Enmienda técnica (revisión de plan.md)
- Al revisar `plan.md` admití una precisión de firma en RF15 y RF16: `paceMin(timeline, isValid): number | null` y `resolvePace(requested, timeline, isValid)` reciben el validador como función `(t: Timeline) => boolean`, y la prop `retime` de `FightStage` pasa a `(timeline, isValid) => Timeline`. `FightStage`, que ya carga el renderer con `import()`, pasa `(t) => validateTimeline(t).ok` sin manifests; los Vitest pasan el validador real. Motivo: `retime.ts` no puede importar valores del renderer (`rendererRestriction` de ESLint, ADR-0009 D4, y `check-renderer-split`, 002 RF-A17). La regla sigue teniendo una sola fuente (`TIMELINE_LIMITS` y `validateTimeline`), y el algoritmo, el piso, el techo, el clamp y el caso `null` no cambian.
- No reabro la revisión de product-owner, security-reviewer ni game-designer: no cambia lo que ve el usuario, el ritmo, los criterios ni la superficie de `?pace`.


### owner — 2026-10-28 — RF20: sí
- Revisé en `/dev/arena` (rama del PR, puerto 5174) `log-ko-clutch` y `log-ko-knockdown` con `pace=1` y `pace=0` (ajustado al mínimo). Se notan los dos ritmos y el resultado queda aprobado tal como está; por ahora los valores `breather_*` e `interval_*` se quedan así.
