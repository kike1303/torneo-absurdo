---
id: "005"
title: "Timeline v2: fines sin KO"
type: spec
status: approved
risk: medium
author: architect
reviewers: [product-owner, game-designer]
approvals: [{by: product-owner, date: 2026-10-27}, {by: game-designer, date: 2026-10-27}]
linear: TA-19
depends_on: ["002", "003"]
created: 2026-10-27
updated: 2026-10-27
---

# Spec: Timeline v2, fines sin KO

> Intent: [intent.md](./intent.md)

## Resumen
Se agrega la `timeline_version` 2 del renderer, un **superconjunto estricto** de la v1. Suma dos cues de cierre, `time_up` y `draw`, y permite un `win` sin `ko` después de un `time_up`. El validador despacha por versión y trata la v1 exactamente igual que hoy. El adaptador `FightLog → Timeline` emite siempre v2 y traduce los tres finales del motor. El player expone un `ending` con códigos (`ko`, `decision` o `draw`), y el HUD de Svelte muestra el resultado con Paraglide. En el canvas solo se suma el banner `time`, que el host traduce igual que hoy `fight` y `ko`. El arena reproduce los tres dorados sin KO con copias comprobadas byte a byte. No se tocan `crates/protocol`, `packages/protocol-ts`, `crates/engine` ni la CSP.

## Requisitos funcionales

### Formato y validador (`packages/renderer/src/timeline/`)
- **RF1. Tipos v2.** `types.ts` exporta:
  - `TIMELINE_VERSIONS = [1, 2] as const`, `type TimelineVersion = 1 | 2` y `TIMELINE_VERSION = 2` (la última, la que emiten los productores).
  - `CUE_TYPES_V1 = ['intro', 'strike', 'knockdown', 'ko', 'win']` (la lista de hoy) y `CUE_TYPES = [...CUE_TYPES_V1, 'time_up', 'draw']`.
  - `TimeUpCue { at_ms; type: 'time_up' }` y `DrawCue { at_ms; type: 'draw' }`, sin más campos, en la unión `Cue`.
  - `Timeline.timeline_version: TimelineVersion`.
  - `ENDINGS = ['ko', 'decision', 'draw'] as const` y `type Ending`.

  → `tsc` y Vitest (las listas y los tipos coinciden con "Diseño → Timeline v2").
- **RF2. Despacho por versión.** `validateTimeline` lee `timeline_version` y aplica las reglas de esa versión. Con `1` el resultado es **idéntico al de hoy para cualquier entrada**: mismos códigos, mismas rutas y mismo `ok`. En particular, `time_up` y `draw` en una v1 dan `timeline.unknown_cue_type`, como hoy. Con `2` aplica las reglas de RF3. Cualquier otro valor da `timeline.unsupported_version`. El `Timeline` devuelto conserva la versión de la entrada (una v1 sale como v1). → Vitest: todos los fixtures de `test/fixtures/timeline/` dan el mismo resultado que antes (el único que cambia es `unsupported_version.json`, ver "Áreas de preocupación" 4), y el guion `spike.timeline.json` (RF-A8 de 002) pasa sin cambios.
- **RF3. Reglas de la v2.** Todas las reglas de la v1 valen igual (orden, intro, separaciones, HP, knockdowns, `strike_during_knockdown`, `cue_after_ko`, `cue_after_win` y `duration_too_short`, entre otras), salvo las de cierre, que pasan a esta tabla. Cada fila tiene su código ("Diseño → Códigos de error nuevos"):
  1. Toda timeline v2 tiene **exactamente un final**: o un `ko` (KO), o un `time_up` (por tiempo). Si no tiene ninguno: `timeline.ending_missing`. Con los dos: `timeline.ending_conflict`. Con más de un `time_up`: `timeline.time_up_count`. Con más de un `ko`: `timeline.ko_count`, como en v1.
  2. **KO:** las reglas de v1 sin cambios. Después del `ko` viene exactamente un `win` del otro lado, ≥ `winGapMs` (1500) más tarde. Un `draw` después de un `ko` da `timeline.cue_after_ko`.
  3. **Por tiempo:** después del `time_up` solo puede venir **un cue de cierre**, `win` (decisión, de cualquier lado) o `draw` (empate). Cualquier otro cue da `timeline.cue_after_time_up`. El cierre llega ≥ `winGapMs` después del `time_up` (`timeline.win_gap` o `timeline.draw_gap`). Sin cierre: `timeline.closing_missing`. Con `win` y `draw` a la vez: `timeline.ending_conflict`.
  4. Un `win` sin `ko` ni `time_up` previos da `timeline.win_gap` (igual que en v1). Un `draw` sin `time_up` previo da `timeline.draw_without_time_up`, y un segundo `draw`, `timeline.draw_count`. Nada después de un `draw` (`timeline.cue_after_draw`) ni de un `win` (`timeline.cue_after_win`).
  5. **Nadie en el suelo al cerrar:** el `at_ms` del cue de cierre (`win` o `draw`) es ≥ `at_ms + duration_ms` de todo `knockdown` (`timeline.closing_during_knockdown`). En un KO ya se cumple por las reglas de v1. En un fin por tiempo, el motor actual lo cumple (ver "Áreas de preocupación" 3).
  6. **Pose:** `duration_ms ≥ cierre.at_ms + winPoseMs` (2000), con el `win` o con el `draw` (`timeline.duration_too_short`).

  → Vitest: un fixture por código nuevo en `test/fixtures/timeline-v2/` (con su `_valid.json`, una decisión), más una variante válida de empate y otra de KO en v2. El test "covers every rule code" pasa a exigir que la unión de los fixtures de `timeline/` y de `timeline-v2/` cubra todos los códigos.
