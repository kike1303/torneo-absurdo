---
id: "007"
title: Estándar de assets para personajes propios
type: spec
status: approved
risk: medium
author: architect
reviewers: [product-owner, game-designer]
approvals:
  - by: product-owner
    date: 2026-10-27
  - by: game-designer
    date: 2026-10-27
linear: TA-22
depends_on: ["002", "003", "006"]
created: 2026-10-27
updated: 2026-10-28
---

# Spec: Estándar de assets para personajes propios

> Intent: [intent.md](./intent.md)

## Resumen
Un set de sprites propio vive en `content/sprites/<id>/` con `manifest.json`, sus `sheet-N.png` y un `provenance.json` sin datos personales. Los personajes lo enlazan desde `character.json` con un campo opcional `sprites` que solo conoce `crates/content`. Quien dibuja entrega un **export abierto**: una carpeta con un PNG por frame en una celda fija de 192×192 y un `export.json` con los tiempos. El importador de `tools/sprite-importer` lo convierte en sheet + manifest. Sobre `validateManifest` se agrega una **capa de guía** (`guide.*`, solo códigos) cuyos rangos viven en un único `GUIDE_LIMITS`. Los tiempos se derivan de `TIMELINE_LIMITS`, de la coreografía y del ruleset, este último leído de un JSON dorado que genera `crates/content`. `just check` valida todo set commiteado, su procedencia y los vínculos. El tope de windup deja de ser un 400 escrito a mano: sale de la regla de 600 ms entre impactos menos la ventana de reacción de 200 ms, y además nunca supera el `windup_*` de ninguna versión vigente del ruleset.

La feature se divide en dos fases, cada una con su PR y su plan:
- **Fase A — contrato en el renderer y atadura al ruleset** (`plan-a.md`): límites derivados, `GUIDE_LIMITS`, capa de guía, dorado del ruleset. No depende de código de 006.
- **Fase B — export, importador, procedencia, vínculo y `just check`** (`plan-b.md`): herramienta, formato de set, campo `sprites`, guía de arte en `docs/` y documentación. Una fila de la guía necesita `strike_gap_min()` de 006 (ver "Dependencia con 006").

## Una fuente para cada cifra
Responde a la restricción del intent: los tiempos no se copian, y cada rango elegido a mano vive en un solo lugar en código.

| Cifra | Qué es | Fuente única | Cómo se ata |
|---|---|---|---|
| `strikeGapMs` 600, `introGapMs` 2000, `winGapMs` 1500 | Reglas de la `Timeline` (002/005) | `TIMELINE_LIMITS` (`packages/renderer/src/timeline/types.ts`) | Las leen el tope de windup y la capa de guía; 006 las ata a `engine::TIMELINE_CONTRACT` |
| `REACTION_WINDOW_MS` 200 | Garantía de la coreografía de 002: el windup siguiente empieza ≥ 200 ms después del impacto anterior | **Nueva**, en `packages/renderer/src/manifest/types.ts` | Test de la coreografía (RF-A1) |
| `LIMITS.windupMaxMs` | Tope estructural de windup del formato | **Derivado**: `strikeGapMs − REACTION_WINDOW_MS` (= 400) | Vitest (RF-A1) |
| `MAX_WINDUP_MS` y `LOOKAHEAD_MS` de la coreografía | Cuánto mira hacia adelante el renderer | **Derivado**: `= LIMITS.windupMaxMs` (hoy es otro 400 escrito a mano en `scene/choreography.ts`) | Vitest (RF-A1) |
| `CONTACT_PX` 64 | Distancia entre pivotes al conectar | `scene/choreography.ts` (sin cambios) | Lo lee la regla de alcance (RF-A3) |
| `windup_*_ms`, `knockdown_ms` (y `strike_gap_min()` en fase B) | Tiempos del motor, por versión | `Ruleset` (`crates/engine/src/ruleset.rs`) | Dorado `crates/content/tests/golden/ruleset-timing.json` (RF-A5) |
| Celda, pivote, alturas, frames por acción, rangos de windup mínimo, mínimos por frame, ciclos | Rangos elegidos a mano (los aprueba game-designer) | **Nuevo** `GUIDE_LIMITS` (`packages/renderer/src/guide/limits.ts`) | La guía de `docs/` los muestra en tablas **generadas** con prueba de frescura (RF-B7) |

## Requisitos funcionales

### Fase A — contrato en el renderer y atadura al ruleset

- **RF-A1. Tope de windup derivado.** `packages/renderer/src/manifest/types.ts` exporta `REACTION_WINDOW_MS = 200` y `LIMITS.windupMaxMs` pasa a calcularse como `TIMELINE_LIMITS.strikeGapMs − REACTION_WINDOW_MS`. `manifest/types.ts` importa `timeline/types.ts`, que no importa nada, así que no hay ciclo. `scene/choreography.ts` define `MAX_WINDUP_MS = LIMITS.windupMaxMs` y quita su literal. `validateManifest` no cambia de comportamiento: el valor sigue en 400 y los placeholders y fixtures pasan igual. → Vitest: `LIMITS.windupMaxMs === TIMELINE_LIMITS.strikeGapMs − REACTION_WINDOW_MS`, `MAX_WINDUP_MS === LIMITS.windupMaxMs`, `LOOKAHEAD_MS === MAX_WINDUP_MS`, y una propiedad sobre `sceneAt` con timelines generadas que cumplen `validateTimeline`: ningún luchador recibe un impacto durante su propio windup, y entre un impacto recibido y el inicio del windup propio siguiente pasan ≥ `REACTION_WINDOW_MS`.

- **RF-A2. `GUIDE_LIMITS`, fuente única de los rangos.** Módulo nuevo y puro `packages/renderer/src/guide/` (sin Pixi ni DOM), expuesto en el subpath `@torneo/renderer/guide`. El runtime del renderer no lo importa (lo comprueba `check-renderer-split` o un test de imports). `limits.ts` exporta `GUIDE_LIMITS` (congelado) con los valores de la tabla "Guía: números" y nada más. Los valores derivados (por ejemplo, `intro ≤ introGapMs − windupMaxMs`) no son campos: los calcula la capa de guía en el momento.

- **RF-A3. Capa de guía.** `validateGuide(manifest: SpriteManifest, sheets: readonly RgbaImage[], rulesets: readonly RulesetTiming[]): GuideResult`, pura y determinista. Recibe un manifest **ya validado** por `validateManifest` (el tipo lo garantiza) y los píxeles RGBA decodificados de cada hoja (`{ width, height, data: Uint8Array }`). No decodifica PNG, eso lo hace la herramienta. Devuelve `{ ok: true }` o `{ ok: false, errors: GuideError[] }`, con `GuideError = { code: \`guide.${string}\`, path: string, data?: Record<string, string | number> }`. Los códigos van en `guide/codes.ts`, `path` es un JSON Pointer del manifest y nunca hay texto (ADR-0007). El tope de errores reutiliza `ErrorSink` y `LIMITS.maxErrors`. Con `rulesets` vacío devuelve `guide.no_rulesets`, para que nunca pase en vacío. Las reglas, cada una con su código, están en la tabla "Guía: reglas validadas". → Vitest: un manifest de referencia válido pasa, y cada regla tiene un caso que falla con exactamente su código, su `path` y sus `data`.

