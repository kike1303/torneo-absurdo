---
id: "010"
title: Scheduler de peleas en vivo, fase C (reproducción parcial en el renderer)
type: plan
status: approved
risk: medium
author: frontend-engineer
reviewers: [architect]
approvals:
  - by: architect
    date: 2026-10-31
linear: TA-13
depends_on: ["002", "005"]
created: 2026-10-31
updated: 2026-10-31
---

# Plan: Scheduler de peleas en vivo, fase C (desde spec.md 2026-10-31)

> [intent](./intent.md) · [spec](./spec.md) · [plan-a2 de 002](../002-spike-renderer/plan-a2.md) · [plan-c de 002](../002-spike-renderer/plan-c.md) · [plan de 005](../005-timeline-v2-fines-sin-ko/plan.md) · ADRs [0002](../../decisions/0002-commit-reveal-semilla.md), [0007](../../decisions/0007-i18n-desde-cero.md), [0009](../../decisions/0009-ui-design-system.md)

**Alcance:** RF46 a RF52 y el RNF de `prepareScene` (apartado "CPU"). Es decir, `Timeline` parcial, `loadLive` y `extend`, el adaptador de prefijos y el goteo sin server en `/dev/arena`.

**No toca** `crates/**`, `packages/protocol-ts/**`, `apps/server/**`, `infra/**`, `content/**`, `routes/+page.*` ni `stage/`. No hay tipos nuevos de `protocol`, cliente WS ni dependencia de las fases A y B. Ninguna ruta de usuario usa esto hasta la fase D.

**Riesgo `medium`, no el `high` de la spec:** el riesgo alto de 010 está en el server (semilla, ledger, recuperación). Esta fase corre solo en `/dev/arena`, no maneja secretos ni monedas, y `load` queda igual, con sus tests sin cambios. El paso delicado (preparación incremental) tiene un oráculo exacto.

## Decisiones del architect
Resueltas en `## Revisión`, abajo:
1. **Preparación incremental ahora:** el paso 5 es un commit propio y no se salta. La medición que lo motivó: con 200 cues sintéticos, una llamada cada 25 ms, en un Apple M2 Pro con Node 22.23.4, `prepareScene` da p50 de 1,21 ms y p95 de 2,09 ms, y el límite es 4 ms en un Android de gama media.
2. **Prueba del RNF por aproximación aceptada,** con los umbrales de la decisión 8. La medición en Android sigue pendiente desde 002.
3. **Desviaciones 1 a 5 aceptadas,** con su texto en "Enmienda propuesta a la spec". **La 6 pasa al plan D.**

## Comprobación previa de RF49
Con un script de solo lectura, sin commitear, comprobé la propiedad contra el código actual. Simula el resultado del adaptador parcial: corta los cues por `t ≤ c`, quita el cierre mientras no llega `end` y usa `duration_ms = 90 000`. Cubre los 7 dorados, el corte más fuerte de cada prefijo y unos 208 000 instantes en sus últimos 4,5 s, con y sin movimiento reducido.

| Retraso | Instantes que difieren |
|---|---|
| 3500 | 0 |
| 2600 | 0 |
| 2500 | 0 |
| 1000 | 666 |
| 800 | 826 |
| 400 | 1244 |
| 0 | 6626 |

La muestra sigue el margen de la spec: el cierre es lo que manda, y **no hace falta tocar la coreografía para cumplir RF49**. En todos los cortes por tiempo, los únicos códigos de `validateTimeline` son `ending_missing` y `closing_missing`.

## Decisiones de implementación

### Validación (RF46)
1. **Firma:** `validateTimeline(json, manifests?, options?: { partial?: boolean })`. Sin la opción, el resultado es idéntico al de hoy, y los snapshots de `timeline-v1` y `timeline-v2` no cambian.
   - `checkEndingV2` recibe `partial`. En `finish` omite solo `ending_missing` y `closing_missing`. El resto de `finish` ya depende de que exista un cierre, así que un cierre presente se valida igual.
   - `partial` con `timeline_version: 1` da `timeline.unsupported_version` en `/timeline_version`, sin código nuevo.
   - Todas las demás reglas siguen, incluidas `intro_first` con `cues` vacío y `hp_zero_without_ko` con un golpe a 0 HP al final. Esta última no molesta: en los 7 dorados, `hit`, `knockdown`, `moment` y `ko` del mismo golpe comparten `t`, y `release_due` (RF23) y RF40 entregan todo lo que cumple `t ≤ elapsed`, así que un corte por tiempo no los separa. Un test lo comprueba sobre los dorados.

### Reproductor (RF47, RF48, RF50)
2. **`loadLive(timeline, assets): Promise<void>`** es como `load` con `partial: true`, y comparte con él los pasos de carga e instalación.
   - El estado nuevo es `live: { timeline, manifests, installed } | null`. Se escribe de forma síncrona tras validar y `load` lo limpia al empezar.
   - Con `loadLive` en curso, `extend` valida y guarda la timeline más nueva, y la instalación prepara esa. Así el host no necesita una cola para los lotes que llegan mientras cargan las hojas.
3. **`extend(timeline): void`** es síncrono y "rechaza" lanzando `RendererError('timeline_invalid')`. Si rechaza, no cambia nada. Tras `destroy` no hace nada, como los demás métodos. Lanza si:
   - no hay un `loadLive` vigente (nunca hubo, falló o lo reemplazó un `load`);
   - falla `validateTimeline(…, { partial: true })`;
   - los `fighters` difieren en `asset` o `max_hp`, porque no se recargan hojas;
   - algún cue anterior cambia o falta (`sameCue`, comparación campo a campo, sin `JSON.stringify`);
   - cambia `duration_ms` sin que llegue el cierre, o cambia después de que llegó.

   Un `extend` con los mismos cues se acepta, porque un `fight_sync` tras reconectar puede repetirlos. No toca `playback` ni `cursor`, ni llama a `stage`: el siguiente frame pinta la escena nueva.
