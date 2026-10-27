---
id: "002"
title: Spike de renderizado de peleas — fase A1
type: plan
status: approved
risk: medium
author: frontend-engineer
reviewers: [architect]
approvals:
  - by: architect
    date: 2026-10-26
linear: TA-2
depends_on: ["001"]
created: 2026-10-26
updated: 2026-10-26
---

# Plan: Spike de renderizado de peleas — fase A1 (desde spec.md 2026-10-26)

> [intent](./intent.md) · [spec](./spec.md) · ADRs [0002](../../decisions/0002-commit-reveal-semilla.md), [0003](../../decisions/0003-stack-tecnologico.md), [0004](../../decisions/0004-personajes-homenaje.md), [0007](../../decisions/0007-i18n-desde-cero.md), [0009](../../decisions/0009-ui-design-system.md) (D4 y D7)

**Alcance:** RF-A1 a RF-A10, más la parte **pura** de RF-A12, RF-A13 y RF-A14 (lo que `sceneAt` describe; ver decisión 1). Es el paquete `@torneo/renderer` sin Pixi en ejecución: formato de assets, timeline, escena pura, PRNG cosmético, placeholders, guion del spike y su cobertura en `just check`. Rama `ta-2-spike-renderer`, un PR. **No toca** `crates/**`, `packages/protocol-ts/**`, `apps/server/**`, `infra/**`, `content/**` ni `.github/**`.

**Por qué parto la fase A.** La spec lo permite si el diff es grande, y lo es: A1 son unos 25 archivos de TS puro con muchos tests, dos fixtures JSON grandes (el guion y los manifests) y la cadena de suministro de Pixi. A2 (Pixi, efectos en pantalla, HUD dibujado, ruta, CSP, carga diferida y medición) tiene los riesgos de integración (WebGL en CI, CSP en dos motores, rendimiento) y se revisa mejor sobre una base pura ya aprobada. `plan-a2.md` se escribe al aprobarse este plan y puede revisarse mientras A1 se implementa. Precio: la vista previa del owner con placeholders llega al fusionar A2, no A1 (spec, "Vista previa tras A").

## Decisiones de implementación

1. **A1 entrega la escena completa, A2 solo la dibuja.** RF-A9 pide que `sceneAt` describa luchadores, HUD, cámara, banners y efectos vivos con sus parámetros. Por eso las reglas medibles de RF-A12 (duraciones del shake, tope de 64 partículas, vida de los números, zoom ≤ 1.15× durante ≤ 300 ms, ≤ 3 destellos en cualquier ventana de 1 s), el layout del HUD de RF-A13 (con anchos de texto falsos) y la parte de `sceneAt` de RF-A14 (luchadores y HUD iguales con y sin movimiento reducido) se implementan y prueban aquí. En A2 quedan la capa Pixi que aplica esa descripción, `onCue`, la parte de reloj falso de RF-A14 y la revisión visual.
2. **`prepareScene` + `sceneAt`.** La firma de la spec es `sceneAt(timeline, manifests, t, opts)`. Para no recorrer la timeline en cada frame, la parto en dos funciones puras: `prepareScene(timeline, manifests): PreparedScene` (una vez por `load`; construye índices ordenados por luchador y por tipo de cue, y precalcula windups, recuperaciones, destellos admitidos y semillas por cue) y `sceneAt(prepared, t, opts): Scene`, que hace búsquedas binarias y mira hacia atrás un número **acotado** de cues (como mucho, los que caben en la ventana de 3 s de la animación más larga). Sigue siendo función pura de `t`: `PreparedScene` es inmutable, con tipos `readonly` y **`Object.freeze` profundo siempre** (no hay forma agnóstica de saber si es desarrollo: el núcleo no ve `process` ni `import.meta.env`, y en B lo carga Node). Congelar cuesta una pasada por `load`, nunca por frame. Exporto además `sceneAt` con la firma de la spec como envoltura (`sceneAtTimeline`) para los tests que la citan. La ventana de cues que mira `sceneAt` sale de una función pura interna, `cueWindow(prepared, t)`, que `sceneAt` usa y que los tests pueden inspeccionar (cota determinista del paso 7, en lugar de medir tiempo).
3. **Colores por rol, no por valor.** La escena nunca lleva colores: los efectos y el HUD dicen `color: 'hp' | 'hpLost' | 'crit' | 'text'` y A2 los traduce con el `RendererTheme`. Así `packages/renderer/src` no tiene literales de color en A1, y un test lo verifica (decisión 9). El blanco neutro del destello (una copia aditiva del sprite, porque el tinte de Pixi multiplica y no puede aclarar) llega en A2 en un único archivo permitido, `src/stage/neutral.ts`.
4. **HUD en espacio de pantalla.** La escena lógica es de 640×360 con letterbox (RF-A15), pero a 375 px de ancho cada píxel lógico mide 0.59 px CSS: un nombre de 14 px lógicos quedaría en 8 px. Por eso `Scene.hud` lleva **solo valores** (HP mostrado, estela, fase de los banners) y no depende del tamaño del contenedor; el layout es una función pura aparte, `layoutHud(viewport, measure, names)`, que llama la capa Pixi de A2. Trabaja en px CSS del contenedor, fuera de la cámara (no se mueve con el shake ni con el zoom), con fuente mínima de 14 px CSS. Los luchadores, los efectos y los números de daño van en coordenadas lógicas. La spec lo admite desde su enmienda técnica (RF-A13). Como `sceneAt` no conoce el layout, el test de RF-A14 compara `hud` sin él.
5. **Imports relativos con extensión `.ts`.** El paquete se consume como TS fuente (RF-A1) por Vite y `svelte-check` (el `$app/tsconfig` de Kit 3 ya trae `allowImportingTsExtensions: true`, lo comprobé en `@sveltejs/kit@3.1.0`, `src/core/sync/write_tsconfig/utils.js`) y, en la fase B, por Node con *type stripping* desde `tools/sff-converter` (RF-B1), que **no** reescribe `.js` a `.ts`. Con `.ts` en los imports relativos, `./manifest` funciona en los dos sin paso de build. `tsconfig` con `allowImportingTsExtensions`, `noEmit`, `erasableSyntaxOnly` y `verbatimModuleSyntax` (nada de `enum` ni `namespace`).
6. **Subpath `./fixtures/*` además de los de RF-A1.** A2 importa `fixtures/spike.timeline.json` desde `FightStage.svelte`; la spec pone el archivo en el paquete pero no lista el subpath. Lo agrego (`"./fixtures/*": "./fixtures/*"`), igual que `./placeholders/*`. Es la única desviación de la forma de RF-A1 y la anoto para que el architect la confirme.
7. **Pixi entra en A1 sin usarse.** RF-A2 está dentro de A1 en la spec. Fijo `pixi.js` en `dependencies` del paquete ya en este PR para que la revisión de cadena de suministro, el lockfile y el `COPY` del Dockerfile se validen en el job `images` antes de A2. Ningún archivo de A1 lo importa (la regla de RF-A3 lo prohíbe en los directorios puros, y `src/stage/` no existe todavía).
8. **Dos `tsconfig`.** `tsconfig.json` cubre `src/` con `lib: ["ES2023", "DOM"]` y `types: []` (el núcleo no ve `node:*`, `process` ni `Buffer`); `tsconfig.node.json` cubre `test/` y `scripts/` con `types: ["node"]`. Los dos con `strict` y `noUncheckedIndexedAccess`, como `apps/web`. `DOM` está en `lib` porque A2 lo necesita en `src/stage/`; en los directorios puros, el DOM lo prohíbe ESLint (decisión 10).
9. **Sin literales de color en `src/`.** `check-tokens.mjs` solo recorre `apps/web/src`. En vez de ampliarlo (tiene reglas de Tailwind que aquí no aplican), un test del paquete (`test/no-color-literals.test.ts`) recorre `src/**/*.ts` y falla con `0x` de 6 dígitos hexadecimales, `#rgb[a]`/`#rrggbb[aa]` y funciones de color CSS, salvo en `src/stage/neutral.ts` (que llega en A2).
10. **ESLint del paquete.** `packages/renderer/eslint.config.js` exporta `createConfig()` (como `apps/web`) para que el test la use. En flat config gana el último objeto que coincide por regla, así que cada grupo de archivos recibe la lista **completa** que le toca:
    - Todo el paquete: `no-restricted-globals` con `Date`, `performance` y `crypto`; `no-restricted-properties` con `Math.random`; `no-restricted-syntax` con `MemberExpression[property.name=/^(Date|performance|crypto)$/]` (atrapa `globalThis.performance.now()` y `window.crypto`) y `MemberExpression[property.name=/^(random|getRandomValues|randomUUID)$/][object.name=/^(Math|crypto)$/]`; `@typescript-eslint/no-restricted-imports` con `svelte`, `svelte/*`, `@sveltejs/*`, `#lib`, `#lib/*`, `@torneo/protocol`, `@torneo/protocol/*` y las rutas relativas `**/packages/protocol-ts/**` y `**/apps/web/**`.
    - `src/{manifest,timeline,scene,fx}/**`: lo anterior más `pixi.js` y `pixi.js/*`, y `no-restricted-globals` suma `window`, `document`, `navigator`, `HTMLElement`, `requestAnimationFrame` y `fetch` (núcleo puro, sin DOM).
    - Reglas recomendadas de `@eslint/js` y `typescript-eslint`, y `eslint-config-prettier`. Ningún archivo queda exento de las prohibiciones de azar y reloj: los tests no las necesitan (el reloj se inyecta, el azar es el PRNG y el costo de `sceneAt` se acota contando cues con `cueWindow`, no midiendo tiempo; decisión 2 y paso 7).