- **RF4. Una v1 válida, con `timeline_version: 2`, sigue siendo válida** (superconjunto). → Vitest de propiedad: el guion de RF-A8, `_valid.json` y las timelines de los cuatro dorados de KO pasan con las dos versiones, con el mismo `Timeline` salvo la versión.

### Escena y player (`packages/renderer/src/scene/`, `player/`, `stage/`)
- **RF5. Coreografía.** `time_up` y `draw` **no crean segmentos ni holds**: ningún luchador cambia de animación por ellos. El `win` de una decisión crea el mismo hold `win` que en un KO, desde su `at_ms` hasta el final. El perdedor de una decisión y los dos luchadores de un empate siguen las reglas de precedencia de siempre: terminan su acción, vuelven a la marca y quedan en `idle`. No hay animaciones nuevas en el formato de assets (`format_version` 1 sin cambios). → Vitest sobre `sceneAt` con las timelines de los tres dorados: en `duration_ms − 1` y en `duration_ms`, el ganador de la decisión está en `win` (o en su fallback) y el perdedor en `idle`; en el empate, los dos en `idle` y ninguno en `win` ni en `ko`; antes del cierre, la animación de los dos es la misma que con el `time_up` quitado de la timeline.
- **RF6. Banner `time`.** `BannerCode` pasa a `'fight' | 'ko' | 'time'`. El banner `time` aparece en el `at_ms` del `time_up` con el mismo tween que `ko` (instantáneo con movimiento reducido) y sigue hasta el final. `RendererLabels.banners` exige la llave `time`, ya traducida por el host, así que el renderer sigue sin texto (ADR-0007). `stage/banners.ts` la suma a `CODES`. → Vitest (`bannersAt` antes y después del `time_up`, con y sin movimiento reducido) y `tsc`.
- **RF7. Fases y final.** `Phase` pasa a `'intro' | 'fight' | 'ko' | 'time_up' | 'win' | 'draw' | 'finished'`, y `Scene` suma `ending: Ending | null`. `ending` vale `null` hasta el `at_ms` del cue de cierre (`win` o `draw`). Desde ahí vale `'ko'` si hubo `ko`, `'decision'` si hubo `time_up` y `win`, y `'draw'` si hubo `draw`. `winner` sigue igual, y en un empate es `null` hasta el final. `RendererSnapshot` suma `ending`. → Vitest: fases y `ending` en los instantes de cada cue de cierre de las tres variantes; `player.snapshot()` en `finished` da `{ phase: 'finished', winner: null, ending: 'draw' }` con el dorado de empate.
- **RF8. Estadísticas de frames.** `getFrameStats()` mide desde el primer `strike` hasta el cue de cierre (`win` o `draw`), no solo hasta `win`. → Vitest con reloj falso.
- **RF9. Cota de `cueWindow` sin cambios.** `LOOKBACK_MS`, `LOOKAHEAD_MS` y `K = 16` no cambian. El máximo real pasa de 10 a 11 (ver "Diseño → Cota de `cueWindow`"), y el comentario de `K` en `prepare.ts` se actualiza. → El test de `scene.test.ts` que recorre las secuencias de separaciones mínimas suma un `time_up` en el mismo `at_ms` que el último `strike` y su cierre 1500 ms después, y comprueba `≤ 11 ≤ K`.
- **RF10. Pureza y determinismo.** Nada nuevo en el renderer usa `Math.random`, `Date`, `performance` ni `crypto` (lo impide la regla de ESLint existente). Los finales nuevos no usan el PRNG cosmético, porque no tienen partículas, shake ni zoom. Con movimiento reducido, la temporización y las animaciones de los luchadores son las mismas (RF-A14 de 002). → ESLint y el test de movimiento reducido de 002, extendido a la timeline de `log-draw` y a la de `log-decision-a`.

### Adaptador (`apps/web/src/lib/features/fight/`)
- **RF11. Siempre v2.** `timelineFromFightLog` emite `timeline_version: 2` para los tres finales. Un KO produce exactamente los mismos cues que hoy; solo cambia la versión. → Vitest: el `EXPECTED` del test del adaptador cambia solo en `timeline_version`; el test "timeline_version is the renderer value" compara con `TIMELINE_VERSION` (que ahora vale 2).
- **RF12. Correspondencia de los fines por tiempo.**
  - `time_up` → `unresolved()` y un cue `{ at_ms: t, type: 'time_up' }`.
  - `end` con `reason: 'decision'` → exactamente un `time_up` previo, ningún `ko` y `winner !== null`. Emite `win` del `winner` en `time_up.t + CLOSING_DELAY_MS`.
  - `end` con `reason: 'draw'` → exactamente un `time_up` previo, ningún `ko` y `winner === null`. Emite `draw` en `time_up.t + CLOSING_DELAY_MS`.
  - `end` con `reason: 'ko'` → como hoy, y además ningún `time_up`.
  - `duration_ms = end.t`, como hoy. El adaptador no inventa nada más; lo que la v2 puede revisar (separaciones, cierre durante un knockdown, pose) lo revisa `validateTimeline` en `load` (plan-c de 002, decisión 3).

  `CLOSING_DELAY_MS = 1500` se exporta (parámetro P2 de la coreografía). → Vitest con logs sintéticos de decisión y de empate.