4. **El renderer no conoce `end`:** "ya llegó" es `(win ?? draw) !== null`. El adaptador solo emite el cierre al procesar `end` y en ese momento fija `duration_ms = end.t`, así que las dos cosas llegan juntas y no hace falta un campo nuevo en la v2.
   - Un helper `reachedEnd(p, t) = t ≥ p.durationMs && (p.win ?? p.draw) !== null` reemplaza los `t >= p.durationMs` de `frame`, `play`, `pause`, `seek` y `settledState`.
   - `phaseAt` usa la misma condición. Si no, una pelea sin `end` daría la fase `finished` en `t = 90 000` (server detenido, por ejemplo), contra RF48.
   - Una timeline de `load` siempre tiene cierre, así que `load` no cambia.
   - `snapshot()` no suma la duración ni el progreso (RF48). `durationMs` de RF47 es el campo interno de `PreparedScene`.
5. **Entrar tarde, solo en vivo.** En `play()`, con `live?.installed` y `playback === INITIAL_PLAYBACK`, `cursor = cursorAt(p.cueTimes, timeAt(...))`. Reanudar tras una pausa no lo mueve. Mientras una `loadLive` carga, `prepared` sigue siendo la pelea anterior y no hereda esta regla.
6. **Reloj negativo, solo en vivo.** `timeAt` ya recorta a `t = 0`, pero `dueCues` daría por vencido el `intro` en `at_ms = 0`, hasta 3,5 s antes de su hora. Con `live?.installed`, `frame` pinta y no dispara cues mientras el reloj sea negativo. El `intro` sale al llegar a 0, en todos los clientes a la vez. Ver desviación 3.
7. **Un listener de cue puede llamar a `extend`.** Hoy, `frame` corta el bucle con `prepared !== p`, y un `extend` dentro de un listener perdería para siempre los cues restantes del lote, porque `cursor` ya avanzó. La condición pasa a usar un contador `installs`, que solo suben `load` y `loadLive` al instalar. `seek` ya se detecta con `cursor !== due.to`. El bucle sigue con `p.cues`: los índices son estables porque los cues nuevos solo se agregan al final.

### Preparación incremental (RNF, sin cambiar RF47)
8. **`prepareSceneFrom(prev, timeline)`** en `scene/prepare.ts`, para cuando el player ya comprobó que los cues de `prev` son prefijo de los de `timeline`. Comparte el núcleo con `prepareScene`.

   Reparto aproximado del total de 1,21 ms en p50, medido por partes: `buildChoreography` 0,32 ms, `deepFreeze` 0,44 ms, tablas de tiempos 0,13 ms, y el resto en construir los cues.

   | Parte | Qué hace | Por qué es correcto |
   |---|---|---|
   | Tablas de tiempos y `scale` | Se reutilizan de `prev.fighters` | Son funciones del manifest, que no cambia. No necesita `manifests` |
   | `buildChoreography` | Se ejecuta completa | Es pura y mantiene una sola lógica. No hay puntos de control de la simulación |
   | `PreparedCue` estables | Se reutiliza el objeto anterior, con `at_ms + LOOKBACK_MS < H`, donde `H = últimoAt(prev) − MAX_WINDUP_MS − RETURN_MS` | Un cue nuevo tiene `at_ms ≥ últimoAt(prev)`, y su windup empieza en `≥ últimoAt(prev) − MAX_WINDUP_MS`. La única lectura del futuro es `afterAction`, a lo sumo `RETURN_MS` por delante. Y un segmento termina en `≤ at_ms + MAX_ACTION_MS + RETURN_MS` (comentario de `Segment.owner`) |
   | `deepFreeze` | Ya se detiene en lo congelado, así que solo camina lo nuevo y los arreglos | Sin cambios |
   | `hp`, `admitFlashes` | Se recalculan sobre los cues completos | Son O(n) muy baratos, y la admisión es voraz hacia adelante |

   **Oráculo con los dorados:** `prepareSceneFrom(prev, t)` es profundamente igual a `prepareScene(t, manifests)`, para los 7 dorados y todos los cortes, avanzando de a un lote y de a varios. **No prueba el límite `H`:** los placeholders tienen windups de hasta 320 ms y acciones de hasta 1050 ms, así que un límite demasiado laxo también pasa.

   **Oráculo del límite, en `packages/renderer/test/prepare-from.test.ts`:**
   - manifests válidos en los límites: windup de 400 ms, reacción de 3000 ms y ataque de 3000 ms en total (el tope de `animation_too_long`);
   - timelines generadas con semilla fija, sin `Math.random`, con golpes a 600 ms, huecos cerca de los 3300 a 4300 ms y knockdowns de hasta 3000 ms, avanzando de a un lote y de a varios;
   - `toStrictEqual` contra `prepareScene`;
   - **control negativo:** con esos datos, algún cue de la zona que `H` prohíbe reutilizar difiere entre un prefijo y la timeline completa. Sin eso, el test no demuestra nada.

   **Cuándo alcanza:**
   - Node, con `packages/renderer/scripts/bench-prepare.ts`: `prepareSceneFrom` con 200 cues, p50 ≤ 0,67 ms.
   - Chromium con la CPU a 6×, `?log=draw&live=1`: `extend_ms` con p50 ≤ 4 ms y p95 ≤ 8 ms, tres corridas tras una de calentamiento, y se reporta la peor. `extend_ms` incluye validar y cotejar, a propósito: lo que importa es el bloqueo del frame.

   Si falla, vuelve al architect con los números, antes de pasar a puntos de control de la simulación.

   `prepareScene` sigue igual para `load` y para la primera preparación de `loadLive`.

