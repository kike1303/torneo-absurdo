---
id: "002"
title: Spike de renderizado de peleas — fase C
type: plan
status: approved
risk: medium
author: frontend-engineer
reviewers: [architect]
approvals:
  - by: architect
    date: 2026-10-27
linear: TA-2
depends_on: ["001", "003"]
created: 2026-10-27
updated: 2026-10-27
---

# Plan: Spike de renderizado de peleas — fase C (desde spec.md 2026-10-27)

> [intent](./intent.md) · [spec](./spec.md) · [plan-a1](./plan-a1.md) · [plan-a2](./plan-a2.md) · [plan-b](./plan-b.md) · [spec de 003](../003-motor-de-pelea/spec.md) · [plan-a de 003](../003-motor-de-pelea/plan-a.md) · ADRs [0007](../../decisions/0007-i18n-desde-cero.md), [0009](../../decisions/0009-ui-design-system.md) (D4)

**Alcance:** RF-C1 a RF-C4, es decir, el adaptador estrecho `FightLog → Timeline` v1, sus pruebas con los dorados del motor y la reproducción de un log del motor en `/[lang]/dev/arena`. Aparte, en un commit propio, van tres observaciones de la revisión del PR #41 (paso 1). Rama `ta-2-fightlog-adapter` desde `main`, con A1 (#36), A2 (#40), B (#41) y la fase A de 003 (#39) ya fusionados. Es un solo PR, y al fusionarlo 002 pasa a `done`.

**No toca** `crates/**`, `packages/protocol-ts/**` (generado), `packages/renderer/src/**`, `apps/server/**`, `infra/**`, `content/**` ni la opción `csp` de `apps/web/vite.config.ts`. El adaptador consume `@torneo/protocol` y `@torneo/renderer/timeline` **solo con `import type`**, como pide RF-C1.

**Lo que ya existe y se reutiliza:**
- de 003: `FightLog`, `FightEvent`, `Side`, `AbilityKind` y `EndReason` de `@torneo/protocol` (ya es dependencia `workspace:*` de `apps/web`), y los siete dorados `crates/engine/tests/golden/log-*.json`, que son exactamente `canonical_json(log)` más un `\n`;
- de A: `validateTimeline`, `TIMELINE_LIMITS` y los manifests de `dummy_a` y `dummy_b`;
- de A2 y B: `FightStage.svelte`, la página `/dev/arena` (`?t=` y `?pace=`) y `arena.spec.ts`.

## Para el architect: lo que tienes que decidir

1. **`timeline_version` 2 (RF-C4): mi recomendación es dejarla fuera de la fase C.** Hoy 3 de los 7 dorados (`log-decision-a`, `log-decision-b` y `log-draw`) terminan sin KO, y v1 no los puede expresar porque exige exactamente un `ko` y un `win`. Con este plan el adaptador los rechaza con `adapter.unsupported_ending`, como acordaron 003 y `docs/architecture.md` (trabajo derivado 6).

   Costo de meter la v2 aquí, estimado contra lo que costaron A1 y A2:
   - **Enmienda de la spec (la escribe el architect) y revisión de game-designer:** hay que decidir cómo se ve una decisión, quién posa y qué pasa en un empate.
   - **`packages/renderer/src`:** tipos y validador con dos versiones (cue `time_up` y un cierre con `winner` que puede ser `null`, más sus reglas), `prepareScene`/`sceneAt` con la coreografía nueva y sus tests de pureza, banners nuevos en `stage` y `fx`, y el estado `finished` sin ganador en el snapshot.
   - **`apps/web`:** mensajes nuevos en `es` y `en` ("tiempo", "decisión", "empate"), `FightStage` con `finished` sin ganador, e2e y capturas.
   - **En total:** unas 2 o 3 veces esta fase, y la fase C dejaría de ser "estrecha".

   Nada lo exige ya. El go/no-go (RF-B9) no lo espera, y `docs/architecture.md` (trabajo derivado 6) solo prohíbe **publicar peleas reales** sin v2, algo que 002 no hace. Propongo abrir un issue en Linear ("Timeline v2: fines sin KO") que bloquee la primera feature que publique peleas, con los tres dorados como casos de aceptación. Así, cuando se diseñe, ya estarán las respuestas del go/no-go sobre el ritmo. Si decides meterla aquí, este plan vuelve a `draft` y la enmienda va primero.