11. **Placeholders: el script dibuja, el test compara píxeles.** `scripts/placeholders/draw.ts` es una función pura que devuelve el manifest y los píxeles RGBA de cada hoja; `scripts/gen-placeholders.ts` solo los escribe con `pngjs`. El test de frescura decodifica los PNG commiteados con `pngjs` y compara **píxeles y JSON**, no bytes: la salida de deflate puede cambiar entre versiones del zlib de Node sin que cambie la imagen. Los `manifest.json` generados van a `.prettierignore` (son salida de un script, como `project.inlang`), y el script los escribe con `JSON.stringify(_, null, 2)` y salto de línea final.
12. **Formas de los placeholders.** `dummy_a`: rectángulo alto con cabeza circular, frames de 64×96, color frío. `dummy_b`: trapecio ancho con cabeza cuadrada, frames de 80×88, color cálido. Las 12 animaciones con poses distinguibles por geometría (brazo extendido en el impacto, torso inclinado en `hit`, agachado en `dodge`, horizontal en `knockdown` y `ko`, brazos arriba en `win`). Una hoja por personaje, `sheet-0.png` de 1024×512 como mucho (unos 55 frames caben de sobra). `pixel_art: true`. Los colores viven en `scripts/placeholders/` (datos del dibujo, no del renderer).
13. **El script corre con Node, sin `tsx`.** `node --experimental-strip-types scripts/gen-placeholders.ts` (el flag sigue aceptándose en 22.23.4, donde el *type stripping* ya está activo por defecto, y lo exige 22.17, el mínimo de `engines`). Script del paquete: `gen:placeholders`. No entra en `just check`: lo cubre el test de frescura.
14. **Errores como datos.** `ManifestError` y `TimelineError` son `{ code: string; path: string; data?: Record<string, string | number> }`, con `path` como JSON Pointer (`/animations/attack_light/frame_ms/2`). Los validadores reúnen todos los errores hasta 100 y después cortan con `*.too_many_errors`. Tratan la entrada como no confiable (en B la escribe el convertidor y en local, el owner): solo `Object.keys` de objetos planos, `Object.hasOwn`, `Number.isInteger` y `Number.isFinite`, sin recursión sobre la entrada, y campos desconocidos rechazados (`manifest.unknown_field`, `timeline.unknown_field`), porque la compatibilidad hacia adelante la da `format_version`/`timeline_version`. `validateManifest(json, { expectedId? })` agrega `manifest.id_mismatch` para la regla "igual al nombre de la carpeta", que la función sola no puede ver.
15. **Windup que empieza durante un `knockdown` propio.** La timeline prohíbe golpes dentro de `(at_ms, at_ms + duration_ms]`, pero un golpe del caído 100 ms después de levantarse tiene su windup (hasta 400 ms) empezando dentro del knockdown. La regla 1 de "Coreografía" gana: se ve el `knockdown` hasta su final, y el windup arranca en el frame que corresponde a `t − (at_ms − windup_ms)`, así que el impacto sigue cayendo en `at_ms` y el avance se comprime. Lo cubre un test propio (paso 7).
16. **Estela de HP cerrada.** Como el HP de la timeline v1 solo baja (`damage ≥ 0`), la estela es `hpTrail(t) = hpLineal(t − 450 ms)`, donde `hpLineal` interpola cada escalón en 350 ms: siempre ≥ HP mostrado, función pura y O(log n). Los 450 y 350 ms son constantes de `src/scene/hud.ts` que el owner puede ajustar en la vista previa.

