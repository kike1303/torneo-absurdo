---
id: "002"
title: Spike de renderizado de peleas
type: spec
status: done
risk: medium
author: architect
reviewers: [product-owner, game-designer]
approvals:
  - by: game-designer
    date: 2026-10-26
  - by: product-owner
    date: 2026-10-26
linear: TA-2
depends_on: ["001"]
created: 2026-10-26
updated: 2026-10-27
---

# Spec: Spike de renderizado de peleas

> Intent: [intent.md](./intent.md) · ADRs: [0003](../../decisions/0003-stack-tecnologico.md), [0004](../../decisions/0004-personajes-homenaje.md), [0007](../../decisions/0007-i18n-desde-cero.md), [0009](../../decisions/0009-ui-design-system.md) (D4, D6 y D7), [0012](../../decisions/0012-logs-y-cabeceras-de-seguridad.md) (B3, B4.2, B4.5 y B6.1) · Dependencia blanda: [003](../003-motor-de-pelea/intent.md) · La consume: [004](../004-pipeline-de-assets/intent.md)

## Resumen
`packages/renderer` (`@torneo/renderer`) reproduce con PixiJS 8 una **línea de tiempo propia del renderer** (`Timeline`, una lista de *cues*), no el `FightLog` del protocolo. Se fija un **formato de assets versionado** (spritesheet PNG + manifest JSON), dos placeholders originales que lo usan, una ruta de prueba `/[lang]/dev/arena` cargada de forma diferida, y un convertidor **solo local** de MUGEN (`tools/sff-converter`, TS) que escribe en `content/_local/`. El owner decide go/no-go con la demo.

**Fases (un PR cada una, con su `plan-<fase>.md`):**
- **A — renderer y ruta de prueba** (RF-A1 a RF-A20). No depende de nada más que de 001. Si el diff es grande, su plan puede partirla en **A1** (paquete, formato, timeline, escena pura, placeholders; RF-A1 a RF-A10, más la parte pura de RF-A12 a RF-A14 que `sceneAt` describe y la parte de RF-A20 que le toca) y **A2** (Pixi, efectos, HUD, ruta, CSP, medición; RF-A11 a RF-A20).
- **B — convertidor de MUGEN y servido local** (RF-B1 a RF-B9). Depende de A (usa su validador de manifests). Cierra con el **go/no-go del owner** (RF-B9).
- **C — adaptador `FightLog → Timeline`** (RF-C1 a RF-C4). **Bloqueada por 003**: empieza cuando `FightLog`/`FightEvent` estén fusionados en `crates/protocol` y generados en `packages/protocol-ts`. **C se queda en 002** (decisión de coordinación del 2026-10-26, ver "Preguntas abiertas" 2): la feature pasa a `done` al fusionar C. El go/no-go (RF-B9) no espera a C.

**Vista previa tras A.** Al fusionar A (o A2), el owner ve la demo con placeholders, sin esperar a B, para descubrir antes el riesgo visual (recomendación 3 del product-owner). No es el go/no-go, pero si algo no funciona, se corrige antes de B.

**Dependencia con 003.** 002 **no define ni edita** tipos en `crates/protocol` ni en `packages/protocol-ts`. La única pieza que conoce la forma de `FightLog` es el adaptador de la fase C, que vive en `apps/web` (lado del consumidor). Todo lo demás se construye ya contra la `Timeline` de esta spec. A cambio, esta spec le entrega a 003 datos de temporización ("Diseño → Datos para 003").

## Requisitos funcionales

Cada requisito dice cómo se verifica (→). "check" es `just check`; "e2e" es `just e2e` (Playwright contra `node build`, instancia con catálogo salvo que se diga otra cosa); "PR" es evidencia pegada en la descripción del PR de la fase.

### Fase A — renderer y ruta de prueba

**Paquete y cadena de suministro**
- **RF-A1. Paquete `@torneo/renderer`** en `packages/renderer/` (ya dentro de `packages/*` del workspace): `private: true`, `type: module`, `license: AGPL-3.0-only`. Exporta el **TS fuente sin paso de build**, igual que `@torneo/protocol`, con estos subpaths: `.` (API con Pixi), `./manifest` y `./timeline` (módulos **puros**, sin Pixi ni DOM, que importan el convertidor y los tests) `./placeholders/*` (archivos de RF-A6) y `./fixtures/*` (el guion de RF-A8, que importa la ruta de prueba). `apps/web` lo declara como `"@torneo/renderer": "workspace:*"` en `devDependencies` (como todo lo que Vite empaqueta). `apps/web/Dockerfile` copia `packages/renderer/package.json` antes del `pnpm install` (si no, `--frozen-lockfile --filter 'web...'` falla). → check (`svelte-check` y `vite build` resuelven el paquete) y job `images` de CI.
- **RF-A2. PixiJS fijado.** `pixi.js` 8.x con versión **exacta** (sin `^`), la estable más reciente que tenga al menos 3 días al escribir el plan (la regla del repo es `minimumReleaseAge: 1440`; uso 3 días por coherencia con Dependabot y `crate-age`). `onlyBuiltDependencies` sigue en `[]` y `strictDepBuilds: true` no cambia: si Pixi o alguna dependencia transitiva necesita scripts de instalación, `pnpm install` falla y el plan vuelve a revisión. Se usa el renderer **WebGL** (`preference: 'webgl'`); WebGPU queda fuera del spike. → CI (`pnpm install --frozen-lockfile`) y PR (lista de paquetes nuevos del lockfile).
- **RF-A3. Cobertura de `just check`.** `check-web` agrega, después de los pasos de `apps/web`: `tsc --noEmit`, ESLint y Vitest de `@torneo/renderer` (`pnpm --filter @torneo/renderer exec …`). `just test` agrega su Vitest. `packages/renderer/eslint.config.js` (typescript-eslint, mismas versiones que `apps/web`) prohíbe en todo el paquete:
  - `Math.random`, `crypto.getRandomValues`, `Date` y `performance.now` (`no-restricted-properties` y `no-restricted-globals`): el tiempo entra por el reloj inyectado (RF-A11) y el azar por el PRNG cosmético (RF-A10);
  - imports de `svelte`, `@sveltejs/*`, `#lib/*` y `@torneo/protocol` (agnóstico de framework y desacoplado de 003);
  - imports de `pixi.js` en `src/manifest/**`, `src/timeline/**`, `src/scene/**` y `src/fx/**` (núcleo puro).

  → check, con fixtures de ESLint que fallan (`Math.random`, `Date.now()`, `import 'svelte'`, `import 'pixi.js'` en `src/scene/`).

**Formato de assets (versionado; lo consume 004)**
- **RF-A4. Manifest de sprites v1.** Cada personaje es una carpeta con `manifest.json` y una o más hojas `sheet-<n>.png`. La especificación normativa está en "Diseño → Formato de assets v1". `@torneo/renderer/manifest` exporta los tipos y `validateManifest(json: unknown): { ok: true; manifest } | { ok: false; errors: ManifestError[] }`, puro, con errores como **códigos** (`manifest.unsupported_version`, `manifest.missing_animation`, …) más la ruta JSON del campo, nunca texto. El renderer rechaza al cargar un manifest inválido o de otra `format_version`. → Vitest: un manifest válido mínimo pasa; un fixture por regla incumplida falla con su código.
- **RF-A5. Animaciones estándar.** Obligatorias: `idle`, `attack_light`, `attack_heavy`, `special`, `hit`, `block`, `dodge`, `ko`, `win`. Opcionales, con fallback fijado **en el formato** (no por manifest): `intro → idle`, `walk → idle`, `knockdown → hit`. Un nombre fuera de esa lista es error. `attack_light`, `attack_heavy` y `special` declaran `impact_frame` y su *windup* (suma de `frame_ms` antes del frame de impacto) es ≤ 400 ms. → Vitest (fixtures de RF-A4) y un test de la función `resolveAnimation(manifest, code)` que comprueba cada fallback.
- **RF-A6. Placeholders originales.** Dos personajes de prueba, `dummy_a` y `dummy_b`, en `packages/renderer/placeholders/<id>/`: figuras geométricas simples, distintas en silueta y color, con las 12 animaciones (obligatorias y opcionales). Se generan con un script determinista `packages/renderer/scripts/gen-placeholders.ts` (con `pngjs`, ver RF-B1) y la salida **se commitea**. No son personajes ni marca: son fixtures del renderer, bajo AGPL (ver "Áreas de preocupación" 8). → Vitest: los dos manifests pasan `validateManifest`; el ancho y alto de cada PNG (leídos del IHDR) coinciden con `sheets[]`; todo frame cae dentro de su hoja.

**Línea de tiempo (forma propia del renderer)**
- **RF-A7. `Timeline` v1.** Forma normativa en "Diseño → Timeline v1". `@torneo/renderer/timeline` exporta los tipos y `validateTimeline(json, manifests?)`, puro, con errores como códigos (`timeline.unsorted`, `timeline.hp_mismatch`, `timeline.strike_gap`, `timeline.knockdown_without_strike`, …). Sin texto para humanos: acciones, resultados y lados son códigos (ADR-0007). → Vitest con un fixture por regla.
- **RF-A8. Guion del spike.** `packages/renderer/fixtures/spike.timeline.json`, escrito a mano. Tiene que contar una pelea con historia y con dos ritmos, para que el owner juzgue el renderer y no un guion plano:
  - **Duración:** `duration_ms` **total** (intro y pose de victoria incluidas) entre 45 y 90 s.
  - **Cobertura:** intro, los cuatro resultados (`hit`, `crit`, `block`, `dodge`), al menos un `knockdown`, un tramo con HP < 25 % de cada lado, KO y pose de victoria.
  - **Densidad:** al menos **30 `strike`**.
  - **Ritmo:** al menos una **ráfaga** de ≥ 3 `strike` seguidos del mismo actor con separaciones ≤ 900 ms, y al menos una **pausa** de ≥ 2.5 s sin `strike` entre el primero y el último.
  - **Remontada:** después del 50 % de `duration_ms`, el ganador va perdiendo (su fracción `hp / max_hp` es menor que la del rival) durante un intervalo continuo de ≥ 5 s, y después recupera la delantera.
  - **Golpe final:** el `strike` que deja HP 0 es `crit` o `special`.
  - **Variedad por luchador:** cada lado usa los tres movimientos (`attack_light`, `attack_heavy`, `special`) y conecta al menos un `crit`.

  → Vitest sobre el fixture: pasa `validateTimeline` con los manifests de los placeholders, `45000 ≤ duration_ms ≤ 90000`, y una aserción por cada punto anterior (conteo, ráfaga, pausa, intervalo de remontada calculado desde los `target_hp`, resultado del golpe final, movimientos y `crit` por lado).