2. **Choque con las reglas de v1: no hay ninguno en los logs de KO.** Lo comprobé antes de escribir el plan con un prototipo desechable del adaptador (en el scratchpad, fuera del repo). Pasé los 7 dorados por la tabla de "Decisiones 2" y después por `validateTimeline`, y los 4 de KO dan `ok: true`. Los 3 sin KO fallan solo con `timeline.ko_count` y `timeline.win_count`. Las reglas que preocupaban, una por una:

   | Regla de v1 | Qué hace el motor (fuente) | Resultado |
   |---|---|---|
   | ≥ 600 ms entre impactos | La invariante de `properties.rs` exige ≥ 600 entre resoluciones en 16 pares × 1000 semillas. En los dorados el mínimo es 700. | Cumple |
   | Primer `strike` ≥ 2000 ms después de `intro` | Primera resolución entre 3825 y 4650 en los dorados, con la invariante ≥ 2000 | Cumple |
   | Windup ≤ 400 ms | El windup simulado (400, 700 o 1100 ms, de `attack.t` a la resolución) **no llega a la timeline**: `strike.at_ms` es el instante del impacto, y el windup que se ve lo pone el manifest. | No aplica (ver el punto 4) |
   | `knockdown` ≤ 100 ms después de un `hit`/`crit` sobre ese luchador | `sim.rs:434-446` emite `knockdown` solo con `hit` y `crit`, en el mismo `t` | Cumple (retraso 0) |
   | `timeline.knockdown_duplicate` | Hay como mucho un `knockdown` por resolución | Cumple |
   | Ningún `strike` durante un `knockdown` | El motor atrasa los relojes de los dos luchadores en `duration_ms` (`sim.rs:445-446`), y la invariante lo comprueba | Cumple |
   | `duration_ms` de 800 a 3000 | Siempre 1500 (`ruleset.rs:157`) | Cumple |
   | HP 0 seguido de su `ko` (`hp_zero_without_ko`) | El `knockdown` exige `def.hp > 0` (`sim.rs:436`), así que nunca se mete entre el golpe final y el `ko`. `low_hp` y `moment` no generan cue. | Cumple |
   | `timeline.ko_without_zero_hp`, y KO por chip de un `block` | El validador solo exige HP 0 antes del `ko`, sin mirar el resultado del `strike`, así que un `block` con `target_hp: 0` seguido de `ko` **es válido en v1**. Ningún dorado lo trae, por eso hay un test sintético. | Cumple, sin enmienda |
   | `win` ≥ 1500 ms después de `ko` y ≥ 2000 ms de pose | `win` cae en `ko.t + 1500` y `end.t = ko.t + end_ms` con `end_ms = 4000` (`ruleset.rs:184`), así que la pose dura 2500 ms | Cumple |
   | ≤ 2000 cues, `max_hp` ≤ 1 000 000 y ≤ 1 h | ≤ 512 eventos y `max_hp: u16`. El fin más tardío es `end.t = 90 000`. | Cumple |

   Si un log real rompe una regla de v1, **el adaptador no corrige ni inventa nada** (RF-C4). La timeline sale tal cual y `load` la rechaza con `timeline_invalid` (en el arena) o el test de RF-C2 falla con el código de `validateTimeline`. La respuesta es una enmienda de la spec, de 003 o de 002 según quién tenga razón, nunca un parche en el adaptador. Hoy no hace falta ninguna.

3. **Desviación de la decisión 6 del plan-a de 003 ("única copia" de los dorados).** Los tests de RF-C2 leen los dorados **en su lugar** con `fs`, sin copia, como pide 003. Para RF-C3, en cambio, **copio un solo dorado** (`log-ko-comeback.json`) dentro de `apps/web`. El motivo es que el arena tiene que empaquetarlo, y `apps/web/Dockerfile` solo copia `apps/web` y `packages` al contexto de build (líneas 25 y 26). Un import desde `crates/` rompería el job `images`. Un test compara la copia byte a byte con el dorado (decisión 5), así que no puede derivar.

4. **Para la v2, no para C (nota para game-designer):** con v1, un `special` (1100 ms simulados) y un `strike` (400 ms) se ven con el mismo windup, porque el que se ve es el del manifest (≤ 400 ms). Además, en un KO por chip el defensor hace `block` y pasa a `ko` en el mismo instante (precedencia 1 de "Coreografía"). Ninguna de las dos cosas rompe v1. Las dejo anotadas como insumo del issue de la v2.

## Decisiones de implementación