## Versiones fijadas

Consultadas en el registro de npm el **2026-10-27 05:09 UTC** (`pnpm view <paquete> time`, `scripts`, `dependencies`). Todas exactas (`save-exact=true`). Ninguna tiene `preinstall`, `install` ni `postinstall`, ni `gypfile`; `pixi.js` solo declara `prepare` (husky), que pnpm no ejecuta en paquetes del registro. **El paso 1 vuelve a comprobarlas** antes de instalar y anota cualquier cambio en "Desviaciones".

| Paquete | Versión | Publicada | Dónde | Notas |
|---|---|---|---|---|
| `pixi.js` | `8.22.0` | 2026-10-01 | `dependencies` de `@torneo/renderer` | `latest` de la línea 8, 25 días. Tiene `./unsafe-eval` en `exports` (RF-A18). Sin `peerDependencies` ni `optionalDependencies`. MIT |
| `pngjs` | `7.0.0` | 2023-02-20 | `devDependencies` del paquete | JS puro, sin dependencias. La fase B la reutiliza (RF-B1) |
| `@types/pngjs` | `6.0.5` | 2024-05-02 | `devDependencies` | Depende de `@types/node: *` (riesgo 5) |

Transitivas nuevas de `pixi.js@8.22.0` (la versión más alta del rango, que es la que pnpm elegirá, más su fecha): `earcut` 3.2.4 (2026-09-28), `@types/earcut` 3.2.4 (2026-10-09), `@webgpu/types` 0.1.74 (2026-09-19), `@xmldom/xmldom` 0.8.15 (2026-08-21), `parse-svg-path` 0.2.0 (2026-06-02), `eventemitter3` 5.0.4, `tiny-lru` 11.4.7, `@pixi/colord` 2.9.6, `gifuct-js` 2.1.2 → `js-binary-schema-parser` 2.0.3, `ismobilejs` 1.1.1. Todas con más de 3 días y sin scripts de instalación.

Ya están en el lockfile y se reutilizan con la misma versión, como `devDependencies` del paquete: `typescript` 6.0.3, `eslint` 10.13.0, `@eslint/js` 10.0.1, `typescript-eslint` 8.72.0, `eslint-config-prettier` 10.1.8, `globals` 17.13.0, `vitest` 5.0.5 y `@types/node` 22.20.5.

## Archivos que cambian

| Archivo | Cambio |
|---|---|
| `packages/renderer/.gitkeep` | se borra |
| `packages/renderer/package.json` | nuevo: `@torneo/renderer`, `private`, `type: module`, `license: AGPL-3.0-only`, `exports` (`.`, `./manifest`, `./timeline` con `types`/`default` a `src/**/index.ts`; `./placeholders/*` y `./fixtures/*`, decisión 6), `dependencies` (`pixi.js`), `devDependencies` (tabla de versiones) y scripts `typecheck`, `lint`, `test`, `gen:placeholders` |
| `packages/renderer/tsconfig.json`, `tsconfig.node.json` | nuevos (decisiones 5 y 8) |
| `packages/renderer/eslint.config.js` | nuevo (decisión 10), exporta `createConfig()` |
| `packages/renderer/vitest.config.ts` | nuevo: `environment: 'node'`, `include: ['test/**/*.test.ts']` |
| `packages/renderer/README.md` | nuevo, en español: qué es el paquete, subpaths, que el formato normativo está en la spec ("Formato de assets v1" y "Timeline v1"), cómo regenerar los placeholders y por qué no hay `Math.random` ni `Date` |
| `packages/renderer/src/index.ts` | nuevo: reexporta los tipos públicos (`RendererTheme` y `RendererOptions` de ADR-0009 D7 con la extensión de RF-A11, `RendererHandle` como interfaz, sin implementación hasta A2) y los módulos puros |
| `packages/renderer/src/manifest/{index,types,codes,validate,resolve}.ts` | nuevos: tipos del formato v1, lista cerrada de animaciones (`REQUIRED_ANIMATIONS`, `OPTIONAL_ANIMATIONS`, `ANIMATION_FALLBACK`), `validateManifest`, `resolveAnimation`, `windupMs(animation)` (RF-A4, RF-A5) |
| `packages/renderer/src/timeline/{index,types,codes,validate}.ts` | nuevos: tipos de la Timeline v1, `validateTimeline(json, manifests?)` (RF-A7) |
| `packages/renderer/src/fx/{prng,hash,particles,shake,numbers,flash,zoom}.ts` | nuevos: `mulberry32`, `hashSeed(cueIndex, channel)` (FNV-1a de 32 bits sobre enteros, canales como constantes numéricas), y cada efecto como función analítica de `(semilla, τ)` (RF-A10, parte pura de RF-A12) |
| `packages/renderer/src/scene/{index,types,prepare,choreography,camera,hud,banners,scene-at}.ts` | nuevos: `prepareScene`, `sceneAt`, `sceneAtTimeline`, reglas de precedencia, layout de HUD en px CSS (RF-A9, decisiones 2, 4, 15 y 16, parte pura de RF-A13 y RF-A14) |
| `packages/renderer/scripts/placeholders/draw.ts`, `scripts/gen-placeholders.ts` | nuevos (decisiones 11 a 13) |
| `packages/renderer/placeholders/{dummy_a,dummy_b}/{manifest.json,sheet-0.png}` | nuevos, generados y commiteados (RF-A6) |
| `packages/renderer/fixtures/spike.timeline.json` | nuevo, escrito a mano (RF-A8) |
| `packages/renderer/test/**` | nuevos: `manifest.test.ts`, `resolve-animation.test.ts`, `timeline.test.ts`, `prng.test.ts`, `fx.test.ts`, `scene.test.ts`, `choreography.test.ts`, `hud.test.ts`, `reduced-motion.test.ts`, `placeholders.test.ts`, `spike-timeline.test.ts`, `eslint.test.ts`, `no-color-literals.test.ts`; `test/fixtures/manifest/*.json` y `test/fixtures/timeline/*.json` (uno por regla incumplida); `test/support/{png,timeline-builder}.ts` (lector de IHDR y constructor de timelines mínimas para los tests de precedencia) |
| `apps/web/package.json` | modificado: `"@torneo/renderer": "workspace:*"` en `devDependencies` (RF-A1) |
| `apps/web/Dockerfile` | modificado: `COPY packages/renderer/package.json packages/renderer/package.json` antes del `pnpm install` (RF-A1) |
| `apps/web/tests/renderer-package.test.ts` | nuevo: importa `validateManifest` de `@torneo/renderer/manifest` y los dos `manifest.json` de `@torneo/renderer/placeholders/*`, y los valida. Prueba que Vite/Vitest y `svelte-check` (que incluye `tests/`) resuelven los subpaths y los imports `.ts` del paquete con el `tsconfig` de `apps/web`. `tests/` está fuera de la regla de ADR-0009 D4, que solo cubre `src/` |
| `pnpm-lock.yaml` | regenerado: el importer `packages/renderer` y los paquetes nuevos de la tabla |
| `justfile` | modificado: `check-web` y `test` (sección "`just check` en A1") |
| `.prettierignore` | modificado: `packages/renderer/placeholders/*/manifest.json` (decisión 11) |
| `docs/glossary.md` | modificado: *Timeline del renderer* (`Timeline`), *Cue* y *Manifest de sprites* (RF-A20) |
| `docs/architecture.md` | modificado: renderer con timeline propia, escena pura (`prepareScene`/`sceneAt`), formato de assets v1 y adaptador del lado del consumidor (RF-A20). A2 completa la parte de Pixi y la carga diferida |
| `CLAUDE.md` | modificado: en "Comandos", `check-web` y `just test` nombran ESLint, `tsc` y Vitest de `packages/renderer`, y se agrega `pnpm --filter @torneo/renderer gen:placeholders` |

