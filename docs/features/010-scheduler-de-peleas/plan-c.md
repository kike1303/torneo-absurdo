---
id: "010"
title: Scheduler de peleas en vivo, fase C (reproducción parcial en el renderer)
type: plan
status: in-review
risk: medium
author: frontend-engineer
reviewers: [architect]
approvals: []
linear: TA-13
depends_on: ["010"]
created: 2026-10-31
updated: 2026-10-31
---

# Plan: Scheduler de peleas en vivo, fase C (desde spec.md 2026-10-31)

> [intent](./intent.md) · [spec](./spec.md) · [plan-a2 de 002](../002-spike-renderer/plan-a2.md) · [plan-c de 002](../002-spike-renderer/plan-c.md) · [plan de 005](../005-timeline-v2-fines-sin-ko/plan.md) · ADRs [0002](../../decisions/0002-commit-reveal-semilla.md), [0007](../../decisions/0007-i18n-desde-cero.md), [0009](../../decisions/0009-ui-design-system.md)

**Alcance:** RF46 a RF52 y el RNF de `prepareScene` (apartado "CPU"). Es decir, `Timeline` parcial, `loadLive` y `extend`, el adaptador de prefijos y el goteo sin server en `/dev/arena`.

**No toca** `crates/**`, `packages/protocol-ts/**`, `apps/server/**`, `infra/**`, `content/**`, `routes/+page.*` ni `stage/`. No hay tipos nuevos de `protocol`, cliente WS ni dependencia de las fases A y B. Ninguna ruta de usuario usa esto hasta la fase D.

**Riesgo `medium`, no el `high` de la spec:** el riesgo alto de 010 está en el server (semilla, ledger, recuperación). Esta fase corre solo en `/dev/arena`, no maneja secretos ni monedas, y `load` queda igual, con sus tests sin cambios. El paso delicado (preparación incremental) tiene un oráculo exacto.

## Para el architect: lo que tienes que decidir
1. **El RNF de `prepareScene` probablemente no se cumple con la preparación completa, y propongo la incremental desde el principio** (decisión 8). Medí el código actual con 200 cues sintéticos, una llamada cada 25 ms, en un Apple M2 Pro con Node 22.23.4, sin navegador: **p50 de 1,21 ms y p95 de 2,09 ms**. El límite es 4 ms en un Android de gama media. Con un factor de 3 a 6 entre esta máquina y ese Android (supuesto mío, sin medir), serían de 3,6 a 7,3 ms en p50 y de 6,3 a 12,5 ms en p95. La spec prevé esta salida ("Si no, el plan C lo vuelve incremental, sin cambiar RF47"). Si prefieres medir antes, el paso 5 es un commit separado que se puede saltar: el resto del plan no depende de él.
2. **El RNF no se puede comprobar hoy en el dispositivo de referencia.** La medición en Android de gama media sigue pendiente desde 002 (nota del owner en su `## Revisión`). Propongo aceptar en este PR la prueba por aproximación del paso 8 y dejar la medición real como pendiente anotada. Ver desviación 5.
3. **Las desviaciones de la spec** están en su sección. La 1 y la 2 piden ajustar la spec. La 6 afecta al plan D.

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
5. **Entrar tarde, solo en vivo.** En `play()`, con `live` vigente y `playback === INITIAL_PLAYBACK`, `cursor = cursorAt(p.cueTimes, timeAt(...))`. Reanudar tras una pausa no lo mueve.
6. **Reloj negativo, solo en vivo.** `timeAt` ya recorta a `t = 0`, pero `dueCues` daría por vencido el `intro` en `at_ms = 0`, hasta 3,5 s antes de su hora. `frame` pinta y no dispara cues mientras el reloj sea negativo. El `intro` sale al llegar a 0, en todos los clientes a la vez. Ver desviación 3.
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

   **Oráculo:** `prepareSceneFrom(prev, t)` es profundamente igual a `prepareScene(t, manifests)`, para los 7 dorados y todos los cortes, avanzando de a un lote y de a varios. Si el límite `H` estuviera mal, este test falla.

   Costo esperado en la misma máquina: unos 0,5 ms en p50 (estimación, se mide en el paso 5). Si con la aproximación del paso 8 no alcanza, no improviso: la siguiente opción son puntos de control de la simulación, y se trae al architect como desviación.

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
    - Con ella: `loadLive`, `onCue`, `play()` y `subscribe`, sin `seek`. El reloj es `session.clock`. `retime` e `initialT` no se aplican.
    - `restart` vuelve a llamar a `start()`, así que "Reiniciar" y "Repetir" reabren el goteo. `toggle` no hace nada. `start()` y el desmontaje cancelan la suscripción.
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
| `apps/web/src/lib/features/fight/timeline-from-fight-log.ts` | modificado: núcleo y `liveTimelineFromEvents` |
| `apps/web/src/lib/features/fight/engine-logs.ts` | modificado: `loadEngineLog` |
| `apps/web/src/lib/features/arena/log-drip.ts` | nuevo: goteo |
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
5. **`prepareSceneFrom`**, con el oráculo y su medición en Node (misma máquina, mismo método). Es el paso más riesgoso, y por eso va después de que RF49 ya esté protegido y en un commit propio. El player pasa a usarlo con una sola línea. Depende de los pasos 2 y 4.
6. **`log-drip.ts`, `FightStage`, `ArenaControls`, `ArenaStats`, la página** y `log-drip.test.ts`. Depende de los pasos 2 y 3.
7. **e2e de RF52** y capturas a 375 px y en escritorio, en dark y light, comparadas con la spec. Depende del paso 6.
8. **Medición y documentos.**
   - `just arena-bench`: fps con `?log=draw&live=1` según el README, con el mismo umbral (p95 ≤ 18,2 ms, FPS medio ≥ 58).
   - `extend_ms` en Chromium con la CPU limitada a 6× (DevTools) con `?log=draw&live=1`, el dorado con más eventos.
   - Node con 200 cues para el número de la spec. Se pegan los tres en el PR, junto con Android si el owner lo tiene a mano.
   - README del renderer.