### Adaptador (RF51)
9. **Un solo núcleo:** `adaptEvents(events, maxHp, fighters, liveMaxDurationMs: number | null)`, con el cuerpo actual. `timelineFromFightLog` lo llama con `null` y conserva firma, salida y códigos de error.
   ```ts
   export type LiveHeader = Readonly<Record<Side, { readonly max_hp: number }>>;
   export function liveTimelineFromEvents(
     header: LiveHeader,
     events: readonly FightEvent[],
     fighters: Readonly<Record<Side, { readonly asset: string }>>,
     maxDurationMs: number,
   ): AdapterResult;
   ```
   - `LiveHeader` es la entrada mínima: `FightLog` y el futuro `FightSummary` encajan por estructura, sin tipos nuevos de `protocol`.
   - En vivo no son errores el `attack` sin resolver **al final**, ni la falta de `end`. Un `attack` sin resolver antes de otro `attack`, de `ko`, de `time_up` o de `end` sigue siendo error.
   - Sin `end`, `duration_ms = maxDurationMs`. Con `end`, `duration_ms = end.t`.
   - Código nuevo `adapter.past_max_duration` para un evento con `t > maxDurationMs`, porque sería un cue inalcanzable. El adaptador sigue sin arreglar nada (RF-C4 de 002).
   - Una lista vacía da `adapter.intro_missing`. El host espera al primer evento antes de llamar (nota para D en la desviación 6).

### Goteo en la arena (RF52)
10. **`lib/features/arena/log-drip.ts`** es la fuente de `?live=1`. Solo importa tipos del adaptador, así que la frontera de `fight-boundary.test.ts` no cambia.
    - `openLogDrip(log, assets, { startElapsedMs, now })` da una sesión `{ clock, timeline, subscribe(push) }`, con un `setInterval` de 50 ms (el tick de RF40) que publica la timeline solo si hay eventos nuevos, y se detiene tras `end`.
    - `clock = elapsed − 3500`.
    - Constantes: `DRIP_TICK_MS = 50`, `PLAYBACK_DELAY_MS = 3500`, y `MAX_DURATION_MS = 90 000`, solo de la arena: en D viene de `FightSummary`.
    - `loadEngineLog(name)` se separa de `loadEngineLogTimeline` en `engine-logs.ts`.
11. **`FightStage.svelte`** suma una prop opcional `live?: (assets) => Promise<LiveSession>`, el punto que D conecta al WS.
    - Con ella, sin `seek`: `loadLive(session.timeline, assets)` y, sin esperarla, `subscribe`, porque `live` se escribe de forma síncrona (decisión 2). `subscribe` entrega al momento la timeline vigente si es más nueva que `session.timeline`, para no perder un `end` publicado mientras se abre o carga. Después, `await`, `onCue` y `play()`. El reloj es `session.clock`. `retime` e `initialT` no se aplican.
    - Cada `extend` aceptado actualiza `loadedTimeline`, para que `currentTimeline()` y `mean_strike_gap_ms` no queden fijos en el primer lote.
    - El `push` respeta `id !== run`.
    - `restart` vuelve a llamar a `start()`, así que "Reiniciar" y "Repetir" reabren el goteo. `toggle` no hace nada. `start()`, el `catch` y el desmontaje cancelan la suscripción.
    - Un `extend` que lanza pasa al estado de error existente (`timeline_invalid`).
12. **`/dev/arena`:** `live` vale solo con `?live=1` exacto y un `?log=` válido. Si no, todo sigue como hoy.
    - `?t=` es el tiempo del server ya transcurrido al abrir, lo que permite probar "entrar tarde" y mantener cortos los e2e.
    - `?pace=` se ignora con `live`, porque `retimeTimeline` necesita la pelea completa.
    - `ArenaControls` suma una prop opcional `live` que deshabilita Reproducir/Pausar, porque pausar rompería `t = clock()`.
    - No hay textos ni tokens nuevos, y tampoco componentes, así que `/dev/components` no cambia.
13. **Medición.** `FightStage` mide con `performance.now()` alrededor de `extend` y lo expone para el JSON de estadísticas como `extend_ms: { count, p50, p95, max }`, sin fila visible, igual que `mean_strike_gap_ms`. El renderer no puede leer `performance` (ESLint), por eso mide el host.

## Archivos que cambian
| Archivo | Cambio |
|---|---|
| `packages/renderer/src/timeline/validate.ts` | modificado: tercer parámetro y `partial` en `checkEndingV2` |
| `packages/renderer/src/timeline/index.ts` | modificado: exporta `ValidateOptions` |
| `packages/renderer/src/scene/prepare.ts` | modificado: `prepareSceneFrom` y núcleo compartido |
| `packages/renderer/src/scene/scene-at.ts` | modificado: `phaseAt` exige cierre para `finished` |
| `packages/renderer/src/scene/index.ts` | modificado: exporta `prepareSceneFrom` |
| `packages/renderer/src/player/types.ts` | modificado: `loadLive` y `extend` en `RendererHandle`. `Stage` sin cambios |
| `packages/renderer/src/player/player.ts` | modificado: decisiones 2 a 7 |
| `packages/renderer/README.md` | modificado: vivo, `extend` y método de medición. "Una vez por carga" deja de ser cierto en `prepareScene` |
| `packages/renderer/test/timeline-partial.test.ts` | nuevo: RF46 |
| `packages/renderer/test/player-live.test.ts` | nuevo: RF47, RF48 y RF50 |
| `packages/renderer/test/prepare-from.test.ts` | nuevo: oráculo del límite `H`, con control negativo |
| `packages/renderer/package.json` | modificado: script `bench:prepare` |
| `packages/renderer/scripts/bench-prepare.ts` | nuevo: 200 cues sintéticos, una llamada cada 25 ms y los dos caminos (`--experimental-strip-types`, como `gen:placeholders`) |
| `apps/web/src/lib/features/fight/timeline-from-fight-log.ts` | modificado: núcleo y `liveTimelineFromEvents` |
| `apps/web/src/lib/features/fight/engine-logs.ts` | modificado: `loadEngineLog` |
| `apps/web/src/lib/features/arena/log-drip.ts` | nuevo: goteo |
| `apps/web/src/lib/features/fight/live-session.ts` | nuevo: solo el tipo `LiveSession`, lo que `FightStage` necesita de una fuente en vivo (el goteo hoy, el WS en D) |
| `apps/web/src/lib/components/fight/FightStage.svelte` | modificado: prop `live` y `extend_ms` |
| `apps/web/src/lib/components/fight/ArenaControls.svelte` | modificado: prop `live` |
| `apps/web/src/lib/components/fight/ArenaStats.svelte` | modificado: `extend_ms` en el contexto del JSON |
| `apps/web/src/routes/dev/arena/+page.svelte` | modificado: lee `?live=1` |
| `apps/web/tests/timeline-from-fight-log.test.ts` | modificado: suma los casos de RF51; los existentes no cambian |
| `apps/web/tests/live-prefix.test.ts` | nuevo: RF49 y el oráculo de la decisión 8 |
| `apps/web/tests/log-drip.test.ts` | nuevo: el goteo con reloj falso |
| `apps/web/e2e/arena.spec.ts` | modificado: RF52 |

