---
id: "002"
title: Spike de renderizado de peleas — fase A2
type: plan
status: done
risk: medium
author: frontend-engineer
reviewers: [architect]
approvals:
  - by: architect
    date: 2026-10-27
linear: TA-2
depends_on: ["001"]
created: 2026-10-26
updated: 2026-10-27
---

# Plan: Spike de renderizado de peleas — fase A2 (desde spec.md 2026-10-26, con su enmienda técnica)

> [intent](./intent.md) · [spec](./spec.md) · [plan-a1](./plan-a1.md) · ADRs [0003](../../decisions/0003-stack-tecnologico.md), [0007](../../decisions/0007-i18n-desde-cero.md), [0009](../../decisions/0009-ui-design-system.md) (D4 a D8), [0012](../../decisions/0012-logs-y-cabeceras-de-seguridad.md) (B3, B4.2, B6 y trabajo derivado 9)

**Alcance:** RF-A11 a RF-A20, sobre el núcleo puro de A1 (PR #36). La capa Pixi (`src/stage/`) que aplica `sceneAt`, un reproductor puro con reloj inyectado (`src/player/`), las estadísticas de frames, la ruta `/[lang]/dev/arena` cargada de forma diferida, el chequeo de la división de chunks, el test de CSP en Chromium y WebKit, y el método de medición de fps. También cierra dos detalles de la revisión de #36. Rama nueva desde `main` después de fusionar #36 (`ta-2-spike-renderer-a2`), un PR. **No toca** `crates/**`, `packages/protocol-ts/**`, `apps/server/**`, `infra/**`, `content/**`, `.github/**` ni la opción `csp` de `apps/web/vite.config.ts`.

**Lo que ya existe y se reutiliza:** `prepareScene`/`sceneAt`/`cueWindow`, `layoutHud`/`fitText`, los efectos puros de `src/fx/`, los tipos `RendererOptions`, `RendererHandle` y `FrameStats` de `src/index.ts`, el guard de `/dev/**`, `theme.svelte.ts`, los tokens `--arena-*` de `tokens.css`, `culori` 4.0.2 en `apps/web` y `pixi.js` 8.22.0 en el paquete.

## Para el architect: la spec necesita una enmienda

_Hechas por el architect en la nota técnica del 2026-10-27 de `spec.md`, que agrega además la excepción de medición de la fase B en `just dev`. Las dejo como registro._

1. **RF-A18 y "Áreas de preocupación" 3 dicen que ADR-0012 no existe.** Ya está en `main`. Su B4.2 fija para Pixi exactamente lo que este plan hace (`pixi.js/unsafe-eval` en la entrada del paquete, `Assets.setPreferences({ preferWorkers: false })`, sin KTX2 ni Basis) y su B6.1 exige la ruta nueva en `csp.spec.ts`. La regla "si hace falta tocar la CSP, se detiene la fase" sigue igual, pero ahora remite a B4.5 (otro ADR), no a "escribir ADR-0012".
2. **Reglas de timeline que A1 agregó y la spec no lista** (desviaciones 4, 5 y 10 de `plan-a1.md`): `timeline.cue_after_ko` (después de `ko`, solo `win`), `timeline.knockdown_duplicate` (un solo `knockdown` por `strike`), `timeline.ko_without_zero_hp` (`ko` solo con HP 0) y `duration_ms ≤ 3 600 000` (1 h). Conviene subirlas a "Diseño → Timeline v1" y a "Datos para 003", porque 003 tiene que respetarlas.
3. **Extensiones del handle de RF-A11** (decisión 4): `onState(cb)` y `snapshot()`. La spec ("Áreas de preocupación" 5) ya admite extender ADR-0009 D7; las dejo escritas para que entren en RF-A11.
4. **Medición sobre el build de producción, no sobre `just dev`** (decisión 12). El RNF dice `just dev` y el puerto 5173; con el servidor de Vite, Pixi va sin minificar y con HMR, así que la medición sale peor de lo que verá el usuario.
5. **RF-A17 más estricto**: ningún chunk, tampoco el de `dev/arena`, alcanza a Pixi ni al renderer por imports estáticos (decisión 9). Así se cumple también la regla "con `import()` en `onMount`" de RF-A16.

## Decisiones de implementación

1. **Tres capas en el paquete.**
   - **`src/player/` (pura, sin Pixi ni DOM):** `createPlayer(options, stage)` implementa el `RendererHandle` sobre una interfaz `Stage` (`init`, `loadAssets`, `apply(scene)`, `setTheme`, `resize`, `destroy`). Lleva el reloj de reproducción, `onCue`, el estado y las estadísticas. Se prueba en Vitest con un `Stage` falso y un reloj falso.
   - **`src/stage/` (Pixi):** `createPixiStage(container)` implementa `Stage`. Solo traduce `Scene` a objetos de Pixi y no guarda estado entre frames, salvo los objetos reutilizables (pools).
   - **`src/index.ts`:** `createRenderer(container, options) = createPlayer(options, createPixiStage(container))`. La primera línea del archivo es `import 'pixi.js/unsafe-eval';` (ADR-0012 B4.2: en la entrada, antes de crear cualquier renderer, para que ningún consumidor lo olvide). Los subpaths `./manifest` y `./timeline` siguen sin Pixi, así que el convertidor de la fase B no lo carga.
   `src/player/**` y `src/stats/**` entran en la lista `PURE` de `eslint.config.js` (sin Pixi ni DOM).
2. **Reloj de reproducción sobre el reloj del host.** El renderer nunca cuenta frames (RF-A11). `src/player/playback.ts` es una función pura del estado `{ playing, anchorClock, anchorT }`: `t = playing ? anchorT + (clock() − anchorClock) : anchorT`, con `play`, `pause` y `seek` que solo cambian el ancla. `t` se recorta a `[0, duration_ms]`. En el spike, el host pasa `clock = () => performance.now() − inicio`. En vivo, el host pasa el reloj del servidor y nunca llama a `pause` ni a `seek`, así que `t = clock()`. Una pestaña en segundo plano no desincroniza: al volver, el primer `tick` lee `clock()` (RF-A9).
3. **`onCue` sin estado oculto.** En cada `tick`, el reproductor dispara en orden los cues con `prevT < at_ms ≤ t`, una vez cada uno. `seek` mueve `prevT` sin disparar los cues saltados, y `play` después de `pause` sigue desde `prevT`. El intervalo está en `src/player/cues.ts` (búsqueda binaria sobre `cueTimes`). Un salto grande, como volver de una pestaña oculta, dispara todos los cues del intervalo en orden, que es lo que necesita un consumidor que los cuenta (el narrador).
4. **Extensiones del handle** (para la enmienda 3). `onState(cb)` emite `'ready' | 'playing' | 'paused' | 'finished'`, y `finished` sale cuando `t` llega a `duration_ms`. `snapshot()` devuelve `{ t, phase, winner, hp: { a, b }, lastCue }` de la última escena aplicada. `FightStage.svelte` los usa para `data-state` y los `data-*` de los e2e, sin rehacer la lógica de HP. Errores: `load` y la inicialización rechazan con `RendererError { code }`, con `code` en `'webgl_unavailable' | 'asset_invalid' | 'asset_load_failed' | 'timeline_invalid'` (RF-A16). `load` valida con `validateTimeline` y `validateManifest` antes de tocar Pixi.
5. **Pixi: inicialización (RF-A2, RF-A15 y RF-A18).**
   - `new Application()` y `app.init({ preference: 'webgl', antialias: false, resolution: Math.min(devicePixelRatio, 2), autoDensity: true, background: theme.colors.bg, resizeTo: container })`.
   - Si `init` falla, o el renderer resultante no es WebGL (`app.renderer.type !== RendererType.WEBGL`, por si la versión trae otro respaldo), da `webgl_unavailable`. El spike no corre sin WebGL.
   - `Assets.setPreferences({ preferWorkers: false })` antes de la primera carga (B4.2). Las hojas se cargan con `Assets.load({ src: url, parser: 'texture' })` desde las URL de `resolveAsset`, que son del mismo origen. Con `pixel_art: true`, `source.scaleMode = 'nearest'`. Un `Texture` por frame, con el `Rectangle` del manifest.
   - **Sin KTX2, Basis, DDS ni workers.** ESLint prohíbe en todo el paquete `pixi.js/ktx2`, `pixi.js/basis`, `pixi.js/ktx`, `pixi.js/dds`, `pixi.js/webworker` y `pixi.js/gif` (decisión 11).
   - Las fuentes del tema se cargan con `document.fonts.load()` antes de crear cualquier `Text` (ADR-0009 D7).
   - `ResizeObserver` sobre el contenedor: recalcula el letterbox de la escena lógica de 640×360 y `layoutHud` en px CSS.
   - Al terminar, `destroy()` llama a `app.destroy(true, { children: true, texture: true })` y a `Assets.unload` de sus URL. Recargar personajes también descarga las URL viejas.
6. **Árbol de la escena.** `world` (escala del letterbox × zoom de la cámara, pivote en el centro de la cámara, desplazamiento del shake) contiene `fighters` (dos `Sprite`, con el `anchor` sacado del pivote del frame y `scale.x` negativo en el espejo), `flashes` (dos `Sprite` que copian la textura del luchador con `blendMode: 'add'` y tinte neutro, decisión 7), `particles` (64 `Sprite` de `Texture.WHITE` en un pool, con su tinte por rol) y `numbers` (un pool de 8 `Text`; el texto se cambia solo cuando el número pertenece a otro cue). Aparte, sin cámara: `hud` (`Graphics` para las barras y la estela y `Text` para los nombres) y `banners` (`Text`). Por frame, `apply(scene)` solo cambia propiedades. No crea objetos salvo cuando un `Text` cambia de contenido.
7. **Colores.** Todos salen de `RendererTheme` por el rol de la escena (`hp`, `hpLost`, `crit`, `text`). La única excepción es `src/stage/neutral.ts`, con `NEUTRAL_WHITE = 0xffffff`, que ya permite el test de A1. Está justificado: en Pixi, un tinte multiplica y el blanco es la identidad, así que para aclarar el sprite golpeado hace falta una copia aditiva con tinte blanco cuya intensidad se controla con `alpha`. Ningún otro literal.
8. **HUD.** `layoutHud(viewport, measure, names)` (A1) con `measure = (text, px) => CanvasTextMetrics.measureText(text, style(px)).width`. La barra ocupa `bar.width × hp / maxHp` con `theme.colors.hp`, y la estela, `trail / maxHp` con `hpLost`, detrás. Los nombres y los banners salen de `labels`. Los números de daño se formatean con `formatNumber` (en espacio lógico, con un tamaño de fuente que compensa la escala para no bajar de 14 px CSS).
9. **Carga diferida y `check-renderer-split.mjs` (RF-A17).**
   - Kit 3 fuerza `build.manifest: true` (lo comprobé en `@sveltejs/kit@3.1.0`, `src/exports/vite/utils.js`, `enforced_config`) y lee `.svelte-kit/output/client/.vite/manifest.json`. El script lee ese archivo.
   - Por cada entrada (`isEntry`) y cada nodo de Kit (`.svelte-kit/generated/client-optimized/nodes/*.js`), calcula la clausura por `imports` estáticos y falla si alcanza un módulo de `pixi.js` o de `packages/renderer/`. Esto vale **también para el nodo de `dev/arena`** (enmienda 5).
   - Además exige el caso positivo: el nodo que importa `src/routes/dev/arena/+page.svelte` alcanza a `pixi.js` por `dynamicImports`. Sin esto, el script pasaría vacío si cambiara el formato del manifest.
   - Imprime, solo como reporte, el tamaño gzip de la clausura diferida (para el RNF de peso, objetivo orientativo ≤ 400 KB).
   - Corre en `just e2e` justo después de `pnpm --filter web build`, y por eso también en el job `e2e` de CI.
   - Exporta `checkRendererSplit(manifest)` para sus tests en `scripts/__tests__/` con fixtures (`scripts/__fixtures__/check-renderer-split/`): uno que pasa, uno con Pixi estático en el layout raíz, uno con el renderer estático en el nodo de `dev/arena` y uno sin el import dinámico.
10. **Assets en `FightStage.svelte`.** Es el único archivo que importa valores del renderer (ADR-0009 D4). Todo va con `import()` dentro de `onMount`: `@torneo/renderer`, `@torneo/renderer/fixtures/spike.timeline.json` y, por placeholder, `manifest.json` y `sheet-0.png?url` (Vite le pone hash al PNG). Los loaders viven en un mapa `id → () => Promise<…>` dentro de `FightStage.svelte`. La lista de ids seleccionables (`ARENA_FIGHTERS = ['dummy_a', 'dummy_b']`) es un dato sin renderer y vive en `#lib/features/arena/fighters.ts`, para que `ArenaControls` y la página la usen. `resolveAsset(id, file)` busca en ese mapa. La fase B le agrega los locales.
11. **ESLint.**
    - En el paquete: la lista de la decisión 5 de submódulos de Pixi prohibidos, en todos los grupos (listas completas por grupo, como en A1).
    - En `apps/web`: `@typescript-eslint/no-restricted-imports` no ve los `import()` dinámicos (es el hueco de TA-11). Si TA-11 no se fusionó antes, A2 agrega a `no-restricted-syntax`, combinado con los selectores de URL absoluta que ya existen, `ImportExpression[source.value=/^@torneo\/renderer(\/|$)/]` fuera de `FightStage.svelte`, con su caso en `tests/eslint/boundaries.test.ts`. Si TA-11 ya está, uso su regla.
12. **Método de medición de fps (RNF "Rendimiento", RF-A19), sin depender del modelo de teléfono.**
    - **Qué se mide:** el build de producción, no `vite dev` (RNF enmendado). La receta `just arena-bench` corre `just build` y después `env -u HOST HOST=127.0.0.1 PORT=3000 PUBLIC_ORIGIN=http://localhost:3000 ENABLE_DEV_CATALOG=1 node apps/web/build`. `HOST` va **fijo** en la receta, y no se lee del entorno ni se acepta como argumento: `adapter-node` escucha por defecto en `0.0.0.0`, y sin esto el puerto quedaría publicado en la LAN, contra la regla de 001. Antes de arrancar, la receta comprueba que el 3000 de `127.0.0.1` está libre, como `_prod-local-preflight`.
    - **Excepción de la fase B** (spec, RNF "Rendimiento"): los personajes convertidos solo existen con `just dev` (RF-B7), así que su medición corre allí, con los placeholders como control en el mismo modo. Eso es del plan de B; A2 solo deja la receta para los placeholders.
    - **Cómo se mide:** `/es/dev/arena` a 375 px en vertical, sin movimiento reducido. Una corrida de calentamiento y tres corridas completas, con "reiniciar" entre ellas, que también reinicia las estadísticas. Después de cada una, "copiar JSON" (frames, FPS medio, p50, p95, p99, frames > 33 ms, `devicePixelRatio`, ancho del canvas en px CSS, ids de los personajes; sin user agent) y se pega en el PR con el modelo, el SO y la versión del navegador, que escribe el owner.
    - **Cómo llega el teléfono al puerto, según el aparato:**
      - **Android:** Chrome → `chrome://inspect` → *Port forwarding* `3000 → localhost:3000` por USB. No publica nada en la LAN.
      - **iPhone:** Safari no reenvía puertos por USB. Hace falta un bind temporal a la IP de la Mac en la LAN, y **no** lo hace la receta. Es un paso manual aparte, documentado en el README del paquete y nunca en una receta de `just`: `HOST=<ip-de-la-LAN> PORT=3000 PUBLIC_ORIGIN=http://<ip-de-la-LAN>:3000 ENABLE_DEV_CATALOG=1 node apps/web/build`, solo mientras dura la medición y solo con el OK previo del owner ("Preguntas abiertas" 1 de la spec), que se anota en el PR. Las estadísticas se leen igual, con el botón de copiar.
      - **Control en escritorio:** la Mac del owner, con el mismo procedimiento sobre `localhost`.
    - **No bloquea el PR:** el umbral (p95 ≤ 18.2 ms y FPS medio ≥ 58, o ≥ 0.95 × la tasa nativa) se comprueba en el dispositivo cuando el owner lo tenga. El PR de A2 lleva la medición de escritorio y, si ya hay teléfono, la del teléfono; si no, la medición del teléfono queda como evidencia pendiente para la vista previa del owner, antes de B.
13. **Estadísticas (RF-A19).** `src/stats/frame-stats.ts` (pura): un `Float64Array` circular de 16 384 intervalos (90 s a 120 Hz caben) con `elapsedMS` del ticker de Pixi, que es el tiempo real sin tope, a diferencia de `deltaMS`, que Pixi recorta con `minFPS`. Así el paquete no necesita `performance.now()`, que su ESLint prohíbe. Se registra solo mientras `playing` y con `t` dentro de `[primer strike, win.at_ms]`. Los percentiles salen por orden sobre una copia (`nearest-rank`). `seek`, `load` y "reiniciar" vacían el buffer.
14. **Accesibilidad e i18n.**
    - El canvas lleva `role="img"` y `aria-label={m.arena_canvas_label()}`.
    - Los controles son `Button` de `ui/` (≥ 44 px), un `<select>` nativo con `Label` por lado y dos interruptores como `<input type="checkbox">` con `Label` (no hay primitiva `switch`, y agregar una con shadcn queda fuera del spike).
    - El estado `finished` anuncia el ganador en un `<p aria-live="polite">`, igual que el error. La narración accesible del combate sigue fuera del spike (RNF).
    - Todo texto visible vive en `lib/components/fight/` (`FightStage`, `ArenaControls`, `ArenaStats`) con mensajes `arena_*` en `es` y `en`.
    - `+page.svelte` solo compone: su único texto es `<title>{m.arena_title()}</title>`.
    - Los números del panel de estadísticas usan un helper nuevo, `formatDecimal(n, locale, digits)`, y los de daño, `formatInteger(n, locale)`, los dos en `lib/i18n/format.ts` (ADR-0009 D8) con sus tests.
15. **Movimiento reducido.** `FightStage` sigue a `matchMedia('(prefers-reduced-motion: reduce)')` y al interruptor "simular movimiento reducido" (OR de los dos) y llama a `setReducedMotion`. El reproductor pasa la bandera a `sceneAt`, que ya hace el resto desde A1, y no la usa para nada más. Así `onCue` y el reloj no pueden cambiar (RF-A14, test 7 de ADR-0009).
16. **Detalles de la revisión de #36.**
    - (a) El comentario de `K` en `src/scene/prepare.ts` (líneas 31–35) suma 7 + 7 + 3 = 17 > 16, y el "13" de la desviación 1 de A1 también sumaba cotas sueltas. Bajo "Timeline v1", el máximo real de `cueWindow` en la ventana cerrada `[t − 3300, t + 400]` (3 700 ms) es **10**, según la búsqueda exhaustiva del architect. Las restricciones que lo fijan:
      - entre dos `strike` hay ≥ 600 ms;
      - un `knockdown` (con `duration_ms ≥ 800`, `(at, at + duration]` sin golpes) empuja el siguiente `strike` a ≥ 801 ms;
      - un solo `knockdown` por `strike`;
      - el golpe que deja HP 0 va seguido de `ko` y no de `knockdown`;
      - `win` llega ≥ 1 500 ms después del `ko`;
      - la `intro` está en 0 y el primer `strike` llega ≥ 2 000 ms después.

      Ejemplo de 10: 6 golpes, 4 de ellos con `knockdown`. El comentario nuevo da esa derivación y deja `K = 16` como margen. Dos tests en `scene.test.ts`:
      - **el ejemplo:** construye con `timeline-builder.ts` una timeline que alcanza 10, comprueba que pasa `validateTimeline` y que `max_t cueWindow(...).length === 10 ≤ K`;
      - **la cota:** un recorrido exhaustivo (`test/support/window-bound.ts`, puro y sin azar) de las secuencias de pasos mínimos que caben en 3 700 ms. Los pasos son golpe solo (+600 ms), golpe con `knockdown` en el mismo `at_ms` (+801 ms hasta el siguiente golpe) y cierre (golpe final, `ko` en ≤ 100 ms y `win` +1 500 ms), con la `intro` como prefijo opcional (+2 000 ms hasta el primer golpe). Para cada secuencia arma la timeline, comprueba que es válida y que ninguna ventana pasa de 10, y además que el máximo encontrado es exactamente 10. Las separaciones mayores que la mínima solo pueden sacar cues de la ventana, así que los pasos mínimos bastan; lo escribo en el comentario del test.
    - (b) `sceneAt` devuelve `scene.t` sin recortar para un `t` finito mayor que `duration_ms` (`scene-at.ts:160`). Lo recorto a `[0, durationMs]`. Test: `sceneAt(p, duration + 5000)` es profundamente igual a `sceneAt(p, duration)`. Importa para la barra de progreso y para `finished`.

## Versiones fijadas

Sin dependencias nuevas. Se usan `pixi.js` 8.22.0 (fijada en A1, sigue siendo `latest` al 2026-10-26) y `culori` 4.0.2, que ya está en `apps/web`. El paso 1 vuelve a comprobar que no hay un aviso de seguridad para `pixi.js@8.22.0` (`pnpm audit`).

## Archivos que cambian

| Archivo | Cambio |
|---|---|
| `packages/renderer/src/scene/prepare.ts`, `src/scene/scene-at.ts` | modificados: comentario de `K` y recorte de `t` (decisión 16) |
| `packages/renderer/src/player/{index,playback,cues,player}.ts` | nuevos: `createPlayer`, reloj de reproducción, despacho de cues, estado y `snapshot` (decisiones 1 a 4) |
| `packages/renderer/src/stats/frame-stats.ts` | nuevo (decisión 13) |
| `packages/renderer/src/scene/viewport.ts` | nuevo, puro: letterbox de 640×360 en el contenedor y conversión de px lógicos a px CSS (RF-A15) |
| `packages/renderer/src/stage/{index,app,textures,fighters,effects,hud,banners,fonts,neutral}.ts` | nuevos: la capa Pixi (decisiones 5 a 8) |
| `packages/renderer/src/index.ts` | modificado: `import 'pixi.js/unsafe-eval'` en la primera línea, `createRenderer`, `RendererError`, y `onState`/`snapshot` en `RendererHandle` |
| `packages/renderer/eslint.config.js` | modificado: `src/player/**` y `src/stats/**` en `PURE`; submódulos de Pixi prohibidos (decisión 11) |
| `packages/renderer/test/{player,playback,cues,frame-stats,viewport}.test.ts` | nuevos; `scene.test.ts` y `eslint.test.ts` ampliados (decisión 16, submódulos de Pixi) |
| `packages/renderer/README.md` | modificado: `createRenderer`, ciclo de vida y condiciones de CSP |
| `apps/web/src/lib/components/fight/FightStage.svelte` | nuevo (decisiones 4, 10 y 15; estados de RF-A16) |
| `apps/web/src/lib/components/fight/renderer-theme.ts` (+ `.test.ts`) | nuevo: `buildRendererTheme(read)` con `culori/fn` (`useMode(modeOklch)`, `useMode(modeRgb)`, `clampRgb`); `read` es `(name) => string`, inyectable en el test |
| `apps/web/src/lib/components/fight/{ArenaControls,ArenaStats}.svelte` | nuevos: controles y panel de estadísticas con "copiar JSON" (`navigator.clipboard.writeText`). Del renderer **solo `import type`** (`FrameStats`): entran en el catálogo, y un valor rompería ADR-0009 D4 y el chequeo de chunks (ESLint ya lo prohíbe fuera de `FightStage.svelte`) |
| `apps/web/src/lib/features/arena/fighters.ts` | nuevo: `ARENA_FIGHTERS` (decisión 10) |
| `apps/web/src/lib/i18n/format.ts` (+ test) | modificado: `formatInteger` y `formatDecimal` (decisión 14) |
| `apps/web/src/routes/dev/arena/+page.svelte` | nuevo: compone `FightStage`, `ArenaControls` y `ArenaStats`; lee `?t=<ms>` (entero ≥ 0, si no, se ignora) de `page.url.searchParams` |
| `apps/web/src/routes/dev/components/_sections/ArenaSection.svelte`, `+page.svelte` | nuevo y modificado: `ArenaControls` y `ArenaStats` en el catálogo, con datos de ejemplo. `FightStage` no entra, porque cargaría Pixi en el catálogo y en sus tests de axe y CSP; su página es `/dev/arena` |
| `apps/web/messages/{es,en}.json` | modificados: `arena_title`, `arena_canvas_label`, `arena_play`, `arena_pause`, `arena_restart`, `arena_replay`, `arena_retry`, `arena_side_a`, `arena_side_b`, `arena_fighter_dummy_a`, `arena_fighter_dummy_b`, `arena_reduced_motion`, `arena_show_stats`, `arena_copy_stats`, `arena_copied`, `arena_banner_fight`, `arena_banner_ko`, `arena_winner`, `arena_loading`, `arena_error_webgl_unavailable`, `arena_error_asset_invalid`, `arena_error_asset_load_failed`, `arena_error_timeline_invalid`, `arena_stats_frames`, `arena_stats_fps_mean`, `arena_stats_p50`, `arena_stats_p95`, `arena_stats_p99`, `arena_stats_over_33` |
| `apps/web/eslint.config.js`, `tests/eslint/boundaries.test.ts` | modificados solo si TA-11 no está en `main` (decisión 11) |
| `apps/web/e2e/arena.spec.ts` | nuevo (sección "Pruebas") |
| `apps/web/e2e/csp.spec.ts` | modificado: `/es/dev/arena` con 5 s de reproducción, en Chromium y WebKit (RF-A18, ADR-0012 B6.1) |
| `apps/web/playwright.config.ts` | modificado: `launchOptions.args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader']` en el proyecto `chromium` (WebGL por software en el contenedor de CI; "Áreas de preocupación" 6) |
| `scripts/check-renderer-split.mjs`, `scripts/__tests__/check-renderer-split.test.mjs`, `scripts/__fixtures__/check-renderer-split/**` | nuevos (decisión 9) |
| `justfile` | modificado: `e2e` corre `node scripts/check-renderer-split.mjs` después del build; receta nueva `arena-bench` (decisión 12) |
| `docs/architecture.md` | modificado: reproductor puro sobre `Stage`, capa Pixi, carga diferida y condiciones de ADR-0012 B4.2 (RF-A20) |
| `CLAUDE.md` | modificado: en "Comandos", `just e2e` con `check-renderer-split` y `just arena-bench` (RF-A20) |

`.github/workflows/ci.yml` no cambia: el filtro `web` ya incluye `apps/web/**`, `packages/**`, `scripts/**` y `justfile`, y el job `e2e` corre `just e2e`, que ahora incluye el chequeo de chunks. `apps/web/vite.config.ts` no cambia: la CSP queda intacta y `check-kit-config.mjs` sigue pasando.

## Orden de trabajo

Commits `feat(renderer): … (TA-2)`, `feat(web): … (TA-2)`, `test(web): … (TA-2)` y `docs(002): … (TA-2)`.

1. **Detalles de #36 (decisión 16).** → `pnpm --filter @torneo/renderer exec vitest run` con el ejemplo de 10, el recorrido exhaustivo de la cota y el recorte de `t`. **Paralelizable con el paso 2.**
2. **Reproductor y estadísticas puros (RF-A11, RF-A14, RF-A19).** `src/player/`, `src/stats/`, `src/scene/viewport.ts` y el `Stage` falso de los tests.
   → `player.test.ts`, con reloj falso:
   - `onCue` se dispara una vez por cue al pasar su `at_ms`, en orden;
   - `seek` no dispara los cues saltados;
   - `pause` congela `t` y `play` sigue sin saltar;
   - un salto del reloj de 10 s dispara todos los cues del intervalo en orden;
   - `onState` llega a `finished` en `duration_ms`;
   - `snapshot().hp` es igual al `target_hp` del último `strike`;
   - **test 7 de ADR-0009:** la secuencia `(índice de cue, t de disparo)` es idéntica con `reducedMotion` `true` y `false`, también si se cambia a mitad de la reproducción.

   → `frame-stats.test.ts`: percentiles con series conocidas, `over33`, el buffer circular al desbordarse y que no se registra fuera de `[primer strike, win]` ni en pausa.
   → `viewport.test.ts`: letterbox a 375×211 y 1280×720 sin recortar la escena.
3. **Capa Pixi mínima y ruta mínima (RF-A15, RF-A16, RF-A18).** `src/stage/` con luchadores y fondo solamente, `createRenderer`, `FightStage.svelte` con `import()` en `onMount`, `renderer-theme.ts` y `+page.svelte`.
   → `just dev` en `/es/dev/arena`: los placeholders se mueven al ritmo del guion. Después, `just e2e` con solo `csp.spec.ts` ampliado. **Punto de corte (el paso más riesgoso):** si Chromium o WebKit reportan un `securitypolicyviolation`, paro. No toco la CSP (ADR-0012 B4.5: otro ADR), lo anoto en "Desviaciones" y el plan vuelve a revisión con security-reviewer. Si WebKit en el contenedor de Linux no tiene WebGL, el test exige que la ruta llegue a `error` con `webgl_unavailable` **después** de cargar los chunks diferidos y de ejecutar `pixi.js/unsafe-eval` (un `data-renderer-loaded` que `FightStage` pone tras el `import()`), y que haya cero violaciones. Esa corrida se marca como limitada en el PR y se repite a mano en Safari de macOS con la consola abierta.
4. **Efectos, HUD y banners en Pixi (RF-A12, RF-A13).** `effects.ts`, `hud.ts`, `banners.ts`, `fonts.ts` y `neutral.ts`.
   → `no-color-literals.test.ts` sigue en verde. Revisión visual con `just dev` a 375 px y 1280 px en los dos temas y los dos idiomas: shake, números, partículas, destello, zoom, estela y nombres recortados. Comparo contra RF-A12 y RF-A13 y ajusto constantes.
5. **Controles, estados, estadísticas e i18n (RF-A16, RF-A19).** `ArenaControls`, `ArenaStats`, `fighters.ts`, mensajes, `formatInteger`/`formatDecimal` y los estados de `FightStage`, con `data-state`, `data-hp-a`, `data-hp-b` y `data-last-cue` solo cuando la página lo pide.
   → `just check` (con `check-i18n` y `check-tokens`) y unit de `renderer-theme.ts` y de los helpers de `format.ts`. **Paralelizable con el paso 4.**
6. **Chequeo de chunks (RF-A17).** `check-renderer-split.mjs`, sus fixtures y la línea de `just e2e`.
   → los tests de `scripts/` en `just check`, y `just e2e` corre el script sobre el build real y reporta el tamaño gzip. **Paralelizable con los pasos 4 y 5.**
7. **e2e completo y capturas.** `arena.spec.ts` y `csp.spec.ts` final.
   → `just e2e` en verde en Chromium y WebKit, con las capturas en `test-results/screenshots/`. Reviso las capturas contra "Diseño → UI" de la spec y ajusto.
8. **Catálogo.** `ArenaSection.svelte` con `ArenaControls` y `ArenaStats`. → axe y capturas del catálogo (ya existentes) en verde.
9. **Docs (RF-A20).** `architecture.md`, `CLAUDE.md` y el README del paquete. → revisión del PR.
10. **Medición (RNF, decisión 12).** `just arena-bench` en la Mac (control) y, si el owner ya tiene el dispositivo, en el teléfono. → el JSON de las 3 corridas en el PR.
11. **Cierre.** `just check` y `just e2e` desde un worktree limpio, `pnpm audit --audit-level moderate`, tamaño gzip del chunk diferido y capturas, todo en el PR. Al fusionar, el owner ve la demo con placeholders ("Vista previa tras A").

## Riesgos

1. **Una ruta interna de Pixi 8.22 que la CSP bloquea** (un worker, `new Function` fuera de lo que cubre `unsafe-eval`, una fuente `blob:` fuera de `img-src`). → El paso 3 lo prueba en los dos motores antes de construir encima, con punto de corte y sin ampliar la CSP (ADR-0012 B4.5).
2. **WebGL en CI.** SwiftShader en Chromium puede necesitar flags, y WebKit en Linux puede no tener WebGL. → Flags en `launchOptions`. En WebKit, el test verifica la carga diferida y `unsafe-eval` antes del error, y queda la verificación manual en Safari (paso 3).
3. **Rendimiento en el teléfono.** Los riesgos son un `Text` que se vuelve a rasterizar en cada frame, GC por la escena nueva de cada frame y la resolución 2× en pantallas grandes. → Pools y `Text` que solo cambia de contenido con otro cue (decisión 6). Si el p95 no llega, en orden: un `out` reutilizable para `sceneAt` (riesgo 2 de A1), `BitmapText` para los números, y bajar `resolution` a 1.5. Cada cambio se anota y no toca la timeline.
4. **El dispositivo de referencia sigue sin definir.** → El método sirve para Android e iPhone (decisión 12). El PR no espera el teléfono; la medición del teléfono queda como evidencia de la vista previa, antes de B. El bind en la LAN para un iPhone solo se hace con el OK del owner.
5. **`import()` dinámicos sin lint** (TA-11). Otro archivo podría importar el renderer con `import()` sin que ESLint lo vea. → Regla puntual (decisión 11) y `check-renderer-split.mjs`, que vería un import estático aunque ESLint fallara.
6. **Formato del manifest de Vite/Kit.** Si cambia la ruta o la forma de `.vite/manifest.json` con una versión de Kit, el script podría pasar vacío. → El caso positivo obligatorio (decisión 9) hace que falle en vez de pasar.
7. **`Assets` es global en Pixi.** Dos `FightStage` en la misma página compartirían la caché. → Solo hay uno, en `/dev/arena`. `destroy` descarga sus URL. Lo anoto en el README para la vista en vivo.
8. **Una diferencia visual entre lo que la escena describe y lo que se ve** (pivote, espejo y escala). → `viewport.test.ts` y la revisión visual del paso 4. El pivote del espejo usa `anchor`, nunca un desplazamiento calculado aparte.

## Alternativas descartadas

- **Medir con `performance.now()` dentro del renderer.** Contradice la regla de ESLint del paquete; `ticker.elapsedMS` da el mismo intervalo (decisión 13).
- **HUD en el DOM.** La spec ya lo descarta ("Alternativas consideradas").
- **Primitiva `switch` o `select` de shadcn para los controles.** Un `<input type="checkbox">` y un `<select>` nativos con `Label` son accesibles y no suman componentes al catálogo solo para el spike.
- **`FightStage` en `/dev/components`.** Cargaría Pixi y WebGL en el catálogo y en sus tests de axe, capturas y CSP. La página del componente es `/dev/arena`.
- **Medir sobre `vite dev`.** Mide Pixi sin minificar y con HMR (enmienda 4).
- **Respaldo a otro renderer de Pixi si no hay WebGL.** La spec fija WebGL; un respaldo silencioso escondería el problema y cambiaría la medición.
- **Ampliar la CSP o usar workers.** Prohibido por ADR-0012 B3 y B4.2.

## Pruebas (cómo se demuestra que funciona)

- [ ] `just check` y `just e2e` en verde, con las salidas en el PR.
- [ ] **RF-A11:** `player.test.ts` (`onCue`, `seek`, `pause`/`play`, salto del reloj y windup sobre `sceneAt`, ya en A1); e2e del ciclo de vida (`playing` → `finished`).
- [ ] **RF-A12 y RF-A13:** las pruebas puras de A1, `no-color-literals.test.ts`, la captura e2e a 375×812 y la revisión visual del owner.
- [ ] **RF-A14:** el test 7 de ADR-0009 en `player.test.ts`, y `reduced-motion.test.ts` de A1.
- [ ] **RF-A15:** `viewport.test.ts`; e2e a 375×812 y 1280×800: el canvas cabe en el ancho y `scrollWidth − clientWidth ≤ 0`.
- [ ] **RF-A16:** `arena.spec.ts`:
  - `/es/dev/arena` y `/en/dev/arena` llegan a `data-state="playing"`;
  - `?t=<duration_ms>` llega a `finished` con el ganador anunciado;
  - en la instancia sin catálogo (puerto 4174) da 404;
  - axe sin violaciones `serious` ni `critical`, en los dos temas;
  - sin errores en la consola;
  - `data-hp-a`/`data-hp-b` cambian después de un golpe;
  - y `check-i18n` en `just check`.
- [ ] **RF-A17:** `check-renderer-split.mjs` sobre el build real dentro de `just e2e`, con sus fixtures en los tests de `scripts/`.
- [ ] **RF-A18:** `csp.spec.ts`: cero `securitypolicyviolation` en `/es/dev/arena` durante la carga y 5 s de reproducción, en Chromium y WebKit (con la nota de WebKit del paso 3 si aplica). `apps/web/vite.config.ts` sin diff, y `check-kit-config.mjs` en verde.
- [ ] **RF-A19:** `frame-stats.test.ts`; e2e: después de `seek` a la mitad y 3 s de reproducción, el panel muestra `frames > 0`. JSON de la Mac (y del teléfono si ya lo hay) en el PR.
- [ ] **RF-A20:** `architecture.md`, `CLAUDE.md` y el README, en la revisión del PR.
- [ ] **Detalles de #36:** el ejemplo de 10 cues de `cueWindow` (válido y `=== 10 ≤ K`), el recorrido exhaustivo que prueba que 10 es la cota, y `sceneAt` más allá de `duration_ms`.
- [ ] **`just arena-bench`:** mientras corre, `lsof -nP -iTCP:3000 -sTCP:LISTEN` muestra solo `127.0.0.1:3000`; salida pegada en el PR.
- [ ] Capturas a 375×812 y 1280×800, en `es` y `en`, con los temas claro y oscuro, como artefacto (sin comparación de píxeles).

## Estimación de CI

Sin cambios en los workflows. El job `web` suma unos 20 s (los tests nuevos del paquete y de `scripts/`). El job `e2e` suma 1–2 min: `arena.spec.ts` (unas 10 pruebas con WebGL por software), el caso nuevo de `csp.spec.ts` en dos motores y `check-renderer-split.mjs` (< 1 s). `images` solo corre si cambia el lock o un Dockerfile, y este plan no cambia ninguno.

## Desviaciones durante la implementación

1. **`check-renderer-split` necesita un mapa de módulos (decisión 9).** El manifest de Vite (Kit 3.1 con Vite 8/Rolldown) da el grafo de chunks, pero no los módulos de cada chunk: un `import` estático de Pixi en el layout cae en un chunk compartido `_xxxx.js` sin nombre de módulo, y el chequeo pasaría. Agregué `apps/web/vite/chunk-modules.ts`, un plugin solo de build y solo del entorno `client` que escribe `.svelte-kit/output/client/.vite/chunk-modules.json` (chunk → módulos). `.vite/` no se publica (adapter-node no lo copia a `build/client`). Por eso `apps/web/vite.config.ts` **sí cambia**, pero solo en la lista de `plugins`; la opción `csp` queda intacta y `check-kit-config` pasa. El script lee los dos archivos, y los fixtures traen los dos. Los nodos de Kit 3.1 están en `.svelte-kit/generated/build/client-optimized/nodes/`, y el de `dev/arena` se reconoce porque su chunk contiene `src/routes/dev/arena/+page.svelte`. El caso positivo es estricto: Pixi tiene que aparecer en lo que agregan los `dynamicImports`, no en la clausura estática. Lo probé también a mano: con un `import` estático del renderer en `+layout.svelte`, el script falla sobre `nodes/0.js`.
2. **`load(timeline: unknown, assets: ReadonlyMap<string, unknown>)`.** El handle recibe JSON no confiable y lo valida dentro, en vez de exigir tipos ya validados; así `FightStage` no castea los JSON importados.
3. **Interfaz `Stage`.** `init(setup)` recibe el tema, los textos, `formatNumber` y el callback `onFrame(elapsedMs)` del ticker. No hay `resize` en la interfaz: el `ResizeObserver` vive en la capa Pixi, que llama a `renderer.resize` en lugar de usar `resizeTo` (que solo escucha el `resize` de la ventana).
4. **`preference: ['webgl']`** (lista) en vez de `'webgl'`: con la cadena, Pixi probaría WebGPU y canvas como respaldo y cargaría sus chunks. La comprobación de `RendererType.WEBGL` sigue.
5. **`app.destroy(true, { children: true })` sin `texture: true`**: destruiría `Texture.WHITE`, compartida por partículas y barras, y rompería un segundo renderer (reintentar o cambiar de luchador). Las texturas de frames se destruyen a mano y las hojas se descargan con `Assets.unload`.
6. **Barras del HUD como sprites de `Texture.WHITE`** (posición, ancho y tinte por frame) en vez de `Graphics`, que reconstruye geometría en cada `clear()`. La línea del suelo sí es un `Graphics`, que se dibuja una vez por tema.
7. **`role="img"` y `aria-label` en el contenedor del canvas**, no en el `<canvas>`: el canvas lo crea Pixi dentro de ese `div`. Los mensajes de error y del ganador quedan fuera de él, en un `aria-live`, para que no los oculte el rol `img`.
8. **`ARENA_FIGHTERS` llega a `ArenaControls` por props.** ESLint (ADR-0009 D4) prohíbe importar valores de `#lib/features/**` en componentes de dominio, así que la página lo importa y lo pasa. Los nombres traducidos salen de `lib/components/fight/fighter-name.ts`. `ArenaControls` tiene `idPrefix` para dos instancias en el catálogo.
9. **Mensaje extra `arena_stats_title`**, el encabezado del panel de estadísticas.
10. **El reproductor aplica la escena antes de disparar los cues**, para que `snapshot()` dentro de un `onCue` ya muestre ese cue (los `data-hp-*` de los e2e se refrescan en `onCue`). `snapshot().lastCue` es `{ index, type } | null`.
11. **Los tipos públicos (`RendererTheme`, `RendererOptions`, `RendererHandle`, …) viven en `src/player/types.ts`** y `src/index.ts` los reexporta; el import de `pixi.js/unsafe-eval` es la primera sentencia del archivo, debajo de un comentario.
12. **`CLAUDE.md` lo aplicó el orquestador**, no el agente de frontend (un agente no cambia `CLAUDE.md` por su cuenta): las dos líneas de "Comandos" (`just e2e` con `check-renderer-split` y `just arena-bench`) entraron en el commit 9ec0b17 de esta rama. `docs/architecture.md` y el README del paquete van en los commits de A2.
13. **Los dos interruptores son `Button` con `role="switch"` y `aria-checked`**, no `<input type="checkbox">` (decisión 14): el checkbox nativo mide 20×20 px y el test de objetivos de 44 px del catálogo falla con él. El `Button` de `ui/` ya mide 44 px y tiene el anillo de foco.
14. **Axe en `/dev/arena` espera 500 ms** después de habilitar los controles: los botones pasan de `disabled:opacity-50` a opacos con `transition-all`, y axe medía el contraste a mitad de la transición.
15. **Paso 10 (medición):** no hay teléfono ni corrida de escritorio en este PR. En el entorno del agente Chromium usa SwiftShader, que no sirve como control. La medición de la Mac y del teléfono queda para el owner con `just arena-bench`.
16. **Hallazgos de QA de A2.** (F1) `Assets` de Pixi es global: al cambiar de luchador muy rápido, el `destroy` del renderer viejo descargaba una hoja que el nuevo ya había sacado de la caché (`addressModeU` de null). `src/stage/shared-cache.ts` cuenta referencias por URL: carga una vez, descarga solo con la última liberación y una carga nueva espera a la descarga en curso. `FightStage` además destruye un renderer que otro arranque dejó obsoleto. Test unitario `shared-cache.test.ts` y e2e de ráfaga en `arena.spec.ts`; la sonda de QA pasó de 5/6 corridas con errores a 0/6. (F2) Una carga fallida se olvida sin `Assets.unload`. (F3) "Copiado" dura 2 s fijos, sin depender del refresco del panel. (F4) `?t=` acepta cualquier entero ≥ 0 y el reproductor lo recorta a la duración.

## Revisión
### architect — 2026-10-27 — REQUEST_CHANGES
- Verifiqué:
  - **Spec.** Cubre RF-A11 a RF-A20 con archivo, paso y prueba. Las cinco enmiendas que pide están hechas en `spec.md` (nota técnica del 2026-10-27), y agregué una excepción: la medición con personajes de la fase B sigue en `just dev`, porque el servido local no existe en el build (RF-B7).
  - **ADR-0012.** Cumple B4.2 y el trabajo derivado 9: `pixi.js/unsafe-eval` en la primera línea de `src/index.ts`, `preferWorkers: false`, ESLint contra `ktx2`/`basis`/`ktx`/`dds`/`webworker`/`gif`, y `/es/dev/arena` en `csp.spec.ts` en dos motores (B6.1). `vite.config.ts` queda sin diff. El punto de corte del paso 3 remite a B4.5 y no amplía la CSP.
  - **ADR-0009.** Respeta D4: solo `FightStage.svelte` importa valores; `ARENA_FIGHTERS` vive fuera del renderer; los `import()` tienen regla puntual mientras falte TA-11. Respeta D7: `culori/fn` con `useMode`, `document.fonts.load` antes de crear `Text`, y el test 7 en `player.test.ts`, también con el cambio a mitad de la reproducción.
  - **Determinismo y límites.** El reproductor es puro con reloj inyectado y no cuenta frames. `elapsedMS` evita `performance.now()` en el paquete. `src/player/**` y `src/stats/**` entran en `PURE`. La capa Pixi no guarda estado entre frames salvo los pools.
  - **Implementable sin la conversación.** Nombra archivos, orden, puntos de corte, riesgos con mitigación y pruebas por RF.
  - **Frontmatter.** Válido.
- Cambios pedidos:
  1. **Decisión 16(a): el peor caso no es 13, es 10.** Bajo las reglas de "Timeline v1" el test `cueWindow(...).length === 13` no se puede construir. Cuentas:
     - un `knockdown` empuja el siguiente `strike` a ≥ 801 ms (`strike_during_knockdown` es `(at, at + duration]` con `duration ≥ 800`);
     - el golpe que deja HP 0 va seguido de `ko`, no de `knockdown`;
     - el `win` llega ≥ 1500 ms después.

     Una búsqueda exhaustiva sobre las separaciones mínimas en la ventana cerrada de 3 700 ms da 10. Un ejemplo: 6 golpes, 4 de ellos con `knockdown`. El "13" de la desviación 1 de `plan-a1.md` sumaba cotas sueltas. Pido:
     - que el comentario de `K` dé la derivación con 10;
     - que el test construya ese caso con `timeline-builder.ts` y compruebe que pasa `validateTimeline` y que da `=== 10 ≤ K`;
     - y, para que sea una cota y no solo un ejemplo, que un test recorra las combinaciones de separaciones mínimas (golpe solo a 600 ms, golpe con `knockdown` a 801 ms, cierre con `ko` y `win`) y compruebe que ninguna pasa de 10. `K = 16` puede quedarse como margen.
  2. **Decisión 12: `HOST=127.0.0.1` explícito en `just arena-bench`.** `adapter-node` escucha por defecto en `0.0.0.0`. El comando del plan (`PORT=3000 … node apps/web/build`) publicaría el puerto en la LAN, contra la regla de 001 que la propia decisión cita ("siempre en `127.0.0.1`"). La receta tiene que fijarlo y no aceptar otro valor. El bind a la IP de la LAN para el iPhone queda como paso manual aparte, con el OK del owner.
- Notas (no bloquean):
  - `ArenaStats` y `ArenaControls` entran en el catálogo, así que solo pueden usar `import type` del renderer (`FrameStats`). Si importan un valor, rompen D4 y el chequeo de chunks.
  - Fuera de este plan: los `webServer` de `playwright.config.ts` tampoco fijan `HOST` y escuchan en `0.0.0.0` mientras corre `just e2e`. Lo anoto para un issue de 001; A2 no tiene que tocarlo.

### frontend-engineer — 2026-10-27 — RESPUESTA
- **Cambio 1 (cota de `cueWindow`).** La decisión 16(a) usa 10 en lugar de 13, con la derivación: ≥ 600 ms entre golpes, ≥ 801 ms después de un `knockdown`, un `knockdown` por golpe, `ko` tras el golpe final y `win` ≥ 1 500 ms después. El ejemplo de 10 (6 golpes, 4 con `knockdown`) se construye con `timeline-builder.ts` y se valida. Un recorrido exhaustivo de las secuencias de pasos mínimos en 3 700 ms prueba que ninguna ventana pasa de 10 y que 10 se alcanza. `K = 16` queda como margen. Paso 1 y "Pruebas" actualizados.
- **Cambio 2 (`HOST`).** `just arena-bench` fija `HOST=127.0.0.1` sin leerlo del entorno ni aceptarlo como argumento, y comprueba antes que el puerto está libre. "Pruebas" suma la verificación con `lsof`. El bind en la LAN para un iPhone es un paso manual aparte, fuera de toda receta, con el OK previo del owner anotado en el PR.
- **Excepción de la fase B** registrada en la decisión 12 (medición con personajes convertidos en `just dev`, placeholders como control; la ejecuta el plan de B).
- **Notas.** `ArenaControls` y `ArenaStats` solo usan `import type` del renderer (tabla de archivos). Lo de `HOST` en los `webServer` de Playwright queda para el issue de 001 que propone el architect.
- `status: in-review`.

### architect — 2026-10-27 — APPROVE (segunda revisión)
- Verifiqué los dos cambios pedidos:
  1. **Decisión 16(a).** Las restricciones que da coinciden con `validate.ts`: ≥ 600 ms entre golpes, `strike_during_knockdown` sobre `(at, at + duration]` con `duration ≥ 800`, `knockdown_duplicate`, el `ko` justo después del golpe con HP 0 y `win` ≥ 1 500 ms. El ejemplo de 10 se valida con `validateTimeline`. El recorrido exhaustivo arma timelines completas y desliza la ventana sobre cada `t`, así que también cubre el caso de un `knockdown` cuyo golpe queda fuera de la ventana. Comprueba "≤ 10 y se alcanza 10". El argumento de que una separación mayor que la mínima solo saca cues de la ventana es correcto.
  2. **Decisión 12.** `HOST=127.0.0.1` va fijo en la receta, con `env -u HOST`, sin leerlo del entorno ni aceptarlo como argumento, y con un chequeo previo de que el puerto está libre. El bind en la LAN queda como paso manual, fuera de `just`, con el OK del owner en el PR.
- Las notas también quedaron atendidas: solo `import type` en `ArenaControls`/`ArenaStats`, y lo de `HOST` en Playwright pasa a un issue de 001. Con esto, `status: approved`.