9. **`just check` y `just e2e`**, con la salida en el PR.

## Riesgos
- **`load` cambia sin querer** → `reachedEnd` solo difiere sin cierre, algo que `load` nunca acepta. Las decisiones 5 y 6 exigen `live`. `player.test.ts`, `timeline-v1.test.ts`, `timeline-v2.test.ts` y sus snapshots no se editan.
- **El límite `H` de la preparación incremental es erróneo y una escena difiere** → el oráculo compara la estructura completa en todos los cortes. Además, RF49 corre contra el camino que usa el player.
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
- [ ] **RNF:** oráculo `prepareSceneFrom` ≡ `prepareScene`. Medición en el PR (paso 8). El tiempo no se comprueba en CI, igual que los fps en 002 (RF-A19).
- [ ] **RF50:** `play()` con el reloj en 30 000 no dispara lo anterior y dispara lo siguiente. Un cue justo en `t` sale en el siguiente frame. Con el reloj en −3500, `t = 0`, sin `onCue`, y el `intro` sale al llegar a 0.
- [ ] **RF51:** `liveTimelineFromEvents(log, log.events, …)` es igual a `timelineFromFightLog(log, …)` en los 7 dorados. `attack` al final sin resolver y falta de `end` dan `ok`. Un `attack` sin resolver en medio sigue fallando. Con `t > maxDurationMs` da `past_max_duration`. Los tests existentes y `fight-boundary.test.ts` no cambian.
- [ ] **RF52, e2e en Chromium:**
  - `?log=ko-knockdown&live=1&t=49169`: sin pulsar Reproducir, termina en `finished` con `data-ending="ko"` y los mismos HP que `?log=ko-knockdown&t=99999999999`, sin errores de consola.
  - `?log=draw&live=1&t=85000`: termina en `data-ending="draw"`.
  - `?log=ko-knockdown&live=1`: empieza en `playing` y llega al primer golpe (8 s).
  - `?live=1` sin `?log=`: la arena carga en pausa, como hoy.
- [ ] **Goteo, con Vitest y reloj falso:** ningún evento antes de su `t`, los de mismo `t` van juntos, el último estado es igual a `timelineFromFightLog`, y la suscripción se cancela.
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
_(Se llena si la implementación se aparta del plan.)_