`apps/web/tests/fight-boundary.test.ts` **no cambia** (RF51).

## Orden de trabajo
Cada paso es un commit con sus tests en verde. Se trabaja en la rama `ta-13-renderer-live`, con `pnpm install` por corepack antes del primero.

1. **`validate.ts`** y `timeline-partial.test.ts`. *Paralelizable con el paso 3.*
2. **Player y `phaseAt`:** decisiones 2 a 7 con `prepareScene` completo en `extend`, y `player-live.test.ts`. Depende del paso 1.
3. **Adaptador** y sus tests. *Paralelizable con el paso 1.*
4. **`live-prefix.test.ts`, RF49.** Depende de los pasos 1 y 3.
5. **`prepareSceneFrom`**, con los dos oráculos, `bench-prepare.ts` y su medición en Node. Es el paso más riesgoso, y por eso va después de que RF49 ya esté protegido y en un commit propio. El player pasa a usarlo con una sola línea. Depende de los pasos 2 y 4.
6. **`log-drip.ts`, `FightStage`, `ArenaControls`, `ArenaStats`, la página** y `log-drip.test.ts`. Depende de los pasos 2 y 3.
7. **e2e de RF52** y capturas a 375 px y en escritorio, en dark y light, comparadas con la spec. Depende del paso 6.
8. **Medición y documentos.**
   - `just arena-bench`: fps con `?log=draw&live=1` según el README, con el mismo umbral (p95 ≤ 18,2 ms, FPS medio ≥ 58).
   - `extend_ms` en Chromium con la CPU limitada a 6× con `?log=draw&live=1`, el dorado con más eventos: tres corridas tras una de calentamiento, y se reporta la peor, contra los umbrales de la decisión 8.
   - Node con 200 cues, `bench-prepare.ts`, contra su umbral. Se pegan los tres en el PR, junto con Android si el owner lo tiene a mano.
   - README del renderer.
9. **`just check` y `just e2e`**, con la salida en el PR.

## Riesgos
- **`load` cambia sin querer** → `reachedEnd` solo difiere sin cierre, algo que `load` nunca acepta. Las decisiones 5 y 6 exigen `live`. `player.test.ts`, `timeline-v1.test.ts`, `timeline-v2.test.ts` y sus snapshots no se editan.
- **El límite `H` de la preparación incremental es erróneo y una escena difiere** → `prepare-from.test.ts` lo prueba con manifests en los límites y un control negativo. Los dorados no sirven para esto. Además, RF49 corre contra el camino que usa el player.
- **RF49 pasa sin probar nada (test vacuo)** → el mismo test exige que con un retraso de 2499 ms la escena **sí** difiera en `t = cierre`, con `c = end.t − 1`, en cada uno de los 7 dorados. Eso fija además el mínimo de 2500 ms que deduce la spec.
- **Cues perdidos por llamar a `extend` dentro de un listener** → decisión 7, con su test.
- **La pelea queda en `finished` sin `end`** → decisión 4, con un test en el que el reloj pasa de `duration_ms` sin cierre: ni el estado ni la fase son `finished`, y al llegar el `end` terminan.
- **Un lote atrasado dispara una ráfaga de `onCue`** → es lo que ya hace un salto de reloj, según el README, y RF47 prohíbe tocar el cursor. La escena es correcta porque es función pura de `t`. Se anota para D.
- **e2e lentos o inestables** → `?t=` deja cada espera en unos 8,5 s. Las aserciones son sobre `data-*`, no sobre píxeles. WebKit queda fuera de RF52 (preocupación 10): el mismo camino puro ya corre en Vitest.
- **`setInterval` del goteo vivo tras cambiar de luchador** → `start()` y el desmontaje cancelan. El e2e F1 de cambios rápidos se mantiene, y se suma un Vitest de cancelación.

## Alternativas descartadas
- **Preparación completa en cada `extend`.** Es la más simple, y la medición indica que no alcanza en el dispositivo de referencia. Queda como camino hasta el paso 5.
- **Puntos de control de la simulación en `buildChoreography`.** Sería la más rápida, pero exige clonar `Sim` y `out`, con una mutación de `a.x1` de por medio, en el código más delicado del paquete. Solo se considera si la decisión 8 no alcanza.
- **Quitar `deepFreeze` en vivo.** Ahorra 0,44 ms, pero rompe la decisión 2 de plan-a1: el núcleo no distingue modos.
- **Campo `final` o `partial` en la `Timeline`.** Cambia el formato sin necesidad: el cierre ya lo dice. La spec descarta además una v3.
- **Que `extend` reciba solo los cues nuevos.** Se ahorra el cotejo del prefijo, pero RF47 define "empiezan por los anteriores", y un `fight_sync` trae todo.
- **Cola en el host para los lotes durante `loadLive`.** Cada host tendría que reimplementarla. En el player son tres líneas.
- **RF50 también para `load`.** RF47 dice que `load` no cambia.

