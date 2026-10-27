# @torneo/renderer

Reproductor de peleas de Torneo Absurdo. Agnóstico de framework: no importa Svelte, Kit ni `@torneo/protocol`. Fase A1 de [002](../../docs/features/002-spike-renderer/spec.md): el núcleo puro. La capa Pixi (`src/stage/`, `createRenderer`) llega en A2.

## Subpaths
- `.`: tipos públicos (`RendererTheme`, `RendererOptions`, `RendererHandle`) y los módulos puros.
- `./manifest`: formato de sprites v1, `validateManifest`, `resolveAnimation` y `windupMs`.
- `./timeline`: `Timeline` v1 y `validateTimeline`.
- `./placeholders/*`: `dummy_a` y `dummy_b` (`manifest.json` y `sheet-0.png`).
- `./fixtures/*`: el guion del spike (`spike.timeline.json`).

Se exporta el TS fuente, sin paso de build. Los imports relativos llevan `.ts` para que funcione igual en Vite y en Node con *type stripping*.

## Formato normativo
El formato de assets, la `Timeline` y las reglas de coreografía están en la spec de 002 ("Formato de assets v1", "Timeline v1" y "Coreografía"). Los validadores tratan su entrada como no confiable y devuelven errores como códigos con JSON Pointer, nunca texto.

## Escena pura
`prepareScene(timeline, manifests)` se llama una vez por carga y devuelve una estructura congelada. `sceneAt(prepared, t, { reducedMotion })` describe la escena en `t`: luchadores, HUD (solo valores), cámara, banners y efectos. Es función pura de `t`, así que `seek` muestra lo mismo que reproducir hasta ahí. El layout del HUD en px CSS es `layoutHud(viewport, measure, names)`.

## Sin `Math.random` ni `Date`
El tiempo lo da el reloj que inyecta el host y el azar cosmético sale de `src/fx/prng.ts` (`mulberry32`), sembrado con el índice del cue y un canal. Así todos los espectadores ven lo mismo, y la semilla de la pelea nunca llega al renderer (ADR-0002). ESLint prohíbe `Math.random`, `Date`, `performance` y `crypto` en todo el paquete, tests incluidos.

## Placeholders
```sh
pnpm --filter @torneo/renderer gen:placeholders
```
`scripts/placeholders/draw.ts` dibuja (función pura) y `scripts/gen-placeholders.ts` escribe con `pngjs`. La salida se commitea; `test/placeholders.test.ts` compara los píxeles decodificados y el JSON con lo que devuelve `drawPlaceholder()`.

## Comandos
`just check-web` corre `tsc` (dos `tsconfig`), ESLint y Vitest de este paquete.