**Escena pura y determinismo**
- **RF-A9. La escena es función pura del tiempo.** `sceneAt(timeline, manifests, t, { reducedMotion })` (en `src/scene/`) devuelve una descripción de la escena en el instante `t` (ms desde el inicio de la timeline): por luchador, animación, índice de frame, desplazamiento y espejo; HUD (HP mostrado y estela de HP perdido); cámara (zoom y offset); banners activos; y la lista de efectos vivos con sus parámetros. El plan puede partirla en una preparación pura por `load` (`prepareScene(timeline, manifests)`, inmutable) y `sceneAt(prepared, t, opts)`, siempre que la composición sea equivalente a la firma anterior y los tests la usen. La capa Pixi (`src/stage/`) solo aplica esa descripción a sus objetos; **no acumula estado por frame**. Consecuencias exigidas:
  - mismos argumentos → resultado profundamente igual;
  - `seek(t)` muestra lo mismo que reproducir hasta `t` (no hay simulación incremental de partículas: posición = f(semilla, t));
  - una caída de frames o una pestaña en segundo plano no desincroniza: al volver, la escena es la de `t` actual.

  → Vitest: igualdad entre dos llamadas; para 200 valores de `t` repartidos en el guion, `sceneAt` no depende del orden de evaluación; HP del HUD en `t` igual al `target_hp` del último `strike` con `at_ms ≤ t` (más la estela, que se mide aparte); y un test por cada regla de precedencia de "Diseño → Coreografía" (timelines mínimas que fuerzan cada solapamiento: combo del mismo actor a 600 ms, golpes alternos a 600 ms con reacciones y recuperaciones largas, impacto durante la recuperación propia, `knockdown` y `ko` sin interrupción), comprobando la animación, el frame y la posición en los instantes del solapamiento.
- **RF-A10. Aleatoriedad cosmética con semilla local.** Dispersión de números de daño, partículas y patrón del shake usan un PRNG del renderer (`src/fx/prng.ts`, p. ej. mulberry32) sembrado con `hash(índice del cue, canal)`. Nunca `Math.random` (RF-A3). Ese PRNG **no es el del motor ni recibe la semilla de la pelea** (ADR-0002): todos los espectadores y las repeticiones ven lo mismo porque derivan de la timeline. → Vitest: dos instancias producen la misma secuencia; dos cues distintos, secuencias distintas; ESLint (RF-A3).

**Reproducción con Pixi**
- **RF-A11. API y reloj.** `createRenderer(container: HTMLElement, options: RendererOptions)` devuelve un handle con `load(timeline, assets)`, `play()`, `pause()`, `seek(ms)`, `setTheme(theme)`, `setReducedMotion(flag)`, `onCue(cb)`, `onState(cb)`, `snapshot()`, `getFrameStats()` y `destroy()`. `RendererOptions` extiende el de ADR-0009 D7 (`theme`, `reducedMotion`) con:
  - `clock: () => number`, la hora en ms de la base de tiempo de la timeline. El host la provee: en el spike, `performance.now()` menos el inicio; en vivo, el reloj del servidor con su offset. El renderer **nunca** avanza la timeline contando frames ni deltas del ticker;
  - `labels: { names: Record<'a' | 'b', string>; banners: Record<'fight' | 'ko', string> }`, ya traducidos por el host;
  - `formatNumber: (n: number) => string` para los números de daño (`Intl` vía `lib/i18n/format.ts`, ADR-0009 D8);
  - `resolveAsset: (assetId: string, file: string) => string`, que da la URL de cada archivo del manifest (Vite pone hash a los nombres, así que no se resuelven por ruta relativa).

  - `onState(cb)` emite `'ready' | 'playing' | 'paused' | 'finished'` (`finished` cuando `t` llega a `duration_ms`); `snapshot()` devuelve `{ t, phase, winner, hp: { a, b }, lastCue }` de la última escena aplicada, con códigos y números, sin texto. El host los usa para sus estados y sus `data-*` sin recalcular HP;
  - la inicialización y `load` rechazan con un `RendererError` cuyo `code` es `'webgl_unavailable' | 'asset_invalid' | 'asset_load_failed' | 'timeline_invalid'` (los estados de error de RF-A16); `load` valida con `validateTimeline` y `validateManifest` antes de tocar Pixi.

  `onCue` se dispara una vez por cue cuando el reloj pasa su `at_ms` durante la reproducción, en orden (un salto del reloj, como volver de una pestaña oculta, dispara todos los del intervalo); un `seek` no dispara los cues que salta. Los ataques empiezan su windup `windup_ms` **antes** del `at_ms` del cue (el impacto cae en el `at_ms`), así que el renderer mira hasta 400 ms hacia adelante (ver "Áreas de preocupación" 2). → Vitest con reloj falso para `onCue` y el windup sobre `sceneAt`; e2e para el ciclo de vida.
- **RF-A12. Efectos.** Con `reducedMotion: false`:
  - **screen shake** en `hit` (leve), `crit` y `special` (fuerte) y KO, ≤ 250 ms;
  - **números de daño** que suben y se desvanecen (≤ 900 ms), con dispersión horizontal del PRNG; en `crit`, más grandes y con `theme.colors.crit`;
  - **partículas** de impacto (≤ 64 vivas en total, tope duro) en `hit`, `crit` y `special`;
  - **destello del sprite golpeado** (tinte, no pantalla completa) y nunca más de 3 destellos por segundo (ADR-0009 D6);
  - **punch zoom** en `special` y KO: ≤ 1.15× durante ≤ 300 ms, centrado entre los dos luchadores.

  Los colores salen solo de `RendererTheme` (ningún literal de color en `packages/renderer/src`, salvo el blanco/negro neutro del tinte, que el plan justifica). → Vitest sobre `sceneAt` (tope de partículas, duraciones, zoom máximo, ≤ 3 destellos en cualquier ventana de 1 s del guion); revisión visual del owner.
- **RF-A13. HUD.** Barras de HP de los dos lados con estela de HP perdido (`hp` y `hpLost` del tema), nombres de `labels.names` y banners `fight` (intro) y `ko` de `labels.banners`. El renderer **no contiene ni dibuja ningún otro texto**; los números se formatean con `formatNumber`. No dibuja texto hasta cargar las fuentes del tema (ADR-0009 D7). A 375 px las barras y los nombres no se solapan (los nombres se recortan con elipsis medida por ancho). El HUD se maqueta en px CSS del contenedor, fuera de la cámara lógica de RF-A15 (no lo mueven el shake ni el zoom), con fuente mínima de 14 px CSS; luchadores, efectos y números de daño siguen en coordenadas lógicas. → Vitest (layout del HUD con anchos de texto falsos) y captura e2e a 375×812.
- **RF-A14. Movimiento reducido** (ADR-0009 D7, que manda sobre la condición 7 del intent): sin shake, sin punch zoom, sin flashes de pantalla completa, **0 partículas**, tweens ≤ 150 ms o instantáneos; el impacto se comunica con el tinte y el número de daño estático. **La temporización no cambia:** mismos `at_ms`, mismas animaciones y frames de los luchadores, mismo HP. → Vitest (es el test 7 de ADR-0009, sobre la timeline): con reloj falso, la secuencia `(índice de cue, hora de disparo)` de `onCue` es idéntica con `true` y `false`; para 200 valores de `t`, los luchadores y el HUD de `sceneAt` son iguales y solo difieren cámara y efectos.
- **RF-A15. Encuadre.** Escena lógica de 640×360, cámara **fija** que contiene a los dos luchadores, escalada al ancho del contenedor (letterbox). Personajes mirando a la derecha en el atlas; el lado `b` se dibuja en espejo. Con `pixel_art: true`, filtrado `nearest`. `resolution = min(devicePixelRatio, 2)`, `antialias: false`. → e2e: a 375×812 y 1280×800 el canvas cabe en el ancho sin scroll horizontal.

**Ruta de prueba en `apps/web`**
- **RF-A16. Ruta `/[lang]/dev/arena`** (`src/routes/dev/arena/+page.svelte`), detrás del guard de `/dev/**` de 001 (RF-A16 de 001): 404 sin `ENABLE_DEV_CATALOG=1`.
  - `src/lib/components/fight/FightStage.svelte` es el **único** archivo que importa valores del renderer (ADR-0009 D4, regla de ESLint ya existente) y lo hace con `import()` dinámico en `onMount`. `src/lib/components/fight/renderer-theme.ts` construye el `RendererTheme` según ADR-0009 D7 (`culori/fn`).
  - Controles (en `src/lib/components/fight/ArenaControls.svelte`): reproducir/pausar, reiniciar, selector de personaje por lado (placeholders y, en la fase B, los locales), un interruptor "simular movimiento reducido" (además de seguir a `prefers-reduced-motion`) y otro de estadísticas (RF-A19). Objetivos táctiles ≥ 44 px.
  - Estados: `loading` (skeleton), `error` (código → mensaje: `webgl_unavailable`, `asset_invalid`, `asset_load_failed`, `timeline_invalid`), `ready`, `playing`, `paused`, `finished` (con el ganador). `FightStage` refleja el estado en `data-state` y, solo en esta ruta, HP y último cue en `data-*` para los e2e.
  - **i18n:** `src/routes/dev/**` está exento de `check-i18n` (ADR-0009 D5), así que la página **solo compone** componentes de `lib/components/fight/` y no tiene texto propio; todo texto visible vive en esos componentes, que `check-i18n` sí revisa, con mensajes `arena_*` en `messages/{es,en}.json`. El canvas lleva `role="img"` y un `aria-label` de Paraglide.

  → e2e: `/es/dev/arena` y `/en/dev/arena` llegan a `data-state="playing"`; con `seek` al final (parámetro `?t=<ms>`, solo en esta ruta) llegan a `finished`; la instancia sin catálogo da 404; axe sin violaciones `serious` ni `critical`; sin errores en consola. check (`check-i18n`).
