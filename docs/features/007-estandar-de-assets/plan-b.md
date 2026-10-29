---
id: "007"
title: "Estándar de assets, fase B: export, importador, procedencia, vínculo y just check"
type: plan
status: approved
risk: medium
author: frontend-engineer
reviewers: [architect]
approvals:
  - by: architect
    date: 2026-10-28
linear: TA-25
depends_on: ["002", "003", "006"]
created: 2026-10-28
updated: 2026-10-28
---

# Plan: Estándar de assets, fase B (desde spec.md 2026-10-27)

> [intent](./intent.md) · [spec](./spec.md) · [plan-a](./plan-a.md) · [plan-b de 002](../002-spike-renderer/plan-b.md) (precedente de `tools/sff-converter`) · [spec de 006](../006-ritmo-rapido/spec.md) · ADRs [0001](../../decisions/0001-motor-propio-determinista.md), [0004](../../decisions/0004-personajes-homenaje.md), [0007](../../decisions/0007-i18n-desde-cero.md)

Alcance: RF-B1 a RF-B9 de la spec, la parte de "Guía: números", "Guía: reglas validadas", "Diseño" y "Estrategia de pruebas" que les aplica, y los cuatro nits que dejó la revisión del PR #48 (fase A, TA-22). La parte Rust (`crates/content`: campo `sprites`, `strike_gap_min_ms` en el dorado y sus pruebas) está redactada como la escribiría backend-engineer, para que la implemente quien tome el PR.

No toca `crates/engine`, `crates/engine-wasm`, `crates/protocol`, `packages/protocol-ts`, `apps/server`, `apps/web`, `tools/sff-converter`, `packages/renderer/placeholders/**` ni el formato `torneo.sprites` v1. El cambio en `packages/renderer` se limita a los nits, al campo nuevo de `RulesetTiming` y a una fila recomendada de `guideTable` (decisiones 1 y 2).

## Para el architect: lo que tienes que decidir
1. **Forma de cada línea de salida de la CLI: `{ code, file, path, data? }`.** RF-B2 pide líneas `{ "code", "path", "data" }` "con rutas relativas a la raíz del repo", y a la vez que `path` sea un JSON Pointer con los nombres del manifest. Las dos cosas no caben en un solo campo: un error de `guide.*` necesita el archivo (`content/sprites/x/manifest.json`) **y** el puntero (`/animations/idle/frames/1`). Propongo sumar `file`, relativo a la raíz del repo en `check` y relativo a la carpeta del export en `import` (el export puede estar fuera del repo, en una carpeta del usuario, y una ruta relativa al repo filtraría `../../Downloads/...`). `path` es siempre un JSON Pointer (vacío si el error es del archivo entero). Nunca hay rutas absolutas.
2. **Códigos que la spec no enumera.** La spec fija las familias y algunos códigos (`set.unexpected_entry`, `set.sheet_header`, `provenance.unknown_field`, `provenance.missing_field`, `link.invalid`, `link.unknown_set`, `link.misplaced_sprites`, `export.too_many_frames`). Propongo la lista cerrada de la decisión 6, con tres que conviene mirar:
   - `link.character_unreadable`: un `character.json` que el chequeo de Node no puede leer (symlink, > 64 KiB, JSON roto, raíz que no es objeto). Sin él, el chequeo tendría que callar (y `crates/content` lo reporta recién en `check-rust`) o reusar `link.invalid`, que dice otra cosa.
   - `provenance.invalid_value` con `path` (`/author`, `/origin`, `/date`…) en lugar de un código por campo, más `provenance.set_mismatch` aparte porque es una relación entre dos archivos.
   - `cli.*` e `io.*` para los errores de uso y de E/S (salida 2), distintos de los del contenido (salida 1).
3. **`"sprites": null` también se rechaza en Rust.** Con `Option<String>` y `#[serde(default)]`, serde acepta `null` como `None`, y el chequeo de Node lo rechaza (`link.invalid`, RF-B6). Propongo un `deserialize_with` mínimo que exige un string cuando la llave está, para que `null` dé `content.invalid_shape` y los dos lectores coincidan. Es lo que dice RF-B5 al pie de la letra ("no valida el valor más allá de que sea un string"): `null` no es un string. No valida el patrón, que sigue siendo del chequeo de Node.
4. **Subpath `./scene` en `@torneo/renderer` para la prueba de reproducción de `ref_a` (RF-B8).** La prueba necesita `prepareScene` y `sceneAt`, que solo salen por `@torneo/renderer` (la raíz, que carga `pixi.js/unsafe-eval` y `stage/`). Propongo exponer `./scene` (`src/scene/index.ts`, puro, ya en `PURE` de ESLint) y sumarlo a `guide-imports.test.ts` como punto de entrada que no llega a `guide/`, `stage/`, `player/` ni `pixi.js`. La alternativa es una excepción de ESLint en un solo test del importador para importar `../../../packages/renderer/src/scene/index.ts` por ruta relativa, que la configuración de `sff-converter` prohíbe a propósito.
5. **Destino del import con entradas ajenas.** El importador reemplaza los `sheet-N.png` previos (RF-B2). Propongo que se niegue (`cli.out_unexpected_entry`, salida 2, sin escribir nada) si el destino tiene cualquier otra cosa que `manifest.json`, `sheet-N.png` y, solo en `content/sprites/<id>/`, `provenance.json`. Eso incluye el marcador `.sff-converter-output`: una carpeta de `_local` escrita por el convertidor de MUGEN con el mismo `id` no se pisa en silencio.
6. **Frontmatter de `docs/guides/sprites.md`.** `type: guide` (RF-B7), `status: in-review`, `author: frontend-engineer`, `reviewers: [game-designer, architect]`, `linear: TA-25`. Pasa a `approved` con la revisión del PR: game-designer revisa la prosa de las reglas (las cifras salen generadas de lo que ya aprobó) y tú, el flujo y los formatos. Si prefieres otro revisor, se cambia solo el frontmatter.

Si te parecen bien, quedan como decisiones 15 a 20 sin más cambios.

**Resuelto por architect (2026-10-28):** los seis puntos se aceptan como decisiones 15 a 20, con estas precisiones:
- 15: `file` es una extensión de RF-B2 que acepto (la spec no podía expresar archivo y puntero en un campo). Las líneas `cli.*`, `io.*` e `internal.*` llevan `file` vacío o relativo a la raíz del repo; nunca la ruta de `--in` ni de `--out` tal como la escribió el usuario si queda fuera del repo. `import.test.ts` y `cli.test.ts` buscan el `tmpdir` en **todas** las líneas, incluidas las de error de uso.
- 16: lista cerrada aceptada, incluidos `link.character_unreadable`, `provenance.invalid_value` con `path` y `provenance.set_mismatch`. La tabla de códigos de la guía (sección 8) cubre exactamente `codes.ts`; un test lo comprueba (cada código de `EXPORT_CODES`, `SET_CODES`, `PROVENANCE_CODES`, `LINK_CODES` y `GUIDE_CODES` aparece en la guía fuera del bloque generado).
- 17: aceptado. `null` no es un string, así que `deserialize_with` cumple RF-B5 al pie de la letra y alinea los dos lectores (Área 3).
- 18: aceptado `./scene`. Es puro y ya está en `PURE`; `apps/web` sigue sin poder importarlo como valor (regla `rendererRestriction` de su ESLint, que cubre `@torneo/renderer/*`), así que no afecta a `check-renderer-split`. Se documenta en `docs/architecture.md`.
- 19: aceptado, marcador `.sff-converter-output` incluido.
- 20: aceptado el frontmatter de la guía con `reviewers: [game-designer, architect]`.
- Aclaro el Área 10 de la spec (que escribí yo): "de a una" se lee "un set a la vez", como dice la alternativa descartada; el tope de 64 MB por set se mantiene.