- **RF13. Errores del adaptador.**
  - `adapter.outcome_mismatch` cubre ahora cualquier final incoherente con sus eventos: `ko` con `time_up`, `decision` o `draw` con `ko`, sin `time_up` o con dos, `decision` sin ganador, `draw` con ganador, además de los casos de hoy. `data: { kos, time_ups }`.
  - `adapter.unsupported_ending` queda **solo** para un `reason` desconocido (una variante futura del protocolo), con `data: { reason }`. **Ningún log válido del motor lo produce.**
  - Los demás códigos no cambian. Solo códigos, sin texto (ADR-0007).

  → Vitest: los casos `unsupported_ending (time_up and decision)` y `(draw)` del test actual pasan a ser casos válidos. Hay un caso por cada incoherencia de la lista, y uno de `reason` desconocido que da `unsupported_ending`.
- **RF14. Dorados.** `log-decision-a`, `log-decision-b` y `log-draw`, leídos en su lugar desde `crates/engine/tests/golden/`, pasan por el adaptador sin errores, y su `Timeline` pasa `validateTimeline` con los manifests de los placeholders. El `win` es del `winner` del `end` (`a` y `b`), y el empate no tiene `win`. Los cuatro de KO siguen pasando igual. En `apps/web/tests/support/golden-logs.ts`, `UNSUPPORTED_LOGS` pasa a llamarse `TIME_LOGS`. → Vitest (`fight-log-goldens.test.ts`).

### Arena y HUD (`apps/web`)
- **RF15. Logs en el arena.** Los tres dorados se copian con `cp` a `apps/web/src/lib/features/fight/fixtures/`, junto a `log-ko-comeback.json`. `ENGINE_LOGS` pasa a `ko-comeback`, `decision-a`, `decision-b` y `draw`, cada uno en su chunk diferido. `.prettierignore` y `.gitattributes` ya cubren el directorio; solo se actualiza su comentario. → Vitest: (a) cada archivo de `fixtures/` es byte a byte su dorado; (b) el conjunto de archivos de `fixtures/` coincide exactamente con las llaves de `ENGINE_LOGS` (con el prefijo `log-`), así que no hay copias huérfanas ni llaves sin copia; (c) `parseEngineLog` acepta exactamente las cuatro llaves.
- **RF16. Resultado en el HUD de Svelte.** `FightStage.svelte`, en `finished`:
  - si `ending` es `ko`, muestra `arena_winner({ name })`, como hoy (`data-testid="fight-winner"`);
  - si es `decision`, muestra `arena_winner_decision({ name })` (también con `data-testid="fight-winner"`);
  - si es `draw`, muestra `arena_draw()` (`data-testid="fight-draw"`).

  Las tres variantes llevan el botón "repetir" y viven en la región `aria-live="polite"` que ya existe. El host pasa `banners.time = m.arena_banner_time()`. Con `debug`, el contenedor expone `data-ending`. Los mensajes nuevos existen en `messages/es.json` y `messages/en.json` ("Diseño → Textos"). → `check-i18n`, `svelte-check` y e2e (RF18).
- **RF17. `?pace=` con fines por tiempo.** `retimeTimeline` escala `time_up` igual que `ko`, y desplaza `draw` igual que `win`, conservando su separación del `time_up`. La cola después del cierre conserva su largo. → Vitest, para la timeline de cada dorado sin KO: (a) con `pace` en 1.5 y en 0.86 pasa `validateTimeline`; (b) con 0.8 el **único** código es `timeline.strike_gap`.

  > **Enmienda (architect, 2026-10-27, a pedido del plan).** La versión original pedía que 0.8 pasara. No se puede: la separación mínima entre impactos de los siete dorados es 700 ms, y 700 × 0,8 = 560 < `strikeGapMs` (600). Pasa lo mismo hoy con `log-ko-comeback`, y es el comportamiento documentado de `retime.ts` (un factor que rompe una regla termina en `timeline_invalid`, nunca se fuerza). 0.86 es el menor factor de dos decimales que deja 700 ms en ≥ 600. El caso (b) demuestra que `time_up`, `win` y `draw` se reubican bien aunque el ritmo rompa la separación. No cambian `PACE_MIN`, `strikeGapMs` ni el motor.