- **RF-A17. Carga diferida.** Pixi y el renderer solo entran en chunks que se cargan con `import()` desde `FightStage.svelte`. `scripts/check-renderer-split.mjs` lee el manifest del build de cliente (`build.manifest: true` o el de Kit) y falla si algún módulo de `pixi.js` o de `packages/renderer` es alcanzable por imports **estáticos** desde cualquier entrada o nodo de ruta, **incluido el de `dev/arena`** (allí solo por `import()`, como pide RF-A16). Exige además el caso positivo: el nodo de `dev/arena` alcanza `pixi.js` por imports dinámicos; si no lo encuentra, falla (así no pasa en vacío si cambia el formato del manifest). Corre en `just e2e` justo después del build (y por eso en el job `e2e` de CI). → ese script, con fixtures de manifest que pasan y fallan (Pixi estático en el layout raíz, el renderer estático en el nodo de `dev/arena`, sin el import dinámico) en Vitest de `scripts/`.
- **RF-A18. CSP sin cambios.** La CSP de `vite.config.ts` (RF-A10 de 001) **no se modifica** en 002 (ADR-0012 B4.2). Las condiciones de ADR-0012 (trabajo derivado 9) para el renderer:
  - importa `pixi.js/unsafe-eval` **en la entrada de `packages/renderer`** (`src/index.ts`), antes de crear cualquier renderer (generación de shaders y sincronización de uniforms sin `new Function`);
  - desactiva los workers de carga de texturas (`Assets.setPreferences({ preferWorkers: false })`), que Pixi crea desde URLs `blob:`;
  - no usa texturas KTX2 ni Basis (ni ningún otro cargador de Pixi con workers);
  - carga imágenes solo del mismo origen (`img-src 'self' data: blob:` ya cubre lo que Pixi genere);
  - la ruta del renderer entra en `csp.spec.ts` (ADR-0012 B6.1).

  Si aun así hace falta tocar la CSP, se detiene la fase y el plan vuelve a revisión con security-reviewer: ninguna ampliación de B4 cubre a Pixi, así que hace falta un ADR que reemplace a ADR-0012 (B4.5) antes de tocarla. → e2e en Chromium **y WebKit**: cero eventos `securitypolicyviolation` en `/es/dev/arena` durante la carga y 5 s de reproducción.
- **RF-A19. Medición de rendimiento.** `getFrameStats()` devuelve, desde el primer `strike` hasta el cue `win`, el número de frames, FPS medio y los percentiles 50, 95 y 99 del tiempo de frame (intervalo entre callbacks del ticker de Pixi) y cuántos frames pasan de 33 ms. Con el interruptor de estadísticas, la ruta muestra esos valores (etiquetas de Paraglide) y un botón para copiar el JSON. Método y umbrales en "Requisitos no funcionales → Rendimiento". → Vitest del cálculo de percentiles; e2e: tras `seek` a un punto intermedio y 3 s de reproducción, `frames > 0`. El umbral **no** se comprueba en CI (CI no tiene GPU); se comprueba en el dispositivo de referencia (PR).
- **RF-A20. Docs.** El PR de A actualiza `docs/architecture.md` (renderer: timeline propia, escena pura, formato de assets, adaptador del lado del consumidor), agrega a `docs/glossary.md` *Timeline del renderer* (`Timeline`), *Cue* y *Manifest de sprites*, y si agrega comandos, la sección "Comandos" de `CLAUDE.md`. → revisión del PR.

### Fase B — convertidor de MUGEN y servido local

- **RF-B1. Paquete `@torneo/sff-converter`** en `tools/sff-converter/`. `pnpm-workspace.yaml` agrega `tools/*`. Corre con el *type stripping* de Node 22 (sin `tsx` ni `esbuild`, que traen scripts de instalación): `tsconfig` con `erasableSyntaxOnly: true` y `allowImportingTsExtensions`. Única dependencia de runtime: `pngjs` (JS puro, versión exacta), más `@types/pngjs`; depende de `@torneo/renderer` (`workspace:*`) solo por `./manifest`. No es dependencia de `apps/web` ni entra en ninguna imagen (el Dockerfile de `web` copia como mucho su `package.json`, si pnpm lo exige para `--frozen-lockfile`; el plan lo verifica en el job `images`). El filtro `web` de `ci.yml` agrega `tools/**`. `check-web` y `just test` agregan su `tsc --noEmit`, ESLint (mismas prohibiciones de azar y reloj que RF-A3, salvo en `src/cli.ts` y en `test/support/elapsed.ts`, que solo puede usar `performance` para medir el límite de tiempo de RF-B4) y Vitest. → check y CI.
- **RF-B2. Uso.** `pnpm --filter @torneo/sff-converter convert -- --sff <ruta> --air <ruta> --map <ruta> [--out <dir>]`. El **mapa** (JSON, lo escribe el owner y vive junto a los archivos de MUGEN en `content/_local/`) dice qué acción de MUGEN es cada animación nuestra (`{ "id": "local_1", "actions": { "idle": 0, "attack_light": 200, … }, "impact_frames": { "attack_light": 2, … }, "scale": 1 }`); trae por defecto las acciones estándar de MUGEN (`idle: 0`, `walk: 20`, `hit: 5000`, `knockdown: 5050`, `intro: 190`, `win: 180`) y obliga a mapear las que dependen del personaje (ataques, `block`, `dodge`, `ko`). Los mensajes de error de la CLI son códigos con sus datos (es una herramienta de desarrollo, pero se mantiene la regla). Un subcomando `inspect --sff <ruta> [--air <ruta>] [--map <ruta>]` lee solo cabeceras, nodos y el AIR, sin decodificar píxeles, y reporta (como JSON sin rutas) la versión, los sprites por método de compresión y, con el mapa, las acciones que faltan y los sprites no soportados que se usarían. → Vitest del parseo del mapa.
- **RF-B3. Formatos.** SFF v1 (PCX de 8 bits con RLE y paletas compartidas o propias) y SFF v2 (raw, RLE8, LZ5 y PNG de 8, 24 y 32 bits). RLE5 y cualquier otro método se reportan como `sff.unsupported_compression` con el índice del sprite, sin abortar la conversión si ninguna animación mapeada lo usa. AIR: bloques `[Begin Action N]`, frames `grupo, imagen, x, y, ticks[, flip]`, `Loopstart`; se ignoran `Clsn*`. → Vitest con fixtures sintéticos de cada caso.
- **RF-B4. Entrada no confiable.** Toda lectura pasa por un lector con límites que lanza `ConverterError` con código ante cualquier acceso fuera de rango. Límites: SFF ≤ 64 MiB, AIR ≤ 1 MiB, ≤ 10 000 sprites, sprites de ≤ 1024×1024, salida descomprimida acotada a `ancho × alto` (los decodificadores RLE/LZ5 nunca escriben más allá), offsets y longitudes validados antes de leer, paletas acotadas a 256 colores: en SFF v1, exactamente 256 (768 bytes RGB); en SFF v2, `numcols` de 1 a 256 con datos de exactamente `numcols × 4` bytes, completadas hasta 256 con negro transparente; un índice de píxel ≥ `numcols` es error (`sff.palette_index_out_of_range`). Nada de recursión sobre datos de entrada. → Vitest: archivos truncados en cada byte de la cabecera y del primer sprite, y 2 000 mutaciones con un PRNG de semilla fija sobre los fixtures sintéticos: solo terminan bien o con `ConverterError`, nunca otra excepción, y cada caso tarda < 1 s, medido con `test/support/elapsed.ts` (RF-B1), porque Vitest no corta un test síncrono por timeout.
- **RF-B5. Salida.**
  - Por defecto `content/_local/sprites/<id>/` (`manifest.json` y `sheet-<n>.png`).
  - **Se niega a escribir** en un directorio que git no ignore (`git check-ignore -q`); así una ruta mal puesta nunca deja sprites de MUGEN listos para commitear.
  - Empaqueta los frames en hojas de ≤ 2048×2048 (algoritmo de estantes, sin dependencias), con el pivote desde el eje de MUGEN, `frame_ms = round(ticks × 1000 / 60)` y `ticks = -1` como último frame sostenido (`loop: false`). Si el pivote de MUGEN (eje menos desplazamiento del AIR) cae fuera del sprite, el frame se rellena con píxeles transparentes hasta contenerlo; el formato no cambia (`convert.pivot_too_far` si el frame supera 1024 px por lado).
  - Valida su propia salida con `validateManifest` antes de escribir y falla con los códigos si no pasa.

  → Vitest: conversión de un SFF/AIR sintético a un directorio temporal ignorado por un `.gitignore` de prueba; negativa con uno no ignorado; la salida pasa `validateManifest`.
- **RF-B6. Sin MUGEN en CI ni en el repo.** Los tests generan sus fixtures en el propio test (un escritor mínimo de SFF v1/v2 y AIR en `tools/sff-converter/test/support/`). Ningún test lee de `content/_local/`. → revisión del PR y CI en verde sin esos archivos.
- **RF-B7. Servido solo en desarrollo.** Un plugin de Vite en `apps/web/vite/local-sprites.ts` con `apply: 'serve'` (no existe en `vite build`) sirve `GET /__local-sprites/index.json` (ids con manifest válido) y `GET /__local-sprites/<id>/<archivo>` desde `content/_local/sprites/`. `<id>` cumple `^[a-z][a-z0-9_]{0,47}$` y `<archivo>` es `manifest.json` o `sheet-<n>.png`; cualquier otra cosa da 404 sin tocar el disco, y la ruta resuelta se comprueba dentro del directorio base. La ruta de prueba pide el índice solo en desarrollo (`import.meta.env.DEV`, que el build elimina junto con las URL `/__local-sprites/…`): en el build no hay petición y el selector solo ofrece los placeholders; en `vite dev`, un 404 o una respuesta inválida se trata como "sin personajes locales", sin mostrar error. Un chequeo sobre `apps/web/build/` falla si aparece `__local-sprites`. `.dockerignore` ya excluye `content/_local/` y el Dockerfile de `web` no copia `content/`. → Vitest del resolvedor (traversal con `..`, `%2e%2e`, barras invertidas, ids y archivos inválidos); e2e contra el build: `/__local-sprites/index.json` da 404 y el selector solo lista `dummy_a` y `dummy_b`; `just check-dockerignore` sigue pasando.
- **RF-B8. Dos personajes locales.** El owner convierte al menos 2 personajes de MUGEN y los reproduce en `/es/dev/arena` con `just dev`. **La evidencia del PR es solo texto** (que se convirtieron, cuántos frames y hojas, las `ManifestError` si hubo, y las estadísticas de RF-A19), **sin capturas ni nombres** de personajes de terceros: el repo y los PRs son públicos (ADR-0004). → PR.
- **RF-B9. Go/no-go del owner.** El owner ve la demo (placeholders y personajes locales) en el dispositivo de referencia y en escritorio, y deja su decisión en la sección `## Revisión` de esta spec: **go**, **go con cambios** (lista) o **no-go** (con el motivo; abre un intent nuevo, por ejemplo animación esqueletal). La nota responde, con sí, no o "a medias" y una línea de por qué, estas preguntas fijas:
  1. A 375 px, ¿se entiende en todo momento quién va ganando?
  2. ¿Un `crit` se distingue de un `hit` sin leer números?
  3. ¿El tramo de HP bajo genera tensión?
  4. ¿El KO se siente como un cierre?
  5. ¿Se nota quién toma la iniciativa en cada intercambio?
  6. ¿Funcionan los dos ritmos (la ráfaga y la pausa)?

  Además registra la **separación media entre golpes** que le pareció buena (en ms; puede probar variantes del guion con `?pace=<factor>` en `/[lang]/dev/arena`, de 0.8 a 1.5, que reescala los tiempos del combate sin cambiar sus cues; el resultado pasa `validateTimeline` y, si no, la ruta queda en `error: timeline_invalid`. La separación media de la variante sale como `mean_strike_gap_ms` en el JSON de "copiar estadísticas", sin texto nuevo en pantalla) y confirma o ajusta los valores de "Datos para 003". → registro en la spec.