## Pruebas (cómo se demuestra que funciona)
- [ ] `just check` en verde, con la salida en el PR
- [ ] `just e2e` en verde
- [ ] **RF46:** con todos los fixtures de `fixtures/timeline-v2/`, los errores con `partial` son exactamente los estrictos menos `ending_missing` y `closing_missing`. Todo corte por tiempo de los dorados pasa. Una v1 con `partial` da `unsupported_version`. Una timeline parcial sin `partial` falla como hoy.
- [ ] **RF47, con `FakeStage`:** `extend` no llama a `loadAssets`, no mueve `t` ni el cursor, y actualiza la duración. Rechaza cada caso de la decisión 3 con `timeline_invalid` y deja el estado intacto. Acepta los mismos cues. Durante `loadLive` en curso, la instalación usa la timeline más nueva. `load` reemplaza a un vivo y entonces `extend` lanza. `extend` dentro de un listener no pierde cues.
- [ ] **RF48:** sin cierre no hay `finished`, ni estado ni fase, tampoco con el reloj más allá de 90 000. Con cierre, termina en `end.t`. `snapshot()` no tiene duración.
- [ ] **RF49, en `live-prefix.test.ts` (importa `@torneo/renderer/scene`, que no carga Pixi):** para los 7 dorados, sin y con movimiento reducido, en el corte más fuerte de cada prefijo (`c` = siguiente `t` distinto − 1) y en `c` = cada `t`. Los instantes son `t = c − 3500`, una rejilla en los últimos 4,5 s y una pasada completa cada 10 cortes. `sceneAt` da lo mismo que con el log completo, por `liveTimelineFromEvents`, `validateTimeline` parcial y el camino incremental. Más el caso de 2499 ms que debe diferir. Comparación rápida por JSON y `expect` solo al fallar, con un presupuesto de pocos segundos.
- [ ] **RNF:** `prepareSceneFrom` ≡ `prepareScene` con los dorados y en `prepare-from.test.ts`, con control negativo. Medición en el PR (paso 8), contra los umbrales de la decisión 8. El tiempo no se comprueba en CI, igual que los fps en 002 (RF-A19).
- [ ] **RF50:** `play()` con el reloj en 30 000 no dispara lo anterior y dispara lo siguiente. Un cue justo en `t` sale en el siguiente frame. Con el reloj en −3500, `t = 0`, sin `onCue`, y el `intro` sale al llegar a 0.
- [ ] **RF51:** `liveTimelineFromEvents(log, log.events, …)` es igual a `timelineFromFightLog(log, …)` en los 7 dorados. `attack` al final sin resolver y falta de `end` dan `ok`. Un `attack` sin resolver en medio sigue fallando. Con `t > maxDurationMs` da `past_max_duration`. Los tests existentes y `fight-boundary.test.ts` no cambian.
- [ ] **RF52, e2e en Chromium:**
  - `?log=ko-knockdown&live=1&t=49169`: sin pulsar Reproducir, termina en `finished` con `data-ending="ko"` y los mismos HP que `?log=ko-knockdown&t=99999999999`, sin errores de consola.
  - `?log=draw&live=1&t=85000`: termina en `data-ending="draw"`.
  - `?log=ko-knockdown&live=1`: empieza en `playing` y llega al primer golpe (8 s).
  - `?live=1` sin `?log=`: la arena carga en pausa, como hoy.
- [ ] **Goteo, con Vitest y reloj falso:** ningún evento antes de su `t`, los de mismo `t` van juntos, el último estado es igual a `timelineFromFightLog`, suscribir después de `end` entrega el final, y la suscripción se cancela.
- [ ] Capturas de `?live=1` a 375 px y en escritorio, en dark y light y en es y en, con los controles deshabilitados y sin scroll horizontal
- [ ] `just arena-bench` con `?live=1` en el umbral de fps, con la salida en el PR

## Estimación de CI
Sin workflows nuevos. Vitest de web sube unos segundos por RF49. El e2e sube cerca de 1 min en total: tres pruebas de 8 a 10 s y una breve por cada proyecto de Chromium, y las capturas.

## Desviaciones propuestas de la spec
_Ninguna está aplicada. La spec no se edita desde este plan._

1. **RF46, texto.** Enumera `ko_count` y `win_count` "por falta", pero esos códigos por falta solo existen en `checkEndingV1`, y la v1 no admite `partial`. En la v2 solo aplican `ending_missing` y `closing_missing`. No cambia el comportamiento. **Propuesta:** quitar esos dos del texto.
2. **"Componentes afectados", `packages/renderer`.** Dice `validate.ts`, `player/`, sin cambios en `Stage`. Hay que tocar también `scene/scene-at.ts` (`phaseAt`, para que RF48 valga también en la fase) y `scene/prepare.ts` (RNF). `Stage` sigue sin cambios. **Propuesta:** sumar `scene/` a la tabla.
3. **RF50, reloj negativo.** Pide `t = 0`, y con solo eso el `intro` dispararía `onCue` hasta 3,5 s antes de su hora, en cada cliente según cuándo cargue, contra "todos reproducen el mismo `t`". El plan no dispara cues con el reloj negativo (decisión 6). **Propuesta:** agregar a RF50 "y no dispara cues hasta que el reloj llega a 0".
4. **RF47, precisiones.** Tres puntos que la spec no fija y este plan sí:
   - "Rechaza" en un método síncrono significa lanzar.
   - También se rechaza un cambio de `fighters` o de duración sin cierre, y un `extend` sin `loadLive` vigente.
   - `durationMs` es interno, porque RF48 prohíbe exponerlo.