- **RF18. e2e.** En `arena.spec.ts`, para `es` y `en` y para cada uno de `decision-a`, `decision-b` y `draw`:
  - con `?log=<nombre>&t=99999999999`, llega a `data-state="finished"` y `data-ending` es `decision` o `draw`. En la decisión, `fight-winner` muestra el ganador correcto (`a` en `decision-a`, `b` en `decision-b`). En el empate, `fight-draw` es visible y `fight-winner` no existe;
  - con `?log=<nombre>&t=86500`, `data-last-cue` es el índice del `time_up`;
  - no hay errores en consola.

  `screenshots.spec.ts` suma capturas de los tres en `t = 88500` (banner `time` y pose) a 375×812 y 1280×800. El proyecto `webkit` suma un test de `?log=draw&t=99999999999` que llega a `finished`. Si ese motor no tiene WebGL, el test acepta `data-error="webgl_unavailable"` y se anota como `limited`, igual que `csp.spec.ts` (ver "Áreas de preocupación" 5). → `just e2e`.
- **RF19. Sin regresiones de 002.** Los e2e de `ko-comeback` y del guion no cambian, y `fight-winner` sigue mostrando "Gana Muñeco B" / "Dummy B wins". → `just e2e`.

### Docs
- **RF20.** El PR actualiza:
  - `docs/architecture.md`: la viñeta "Timeline propia" (cues `time_up` y `draw`, v2, sin `unsupported_ending` para logs válidos) y el trabajo derivado 6, que pasa a resuelto por TA-19;
  - `docs/glossary.md`: la entrada *Cue*, con `time_up` y `draw`;
  - la sección "Coreografía de los fines por tiempo" de esta spec, si game-designer cambia valores en su revisión.

  → Revisión del PR.

## Requisitos no funcionales
- **Rendimiento:** solo cliente, sin carga nueva en el VPS. `sceneAt` sigue acotado por `K = 16` (RF9). Cada dorado copiado pesa ~14 KB y va en un chunk diferido, solo en `/dev/arena`. RF-A17 de 002 sigue igual (`check-renderer-split`).
- **i18n (ADR-0007):** el renderer y el adaptador solo manejan códigos (`time`, `ending`, `adapter.*`, `timeline.*`). Los tres textos nuevos salen de Paraglide en es y en, y `FightStage.svelte` está bajo `check-i18n`.
- **Accesibilidad:** el resultado se anuncia en la región `aria-live` que ya existe. El banner `time` es decorativo, igual que `ko`, porque el estado lo lleva el texto del HUD. Con `prefers-reduced-motion`, el banner aparece sin tween y la temporización no cambia.
- **Móvil:** a 375 px, "¡Tiempo!" (8 caracteres) cabe donde ya cabe "¡A pelear!" (10). El texto del resultado está en el flujo del documento y se parte en líneas. Las capturas de RF18 lo comprueban.
- **Seguridad (ADR-0012):** la CSP no se toca. No hay red, workers ni `eval` nuevos.
- **Licencias (ADR-0004):** sin assets nuevos. Los placeholders `dummy_a` y `dummy_b` alcanzan porque no hay animaciones nuevas.

## Diseño

### Componentes afectados
```
packages/renderer/
├─ src/timeline/{types.ts, codes.ts, validate.ts, index.ts}   v2, despacho por versión, códigos
├─ src/scene/{types.ts, prepare.ts, choreography.ts,
│             banners.ts, scene-at.ts}                        time_up/draw sin segmentos, banner
│                                                             time, Phase, ending, comentario de K
├─ src/player/{types.ts, player.ts}                           snapshot.ending, labels.banners.time,
│                                                             ventana de stats
├─ src/stage/banners.ts                                       CODES + 'time'
└─ test/ (timeline-v2 fixtures, scene, choreography, player, reduced-motion, frame-stats)
apps/web/
├─ src/lib/features/fight/timeline-from-fight-log.ts          RF11 a RF13
├─ src/lib/features/fight/engine-logs.ts                      RF15
├─ src/lib/features/fight/fixtures/log-{decision-a,decision-b,draw}.json   copias (cp)
├─ src/lib/features/arena/retime.ts                           RF17
├─ src/lib/components/fight/FightStage.svelte                 RF16
├─ messages/{es,en}.json                                      3 mensajes
├─ tests/{timeline-from-fight-log, fight-log-goldens, engine-logs, retime}.test.ts,
│  tests/support/golden-logs.ts
└─ e2e/{arena, screenshots}.spec.ts
docs/architecture.md · docs/glossary.md
```
No se tocan `crates/**`, `packages/protocol-ts/**`, `apps/server/**`, `infra/**`, `content/**`, la CSP de `vite.config.ts` ni el formato de assets. `packages/renderer/fixtures/spike.timeline.json` tampoco cambia: sigue en v1.

### Modelo de datos / tipos (protocol)
`crates/protocol` y `packages/protocol-ts` **no cambian**. La v2 es un formato interno del renderer, que se consume del lado de `apps/web` (RF-C1 de 002).

### Timeline v2
```jsonc
{
  "timeline_version": 2,
  "duration_ms": 90000,
  "fighters": { "a": { "asset": "dummy_a", "max_hp": 860 }, "b": { "asset": "dummy_b", "max_hp": 900 } },
  "cues": [
    { "at_ms": 0,     "type": "intro" },
    { "at_ms": 3400,  "type": "strike", "actor": "a", "move": "attack_light", "result": "hit", "damage": 12, "target_hp": 888 },
    // …
    { "at_ms": 85218, "type": "strike", "actor": "b", "move": "attack_light", "result": "hit", "damage": 13, "target_hp": 476 },
    { "at_ms": 86000, "type": "time_up" },
    { "at_ms": 87500, "type": "win", "fighter": "a" }      // empate: { "at_ms": 87500, "type": "draw" }
  ]
}
```
Por qué un cue `draw` y no "ningún `win`": el empate necesita un instante explícito, para la fase `draw`, el `ending` del snapshot, el fin de la ventana de estadísticas (RF8) y la regla de pose. Además, así el validador distingue un empate de una timeline truncada (`closing_missing`).