### Fase C — adaptador `FightLog → Timeline` (bloqueada por 003)

- **RF-C1. Adaptador** `apps/web/src/lib/features/fight/timeline-from-fight-log.ts`: función pura `timelineFromFightLog(log: FightLog, fighters): Result<Timeline, AdapterError[]>`, con `import type` de `@torneo/protocol` y de `@torneo/renderer` (ADR-0009 D4 lo permite). Es el **único** archivo del repo que conoce a la vez la forma de `FightLog` y la de `Timeline`. Un `FightEvent` sin cue equivalente se ignora y se cuenta; un evento incoherente (HP que no cuadra, orden) es un `AdapterError` con código. → Vitest.
- **RF-C2. Pruebas con logs reales.** Fixtures de `FightLog` producidos por el motor de 003 (JSON commiteado por 003 o generado en el test con `engine-wasm`, según exista): su salida pasa `validateTimeline` con los manifests de los placeholders. → Vitest.
- **RF-C3. Reproducción.** La ruta de prueba puede reproducir uno de esos logs además del guion de RF-A8. → e2e (llega a `finished`).
- **RF-C4. Cambios de forma.** Si 003 emite algo que la `Timeline` v1 no puede expresar y hace falta, se sube `timeline_version` a 2 con una enmienda de esta spec; el adaptador nunca mete campos fuera del formato. → revisión del PR.
  - **Fines sin KO, fuera de 002** (enmienda técnica del 2026-10-27, revisión de `plan-c.md`). `time_up` y `end` con `reason` `decision` o `draw` no son expresables en v1, y en 002 **no hace falta** expresarlos: 002 no publica peleas y el go/no-go (RF-B9) no los usa. El adaptador los rechaza con `adapter.unsupported_ending`, sin inventar cues, y RF-C2 lo comprueba con `log-decision-a`, `log-decision-b` y `log-draw`. La `timeline_version` 2 (cue `time_up`, `win` sin `ko` y cierre sin ganador, preocupación 7 de 003) pasa a un issue de Linear, "Timeline v2: fines sin KO", que bloquea la primera feature que publique peleas (trabajo derivado 6 de `docs/architecture.md`), con esos tres dorados como casos de aceptación y game-designer como revisor de su coreografía. El PR de C enlaza el issue.
- **Trazabilidad:** RF-C1 a RF-C4 se implementan según [plan-c.md](./plan-c.md). El issue de la v2 es TA-19.

## Requisitos no funcionales
- **Rendimiento (criterio "≥ 55 fps").**
  - **Dispositivo de referencia:** un Android de gama media de 2022–2024 (clase Galaxy A14/A24/A34, Moto G54/G84 o Redmi Note 12) con Chrome estable. El PR anota modelo, versión de Android y de Chrome. El owner confirma el modelo (ver "Preguntas abiertas").
  - **Acceso:** sobre el **build de producción** (`node apps/web/build` con `ENABLE_DEV_CATALOG=1` y `HOST=127.0.0.1` explícito), no sobre `just dev`: el servidor de Vite sirve Pixi sin minificar y con HMR, y la medición saldría peor que lo que verá el usuario. El plan lo deja en una receta de `just`. En Android, reenvío de puertos por USB (`chrome://inspect` → *Port forwarding* del puerto de esa receta), sin publicar puertos en la LAN (001 ata todo puerto local a `127.0.0.1`). **Excepción de la fase B:** los personajes convertidos solo existen con `just dev` (RF-B7, `apply: 'serve'`), así que su medición corre allí; para separar el costo del personaje del de Vite, el PR de B mide en la misma sesión los placeholders con `just dev` y compara.
  - **Método:** `getFrameStats()` (RF-A19) sobre el guion completo, a 375 px de ancho en vertical, sin movimiento reducido, tres corridas seguidas después de una corrida de calentamiento; con placeholders (fase A) y con un personaje convertido (fase B).
  - **Umbral:** en cada corrida, p95 del tiempo de frame ≤ 18.2 ms (≥ 55 fps) y FPS medio ≥ 58, o ≥ 0.95 × la tasa nativa si la pantalla es de 90/120 Hz y el navegador la usa. En escritorio (la Mac del owner), lo mismo como control.
- **Memoria y peso.** Hojas de ≤ 2048×2048 y ≤ 4 por personaje (≤ 64 MiB de textura en el peor caso por personaje; el spike usa 1–2). El chunk diferido del renderer se reporta en el PR (tamaño gzip); objetivo orientativo ≤ 400 KB gzip. El resto de las rutas no crece (RF-A17).
- **Sin impacto en el VPS.** Nada de 002 corre en el server ni en Postgres; la ruta vive bajo `/dev/**` y en producción da 404.
- **Determinismo cosmético.** Misma timeline y mismos assets → misma escena en cualquier `t` (RF-A9, RF-A10).
- **i18n.** Cero texto literal visible fuera de `src/routes/dev/**`; el renderer no contiene texto, solo dibuja lo que le da el host (RF-A13, RF-A16).
- **Accesibilidad.** WCAG 2.2 AA en los controles; reduced motion según ADR-0009 D6/D7; ≤ 3 destellos/s; canvas con `role="img"` y `aria-label`. La narración accesible de la pelea (región `aria-live`) queda para la feature del narrador.
- **Seguridad.** CSP sin cambios (RF-A18); el convertidor trata su entrada como no confiable (RF-B4) y no escribe fuera de directorios ignorados (RF-B5); el servido local no existe en el build (RF-B7); ningún asset de MUGEN en el repo, el build, la imagen ni los PRs (RF-B6, RF-B8).
- **Navegadores.** Los de 001: las dos últimas versiones de Safari iOS y Chrome Android, y los de escritorio actuales, todos con WebGL.

## Diseño

### Componentes afectados
```
pnpm-workspace.yaml                         (+ tools/*)                         (B)
justfile                                    (check-web, test, e2e)              (A, B)
.github/workflows/ci.yml                    (filtro web + tools/**)             (B)
apps/web/
├─ package.json                             (+ @torneo/renderer workspace:*)   (A)
├─ Dockerfile                               (COPY de package.json del workspace)(A, B)
├─ vite.config.ts                           (plugin local-sprites; CSP intacta) (B)
├─ vite/local-sprites.ts (+ test)                                               (B)
├─ messages/{es,en}.json                    (arena_*)                           (A)
├─ src/lib/components/fight/{FightStage.svelte, ArenaControls.svelte,
│                            renderer-theme.ts}                                 (A)
├─ src/lib/features/fight/timeline-from-fight-log.ts (+ test)                   (C)
├─ src/routes/dev/arena/+page.svelte                                            (A)
└─ e2e/arena.spec.ts                                                            (A, B, C)
packages/renderer/                          (@torneo/renderer)                  (A)
├─ package.json · tsconfig.json · eslint.config.js · vitest.config.ts
├─ src/manifest/   (tipos, validateManifest, resolveAnimation)   puro
├─ src/timeline/   (tipos, validateTimeline)                     puro
├─ src/scene/      (sceneAt, layout del HUD, coreografía)        puro
├─ src/fx/         (prng, partículas, shake, números)            puro
├─ src/stage/      (Pixi: app, luchadores, HUD, efectos, stats)
├─ src/index.ts    (createRenderer, tipos públicos)
├─ placeholders/{dummy_a,dummy_b}/{manifest.json, sheet-0.png}
├─ fixtures/spike.timeline.json
└─ scripts/gen-placeholders.ts
tools/sff-converter/                        (@torneo/sff-converter)             (B)
├─ src/{cli.ts, reader.ts, sff-v1.ts, sff-v2.ts, pcx.ts, rle8.ts, lz5.ts,
│       air.ts, map.ts, pack.ts, write.ts}
└─ test/ (support/ con el escritor de fixtures sintéticos)
scripts/check-renderer-split.mjs (+ fixtures y test)                            (A)
docs/architecture.md · docs/glossary.md · CLAUDE.md                             (A)
```
`crates/**`, `packages/protocol-ts/**`, `apps/server/**`, `infra/**` y `content/**` no se tocan (salvo `content/_local/`, que no está en git).