- **RF-A4. Tope de windup por ataque y por versión.** `windupCap(move, rulesets) = min(LIMITS.windupMaxMs, min_v windup_<move>_ms(v))`, donde `windup_attack_light` es `windup_strike_ms`, `attack_heavy` es `windup_heavy_ms` y `special` es `windup_special_ms`. En v1 da 400, 400 y 400. La regla `guide.windup_long` compara el windup del manifest (suma de `frame_ms` antes de `impact_frame`, la misma cuenta que `validateManifest`, usando `windupMs` de `manifest/resolve.ts`) contra `windupCap` y, si falla, reporta en `data` la versión que lo limita (`ruleset_version`, `cap_ms`, `windup_ms`). La justificación está en "Windup por ataque" (punto 1 de game-designer).

- **RF-A5. Dorado de tiempos del ruleset.** Test nuevo `crates/content/tests/ruleset_timing.rs`. Recorre `Ruleset::for_version(v)` para todo `v` en `0..=u16::MAX` (son llamadas `const` y baratas) y escribe, en JSON canónico (llaves ordenadas, 2 espacios, salto de línea final), `crates/content/tests/golden/ruleset-timing.json`:
  ```json
  { "versions": [ { "ruleset_version": 1, "windup_strike_ms": 400, "windup_heavy_ms": 700,
                    "windup_special_ms": 1100, "knockdown_ms": 1500 } ] }
  ```
  Compara bytes con el archivo commiteado y lo regenera con `UPDATE_GOLDEN=1`, igual que los dorados del motor. `crates/content/tests/golden/` se suma a `.prettierignore`, como `crates/engine/tests/golden/`, porque sus bytes son exactos. Si cambia un campo de una versión, o aparece una versión nueva, el test falla con un mensaje que dice regenerar y volver a correr `just check`. Ese es el fallo explícito del criterio de éxito. Después, `just check` revalida todos los sets contra los números nuevos (RF-B6). Vive en `crates/content` y no en `crates/engine`: solo usa los accessors públicos de `Ruleset`, y el intent deja el motor fuera. Del lado TS, `guide/ruleset.ts` exporta el tipo `RulesetTiming` y `parseRulesetTiming(json: unknown)`: llaves exactas, enteros ≥ 0, versiones únicas y ordenadas, y al menos una versión. Un error da `ruleset.invalid` con su `path`. → Vitest: parsea el dorado real; un JSON con una llave de más, una de menos o una versión repetida falla.

- **RF-A6. Datos de la guía.** `guideTable(rulesets)` devuelve, como datos (códigos y números, sin texto), una fila por animación con sus rangos efectivos, ya resueltos los derivados y el mínimo entre versiones. La guía de `docs/` se genera a partir de esto (RF-B7).

### Fase B — export, importador, procedencia, vínculo y `just check`

- **RF-B1. Formato de export abierto (`torneo.sprites.export` v1).** Es una carpeta que cualquier herramienta puede producir. No se elige ni se recomienda ninguna (frontera con 004):
  ```
  <export>/
    export.json
    frames/<animación>/<nn>.png    # nn = 00, 01, … contiguos; uno por frame, en orden
  ```
  ```jsonc
  {
    "format": "torneo.sprites.export",
    "format_version": 1,
    "id": "ogro_del_pantano",          // mismo patrón que el manifest (LIMITS.idPattern)
    "pixel_art": true,
    "animations": {
      "idle":         { "frame_ms": [150, 150, 180, 150], "loop": true },
      "attack_light": { "frame_ms": [60, 60, 90, 120], "loop": false, "impact_frame": 2 }
    }
  }
  ```
  - Cada PNG mide exactamente la celda (`GUIDE_LIMITS.cellPx` × `cellPx`), mira a la derecha y tiene los pies sobre la fila del pivote (`pivotY`). El pivote **no se declara**: lo fija el estándar, así que no hay forma de equivocarlo. Se acepta cualquier PNG que `pngjs` decodifique, y la salida siempre es RGBA de 8 bits.
  - No hay `scale` (ver la respuesta sobre el tamaño de frame). `loop`, `impact_frame` y `frame_ms` pasan tal cual al manifest, y los valida `validateManifest` (un solo validador). El export solo comprueba lo que el manifest no puede expresar: forma de `export.json` (llaves exactas), archivos, nombres, IHDR y tamaño de cada PNG. Esa comprobación produce códigos `export.*` y los `path` usan los mismos nombres que el manifest (`/animations/attack_light/impact_frame`), así que un error de `manifest.*` o `guide.*` sobre la salida se lee igual sobre el export.
  - Las carpetas de `frames/` son solo códigos de animación conocidos, y su número de PNG es igual al largo de `frame_ms`. Un archivo de más, de menos o con otro nombre falla.

- **RF-B2. Importador.** Paquete nuevo `tools/sprite-importer` (`@torneo/sprite-importer`, `private`, AGPL), con el mismo esqueleto que `tools/sff-converter`: TS ejecutado con `--experimental-strip-types`, dependencias `@torneo/renderer: workspace:*` y `pngjs` (ya fijado en el lockfile, sin dependencias nuevas). Comandos:
  - `pnpm -s --filter @torneo/sprite-importer import -- --in <export> --out <dir>`. Valida el export (RF-B1), arma el manifest, corre `validateManifest` con `expectedId`, decodifica y corre `validateGuide` contra el dorado del ruleset (RF-A5). Si todo pasa, escribe `manifest.json` y `sheet-N.png`. Si algo falla, no escribe nada.

    > **Enmienda (fase B, TA-25, 2026-10-28).** El comando real es `pnpm -s --filter @torneo/sprite-importer run import -- --in <export> --out <dir>`, con `run`: `pnpm import` es un comando propio de pnpm (convierte lockfiles) y sin `run` nunca llega al script. El script sigue llamándose `import`, y lo mismo vale para el diagrama de "Diseño". La guía y el README del paquete documentan la forma con `run`, y `guide-doc.test.ts` falla si vuelve la otra. Ver la desviación 1 de [plan-b](./plan-b.md).

  - **Empaquetado determinista:** frames en el orden canónico (`REQUIRED_ANIMATIONS` y luego `OPTIONAL_ANIMATIONS`, cada uno en orden de `nn`). Frames con píxeles decodificados idénticos se deduplican en un solo frame del manifest. Rejilla de `floor(2048 / cellPx)` columnas por hoja (10 con 192), filas según hagan falta y una hoja nueva cuando se llena. Más hojas que `LIMITS.sheetsMax` da `export.too_many_frames`. Mismo export, mismos bytes de salida.
  - **`--out` cerrado:** solo `content/sprites/<id>/` (se commitea) o `content/_local/sprites/<id>/` (para probar en `/[lang]/dev/arena` con `just dev`, que ya sirve esa carpeta en dev). El importador se niega a escribir en cualquier otra ruta, igual que el convertidor de 002. Nunca toca ni crea `provenance.json`. Sí reemplaza los `sheet-N.png` previos del destino, y no deja sobrantes.
  - **Entrada no confiable:** no sigue symlinks. Lee el IHDR antes de decodificar y rechaza un PNG que no mida la celda. Cada archivo tiene un tope de bytes (`GUIDE_LIMITS.exportFileMaxBytes`) y el número de archivos está acotado por los máximos de frames.
  - **Salida:** líneas JSON `{ "code", "path", "data" }` con rutas relativas a la raíz del repo, sin rutas absolutas ni texto. Sale con 0 si pasa, con 1 si el export no cumple y con 2 ante un error de uso o de I/O. Si pasa, emite además un resumen informativo `{ "frames", "sheets", "colors", "partial_alpha_pixels" }` (los dos últimos sirven a las recomendaciones de paleta, ver RF-B7).