5. **RNF de CPU, dispositivo de referencia.** No se puede medir hoy (punto 2 de "Para el architect"), y 200 cues no se alcanzan con un dorado real: el mayor (`draw`) da 78 cues, y el máximo teórico con 600 ms de separación ronda los 140 golpes. **Propuesta:** para este PR, Node con 200 cues y Chromium a 6× con un dorado real. La medición en Android queda pendiente junto con la de 002.
6. **RF54 frente a RF50, para el plan D. Es un problema real.** RF54 dice que el resultado se muestra "por el cue de cierre del renderer", pero RF50 no dispara lo saltado. Quien entra con `t` ya pasado del cierre nunca recibe ese cue. Es una situación alcanzable: `t ≥ cierre` ocurre desde `ko.t + 5000` de tiempo del server, y `Settled` llega en `ko.t + 9000`, así que la ventana dura 4 s, y más si la liquidación se atrasa. **Propuesta:** RF54 dice "por `snapshot().ending`", que ya es función de `t` y cubre las dos entradas. Esta fase no necesita cambios: `snapshot()` ya lo da. Otras dos notas para D:
   - `fight_sync` puede traer `events` vacío justo al pasar a `live`, y el host debe esperar al `intro`.
   - Tras un corte largo, `extend` dispara en ráfaga los cues atrasados, así que los efectos que dependan de `onCue` deben tolerarlo.

## Desviaciones durante la implementación
1. **Decisión 8, `hp` y coreografía.** El plan decía que `hp` se recalculaba entero y que `choreography.ts` no cambiaba, y con eso `prepareSceneFrom` daba p50 de 0,66 a 0,68 ms en `bench-prepare.ts`, justo en el umbral de 0,67 ms. Tres cambios, sin efecto en el resultado (los dos oráculos y los snapshots siguen iguales): los `HpStep` de los cues fijos se reutilizan, `afterAction` usa búsqueda binaria en vez de `windups.find`, y `curX` ya no copia el segmento. Queda en p50 de 0,57 a 0,58 ms.
2. **Paso 8, la limitación de CPU no se comporta como un factor 6 en tareas tan cortas.** Los dos umbrales de Chromium se cumplen, pero el p50 limitado sale más bajo que sin limitar, así que ese número no prueba por sí solo el factor. Un bucle de calibración sí se alarga por 6 en la misma página (de 1,9 a 11,6 ms). Corridas de `?log=draw&live=1`, 152 `extend` por corrida, peor de tres tras una de calentamiento:

   | Corrida | GL | `extend_ms` p50 | p95 | máx. |
   |---|---|---|---|---|
   | CPU a 6× | SwiftShader | 0,2 ms | 1,4 ms | 2,3 ms |
   | Sin límite | Metal | 0,4 ms | 0,6 ms | 0,9 ms |

   Como comprobación más exigente, multiplicar por 6 la corrida sin límite da p50 de 2,4 ms y p95 de 3,6 ms, también dentro de los umbrales (p50 ≤ 4 ms y p95 ≤ 8 ms). En Node, `prepareSceneFrom` con 200 cues dio p50 de 0,57 a 0,58 ms (umbral 0,67 ms). El límite hay que ponerlo después de cargar la página.
3. **Paso 8, fps.** No medí con `just arena-bench` a mano, sino con Playwright contra el build de producción, Chromium sin ventana y Metal: FPS medio de 60,0, p95 de 16,7 ms y 0 frames sobre 33 ms en las tres corridas (umbral: p95 ≤ 18,2 ms y FPS medio ≥ 58). No hay medición en un teléfono.
4. **Tras la revisión de QA, el primer `play()` en vivo pinta.** La decisión 5 solo movía el cursor, y quien entraba con la pelea terminada quedaba en `finished` con la escena de `t = 0`, sin resultado. `play()` llama a `render(p, t)` en esa rama, con tests sin `frame()` y e2e con `?t=99999999999`. `load` no cambia.
5. **e2e.** Además de lo previsto, `arena.spec.ts` suma "Reiniciar" en vivo y `?log=…&live=0`. La parte de RF52 "con los mismos HP que `?t=`" comprueba al perdedor en 0, sin comparar los dos HP.

## Enmienda propuesta a la spec
_Redactada por el architect a partir de las desviaciones 1 a 5. **No está aplicada. Se aplica en la fase D, al cierre,** porque `spec.md` recibe en paralelo una enmienda de la fase A en `ta-13-fight-runner`, y va con la confirmación de product-owner para RF50 y el RNF. Lo aplica el plan D, que lo lista en su trabajo de cierre, y con eso queda escrito en la spec el plazo de la medición en Android: antes del primer despliegue público. Hasta entonces, `spec.md` sigue diciendo que `prepareScene` corre en cada `extend`, y lo vigente es esta sección. Cada entrada reemplaza completo el texto citado. El APPROVE de abajo cubre el plan del frontend-engineer, no esta sección, porque el architect no aprueba lo que escribe: RF50 y el RNF cambian una regla o su prueba y los confirma product-owner al aplicarla. RF46, RF47 y la tabla solo precisan lo que ya era cierto._

**RF46** (desviación 1):
> - **RF46. Timeline parcial.** `validateTimeline(json, manifests, { partial: true })` aplica las mismas reglas salvo las que exigen cierre (`ending_missing` y `closing_missing`). La `timeline_version` sigue siendo 2. La v1 no admite `partial`: da `unsupported_version`. → Vitest.