### Formato de assets v1
```jsonc
{
  "format": "torneo.sprites",        // exacto
  "format_version": 1,               // entero; otro valor → manifest.unsupported_version
  "id": "dummy_a",                   // ^[a-z][a-z0-9_]{0,47}$, igual al nombre de la carpeta
  "pixel_art": true,                 // true → filtrado nearest
  "scale": 1,                        // opcional, 0.25–4; normaliza alturas entre personajes
  "sheets": [{ "file": "sheet-0.png", "width": 512, "height": 512 }],
  "frames": [{ "sheet": 0, "x": 0, "y": 0, "w": 64, "h": 96, "pivot_x": 32, "pivot_y": 92 }],
  "animations": {
    "idle":         { "frames": [0, 1, 2, 3], "frame_ms": [150, 150, 150, 150], "loop": true },
    "attack_light": { "frames": [4, 5, 6, 7], "frame_ms": [60, 60, 90, 120], "loop": false, "impact_frame": 2 }
  }
}
```
Reglas (cada una con su código de error):
- `sheets`: 1 a 4; `file` cumple `^sheet-[0-9]+\.png$` (sin rutas); `width` y `height` enteros de 1 a 2048. PNG RGBA no premultiplicado.
- `frames`: 1 a 1024; dentro de su hoja; `w`, `h` ≥ 1; el pivote (los pies, en coordenadas del frame) dentro del frame. **Todos los frames miran a la derecha.**
- `animations`: solo códigos de RF-A5; las nueve obligatorias presentes. `frames` de 1 a 64 índices válidos; `frame_ms` del mismo largo, enteros de 16 a 2000. `idle` y `walk` con `loop: true`; `ko` con `loop: false` (sostiene el último frame). Animaciones sin loop de ≤ 3000 ms en total.
- `impact_frame`: obligatorio en `attack_light`, `attack_heavy` y `special` y prohibido en las demás; windup ≤ 400 ms.
- Un cambio incompatible sube `format_version`; el renderer solo acepta las versiones que conoce. 004 exporta a este formato.

### Timeline v1
```jsonc
{
  "timeline_version": 1,
  "duration_ms": 64000,
  "fighters": { "a": { "asset": "dummy_a", "max_hp": 1000 }, "b": { "asset": "dummy_b", "max_hp": 1000 } },
  "cues": [
    { "at_ms": 0,     "type": "intro" },
    { "at_ms": 3200,  "type": "strike", "actor": "a", "move": "attack_light", "result": "hit",   "damage": 42, "target_hp": 958 },
    { "at_ms": 4100,  "type": "strike", "actor": "b", "move": "attack_heavy", "result": "dodge", "damage": 0,  "target_hp": 1000 },
    { "at_ms": 30500, "type": "knockdown", "fighter": "b", "duration_ms": 1600 },
    { "at_ms": 58000, "type": "ko",  "fighter": "b" },
    { "at_ms": 60500, "type": "win", "fighter": "a" }
  ]
}
```
- Cues: `intro`, `strike` (`move`: `attack_light | attack_heavy | special`; `result`: `hit | crit | block | dodge`), `knockdown`, `ko`, `win`. `at_ms` de un `strike` es el **instante del impacto**.
- Reglas: `cues` ordenados por `at_ms` (empates en orden de lista); el primero es `intro` en 0 y el primer `strike` llega ≥ 2000 ms después; `damage` entero ≥ 0, 0 si `dodge`; `target_hp` = HP anterior del objetivo − `damage`, en `[0, max_hp]`; HP 0 solo seguido de `ko` de ese luchador; dos `strike` seguidos separados por ≥ 600 ms; todo `knockdown` sigue a un `strike` con resultado `hit` o `crit` sobre ese mismo luchador, con el mismo `at_ms` o hasta 100 ms después (nadie cae sin que lo toquen); ningún `strike` durante un `knockdown` (`(at_ms, at_ms + duration_ms]`, con `duration_ms` de 800 a 3000); un solo `knockdown` por `strike` (`timeline.knockdown_duplicate`); exactamente un `ko`, solo cuando el HP de ese luchador es 0 (`timeline.ko_without_zero_hp`), y después de `ko` solo puede venir exactamente un `win` del otro lado, ≥ 1500 ms más tarde (`timeline.cue_after_ko`); nada después de `win`; `duration_ms ≥ win.at_ms + 2000` y `duration_ms ≤ 3 600 000` (1 h, cota de sanidad; vale también para todo `at_ms`); ≤ 2000 cues; `asset` existe entre los manifests cargados.
- La timeline no lleva semilla ni datos del motor. El renderer deriva todo lo cosmético del índice del cue.

### Coreografía (dentro del renderer, no de la timeline)
Posiciones fijas de pie a 1/3 y 2/3 del ancho lógico. En un `strike`, el atacante avanza durante el windup (con `walk` si existe) hasta la distancia de contacto, el defensor reproduce `hit`, `block` o `dodge` en el impacto (en `crit`, `hit` más el efecto fuerte), y después de la recuperación del ataque los dos vuelven a su marca en ≤ 300 ms. `knockdown` reproduce `knockdown` (o `hit`) y vuelve a `idle` al terminar. `ko` y `win` sostienen su animación hasta el final. Los efectos de RF-A12 se disparan en el instante del impacto.

**Precedencia (regla única, para que `sceneAt` sea función pura).** Con ≥ 600 ms entre impactos, windup ≤ 400 ms y reacciones o recuperaciones de hasta 3000 ms, las animaciones se solapan. Por luchador, en cada `t`, gana la primera que aplique:
1. **`ko` y `knockdown`** no se interrumpen: desde su `at_ms` hasta el final (`ko`) o hasta `at_ms + duration_ms` (`knockdown`). La timeline ya prohíbe golpes en ese intervalo.
2. **Windup propio:** empieza en `at_ms − windup_ms` del siguiente `strike` del luchador e interrumpe cualquier reacción (`hit`, `block`, `dodge`) o recuperación propia. Si empieza antes de que termine el regreso a la marca, el luchador **se queda a distancia de contacto** y encadena (combo), sin volver a avanzar.
3. **Impacto recibido:** en el `at_ms` de un `strike` del rival, la reacción interrumpe la recuperación del ataque propio y el regreso.
4. **Recuperación propia** (frames del ataque después de `impact_frame`) y, al terminar, regreso a la marca.
5. **Reacción en curso** hasta que termina.
6. `idle`.

Un luchador nunca recibe un impacto durante su propio windup: entre dos `strike` consecutivos no hay otro, y el windup del segundo empieza ≥ 200 ms después del impacto del primero. Así, "≥ 600 ms entre impactos" queda validado para 003 por los tests de RF-A9.

### Flujo
```
spike.timeline.json ─┐                          (fase C) FightLog ─▶ timelineFromFightLog ─┐
placeholders/ ───────┤                                                (apps/web)           │
content/_local/ (dev)┘                                                                     ▼
            FightStage.svelte ── import() ──▶ createRenderer(container, { clock, theme, labels, … })
                                                 │ load(timeline, assets)  → validate*
                                                 ▼
                         ticker de Pixi ─▶ t = clock() ─▶ sceneAt(t) ─▶ stage aplica ─▶ canvas
                                                                       └▶ onCue / stats
```

### Datos para 003
Valores iniciales que 003 puede usar como restricciones del ritmo; el go/no-go (RF-B9) los confirma o ajusta:
- **Duración de 45 a 90 s de `duration_ms` total**, intro y pose de victoria incluidas, no solo el tramo de combate. El guion del spike dura ~64 s.
- Densidad y ritmo de referencia: ≥ 30 golpes en el guion, con ráfagas de golpes a ≤ 900 ms y pausas de ≥ 2.5 s. **Separación media entre golpes de ~1.2 s**, fijada por el owner en RF-B9 (2026-10-27, `?pace=0.8` sobre los 1468 ms que mide el guion; ver la nota del owner en `## Revisión`). Es un parámetro del ruleset, no una constante.
- Un `knockdown` siempre coincide con un golpe `hit` o `crit` sobre ese luchador (o lo sigue en ≤ 100 ms), y como mucho uno por golpe.
- El `ko` llega solo con HP 0, justo después del golpe que lo deja en 0, y después de él solo viene el `win`. Ninguna timeline pasa de 1 h.
- Intro ≥ 2 s antes del primer golpe; ≥ 600 ms entre impactos consecutivos; knockdown de 0.8 a 3 s; ≥ 1.5 s de KO a victoria y ≥ 2 s de pose de victoria.
- Resultados de un golpe que el renderer sabe mostrar: `hit`, `crit`, `block`, `dodge`; movimientos: ligero, pesado y especial. Lo que 003 emita fuera de esto (por ejemplo, "HP bajo" o "remontada") el renderer lo deriva del HP o lo ignora, y el narrador lo usa por su cuenta (ADR-0010).
- El renderer mira hasta 400 ms hacia adelante (windup), y las reglas de precedencia de "Coreografía" resuelven los solapamientos.
- **Futuro (no en el spike):** un cue `taunt` (provocación) con animación opcional `taunt → idle`, para que 003 exprese la personalidad (pilar 5 de la visión). Si el go sale bien, entra con una `format_version` y una `timeline_version` nuevas, a pedido de game-designer.

### UI
- `/[lang]/dev/arena`: arriba el `FightStage` (ancho completo, 16:9); debajo `ArenaControls` en una fila que se parte en dos a 375 px. Estados de RF-A16: `loading` con skeleton del tamaño del canvas; `error` con el mensaje del código y un botón de reintentar; `finished` con el ganador (nombre del host) y "repetir".
- Escritorio: mismo layout, ancho máximo del contenedor de la app.