- **RF-B3. Ubicación y forma de un set.** `content/sprites/<id>/` contiene exactamente `manifest.json`, `sheet-0.png`…`sheet-N.png` (los que declara el manifest) y `provenance.json`. Cualquier otro archivo (fuentes de una herramienta, exports, `.DS_Store`), subcarpeta o symlink da `set.unexpected_entry`. El nombre de la carpeta es el `id` del manifest (`manifest.id_mismatch` vía `expectedId`). `manifest.json` y `provenance.json` tienen un tope de 64 KiB y cada `sheet-N.png` uno de `GUIDE_LIMITS.sheetFileMaxBytes`. El IHDR de cada hoja coincide con `sheets[]` y es RGBA de 8 bits (`set.sheet_header`).

- **RF-B4. Procedencia (`torneo.provenance` v1).** `content/sprites/<id>/provenance.json`, obligatorio:
  ```json
  {
    "format": "torneo.provenance",
    "format_version": 1,
    "set": "ogro_del_pantano",
    "author": "encargo-0003",
    "origin": "commission",
    "date": "2026-11-02",
    "ai_assisted": false,
    "license": "content"
  }
  ```
  - Llaves exactas (`provenance.unknown_field`, `provenance.missing_field`) y **sin campos de texto libre**.
  - `set` es igual al `id` de la carpeta.
  - `author` es el identificador público con consentimiento del autor: `^[a-z0-9]+(-[a-z0-9]+)*$`, de 2 a 32 caracteres (`owner`, `encargo-0003`, un alias). El patrón deja fuera espacios, `@`, puntos y barras, así que no caben correos, URLs ni nombres completos escritos como tales.
  - `origin` es `original` (owner o equipo), `commission` (encargo) o `generated` (lo genera código de este repo).
  - `date` es una fecha `YYYY-MM-DD` válida del calendario.
  - `ai_assisted` es un booleano obligatorio: deja constancia de si intervino generación por IA. Es un dato de trazabilidad del origen: sirve a 004, que elige cómo producir el arte, para saber qué sets salieron de cada método, y al owner, si más adelante hay que revisar o retirar sets por su método de producción. No es un término legal ni una regla de 007.
  - `license` es solo `"content"`: el set se publica bajo `LICENSE-CONTENT.md`. Es una referencia, no un término legal.

  Los datos que identifican al autor y la prueba de cesión de derechos **nunca** van al repo: viven en `private/provenance/<id>.md` por convención. Ningún chequeo lo lee, porque `private/` no existe en CI. Todos los errores son códigos `provenance.*`.

- **RF-B5. Vínculo en `character.json` (solo `crates/content`).** `CharacterFile` suma `sprites: Option<String>` con `#[serde(default)]` como **último** campo (sigue `deny_unknown_fields`). `into_parts` lo separa, `Character` gana `pub fn sprites(&self) -> Option<&str>`, y `CharacterSpec`, `canonical_spec_json`, el motor, `engine-wasm` y los bytes de cualquier pelea **no cambian**. `crates/content` no valida el valor más allá de que sea un string (contrato de "I/O y conversión"). El patrón y la existencia los comprueba `just check` con `LIMITS.idPattern` (RF-B6), y así el patrón no se copia a Rust. → tests de `crates/content`: un personaje con `"sprites": "x"` y otro sin el campo cargan igual y tienen el mismo `spec()`; `real_content.rs` y `just sim --check` siguen pasando.

- **RF-B6. Chequeo en `just check`.** `pnpm -s --filter @torneo/sprite-importer check` (sin argumentos, sobre el repo real; acepta `--root` para los tests) recorre:
  1. cada `content/sprites/<id>/`: RF-B3, `validateManifest` con `expectedId`, `validateGuide` contra **todas** las versiones del dorado y RF-B4;
  2. cada `content/characters/<slug>/character.json`. Solo lee `sprites`, sin validar reglas de juego (eso es de `crates/content` y el motor). `sprites` ausente está bien. Si está, tiene que ser un string que cumpla `LIMITS.idPattern` (`link.invalid`, también para `null`) y apuntar a un set existente (`link.unknown_set`). Una carpeta `content/characters/<slug>/sprites/` da `link.misplaced_sprites`, porque el lugar de los sets es `content/sprites/`.

  Un set sin personaje que lo enlace está permitido (trabajo en curso), y un set puede ser compartido por varios personajes. El comando se agrega a `check-web` y a `test` en el `justfile`, con el `tsc`, el `eslint` y el `vitest` del paquete, igual que `sff-converter`. Si no hay ningún set, pasa. → Vitest con raíces de fixture: un caso por código de `set.*`, `provenance.*` y `link.*`, uno por cada familia de `guide.*`, el set válido que pasa y el caso "versión nueva del ruleset con windup menor rompe un set ya válido".

- **RF-B7. Guía de arte.** `docs/guides/sprites.md`, con frontmatter `type: guide`. Ese tipo se suma al esquema de `docs/process.md` en el mismo PR. Cubre:
  - celda, pivote, facing, alturas y lectura a 375 px;
  - acciones obligatorias y opcionales con su fallback (las del manifest);
  - frames por acción, windup, frame de impacto, recuperación, reacciones e `idle` (con el porqué de cada regla, citando el nombre de su fuente: `TIMELINE_LIMITS.strikeGapMs`, `windup_heavy_ms`, `REACTION_WINDOW_MS`, etc.);
  - formato de export (RF-B1), importador (RF-B2), set (RF-B3) y procedencia (RF-B4), con el flujo "export → import a `_local` → probar en la arena → import a `content/sprites` → `provenance.json` → PR";
  - recomendaciones no validadas: paleta y contorno (ver respuestas).

  Las tablas numéricas van entre marcadores `<!-- guide:generated:start -->` y `<!-- guide:generated:end -->`. Las escribe `pnpm --filter @torneo/sprite-importer gen:guide` a partir de `guideTable(rulesets)` y `GUIDE_LIMITS` (Prettier ignora `**/*.md`, así que no reformatea el bloque). Un Vitest falla si el bloque commiteado no coincide con lo que genera el script. Fuera del bloque, la prosa no escribe cifras de tiempo: nombra la fuente.

- **RF-B8. Export de referencia.** Un script determinista del paquete (`gen:fixture`) dibuja un export **geométrico** (`ref_a`: figura simple, sin personaje ni marca, que puede reutilizar las primitivas de `packages/renderer/scripts/placeholders/draw.ts`) con las 12 animaciones, en la celda estándar y cumpliendo la guía. Se commitea en `tools/sprite-importer/fixtures/export-ref/` (AGPL, igual que los placeholders) con prueba de frescura. → Vitest: el import a un directorio temporal pasa `validateManifest` y `validateGuide`; dos imports dan los mismos bytes; `prepare`/`sceneAt` del renderer reproducen `ref_a` contra `dummy_b` con el guion del spike y con un dorado de log adaptado, sin errores. Manual, registrado en el PR: import a `content/_local/sprites/ref_a/` y reproducción en `/[lang]/dev/arena` con `just dev`.