## Decisión sobre 006 (sin fallback)
006 (TA-21, PR #46) está en main desde `d1f0ffe`. `Ruleset::strike_gap_min()` existe (`crates/engine/src/ruleset.rs`, `pub const fn strike_gap_min(&self) -> u32`, 700 en v1, con su aserción en `v1_respects_the_bounds`). Por eso este plan **no usa el fallback** de "Dependencia con 006": `strike_gap_min_ms` entra en el dorado `crates/content/tests/golden/ruleset-timing.json` en este PR, y la fila "recuperación recomendada" de la guía muestra la cifra generada. Queda registrado aquí y en la descripción del PR.

## Decisiones de implementación

### Rust (`crates/content`)
1. **`strike_gap_min_ms` en el dorado.** En `crates/content/tests/ruleset_timing.rs`, `Timing` suma `strike_gap_min_ms: u32` entre `ruleset_version` y `windup_heavy_ms` (orden alfabético, que vigila `timing_fields_are_sorted`), y `timing()` lo llena con `r.strike_gap_min()`. Sigue usando solo API pública del motor. Se regenera con `UPDATE_GOLDEN=1 cargo test -p content --test ruleset_timing`; el JSON queda con `"strike_gap_min_ms": 700` en v1 y los otros cuatro valores sin cambios (se revisa a mano en el diff). El comentario del archivo cita 006 RF1. `every_listed_version_exists` suma una aserción: `strike_gap_min_ms ≥ windup_strike_ms` en cada versión (sanidad del dato, no regla de juego). `.gitattributes` suma `crates/content/tests/golden/* -text`, como los dorados del motor, que la fase A no agregó.
2. **Campo `sprites` (RF-B5).** En `src/file.rs`, `CharacterFile` suma como **último** campo:
   ```rust
   #[serde(default, deserialize_with = "present_string")]
   pub(crate) sprites: Option<String>,
   ```
   con `fn present_string<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> { String::deserialize(d).map(Some) }` (decisión 17: con la llave presente exige un string; ausente queda `None` por `default`). `deny_unknown_fields` no cambia. `into_parts` pasa de tupla a un struct privado `FileParts { spec, archetype, rivals, sprites }` para no crecer a una tupla de cuatro. `CharacterSpec` no cambia.
3. **`Character::sprites()`.** En `src/lib.rs`, `Character` suma `sprites: Option<String>` y `#[must_use] pub fn sprites(&self) -> Option<&str> { self.sprites.as_deref() }`, con un comentario: "id of the sprite set in content/sprites/ (007 RF-B5); the pattern and the existence are checked by `just check`, not here". El doc del módulo deja de citar `sprites/` como ejemplo de entrada ignorada dentro del personaje (ahora los sets viven en `content/sprites/`, y el chequeo de Node marca esa carpeta con `link.misplaced_sprites`); el cargador sigue ignorando otras entradas igual que hoy. `ContentError` no cambia: `null` o un número dan `InvalidShape`, que ya existe.
4. **Pruebas de `crates/content`.** Fixtures nuevos en `tests/fixtures/` (ya en `.prettierignore`), copias de `valid-solo-base/solo-one` con una línea más en `character.json`:
   - `sprites-link/solo-one`: `"sprites": "solo_one"`;
   - `sprites-null/solo-one`: `"sprites": null`;
   - `sprites-not-string/solo-one`: `"sprites": 3`;
   - `sprites-any-string/solo-one`: `"sprites": "Not An Id!"` (documenta que el crate no valida el patrón).

   Tests en `tests/load.rs`: `sprites_is_optional_and_never_reaches_the_spec` (cargan `valid-solo-base` y `sprites-link`; `spec()` y `canonical_spec_json(spec())` iguales byte a byte; `sprites()` es `None` y `Some("solo_one")`); `sprites_null_is_invalid_shape` y `sprites_not_a_string_is_invalid_shape` (`ContentError::InvalidShape` con su `path`); `sprites_pattern_is_not_checked_here` (carga con `Some("Not An Id!")`). `real_content.rs`, `crates/sim/tests/balance.rs` y `just sim --check` no cambian y tienen que seguir pasando. Sin crates nuevos: `serde` ya trae `Deserializer`; `Cargo.lock` no cambia, así que `just crate-age` no aplica (se confirma con `git diff --stat Cargo.lock` vacío).

### Renderer (`packages/renderer`)
5. **`RulesetTiming` con `strike_gap_min_ms`.** `guide/ruleset.ts` suma el campo al tipo y a `TIMING_KEYS` (llaves exactas: un dorado sin el campo da `ruleset.invalid` en `/versions/0/strike_gap_min_ms`). `guide/table.ts` suma a `GuideRow` `recovery_recommended_max_ms?: number` en las tres filas de ataque, igual a `min_v strike_gap_min_ms(v)` (700 en v1). Es una **recomendación**: `validateGuide` no la lee y ningún código la valida (spec, punto 2 de game-designer). Las pruebas de la fase A que arman `RulesetTiming` a mano (`guide-ruleset.test.ts`, `guide-table.test.ts`, `guide-validate.test.ts`, `support/guide-ref.ts`) suman el campo. La atadura de `guide-ruleset.test.ts` (decisión 6 de plan-a) suma, por versión: `strike_gap_min_ms ≥ REACTION_WINDOW_MS`, para que la recomendación nunca quede por debajo del mínimo validado.
6. **Nit (a): orden de `idle_distinct`.** La revisión de qa (PR #48) marcó que el código emite `idle_frame_long` y `cycle_range` antes de `idle_distinct`, contra el orden de la decisión 3.5 de plan-a. Se corrige el código, no el plan: `checkTiming` se parte en `checkCounts` (`frames_count`, `frame_ms_range`) y `checkDurations` (el resto), y el lazo de `validateGuide` queda `checkCounts` → `idleDistinct` (solo `idle`) → `checkDurations` → píxeles de `idle` → alcance. Caso nuevo en `guide-validate.test.ts`, con dos errores y `toEqual` sobre la lista ordenada: `idle.frames = [0,1,0,1]`, `frame_ms = [300,200,200,200]` (ciclo 900, dentro de rango) da `[idle_distinct { count: 2 }, idle_frame_long /frame_ms/0 { ms: 300 }]`.
7. **Nit (b): celdas vacías en `idleDistinct`.** Criterio: `idle_distinct` es una regla de píxeles y se comporta como las demás (sin cascada). Si algún frame que usa `idle` no está en `boxes` (falló `cell_size` o `frame_empty`), `idle_distinct` no se evalúa: el set ya falla con un error más preciso. `idleDistinct` recibe `boxes` y sale antes si falta alguno. Casos nuevos: celdas 1 y 2 vacías da exactamente `[frame_empty /frames/1, frame_empty /frames/2]`, sin `idle_distinct`; y celda 1 con `w: 190` da solo `cell_size`. El caso existente `[0,1,0,1]` sigue dando `idle_distinct { count: 2 }`.
8. **Nit (c): `CONTACT_PX` sin arrastrar `scene/choreography.ts`.** La revisión del architect pidió que, cuando una herramienta Node importe `./guide`, `CONTACT_PX` no traiga todo `scene/`. `CONTACT_PX = 64` se mueve a `src/scene/types.ts` (geometría del escenario, junto a `STAGE_WIDTH`, `GROUND_Y` y `MARK_X`; ese archivo solo tiene `import type`). `choreography.ts` lo importa de `./types.ts`; `scene/index.ts` ya reexporta `types.ts` con `export *`, así que se quita `CONTACT_PX` de su lista de `choreography.ts`; `test/choreography.test.ts` sigue importando de `../src/scene/index.ts` sin cambios. `guide/validate.ts` importa `CONTACT_PX` de `../scene/types.ts`. Comportamiento idéntico (mismo valor, mismo nombre público).
9. **Nit (d): más puntos de entrada en `guide-imports.test.ts`.** El `it.each` suma `manifest/index.ts`, `timeline/index.ts` y `scene/index.ts` (este último, el subpath de la decisión 18). Un caso nuevo fija el cierre de `guide/index.ts` dentro de `scene/`: solo puede contener `src/scene/types.ts` (si alguien vuelve a importar `choreography.ts`, falla). El caso de `package.json` comprueba también `./scene`.

### Herramienta (`tools/sprite-importer`)
10. **Esqueleto, igual que `tools/sff-converter`.** `package.json` (`@torneo/sprite-importer`, `private`, `AGPL-3.0-only`, `type: module`), dependencias `@torneo/renderer: workspace:*` y `pngjs: 7.0.0`, y devDependencies con las **mismas versiones exactas** que ya fija `sff-converter` (`@eslint/js 10.0.1`, `@types/node 22.20.5`, `@types/pngjs 6.0.5`, `eslint 10.13.0`, `eslint-config-prettier 10.1.8`, `globals 17.13.0`, `typescript 6.0.3`, `typescript-eslint 8.72.0`, `vitest 5.0.5`). No hay paquetes nuevos en el lockfile: `pnpm install` solo agrega el importer del workspace (`pnpm-workspace.yaml` ya incluye `tools/*`). Scripts:
    ```json
    "import":       "node --experimental-strip-types --disable-warning=ExperimentalWarning src/cli.ts import",
    "check":        "node --experimental-strip-types --disable-warning=ExperimentalWarning src/cli.ts check",
    "gen:guide":    "node --experimental-strip-types --disable-warning=ExperimentalWarning scripts/gen-guide.ts",
    "gen:fixture":  "node --experimental-strip-types --disable-warning=ExperimentalWarning scripts/gen-fixture.ts",
    "typecheck", "lint", "test"   (como sff-converter)
    ```
    `tsconfig.json` copia el de `sff-converter` e incluye `scripts`. `eslint.config.js` copia su estructura (sin reloj ni aleatoriedad fuera de `src/cli.ts`) y abre los imports a exactamente `@torneo/renderer/manifest`, `@torneo/renderer/guide` y, solo en `test/`, `@torneo/renderer/scene`. `test/eslint.test.ts` cubre esas tres fronteras y `Math.random` prohibido. `README.md` corto: qué es, los tres comandos y el enlace a la guía (la guía es la fuente; el README no repite reglas).
11. **Módulos de `src/`** (cada uno puro salvo `fsx.ts`, `write.ts` y `cli.ts`):
    - `codes.ts`: `EXPORT_CODES`, `SET_CODES`, `PROVENANCE_CODES`, `LINK_CODES`, `CLI_CODES`, `IO_CODES` (decisión 6 y "Para el architect", punto 2).
    - `limits.ts`: `TOOL_LIMITS = { jsonFileMaxBytes: 64 * 1024, columns: Math.floor(LIMITS.sheetSideMax / GUIDE_LIMITS.cellPx) }` (10 con 192). El tope de 64 KiB de `manifest.json`, `provenance.json` y `character.json` (RF-B3, igual a `content::MAX_FILE_BYTES`). Todo lo demás se lee de `GUIDE_LIMITS` y `LIMITS`.
    - `report.ts`: `Issue = { code, file, path, data? }` y el emisor de líneas JSON (una por error, llaves en ese orden, `data` solo con strings y números).
    - `ihdr.ts`: `readIhdr(bytes)`: firma PNG, primer chunk `IHDR` de largo 13, `width`, `height`, `bitDepth`, `colorType`, `interlace`. No descomprime nada. Error: `null` (quien llama elige el código).
    - `fsx.ts`: lecturas acotadas y sin symlinks: `listDir(dir)` con `lstat` por entrada (orden por bytes del nombre, como `load_roster`), `readBounded(file, max)` que hace `lstat`, abre, comprueba con `fstat` que es el mismo archivo regular y lee como mucho `max + 1` bytes (más que eso es "demasiado grande").
    - `export.ts`: `readExport(dir)` (RF-B1, decisión 12).
    - `pack.ts`: deduplicación y rejilla (decisión 13).
    - `import.ts`: `runImport({ root, inDir, outDir })`, todo en memoria hasta el final (decisión 13).
    - `write.ts`: comprobación del destino y escritura (decisión 13).
    - `set.ts`, `provenance.ts`, `link.ts`, `check.ts`: `runCheck({ root })` (decisión 14).
    - `golden.ts`: lee `<root>/crates/content/tests/golden/ruleset-timing.json` (acotado a 64 KiB) y lo pasa por `parseRulesetTiming`. Si falla, `io.golden_invalid` (salida 2): sin rulesets no hay guía.
    - `cli.ts`: `main(argv, env)`; raíz por defecto desde la ubicación del archivo (`../../../`, como `sff-converter`); rutas relativas contra `INIT_CWD`. `--root` solo en `check` (RF-B6), nunca en `import`: así el destino del import queda cerrado al repo real. Salida 0 si pasa, 1 si el contenido no cumple, 2 ante error de uso o de E/S; una excepción inesperada sale como `{ "code": "internal.unexpected" }` con 2, sin traza ni ruta.
12. **Lectura del export (RF-B1).** En este orden, y sin decodificar ningún PNG hasta que todo lo anterior pasa:
    1. La raíz contiene exactamente `export.json` y `frames/` (otra cosa: `export.unexpected_entry`; un symlink en cualquier nivel: `export.symlink`).
    2. `export.json` ≤ `exportFileMaxBytes`, JSON, objeto con llaves exactas `format`, `format_version`, `id`, `pixel_art`, `animations` (`export.unknown_field`, `export.missing_field`); `format === 'torneo.sprites.export'` y `format_version === 1` (`export.unsupported_format`); `id` cumple `LIMITS.idPattern` y `pixel_art` es booleano (`export.invalid_value`); cada llave de `animations` es un código de animación conocido (`export.unknown_animation`); cada animación es un objeto con llaves de `{frame_ms, loop, impact_frame}` (desconocidas: `export.unknown_field`; faltan `frame_ms` o `loop`: `export.missing_field`); `frame_ms` es un array de largo 1 a `LIMITS.animationFramesMax` (`export.invalid_value`). Los valores de `frame_ms`, `loop` e `impact_frame` no se validan aquí: pasan tal cual y los valida `validateManifest` (un solo validador).
    3. `frames/` contiene solo carpetas con el nombre de una animación declarada (`export.unexpected_entry`); cada animación declarada tiene su carpeta (`export.frame_missing` en `/animations/<a>/frames`); cada carpeta tiene exactamente `00.png` … `NN.png` contiguos, con `NN + 1 = frame_ms.length` (`export.frame_missing` y `export.frame_extra`, con `path /animations/<a>/frames/<k>`, y `export.unexpected_entry` para otro nombre).
    4. Por PNG: tamaño ≤ `exportFileMaxBytes` (`export.file_too_large`), IHDR legible (`export.png_invalid`) y de `cellPx × cellPx` (`export.png_size`, `data { w, h }`). Solo entonces `PNG.sync.read`; un fallo del decodificador es `export.png_invalid`. La salida de `pngjs` es siempre RGBA de 8 bits (paleta, gris, RGB y 16 bits incluidos); se comprueba `data.length === cellPx² × 4`.

    `file` es relativo a la carpeta del export (`export.json`, `frames/attack_light/02.png`) y `path` usa los nombres del manifest (`/animations/attack_light/frames/2`). El número de archivos que se abren está acotado por 12 animaciones × `animationFramesMax`, y la memoria, por `cellPx² × 4` × frames (≤ 64 × 12 × 147 KiB en el peor caso teórico, unos 113 MB; con los máximos de la guía, 134 frames, unos 20 MB). Si el export no pasa este paso, el import termina con 1 sin construir el manifest.
13. **Import (RF-B2).**
    - **Destino.** `--out` se resuelve contra `INIT_CWD` y tiene que ser exactamente `<root>/content/sprites/<id>` o `<root>/content/_local/sprites/<id>`, con `<id>` igual al `id` del export (`cli.out_forbidden` y `cli.out_id_mismatch`, salida 2). Cada componente existente desde `<root>` hasta `<id>` se mira con `lstat`: un symlink da `cli.out_symlink`. Si `<id>` existe, solo puede contener lo de "Para el architect", punto 5 (`cli.out_unexpected_entry`). Todo esto se comprueba **antes** de leer el export, para no decodificar en vano.
    - **Empaquetado determinista.** Frames en orden canónico (`REQUIRED_ANIMATIONS`, luego `OPTIONAL_ANIMATIONS`, cada uno por `nn`). Deduplicación por contenido: hash FNV-1a de 32 bits de los bytes RGBA para agrupar y comparación byte a byte para confirmar (sin `node:crypto`, que ESLint prohíbe en el paquete); el primer frame con un contenido dado fija su índice. Rejilla: `TOOL_LIMITS.columns` columnas (10) y como mucho 10 filas por hoja, sin margen entre celdas (no hace falta: `guide.frame_clipped` garantiza que el borde de cada celda es transparente, así que el filtrado lineal no mezcla vecinos). Hoja `k`: ancho `min(n_k, 10) × 192`, alto `ceil(n_k / 10) × 192`. Más de `LIMITS.sheetsMax × 100` frames únicos da `export.too_many_frames` (`data { frames, max }`). Cada frame del manifest: `{ sheet, x, y, w: 192, h: 192, pivot_x: 96, pivot_y: 184 }` (el pivote lo fija el estándar). `animations[a] = { frames: [índices], frame_ms, loop, impact_frame? }` tal cual del export. Sin `scale`.
    - **Validación.** `validateManifest(manifest, { expectedId: id })`; si falla, sus errores salen con `file: "export.json"` (los `path` coinciden con el export) y termina con 1. Después `validateGuide(manifest, sheets, rulesets)` con el dorado de `golden.ts`; las hojas se arman en memoria con el tamaño exacto del manifest, así que la precondición de `guide.sheets_mismatch` se cumple por construcción. Los errores de la guía salen con `file` del PNG de origen cuando el `path` apunta a un frame de animación y `export.json` si no.
    - **Escritura.** Solo si todo pasó: los PNG se codifican en memoria con `PNG.sync.write(png, { colorType: 6 })`; después se crea el directorio si falta, se borran los `sheet-N.png` previos, se escriben las hojas y al final `manifest.json` (`JSON.stringify(manifest, null, 2) + '\n'`). `provenance.json` no se toca ni se crea. Un fallo de E/S a mitad de camino sale con 2 (`io.write_failed`); el estado intermedio no pasa `check` (faltaría o sobraría una hoja, o el manifest no coincide), así que no se cuela en un PR.
    - **Resumen.** Si pasa, una línea `{ "frames", "sheets", "colors", "partial_alpha_pixels" }`: frames únicos, hojas, colores RGBA distintos entre los píxeles con alfa > 0 y píxeles con alfa entre 1 y 254, contados sobre los frames únicos.
14. **Chequeo (RF-B3, RF-B4, RF-B6).** `runCheck({ root })` recorre, en orden por bytes del nombre:
    1. **`content/sprites/`.** Si no existe, cero sets. En la raíz, `README.md` (archivo regular) se ignora; cualquier otro archivo o symlink da `set.unexpected_entry`; cada directorio es un set. En cada set:
       - entradas: exactamente `manifest.json`, `provenance.json` y las hojas que declara el manifest; faltante: `set.missing_file`; otra cosa (incluido `.DS_Store`, subcarpetas, symlinks, una hoja no declarada): `set.unexpected_entry`;
       - `manifest.json` ≤ 64 KiB (`set.file_too_large`), JSON (`set.invalid_json`), `validateManifest` con `expectedId` = nombre de la carpeta (errores `manifest.*` tal cual);
       - si el manifest es válido: por hoja, ≤ `sheetFileMaxBytes` (`set.file_too_large`), IHDR igual a `sheets[i]` y RGBA de 8 bits, `colorType 6`, `bitDepth 8` (`set.sheet_header`, `data { width, height, bit_depth, color_type }`); recién entonces se decodifican las hojas del set (como mucho 4 de 2048², 64 MB) y se llama a `validateGuide` contra **todas** las versiones del dorado. Si una hoja falla el IHDR o el decodificador, `validateGuide` no se llama: así un set del repo nunca produce el `RangeError('guide.sheets_mismatch')` (condición del architect en plan-a). Las hojas se sueltan antes del set siguiente;
       - `provenance.json` (siempre, aunque el manifest falle): ≤ 64 KiB, JSON (`provenance.invalid_json`), objeto con las llaves exactas de RF-B4 (`provenance.unknown_field`, `provenance.missing_field`), `format`/`format_version` (`provenance.invalid_value` en `/format`), `set` igual al nombre de la carpeta (`provenance.set_mismatch`), `author` con `^[a-z0-9]+(-[a-z0-9]+)*$` y largo 2–32, `origin` en `original | commission | generated`, `date` `YYYY-MM-DD` válida del calendario (se comprueba con aritmética entera, sin `Date`, que ESLint prohíbe), `ai_assisted` booleano y `license === "content"` (todos `provenance.invalid_value` con su `path`).
    2. **`content/characters/`.** Por cada directorio (no symlink; los archivos sueltos como el README se ignoran, igual que `load_roster`): si existe `<slug>/sprites` (de cualquier tipo), `link.misplaced_sprites`; se lee `character.json` acotado a 64 KiB (`link.character_unreadable` si no se puede leer, es un symlink, es demasiado grande, no es JSON o no es un objeto); si tiene la llave propia `sprites`: string que cumple `LIMITS.idPattern` o `link.invalid` en `/sprites` (también `null`), y `content/sprites/<id>/` tiene que ser un directorio (no symlink) o `link.unknown_set`. No se valida nada más del personaje.

    Un set sin personaje que lo enlace y un set compartido están permitidos. Al final, si no hubo errores, una línea `{ "sets", "characters", "links" }` y salida 0. Con cero sets pasa (es el estado del repo tras este PR).
15. a 20. Ver "Para el architect", puntos 1 a 6, con las precisiones de "Resuelto por architect".

### Export de referencia (RF-B8)
21. **`ref_a`, geométrico y propio.** `scripts/ref/draw.ts` dibuja con rectángulos (cuerpo, cabeza y brazo; un color de relleno, un contorno oscuro de 1 px y alfa binaria, para cumplir también las recomendaciones de paleta) y devuelve `{ exportJson, frames: Map<'<anim>/<nn>', RgbaImage> }`. No reutiliza `packages/renderer/scripts/placeholders/draw.ts`: sus primitivas no se exportan y la configuración de ESLint de las herramientas prohíbe importar `packages/**` por ruta relativa; las pocas primitivas que hacen falta (rectángulo, contorno) son unas 30 líneas. Tiempos: los de `support/guide-ref.ts` de la fase A (los de `dummy_a`, que cumplen todas las reglas, incluido `windup_order` 120/300/320). Píxeles: cuerpo de 40 px de ancho con pies en la fila 183, alto opaco 120; `idle` con 3 poses distintas (respiración de 1 px); en cada ataque, windup con el brazo atrás y frame de impacto con el brazo hasta la columna 136 (`reach` 40 ≥ 32). Unos 49 PNG de 192² casi transparentes, unos pocos KB cada uno.
22. **`gen:fixture` y frescura.** `scripts/gen-fixture.ts` escribe `tools/sprite-importer/fixtures/export-ref/` (`export.json` y `frames/<anim>/<nn>.png`), borrando antes lo que haya. `tools/sprite-importer/fixtures/` se suma a `.prettierignore` (el JSON lo escribe el script, como los manifests de los placeholders). La prueba de frescura compara la lista exacta de archivos, los bytes de `export.json` y los **píxeles** decodificados de cada PNG con lo que devuelve `draw.ts` (no los bytes del PNG, que dependen de la versión de `zlib`; mismo criterio que `placeholders.test.ts`). AGPL, como los placeholders: es código y figuras generadas, sin personaje ni marca.

### Guía de arte (RF-B7)
23. **`docs/guides/sprites.md`.** Frontmatter de "Para el architect", punto 6. Secciones, en este orden:
    1. Para quién es y qué no es: no elige ni recomienda herramientas de producción de arte (eso es de 004), y los placeholders `dummy_a`/`dummy_b` no son modelo (Área 5 de la spec).
    2. Celda, pivote, facing, alturas y lectura a 375 px.
    3. Acciones obligatorias y opcionales con su fallback (`ANIMATION_FALLBACK`).
    4. Tiempos: frames por acción, windup y su tope (C1 y C2, citando `TIMELINE_LIMITS.strikeGapMs`, `REACTION_WINDOW_MS`, `windup_strike_ms`/`windup_heavy_ms`/`windup_special_ms`), `windup_order`, frame de impacto, recuperación (validada y recomendada, citando `strike_gap_min()`), reacciones y `idle` en los respiros de 006 (con la aclaración de game-designer: el frame "largo" sigue topado por `frame_max_ms`).
    5. Bloque generado (decisión 24).
    6. Formato de export, importador, set y procedencia, con el flujo "export → import a `content/_local/sprites/<id>` → probar en `/[lang]/dev/arena` con `just dev` → import a `content/sprites/<id>` → `provenance.json` a mano → PR que declara que existe `private/provenance/<id>.md`, sin detalles".
    7. Recomendaciones no validadas: paleta (≤ 32 colores por set en pixel art, alfa binaria) y contorno oscuro de 1 px lógico, medibles con el resumen del importador (`colors`, `partial_alpha_pixels`). Las cifras de estas recomendaciones no son de tiempo; van en el bloque generado igual, como constantes del generador (`RECOMMENDED`), para que la prosa no tenga números sueltos.
    8. Tabla de códigos: cada código `guide.*`, `export.*`, `set.*`, `provenance.*` y `link.*` con una línea que lo explica (los códigos de la CLI no tienen texto; la guía es su diccionario para quien lee la salida).

    En español (convención de `docs/`; una traducción para artistas por encargo es de 004, nota 2 de product-owner).
24. **Bloque generado y su frescura.** `src/guide-doc.ts` exporta `renderGuideBlock(rulesets): string`, puro, que arma tres tablas markdown a partir de `GUIDE_LIMITS`, `guideTable(rulesets)`, `LIMITS`, `TIMELINE_LIMITS`, `REACTION_WINDOW_MS` y `CONTACT_PX`: "Celda y encuadre" (celda, pivote derivado, alturas, `feetTolerancePx`, `frameMsMin`/`frameMsMax`, pesos de archivo), "Por acción" (una fila por animación, en el orden de `guideTable`, con frames, duración o ciclo, windup con su tope y, si lo limita una versión, la versión, impacto, recuperación mínima y recomendada, primer frame de las reacciones) y "Recomendaciones" (paleta y contorno). Los encabezados de las tablas son constantes en español dentro del generador: es un generador de documentación, no UI ni dominio (ADR-0007 aplica a la UI, al motor y al protocolo). `scripts/gen-guide.ts` lee el dorado, llama a `renderGuideBlock` y reemplaza lo que hay entre `<!-- guide:generated:start -->` y `<!-- guide:generated:end -->`; si los marcadores faltan o están repetidos, sale con 2 sin escribir. Prettier ya ignora `**/*.md` y `docs/`. La prueba `test/guide-doc.test.ts` lee `docs/guides/sprites.md` con `readFileSync`, extrae el bloque y lo compara con `renderGuideBlock(rulesets del dorado real)`; si difiere, falla con "run pnpm --filter @torneo/sprite-importer gen:guide". Una segunda prueba recorre el texto **fuera** del bloque y falla si encuentra una cifra de tiempo (`/\b\d+\s?ms\b/`), para cumplir "la prosa no escribe cifras: nombra la fuente".
25. **`type: guide` en `docs/process.md`.** Se suma `guide` a la lista de `type:` del esquema de frontmatter y una línea en "Estructura" que dice que las guías transversales viven en `docs/guides/`. Cambio aditivo (Área 9 de la spec).

### `just check`, CI y política
26. **`justfile`.** En `check-web`, después del bloque de `sff-converter`:
    ```
    pnpm --filter @torneo/sprite-importer exec tsc -p tsconfig.json --noEmit
    pnpm --filter @torneo/sprite-importer exec eslint .
    pnpm --filter @torneo/sprite-importer exec vitest run
    pnpm -s --filter @torneo/sprite-importer check
    ```
    En `test`, el `vitest run` y el `check` del paquete. El comentario de la receta `test` suma "sprite importer". El `check` corre sobre el repo real: valida todo set commiteado, su procedencia y los vínculos (RF-B6), y lee el dorado, que `check-rust` revalida después contra el motor.
27. **Filtros de `ci.yml`.** El job `web` corre `just check-web`, que ahora lee archivos fuera de sus filtros. Se suman al filtro `web`: `content/sprites/**`, `content/characters/**`, `crates/content/tests/golden/**` y `docs/guides/**`. `tools/**` ya está. El filtro `rust` no cambia (`content/**` y `crates/**` ya están). Sin jobs nuevos, así que la tabla de permisos de `scripts/check-workflows.mjs` no cambia. En `scripts/__tests__/check-workflows.test.mjs` (job `policy`), un caso nuevo en "ci.yml filters cover what each job runs": el filtro `web` cubre `content/sprites/x/manifest.json`, `content/characters/x/character.json`, `crates/content/tests/golden/ruleset-timing.json` y `docs/guides/sprites.md`. `node scripts/check-workflows.mjs` tiene que seguir pasando.
28. **Docker y `.dockerignore`.** Sin cambios: `tools/` ya está fuera del contexto (`check-compose` lo exige) y el Dockerfile de `web` instala con `--filter 'web...'`, que no incluye la herramienta. `docs/` y `content/_local/` también están fuera.

### Documentación (RF-B9)
29. **Archivos.**
    - `content/characters/README.md`: el árbol pierde `sprites/`; una línea con `"sprites": "<id>"` opcional y el enlace a `content/sprites/`.
    - `content/sprites/README.md` (nuevo): qué va (sets con `manifest.json`, hojas y `provenance.json`), qué no (MUGEN, franquicias, fuentes de herramientas, exports), la procedencia sin datos personales con `private/provenance/<id>.md`, la licencia (`LICENSE-CONTENT.md`) y el enlace a la guía. Es el único archivo suelto que el chequeo acepta en la raíz.
    - `.claude/skills/write-character/SKILL.md`: árbol y fila de la tabla para `sprites` (opcional, patrón `LIMITS.idPattern` validado por `just check`, fuera de la forma motor).
    - `docs/features/003-motor-de-pelea/spec.md`, "Esquema de personaje": enmienda fechada con `sprites` opcional y fuera de la forma motor (sin tocar el resto de la spec aprobada).
    - `docs/features/002-spike-renderer/spec.md`, nota 8 de "Áreas de preocupación": los personajes reales van en `content/sprites/<id>/` (enmienda fechada que remite a 007).
    - `docs/architecture.md`: `tools/sprite-importer`, `content/sprites/` y la capa de guía (`@torneo/renderer/guide`), y `./scene` si se acepta la decisión 18.
    - `docs/glossary.md`: *set de sprites*, *export*, *procedencia*, *capa de guía*.
    - `CLAUDE.md`, "Comandos": `import`, `check` y `gen:guide` (con `-s`), y la lista de `check-web` y `test`.

    La enmienda del intent de 004 sigue fuera (spec, RF-B9).

### Sets reales en este PR
30. **No se commitea ningún set bajo `content/sprites/`.** Ninguno tiene hoy procedencia ni encargo detrás, y elegir cómo se produce el arte es de 004. El set de PixelLab que existe sin commitear en el checkout principal (`content/_local/sprites/ogro_del_pantano/`) es un manifest con `scale: 2` y frames de 64 px (no un export) y queda **fuera del repo**: solo sirve de prueba manual del importador (paso 13 del orden de trabajo), con la conversión a export hecha por un script descartable en el directorio temporal de la sesión, nunca en el repo. El PR no lo cita como set del repo ni adjunta imágenes suyas. El chequeo pasa con cero sets, y todas sus ramas se prueban con raíces de fixture armadas en un directorio temporal (decisión 31).
31. **Raíces de fixture sin PNG commiteados.** `test/support/fixture-root.ts` arma en `mkdtemp` una raíz mínima: `crates/content/tests/golden/ruleset-timing.json` (copia del real o una variante), `content/sprites/ref_a/` (resultado de `runImport` sobre `fixtures/export-ref` en otra raíz temporal, más un `provenance.json` válido), `content/sprites/README.md` y `content/characters/<slug>/character.json` con o sin `sprites`. Cada caso parte de la raíz válida y cambia una sola cosa (borrar, agregar, sobrescribir bytes, crear un symlink). Los PNG malformados de los casos de IHDR se generan en el test (`pngjs` con otro `colorType`, bytes truncados). Así el repo solo lleva los ~49 PNG de `export-ref`.

## Archivos que cambian
| Archivo | Cambio |
|---|---|
| `crates/content/tests/ruleset_timing.rs` | modificado: `strike_gap_min_ms` (decisión 1) |
| `crates/content/tests/golden/ruleset-timing.json` | regenerado con `UPDATE_GOLDEN=1`: `"strike_gap_min_ms": 700` |
| `crates/content/src/file.rs` | modificado: `sprites` con `present_string`, `FileParts` (decisión 2) |
| `crates/content/src/lib.rs` | modificado: `Character::sprites()`, doc del módulo (decisión 3) |
| `crates/content/tests/load.rs` | modificado: cuatro tests de `sprites` (decisión 4) |
| `crates/content/tests/fixtures/sprites-{link,null,not-string,any-string}/solo-one/*` | nuevos (decisión 4) |
| `.gitattributes` | modificado: `crates/content/tests/golden/* -text` |
| `packages/renderer/src/guide/ruleset.ts` | modificado: `strike_gap_min_ms` (decisión 5) |
| `packages/renderer/src/guide/table.ts` | modificado: `recovery_recommended_max_ms` (decisión 5) |
| `packages/renderer/src/guide/validate.ts` | modificado: orden de `idle_distinct`, sin cascada, `CONTACT_PX` de `scene/types.ts` (decisiones 6 a 8) |
| `packages/renderer/src/scene/types.ts` | modificado: `CONTACT_PX` (decisión 8) |
| `packages/renderer/src/scene/choreography.ts`, `src/scene/index.ts` | modificados: `CONTACT_PX` desde `types.ts` (decisión 8) |
| `packages/renderer/package.json` | modificado: subpath `./scene` (decisión 18) |
| `packages/renderer/test/guide-imports.test.ts` | modificado: entradas nuevas, cierre de `scene/` (decisión 9) |
| `packages/renderer/test/guide-{ruleset,table,validate}.test.ts`, `test/support/guide-ref.ts` | modificados: campo nuevo, casos de los nits (decisiones 5 a 7) |
| `tools/sprite-importer/{package.json,tsconfig.json,eslint.config.js,vitest.config.ts,README.md}` | nuevos (decisión 10) |
| `tools/sprite-importer/src/*.ts` | nuevos (decisiones 11 a 14 y 24) |
| `tools/sprite-importer/scripts/{gen-fixture.ts,gen-guide.ts,ref/draw.ts}` | nuevos (decisiones 21, 22 y 24) |
| `tools/sprite-importer/fixtures/export-ref/**` | nuevo, generado y commiteado (decisión 22) |
| `tools/sprite-importer/test/*.test.ts`, `test/support/*.ts` | nuevos (ver "Pruebas") |
| `pnpm-lock.yaml` | modificado: solo el importer nuevo, sin paquetes nuevos |
| `.prettierignore` | modificado: `tools/sprite-importer/fixtures/` |
| `justfile` | modificado: `check-web` y `test` (decisión 26) |
| `.github/workflows/ci.yml` | modificado: filtro `web` (decisión 27) |
| `scripts/__tests__/check-workflows.test.mjs` | modificado: caso del filtro `web` (decisión 27) |
| `docs/guides/sprites.md` | nuevo (decisiones 23 y 24) |
| `docs/process.md` | modificado: `type: guide` (decisión 25) |
| `docs/architecture.md`, `docs/glossary.md`, `CLAUDE.md` | modificados (decisión 29) |
| `docs/features/003-motor-de-pelea/spec.md`, `docs/features/002-spike-renderer/spec.md` | enmiendas fechadas (decisión 29) |
| `content/characters/README.md`, `content/sprites/README.md`, `.claude/skills/write-character/SKILL.md` | modificados / nuevo (decisión 29) |
| `docs/features/007-estandar-de-assets/plan-b.md` | este plan; "Desviaciones" si hace falta |

No cambian: `crates/engine`, `crates/engine-wasm`, `crates/protocol`, `packages/protocol-ts`, `apps/server`, `apps/web`, `tools/sff-converter`, los placeholders, `Cargo.toml`/`Cargo.lock`, `apps/web/Dockerfile`, `.dockerignore`, `scripts/check-workflows.mjs` y `scripts/check-compose.mjs`.

## Orden de trabajo
1. **Rust: dorado** (decisión 1). `ruleset_timing.rs`, `UPDATE_GOLDEN=1 cargo test -p content --test ruleset_timing`, revisar el diff del JSON (solo una llave nueva), `.gitattributes`. Commit `test(content): add strike_gap_min_ms to the ruleset timing golden (TA-25)`.
2. **Rust: `sprites`** (decisiones 2 a 4): primero los fixtures y los tests de `load.rs` en rojo, después `file.rs` y `lib.rs`. `cargo test -p content`, `cargo test -p sim`, `cargo clippy -p content --all-targets -- -D warnings`. Commit `feat(content): optional sprites link in character.json (TA-25)`. **Paralelizable** con 3 y 4 (archivos distintos).
3. **Renderer: dorado nuevo y nits** (decisiones 5 a 9), después del paso 1. Primero los casos nuevos en rojo (orden de `idle_distinct`, celdas vacías, cierre de `scene/` en `guide-imports`), después el código. `pnpm --filter @torneo/renderer exec vitest run`, `tsc` ×2 y `eslint`. Commit `fix(renderer): guide nits from the phase A review (TA-25)` y, aparte, `feat(renderer): strike_gap_min_ms and the scene subpath (TA-25)`.
4. **Esqueleto de la herramienta** (decisión 10): `package.json`, `pnpm install` (comprobar que el lockfile no suma paquetes: `git diff pnpm-lock.yaml` solo con el importer), `tsconfig`, ESLint con `eslint.test.ts`, `vitest.config.ts`, las tres primeras líneas nuevas de `check-web`. **Paralelizable** con 2.
5. **Núcleo**: `codes.ts`, `limits.ts`, `report.ts`, `ihdr.ts`, `fsx.ts`, `golden.ts`, con sus tests.
6. **Export** (decisión 12) con `export.test.ts`: un caso por código `export.*`.
7. **Export de referencia** (decisiones 21 y 22): `draw.ts`, `gen:fixture`, generar y commitear `fixtures/export-ref/`, `.prettierignore`, `fixture.test.ts` (frescura). Necesita 5 para los tipos.
8. **Import** (decisión 13): `pack.ts`, `import.ts`, `write.ts`, con `pack.test.ts` e `import.test.ts` (sobre `export-ref` y mutaciones), y `replay.test.ts` (necesita el subpath del paso 3). Es el paso más riesgoso: mezcla E/S, decodificación de entrada no confiable y la rejilla.
9. **Chequeo** (decisión 14): `set.ts`, `provenance.ts`, `link.ts`, `check.ts`, `support/fixture-root.ts`, `check.test.ts`, y `cli.ts` con `cli.test.ts`. Después, la cuarta línea de `check-web` (`check` sobre el repo real) y las de `test`. Commit `feat(tools): sprite importer and set check (TA-25)` (pasos 4 a 9, o uno por paso si queda grande).
10. **Guía** (decisiones 23 a 25): `guide-doc.ts`, `gen-guide.ts`, `docs/guides/sprites.md` con los marcadores, `pnpm --filter @torneo/sprite-importer gen:guide`, `guide-doc.test.ts`, `docs/process.md`. Commit `docs(guides): sprite art guide with generated tables (TA-25)`.
11. **CI** (decisión 27): filtro `web` y el caso de `check-workflows.test.mjs`; `node scripts/check-workflows.mjs` y `pnpm exec vitest run --config scripts/vitest.policy.config.mjs`. **Paralelizable** con 10.
12. **Docs** (decisión 29). **Paralelizable** con 10 y 11.
13. **Pruebas manuales** (registradas en el PR, solo texto):
    - `ref_a`: `pnpm -s --filter @torneo/sprite-importer import -- --in tools/sprite-importer/fixtures/export-ref --out content/_local/sprites/ref_a`, `just dev`, reproducir `ref_a` contra `dummy_b` en `/es/dev/arena` y `/en/dev/arena`, a 375 px y en desktop, tema claro y oscuro. Capturas de `ref_a` (es AGPL y geométrico) en `apps/web/test-results/screenshots/`, sin commitear.
    - Entrada real (fuera del repo): un script descartable en el directorio temporal de la sesión convierte `content/_local/sprites/ogro_del_pantano/` del checkout principal a un export (escala 2× por vecino más cercano y centrado en la celda de 192 con el pivote en (96, 184)) y se importa a `content/_local/sprites/ogro_import_test/`. Se registra en el PR solo la lista de códigos que sale (se espera que falle algunas reglas de tiempo, por ejemplo `guide.windup_short` en `attack_light`), sin nombres, rutas ni imágenes. Después se borra `content/_local/sprites/ogro_import_test/`.
    - `git status --ignored` antes del PR: nada de `content/_local/` en el índice y ningún archivo nuevo bajo `content/sprites/` salvo `README.md`.
14. `just fmt`, `just check` completo, y la salida pegada en el PR (ver "Salida esperada").

## Riesgos
- **`--experimental-strip-types` no carga el cierre de `@torneo/renderer/guide`** (Node se niega a quitar tipos dentro de `node_modules`). → El enlace del workspace resuelve a `packages/renderer/src/...` por `realpath`, igual que el de `sff-converter` con `./manifest`; `guide/` solo usa sintaxis borrable (`erasableSyntaxOnly` en el `tsconfig` del renderer). `cli.test.ts` lanza la CLI real con `spawnSync` sobre una raíz de fixture (`check --root`), así que un fallo de carga rompe el test, no solo `just check`.
- **Entrada no confiable** (PNG enormes, bombas de descompresión, symlinks, carreras entre `lstat` y `open`). → IHDR antes de decodificar, con la celda o el tamaño del manifest como único tamaño aceptado; topes de bytes por archivo antes de leer; `fsx.readBounded` comprueba con `fstat` que el archivo abierto es el que se miró; nunca se sigue un symlink; número de archivos acotado. Casos de test para cada uno.
- **El import deja el destino a medias.** → Todo se valida y se codifica en memoria antes de tocar el disco; el manifest se escribe al final; un estado intermedio no pasa `check`. El destino solo puede contener archivos que el importador reconoce (decisión 19).
- **Escribir fuera de las dos rutas.** → `--out` cerrado por igualdad exacta de rutas tras resolver, componentes sin symlinks, `--root` solo en `check`. Tests con `../`, rutas absolutas ajenas, `content/sprites` como symlink y un `id` distinto del de la carpeta.
- **La guía queda vieja o con cifras sueltas.** → Prueba de frescura del bloque y prueba de "sin `ms` fuera del bloque", las dos en `just check`.
- **Un cambio de `content/` o del dorado no dispara el job `web` en CI** (el `check` pasaría solo en local). → Filtro `web` ampliado y caso de política que lo fija.
- **Cambiar `idle_distinct` rompe casos de la fase A.** → Solo cambia el orden cuando hay varios errores y la ausencia de cascada; los casos existentes tienen un solo error y siguen igual. Los casos nuevos se escriben primero, en rojo.
- **Mover `CONTACT_PX` cambia la API pública.** → El nombre y el valor siguen saliendo de `@torneo/renderer` (vía `scene/index.ts` → `export * from './types.ts'`); `tsc` del renderer y de la web lo detectan si no.
- **Rechazar `null` en Rust rompe un personaje real.** → Ninguno tiene `sprites`; `real_content.rs` y `balance.rs` lo confirman.
- **Bytes del PNG distintos entre máquinas.** → No se commitea ninguna salida del importador; la determinación se prueba con dos corridas en el mismo proceso, y la frescura de `export-ref` compara píxeles.
- **Sprites de terceros en el repo o en el PR (ADR-0004).** → El set de PixelLab solo se usa en `content/_local/`, el script de conversión vive fuera del repo, el PR solo lleva códigos, y el paso 13 corre `git status --ignored`.
- **Tiempo de `just check`.** → Sin sets reales el `check` tarda lo que tarda arrancar Node (< 1 s). El Vitest del paquete importa `export-ref` varias veces: objetivo < 10 s en total, medido y anotado en el PR; si pasa de 20 s, las raíces de fixture comparten un import hecho una vez por archivo (`beforeAll`).

## Alternativas descartadas
- **Commitear un set real (por ejemplo, el de PixelLab).** No tiene procedencia aprobada ni encargo detrás, y cómo se produce el arte es de 004. El estándar se demuestra con `ref_a` y con raíces de fixture.
- **Fixtures de raíz commiteados con PNG malformados.** Inflan el repo y se desactualizan con el formato. Se generan en el test a partir de `export-ref`.
- **Poner el chequeo en `scripts/*.mjs`.** Necesita la capa de guía en TS y `pngjs`; la spec lo pone en el paquete, con su `tsc`, ESLint y Vitest.
- **Marcador de salida como `.sff-converter-output`.** En `content/sprites/<id>/` sería una entrada inesperada (RF-B3). El destino cerrado y la lista de entradas aceptadas cumplen el mismo fin.
- **`git check-ignore` para el destino `_local`.** Con dos rutas exactas no hace falta, y `content/sprites/` es justamente lo que se commitea.
- **Escritura atómica con un directorio temporal y `rename`.** `provenance.json` vive en el mismo directorio y no lo escribe el importador; mover el directorio lo arrastraría o lo perdería.
- **Un bloque generado por tabla en la guía.** La spec fija un solo par de marcadores; tres tablas en un bloque cumplen igual.
- **Reutilizar `placeholders/draw.ts` para `ref_a`.** No exporta sus primitivas y cruzar paquetes por ruta relativa está prohibido por ESLint; dibujar rectángulos cuesta unas 30 líneas.
- **Validar el patrón de `sprites` en Rust.** Copiaría `LIMITS.idPattern` (spec, Área 3).
- **Hash con `node:crypto` para deduplicar.** ESLint lo prohíbe en las herramientas deterministas (comparte nombre con la aleatoriedad); FNV-1a más comparación byte a byte es exacto.
- **Decodificar las hojas de un set de a una.** `validateGuide` recibe todas las hojas del set juntas (contrato de la fase A). Se lee "una a la vez" de la spec como "un set a la vez": como mucho 64 MB, que se sueltan antes del set siguiente.

## Pruebas (cómo se demuestra que funciona)
- [ ] `just check` en verde, con la salida pegada en el PR.
- [ ] **Dorado (RF-A5 + 006):** `cargo test -p content --test ruleset_timing` (3 tests) con `strike_gap_min_ms: 700`; el diff del JSON tiene solo esa llave. A mano, una vez: cambiar en local `recovery_ms` en `ruleset.rs`, ver fallar el dorado con el mensaje de regenerar, revertir sin commitear.
- [ ] **RF-B5** (`cargo test -p content`): los cuatro tests de la decisión 4; `real_content.rs` y `cargo test -p sim` (`balance.rs`) siguen pasando; `canonical_spec_json` igual con y sin `sprites`.
- [ ] **Nits:** (a) orden `[idle_distinct, idle_frame_long]`; (b) celdas vacías sin `idle_distinct`; (c) `guide-imports.test.ts`: el cierre de `guide/index.ts` en `scene/` es solo `src/scene/types.ts`; (d) `index.ts`, `stage/index.ts`, `manifest/index.ts`, `timeline/index.ts` y `scene/index.ts` no llegan a `src/guide/`; `scene/index.ts` no llega a `stage/`, `player/` ni `pixi.js`.
- [ ] **`RulesetTiming`:** `guide-ruleset.test.ts` rechaza un dorado sin `strike_gap_min_ms` (`/versions/0/strike_gap_min_ms`) y la atadura `strike_gap_min_ms ≥ REACTION_WINDOW_MS` pasa con el real; `guide-table.test.ts`: `recovery_recommended_max_ms` 700 en los tres ataques con `[V1]` y el mínimo con `[V1, V2]`.
- [ ] **RF-B1** (`export.test.ts`): un caso por código, cada uno con `toEqual` sobre la lista exacta de líneas (`code`, `file`, `path`, `data`): `export.unexpected_entry` (raíz, `frames/`, nombre de PNG), `export.symlink` (archivo y carpeta), `export.unknown_field` y `export.missing_field` (raíz y animación), `export.unsupported_format`, `export.invalid_value` (`id`, `pixel_art`, `frame_ms` no array y largo 0), `export.unknown_animation`, `export.frame_missing` (carpeta y PNG), `export.frame_extra`, `export.file_too_large`, `export.png_invalid` (firma y cuerpo truncado), `export.png_size`. PNG de entrada en gris con alfa, RGB de 16 bits y RGBA dan los mismos píxeles RGBA. Un `frame_ms` con un valor inválido pasa el export y falla en `manifest.*` con el mismo `path`.
- [ ] **RF-B2** (`import.test.ts`, en proceso, con raíces temporales): `export-ref` pasa `validateManifest` y `validateGuide` y escribe `manifest.json` y sus hojas; dos imports dan los mismos bytes; un export con frames repetidos se deduplica (menos frames en el manifest, mismos índices por animación); 101 frames únicos dan dos hojas (la segunda de 192×192); `LIMITS.sheetsMax × 100 + 1` frames únicos dan `export.too_many_frames` (con un export sintético de celdas de un píxel distinto, sin escribir nada); un export que falla una regla de guía (windup corto) no escribe nada; `--out` fuera de las dos rutas, con `..`, con `id` distinto y con `content/sprites` como symlink dan `cli.*` con 2 y no crean nada; un destino con `.sff-converter-output` da `cli.out_unexpected_entry`; un destino con `provenance.json` lo conserva intacto (mismo contenido y mtime) y reemplaza las hojas sin dejar sobrantes; el resumen de `export-ref` tiene `colors ≤ 32` y `partial_alpha_pixels: 0`; ninguna línea contiene una ruta absoluta (se busca el `tmpdir` en la salida).
- [ ] **RF-B3, RF-B4, RF-B6** (`check.test.ts`, raíces de fixture): la raíz válida pasa con la línea `{ sets: 1, characters: 1, links: 1 }`; raíz sin `content/sprites/` y raíz con solo el README pasan con `sets: 0`. Un caso por código: `set.unexpected_entry` (`.DS_Store`, subcarpeta, symlink, hoja no declarada, archivo suelto en la raíz que no es el README), `set.missing_file` (manifest, procedencia, hoja), `set.file_too_large` (manifest y hoja, con archivos dispersos para no ocupar disco), `set.invalid_json`, `set.sheet_header` (tamaño distinto, `colorType 2`, 16 bits), `manifest.id_mismatch` (carpeta renombrada); `provenance.invalid_json`, `provenance.unknown_field`, `provenance.missing_field`, `provenance.set_mismatch`, y `provenance.invalid_value` en `/format`, `/author` (con `@`, con punto, con espacio, de 1 y de 33 caracteres), `/origin`, `/date` (`2026-02-30`, `2026-13-01`, `26-01-01`), `/ai_assisted` (`"no"`) y `/license`; `link.invalid` (`null`, número, `"Bad-Id"`), `link.unknown_set`, `link.misplaced_sprites`, `link.character_unreadable` (JSON roto, symlink). Uno por familia de `guide.*` sobre el set (tiempo: `windup_short`; píxeles: `frame_clipped`; orden: `windup_order`), reescribiendo `manifest.json` o una hoja del set válido. Y **"versión nueva del ruleset con windup menor rompe un set ya válido"**: el dorado de la raíz con v1 y una v2 con `windup_heavy_ms: 250` da `guide.windup_long` en `/animations/attack_heavy/impact_frame` con `{ windup_ms: 300, cap_ms: 250, ruleset_version: 2 }`. Un set con IHDR que no coincide nunca llega a `validateGuide` (sin `RangeError`). Un set compartido por dos personajes y un set sin personaje pasan.
- [ ] **CLI** (`cli.test.ts`, `spawnSync` de la CLI real): `check --root <válida>` sale con 0; `check --root <con un error>` con 1 y líneas JSON parseables; argumentos desconocidos y comando desconocido con 2; `import` sin `--in` con 2; `import --root …` con 2 (`--root` no existe en `import`); `import` con `--out` fuera del repo con 2 sin crear el directorio.
- [ ] **RF-B7** (`guide-doc.test.ts`): el bloque commiteado es igual a `renderGuideBlock` del dorado real; con un dorado con `windup_heavy_ms: 280` la fila de `attack_heavy` cambia y muestra la versión (la tabla sigue a los datos); ninguna cifra de `ms` fuera del bloque; los marcadores aparecen una sola vez; el frontmatter tiene `type: guide`.
- [ ] **RF-B8** (`fixture.test.ts` y `replay.test.ts`): frescura de `export-ref` (lista de archivos, `export.json` y píxeles); el import pasa `validateManifest` y `validateGuide`; `windup_order` 120/300/320 se cumple; `prepareScene` con el guion del spike (`@torneo/renderer/fixtures/spike.timeline.json`) y con las timelines doradas (`packages/renderer/test/fixtures/golden/*.timeline.json`, leídas con `readFileSync`), con el lado `a` reapuntado a `ref_a` y el `b` a `dummy_b`, no lanza, y `sceneAt` cada 16 ms durante toda la pelea devuelve frames dentro de `manifest.frames` para los dos lados.
- [ ] **Política** (`vitest.policy.config.mjs`): el filtro `web` cubre las cuatro rutas de la decisión 27; `node scripts/check-workflows.mjs` pasa.
- [ ] **Manual** (paso 13): reproducción de `ref_a` en la arena (es y en, 375 px y desktop, claro y oscuro), import de la entrada real fuera del repo con sus códigos, `git status --ignored` limpio.

### Salida esperada de `just check`
Pasan todos los pasos de `check-web` y `check-rust`. Lo nuevo respecto de main:
```
pnpm --filter @torneo/sprite-importer exec tsc -p tsconfig.json --noEmit
pnpm --filter @torneo/sprite-importer exec eslint .
pnpm --filter @torneo/sprite-importer exec vitest run
 ✓ test/eslint.test.ts
 ✓ test/ihdr.test.ts
 ✓ test/fsx.test.ts
 ✓ test/export.test.ts
 ✓ test/pack.test.ts
 ✓ test/import.test.ts
 ✓ test/fixture.test.ts
 ✓ test/replay.test.ts
 ✓ test/check.test.ts
 ✓ test/cli.test.ts
 ✓ test/guide-doc.test.ts
pnpm -s --filter @torneo/sprite-importer check
{"sets":0,"characters":4,"links":0}
…
     Running tests/ruleset_timing.rs
test result: ok. 3 passed; 0 failed
     Running tests/load.rs
test sprites_is_optional_and_never_reaches_the_spec ... ok
…
```
Sin advertencias de `svelte-check` ni de clippy, y `_check-gen` sin diferencias (no cambia `protocol`). El PR lleva la salida real completa, no esta.

## Estimación de CI
Sin jobs nuevos. El filtro `web` crece en cuatro rutas, así que un PR que solo toca `content/characters/`, `content/sprites/` o el dorado ahora dispara también `web` y `e2e` (unos 10 a 15 min de runner, en paralelo con `rust`). El costo agregado dentro de `web` es el Vitest y el `check` del paquete nuevo: menos de 30 s.

## Desviaciones durante la implementación
Registradas por frontend-engineer al implementar (2026-10-28).

1. **`pnpm … run import`, no `pnpm … import`.** `pnpm import` es un comando propio de pnpm (convierte lockfiles), así que `pnpm -s --filter @torneo/sprite-importer import -- …` (RF-B2 y paso 13 de este plan) nunca llega al script y falla con `Unknown option`. El script sigue llamándose `import`; la guía y el README documentan `pnpm -s --filter @torneo/sprite-importer run import -- --in … --out …`, y `guide-doc.test.ts` falla si vuelve la forma sin `run`. No edité la spec ni el texto aprobado de este plan: queda para quien los mantenga.
2. **`CLAUDE.md` y `.claude/skills/write-character/SKILL.md` sin tocar.** La decisión 29 pide editarlos, pero un cambio a `CLAUDE.md` o a la configuración de los agentes no lo puede autorizar un agente, así que quedan pendientes para el owner. Texto propuesto: en `CLAUDE.md`, "Comandos", sumar `tools/sprite-importer` (Vitest, `tsc`, ESLint y el `check` de sets) a las listas de `just check` y `just test`, y una línea con `run import`, `check` y `gen:guide` que remita a `docs/guides/sprites.md`; en el skill, quitar `sprites/` del árbol, decir que el set vive en `content/sprites/<id>/` y sumar la fila `sprites` (opcional, último campo, patrón `LIMITS.idPattern`, fuera de la forma motor, `null` falla en `crates/content`, patrón y existencia en `just check`).
3. **Códigos que el plan no nombraba** (lista cerrada en `src/codes.ts`, toda documentada en la sección 8 de la guía): `export.missing_file` (falta `export.json` o `frames/`), `export.invalid_json`, `set.sheet_invalid` (hoja con IHDR ilegible o que el decodificador rechaza; `set.sheet_header` queda para una cabecera legible que no coincide), `provenance.file_too_large`, `report.too_many_errors` (corte de la salida en `LIMITS.maxErrors` líneas), `cli.missing_argument`, `cli.in_not_a_directory`, `cli.out_not_a_directory` e `internal.unexpected`. `link.unknown_set` lleva `data { set }` (el id ya cumple el patrón, no filtra nada). No hay `io.read_failed`: un error de E/S inesperado al leer sale como `internal.unexpected`.
4. **Errores de píxeles en `import`.** Un error de la guía sobre un frame del manifest (`/frames/<i>`) sale con `file` igual al primer PNG del export que produjo ese frame y con el `path` del manifest sin cambiar (en el export no existe `/frames/<i>`). Los de `/animations/<a>/frames/<k>` salen con su PNG, y el resto con `export.json`, como decía la decisión 13.
5. **`@torneo/renderer/guide` reexporta `CONTACT_PX` y `TIMELINE_LIMITS`.** El generador de la guía (decisión 24) los necesita y la herramienta solo puede importar `./manifest` y `./guide` en `src/`. Los dos ya estaban en el cierre de `guide/` (`scene/types.ts` y `timeline/types.ts`), así que `guide-imports.test.ts` sigue igual.
6. **Orden de los commits del renderer.** El dorado nuevo (paso 1) hace fallar `parseRulesetTiming` hasta que el renderer suma el campo, así que `feat(renderer): strike_gap_min_ms and the scene subpath` va antes que `fix(renderer): guide nits…`. Entre el commit del dorado y ese, los tests de la guía del renderer están en rojo.
7. **Detalles menores.** `TOOL_LIMITS` suma `rows` (igual a `columns`) y `limits.ts` exporta `PIVOT`. La lectura del export corta en el primer paso de la decisión 12 que falla (dentro de cada paso junta todos los errores). En `check`, si el manifest de un set es inválido, los `sheet-N.png` del set no se reportan como inesperados (no se sabe cuáles declara). Hay un `test/golden.test.ts` más y helpers en `test/support/` (`tmp.ts`, `png.ts`, `root.ts`); el PNG de 16 bits de los tests se arma a mano porque `pngjs` no escribe bien 16 bits desde una entrada de 8.

## Revisión
### architect — 2026-10-28 — APPROVE
- Verifiqué: cobertura de RF-B1 a RF-B9 y de cada fila B de la "Estrategia de pruebas" de la spec (códigos `export.*`, import determinista con deduplicación y `export.too_many_frames`, raíces de fixture con un caso por `set.*`/`provenance.*`/`link.*`, una familia por `guide.*` y la versión nueva del ruleset que rompe un set válido, frescura de la guía y de `ref_a`, reproducción con `prepareScene`/`sceneAt`, `sprites` con el mismo `spec()`). 006 está en main (`Ruleset::strike_gap_min()` en `crates/engine/src/ruleset.rs`), y el plan usa `strike_gap_min_ms` en el dorado y la fila recomendada sin fallback, como recomendación no validada (punto 2 de game-designer). Los cuatro nits del PR #48 están con caso en rojo primero: orden de `idle_distinct` (a), sin cascada sobre celdas vacías (b), `CONTACT_PX` a `scene/types.ts` con el cierre fijado en `guide-imports.test.ts` (c) y entradas `manifest`, `timeline` y `scene` (d). `just check`: cuatro líneas en `check-web`, `vitest` y `check` en `test`. CI: el filtro `web` suma `content/sprites/**`, `content/characters/**`, el dorado y `docs/guides/**`, con su caso de política; sin jobs nuevos, Dependabot (`directory: /`) y Docker (`tools/` fuera del contexto) no cambian. Nada de `content/_local/` se commitea: cero sets reales, el set de PixelLab solo como prueba manual con script en el temporal de la sesión y `git status --ignored` antes del PR. La guía dice explícitamente que no elige ni recomienda herramientas (frontera con 004). Límites: no toca motor, `engine-wasm`, `protocol` ni el formato v1; la herramienta es determinista (FNV-1a sin `node:crypto`, fechas sin `Date`) y respeta ADR-0001, 0002, 0005 y 0007 (solo códigos; los encabezados en español del generador de docs no son UI ni dominio). Entrada no confiable: IHDR antes de decodificar, topes de bytes, `lstat`/`fstat`, sin symlinks, `--out` cerrado y `--root` solo en `check`. Frontmatter válido.
- Decisiones: resueltas en "Para el architect" (15 a 20 aceptadas, con precisiones sobre `file` en errores de uso, test de cobertura de la tabla de códigos y lectura del Área 10).