## Áreas de preocupación
1. **Dependencia con 003.** Si el vocabulario de 003 no encaja con los cues, el costo cae en el adaptador (fase C) o en una `timeline_version` 2 (RF-C4), no en el renderer. Para reducirlo, "Datos para 003" se comparte con game-designer, autor de 003, y por eso game-designer revisa esta spec.
2. **Mirar hacia adelante frente a "nunca eventos futuros".** El windup exige conocer el impacto hasta 400 ms antes. En vivo, el server ya manda cada evento "cuando le toca más un pequeño buffer" (`docs/architecture.md`); la feature del vivo tiene que fijar ese buffer en ≥ 400 ms más la latencia. No filtra nada que importe: las apuestas están cerradas durante `Live` y la semilla no sale. Lo dejo escrito para que esa spec no lo redescubra.
3. **CSP y ADR-0012.** ADR-0012 (B4.2) fija que Pixi funciona sin tocar la CSP, con las condiciones de RF-A18. Riesgo: que alguna ruta interna de Pixi 8 cree un worker o use `eval` que no conozco; el e2e en dos motores lo detecta, y la respuesta es parar, no ampliar a escondidas (B4.5 exige un ADR nuevo). `engine-wasm` necesitará `'wasm-unsafe-eval'` en su feature; no es de 002.
4. **Movimiento reducido: intent frente a ADR.** La condición 7 del intent dice "bajan las partículas"; ADR-0009 D7 dice 0. Manda el ADR (RF-A14).
5. **Extensión de ADR-0009 D7, sin contradecirlo.** `RendererOptions` suma `clock`, `labels`, `formatNumber` y `resolveAsset`, y el handle suma `load`, `play`, `pause`, `seek`, `onCue`, `onState`, `snapshot`, `getFrameStats` y `destroy`. El test 7 del ADR habla de "eventos del `FightLog`"; aquí se aplica a los cues de la `Timeline`, que es lo que el renderer recibe. Si frontend-engineer o el owner prefieren, se registra como nota en el ADR al aprobar esta spec.
6. **WebGL en CI.** Chromium headless dentro de la imagen de Playwright usa SwiftShader; las versiones recientes pueden exigir `--enable-unsafe-swiftshader` (o `--use-angle=swiftshader`) en `launchOptions` para el proyecto de esta ruta. WebKit en Linux puede no tener WebGL: allí solo se corre el test de CSP de RF-A18 (que vale también si la ruta acaba en `error: webgl_unavailable`, pero el plan tiene que confirmar que en ese caso Pixi llegó a inicializar lo suficiente para que el test sea útil; si no, el test de CSP en WebKit se marca como limitado en el PR y se repite a mano en Safari).
7. **Medición en el teléfono.** El reenvío por USB funciona en Android. Si el owner solo tiene iPhone, no hay forma equivalente sin publicar el puerto en la LAN, que 001 evita; la salida sería servir el build de producción con `HOST=<ip-de-la-LAN>` solo mientras dura la medición, de forma explícita, y eso necesita su OK (ver "Preguntas abiertas").
8. **Licencia de los placeholders.** Los pongo en `packages/renderer/placeholders/` (AGPL) y no en `content/` (todos los derechos reservados): son figuras geométricas generadas por código, fixtures del renderer, sin lore ni marca. Los personajes reales de 004 van en `content/characters/<slug>/sprites/` bajo `LICENSE-CONTENT.md`. Si el owner prefiere que todo sprite esté bajo la licencia de contenido, se mueven a `content/placeholders/` y el renderer los importa desde ahí.
9. **Evidencia de MUGEN en un repo público.** Las capturas o los nombres de personajes de terceros en un PR o en Linear también son "commitear" a efectos de ADR-0004. RF-B8 exige evidencia solo textual.
10. **Dockerfile y el nuevo importer del lockfile.** Con `tools/*` en el workspace, `pnpm install --frozen-lockfile --filter 'web...'` en el Dockerfile puede exigir el `package.json` de `tools/sff-converter` aunque `web` no lo use. El plan de B lo prueba en el job `images` y, si hace falta, copia solo ese `package.json`; el código de `tools/` nunca entra en el contexto que se copia.
11. **Código de un spike que se queda.** `packages/renderer` no es desechable: con go, es la base del renderer real; con no-go, queda detrás de `/dev/**` hasta que un intent nuevo decida. Por eso se le exige `just check` completo desde el primer PR.
12. **Peso de Pixi.** Pixi 8 completo es grande. RF-A17 garantiza que no pesa fuera de la ruta; el tamaño del chunk de la ruta se reporta y no bloquea el spike.

## Respuestas a preguntas abiertas del intent
- **P: ¿Lista mínima de animaciones?** → R: obligatorias `idle`, `attack_light`, `attack_heavy`, `special`, `hit`, `block`, `dodge`, `ko` y `win`; opcionales con fallback fijo `intro → idle`, `walk → idle` y `knockdown → hit` (RF-A5). Baja el costo por personaje de 004 sin cerrar la puerta a más animaciones (se agregan con una `format_version` nueva o como opcionales con fallback).
- **P: ¿Cámara fija o con zoom dinámico?** → R: encuadre **fijo** que contiene a los dos a 375 px, con **punch zoom** breve (≤ 1.15×, ≤ 300 ms) en especiales y KO, apagado con movimiento reducido (RF-A12, RF-A15). Nada de cámara que siga a los luchadores en el spike.
- **P: ¿Convertidor en Rust o en TS?** → R: **TS (Node)**, paquete del workspace de pnpm en `tools/sff-converter`, con type stripping de Node y solo `pngjs` (RF-B1). En Rust entraría al workspace de Cargo con `crate-age`, `deny` y clippy en `just check` sin beneficio, y reutiliza el validador de manifests del renderer.

## Preguntas abiertas
1. **Dispositivo de referencia.** ¿El owner tiene un Android de gama media de la clase de RNF "Rendimiento"? Si solo tiene iPhone, ¿qué modelo, y autoriza un bind temporal del build de producción a la IP de la LAN para medir ("Áreas de preocupación" 7)?
2. **Fase C dentro de 002.** *Resuelta (coordinación, 2026-10-26): C se queda en 002*, bloqueada por 003. El product-owner recomendaba sacarla para no dejar 002 abierta por algo ajeno; lo mitiga que el go/no-go (RF-B9) no espera a C y que el resultado del spike queda registrado antes.
3. **Placeholders bajo AGPL** ("Áreas de preocupación" 8). Recomendación: sí.

## Estrategia de pruebas

| Criterio del intent | RF | Prueba | Nivel |
|---|---|---|---|
| Guion completo y sincronizado, a 375 px y en escritorio | A8, A9, A11, A15, A16 | `sceneAt` puro y `seek`; `onCue` con reloj falso; ruta llega a `playing` y `finished`; sin scroll horizontal a 375×812 y 1280×800 | unit · e2e |
| ≥ 55 fps en gama media | A19, RNF | `getFrameStats` en el dispositivo de referencia, 3 corridas | manual (PR) |
| Formato documentado y usado por los placeholders | A4–A6 | `validateManifest` sobre fixtures y placeholders; IHDR contra `sheets[]` | unit |
| Convertidor genera assets jugables de 2 personajes | B2–B5, B8 | fixtures sintéticos, mutaciones, negativa a escribir fuera de lo ignorado; evidencia textual | unit · manual |
| Go/no-go del owner | B9 | registro en `## Revisión` | manual |
| Condición 3 (azar cosmético) | A3, A10 | ESLint con fixtures; secuencias del PRNG | lint · unit |
| Condición 4 (MUGEN fuera del build) | B7 | resolvedor del plugin; 404 en el build; `check-dockerignore` | unit · e2e |
| Condición 6 (carga diferida) | A17 | `check-renderer-split.mjs` sobre el manifest del build | e2e (script) |
| Condición 7 (i18n y movimiento reducido) | A14, A16 | `check-i18n`; test 7 de ADR-0009 sobre cues y `sceneAt` | check · unit |
| Condición 8 (45–90 s) | A8 | `duration_ms` total del guion | unit |
| Pelea con historia y dos ritmos (game-designer) | A8 | ≥ 30 golpes, ráfaga, pausa, remontada, golpe final, variedad por lado | unit |
| Precedencia de animaciones | A9 | un test por regla de "Coreografía" sobre `sceneAt` | unit |
| CSP intacta | A18 | sin `securitypolicyviolation` en Chromium y WebKit | e2e |
| Adaptador (fase C) | C1–C3 | logs de 003 → `validateTimeline`; reproducción hasta `finished` | unit · e2e |

Además: axe en `/es/dev/arena` (0 `serious` o `critical`) y capturas a 375×812 y 1280×800 como artefacto, sin comparación de píxeles (igual que 001).

## Alternativas consideradas
- **Que el renderer consuma `FightLog` directamente.** Lo acopla a 003, que está en definición, y bloquea todo el spike. La `Timeline` propia y un adaptador estrecho dejan construir ya y aíslan los cambios del protocolo.
- **Avanzar la animación con el delta del ticker.** Más simple, pero se desincroniza con caídas de frames y pestañas en segundo plano, y hace imposible `seek` y la entrada tardía. `sceneAt(t)` lo resuelve a cambio de calcular partículas de forma analítica.
- **HUD en el DOM (Svelte) sobre el canvas.** Es más accesible y trivial de traducir, pero parte la escena en dos capas que hay que sincronizar y deja fuera del renderer lo que una repetición o un clip necesitan. Lo descarto para el spike; la accesibilidad se cubre con el `aria-label` y, después, con el narrador.
- **Placeholders dibujados en runtime con `Graphics` de Pixi.** No ejercitan el formato de assets, que es la mitad del entregable.
- **Convertidor en Rust.** Ver "Respuestas".
- **`sharp` o `tsx` para el convertidor.** Traen binarios o scripts de instalación que rompen `strictDepBuilds`.
- **Ampliar la CSP (`worker-src blob:`, `'unsafe-eval'`).** Prohibido por ADR-0012 (B3 y B4.5) y sin necesidad demostrada (RF-A18).
- **Exponer la ruta en producción.** El spike no es para el público; queda bajo `/dev/**`.

## Revisión
### product-owner — 2026-10-26 — APPROVE
- Verifiqué:
  - **Problema del intent.** La pregunta es si una pelea animada desde un log se siente emocionante o plana en el celular, y la spec llega a ella. El guion de RF-A8 tiene drama verificable: más de 20 golpes, los cuatro resultados, un knockdown, un tramo con HP < 25 % de cada lado, KO y victoria. Los efectos de RF-A12 tienen topes medibles. RF-B9 deja la decisión del owner registrada con tres salidas: go, go con cambios o no-go con un intent nuevo.
  - **Criterios de éxito.** Los cinco del intent y las condiciones 3, 4, 6, 7 y 8 del architect tienen un RF y una prueba en la tabla de "Estrategia de pruebas". El "≥ 55 fps" ahora se puede verificar: hay un dispositivo de referencia, un método (tres corridas tras el calentamiento) y un umbral (p95 ≤ 18.2 ms). Que no se compruebe en CI está justificado.
  - **Preguntas abiertas del intent.** Las tres tienen respuesta y coinciden con lo que recomendó el architect. Las tres nuevas quedan arrastradas a la sección "Preguntas abiertas", con recomendación y dirigidas al owner.
  - **ADR-0004.** Los sprites de MUGEN solo están en `content/_local/`. El convertidor se niega a escribir en directorios que git no ignore (RF-B5). El servidor local no existe en el build (RF-B7). La evidencia del PR es solo texto, sin capturas ni nombres (RF-B8). Esto último cubre un hueco que el intent no veía.
  - **ADR-0005.** No hay monedas ni apuestas.
  - **ADR-0007.** El renderer no tiene texto. Los nombres y banners le llegan traducidos por `labels`, los errores son códigos y los mensajes `arena_*` están en es y en.
  - **Mobile-first.** Pruebas a 375×812, sin scroll horizontal, con objetivos táctiles ≥ 44 px y nombres recortados con elipsis.
  - **Producto.** Inyectar `clock` y hacer de la escena una función pura de `t` deja resueltas de antemano la entrada tardía al vivo y las repeticiones. Son dos pilares de la visión que el spike no construye, pero tampoco bloquea.
  - **Alcance.** Lo que el intent deja fuera sigue fuera: motor, WebSockets, apuestas, narrador y arte final. La ruta vive bajo `/dev/**` y no toca el VPS.
  - **Frontmatter.** Es válido según `docs/process.md`.