- **RF-B9. Documentación.**
  - `content/characters/README.md`: `sprites/` deja de ir dentro del personaje; el vínculo es `"sprites": "<id>"` y los sets van en `content/sprites/`.
  - Nuevo `content/sprites/README.md`: qué va ahí, la procedencia y el enlace a la guía.
  - Skill `write-character`: el árbol y el campo opcional.
  - Spec de 003, "Esquema de personaje": enmienda con `sprites` opcional, fuera de la forma motor.
  - `docs/architecture.md`: `tools/sprite-importer`, `content/sprites/` y la capa de guía.
  - `docs/glossary.md`: *set de sprites*, *export*, *procedencia*, *capa de guía*.
  - `CLAUDE.md`, "Comandos": `import`, `check` y `gen:guide`.

  La enmienda del intent de 004 sigue fuera (se hace en su revisión).

## Guía: números (fuente: `GUIDE_LIMITS`; los aprueba game-designer)

Celda y encuadre (escena lógica de 640×360, suelo en `GROUND_Y` 320, a 375 px de ancho se ve a 0.586×):

| Parámetro | Valor | Por qué |
|---|---|---|
| `cellPx` | 192 × 192 | Cabe el personaje más grande con el brazo extendido: el pivote queda a 96 px de cada borde, más que `CONTACT_PX` (64) |
| Pivote | `(cellPx/2, cellPx − groundMarginPx)` = (96, 184); `groundMarginPx` 8 | Pies centrados; 8 px bajo los pies para sombra o polvo |
| `heightMinPx`–`heightMaxPx` (alto opaco de `idle` frame 0) | 64–176 | 64 px lógicos ≈ 37 px CSS a 375 px, el mínimo legible; 176 = celda − margen − 8 arriba. Una rata y un ogro conviven en la misma celda |
| `windupStepMs` | 100 | Diferencia mínima de windup entre `attack_light` y `attack_heavy` dentro del mismo set (≈ 6 refrescos a 60 Hz, perceptible) |
| `feetTolerancePx` | 2 | La fila opaca más baja de cada frame de `idle` está entre `pivotY − 3` y `pivotY − 1` |
| `frameMsMin` / `frameMsMax` (guía) | 33 / 1000 | Todo frame dura ≥ 2 refrescos a 60 Hz; ninguno se congela más de 1 s |
| `sheetFileMaxBytes` / `exportFileMaxBytes` | 4 MiB / 1 MiB | Peso del repo público y memoria acotada del chequeo |

Por acción (W = windup, suma de `frame_ms` antes de `impact_frame`; R = recuperación, desde el inicio del frame de impacto hasta el final; "derivado" = calculado, no es campo de `GUIDE_LIMITS`):

| Acción | Frames | Duración | Reglas propias |
|---|---|---|---|
| `idle` | 4–12, ≥ 3 distintos | ciclo 600–2400 ms | cada frame ≤ 250 ms; pies en el pivote |
| `walk` (opc.) | 4–12 | ciclo 300–1200 ms | — |
| `intro` (opc.) | 3–24 | ≤ `introGapMs − windupMaxMs` (derivado: 1600) | — |
| `attack_light` | 3–8 | W 100–250 (y ≤ `windupCap`) | `impact_frame` ≥ 1; frame de impacto ≥ 50 ms; R ≥ `REACTION_WINDOW_MS` |
| `attack_heavy` | 4–10 | W 250–`windupCap` | `impact_frame` ≥ 2; frame de impacto ≥ 66 ms; R ≥ `REACTION_WINDOW_MS` |
| `special` | 5–14 | W 300–`windupCap` | `impact_frame` ≥ 3; frame de impacto ≥ 66 ms; R ≥ `REACTION_WINDOW_MS` |
| `hit`, `block`, `dodge` | 2–6 | total `REACTION_WINDOW_MS`–1000 | `frame_ms[0]` ≤ `REACTION_WINDOW_MS − keyPoseMinMs` (derivado: 200 − 66 = 134) |
| `knockdown` (opc.) | 3–12 | ≤ `min_v knockdown_ms` (derivado: 1500) | — |
| `ko` | 3–12 | ≤ `winGapMs` (derivado: 1500) | sostiene el último frame (formato) |
| `win` | 2–12 | — | — |

Además, en el frame de impacto de los tres ataques, la columna opaca más a la derecha llega a ≥ `pivotX + CONTACT_PX / 2` (derivado: 32 px delante del pivote). El golpe alcanza al rival y, de paso, un frame dibujado mirando a la izquierda falla. Dentro del set, los windups cumplen además el orden `W(attack_light) + windupStepMs ≤ W(attack_heavy) ≤ W(special)` (`guide.windup_order`). Es compatible con los rangos absolutos y el tope de 400: ligero 100–250, así que el pesado puede ir de 250 a 400 y, como mínimo, queda en ligero + 100; por ejemplo, 120/300/320. En el peor caso caben 134 frames, que entran en 2 hojas de 1920×1920.

## Guía: reglas validadas (códigos `guide.*`)

| Código | Regla | `data` |
|---|---|---|
| `guide.no_rulesets` | `rulesets` vacío | — |
| `guide.scale_forbidden` | el manifest declara `scale` (en v1 del estándar no se usa) | — |
| `guide.cell_size` | un frame no mide `cellPx × cellPx` | `w`, `h` |
| `guide.pivot_position` | pivote ≠ (96, 184) | `pivot_x`, `pivot_y` |
| `guide.frame_ms_range` | `frame_ms` fuera de 33–1000 | `ms` |
| `guide.frames_count` | frames de la acción fuera de rango | `count`, `min`, `max` |
| `guide.idle_distinct` | `idle` con < 3 frames distintos | `count` |
| `guide.idle_frame_long` | un frame de `idle` > 250 ms | `ms` |
| `guide.cycle_range` | ciclo de `idle`/`walk` fuera de rango | `total_ms`, `min`, `max` |
| `guide.duration_range` | total de una acción fuera de su rango (incluidos los derivados) | `total_ms`, `min`, `max` |
| `guide.windup_short` | W bajo el mínimo de la acción | `windup_ms`, `min` |
| `guide.windup_long` | W sobre `windupCap` (o sobre 250 en `attack_light`) | `windup_ms`, `cap_ms`, `ruleset_version` (si lo limita el ruleset) |
| `guide.windup_order` | no se cumple `W(attack_light) + windupStepMs ≤ W(attack_heavy) ≤ W(special)` | `light_ms`, `heavy_ms`, `special_ms`, `min_step` |
| `guide.anticipation` | `impact_frame` bajo el mínimo de la acción | `impact_frame`, `min` |
| `guide.impact_hold` | frame de impacto más corto que el mínimo | `ms`, `min` |
| `guide.recovery_short` | R < `REACTION_WINDOW_MS` | `recovery_ms`, `min` |
| `guide.reaction_slow` | `frame_ms[0]` de `hit`/`block`/`dodge` > 134 | `ms`, `max` |
| `guide.frame_empty` | un frame sin ningún píxel con alfa > 0 | — |
| `guide.frame_clipped` | píxel con alfa > 0 en el borde exterior de la celda | — |
| `guide.feet_off_pivot` | en `idle`, la fila opaca más baja no está en `[pivotY − 3, pivotY − 1]` | `row` |
| `guide.height_range` | alto opaco de `idle` frame 0 fuera de 64–176 | `height` |
| `guide.impact_short` | el frame de impacto no llega a `pivotX + CONTACT_PX/2` | `reach` |