**RF47** (desviación 4):
> - **RF47. API.** `RendererHandle` suma `loadLive(timeline, assets)` y `extend(timeline)`. `extend` es síncrono, no recarga hojas y no toca la reproducción ni el cursor. Rechaza lanzando `RendererError('timeline_invalid')`, sin cambiar nada, si no hay un `loadLive` vigente, si la timeline no pasa RF46, si los cues nuevos no empiezan por los anteriores, si cambia el `asset` o el `max_hp` de un luchador, o si cambia `duration_ms` sin que llegue el cierre o después de que llegó. Actualiza `durationMs`, que es interno de `PreparedScene` y no se expone (RF48). `load` no cambia. → Vitest con un `Stage` falso.

**RF50** (desviación 3):
> - **RF50. Entrar tarde.** `play()` sobre un reloj ya avanzado deja el cursor en `cursorAt(t)`: no dispara `onCue` por lo saltado. Con el reloj negativo (antes de empezar), `t = 0` y no se dispara ningún cue hasta que el reloj llega a 0. → Vitest.

**RNF "CPU"** (desviación 5 y decisión 8):
> - **CPU:** el motor corre una vez por pelea y al reanudar. Con 2000 sockets, un lote cada ~600 ms. `prepareScene` se ejecuta en la primera carga y `extend` usa la preparación incremental (`prepareSceneFrom`, equivalente por oráculo): ≤ 4 ms en p50 con 200 cues en el dispositivo de referencia de 002. Mientras no se pueda medir en ese dispositivo, la fase C lo prueba por aproximación: en Node, `prepareSceneFrom` con 200 cues y p50 ≤ 0,67 ms (4 ms / 6); en Chromium con la CPU limitada a 6×, `/dev/arena?log=draw&live=1` con `extend_ms` en p50 ≤ 4 ms y p95 ≤ 8 ms. La medición en Android sigue pendiente, junto con la de 002, y se hace antes del primer despliegue público.

**"Componentes afectados", fila `packages/renderer`** (desviación 2):
> | `packages/renderer` | `timeline/validate.ts`, `player/`, `scene/` (`prepare.ts`, `scene-at.ts`), sin cambios en `Stage` | C |

### Nota para el plan D (RF54, no forma parte de esta enmienda)
El problema de la desviación 6 es real: RF54 muestra el resultado "por el cue de cierre del renderer", y RF50 no dispara lo saltado, así que quien entra con `t` más allá del cierre nunca recibe ese cue. La ventana va de `ko.t + 5000` a `ko.t + 9000` de tiempo del server, y dura más si la liquidación se atrasa. Esta fase no cambia nada, porque `snapshot().ending` ya es función de `t`. El plan D resuelve y propone la enmienda con este texto de partida:

> El resultado se muestra por `snapshot().ending` del renderer o, sin WebGL, por `t ≥ end.t` en texto, nunca antes por llegar `end`.

El plan D debe decidir además:
- **Cómo se entera el host.** `snapshot()` es una consulta y no avisa. El primer `play()` en vivo ya pinta `t`, así que justo después `snapshot()` da el resultado de quien entra tarde (corregido tras la revisión de QA), pero el cierre de quien ya está mirando no avisa por ahí. `onCue` no sirve para quien entra tarde (ese es el problema), y `onState` solo avisa en `finished`, en `end.t`, 2500 ms después del cierre. Hace falta una lectura periódica u otro mecanismo, con su prueba de entrada dentro de la ventana.
- **`fight_sync` con `events` vacío** justo al pasar a `live`: el host espera al `intro` antes de `loadLive`, porque una lista vacía da `adapter.intro_missing`.
- **Ráfaga de `onCue`** tras un corte largo: `extend` deja vencidos los cues atrasados, así que los efectos que dependan de `onCue` deben tolerarla.
- **`extend` rechazado en `FightStage`, con test.** Hoy no se alcanza, porque el goteo solo publica dorados válidos, y el WS de D es la primera fuente real. Criterio: un test de un `extend` rechazado, tanto en la entrega inmediata de `subscribe` como en una posterior, que compruebe el estado de error y la suscripción cancelada.

## Revisión
### architect — 2026-10-31 — APPROVE (con las correcciones de abajo, antes de la implementación)
- Verifiqué, contra el código de la rama y sin compilar Rust:
  - **RF46 y desviación 1:** en `checkEndingV2`, la falta de cierre solo produce `ending_missing` y `closing_missing`, y `ko_count` y `win_count` por falta solo existen en `checkEndingV1`. `hp_zero_without_ko` mira `cues[i + 1]`, y en los 7 dorados el golpe a 0 HP comparte `t` con su `ko`, así que un corte por tiempo no los separa.
  - **RF48 y desviación 2:** `durationMs` solo se lee en `phaseAt` y en el recorte de `sceneAt`, y el player en los sitios que nombra la decisión 4. Sin tocar `phaseAt`, una pelea sin `end` daría `finished` en 90 000.
  - **RF50 y desviación 3:** `timeAt` recorta a 0 y `dueCues` daría el `intro` por vencido con el reloj negativo. `INITIAL_PLAYBACK` da `t = clock()`, así que las decisiones 5 y 6 caben sin tocar `playback.ts`.
  - **Decisión 7:** `frame` corta hoy con `prepared !== p`, y un `extend` dentro de un listener perdería cues. Los índices son estables, porque solo se agrega al final.
  - **Decisión 8, por separado del plan:** prototipé `prepareSceneFrom` fuera del repo. No hubo diferencias con `prepareScene` en 1329 pares de prefijos de los dorados, ni en 15 012 de timelines generadas con manifests en los límites (windup de 400 ms, acciones de 3000 ms, knockdowns de hasta 3000 ms). `admitFlashes` es voraz hacia adelante, como dice el plan, y `deepFreeze` se detiene en lo congelado.
  - **Medición, en Node 22.23.4, una llamada cada 25 ms** (repite la del plan, 1,21 ms, con 1,18 ms):

    | Caso | `prepareScene`, p50 / p95 | Prototipo incremental, p50 / p95 |
    |---|---|---|
    | 200 cues | 1,18 / 1,65 ms | 0,49 / 0,65 ms |
    | `draw`, 78 cues | 0,55 / 1,17 ms | 0,28 / 0,46 ms |

    Una llamada cada 600 ms, la cadencia real, da con 200 cues 1,37 / 2,99 ms y 0,50 / 1,53 ms. `validateTimeline` suma 0,18 / 0,43 ms con 200 cues y no estaba en la cuenta del plan.
  - **Números del plan:** los `t` de los dorados (fin de `ko-knockdown` en 54 169, primer golpe en 4525, `draw` con 78 cues y fin en 90 000) dan las esperas de 8 a 8,5 s de los e2e. `FightLog` y `FighterCard` encajan en `LiveHeader`. RF51 y `fight-boundary.test.ts` son compatibles con el núcleo único. `?t=`, `debug` y `data-ending` ya existen en la arena.
  - **Riesgo `medium`:** la plantilla dice heredar `high`, pero `process.md` reserva `high` para auth, monedas, seguridad, datos personales y temas legales, y esta fase no toca ninguno. Aceptado: el PR no necesita pase de seguridad.