- Recomendaciones (no bloquean):
  1. **Pregunta abierta 2: muevan la fase C fuera de 002.** El entregable del spike es la decisión de go o no-go. Si 002 depende de que 003 se fusione, queda abierta por algo ajeno a su pregunta. Propongo cerrar 002 con RF-B9 y llevar RF-C1 a RF-C4 tal cual a la primera feature que reproduzca peleas del motor. La decisión es del owner.
  2. **Guía para el go/no-go.** Propongo que RF-B9 incluya tres o cuatro preguntas fijas para el owner, para que la decisión no se quede en "se ve bien":
     - A 375 px, ¿se entiende en todo momento quién va ganando?
     - ¿Un `crit` se distingue de un `hit` sin leer números?
     - ¿El tramo de HP bajo genera tensión?
     - ¿El KO se siente como un cierre?
     Son las preguntas que deciden si alguien que apostó se queda mirando hasta el final.
  3. **Tamaño de la fase A.** La fase A es pesada para un spike: 20 RF, separación de chunks y CSP en dos motores. Lo acepto por la preocupación 11 (el código se queda). Aun así, si el plan de A crece, conviene que el owner vea la demo con placeholders al terminar A1 o A2, sin esperar a la fase B. Así el riesgo visual se descubre antes.

### game-designer — 2026-10-26 — REQUEST_CHANGES
- Verifiqué:
  - **Vocabulario de cues frente al intent de 003.** `intro`, `strike` (`attack_light | attack_heavy | special` × `hit | crit | block | dodge`), `knockdown`, `ko` y `win` cubren todos los eventos que el intent de 003 promete: intro, ataques, golpes, esquivas, bloqueos, críticos, especiales y KO. "HP bajo" y "remontada" se derivan del HP o quedan para el narrador (ADR-0010), y me parece la frontera correcta: son estados de la pelea, no animaciones. `block` con `damage > 0` (daño por bloqueo) está permitido, y 003 lo va a necesitar. El escape a `timeline_version` 2 (RF-C4) le basta a 003 v0.
  - **Animaciones.** Las nueve obligatorias y los tres fallbacks (`intro → idle`, `walk → idle`, `knockdown → hit`) son un mínimo razonable para el costo por personaje de 004. Que el fallback lo fije el formato y no cada manifest es lo correcto: todos los personajes caen igual. El windup ≤ 400 ms deja leer el golpe sin frenar el ritmo.
  - **Duración.** 45–90 s cuadra con peleas rápidas cada 10–15 min (visión) y con la restricción de 003.
  - **ADR-0004.** `dummy_a`/`dummy_b` son figuras geométricas sin referencia a nadie. La evidencia de RF-B8 no lleva nombres ni capturas. Los nombres los pone el host vía `labels`.
  - **Frontmatter y glosario.** Válidos. Los términos nuevos (`Timeline`, *Cue*, *Manifest de sprites*) se agregan al glosario en RF-A20.
- Cambios pedidos:
  1. **Reglas de precedencia en "Coreografía" (bloqueante).** Con ≥ 600 ms entre impactos, windup de hasta 400 ms y regreso de ≤ 300 ms, la coreografía se solapa. Dos ejemplos:
     - Si el mismo actor golpea dos veces seguidas, su siguiente windup empieza a los 200 ms del impacto anterior, mientras todavía regresa a su marca.
     - Si los golpes alternan, el defensor tiene que empezar su windup mientras reproduce `hit`/`block`/`dodge`. Esas animaciones, y la recuperación de un `attack_heavy`, pueden durar hasta 3000 ms según el formato.

     La spec no dice qué gana, y `sceneAt` necesita una regla única para ser función pura. Pido fijar, con un test sobre `sceneAt`:
     - un windup interrumpe cualquier animación de reacción o de recuperación;
     - un impacto recibido interrumpe la recuperación del ataque propio;
     - si el siguiente windup del mismo actor empieza antes de que termine su regreso, se queda a distancia de contacto (combo);
     - `knockdown` y `ko` no se interrumpen.

     Sin esto, el "≥ 600 ms" que se le entrega a 003 no está validado.
  2. **Arco dramático en el guion (RF-A8).** "HP < 25 % de cada lado" garantiza un final apretado, pero no una pelea con historia. La pregunta del spike es si se siente emocionante, y 003 se compromete a que ≥ 15 % de las peleas tengan un "momento". El guion tiene que mostrar al owner cómo se ve uno. Pido que RF-A8 exija, verificable en Vitest sobre el fixture:
     - (a) al menos un **cambio de líder** en HP en la segunda mitad: el ganador va perdiendo durante ≥ 5 s después del 50 % de la duración (remontada);
     - (b) el golpe que deja HP 0 es un `crit` o un `special`;
     - (c) los dos luchadores usan los tres movimientos y conectan al menos un `crit`.
  3. **Densidad y variedad de ritmo (RF-A8 y "Datos para 003").** 20 golpes en ~56 s de combate son un golpe cada ~2.8 s. Para un juego de peleas se siente lento, y el owner podría dar no-go por el ritmo y no por el renderer. Pido:
     - subir el mínimo a **≥ 30 `strike`**;
     - exigir al menos una ráfaga de ≥ 3 golpes seguidos del mismo actor con separaciones ≤ 900 ms y al menos una pausa de ≥ 2.5 s sin golpes (respiro antes del tramo final), para que el owner juzgue los dos ritmos;
     - que RF-B9 registre, junto a los valores de "Datos para 003", la **separación media entre golpes** que le pareció buena. Es el dato que más necesito para calibrar 003, y hoy no se pide.
  4. **Aclarar la duración en "Datos para 003".** Hay que decir que 45–90 s es `duration_ms` total, intro y pose de victoria incluidas, y no solo el tramo de combate. Si no, 003 puede calibrar 90 s de combate y salirse del rango.
- Recomendaciones (no bloquean):
  - Si `knockdown` cae después de un golpe, que coincida con el `at_ms` de un `strike` sobre ese luchador (o lo siga en ≤ 100 ms). Un luchador que cae sin que nadie lo toque se ve como un error. Puede quedar como regla de `validateTimeline` o como guía para 003.
  - Para la personalidad y el absurdo (pilar 5), conviene prever pronto un cue `taunt` (provocación) con animación opcional `taunt → idle`. No lo pido en el spike, pero si el go sale bien, es lo primero que pediré en una `format_version` o `timeline_version` nueva para que 003 exprese la personalidad.
  - Apoyo las preguntas fijas del product-owner para RF-B9. Agregaría una: ¿se nota quién toma la iniciativa en cada intercambio?

### architect — 2026-10-26 — Respuesta del autor
Atendí los cuatro cambios bloqueantes de game-designer y parte de las recomendaciones:
1. **Precedencia de animaciones.** "Diseño → Coreografía" fija una regla única por luchador (`ko`/`knockdown` > windup propio, con combo a distancia de contacto > impacto recibido > recuperación propia > reacción > `idle`) y explica por qué nadie recibe un impacto durante su windup. RF-A9 exige un test sobre `sceneAt` por cada regla.
2. **Arco dramático.** RF-A8 exige remontada (el ganador va perdiendo ≥ 5 s después del 50 %), golpe final `crit` o `special`, y los tres movimientos y al menos un `crit` por lado, cada uno con su aserción en Vitest.
3. **Ritmo.** RF-A8 sube a ≥ 30 `strike` y exige una ráfaga (≥ 3 golpes del mismo actor a ≤ 900 ms) y una pausa de ≥ 2.5 s. RF-B9 registra la separación media entre golpes que el owner prefiera.
4. **Duración.** "Datos para 003" y RF-A8 dicen que 45–90 s es `duration_ms` total, intro y victoria incluidas.
- Recomendaciones que tomé: regla de `validateTimeline` "todo `knockdown` coincide con un `hit`/`crit` sobre ese luchador o lo sigue en ≤ 100 ms" (`timeline.knockdown_without_strike`); `taunt` anotado como trabajo futuro en "Datos para 003", fuera del spike; las cuatro preguntas fijas del product-owner, más la de iniciativa de game-designer y una sobre los dos ritmos, en RF-B9; vista previa con placeholders al fusionar A (Resumen).
- Recomendación que no tomé: sacar la fase C de 002 (product-owner 1). La coordinación decidió dejarla, bloqueada por 003. El go/no-go no la espera ("Preguntas abiertas" 2, resuelta).
- **Aprobación del product-owner retirada** de `approvals`: su verificación citaba el contenido de RF-A8 ("más de 20 golpes") y de RF-B9, y los dos cambiaron de forma sustantiva, así que tiene que volver a revisar. `status: in-review`.

### game-designer — 2026-10-26 — APPROVE (segunda revisión)
- Verifiqué los cuatro cambios pedidos:
  1. **Precedencia ("Coreografía").** Hay una regla única por luchador, en orden: `ko`/`knockdown`, windup propio con combo a distancia de contacto, impacto recibido, recuperación, reacción, `idle`. Comprobé el argumento de que nadie recibe un impacto durante su propio windup: entre dos `strike` consecutivos no hay otro, y el windup del segundo empieza ≥ 200 ms después del primer impacto. RF-A9 exige un test por regla, así que el "≥ 600 ms" para 003 queda validado.
  2. **Arco (RF-A8).** Remontada en la segunda mitad (≥ 5 s y recupera la delantera), golpe final `crit` o `special`, y los tres movimientos y un `crit` por lado, cada uno con su aserción en Vitest.
  3. **Ritmo.** ≥ 30 golpes (~1.9 s de media), una ráfaga ≤ 900 ms y una pausa ≥ 2.5 s. RF-B9 registra la separación media que prefiera el owner, y la pregunta 6 evalúa los dos ritmos.
  4. **Duración.** 45–90 s es `duration_ms` total, en RF-A8 y en "Datos para 003".