Las reglas de píxeles miran cada frame **distinto** una sola vez, y el costo es lineal en los píxeles de las celdas usadas (≤ 134 × 192² ≈ 5 Mpx).

## Requisitos no funcionales
- **Determinismo y límites (ADR-0001):** el motor, `CharacterSpec` y los bytes de las peleas no cambian. Los sprites no entran en el motor ni en `engine-wasm`. La capa de guía es pura (sin reloj ni aleatoriedad) y el importador es byte-determinista.
- **Semilla y look-ahead (ADR-0002):** el renderer sigue mirando como máximo `MAX_WINDUP_MS` (400) hacia adelante, igual que hoy. Además, `windupCap ≤ windup_<move>_ms` garantiza que el windup visual **nunca empieza antes que el `attack` del motor**: en vivo no hace falta conocer un movimiento, y mucho menos un resultado, antes de que el motor lo anuncie. Esta feature no amplía ni condiciona el buffer de la spec de sincronización en vivo.
- **Sin texto en el dominio (ADR-0007):** `guide.*`, `export.*`, `set.*`, `provenance.*`, `link.*` y `ruleset.*` son códigos con datos. La CLI emite JSON y no hay UI nueva.
- **Datos personales:** el esquema de procedencia no tiene texto libre y `author` tiene un patrón cerrado. Lo identificable va a `private/`.
- **Recursos:** todo corre en desarrollo y CI, nada en el VPS. El chequeo decodifica como mucho 4 hojas de 2048² (64 MB) por set, una a la vez. Sin dependencias nuevas ni de pago (`pngjs` ya está fijado).
- **Mobile-first:** alturas de 64–176 px lógicos, que son ≈ 37–103 px CSS a 375 px.
- **Independencia de herramienta:** el importador y `just check` solo leen PNG + JSON. Ninguna herramienta de autoría hace falta para nada.

## Diseño

### Componentes afectados
```
packages/renderer/src/manifest/types.ts     A  REACTION_WINDOW_MS; LIMITS.windupMaxMs derivado
packages/renderer/src/scene/choreography.ts A  MAX_WINDUP_MS = LIMITS.windupMaxMs
packages/renderer/src/guide/{limits,codes,ruleset,validate,table,index}.ts   A  nuevo, puro
packages/renderer/package.json              A  subpath "./guide"
packages/renderer/test/guide*.test.ts       A
crates/content/tests/ruleset_timing.rs      A  dorado
crates/content/tests/golden/ruleset-timing.json  A
.prettierignore                             A  crates/content/tests/golden/
crates/content/src/{file,lib}.rs            B  campo `sprites`, `Character::sprites()`
crates/content/tests/load.rs (+ fixtures)   B
tools/sprite-importer/**                    B  import, check, gen:guide, gen:fixture, fixtures
justfile                                    B  check-web y test
docs/guides/sprites.md                      B  nuevo (type: guide)
docs/process.md, docs/architecture.md, docs/glossary.md, CLAUDE.md          B
docs/features/003-motor-de-pelea/spec.md    B  enmienda "Esquema de personaje"
content/characters/README.md, content/sprites/README.md, .claude/skills/write-character/SKILL.md  B
```
No se tocan: `crates/engine`, `crates/engine-wasm`, `crates/protocol`, `packages/protocol-ts`, `apps/server`, `apps/web` (la arena ya sirve `content/_local/sprites/` en dev), `tools/sff-converter`, `packages/renderer/placeholders/**` ni el formato `torneo.sprites` v1.

### Modelo de datos / tipos (protocol)
Sin cambios en `protocol`. Los tipos nuevos son de TS, para desarrollo: `RulesetTiming`, `GuideError`, `RgbaImage`, y las formas de export y de procedencia de RF-B1 y RF-B4. `character.json` suma `sprites?: string`.

### API / mensajes WS
Ninguno. La entrega del set al cliente queda fuera de alcance.

### Flujo
```
 herramienta de autoría (cualquiera, 004)
        │ exporta
        ▼
 export/ (export.json + frames/<anim>/<nn>.png)
        │ sprite-importer import
        ▼ validateManifest ── validateGuide ◀── ruleset-timing.json ◀── crates/content test ◀── Ruleset
 content/_local/sprites/<id>/  ──▶ /[lang]/dev/arena (just dev)
 content/sprites/<id>/ + provenance.json (a mano) ──▶ PR
        │
        ▼ just check: sprite-importer check (sets, procedencia, vínculos) + cargo test (dorado, load_roster)
 content/characters/<slug>/character.json  "sprites": "<id>"
```

### UI
No hay UI nueva. La verificación visual usa la arena existente.

## Respuestas a preguntas abiertas del intent
- **P: ¿Dónde vive el set?** → R: en `content/sprites/<id>/`, enlazado con `"sprites": "<id>"`. Por qué: (1) el `id` del manifest es `snake_case` (`^[a-z][a-z0-9_]{0,47}$`) y los slugs usan guiones, así que dentro de `characters/<slug>/sprites/` el `id` no podría coincidir con su carpeta, que es lo que `validateManifest` ya exige con `expectedId`; (2) un set con su procedencia es una unidad propia que varios personajes pueden compartir, por ejemplo skins o un set reutilizado; (3) `load_roster` sigue leyendo solo `characters/`. Se actualizan el README de personajes y la nota 8 de 002, que anticipaba la otra ruta.
- **P: ¿Tamaño de frame único o por personaje?** → R: **una celda única de 192×192 lógica, sin `scale`**, y el tamaño del personaje sale de su **alto dibujado** (64–176 px, validado sobre los píxeles). Un ogro se ve más grande que una rata sin escalar, el pivote es el mismo para todos y el export no tiene que declararlo. `scale` queda prohibido en v1 del estándar: con `pixel_art` y un escalado no entero los píxeles se deforman, y usarlo para que un set dibujado al doble de resolución quepa en el límite de hojas (`sheetsMax` 4) exige recortar frames, que es otra versión del estándar.
- **P: ¿Formato de la procedencia?** → R: un **JSON estructurado y validable** (RF-B4) en lugar de markdown. Se valida en `just check` y, al no tener texto libre, no deja dónde escribir un dato personal. La prueba de cesión y los datos del autor van a `private/provenance/<id>.md`.
- **P: ¿Se migran `dummy_a` y `dummy_b`?** → R: **no**. Siguen en `packages/renderer/placeholders/`, bajo AGPL, como fixtures del renderer (nota 8 de 002), con su celda actual, y quedan fuera de la capa de guía. Moverlos a `content/` pasaría figuras generadas por código a "todos los derechos reservados" sin ganar nada, y rehacerlos en la celda de 192 movería tests, capturas e2e y la medición de rendimiento de 002. El ejemplo de referencia del estándar es el export geométrico `ref_a` (RF-B8), también AGPL y en `tools/`, que es justo lo que un artista tiene que entregar.
- **P: ¿Cómo llega el ruleset al validador TS y contra qué `ruleset_version`?** → R: por un **dorado** que escribe un test de `crates/content` (RF-A5), con el mismo patrón de 006 (Rust escribe JSON canónico y TS lo lee en un test o una herramienta). Un set **no declara versión** (el formato v1 no tiene el campo y no se sube por esto): se valida contra **todas** las versiones que devuelve `Ruleset::for_version`, porque un set también reproduce repeticiones de peleas viejas y las versiones viejas nunca se borran (ADR-0001). Con una versión nueva, el dorado falla hasta que se regenera en el mismo PR, y entonces `just check` revalida todos los sets. Si alguno no cumple con la versión nueva, se arregla en ese PR o se reconsidera la versión. Nunca pasa en silencio.
- **P: ¿Paleta y contorno se validan?** → R: **solo recomendaciones** hasta que exista la identidad visual, que el intent deja fuera. Validarlas ahora fijaría un estilo y le quitaría decisiones a 004. La guía recomienda: en pixel art, ≤ 32 colores por set y alfa binaria (0 o 255); contorno oscuro de 1 px lógico para leer la silueta a 375 px sobre los dos temas. El importador informa `colors` y `partial_alpha_pixels` para que 004 los mida. Pasan a regla, con un código `guide.*` nuevo, cuando la identidad visual las fije.