- Decisiones que me pidió el plan:
  1. **Preparación incremental ahora.** Medir antes no decidiría nada, porque no hay dispositivo de referencia donde medir. Con la completa y 200 cues, el p50 pasa de 4 ms con un factor de solo 3,4, y la incremental queda en 2,9 ms a 6×. La spec prevé esta salida, RF47 no cambia y el costo es acotado. El paso 5 sigue siendo un commit propio, pero **no se salta**.
  2. **Prueba del RNF por aproximación, aceptada,** con el criterio de la corrección 2, que el plan no fijaba. Android sigue pendiente desde 002 y queda con un plazo, en la enmienda.
  3. **Desviaciones 1 a 4, aceptadas; la 5, aceptada con criterio.** Su texto está en "Enmienda propuesta a la spec". **La 6 pasa al plan D** como nota.
- Correcciones que el frontend-engineer incorpora al plan antes de empezar. No requieren otra ronda:
  1. **El oráculo de la decisión 8 no prueba el límite `H`.** La frase "si el límite `H` estuviera mal, este test falla" es falsa con los dorados y los placeholders. Lo comprobé con mutantes: reutilizar todo cue anterior al último (el límite más laxo posible) da 0 diferencias en los dorados, porque los placeholders tienen windups de hasta 320 ms y acciones de hasta 1050 ms. Hay que agregar `packages/renderer/test/prepare-from.test.ts`:
     - manifests válidos en los límites (windup de 400 ms, reacción de 3000 ms, ataque de 3000 ms en total, que es el tope de `animation_too_long`);
     - timelines generadas con semilla fija, sin `Math.random`, con golpes a 600 ms, huecos cerca de los 3300 a 4300 ms y knockdowns de hasta 3000 ms, avanzando de a un lote y de a varios;
     - `toStrictEqual` contra `prepareScene`;
     - un **control negativo**: con esos datos, algún cue de la zona que `H` prohíbe reutilizar difiere entre un prefijo y la timeline completa. Con mis timelines, el mutante `at_ms + 1000 < últimoAt` falla y `at_ms < últimoAt` también; los cortes de los dorados no sirven de control.
  2. **"Alcanza" no está definido.** La decisión 8 y el paso 8 dicen qué hacer si no alcanza, pero no con qué número. Queda así:
     - Node: `prepareSceneFrom` con 200 cues, p50 ≤ 0,67 ms.
     - Chromium a 6×, `?log=draw&live=1`: `extend_ms` con p50 ≤ 4 ms y p95 ≤ 8 ms, tres corridas tras una de calentamiento, y se reporta la peor. `extend_ms` incluye validar y cotejar, así que es más exigente que el RNF, y es a propósito: lo que importa es el bloqueo del frame.
     - Si falla, vuelve al architect con los números, antes de pasar a puntos de control.
  3. **Falta el script de la medición.** Agregar a "Archivos que cambian" `packages/renderer/scripts/bench-prepare.ts` (con `--experimental-strip-types`, como `gen:placeholders`): timeline sintética de 200 cues, una llamada cada 25 ms y los dos caminos. Si no, nadie puede repetir el número ni comparar.
  4. **Carrera en la decisión 11: el `end` se puede perder y la pelea no termina.** El goteo publica solo cuando hay eventos nuevos y se detiene tras `end`. El orden `loadLive`, `onCue`, `play()`, `subscribe` deja sin suscriptor todo lo que salga mientras cargan las hojas, y si en ese tramo sale `end`, no hay otra publicación: queda en `playing` para siempre. Es además el caso para el que existe la decisión 2. El orden pasa a ser:
     - `loadLive(session.timeline, assets)`, y sin esperarla, `subscribe`, porque `live` se escribe de forma síncrona;
     - `subscribe` entrega al momento la timeline vigente si es más nueva que `session.timeline` leída;
     - después, `await`, `onCue` y `play()`.

     Agregar a `log-drip.test.ts` "suscribir después de `end` entrega el final", y a la decisión 11 que el `catch` y `start()` cancelan la suscripción y que el `push` respeta `id !== run`.
  5. **`loadedTimeline` en `FightStage`:** cada `extend` aceptado la actualiza. Si no, `currentTimeline()` y `mean_strike_gap_ms` quedan fijos en el primer lote (decisión 11).
  6. **"`live` vigente" en las decisiones 5 y 6** significa `live?.installed`. Mientras una `loadLive` carga, `prepared` sigue siendo la pelea anterior, y no debe heredar esas reglas.
  7. **Frontmatter:** `depends_on: ["010"]` apunta a sí mismo. Debe ser `["002", "005"]`, las features cuyo código cambia.