1. **Firma.** `apps/web/src/lib/features/fight/timeline-from-fight-log.ts`:
   ```ts
   import type { FightEvent, FightLog, Side } from '@torneo/protocol';
   import type { Timeline } from '@torneo/renderer/timeline';

   export type AdapterErrorCode = `adapter.${string}`;
   export interface AdapterError {
     readonly code: AdapterErrorCode;
     readonly path: string; // JSON pointer into the log, e.g. "/events/12"
     readonly data?: Readonly<Record<string, string | number>>;
   }
   export type IgnoredCounts = Readonly<Record<'low_hp' | 'moment' | 'unknown', number>>;
   export type AdapterResult =
     | { readonly ok: true; readonly timeline: Timeline; readonly ignored: IgnoredCounts }
     | { readonly ok: false; readonly errors: readonly AdapterError[] };

   export function timelineFromFightLog(
     log: FightLog,
     fighters: Readonly<Record<Side, { readonly asset: string }>>,
   ): AdapterResult;
   ```
   - El resultado sigue la forma de `TimelineResult` (`ok` más `timeline` o `errors`) en lugar de un `Result` genérico, porque `apps/web` no tiene uno y es la convención del repo. El "se cuenta" de RF-C1 es `ignored`.
   - `fighters` dice qué asset dibuja a cada lado. Los `slug` del motor (`striker`, `tank`) no son ids de manifest, igual que el guion del spike reasigna sus assets en `FightStage`. `max_hp` sale del log.
   - Es función pura: sin I/O, sin reloj y sin `Math.random`. Un test lo comprueba leyendo el texto del archivo, como el test de `write.ts` de B.
   - **No llama a `validateTimeline`.** RF-C1 solo permite `import type` del renderer, y `load` ya valida. Los tests de RF-C2 sí validan.

2. **Correspondencia** (la de la nota de 003, que coincide con la spec):

   | `FightEvent` | Cue / efecto |
   |---|---|
   | `intro` | `{ at_ms: t, type: 'intro' }` |
   | `attack` | Ningún cue. Guarda `{ attacker, ability, move }`, con `kind` `strike` → `attack_light`, `heavy` → `attack_heavy` y `special` → `special`. |
   | `hit` | `strike` con `result: crit ? 'crit' : 'hit'`, `damage` y `target_hp: hp`. `weak` se descarta. |
   | `block` | `strike` con `result: 'block'`, `damage` y `target_hp: hp` (puede ser 0: KO por chip) |
   | `dodge` | `strike` con `result: 'dodge'`, `damage: 0` y `target_hp` igual al HP que lleva el adaptador |
   | `knockdown` | `{ type: 'knockdown', fighter: side, duration_ms }` |
   | `ko` | `{ type: 'ko', fighter: side }` |
   | `low_hp`, `moment` | Ningún cue. Suman en `ignored`. |
   | `end` con `reason: 'ko'` | `win` del ganador en `ko.t + WIN_DELAY_MS`, y `duration_ms = end.t` |
   | `time_up`, o `end` con `decision` o `draw` | `adapter.unsupported_ending` (ver la decisión del architect 1) |
   | `type` desconocido (una variante futura) | Ningún cue. Suma en `ignored.unknown`. |

   `WIN_DELAY_MS = 1500` es una constante local, porque el adaptador no puede importar valores del renderer. Un test comprueba que es igual a `TIMELINE_LIMITS.winGapMs`. `timeline_version` es el literal `1`, verificado de la misma forma contra `TIMELINE_VERSION`.

3. **Errores de coherencia (`AdapterError`).** Cubren solo lo que la timeline **pierde** y `validateTimeline` ya no puede ver. Lo que v1 sí ve (separaciones, knockdowns, `ko` con HP 0 y pose) queda para el validador, que es la única fuente de esas reglas. Los errores se acumulan hasta 100, como `ErrorSink`, y el siguiente es `adapter.too_many_errors`.

   | Código | Cuándo |
   |---|---|
   | `adapter.intro_missing` | El primer evento no es `intro` en `t = 0` |
   | `adapter.unsorted` | Un `t` menor que el anterior |
   | `adapter.resolution_without_attack` | Un `hit`, `block` o `dodge` sin un `attack` pendiente |
   | `adapter.attack_mismatch` | La resolución tiene otro `attacker` u otra `ability` que el `attack` pendiente |
   | `adapter.attack_without_resolution` | Un `attack` seguido de otro `attack`, o de `ko` o `end`, sin resolución |
   | `adapter.hp_mismatch` | `damage` mayor que el HP previo del objetivo, o `hp ≠ previo − damage` (el HP de `dodge` depende de esta cadena) |
   | `adapter.end_missing` / `adapter.event_after_end` | Falta el `end`, o hay algo después de él |
   | `adapter.unsupported_ending` | `time_up`, o `end.reason` distinto de `ko` |
   | `adapter.outcome_mismatch` | `reason: 'ko'` con cero o más de un `ko`, o con `winner` nulo o igual al lado del `ko` |

   `path` apunta al evento (`/events/<i>`). Ningún código lleva texto (ADR-0007). Validar la forma del JSON del log no le toca al adaptador: hoy recibe fixtures commiteados, y la feature del vivo validará los mensajes. La red de seguridad es `validateTimeline` en `load`.