## Puntos de game-designer (revisión del intent)

### 1. Windup por ataque
El tope real tiene dos restricciones, y el windup visual del movimiento M en la versión v cumple las dos:
- **C1 (motor y vivo):** W(M) ≤ `windup_M_ms(v)`. El windup visual nunca empieza antes del evento `attack` del motor, así que en vivo no exige mirar más adelante de lo que el motor ya anunció (ADR-0002, nota 2 de 002).
- **C2 (coreografía):** W(M) ≤ `strikeGapMs − REACTION_WINDOW_MS`. Una `Timeline` válida solo garantiza 600 ms entre dos impactos cualesquiera, **sin importar el movimiento**: el guion del spike, el `?pace` mínimo de 006 (que lleva las separaciones del motor hasta cerca de 600) y cualquier timeline futura. Si un pesado tuviera 700 ms de windup y lo precediera un impacto del rival 600 ms antes, su windup empezaría 100 ms **antes** de ese impacto. Por la regla 2 de precedencia, el windup gana y la reacción `hit` no se vería: el golpe recibido parecería ignorado.

Así, `windupCap(M) = min(C1, C2)`, que en v1 da 400, 400 y 400. Para pesado y especial manda C2, no el `windup_strike_ms`. La atadura es a la restricción real: si `strikeGapMs` o `REACTION_WINDOW_MS` cambian, `windupMaxMs` cambia solo, y si una versión del ruleset baja un `windup_*` por debajo, `guide.windup_long` lo señala con esa versión. Para que un pesado "se vea venir" dentro del tope, la guía exige **mínimos** de windup distintos: ligero de 100 a 250 ms, pesado ≥ 250 y especial ≥ 300, con más frames de anticipación (`impact_frame` ≥ 2 en el pesado y ≥ 3 en el especial). Además, la regla validada `guide.windup_order` exige, **dentro del mismo set**, `W(attack_light) + windupStepMs (100) ≤ W(attack_heavy) ≤ W(special)`. Por eso un pesado se anticipa al menos 100 ms más que el ligero de ese personaje, y un especial nunca se anticipa menos que su pesado. Los rangos absolutos solos no lo garantizaban (ligero 250 y pesado 250 pasaban). Con el tope de 400 la regla siempre se puede cumplir, porque el ligero tiene un máximo de 250. El motor ya separa los impactos por movimiento (`recovery_ms + windup_heavy_ms` ≥ 1000 ms antes de un pesado), así que un windup visual más largo es posible en el futuro, pero exige que la `Timeline` lleve separación por movimiento (`timeline_version` nueva, ver "Alternativas").

### 2. Recuperación y reacciones frente a `recovery_ms` y `strike_gap`
- **Reacciones (`hit`, `block`, `dodge`).** Las corta el windup propio siguiente o el impacto siguiente. En cualquier `Timeline` válida, entre el impacto recibido y el inicio del windup propio pasan ≥ `REACTION_WINDOW_MS` (200 ms, C2). En los logs del motor son ≥ `recovery_ms + windup_strike_ms − windupMaxMs` (300 ms en v1). Regla validada: la pose que se lee (el frame 1) empieza a ≤ 134 ms, así que se ve ≥ 66 ms (`keyPoseMinMs`) antes del corte más temprano posible. La duración total va de 200 a 1000 ms; lo que pase de ~300 ms solo se ve si nadie actúa enseguida, y la guía lo dice.
- **Recuperación de los ataques (desde `impact_frame`).** La corta el impacto del rival (≥ `strike_gap_min()` en el motor: 700 ms en v1, de 006) o el windup propio siguiente (≥ 200 ms en cualquier timeline). Reglas validadas: el frame de impacto dura ≥ 50 ms (ligero) o ≥ 66 ms (pesado y especial), y la recuperación total es ≥ 200 ms, para que el ataque no vuelva a `idle` mientras el rival todavía reacciona. Recomendación: ≤ `strike_gap_min()`, porque más allá casi nunca se ve en un intercambio rápido.
- En un combo a 600–700 ms, entonces, el impacto y la pose de reacción siempre se leen, y lo demás es adorno que puede cortarse.

### 3. `idle` en los respiros de 006
Los respiros dejan varios segundos solo con `idle` (la separación más larga por pelea la mide `just sim`; 006 reporta un máximo de unos 4.6 s). Reglas validadas: ≥ 4 frames y ≥ 3 distintos, ningún frame de más de 250 ms (nunca hay más de un cuarto de segundo quieto) y un ciclo de 600 a 2400 ms. Recomendación: un ciclo de 800 a 1600 ms (de 3 a 6 vueltas en la pausa más larga), con `frame_ms` irregulares para que el loop no se note: por ejemplo, un parpadeo o un respiro más largo. Ese frame "más largo" sigue topado en 250 ms (`guide.idle_frame_long`): la irregularidad se logra alternando frames más cortos (por ejemplo, 120 y 250 ms), nunca con un frame de más de 250. No se agregan animaciones nuevas: variantes de `idle` o un `taunt` serían opcionales con fallback en una `format_version` nueva (intent: formato estable).