`.github/workflows/ci.yml` y `scripts/check-workflows.mjs` **no cambian**: el filtro `web` ya incluye `packages/**`, `pnpm-lock.yaml` y `justfile`, y el filtro `images` ya incluye `**/Dockerfile` y `pnpm-lock.yaml`, así que este PR dispara `web`, `e2e` e `images` sin tocar la política.

## `just check` en A1

```
check-web: _web-prepare
    pnpm exec prettier --check .
    pnpm --filter web exec eslint .
    pnpm --filter web exec svelte-check --fail-on-warnings
    pnpm --filter web exec vitest run
    pnpm --filter @torneo/renderer exec tsc -p tsconfig.json --noEmit
    pnpm --filter @torneo/renderer exec tsc -p tsconfig.node.json --noEmit
    pnpm --filter @torneo/renderer exec eslint .
    pnpm --filter @torneo/renderer exec vitest run
    pnpm exec vitest run --config scripts/vitest.config.mjs
    node scripts/check-tokens.mjs
    …   (el resto sin cambios)

test: _web-prepare
    pnpm --filter web exec vitest run
    pnpm --filter @torneo/renderer exec vitest run
    pnpm exec vitest run --config scripts/vitest.config.mjs
    …
```

- Va después de los pasos de `apps/web`, como pide RF-A3. Prettier ya cubre `packages/renderer` desde la raíz (`.ts` y `.json`; los `.md` están ignorados).
- El paquete no necesita `_web-prepare`, pero `check-web` ya depende de él.

## Orden de trabajo

Cada paso termina con un comando que lo verifica. Commits `feat(renderer): … (TA-2)`, `build(deps): … (TA-2)` o `docs(002): … (TA-2)`.

1. **Cadena de suministro (RF-A1, RF-A2).** Vuelvo a comprobar fechas y scripts de la tabla. Escribo `packages/renderer/package.json` y el cambio de `apps/web/package.json`, `pnpm install` y después:
   → `pnpm install --frozen-lockfile` pasa con `strictDepBuilds: true`; `pnpm ignored-builds` vacío; `git diff --stat pnpm-lock.yaml` solo agrega el importer y los paquetes de la tabla; `pnpm why @types/node` muestra una sola versión (riesgo 5); `pnpm audit --audit-level moderate` sin hallazgos nuevos. Agrego el `COPY` al Dockerfile y lo pruebo con `just prod-local` (o, si la Mac no tiene emulación a mano, con `docker buildx build -f apps/web/Dockerfile .`). **Punto de corte:** si algo pide scripts de instalación o `audit` reporta algo de severidad moderada o mayor en el árbol de Pixi, paro y el plan vuelve a revisión (RF-A2).
2. **Herramientas del paquete (RF-A3).** `tsconfig*.json`, `eslint.config.js`, `vitest.config.ts`, un `src/index.ts` mínimo, `test/eslint.test.ts`, `test/no-color-literals.test.ts` y los cambios de `justfile` y `.prettierignore`.
   → `just check-web` en verde. `eslint.test.ts` falla, con su regla, en: `Math.random()`, `Date.now()`, `new Date()`, `performance.now()`, `globalThis.performance.now()`, `crypto.getRandomValues(x)`, `import 'svelte'`, `import { x } from '@sveltejs/kit'`, `import x from '#lib/utils.js'`, `import type { X } from '@torneo/protocol'`, `import 'pixi.js'` en `src/scene/x.ts` y en `src/fx/x.ts`, y `document.body` en `src/timeline/x.ts`. Pasa con `import 'pixi.js'` en `src/stage/x.ts`. `no-color-literals.test.ts` tiene un caso que falla (texto con `0xff00aa`) y uno que pasa (`src/stage/neutral.ts`). **Paralelizable con el paso 1** una vez creado `package.json`.
3. **Manifest v1 (RF-A4, RF-A5).** Tipos, códigos y `validateManifest` con todas las reglas de "Diseño → Formato de assets v1", `resolveAnimation` y `windupMs`.
   → `manifest.test.ts`: un manifest mínimo válido pasa; un fixture por regla falla con su código y su `path` (`unsupported_version`, `bad_format`, `bad_id`, `id_mismatch`, `bad_scale`, `sheets_count`, `sheet_file`, `sheet_size`, `frames_count`, `frame_sheet`, `frame_size`, `frame_out_of_sheet`, `pivot_out_of_frame`, `unknown_animation`, `missing_animation`, `animation_frames_count`, `animation_frame_index`, `frame_ms_length`, `frame_ms_range`, `loop_required`, `loop_forbidden`, `animation_too_long`, `impact_frame_missing`, `impact_frame_forbidden`, `impact_frame_range`, `windup_too_long`, `unknown_field`); un JSON con `__proto__` como animación da `unknown_animation` sin tocar prototipos; 101 errores cortan en `too_many_errors`. `resolve-animation.test.ts`: `intro → idle`, `walk → idle`, `knockdown → hit` cuando faltan, y la propia cuando existe. **Paralelizable con los pasos 4 y 6.**