4. **Dónde viven los tests.** En `apps/web/tests/`, no en `src/`. La regla de ESLint de `src/**` solo deja importar tipos del renderer, y los tests necesitan `validateTimeline` y `TIMELINE_LIMITS` como valores. `tests/` ya lo hace (`renderer-package.test.ts` y `retime.test.ts`).

5. **Fixtures de RF-C2: los dorados en su lugar, con guardas.** `apps/web/tests/support/golden-logs.ts` es el **único** archivo que conoce la ruta: `new URL('../../../../crates/engine/tests/golden/', import.meta.url)`. Exporta `KO_LOGS` (los cuatro de KO), `UNSUPPORTED_LOGS` (`decision-a`, `decision-b` y `draw`) y `readGoldenLog(name): FightLog`. Para que el acoplamiento no se rompa en silencio:
   - un test exige que el conjunto de `log-*.json` del directorio sea **exactamente** `KO_LOGS ∪ UNSUPPORTED_LOGS`. Si 003 agrega, renombra o borra un dorado, el test falla con el nombre, en vez de saltárselo;
   - el filtro `web` de CI ya incluye `crates/engine/**` (`ci.yml:78`), así que un cambio de dorados vuelve a correr estos tests;
   - la copia del arena (`apps/web/src/lib/features/fight/fixtures/log-ko-comeback.json`) se compara **byte a byte** con su dorado. Va en `.prettierignore` y con `-text` en `.gitattributes`, igual que los dorados.

   Descarto `engine-wasm`. Existe, pero no tiene glue JS ni `wasm-bindgen-cli` en la cadena (003 RF19 lo deja fuera), y meterlos es una dependencia nueva para un test.

6. **Qué log reproduce el arena: `log-ko-comeback.json`.** Dura 76,8 s y tiene 3 `knockdown`, 9 `crit`, 11 `block` y 9 `dodge`. Además gana `b`, al revés que el guion del spike, donde gana `a`, así que el e2e distingue una fuente de la otra. Pesa 11 KB y va en un chunk diferido (no entra al resto de la app).

7. **Cómo elige el arena: `?log=<nombre>`, sin controles nuevos.** El patrón es el de `?pace=`. Así no hay strings nuevos: el error de adaptación se muestra con el mensaje existente `timeline_invalid`.
   - `apps/web/src/lib/features/fight/engine-logs.ts` declara `ENGINE_LOGS = { 'ko-comeback': () => import('./fixtures/log-ko-comeback.json') }` y `parseEngineLog(raw: string | null): EngineLogName | null`, con una lista blanca exacta (cualquier otro valor da `null`, y el arena usa el guion).
   - También declara `loadEngineLogTimeline(name, assets): Promise<Timeline>`, que importa el JSON, llama al adaptador y, con `ok: false`, lanza `Object.assign(new Error('timeline_invalid'), { code: 'timeline_invalid' })`, la forma que ya captura `FightStage`. No escribe nada en la consola (ADR-0012: los logs del cliente van por código, y la ruta de dev no los necesita).
   - `FightStage.svelte` recibe la prop nueva `source?: (assets: Readonly<Record<Side, string>>) => Promise<Timeline>`. Sin ella hace lo de hoy: importa el guion y le pone los assets. Con ella, `start()` llama a `source(side)` en el mismo `Promise.all` en que hoy importa el guion. `retime` (`?pace=`) se aplica a las dos fuentes y `load` valida el resultado. Con `?log` y un `pace < 0,86`, la separación mínima de 700 ms baja de 600 y el arena termina en `timeline_invalid`. Lo dejo así porque es el contrato de `?pace` ("nunca forzado").
   - La página (`src/routes/dev/arena/+page.svelte`) lee `?log=` y pasa `source`. Es la única que importa de `#lib/features/fight`, porque los componentes no importan valores de features (ADR-0009 D4) y `FightStage` lo recibe por props. Ningún archivo nuevo importa valores de `@torneo/renderer`: la regla de `import()` (TA-11) no cambia.