- También verifiqué `timeline.knockdown_without_strike` y `taunt` anotado como futuro. Que la fase C se quede en 002 no afecta lo que reviso, porque el go/no-go no la espera.

### product-owner — 2026-10-26 — APPROVE (segunda revisión)
- Revisé solo lo que cambió después de mi primera revisión:
  - **RF-A8.** El guion ya no es solo plano y apretado al final: tiene remontada, golpe final `crit` o `special`, al menos 30 golpes, una ráfaga, una pausa y variedad por luchador. Cada punto tiene su aserción en Vitest sobre el fixture. Comprobé que la ráfaga (separaciones ≤ 900 ms) es compatible con la regla de ≥ 600 ms entre impactos. Ahora el owner juzga cómo se ve un "momento" de pelea, y no solo si se mueve. Es lo que hace volver al que apostó.
  - **RF-B9.** Recoge mis cuatro preguntas, la de iniciativa de game-designer y la de los dos ritmos, con respuesta sí, no o "a medias" y su motivo. También registra la separación media entre golpes. La decisión es verificable y le da a 003 un dato que puede usar.
  - **Precedencia en "Coreografía".** Es una regla única y ordenada que hace de `sceneAt` una función pura de verdad. El ritmo que se entrega a 003 queda validado por los tests de RF-A9.
  - **Duración.** 45–90 s es `duration_ms` total, en RF-A8 y en "Datos para 003". Cuadra con peleas cada 10–15 min.
  - **Vista previa tras A** (Resumen). Atiende mi recomendación 3 sin crear una fase nueva.
  - **Fase C.** Acepto la decisión de coordinación de dejarla en 002. Lo que me preocupaba, que el resultado del spike esperara a 003, queda resuelto porque el go/no-go no espera a C y se registra antes.
  - `taunt` está anotado como futuro y fuera del spike. Así no hay scope creep.
- Sin cambios pedidos. Con las dos aprobaciones, `status: approved`.

### architect — 2026-10-26 — Enmienda técnica (revisión de plan-a1)
- Al revisar `plan-a1.md` admití cuatro precisiones que no cambian el comportamiento del producto ni el arco del guion, por eso no reabro la revisión de product-owner y game-designer: el subpath `./fixtures/*` (RF-A1); la partición `prepareScene` + `sceneAt` (RF-A9); el HUD en px CSS del contenedor, fuera de la cámara (RF-A13); y la fase A1 con la parte pura de RF-A12 a RF-A14 y su parte de RF-A20 (Resumen).

### architect — 2026-10-27 — Enmienda técnica (revisión de plan-a2)
- Al revisar `plan-a2.md` admití cinco precisiones:
  1. RF-A18, "Áreas de preocupación" 3 y "Alternativas" ya citan ADR-0012 (aprobado): las condiciones de B4.2 y del trabajo derivado 9, y la regla de parada remite a B4.5.
  2. "Timeline v1" y "Datos para 003" listan las reglas que A1 implementó (`timeline.cue_after_ko`, `timeline.knockdown_duplicate`, `timeline.ko_without_zero_hp` y la cota de 1 h).
  3. RF-A11 suma `onState`, `snapshot` y `RendererError` con los códigos de RF-A16 ("Áreas de preocupación" 5).
  4. El RNF de rendimiento se mide sobre el build de producción atado a `127.0.0.1`, no sobre `just dev`; umbrales y método sin cambios. La medición con personajes convertidos de la fase B sigue en `just dev`, porque el servido local no existe en el build (RF-B7), con los placeholders como control en el mismo modo.
  5. RF-A17 se endurece: tampoco el nodo de `dev/arena` alcanza Pixi por imports estáticos, y se exige el caso positivo del import dinámico.
- No reabro la revisión de product-owner y game-designer: ningún cambio altera lo que ve el usuario, el guion, los umbrales ni el vocabulario de cues. Las reglas de timeline ya estaban implícitas en lo aprobado (regla 1 de "Coreografía", "HP 0 solo seguido de `ko`" y "todo `knockdown` sigue a un `strike`"); las aviso a game-designer porque tocan "Datos para 003", sin pedirle una nueva aprobación.

### architect — 2026-10-27 — Enmienda técnica (revisión de plan-b)
- Al revisar `plan-b.md` admití cinco precisiones:
  1. RF-B7: el cliente pide el índice local solo en desarrollo (`import.meta.env.DEV`). En el build no hay petición ni error en la consola, y un chequeo de `build/` lo garantiza. El e2e del 404 no cambia.
  2. RF-B4: las paletas de SFF v2 pueden tener de 1 a 256 colores (`numcols`) y se completan hasta 256. Un índice fuera de la paleta es error con código. "Exactamente 256" habría rechazado SFF v2 válidos. La cota de seguridad no cambia.
  3. RF-B5: si el pivote de MUGEN cae fuera del sprite, el frame se rellena con transparente. El formato no cambia.
  4. RF-B1 y RF-B4: `test/support/elapsed.ts` es la única excepción de test a la prohibición de reloj, para medir el "< 1 s por caso". No toca código de producción ni la salida del convertidor.
  5. RF-B2 y RF-B9: el subcomando `inspect` y `?pace=<factor>` en `/dev/arena`, con `mean_strike_gap_ms` en las estadísticas copiadas.
- No reabro la revisión de product-owner y game-designer. Ninguna precisión cambia el guion, los umbrales, el formato de assets, el vocabulario de cues ni las preguntas de RF-B9. `?pace` solo facilita algo que RF-B9 ya permitía ("probar variantes del guion"). Aviso a game-designer porque `mean_strike_gap_ms` es el dato que pidió para 003, sin pedirle una nueva aprobación.

### architect — 2026-10-27 — Enmienda técnica (revisión de plan-c)
- Al revisar `plan-c.md` admití tres precisiones:
  1. RF-C4: los fines sin KO quedan fuera de 002, con `adapter.unsupported_ending`, y la `timeline_version` 2 pasa a un issue de Linear que bloquea la primera feature que publique peleas (ver la nota en RF-C4). RF-C4 ya condicionaba la v2 a que "haga falta", y la revisión de product-owner de la spec de 003 dejaba el dueño como "la fase C de 002 o la feature que herede RF-C1 a RF-C4".
  2. RF-C1: el resultado tiene la forma de `TimelineResult` (`{ ok: true, timeline, ignored }` o `{ ok: false, errors }`) en lugar de un `Result` genérico, y "se cuenta" es `ignored`.
  3. RF-C2 y RF-C3: los tests leen los dorados en su lugar, y el arena empaqueta una sola copia (`log-ko-comeback.json`) comprobada byte a byte contra su dorado, porque el contexto de Docker de `apps/web` no incluye `crates/`. Se aparta de la decisión 6 del plan-a de 003 ("única copia") solo en ese archivo.
- No reabro la revisión de product-owner ni de game-designer: no cambia lo que ve el usuario, el guion, el vocabulario de cues ni la Timeline v1, y la puerta de publicación sigue en `docs/architecture.md`. **Aviso a game-designer**, autor de 003, porque su pedido de v2 cambia de dueño: su diseño (decisión, empate, `time_up`, más las notas de windup y KO por chip del punto 4 de `plan-c.md`) entra en el issue nuevo, donde sí revisa.

### Cierre — 2026-10-27
Fases A1 (#36), A2 (#40), B (#41) y C (#42) fusionadas: la feature pasa a `done` al fusionar C, como fija el Resumen. Quedan fuera y con su issue: el go/no-go del owner (RF-B8 y RF-B9) en **TA-20**, y la `timeline_version` 2 (fines sin KO) en **TA-19**.

### owner — 2026-10-27 — Go con cambios (RF-B8 y RF-B9, TA-20)
Nota del owner, transcrita por la sesión de 002 a partir de lo que dijo en el chat. Vio la demo con los placeholders y con dos personajes convertidos en local (`local_1` y `local_2`, ambos SFF v2: uno LZ5 y otro PNG8), en escritorio y en su teléfono.

**Decisión: go con cambios.** El enfoque cinemático coreografiado funciona y la pelea fluye, pero hoy se ve simple. Cambios pedidos:
1. La arena de prueba arranca en pausa y cambiar de luchador no reinicia la pelea sola (en este mismo PR).
2. "Copiar JSON" tiene que funcionar también fuera de un contexto seguro (en este mismo PR).
3. Más vida: más vocabulario de animaciones por personaje (más golpes, desplazamientos, combos) y más pulido de efectos y cámara. Es trabajo de las features siguientes (004 y el motor), no de 002.

**Preguntas fijas:**
1. A 375 px, ¿se entiende en todo momento quién va ganando? **Sí.**
2. ¿Un `crit` se distingue de un `hit` sin leer números? **A medias.**
3. ¿El tramo de HP bajo genera tensión? **No.**
4. ¿El KO se siente como un cierre? **A medias.**
5. ¿Se nota quién toma la iniciativa en cada intercambio? **Sí.**
6. ¿Funcionan los dos ritmos (la ráfaga y la pausa)? **Sí** ("creo").

**Ritmo.** Prefiere un poco más rápido: `?pace=0.8` le pareció bien, y pide que el ritmo siga siempre parametrizado para volver a ajustarlo. Con `pace=1` el guion mide `mean_strike_gap_ms: 1468`, así que la separación media preferida es de **~1.2 s** (1468 × 0.8 ≈ 1174 ms). Ajusta "Datos para 003": la separación media de referencia baja de ~1.9 s a ~1.2 s. Lo demás sigue igual: ráfagas a ≤ 900 ms, pausas de ≥ 2.5 s, ≥ 600 ms entre impactos y 45 a 90 s en total.

**Rendimiento** (`just dev`, `local_1` contra `local_2`, guion completo):

| Dispositivo | Frames | FPS medio | p50 | p95 | p99 | > 33 ms |
|---|---|---|---|---|---|---|
| Mac, navegador de escritorio (992 px, DPR 1) | 3377 | 60.0 | 16.7 ms | 17.5 ms | 17.7 ms | 0 |
| iPhone 17, Safari (LAN temporal, con OK del owner) | 3410 | 60.6 | 17.0 ms | 17.0 ms | 21.0 ms | 9 |

Las dos corridas cumplen el umbral (p95 ≤ 18.2 ms y FPS medio ≥ 58). El iPhone 17 es gama alta: el dispositivo de referencia de la RNF ("Rendimiento") es un Android de gama media, y esa medición sigue **pendiente**. Hay una sola corrida por dispositivo, no tres.