La v2 es un **superconjunto estricto** de la v1: toda v1 válida es una v2 válida cambiando solo la versión (RF4), y los cues nuevos solo aparecen en los fines por tiempo.

### Códigos de error nuevos (`TIMELINE_CODES`, solo v2)
| Código | Cuándo |
|---|---|
| `timeline.ending_missing` | ni `ko` ni `time_up` |
| `timeline.ending_conflict` | `ko` y `time_up`, o `win` y `draw` en la misma timeline |
| `timeline.time_up_count` | más de un `time_up` |
| `timeline.cue_after_time_up` | después del `time_up`, un cue que no es el único cierre (`win` o `draw`) |
| `timeline.closing_missing` | `time_up` sin `win` ni `draw` |
| `timeline.draw_without_time_up` | `draw` sin `time_up` previo |
| `timeline.draw_count` | más de un `draw` |
| `timeline.draw_gap` | `draw` < `time_up.at_ms + winGapMs` |
| `timeline.cue_after_draw` | cualquier cue después de `draw` |
| `timeline.closing_during_knockdown` | cierre antes del fin de un `knockdown` |

Se reutilizan con el mismo significado: `win_gap` (también de `time_up` a `win`), `ko_count`, `win_count`, `win_side` (solo en KO), `cue_after_ko`, `cue_after_win` y `duration_too_short` (también para `draw`). En v2, `ko_count` y `win_count` **no** se emiten "por ausencia" en `/cues`; esos casos son `ending_missing` y `closing_missing`. En v1 se emiten como hoy. Los errores del handle (`RendererErrorCode`) no cambian: una v2 inválida sigue siendo `timeline_invalid`.

### Implementación del despacho
`checkCues` recibe la versión. Las reglas comunes no cambian. Las de cierre se separan en dos funciones, `checkEndingV1` (el código actual de `ko`/`win`, movido sin cambios) y `checkEndingV2`. `parseCue` usa `CUE_TYPES_V1` o `CUE_TYPES` según la versión. Una sola pasada y un solo `ErrorSink`, como hoy.

### Coreografía de los fines por tiempo (propuesta para game-designer)
> **Sección delimitada para game-designer.** Los valores son una propuesta concreta. game-designer los confirma o los ajusta en su revisión, y si cambia alguno, el autor actualiza esta tabla antes de aprobar (RF20). El resto de la spec no depende de los números, solo de los límites de la columna "Rango válido".

| # | Parámetro | Propuesta | Rango válido (por qué) |
|---|---|---|---|
| P1 | Campana | El banner `time` aparece en el `time_up` del log (t = 86 000 en los dorados) con el tween de `ko` (200 ms) y sigue hasta el final. **Sin** shake, zoom, partículas ni destello: la campana es el banner. | Cualquier efecto de RF-A12 de 002 vale si respeta movimiento reducido (sin efecto). |
| P2 | Pausa campana → cierre (`CLOSING_DELAY_MS`) | **1500 ms**, igual que KO → victoria. Cierre en t = 87 500. | 1500 ≤ P2 ≤ `end_ms − 2000` = 2000 (mínimo `winGapMs`, y la pose de P3 tiene que caber en `end.t`). |
| P3 | Pose de victoria (decisión) | El ganador sostiene `win` desde el cierre hasta `end.t`: **2500 ms**, como en KO. | ≥ 2000 (`winPoseMs`). Sale de `end_ms − P2`. |
| P4 | Perdedor (decisión) | Termina su acción, vuelve a su marca y queda en `idle`. No cae ni hace `hit`. | `idle`, o una animación opcional nueva con fallback (fuera de esta feature, ver "Alternativas"). |
| P5 | Empate | Los dos terminan su acción, vuelven a su marca y quedan en `idle`. Ninguno hace `win`. El banner `time` sigue. | Igual que P4. |
| P6 | Luchadores en la campana | El `time_up` **no interrumpe** nada: la acción en curso termina con las reglas de precedencia (≤ 3300 ms después del último impacto, que llega antes del `time_up` o en el mismo instante). | El ganador puede estar en recuperación o en reacción cuando llega el cierre. Entonces `win` interrumpe, igual que hoy en KO. |
| P7 | Texto del resultado (HUD de Svelte) | En `finished`, igual que el ganador de un KO: "Gana {name} por decisión" o "Empate". Sin porcentajes: las barras de HP ya muestran la diferencia. | Mostrarlo desde el cierre en lugar de en `finished` es un cambio solo de `FightStage` (`snapshot.ending` ya existe desde el cierre). |

Con P2 = 1500 y `end_ms = 4000`, los tres dorados quedan así: `time_up` en 86 000, cierre en 87 500 y `end` en 90 000, con 2500 ms de pose o de cierre. Entran en los 90 s de 002 (nota 1 del architect en el intent). RF14 lo comprueba con `duration_too_short`.