4. **Timeline v1 (RF-A7).** Tipos, códigos y `validateTimeline` con todas las reglas de "Diseño → Timeline v1".
   → `timeline.test.ts`: la timeline de ejemplo de la spec completada pasa; un fixture por regla falla con su código (`unsupported_version`, `bad_duration`, `bad_fighters`, `bad_max_hp`, `too_many_cues`, `unknown_cue_type`, `unknown_field`, `unsorted`, `intro_first`, `intro_gap`, `strike_gap`, `bad_damage`, `dodge_damage`, `hp_mismatch`, `hp_range`, `hp_zero_without_ko`, `knockdown_without_strike` (sin golpe, golpe `block`, golpe sobre el otro luchador y golpe 101 ms antes), `knockdown_duration`, `strike_during_knockdown` (incluido el borde `at_ms + duration_ms`, que cuenta como dentro), `ko_count`, `win_count`, `win_side`, `win_gap`, `cue_after_win`, `duration_too_short`, `unknown_asset` solo cuando se pasan manifests). **Paralelizable con los pasos 3 y 6.**
5. **Placeholders (RF-A6).** `draw.ts`, `gen-placeholders.ts` y la salida commiteada.
   → `pnpm --filter @torneo/renderer gen:placeholders` dos veces seguidas deja `git status` limpio. `placeholders.test.ts`: los dos manifests pasan `validateManifest(_, { expectedId })` con las 12 animaciones; el ancho y alto del IHDR de cada PNG (leídos con `test/support/png.ts`, sin `pngjs`) coinciden con `sheets[]`; todo frame cae dentro de su hoja; los PNG son RGBA de 8 bits (tipo de color 6); y los píxeles decodificados y el JSON coinciden con lo que devuelve `draw()` (decisión 11). Depende del paso 3.
6. **PRNG y efectos puros (RF-A10, parte pura de RF-A12).** `prng.ts`, `hash.ts` y cada efecto como función de `(semilla, τ)`.
   → `prng.test.ts`: dos instancias con la misma semilla dan la misma secuencia de 1 000 valores, todos en `[0, 1)`; `hashSeed` de dos cues distintos, o del mismo cue en dos canales, da secuencias distintas; un vector de referencia fijo (los 5 primeros valores de `mulberry32(hashSeed(0, PARTICLES))`) detecta un cambio involuntario del algoritmo. `fx.test.ts`: shake nulo fuera de `[0, 250] ms` y con amplitud `hit` < `crit` = `special`; números de daño vivos ≤ 900 ms; posición de una partícula en `τ` igual se calcule directo o después de otros `τ`. **Paralelizable con los pasos 3 y 4.**
7. **Escena pura (RF-A9, decisiones 2, 4, 15 y 16, parte pura de RF-A12 a RF-A14).** `prepareScene`, `sceneAt`, coreografía con las reglas de precedencia, cámara, banners y HUD.
   → `scene.test.ts`: dos llamadas con los mismos argumentos dan resultados profundamente iguales; para 200 valores de `t` repartidos en una timeline de prueba, evaluar en orden, al revés y barajado (con el PRNG del paquete) da lo mismo; HP del HUD en `t` igual al `target_hp` del último `strike` con `at_ms ≤ t`; estela ≥ HP y igual a él cuando el último golpe fue hace más de 800 ms; luchadores en sus marcas a 1/3 y 2/3 del ancho en `idle`; el lado `b` en espejo; banner `fight` solo en la intro y `ko` desde el `ko`; ganador y fase (`intro`, `fight`, `ko`, `win`, `finished`).
   → `choreography.test.ts`, un test por regla de precedencia con timelines mínimas de `timeline-builder.ts`, comprobando animación, frame y posición en los instantes del solapamiento: combo del mismo actor a 600 ms (se queda a distancia de contacto, no vuelve a la marca); golpes alternos a 600 ms con `hit`/`block`/`dodge` y recuperación de `attack_heavy` largas (el windup del defensor corta su reacción); impacto recibido durante la recuperación propia (la corta); `knockdown` y `ko` sin interrupción; windup que empieza dentro de un `knockdown` propio (decisión 15: se ve el knockdown hasta su fin y el impacto cae en `at_ms`); el windup empieza `windup_ms` antes del `at_ms` y el frame de impacto está en pantalla en `at_ms` (RF-A11, parte de escena); regreso a la marca en ≤ 300 ms.
   → `fx.test.ts` (sobre la escena del guion del paso 8 y sobre timelines forzadas): ≤ 64 partículas vivas en cualquier `t`, también con una timeline de golpes `special` a 600 ms; zoom ≤ 1.15 y > 1 solo en ventanas de ≤ 300 ms tras un `special` o el `ko`; ≤ 3 destellos que empiezan en cualquier ventana de 1 s.
   → `reduced-motion.test.ts`: para 200 valores de `t`, `fighters` y `hud` iguales con `reducedMotion` `true` y `false`; con `true`, shake 0, zoom 1, 0 partículas, números de daño sin desplazamiento y banners sin tween.
   → `hud.test.ts`: con un `measure` falso (ancho = 9 px × caracteres), a 375×211 y a 1280×720 px las dos barras y los dos nombres no se solapan, la fuente no baja de 14 px y un nombre de 60 caracteres se recorta con `…` hasta caber; con nombres cortos no se recorta.
   → `scene.test.ts` además acota el costo sin reloj: para cada `t` en pasos de 16 ms sobre el guion, y sobre una timeline sintética válida de 1 000 cues (generada en el test con `timeline-builder.ts`), `cueWindow(prepared, t).length ≤ K`, con `K = 16` (constante exportada de `src/scene/prepare.ts`: con ≥ 600 ms entre golpes, en la ventana de 3 s de la animación más larga caben como mucho 6 `strike` más sus `knockdown`, `ko` y `win`, con margen). Si `sceneAt` recorriera la timeline, la timeline de 1 000 cues lo delata de forma determinista; y `sceneAt` solo lee cues a través de `cueWindow` (lo garantiza que la timeline se pasa solo dentro de `PreparedScene` y que `cueWindow` es el único acceso a `cues` en `scene-at.ts`, revisable en el PR).
   Depende de los pasos 3, 4 y 6.