8. **Frontera "un solo archivo conoce las dos formas" (RF-C1).** La hace cumplir un test en `apps/web/tests/fight-boundary.test.ts`. Recorre `apps/web/src/**/*.{ts,svelte}` y exige que `timeline-from-fight-log.ts` sea el único archivo que importa a la vez de `@torneo/protocol` y de `@torneo/renderer` (en cualquiera de sus subpaths). Es barato y evita que la forma del log se filtre al arena.

9. **Observaciones de la revisión del PR #41** (paso 1, commit propio `fix(tools): …`):
   - **(a) architect: `local-sprites.ts` toma la carpeta base de `server.config.root`.** El fallback de `configureServer` (`apps/web/vite/local-sprites.ts:161-163`) pasa a una constante exportada `DEFAULT_LOCAL_SPRITES_DIR = fileURLToPath(new URL('../../../content/_local/sprites/', import.meta.url))`, como dice la decisión 14 de plan-b. Así no depende de con qué `root` se arranque Vite. Se corrige el comentario de `LocalSpritesOptions.baseDir`. Test en `tests/local-sprites.test.ts`: la constante es `<repo>/content/_local/sprites` (relativa a `REPO`, como en `vite-config.test.ts`), y `localSprites().configureServer` con un `server.config.root` falso (`/elsewhere`) registra un handler que sirve desde esa constante. El test inyecta `fs` con las opciones existentes y comprueba la ruta pedida.
   - **(b) security: `write.ts` borra `manifest.json` y `sheet-N.png` en cualquier `--out` ignorado.** **Elijo un marcador**, no restringir a `content/_local/sprites/<id>/`: `--out` admite otras carpetas ignoradas (`tools/sff-converter/output/`, que está en `.gitignore`) y los tests usan repos temporales. La regla:
     - `writeOutput` escribe `.sff-converter-output`, un archivo vacío, en la carpeta de salida **antes** que las hojas y el manifest. Si la escritura se corta a la mitad, la carpeta queda marcada y la siguiente corrida puede limpiarla.
     - Antes de tocar el disco, en el paso 3, el marcador pasa también por `git check-ignore`.
     - Si la carpeta existe y contiene algún nombre de `OLD_OUTPUT` **sin** el marcador (con `lstat`: archivo regular, no enlace), falla con `write.foreign_output` sin borrar nada. Un marcador que es un enlace simbólico da `write.symlink`.
     - Una carpeta nueva, vacía o sin salidas viejas se acepta sin marcador.
     - `resolveLocalSpriteRequest` no sirve el marcador, porque su expresión regular solo admite `manifest.json` y `sheet-N.png`.

     El código nuevo va en la tabla de errores del README. Las salidas de B que el owner ya tiene (sin marcador) fallan una vez con `write.foreign_output`, y el README dice que se borre esa carpeta y se vuelva a convertir. Tests en `test/write.test.ts`:
     - una carpeta ignorada con `manifest.json` ajeno da `write.foreign_output`, y el archivo sigue intacto, byte a byte;
     - con el marcador, limpia y reescribe;
     - una carpeta nueva queda con el marcador;
     - un marcador que es un enlace simbólico da `write.symlink`;
     - si el marcador no está ignorado (por una regla `!`), da `write.not_ignored`.
   - **(c) devops: `scripts/check-dockerignore.sh:36` crea un solo nivel de carpeta.** Antes del `mkdir -p "$dir"`, el script sube desde `$dir` y anota en `created_dirs` cada ancestro que falta, del más profundo al más alto (`d=$dir; while [ ! -d "$d" ]; do created_dirs="$created_dirs $d"; d=$(dirname "$d"); done`). `cleanup` ya los recorre en ese orden con `rmdir`, que solo borra carpetas vacías, así que nunca toca una carpeta que existía. Se corrige el comentario ("Only one level…").

## Archivos que cambian