### Cota de `cueWindow`
`time_up` y `draw` no tienen segmentos, y los holds (`win`) se leen de `PreparedFighter.hold`, no de la ventana. Así, `LOOKBACK_MS = MAX_ACTION_MS + RETURN_MS` sigue alcanzando a todo segmento activo y no hay que cambiarlo (nota 4 del architect). Lo que sí crece es el número de cues en la ventana cerrada de 3700 ms:
- **Ventana sin el cierre:** el caso más denso de v1 (6 `strike`, 4 con `knockdown`, 10 cues) más un `time_up` en el mismo `at_ms` que el último `strike` da **11**.
- **Ventana con `time_up` y cierre:** entre los dos hay ≥ 1500 ms sin cues, así que quedan ≤ 2200 ms para golpes. Caben ≤ 4 `strike` y ≤ 2 `knockdown`, y con el `time_up` y el cierre suman ≤ 8.

El máximo pasa de 10 a 11, por debajo de `K = 16`, que no cambia. RF9 lo prueba.

### Flujo
```
FightLog (golden) ─▶ timelineFromFightLog ─▶ Timeline v2 ─▶ handle.load ─▶ validateTimeline (v1 | v2)
                       (time_up → cue,                                     └▶ prepareScene ─▶ sceneAt
                        end → win | draw en time_up.t + 1500)                    │ phase, winner, ending,
                                                                                 │ banners [time]
                                              FightStage ◀── onState / snapshot ─┘
                                              └▶ HUD: arena_winner | arena_winner_decision | arena_draw
```

### Textos (Paraglide, `apps/web/messages/`)
| Llave | es | en |
|---|---|---|
| `arena_banner_time` | ¡Tiempo! | Time! |
| `arena_winner_decision` | Gana {name} por decisión | {name} wins by decision |
| `arena_draw` | Empate | Draw |

`arena_winner` (KO) no cambia. Son mensajes `arena_*` porque hoy solo los usa la ruta de prueba. La página pública de pelea decidirá sus propias llaves cuando exista.

### UI
- `/[lang]/dev/arena?log=decision-a|decision-b|draw`: mismo layout que `?log=ko-comeback`. Estados: `loading`, `error`, `playing`, `paused` y `finished`, este último con el texto de RF16 y "repetir". En el canvas, el banner `time` desde la campana hasta el final.
- Móvil (375 px) y escritorio: sin cambios de layout. El texto del resultado se parte en líneas si no cabe.

## Áreas de preocupación
1. **Texto del cierre: canvas o HUD (nota 6 del architect).** La nota prefería todo el texto en el HUD de Svelte. La spec la sigue para el **resultado** (`decision`/`draw`, con el nombre), que sale de `snapshot.ending` en Svelte. Para la **campana** se aparta de la nota: usa un banner en el canvas, `time`, con texto que pasa el host. Así el fin por tiempo se ve igual que el KO, que ya muestra su banner `ko` en el canvas con `labels.banners` (RF-A13 de 002). El renderer no contiene texto en ninguno de los dos casos (ADR-0007), y el cambio de contrato es una llave más en un `Record` que TypeScript exige al host. La alternativa (un overlay de Svelte sobre el canvas) duplicaría el mecanismo de banners y su tween, y además no seguiría a `seek` sin lógica nueva en el host.
2. **El resultado de una decisión no se verifica en el cliente.** El adaptador comprueba que `decision` tenga ganador y `draw` no, pero **no** recalcula la regla del motor (`hp·1000/max_hp`, RF9 de 003). Si lo hiciera, `apps/web` duplicaría una regla del `ruleset` y se rompería en silencio cuando cambie. Verificar el resultado es tarea de `engine-wasm` al repetir la simulación con la semilla revelada (ADR-0002), no del adaptador. Un log adulterado con el ganador "equivocado" se reproduciría con barras que lo contradicen. En los dorados, el ganador tiene el mayor HP relativo (`decision-a`: 476/860 frente a 218/900).
3. **`closing_during_knockdown` y el `ruleset`.** Con `knockdown_ms = 1500` y el último impacto en t ≤ `time_limit_ms` (RF8 de 003), un `knockdown` termina como mucho en 87 500, justo en el cierre propuesto. Si game-designer sube `knockdown_ms` por encima de P2 en un `ruleset` futuro, una pelea por tiempo con un knockdown en el último golpe daría `timeline_invalid`. La regla es a propósito: nadie recibe la victoria en el suelo. La salida sería subir P2 dentro de su rango o acotar `knockdown_ms ≤ P2` en el test de cotas del `ruleset` de 003. Lo dejo escrito para el próximo cambio de `ruleset`. Hoy no pasa: los tres dorados no tienen knockdowns.
4. **Fixtures que cambian sin aflojar nada.**
   - `test/fixtures/timeline/unsupported_version.json` hoy pone `timeline_version: 2` para provocar el error. Con la v2 soportada pasa a `3`. Es un cambio de contrato, no un test aflojado.
   - En el test del adaptador, `EXPECTED.timeline_version` pasa a `2`, y los dos casos `unsupported_ending` pasan a válidos (es el objetivo de la feature).
   - En `golden-logs.ts`, `UNSUPPORTED_LOGS` pasa a `TIME_LOGS`.

   El guion de RF-A8 y los demás fixtures de v1 **no** cambian (criterio del intent). El PR lo lista en su descripción para que qa-engineer lo compruebe.