8. **Guion del spike (RF-A8).** `fixtures/spike.timeline.json` a mano, ~64 s y ~34 golpes, con este arco: intro (0–3.2 s); intercambio parejo con ventaja corta de `a` (hasta ~20 s); ráfaga de 3 golpes de `a` a 700–800 ms y `knockdown` de `b` (~26 s); `b` se levanta y toma la delantera con pesados y un `special` (32–46 s: el ganador, `a`, va perdiendo ≥ 5 s después del 50 %); `a` baja de 25 %; **pausa** de ~3 s sin golpes (~43–46 s); remontada de `a`, `b` baja de 25 %; golpe final `special` o `crit` de `a`; `ko` (~57.5 s), `win` (~59.5 s) y pose hasta 64 s. Cada lado usa los tres movimientos y conecta al menos un `crit`; aparecen `hit`, `crit`, `block` (con algún `block` con daño) y `dodge`.
   → `spike-timeline.test.ts`: pasa `validateTimeline` con los manifests de los placeholders; `45000 ≤ duration_ms ≤ 90000`; ≥ 30 `strike`; los cuatro resultados y ≥ 1 `knockdown`; un tramo con HP < 25 % de cada lado; ráfaga de ≥ 3 `strike` seguidos del mismo actor con separaciones ≤ 900 ms; pausa de ≥ 2.5 s sin `strike` entre el primero y el último; intervalo continuo de ≥ 5 s después del 50 % en que la fracción de HP del ganador es menor que la del rival, seguido de una recuperación de la delantera (calculado desde los `target_hp`); el `strike` que deja HP 0 es `crit` o `special`; movimientos y `crit` por lado. Si falla, el mensaje del test incluye la curva de HP por segundo, para corregir el JSON sin adivinar. Depende de los pasos 4 y 5; **paralelizable con el 7**.
9. **Resolución desde `apps/web` (RF-A1).** `apps/web/tests/renderer-package.test.ts`.
   → `pnpm --filter web exec vitest run` y `svelte-check --fail-on-warnings` en verde. **Punto de corte:** si `svelte-check` reporta los imports `.ts` del paquete o no resuelve `./placeholders/*.json`, vuelvo a imports `.js` (decisión 5) y anoto en "Desviaciones" que la fase B necesitará otra forma de cargar `./manifest` desde Node.
10. **Docs (RF-A20, parte de A1).** README del paquete, glosario, `architecture.md` y `CLAUDE.md`. → revisión del PR.
11. **Cierre.** `just check` desde un worktree limpio (`git clean -xdf` y `just install`), `pnpm audit --audit-level moderate` y la lista de paquetes nuevos del lockfile, todo pegado en el PR. `just e2e` también, para confirmar que no cambia nada de 001 (el job `e2e` correrá por el filtro `packages/**`).

## Riesgos

1. **La coreografía tiene huecos que la spec no ve.** Las seis reglas de precedencia cubren los solapamientos declarados, pero no casos como el windup dentro de un knockdown. → Decisión 15 y su test; cualquier otro caso que aparezca se resuelve con la misma regla (el frame del atacante siempre sale de `t − (at_ms − windup_ms)`, así el impacto nunca se mueve), se agrega un test y se anota en "Desviaciones".
2. **`sceneAt` en cada frame: costo y basura.** Una escena nueva por frame con hasta 64 partículas son unos 100 objetos pequeños cada 16 ms. → Índices en `prepareScene` y búsqueda binaria (decisión 2), la cota determinista de `cueWindow` (≤ K cues por llamada, paso 7) y listas de efectos acotadas. El tiempo real por frame se mide en A2 en el dispositivo de referencia (RF-A19), no en CI. Si en A2 el perfil del teléfono muestra pausas de GC, `sceneAt` acepta un `out` reutilizable sin cambiar su semántica (los tests comparan valores, no identidad).
3. **El guion a mano no cumple a la primera.** Con ≥ 30 golpes, HP encadenado y siete restricciones de arco, un error de suma rompe varias aserciones. → Las aserciones son exactas y el mensaje de fallo trae la curva de HP. La forma del arco queda fija en el paso 8; los números exactos se ajustan hasta que pase.
4. **Imports `.ts` desde `apps/web`.** Si `svelte-check` o Vite no aceptan imports con `.ts` en archivos de otro paquete, A2 no compila. → Lo prueba el paso 9 en A1, con punto de corte y un respaldo (`.js`) cuyo costo cae en B y queda escrito.
5. **Segunda versión de `@types/node`.** `@types/pngjs` pide `@types/node: *` y pnpm puede resolver la más nueva. → `@types/node` 22.20.5 como `devDependency` del paquete y `pnpm why @types/node` en el paso 1. Si aun así aparece otra, `pnpm.overrides` en la raíz no es aceptable sin revisión: la quito con `pnpm dedupe` o lo anoto y lo decide el architect.
6. **Pixi instalado y sin usar en A1.** Si A2 descubre que 8.22.0 no sirve (por ejemplo, un worker interno que la CSP bloquea), hay que cambiar la versión. → `plan-a2.md` puede volver a fijarla con las mismas reglas; el costo es un bump del lockfile.
7. **Licencia de los placeholders** ("Preguntas abiertas" 3 de la spec, todavía sin respuesta del owner). → Por defecto AGPL en `packages/renderer/placeholders/`, como recomienda la spec. Si el owner prefiere la licencia de contenido, se mueven a `content/placeholders/` antes de fusionar: cambian el subpath `./placeholders/*`, la ruta del script y los tests, nada más.
8. **Flat config: gana el último objeto.** Una regla definida en dos grupos pierde la del primero sin avisar. → Decisión 10 (listas completas por grupo) y los casos de `eslint.test.ts` en los dos grupos, incluido uno que solo atrapa el grupo puro.
9. **Determinismo de los PNG entre versiones de Node.** → El test compara píxeles, no bytes (decisión 11).

## Alternativas descartadas

- **Un solo plan para A.** Cabe en un PR, pero mezcla la revisión del núcleo puro con la de WebGL, CSP y rendimiento, y alarga la revisión de un diff de miles de líneas.
- **Dejar Pixi para A2.** Retrasa la revisión de cadena de suministro hasta el PR más riesgoso y contradice la división de la spec (RF-A2 en A1).
- **Ampliar `check-tokens.mjs` a `packages/renderer`.** Sus reglas son de Tailwind y CSS; el test propio de colores es más corto y exacto (decisión 9).
- **Comparar los PNG por bytes en el test de frescura.** Frágil ante un cambio del zlib de Node (decisión 11).
- **Placeholders con `prettier` dentro del script.** Obliga a declarar Prettier en el paquete y a depender de su versión para un archivo generado; ignorarlos es lo que ya se hace con `project.inlang`.
- **HUD en coordenadas lógicas.** Ilegible a 375 px (decisión 4).
- **Test de rendimiento con `performance.now()` y umbral en ms.** Necesita una excepción a la regla de reloj y es frágil en CI; la cota de `cueWindow` mide lo mismo (que no hay recorrido O(n) por frame) sin reloj.
- **`Object.freeze` solo en desarrollo.** El núcleo no puede saber en qué modo corre sin acoplarse a Vite o a Node.
- **`sceneAt` con caché interna (`WeakMap`) en lugar de `prepareScene`.** Esconde estado, y el costo de preparación quedaría en el primer frame de la reproducción.
- **`tsx` para el script de placeholders.** Trae `esbuild` con scripts de instalación, que rompe `strictDepBuilds` (la spec ya lo descarta para B).