| Archivo | Cambio |
|---|---|
| `apps/web/src/lib/features/fight/timeline-from-fight-log.ts` | nuevo: el adaptador (RF-C1, decisiones 1 a 3) |
| `apps/web/src/lib/features/fight/engine-logs.ts` | nuevo: lista blanca `?log=`, `parseEngineLog` y `loadEngineLogTimeline` (decisión 7) |
| `apps/web/src/lib/features/fight/fixtures/log-ko-comeback.json` | nuevo: copia exacta del dorado (decisiones 3 y 5) |
| `apps/web/src/lib/components/fight/FightStage.svelte` | modificado: prop `source` opcional (decisión 7) |
| `apps/web/src/routes/dev/arena/+page.svelte` | modificado: lee `?log=` y pasa `source` |
| `apps/web/tests/support/golden-logs.ts` | nuevo: la única ruta a los dorados y las listas `KO_LOGS` y `UNSUPPORTED_LOGS` |
| `apps/web/tests/timeline-from-fight-log.test.ts` | nuevo: unidad del adaptador con logs sintéticos |
| `apps/web/tests/fight-log-goldens.test.ts` | nuevo: RF-C2, la guarda del conjunto de dorados y la de la copia |
| `apps/web/tests/fight-boundary.test.ts` | nuevo: frontera de RF-C1 (decisión 8) |
| `apps/web/tests/engine-logs.test.ts` | nuevo: `parseEngineLog` y `loadEngineLogTimeline` (OK y error con `code`) |
| `apps/web/e2e/arena.spec.ts` | modificado: casos `?log=` (RF-C3) |
| `apps/web/e2e/screenshots.spec.ts` | modificado: capturas del arena con `?log=ko-comeback` (375 px y escritorio, `es` y `en`) |
| `.prettierignore`, `.gitattributes` | modificado: `apps/web/src/lib/features/fight/fixtures/` (sin formato y `-text`) |
| `apps/web/vite/local-sprites.ts`, `apps/web/tests/local-sprites.test.ts` | modificado: observación (a) |
| `tools/sff-converter/src/write.ts`, `tools/sff-converter/test/write.test.ts`, `tools/sff-converter/README.md` (los códigos `write.*` solo viven en `write.ts`; el README los lista) | modificado: observación (b) |
| `scripts/check-dockerignore.sh` | modificado: observación (c) |
| `docs/architecture.md` | modificado: el párrafo de "Timeline propia" dice que el adaptador existe, con su ruta, y que da `adapter.unsupported_ending` hasta la v2 |
| `docs/features/002-spike-renderer/spec.md` | modificado: solo `updated` y una línea de trazabilidad (RF-C1 a RF-C4 → este plan). Si el architect acepta la recomendación 1, una nota en RF-C4 apunta al issue de la v2. El texto normativo no cambia. |

No cambian: el `justfile` (los tests nuevos entran por los Vitest de web que ya existen), `package.json` y el lockfile (sin dependencias nuevas), ni los mensajes de Paraglide (sin strings visibles nuevos) ni `/dev/components` (sin componentes nuevos).

## Orden de trabajo
1. **Observaciones del PR #41** (decisión 9), en un commit propio, con un test primero en (a) y (b). **Se puede hacer en paralelo** con los pasos 2 y 3, porque toca otros archivos.
2. **Adaptador y su unidad.** Los tests primero (`timeline-from-fight-log.test.ts`):
   - un log mínimo de KO con todas las variantes da la timeline esperada, comparada en profundidad;
   - KO por chip (`block` con `hp: 0`, después `ko`) da `ok` y pasa `validateTimeline`;
   - un `dodge` toma `target_hp` de la cadena;
   - `low_hp`, `moment` y un `type` desconocido (por cast) solo suman en `ignored`;
   - un fixture por código de la decisión 3, incluidos `time_up` y `end` con `decision` y con `draw`, que dan `adapter.unsupported_ending`;
   - `too_many_errors` con 150 resoluciones huérfanas;
   - `WIN_DELAY_MS` y `timeline_version` coinciden con los valores del renderer;
   - el archivo no menciona `Math.random`, `Date` ni `performance` (test sobre el texto);
   - misma entrada, misma salida.

   Después, la implementación.
3. **RF-C2.** `golden-logs.ts`, la copia `log-ko-comeback.json` (con `cp`, nunca a mano), `.prettierignore` y `.gitattributes`, y `fight-log-goldens.test.ts`:
   - cada uno de `KO_LOGS` da `ok`, y su timeline pasa `validateTimeline(t, manifests)` con `dummy_a` y `dummy_b`;
   - cada uno de `UNSUPPORTED_LOGS` da solo `adapter.unsupported_ending`;
   - el conjunto del directorio es igual a la unión de las dos listas;
   - la copia es byte a byte igual a su dorado;
   - por log de KO, el número de `strike` es igual al de resoluciones del log, y `win.fighter` es igual a `end.winner`.

   Por último, `fight-boundary.test.ts`.
4. **RF-C3.** `engine-logs.ts` con su test, la prop `source` de `FightStage` y `?log=` en la página. En `arena.spec.ts`, para `es` y `en`:
   - `?log=ko-comeback` llega a `playing` sin errores en la consola;
   - `?log=ko-comeback&t=99999999999` llega a `finished`, con `fight-winner` "Gana Muñeco B" o "Muñeco B wins" (según el mensaje existente) y `data-hp-a="0"`;
   - `?log=desconocido` se comporta como el guion del spike (`finished` con "Gana Muñeco A" en `t` grande);
   - `?log=ko-comeback&t=<un knockdown + 200>` en Chromium, con `data-last-cue` igual al índice del cue `knockdown`, calculado en el test con el adaptador sobre la copia.

   Capturas en `screenshots.spec.ts`.