5. **WebKit sin WebGL.** El proyecto `webkit` de Playwright hoy solo corre `theme.spec.ts` y `csp.spec.ts`, porque en Linux WebKit puede no tener WebGL (preocupación 6 de 002). El criterio del intent ("Chromium y WebKit") se cumple en Chromium con todo RF18. En WebKit se cumple con un test de `finished` si el motor tiene WebGL, y si no, queda anotado como `limited` y se repite a mano en Safari, con el resultado en el PR. No se fuerza WebGL por software en WebKit.
6. **El windup de `special` y el KO por chip (nota 4 del plan-c de 002).** Son temas de coreografía de golpes, no de fines por tiempo. Quedan fuera de esta feature y siguen como insumo de game-designer para la siguiente versión del formato.
7. **Copias de los dorados.** Ahora son cuatro las copias en `apps/web`, apartándose de la "única copia" de 003 por la misma razón que en 002: el contexto de Docker de `apps/web` no incluye `crates/`. El test (a) de RF15 impide que deriven, y el (b), que se acumulen copias sin uso. Si `just gen` o el motor regeneran un dorado, el test falla hasta repetir el `cp`.

## Respuestas a preguntas abiertas del intent
- **P: ¿Se explica la decisión?** → R: propuesta P7: no se explica con números. Basta con "por decisión" y las barras de HP. Lo confirma game-designer.
- **P: ¿Qué hacen los luchadores en un empate?** → R: propuesta P5: los dos en `idle`, sin animación nueva y con el banner `time`. Una pose opcional queda en "Alternativas". Lo confirma game-designer.
- **P: ¿v2 siempre o solo en fines por tiempo?** → R: **siempre v2** (RF11), porque la v2 es un superconjunto estricto (RF4) y así hay un solo camino en producción. La v1 queda para el guion de RF-A8 y los fixtures. El KO se ve igual porque la escena no depende de la versión.
- **P: Duración en pantalla.** → R: cabe. `time_up` en 86 000, cierre en 87 500 y fin en 90 000, con 2500 ms de pose, dentro de los 90 s de 002. RF14 lo prueba con los dorados.
- **P: Logs en el arena.** → R: copias con `cp` en `fixtures/`, comprobadas byte a byte y con un test de conjunto exacto contra `ENGINE_LOGS` (RF15).
- **P: ¿Dónde se dibuja el texto del cierre?** → R: el resultado, en el HUD de Svelte a partir de `snapshot.ending`; la campana, como banner `time` del canvas con texto del host ("Áreas de preocupación" 1).
- **Notas del architect en la revisión del intent:**
  1. Duración → "Coreografía", P2 y P3, y RF14.
  2. Reglas de pose → RF3.3 y RF3.6: de `time_up` al cierre ≥ 1500 ms y pose ≥ 2000 ms, revisadas en `checkEndingV2`.
  3. Versionado → RF2, RF4 y RF11.
  4. `cueWindow` → RF9 y "Cota de `cueWindow`".
  5. Arena → RF15.
  6. Texto → RF16 y "Áreas de preocupación" 1.
  7. `game-designer` está en `reviewers`.

## Estrategia de pruebas
- **Unidad (Vitest, `packages/renderer`):**
  - fixtures de reglas v2 (un archivo por código nuevo, con su `_valid.json`) y cobertura de todos los códigos;
  - igualdad de resultados v1 antes y después en todos los fixtures existentes;
  - propiedad de superconjunto (RF4);
  - `sceneAt`, fases, `ending` y banners en los instantes de cierre (RF5 a RF7);
  - `snapshot()` en `finished` con el empate;
  - ventana de stats (RF8);
  - cota de `cueWindow` con `time_up` (RF9);
  - movimiento reducido con las timelines por tiempo (RF10).
- **Unidad (Vitest, `apps/web`):**
  - adaptador con logs sintéticos de decisión y empate, cada incoherencia de RF13 y un `reason` desconocido;
  - dorados en su lugar (RF14);
  - copias byte a byte y conjunto exacto (RF15);
  - `retime` (RF17);
  - frontera de un solo archivo (`fight-boundary.test.ts`, sin cambios).
- **Estático:** `tsc`, `svelte-check`, ESLint (incluida la regla de pureza del renderer), `check-i18n` y `check-renderer-split`.
- **e2e (Playwright):** RF18 en Chromium (es y en, los tres logs, `finished` y `time_up`); el test de WebKit con la salvedad de "Áreas de preocupación" 5; capturas; y RF19 sin regresiones.
- **Revisión visual:** game-designer y el owner revisan las capturas de RF18 en el PR. El go/no-go visual (TA-20) queda fuera.