## Pruebas (cómo se demuestra que funciona)

- [ ] `just check` en verde, con la salida pegada en el PR.
- [ ] **RF-A1:** `apps/web/tests/renderer-package.test.ts` (Vitest y `svelte-check` resuelven los subpaths) y el job `images` en verde con el `COPY` nuevo. `vite build` resolviendo el renderer desde `src/` se prueba en A2, cuando `FightStage.svelte` lo importa.
- [ ] **RF-A2:** `pnpm install --frozen-lockfile` en CI, `pnpm ignored-builds` vacío y la lista de paquetes nuevos del lockfile en el PR.
- [ ] **RF-A3:** `check-web` corre `tsc`, ESLint y Vitest del paquete; `test/eslint.test.ts` con los casos del paso 2.
- [ ] **RF-A4 y RF-A5:** `manifest.test.ts` y `resolve-animation.test.ts`.
- [ ] **RF-A6:** `placeholders.test.ts` y `gen:placeholders` idempotente.
- [ ] **RF-A7:** `timeline.test.ts`.
- [ ] **RF-A8:** `spike-timeline.test.ts`.
- [ ] **RF-A9:** `scene.test.ts` y `choreography.test.ts`.
- [ ] **RF-A10:** `prng.test.ts` y las reglas de azar de `eslint.test.ts`.
- [ ] **Parte pura de RF-A12, RF-A13 y RF-A14:** `fx.test.ts`, `hud.test.ts`, `reduced-motion.test.ts` y `no-color-literals.test.ts`.
- [ ] **RF-A20 (parte de A1):** glosario, `architecture.md`, `CLAUDE.md` y README del paquete, en la revisión del PR.
- [ ] `pnpm audit --audit-level moderate` y `just e2e` en el PR (sin cambios visibles: A1 no tiene UI, así que no hay capturas).

## Estimación de CI

Sin cambios en los workflows. El job `web` suma unos 30–45 s (dos `tsc`, ESLint y Vitest del paquete). Este PR dispara también `e2e` e `images` (por `packages/**`, `pnpm-lock.yaml` y el Dockerfile), con su duración de siempre más la descarga de Pixi en frío (unos 76 MB desempaquetados) la primera vez que cambia el lock.

## Desviaciones durante la implementación

Versiones de la tabla comprobadas de nuevo el 2026-10-26: sin cambios (`pixi.js` 8.22.0 sigue siendo `latest`). `pnpm install --frozen-lockfile` pasa, `pnpm audit --audit-level moderate` sin hallazgos, una sola versión de `@types/node` (22.20.5) y el lockfile solo agrega el importer y los 14 paquetes de la tabla. La etapa `build` de `apps/web/Dockerfile` construye con el `COPY` nuevo (`docker buildx build --target build`).

1. **Coreografía resuelta en `prepareScene`.** Las reglas de precedencia se aplican una vez por carga y producen segmentos por luchador (animación, inicio de animación y movimiento lineal), cada uno asignado al cue que lo causa. Todo segmento empieza como mucho 400 ms antes de su cue y termina como mucho 3 300 ms después (3 000 ms de acción más 300 ms de regreso), así que `cueWindow(prepared, t)` devuelve los cues con `at_ms` en `[t − 3300, t + 400]`, con los 400 ms hacia adelante del windup que pidió el architect, y `sceneAt` busca el segmento activo solo ahí. `ko` y `win` sostienen su animación hasta el final, fuera de la ventana, como dato escalar de `PreparedFighter`. Un test comprueba la cota de cada segmento, y otro, `cueWindow(…).length ≤ K` (`K = 16`; el peor caso válido en 3.7 s es 13). Sin esto, una cadena de interrupciones (una reacción que empieza a mitad de un regreso) dependería de cues fuera de la ventana.
2. **`walk` en el regreso, no en el avance.** El avance ocurre durante el windup con los frames del ataque, para que el impacto caiga en `at_ms` sin mirar más de 400 ms hacia adelante. `walk` (o `idle`, por su fallback) se usa en el regreso a la marca. El `idle` en la marca usa una fase global (`frame(t)`), así no depende de la historia.
3. **Banners en `Scene.banners`, no dentro de `hud`.** La decisión 4 los ponía en el HUD, pero RF-A14 exige `hud` igual con y sin movimiento reducido, y los banners difieren justo en el tween. `hud` lleva solo HP y estela; los banners van aparte, como en la lista de RF-A9.
4. **Códigos de error adicionales.** `manifest.not_object`, `manifest.invalid_type` y `manifest.missing_field`; `timeline.not_object`, `timeline.invalid_type`, `timeline.invalid_value` y `timeline.missing_field`, para la entrada malformada. Y `timeline.cue_after_ko`: solo `win` puede seguir a `ko`. La regla 1 de "Coreografía" dice que la timeline ya prohíbe golpes durante el `ko`, pero "Timeline v1" no lo listaba. Los helpers de entrada no confiable que comparten los dos validadores viven en `src/manifest/input.ts`.
5. **Duración máxima de la timeline: 1 h** (`TIMELINE_LIMITS.maxDurationMs`). La spec no fija una; es una cota de sanidad, y deja caber la timeline sintética de 1 000 cues.
6. **ESLint: `**/protocol-ts/**` en lugar de `**/packages/protocol-ts/**`.** Desde `packages/renderer`, una ruta relativa a `protocol-ts` nunca nombra `packages/`, así que el patrón del plan no la atrapaba. Lo cubre un caso de `eslint.test.ts`.
7. **`tsconfig.node.json` incluye `eslint.config.js`** (`allowJs` y `checkJs`), porque `test/eslint.test.ts` importa `createConfig()`.
8. **`scripts/__fixtures__/check-compose/valid/apps/web/Dockerfile` resincronizado** con el `COPY` nuevo: el test de `check-compose` exige que el fixture válido coincida con el Dockerfile real. No cambia ninguna regla.
9. **Destello con movimiento reducido.** Se mantiene, porque es un tinte del sprite y no un destello de pantalla completa (RF-A14: "el impacto se comunica con el tinte").
10. **Reglas agregadas tras la verificación de QA.** `timeline.knockdown_duplicate`: un solo `knockdown` por `strike` (sin ella, 51 knockdowns tras un golpe pasaban la validación y `cueWindow` llegaba a 54 cues, por encima de `K`). `timeline.ko_without_zero_hp`: un `ko` solo cuando el HP de ese luchador es 0. Además, `sceneAt` lleva un `t` no finito a 0 (`NaN`, `−∞`) o al final (`+∞`), y `prepareScene` copia los `frames` de los manifests antes de congelar, para no congelar los objetos del llamador.