5. **Docs** (`architecture.md`, trazabilidad en la spec y README del convertidor) y la sección "Desviaciones" de este plan si hizo falta.
6. **Verificación.** `just check` y `just e2e`, más la prueba manual de (c) (ver "Pruebas"). Reviso las capturas en móvil y escritorio, en los dos temas, contra "Diseño → UI" de la spec.

## Riesgos
- **Los dorados de 003 cambian (nuevo `ruleset_version`, otras semillas) y RF-C2 se rompe en un PR de 003.** → Eso es lo buscado: el filtro `web` corre estos tests. Si un dorado nuevo rompe una regla de v1, el fallo trae el código de `validateTimeline`, y la respuesta es una enmienda (decisión del architect 2), no un parche. La copia del arena también falla en su guarda y se actualiza con `cp` en el mismo PR de 003.
- **Ruta relativa frágil a `crates/engine/tests/golden/`.** → Vive en un solo archivo, y si el directorio no existe, el test del conjunto falla con un error claro (no da cero casos y pasa).
- **La prop `source` cambia el flujo de `start()` en `FightStage`, que es crítico para el e2e de A2 y B.** → Sin `source` el código es el de hoy, solo extraído a una función, y los e2e de A2 y B siguen igual sin tocarse.
- **El KO por chip se ve raro** (`block` y luego `ko` en el mismo instante). → Válido en v1. Lo anoto para game-designer (decisión del architect 4), sin cambio aquí.
- **El marcador de (b) molesta al owner con sus salidas de B.** → Falla una vez con un código claro, y el README dice qué hacer. No borra nada.
- **(c) borra una carpeta que no creó.** → Solo anota las que faltaban antes del `mkdir -p`, y `rmdir` no borra carpetas con contenido.

## Alternativas descartadas
- **Meter `timeline_version` 2 en esta fase.** Ver la decisión del architect 1: cuesta 2 o 3 veces la fase, necesita diseño de game-designer y nada lo exige antes de publicar peleas.
- **Que el adaptador "arregle" lo que v1 no acepta** (mapear `decision` a un `ko` falso, estirar separaciones o soltar `knockdown`). RF-C4 lo prohíbe, y mentiría sobre la pelea.
- **Llamar a `validateTimeline` dentro del adaptador.** RF-C1 solo permite `import type` del renderer, y duplicaría la validación de `load`.
- **Importar el dorado desde `crates/` en el bundle del arena.** El contexto de Docker no incluye `crates/` (decisión del architect 3). Además, en `vite dev` habría que ampliar `server.fs.allow`, que hoy un test fija solo en los placeholders.
- **Generar los logs con `engine-wasm` en Vitest.** Ver la decisión 5.
- **Copiar los siete dorados a `apps/web`.** Siete copias que mantener, cuando solo una se empaqueta.
- **Un selector "guion / log del motor" en `ArenaControls`.** Necesita mensajes nuevos y una decisión de UI para una ruta de dev. `?log=` alcanza para la demo y el e2e. Si el owner lo quiere visible, es un cambio chico.
- **(b) restringir `--out` a `content/_local/sprites/<id>/`.** Rompe `tools/sff-converter/output/` y los tests con repos temporales, y ata el convertidor a la estructura del repo.

## Pruebas (cómo se demuestra que funciona)
- [ ] `just check` en verde, con la salida pegada en el PR.
- [ ] **RF-C1:** `timeline-from-fight-log.test.ts` (paso 2) y `fight-boundary.test.ts`. ESLint y `svelte-check` sin errores con `import type` únicamente.
- [ ] **RF-C2:** `fight-log-goldens.test.ts`. Los 4 logs de KO pasan `validateTimeline` con los manifests de los placeholders, los 3 sin KO dan `adapter.unsupported_ending`, y pasan las guardas del conjunto y de la copia.
- [ ] **RF-C3:** `just e2e` (Chromium y WebKit) con los casos `?log=` del paso 4, que llegan a `finished` con el ganador `b`.
- [ ] **RF-C4:** en el PR, la lista de campos que produce el adaptador es exactamente la de v1 (el test de forma del paso 2 compara en profundidad), junto con la decisión del architect sobre la v2.
- [ ] Capturas `arena-log` en 375 px y escritorio, en `es` y `en`, con tema oscuro y claro, en `apps/web/test-results/screenshots/`.
- [ ] **Observación (a):** el test nuevo de `local-sprites.test.ts`.
- [ ] **Observación (b):** los cinco casos nuevos de `write.test.ts`.
- [ ] **Observación (c):** `just check-dockerignore` en verde. A mano, en un worktree temporal de esta rama, con `tools/` borrado **solo en ese worktree**, el script pasa y al salir no deja ni `tools/` ni `tools/sff-converter/`. Pego la salida en el PR.

