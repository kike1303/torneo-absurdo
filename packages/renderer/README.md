# @torneo/renderer

Reproductor de peleas de Torneo Absurdo. Agnóstico de framework: no importa Svelte, Kit ni `@torneo/protocol`. Fases A1 (núcleo puro) y A2 (reproductor, capa Pixi y medición) de [002](../../docs/features/002-spike-renderer/spec.md), y fase C (pelea en curso) de [010](../../docs/features/010-scheduler-de-peleas/spec.md).

## Subpaths

- `.`: `createRenderer`, los tipos públicos (`RendererTheme`, `RendererOptions`, `RendererHandle`, `RendererError`, `FrameStats`) y los módulos puros. Es el único subpath que carga Pixi.
- `./manifest`: formato de sprites v1, `validateManifest`, `resolveAnimation` y `windupMs`.
- `./timeline`: `Timeline` v1 y v2, y `validateTimeline`.
- `./scene`: `prepareScene`, `prepareSceneFrom` y `sceneAt`, sin Pixi.
- `./placeholders/*`: `dummy_a` y `dummy_b` (`manifest.json` y `sheet-0.png`).
- `./fixtures/*`: el guion del spike (`spike.timeline.json`).

Se exporta el TS fuente, sin paso de build. Los imports relativos llevan `.ts` para que funcione igual en Vite y en Node con _type stripping_.

## Formato normativo

El formato de assets, la `Timeline` y las reglas de coreografía están en la spec de 002 ("Formato de assets v1", "Timeline v1" y "Coreografía"). Los validadores tratan su entrada como no confiable y devuelven errores como códigos con JSON Pointer, nunca texto.

## Escena pura

`prepareScene(timeline, manifests)` se llama una vez por carga y devuelve una estructura congelada. `prepareSceneFrom(previa, timeline)` da el mismo resultado en cada `extend`, reutilizando lo que ningún cue nuevo puede cambiar. `sceneAt(prepared, t, { reducedMotion })` describe la escena en `t`: luchadores, HUD (solo valores), cámara, banners y efectos. Es función pura de `t`, así que `seek` muestra lo mismo que reproducir hasta ahí. El layout del HUD en px CSS es `layoutHud(viewport, measure, names)`.

## `createRenderer` y ciclo de vida

```ts
const handle = createRenderer(container, { theme, reducedMotion, clock, labels, formatNumber, resolveAsset });
await handle.load(timelineJson, new Map([['dummy_a', manifestJson], ['dummy_b', manifestJson]]));
handle.onState((state) => …); // 'ready' | 'playing' | 'paused' | 'finished'
handle.seek(0);
handle.play();
// …
handle.destroy();
```

- `createRenderer` arranca Pixi en el acto; `load` espera esa inicialización, valida la timeline y los manifests (entrada no confiable) y carga las hojas. Rechaza con `RendererError` y un `code`: `webgl_unavailable`, `asset_invalid`, `asset_load_failed` o `timeline_invalid`.
- **Reloj:** `t = anclaT + (clock() − anclaReloj)`, recortado a `[0, duration_ms]`. `play`, `pause` y `seek` solo mueven el ancla. Sin `seek` ni `pause`, `t = clock()`: así se reproduce en vivo con el reloj del servidor. El renderer nunca cuenta frames.
- **Cues:** `onCue` dispara cada cue una vez, en orden, cuando la reproducción pasa su `at_ms`. Un `seek` no dispara lo que salta; un salto del reloj (volver de una pestaña oculta) dispara todo el intervalo. `snapshot()` da `t`, fase, ganador, HP y el último cue de la escena aplicada, sin texto.
- **`Assets` de Pixi es global:** por eso las hojas pasan por una caché compartida con conteo de referencias (`src/stage/shared-cache.ts`). Cada URL se carga una vez, `destroy` solo suelta las referencias de su renderer, una hoja se descarga cuando la suelta su último dueño y una carga nueva espera a la descarga en curso. Dos renderers que se solapan (un cambio rápido de luchador, un reintento) no se destruyen las texturas entre sí.

## Pelea en curso

```ts
const handle = createRenderer(container, { …, clock: () => serverNow - liveStartedAt - 3500 });
const loading = handle.loadLive(partialTimeline, assets); // `validateTimeline` con `partial: true`
// Cada lote, también mientras cargan las hojas:
handle.extend(longerTimeline);
await loading;
handle.play();
```

- **Sin `seek` ni `pause`:** `t = clock()`, así que todos los espectadores ven lo mismo.
- **Timeline parcial:** las mismas reglas de la v2, sin `ending_missing` ni `closing_missing`. Mientras no llega el cierre, `duration_ms` es la cota de la pelea, y `finished` no existe ni como estado ni como fase, aunque el reloj la pase. Cuando llega el cierre, `duration_ms` es el fin real.
- **`extend`** es síncrono, no recarga hojas y no mueve la reproducción ni el cursor. Lanza `RendererError('timeline_invalid')` sin cambiar nada si no hay un `loadLive` vigente, si la timeline no es válida, si cambia un cue anterior o un luchador, o si cambia `duration_ms` sin que llegue el cierre o después de que llegó.
- **Cues:** el primer `play()` deja el cursor en `t` y pinta ese instante, así que quien entra tarde no recibe `onCue` por lo saltado, y `snapshot()` ya muestra el HP y el resultado. Con el reloj negativo, `t = 0` y no sale ningún cue. Un lote atrasado dispara sus cues en el siguiente frame, en ráfaga si son varios.