## Dependencia con 006
- **Fase A no depende de código de 006.** Usa `TIMELINE_LIMITS`, que ya está en main, y accessors de `Ruleset` que ya existen. 006 es solo el precedente del camino Rust → JSON dorado → TS. Si 006 se fusiona antes, no hay conflicto: 006 no cambia `windup_*` ni `knockdown_ms`, así que el dorado de 007 no se mueve.
- **Fase B usa `strike_gap_min()` de 006** para la fila "recuperación recomendada" de la guía, y suma `strike_gap_min_ms` al dorado. **Orden de merge: 006 antes de la fase B.** Fallback: si 006 no está en main cuando se apruebe `plan-b.md`, el campo no entra en el dorado, la guía nombra `strike_gap_min()` de 006 sin cifra, y el campo y la fila se agregan en un PR de seguimiento al fusionarse 006. Ninguna regla **validada** depende de 006.

## Áreas de preocupación
1. **Pesado y especial topados en 400 ms (C2 frente a la legibilidad).** game-designer pidió considerar windups por golpe. Con la `Timeline` actual (600 ms entre impactos sin importar el movimiento), más windup rompe la regla de "nadie recibe un impacto durante su windup". Lo compenso con mínimos por acción. Si en pantalla un pesado sigue sin "verse venir", la salida es una `timeline_version` con separación por movimiento, en una feature propia.
2. **"No toca `crates/engine`".** El dorado del ruleset vive en un test de `crates/content`, que usa solo API pública. No hay ni una línea nueva en el motor.
3. **Dos lectores de `character.json`.** Rust (`crates/content`, la fuente) y el chequeo de Node, que **solo** lee `sprites` para validar el vínculo. Lo acepto para no copiar `LIMITS.idPattern` en Rust. Si en el futuro el server necesita el vínculo validado, `crates/content` podrá validarlo con el patrón leído de un dorado, como el ruleset.
4. **La procedencia no puede probar el consentimiento.** El patrón de `author` bloquea correos, URLs y nombres con espacios, pero no un nombre real escrito como `juan-perez`. El control es humano: el PR que agrega un set declara en la descripción que existe `private/provenance/<id>.md`, sin detalles, y el owner lo confirma. **Esta feature no redacta términos de encargo ni de cesión.** Si se quiere una plantilla de contrato, es otra feature con `risk: high`.
5. **Dos clases de sprites en el repo.** Los placeholders (AGPL, celda propia, sin capa de guía) y los sets de `content/sprites/` (todos los derechos reservados, con guía y procedencia). La guía y `content/sprites/README.md` lo dicen explícitamente para que nadie tome a `dummy_a` como modelo.
6. **Celda fija de 192 y repo público.** 134 frames en el peor caso son ~2 hojas de 1920² por personaje, comprimidas en unos cientos de KB a pocos MB. El tope por hoja (4 MiB) limita el crecimiento del repo. El HD (`scale` 0.5 con frames recortados) queda para otra versión del estándar.
7. **Rangos elegidos por mí.** Las cifras de `GUIDE_LIMITS` son una propuesta inicial calibrada con los placeholders (sus tiempos cumplen todas las reglas de tiempo). Las aprueba game-designer en esta spec, y cambiarlas después es editar un solo archivo y regenerar la guía.
8. **Tablas generadas: la guía puede quedar vieja.** Si alguien cambia `GUIDE_LIMITS` o el dorado y no corre `gen:guide`, la prueba de frescura falla en `just check`. El costo es un paso más al editar un límite, y lo acepto a cambio de que la guía nunca muestre una cifra distinta de la del código.
9. **`type: guide` en el esquema de `docs/process.md`.** Es un cambio mínimo a un documento del owner. Si el owner prefiere otro tipo, se cambia sin afectar al resto.
10. **Tope de memoria del chequeo.** Las hojas se decodifican de a una y se descartan. Con muchos personajes, el chequeo crece de forma lineal, y el plan mide su tiempo con el fixture.

## Estrategia de pruebas
| Qué | RF | Nivel |
|---|---|---|
| `windupMaxMs`, `MAX_WINDUP_MS` y `LOOKAHEAD_MS` derivados; propiedad de la coreografía (sin impacto durante el windup propio, reacción ≥ `REACTION_WINDOW_MS`) | A1 | Vitest (unit + propiedad sobre `sceneAt`) |
| Cada regla de `validateGuide`: el caso válido pasa y cada código tiene su caso que falla, con `path` y `data` | A3, A4 | Vitest con manifests y píxeles construidos |
| `windupCap` con varias versiones; `guide.windup_long` reporta la versión que limita | A4 | Vitest |
| `guide.windup_order`: 120/300/320 pasa; ligero 250 con pesado 300 (paso < 100) falla; especial 320 con pesado 380 falla; con `data` exactos; el `ref_a` de RF-B8 la cumple | A3, B8 | Vitest |
| Dorado del ruleset: bytes iguales, `UPDATE_GOLDEN`; `parseRulesetTiming` rechaza llaves de más o de menos y versiones repetidas | A5 | `cargo test -p content`, Vitest |
| `guideTable` y frescura del bloque generado de la guía | A6, B7 | Vitest |
| Export: cada código `export.*`; el import no escribe nada si falla; `--out` fuera de las dos rutas se rechaza; symlinks rechazados | B1, B2 | Vitest (tmpdir) |
| Import determinista (dos corridas, mismos bytes); deduplicación; partición en hojas y `export.too_many_frames` | B2 | Vitest |
| `check` sobre raíces de fixture: set válido; un caso por `set.*`, `provenance.*`, `link.*`; versión nueva del ruleset con windup menor que rompe un set válido | B3, B4, B6 | Vitest |
| Ningún `provenance.json` del repo tiene llaves fuera del esquema (lo cubre el `check` real dentro de `just check`) | B4 | `just check` |
| `sprites` opcional: con y sin el campo, mismo `spec()`; `real_content.rs` y `balance.rs` pasan | B5 | `cargo test -p content`, `cargo test -p sim` |
| `ref_a`: frescura, import válido, reproducción con `prepare`/`sceneAt` contra `dummy_b` | B8 | Vitest |
| Reproducción visual en `/[lang]/dev/arena` desde `content/_local/sprites/ref_a/` | B8 | Manual, registrado en el PR |

## Alternativas consideradas
- **Windup visual igual al del motor (400/700/1100).** Más legible para el pesado, pero rompe C2 con cualquier `Timeline` que tenga impactos a 600 ms, incluidos el guion del spike y el `?pace` mínimo de 006. Hacerlo bien exige una `timeline_version` con separación mínima por movimiento y revisar la precedencia. Queda como trabajo futuro, en su propia feature.
- **Set dentro de `content/characters/<slug>/sprites/`.** Choca con el patrón del `id` (guiones frente a guiones bajos) y obliga a una regla de traducción slug → id. Además, ata un set a un solo personaje.
- **Celda por personaje o `scale` libre.** Da más libertad, pero el pivote, el encuadre y la regla de alcance pasan a depender de cada set, y la guía no puede dar una plantilla única. Con `pixel_art`, un `scale` no entero deforma.
- **Procedencia en markdown.** Es más amable de escribir, pero no se puede validar sin parsear texto libre, y el texto libre es justo donde se cuela un dato personal.
- **Dorado del ruleset en `crates/engine` (como `TIMELINE_CONTRACT`).** Es simétrico con 006, pero toca el motor, que el intent deja fuera, y el dato no es un contrato que el motor deba publicar.
- **Generar `GUIDE_LIMITS` desde Rust, o copiar el patrón del `id` a Rust.** Pondría una segunda fuente para números que son del renderer. Descartado: la fuente es TS, y Rust solo aporta los tiempos del ruleset.
- **Migrar los placeholders al estándar.** Mueve tests, capturas y la medición de 002 sin beneficio, y cambia su licencia si van a `content/`. El ejemplo de referencia es `ref_a`.
- **Validar paleta y contorno ya.** Fijaría un estilo antes de la identidad visual y le quitaría a 004 una decisión que es suya.