## Alternativas consideradas
- **v1 para KO y v2 solo para fines por tiempo.** Descartada: tendría dos caminos de emisión y de prueba sin ningún beneficio, porque la v2 acepta todo lo que acepta la v1.
- **Un campo raíz `ending` en lugar de cues `time_up` y `draw`.** Descartada: la escena y `onCue` trabajan por cues con `at_ms`, y la campana y el cierre son instantes. Un campo raíz obligaría a duplicar los instantes.
- **Animación opcional nueva (`draw`/`lose` con fallback a `idle`).** Pospuesta: necesita un código nuevo en `OPTIONAL_ANIMATIONS`, que un renderer viejo rechaza en un manifest nuevo, y regenerar los placeholders. Como el empate es < 1 % de las peleas (RF17 de 003), no vale abrir el formato de assets ahora. Si game-designer la pide, entra con el cue `taunt` en una `format_version` nueva.
- **Texto de la campana en Svelte, sobre el canvas.** Descartada en "Áreas de preocupación" 1.
- **Recalcular la decisión en el adaptador.** Descartada en "Áreas de preocupación" 2.
- **Interrumpir a los luchadores en la campana (todos a `idle` en el `time_up`).** Descartada: un corte en seco se ve como un fallo, y la precedencia existente ya los devuelve a la marca de forma natural.

## Revisión
### product-owner — 2026-10-27 — APPROVE
- Verifiqué: resuelve el problema del intent. Los tres finales del motor se reproducen (RF11 a RF14), `adapter.unsupported_ending` deja de salir para logs válidos (RF13) y el KO se ve igual que hoy (RF11, RF19). Cada criterio de éxito del intent tiene su RF y su prueba: dorados (RF14), arena hasta `finished` (RF18), ganador correcto y empate sin pose (RF5, RF7, RF18), v1 intacta (RF2, RF4), i18n (RF16, "Textos"), aprobación de game-designer (sección delimitada). Las seis preguntas abiertas están respondidas; las dos de coreografía quedan explícitamente a confirmar por game-designer. Respeta el alcance: sin cambios en motor, protocolo ni formato de assets, la animación de empate se pospone, y las apuestas del empate, el narrador, `taunt` y TA-20 siguen fuera. ADR-0004 (solo placeholders), ADR-0005 (no toca monedas) y ADR-0007 (solo códigos; texto por Paraglide en es y en) se cumplen. Para el espectador: campana, pausa y "Gana X por decisión" o "Empate" se entienden sin números, y caben en 375 px.
- Notas (no bloquean):
  1. "Requisitos no funcionales → i18n" habla de "cuatro textos nuevos", pero la tabla de "Textos" y RF16 tienen tres llaves. Corregir la cifra.
  2. WebKit queda como `limited` si no hay WebGL ("Áreas de preocupación" 5). Lo acepto si el PR deja el resultado de la prueba manual en Safari: iOS es buena parte del público móvil.
  3. Para cuando exista la página pública: conviene mostrar el resultado desde el cierre (P7 ya lo deja como cambio solo de `FightStage`), no 2,5 s después. El apostador quiere saber cuanto antes. No es de esta feature.

### game-designer — 2026-10-27 — APPROVE
- Verifiqué: la coreografía P1 a P7 contra los dorados de `crates/engine/tests/golden/`. `decision-a` tiene su último impacto en 85 218 y `draw` en 84 808, el `time_up` en 86 000 y el `end` en 90 000. Con P2 = 1500 el cierre cae en 87 500 y quedan 2500 ms de pose (≥ `winPoseMs` 2000). Por P6, la acción en curso termina como mucho en 85 218 + 3300 = 88 518, antes del `end`: en el empate los dos están en `idle` a tiempo para la captura final, y en la decisión el `win` interrumpe al ganador como en KO. Sin knockdowns en los tres dorados, así que `closing_during_knockdown` no se dispara hoy.
- Confirmo los valores sin cambios: P1 (banner `time`, tween de 200 ms, sin shake, zoom, partículas ni destello), P2 = 1500 ms, P3 = 2500 ms, P4 y P5 en `idle` y P7 en `finished`. Que la campana → cierre dure lo mismo que KO → victoria da un ritmo único para los tres finales. Un fin por tiempo sin efectos se lee como "se acabó el reloj", no como un golpe final, y ese contraste con el KO es lo que buscamos.
- Textos: "¡Tiempo!"/"Time!" siguen el registro de "¡A pelear!"/"Fight!" y "K.O." (términos genéricos del género, sin frases icónicas de nadie, ADR-0004). "Gana {name} por decisión"/"{name} wins by decision" y "Empate"/"Draw" son sobrios como `arena_winner`, y está bien: es la arena de prueba, y el humor absurdo va en el narrador (ADR-0010) y en la página pública. En inglés no es traducción literal forzada. Nada se revela antes de tiempo: el banner sale en el `time_up` del log y el resultado en `finished` (con P7 sin cambios). El HP de las barras ya es público en cada instante.
- Notas (no bloquean):
  1. Con `ruleset` futuro: si `knockdown_ms` sube por encima de 1500, prefiero acotarlo a `knockdown_ms ≤ P2` en el test de cotas de 003 antes que alargar P2. Una pausa más larga tras la campana se siente muerta. Coincide con "Áreas de preocupación" 3.
  2. En el empate son 4 s de dos `idle` con el banner. Para la arena alcanza. En la página pública, el narrador debería llenar ese silencio (ADR-0010), y la pose `draw` opcional queda para cuando entre `taunt`.
  3. Apoyo la nota 3 de product-owner: en la página pública, el resultado desde el cierre (87 500).