`/dev/arena?log=<nombre>&live=1` entrega un dorado por lotes según su `t`, sin server. `?t=` es el tiempo del server ya transcurrido al abrir, y sirve para probar una entrada tarde.

## Condiciones de CSP (ADR-0012 B4.2)

- `src/index.ts` importa `pixi.js/unsafe-eval` en su primera línea, antes de crear cualquier renderer.
- `Assets.setPreferences({ preferWorkers: false })` antes de la primera carga: Pixi crearía los workers desde URLs `blob:`.
- Sin texturas KTX2, Basis, DDS ni GIF, y sin `pixi.js/webworker`: ESLint los prohíbe en todo el paquete.
- Solo imágenes del mismo origen (`resolveAsset` devuelve las URL con hash de Vite).
- Solo WebGL (`preference: ['webgl']`), sin respaldo silencioso a otro renderer.

## Medición de fps (RNF "Rendimiento")

`just arena-bench` construye y sirve el build de producción en `127.0.0.1:3000` con `ENABLE_DEV_CATALOG=1`. La receta fija `HOST=127.0.0.1` y no lo lee del entorno.

1. Abre `/es/dev/arena` a 375 px en vertical, sin movimiento reducido, y activa "Mostrar estadísticas". La arena carga en pausa (en `t=0` o en `?t=`) y cambiar de personaje la recarga en pausa: nada se reproduce hasta pulsar "Reproducir".
2. Pulsa "Reproducir" para la corrida de calentamiento; después, tres corridas completas, con "Reiniciar" entre ellas (reinicia la reproducción y las estadísticas).
3. Después de cada una, "Copiar JSON" y pégalo en el PR con el modelo, el SO y la versión del navegador. Fuera de un contexto seguro (la IP de la LAN en el iPhone) no hay portapapeles: el JSON aparece seleccionado en un cuadro de texto para copiarlo a mano.

Umbral por corrida: p95 ≤ 18.2 ms y FPS medio ≥ 58 (o ≥ 0.95 × la tasa nativa en pantallas de 90/120 Hz).

El bloqueo de cada `extend` (RNF "CPU" de 010) se mide de dos formas:

- `pnpm --filter @torneo/renderer bench:prepare` da en Node el costo con 200 cues. Umbral: `prepareSceneFrom` con p50 ≤ 0,67 ms.
- `/es/dev/arena?log=draw&live=1`, con la pelea entera, deja `extend_ms` en el JSON de "Copiar JSON". Umbral con la CPU limitada a 6×: p50 ≤ 4 ms y p95 ≤ 8 ms, en la peor de tres corridas tras una de calentamiento. Pon el límite después de cargar la página, porque la navegación lo puede quitar, y compara con una corrida sin límite: el límite no siempre alarga por igual las tareas tan cortas.

- **Android:** Chrome → `chrome://inspect` → _Port forwarding_ `3000 → localhost:3000` por USB. No publica nada en la LAN.
- **iPhone:** Safari no reenvía puertos por USB. Solo con el OK previo del owner, anotado en el PR, y solo mientras dura la medición, a mano y fuera de `just`: `HOST=<ip-de-la-LAN> PORT=3000 PUBLIC_ORIGIN=http://<ip-de-la-LAN>:3000 ENABLE_DEV_CATALOG=1 node apps/web/build` (después de `just build`).
- **Control en escritorio:** el mismo procedimiento en `localhost:3000`.

## Sin `Math.random` ni `Date`

El tiempo lo da el reloj que inyecta el host y el azar cosmético sale de `src/fx/prng.ts` (`mulberry32`), sembrado con el índice del cue y un canal. Así todos los espectadores ven lo mismo, y la semilla de la pelea nunca llega al renderer (ADR-0002). ESLint prohíbe `Math.random`, `Date`, `performance` y `crypto` en todo el paquete, tests incluidos, y Pixi y el DOM en el núcleo puro (`src/{manifest,timeline,scene,fx,player,stats}`).

## Placeholders

```sh
pnpm --filter @torneo/renderer gen:placeholders
```

`scripts/placeholders/draw.ts` dibuja (función pura) y `scripts/gen-placeholders.ts` escribe con `pngjs`. La salida se commitea; `test/placeholders.test.ts` compara los píxeles decodificados y el JSON con lo que devuelve `drawPlaceholder()`.

## Comandos

`just check-web` corre `tsc` (dos `tsconfig`), ESLint y Vitest de este paquete.