## Revisión

### architect — 2026-10-26 — REQUEST_CHANGES
- Verifiqué:
  - **Contra la spec.** Cada RF de A1 tiene paso, archivo y prueba; las listas de códigos de los pasos 3 y 4 cubren las reglas de "Formato de assets v1" y "Timeline v1"; el guion del paso 8 tiene una aserción por punto de RF-A8; las seis reglas de "Coreografía" tienen su test, y la decisión 15 resuelve el windup dentro de un `knockdown` propio sin mover el impacto, coherente con la regla 1.
  - **Las cuatro desviaciones, aceptadas y admitidas en la spec** (enmienda técnica al final de `spec.md`): `./fixtures/*` en RF-A1; `prepareScene` + `sceneAt` en RF-A9 (preparación inmutable, sin caché oculta: mejor que un `WeakMap`); HUD en px CSS fuera de la cámara en RF-A13 (el argumento de los 8 px a 375 de ancho es correcto); y A1 con la parte pura de RF-A12 a RF-A14, que deja a A2 solo la capa Pixi y los riesgos de integración.
  - **Placeholders bajo AGPL** en `packages/renderer/placeholders/`, como recomienda la spec ("Preguntas abiertas" 3); el riesgo 7 deja escrito el costo de moverlos.
  - **Límites.** No toca `crates/**` ni `packages/protocol-ts/**`; ESLint prohíbe `@torneo/protocol`, Svelte, `#lib` y Pixi en el núcleo, y el PRNG cosmético no recibe la semilla de la pelea (ADR-0002). Sin texto en la escena: errores como códigos con JSON Pointer y colores por rol (ADR-0007, ADR-0009 D7).
  - **Cadena de suministro.** Versiones exactas con fecha, ≥ 3 días, sin scripts de instalación; `strictDepBuilds` y `onlyBuiltDependencies: []` intactos; punto de corte con `audit`. El `COPY` del manifest en `apps/web/Dockerfile` es el que falta (el `COPY packages packages` posterior ya trae el fuente); los filtros `web`, `e2e` e `images` de CI ya cubren el diff.
  - **`just check`.** Dos `tsc`, ESLint y Vitest del paquete después de los pasos de `apps/web`, y Vitest en `just test` (RF-A3). `apps/web/tests/` ya está en el `include` de Vitest y del `tsconfig` de web, así que el paso 9 sí prueba la resolución.
- Cambios pedidos:
  1. **El test de rendimiento del paso 7 contradice la decisión 10.** Medir "10 000 llamadas, promedio ≤ 0.5 ms" necesita `performance.now()` (o `Date`), que ESLint prohíbe en todo el paquete, y la decisión 10 dice que ningún archivo queda exento. Además, un umbral de tiempo en CI es frágil. Elige una opción y escríbela en la decisión 10 y en el paso 7: (a) **preferida:** sustituir la medición por una cota determinista, por ejemplo una función pura interna (`cueWindow(prepared, t)`) que devuelva los cues que `sceneAt` mira, y un test que compruebe que su tamaño es ≤ K para todo `t` del guion y de una timeline sintética de 1 000 cues; o (b) un único archivo `test/scene.perf.test.ts` con una excepción de ESLint acotada a `performance` en ese archivo, más un caso en `eslint.test.ts` que demuestre que `performance.now()` sigue fallando en cualquier otro test.
  2. **"`Object.freeze` en desarrollo" (decisión 2) no tiene cómo saber que está en desarrollo.** El núcleo tiene `types: []` (sin `process`) y tiene que seguir siendo agnóstico de Vite (`import.meta.env`), porque en la fase B lo carga Node. Congela siempre (es una vez por `load` y cuesta poco) o no congeles nunca y confía en los tipos `readonly`. Escribe cuál.
- Recomendación (no bloquea): en la decisión 4, di que `Scene.hud` lleva solo valores (HP mostrado, estela y fase de los banners) y que `layoutHud(viewport, measure, names)` es una función pura aparte que llama la capa Pixi. Así queda claro que `sceneAt` no depende del tamaño del contenedor y que el test de RF-A14 compara el HUD sin el layout.

### frontend-engineer — 2026-10-26 — RESPUESTA
- **Cambio 1 (test de rendimiento).** Tomé la opción (a). Quité la medición con reloj del paso 7; en su lugar, `cueWindow(prepared, t)` (función pura interna que `sceneAt` usa para leer cues) y un test que comprueba `cueWindow(prepared, t).length ≤ K` (`K = 16`, con su justificación) para cada `t` cada 16 ms del guion y de una timeline sintética válida de 1 000 cues. La decisión 10 sigue sin excepciones y lo dice; la decisión 2, el riesgo 2 y "Alternativas descartadas" lo recogen.
- **Cambio 2 (`Object.freeze`).** Congelo **siempre**, en profundidad, una vez por `load` (decisión 2), además de los tipos `readonly`. Lo anoto en "Alternativas descartadas".
- **Recomendación.** Tomada en la decisión 4: `Scene.hud` lleva solo valores (HP, estela, fase de los banners) y `layoutHud(viewport, measure, names)` es una función pura aparte que llama la capa Pixi; el test de RF-A14 compara `hud` sin layout. También cité la enmienda técnica de la spec en lugar de "la spec no fija esto".
- `status: in-review`.

### architect — 2026-10-26 — APPROVE (segunda revisión)
- Revisé solo lo afectado:
  - **Cambio 1.** La decisión 10 sigue sin excepciones de reloj y lo dice. El paso 7 reemplaza la medición por la cota determinista `cueWindow(prepared, t).length ≤ K` sobre el guion y sobre una timeline sintética de 1 000 cues, que delata un recorrido O(n). La medición de tiempo real pasa a A2 en el dispositivo de referencia (riesgo 2). Comprobé `K = 16`: con ≥ 600 ms entre impactos, la ventana de 3 s hacia atrás más los 400 ms del windup hacia adelante admite como mucho 6 `strike`, sus `knockdown`, más `intro`, `ko` y `win`, o sea ≤ 15. Al implementar, la ventana de `cueWindow` tiene que incluir esos 400 ms hacia adelante.
  - **Cambio 2.** `Object.freeze` profundo siempre, una vez por `load`, con el motivo escrito. Resuelto.
  - **Recomendación.** Tomada en la decisión 4: `Scene.hud` lleva solo valores y `layoutHud(viewport, measure, names)` es una función aparte.
- Sin cambios pedidos. Soy el único reviewer, así que `status: approved`.