## Revisión
### product-owner — 2026-10-27 — APPROVE
- Verifiqué: los cuatro puntos del Resultado del intent (guía, export abierto + importador, vínculo + procedencia, `just check`) tienen RF y prueba, y cada criterio de éxito tiene su verificación (windup derivado con fallo explícito vía dorado RF-A5; `CharacterSpec` intacto en RF-B5; rechazos con códigos en RF-B1/B2; frescura de la guía en RF-B7). Restricciones: sin datos personales (esquema sin texto libre, patrón cerrado de `author`, cesión en `private/` y control humano en el PR, Área 4); sin elegir herramienta (RF-B1 y flujo); frontera con 004 (no recomienda herramienta, paleta y contorno quedan como recomendación y la enmienda de 004 sigue fuera); sin texto en código (familias `guide.*`, `export.*`, `set.*`, `provenance.*`, `link.*`, `ruleset.*` y CLI en JSON, ADR-0007); no redacta términos legales, así que `risk: medium` se sostiene. Las seis preguntas abiertas están respondidas con razón. Para quien produce arte son razonables: celda única de 192 sin pivote que declarar, PNG sueltos + un JSON, mínimo de ~27 frames por personaje (costo de encargo acotado), `ref_a` como plantilla y flujo con prueba en la arena antes del PR. No contradice ADR-0004 ni 0005; no hay dinero ni contenido con copyright, y el alcance no se sale del intent. `type: guide` en `docs/process.md` es aceptable: es aditivo, tiene el precedente de `legal` y no está entre los gates del owner.
- Notas para el plan (no bloquean): (1) RF-B4 justifica `ai_assisted` con "el checklist de ADR-0004", pero ese ADR no habla de IA (su checklist es "¿se parece demasiado?"); reformular como "registro para 004" o citar la fuente correcta. (2) La guía está en español por convención; si los artistas por encargo no lo leen, una traducción queda para 004, no para 007.

### game-designer — 2026-10-27 — REQUEST_CHANGES
- Verifiqué: `Ruleset::v1()` (windups 400/700/1100, `recovery_ms` 300, `knockdown_ms` 1500); `LIMITS.windupMaxMs` 400 y `MAX_WINDUP_MS` 400 escrito a mano en `scene/choreography.ts`, con su precedencia (el windup propio gana a la reacción); la spec de 006 (`strike_gap_min()` = 700, respiros con hueco ≥ 2.5 s y máximo ~4.6 s). C1 y C2 son correctas: con W ≤ 400 y 600 ms entre impactos, el windup propio empieza ≥ 200 ms después del impacto recibido, y `windupCap ≤ windup_<move>_ms` evita mirar antes del `attack` del motor. Atiende mis puntos 2 y 3: pose de reacción a ≤ 134 ms con ≥ 66 ms visibles antes del corte, R ≥ 200, `idle` de ≥ 4 frames, ≥ 3 distintos, ≤ 250 ms por frame y ciclo de 600–2400 ms. Los tiempos de `dummy_a` y `dummy_b` cumplen todas las reglas de tiempo (los recorrí uno por uno). Costo por personaje: el mínimo es de 27 frames y el máximo de 134, con deduplicación; un set como los placeholders (~49) es un encargo razonable y la celda única abarata el encuadre.
- Cambios pedidos:
  1. **El orden de anticipación no está garantizado.** El punto 1 afirma que "un pesado o un especial siempre se anticipan más que un ligero", pero los rangos absolutos lo permiten igual: ligero 250 y pesado 250 pasan, y un especial de 300 con un pesado de 400 también. Con solo 100–150 ms de margen bajo el tope de 400, la diferencia entre golpes tiene que medirse dentro del mismo set. Agregar una regla validada `guide.windup_order` con `W(attack_light) + windupStepMs ≤ W(attack_heavy) ≤ W(special)`, donde `windupStepMs` vive en `GUIDE_LIMITS` (propongo 100 ms, unos 6 refrescos, perceptible), con `data` `light_ms`, `heavy_ms`, `special_ms` y `min_step`. Corregir la frase del punto 1 y sumar su caso a la estrategia de pruebas. `dummy_a` la cumple (120/300/320); `dummy_b` no (especial 320 < pesado 380), pero los placeholders están fuera de la capa de guía.
- No bloquean: (a) considerar `impact_frame` ≥ 3 para `special`, para que se distinga del pesado por anticipación y no solo por tiempo; (b) la guía debería aclarar que el "respiro más largo" recomendado para `idle` sigue topado en 250 ms por frame.

**Respuesta del autor (architect, 2026-10-27):**
- game-designer (1, bloqueante): `windupStepMs: 100` está en `GUIDE_LIMITS` (tabla "Guía: números"). La regla `guide.windup_order` exige `W(attack_light) + windupStepMs ≤ W(attack_heavy) ≤ W(special)`, con `data` `light_ms`, `heavy_ms`, `special_ms` y `min_step`, en la tabla de reglas validadas. Una nota bajo la tabla por acción muestra que la regla es compatible con los rangos absolutos y el tope de 400, porque el ligero tiene un máximo de 250. La frase del punto 1 ahora dice lo que garantiza la regla, y la estrategia de pruebas suma sus casos (120/300/320 pasa; un paso < 100 falla; especial < pesado falla; `ref_a` la cumple).
- game-designer (a): tomado, `impact_frame` ≥ 3 para `special`. Cabe con su mínimo de 5 frames, y `dummy_a`/`dummy_b` (impacto en 4) la cumplirían igual.
- game-designer (b): tomado. El punto 3 aclara que el frame "más largo" de `idle` sigue topado en 250 ms y que la irregularidad sale de alternar frames más cortos.
- product-owner (nota 1): RF-B4 ya no cita ADR-0004. `ai_assisted` se justifica como dato de trazabilidad del origen, para 004 y para el owner, sin término legal. No cambia el esquema ni nada de lo que product-owner revisó, así que su aprobación se mantiene. La nota 2 (traducción de la guía) queda para 004, como propone.

### game-designer — 2026-10-27 — APPROVE
- Verifiqué lo afectado: `windupStepMs` 100 en "Guía: números"; `guide.windup_order` con `W(attack_light) + windupStepMs ≤ W(attack_heavy) ≤ W(special)` y sus `data` en la tabla de reglas; que siempre se puede cumplir bajo el tope de 400 (ligero ≤ 250, así que el pesado entra en 350–400 en el peor caso); la frase del punto 1, que ahora dice solo lo que garantiza la regla; sus casos en la estrategia de pruebas (pasa, paso < 100, especial < pesado, `ref_a`). También `impact_frame` ≥ 3 en `special` (cabe con su mínimo de 5 frames) y la aclaración del tope de 250 ms en el `idle` irregular. Los rangos de frames y tiempos de la guía quedan aprobados según el criterio del intent.