## Estimación de CI
Sin cambios en los workflows. El job `web` suma unos segundos de Vitest. El job `e2e` suma unos 4 tests por motor, alrededor de 20 a 30 s por corrida.

## Desviaciones durante la implementación
_(Se llena si la implementación se aparta del plan.)_

## Revisión
### architect — 2026-10-27 — APPROVE
- Verifiqué:
  - el frontmatter, el alcance contra RF-C1 a RF-C4 y la tabla de correspondencias contra `crates/protocol/src/fight.rs`, la tabla de la spec de 003 y la decisión 6 y la nota de consumo del plan-a de 003;
  - las afirmaciones de la decisión 2 contra el código: el `knockdown` en el mismo `t` que la resolución y solo con `def.hp > 0` (`sim.rs:434-446`), `knockdown_ms = 1500` y `end_ms = 4000` (`ruleset.rs`), y los siete dorados en `crates/engine/tests/golden/`;
  - que `apps/web/Dockerfile` solo copia `apps/web` y `packages` (líneas 25 y 26), que el filtro `web` de `ci.yml` incluye `crates/engine/**`, y que existen los puntos que tocan las tres observaciones: `local-sprites.ts:161-163` y `check-dockerignore.sh:36`;
  - ADR-0001, 0002, 0005 y 0007: el adaptador es puro, sin semilla y sin texto, y ningún código lleva mensaje. ADR-0009 D4: `FightStage` recibe `source` por props y solo la ruta importa valores de `features/fight`. ADR-0012: sin consola en el cliente.
- **Decisión 1 (v2): fuera de 002.** Acepto la recomendación. RF-C4 condiciona la v2 a que "haga falta", 002 no publica peleas y la puerta ya está en el trabajo derivado 6 de `docs/architecture.md`. Enmendé RF-C4 (enmienda técnica del 2026-10-27 en la spec). Condición: antes de fusionar, frontend-engineer abre el issue "Timeline v2: fines sin KO" en Linear, con relación *blocks* hacia la primera feature que publique peleas (o, si todavía no existe, con esa condición en la descripción), los tres dorados como casos de aceptación, el punto 4 de este plan como insumo y game-designer como revisor del diseño. El PR lo enlaza, y `docs/architecture.md` (trabajo derivado 6) cita su ID. Game-designer recibe aviso, pero no tiene que volver a revisar la spec de 002.
- **Decisión 3 (desviación de la "única copia"): aceptada.** Leer en su lugar es lo que pidió 003, y la copia única está justificada por el contexto de Docker. Las guardas (conjunto exacto de `log-*.json`, comparación byte a byte, `-text` y `.prettierignore`) impiden que se desvíe en silencio. Al actualizar `docs/architecture.md`, la línea de "Serialización canónica" (que dice que los `log-*.json` son los fixtures del adaptador) menciona la copia del arena y su guarda.
- **Observaciones del PR #41: bien resueltas.** (a) `import.meta.url` funciona en la config empaquetada por Vite, y el test con un `root` falso cubre la regresión. (b) El marcador es mejor que atar `--out` a una ruta: no borra nada ajeno, y escribirlo primero cubre las corridas cortadas. (c) Basta con anotar los ancestros que faltan y usar `rmdir`.
- No bloqueante (para la implementación):
  1. **La frontera de la decisión 8 contra `engine-logs.ts`.** Ese archivo devuelve `Promise<Timeline>`, y el JSON importado no se puede pasar al adaptador sin hacerle un cast a `FightLog`, porque `type` sale como `string`. Si importa los dos tipos, el test de frontera falla. Resuélvelo sin aflojar el test: tipa el retorno con un tipo que exporte el adaptador (por ejemplo, `Extract<AdapterResult, { ok: true }>['timeline']`), o deja el cast dentro del adaptador.
  2. **Un dato de la tabla de la decisión 2.** El `knockdown` solo sale con `crit` (`Defense::Hit && crit`), no con cualquier `hit`. No cambia el resultado.
  3. En "Archivos que cambian", la fila de la spec ya no aplica: la nota de RF-C4 la escribí yo en esta revisión. En el PR solo queda la línea de trazabilidad, si quieres.
