---
id: "sprites"
title: Guía de arte para sets de sprites
type: guide
status: in-review
risk: medium
author: frontend-engineer
reviewers: [game-designer, architect]
approvals: []
linear: TA-25
depends_on: ["002", "003", "006", "007"]
created: 2026-10-28
updated: 2026-10-28
---

# Guía de arte para sets de sprites

> Spec: [007](../features/007-estandar-de-assets/spec.md) · Herramienta: [`tools/sprite-importer`](../../tools/sprite-importer/README.md) · Formato del manifest: [002](../features/002-spike-renderer/spec.md)

## 1. Para quién es y qué no es
Para quien dibuja un personaje propio de Torneo Absurdo y para quien revisa el PR que lo agrega. Dice qué tiene que entregar (un **export**: un PNG por frame y un `export.json`), qué reglas valida la herramienta y por qué existe cada una.

No elige ni recomienda herramientas para producir el arte: eso es de la feature 004. Cualquier programa sirve si escribe PNG y JSON.

Los placeholders `dummy_a` y `dummy_b` de `packages/renderer/placeholders/` **no son modelo**: usan otra celda, están bajo AGPL y quedan fuera de esta guía. El ejemplo de referencia es `ref_a` (`tools/sprite-importer/fixtures/export-ref/`), una figura geométrica que cumple todas las reglas.

Las cifras viven en el código y aparecen solo en las tablas generadas de la sección 5. La prosa nombra su fuente (`GUIDE_LIMITS`, `TIMELINE_LIMITS.strikeGapMs`, `REACTION_WINDOW_MS`, el ruleset del motor), así que la guía nunca se contradice con lo que se valida.

## 2. Celda, pivote, facing y alturas
- **Una celda única** (`GUIDE_LIMITS.cellPx`) para todos los personajes, sin `scale`. Un ogro se ve más grande que una rata porque se dibuja más alto, no porque se escale.
- **El pivote lo fija el estándar** y no se declara: está centrado en horizontal y a `groundMarginPx` del borde inferior. Los pies van en la fila justo encima del pivote; el margen de abajo queda para una sombra o polvo.
- **Facing:** todo frame mira a la derecha. El renderer refleja al luchador del lado `b`. Un frame de ataque dibujado mirando a la izquierda falla la regla de alcance (`guide.impact_short`).
- **Alturas:** el alto opaco del primer frame de `idle` va entre `heightMinPx` y `heightMaxPx`. El mínimo es lo que todavía se lee en un celular de 375 px de ancho, donde la escena se ve a poco más de la mitad de su tamaño lógico.
- **Bordes:** ningún píxel visible toca el borde exterior de la celda (`guide.frame_clipped`). Así nada queda cortado y las celdas vecinas de la hoja no se mezclan al filtrar.
- **Alcance:** en el frame de impacto de los tres ataques, la columna visible más a la derecha llega al menos a la mitad de `CONTACT_PX` delante del pivote: el golpe alcanza al rival.

## 3. Acciones obligatorias y opcionales
Las obligatorias son las del manifest (`REQUIRED_ANIMATIONS`): `idle`, `attack_light`, `attack_heavy`, `special`, `hit`, `block`, `dodge`, `ko` y `win`. Las opcionales (`OPTIONAL_ANIMATIONS`) tienen un fallback fijado por el formato (`ANIMATION_FALLBACK`): `intro` y `walk` usan `idle`, y `knockdown` usa `hit`. Si una opcional falta, el renderer reproduce su fallback; si está, cumple sus propias reglas.

## 4. Tiempos y el porqué de cada regla
En lo que sigue, W es el windup (la suma de `frame_ms` antes de `impact_frame`) y R la recuperación (desde el inicio del frame de impacto hasta el final).

- **Duración de cada frame.** Todo frame dura entre `GUIDE_LIMITS.frameMsMin` (dos refrescos a 60 Hz) y `frameMsMax` (nada se congela más de eso).
- **Tope de windup.** Tiene dos restricciones y vale la menor:
  - **C1, motor y vivo:** W nunca supera el `windup_strike_ms`, `windup_heavy_ms` o `windup_special_ms` de ninguna versión del ruleset. El windup visual nunca empieza antes que el evento `attack` del motor, así que en vivo no hace falta conocer un movimiento antes de que el motor lo anuncie.
  - **C2, coreografía:** W nunca supera `TIMELINE_LIMITS.strikeGapMs − REACTION_WINDOW_MS`. Una `Timeline` válida solo garantiza `strikeGapMs` entre dos impactos cualesquiera; con este tope, el windup propio empieza al menos `REACTION_WINDOW_MS` después del impacto recibido y la reacción siempre se ve.

  Si una versión nueva del ruleset baja un `windup_*` por debajo de C2, la tabla lo muestra con esa versión y `guide.windup_long` la nombra en sus datos.
- **Mínimos de windup y orden.** Cada ataque tiene su mínimo, para que el pesado y el especial "se vean venir" dentro del tope. Además, dentro del mismo set, `W(attack_light) + GUIDE_LIMITS.windupStepMs ≤ W(attack_heavy) ≤ W(special)` (`guide.windup_order`): el pesado se anticipa claramente más que el ligero y el especial nunca menos que el pesado.
- **Frame de impacto.** `impact_frame` tiene un mínimo por ataque (más frames de anticipación en el pesado y el especial) y el frame de impacto dura al menos lo que dice la tabla, para que el golpe se lea.
- **Recuperación.** R es al menos `REACTION_WINDOW_MS`: el atacante no vuelve a `idle` mientras el rival todavía reacciona. La recuperación **recomendada** (no validada) es hasta `strike_gap_min()` del motor (feature 006, el mínimo entre dos impactos): más allá casi nunca se ve en un intercambio rápido, porque la corta el impacto siguiente.
- **Reacciones (`hit`, `block`, `dodge`).** Las corta el windup propio siguiente o el impacto siguiente. La pose que se lee (el segundo frame) empieza a más tardar en `REACTION_WINDOW_MS − GUIDE_LIMITS.keyPoseMinMs`, así se ve al menos `keyPoseMinMs` antes del corte más temprano posible. Lo que dure más allá de unos cientos de milisegundos solo se ve si nadie actúa enseguida.
- **`knockdown` y `ko`.** `knockdown` no dura más que el `knockdown_ms` más bajo del ruleset; `ko` no dura más que `TIMELINE_LIMITS.winGapMs` y sostiene su último frame (lo exige el formato).
- **`intro`.** Termina antes de que pueda empezar el primer windup: `TIMELINE_LIMITS.introGapMs − LIMITS.windupMaxMs`.
- **`idle` en los respiros.** Los respiros de 006 dejan varios segundos solo con `idle`. Por eso tiene un mínimo de frames y de frames **distintos**, ningún frame de más de `frame_max_ms` y un ciclo dentro de su rango. Para que el loop no se note, conviene usar `frame_ms` irregulares (un parpadeo, un respiro más largo). Ese frame "más largo" sigue topado por `frame_max_ms` (`guide.idle_frame_long`): la irregularidad se logra alternando frames más cortos y más largos dentro del tope, nunca con un frame que lo supere.
- **Pies en `idle`.** La fila visible más baja de cada frame de `idle` queda en la franja de `feetTolerancePx` sobre el pivote, para que el personaje no "flote" ni "se hunda" al respirar.

## 5. Tablas (generadas)
Las escribe `pnpm --filter @torneo/sprite-importer gen:guide` a partir de `GUIDE_LIMITS`, `guideTable(rulesets)` y el dorado del ruleset (`crates/content/tests/golden/ruleset-timing.json`). Un test falla si el bloque no coincide con lo que genera el script. No las edites a mano.

<!-- guide:generated:start -->

### Celda y encuadre

| Parámetro | Valor | Fuente |
| --- | --- | --- |
| Celda | 192 × 192 px | `GUIDE_LIMITS.cellPx` |
| Pivote (pies) | (96, 184) | `cellPx / 2`, `cellPx − groundMarginPx` |
| Margen bajo los pies | 8 px | `GUIDE_LIMITS.groundMarginPx` |
| Alto opaco de `idle` (frame 0) | 64–176 px | `GUIDE_LIMITS.heightMinPx–heightMaxPx` |
| Fila opaca más baja de cada frame de `idle` | 181–183 | `pivotY − 1 − feetTolerancePx … pivotY − 1` |
| Alcance en el frame de impacto | ≥ 32 px delante del pivote (columna ≥ 128) | `CONTACT_PX / 2` |
| Duración de cada frame | 33–1000 ms | `GUIDE_LIMITS.frameMsMin–frameMsMax` |
| Pose clave mínima | 66 ms | `GUIDE_LIMITS.keyPoseMinMs` |
| Ventana de reacción | 200 ms | `REACTION_WINDOW_MS` |
| Separación mínima entre impactos | 600 ms | `TIMELINE_LIMITS.strikeGapMs` |
| Tope estructural de windup | 400 ms | `TIMELINE_LIMITS.strikeGapMs − REACTION_WINDOW_MS` |
| Paso mínimo de windup de `attack_light` a `attack_heavy` | 100 ms | `GUIDE_LIMITS.windupStepMs` |
| Frames por animación (formato) | ≤ 64 | `LIMITS.animationFramesMax` |
| Hojas por set | ≤ 4, de 10 × 10 celdas | `LIMITS.sheetsMax`, `LIMITS.sheetSideMax` |
| Peso de cada hoja | 4 MiB | `GUIDE_LIMITS.sheetFileMaxBytes` |
| Peso de cada archivo del export | 1 MiB | `GUIDE_LIMITS.exportFileMaxBytes` |
| Peso de `manifest.json` y `provenance.json` | 64 KiB | `TOOL_LIMITS.jsonFileMaxBytes` |

### Por acción

| Acción | Obligatoria | Frames | Duración | Windup | Impacto | Recuperación | Primer frame |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `idle` | sí | 4–12 (≥ 3 distintos) | ciclo 600–2400 ms; cada frame ≤ 250 ms | — | — | — | — |
| `attack_light` | sí | 3–8 | — | 100–250 ms | `impact_frame` ≥ 1; dura ≥ 50 ms | ≥ 200 ms (recomendada ≤ 700 ms) | — |
| `attack_heavy` | sí | 4–10 | — | 250–400 ms | `impact_frame` ≥ 2; dura ≥ 66 ms | ≥ 200 ms (recomendada ≤ 700 ms) | — |
| `special` | sí | 5–14 | — | 300–400 ms | `impact_frame` ≥ 3; dura ≥ 66 ms | ≥ 200 ms (recomendada ≤ 700 ms) | — |
| `hit` | sí | 2–6 | 200–1000 ms | — | — | — | ≤ 134 ms |
| `block` | sí | 2–6 | 200–1000 ms | — | — | — | ≤ 134 ms |
| `dodge` | sí | 2–6 | 200–1000 ms | — | — | — | ≤ 134 ms |
| `ko` | sí | 3–12 | ≤ 1500 ms | — | — | — | — |
| `win` | sí | 2–12 | — | — | — | — | — |
| `intro` | no (usa `idle`) | 3–24 | ≤ 1600 ms | — | — | — | — |
| `walk` | no (usa `idle`) | 4–12 | ciclo 300–1200 ms | — | — | — | — |
| `knockdown` | no (usa `hit`) | 3–12 | ≤ 1500 ms | — | — | — | — |

### Recomendaciones

| Recomendación (no validada) | Valor | Cómo se mide |
| --- | --- | --- |
| Colores por set (pixel art) | ≤ 32 | `colors` del resumen del importador |
| Alfa | binaria (0 o 255) | `partial_alpha_pixels` = 0 en el resumen |
| Contorno | oscuro, de 1 px lógico | — |
| Ciclo de `idle` | 800–1600 ms | suma de `frame_ms` |

<!-- guide:generated:end -->

## 6. Export, importador, set y procedencia

### Export (`torneo.sprites.export` v1)
```
<export>/
  export.json
  frames/<animación>/<nn>.png    # nn = 00, 01, … contiguos; uno por frame, en orden
```
```json
{
  "format": "torneo.sprites.export",
  "format_version": 1,
  "id": "mi_personaje",
  "pixel_art": true,
  "animations": {
    "idle": { "frame_ms": [200, 250, 200, 250], "loop": true },
    "attack_light": { "frame_ms": [60, 60, 90, 120], "loop": false, "impact_frame": 2 }
  }
}
```
- Llaves exactas. `id` sigue el patrón de los manifests (`LIMITS.idPattern`: minúsculas, dígitos y guion bajo). Cada animación tiene `frame_ms` y `loop`, y los ataques además `impact_frame`.
- En `frames/` va una carpeta por animación declarada, con exactamente tantos PNG como valores tenga su `frame_ms`.
- Cada PNG mide exactamente la celda, mira a la derecha y tiene los pies sobre el pivote. Se acepta cualquier PNG que se pueda decodificar (paleta, gris, RGB, 16 bits); la salida siempre es RGBA de 8 bits.
- Nada de symlinks ni de archivos de más.

### Importador
```sh
pnpm -s --filter @torneo/sprite-importer import -- --in <carpeta-del-export> --out content/_local/sprites/<id>
```
- Valida el export, arma el manifest (un frame por contenido distinto, en una rejilla de hojas), corre `validateManifest` y la capa de guía contra **todas** las versiones del ruleset y, solo si todo pasa, escribe `manifest.json` y `sheet-N.png`. Si algo falla, no escribe nada.
- `--out` solo puede ser `content/_local/sprites/<id>` (para probar) o `content/sprites/<id>` (para commitear), con `<id>` igual al del export. Nunca toca `provenance.json`.
- La salida es una línea JSON por problema, `{ "code", "file", "path", "data" }`. `file` es relativo a la carpeta del export y `path` usa los nombres del manifest, así que `/animations/attack_light/impact_frame` se lee igual en el export. Si pasa, imprime un resumen con `frames`, `sheets`, `colors` y `partial_alpha_pixels`.

### Set (`content/sprites/<id>/`)
Contiene exactamente `manifest.json`, las hojas que declara y `provenance.json`. El nombre de la carpeta es el `id`. Nada más: ni fuentes de la herramienta de dibujo, ni el export, ni `.DS_Store`. Un personaje lo enlaza con `"sprites": "<id>"` en su `character.json`; varios personajes pueden compartir un set.

### Procedencia (`torneo.provenance` v1)
```json
{
  "format": "torneo.provenance",
  "format_version": 1,
  "set": "mi_personaje",
  "author": "encargo-0003",
  "origin": "commission",
  "date": "2026-11-02",
  "ai_assisted": false,
  "license": "content"
}
```
- Sin texto libre. `author` es un identificador público con consentimiento (minúsculas, dígitos y guiones), nunca un correo ni un nombre completo. `origin` es `original`, `commission` o `generated`. `ai_assisted` deja constancia de si intervino generación por IA. `license` es siempre `content` (`LICENSE-CONTENT.md`).
- Los datos que identifican al autor y la prueba de cesión de derechos **nunca** van al repo: viven en `private/provenance/<id>.md`.

### Flujo
1. Exporta desde tu herramienta a una carpeta propia, fuera del repo o en `content/_local/`.
2. Importa a `content/_local/sprites/<id>` y prueba en `/[lang]/dev/arena` con `just dev`.
3. Cuando se vea bien, importa a `content/sprites/<id>`.
4. Escribe `provenance.json` a mano.
5. Abre el PR. La descripción declara que existe `private/provenance/<id>.md`, sin detalles. `just check` valida el set, su procedencia y los vínculos.

## 7. Recomendaciones (no validadas)
Hasta que exista la identidad visual, la paleta y el contorno son recomendaciones, no reglas: validarlos ahora fijaría un estilo antes de tiempo.
- **Paleta:** en pixel art, pocos colores por set y alfa binaria (cada píxel totalmente visible o totalmente transparente). El resumen del importador informa `colors` y `partial_alpha_pixels` para medirlo.
- **Contorno:** oscuro, de un píxel lógico, para que la silueta se lea a 375 px sobre los dos temas.
- **Ciclo de `idle`:** dentro del rango recomendado, con `frame_ms` irregulares.

Las cifras están en la tabla "Recomendaciones" de la sección 5.

## 8. Códigos
Los comandos solo emiten códigos (ADR-0007); esta tabla es su diccionario. `data` trae los números que explican cada caso. Salida 1: el contenido no cumple. Salida 2: error de uso (`cli.*`), de E/S (`io.*`, por ejemplo `io.golden_invalid` si falta el dorado del ruleset) o inesperado (`internal.unexpected`). Pasados `LIMITS.maxErrors` problemas, la salida se corta con `report.too_many_errors`.

### Export
| Código | Qué significa |
|---|---|
| `export.unexpected_entry` | Hay un archivo o carpeta que el export no admite (en la raíz, en `frames/` o un PNG con otro nombre). |
| `export.symlink` | Un enlace simbólico; nunca se siguen. |
| `export.missing_file` | Falta `export.json` o `frames/`. |
| `export.file_too_large` | Un archivo supera `GUIDE_LIMITS.exportFileMaxBytes`. |
| `export.invalid_json` | `export.json` no es JSON. |
| `export.unknown_field` | Una llave que el formato no tiene (en la raíz o en una animación). |
| `export.missing_field` | Falta una llave obligatoria. |
| `export.unsupported_format` | `format` o `format_version` no son los de este formato. |
| `export.invalid_value` | Un valor con el tipo o la forma equivocados (`id`, `pixel_art`, `frame_ms`, la raíz). |
| `export.unknown_animation` | Una animación que el formato no conoce. |
| `export.frame_missing` | Falta la carpeta de una animación o uno de sus PNG. |
| `export.frame_extra` | Sobra un PNG: hay más que valores en `frame_ms`. |
| `export.png_invalid` | El PNG no se puede leer o decodificar. |
| `export.png_size` | El PNG no mide la celda (`data`: `w`, `h`). |
| `export.too_many_frames` | Más frames distintos de los que caben en `LIMITS.sheetsMax` hojas (`data`: `frames`, `max`). |

### Manifest
Los errores `manifest.*` son los de `validateManifest` (formato de 002); en `import` salen con `file: "export.json"` y el mismo `path`.

### Guía
| Código | Qué significa |
|---|---|
| `guide.no_rulesets` | No hay versiones del ruleset contra las que validar. |
| `guide.scale_forbidden` | El manifest declara `scale`; el estándar no lo usa. |
| `guide.cell_size` | Un frame no mide la celda (`data`: `w`, `h`). |
| `guide.pivot_position` | El pivote no es el del estándar. |
| `guide.frame_ms_range` | Un frame dura fuera del rango de la guía (`data`: `ms`). |
| `guide.frames_count` | La acción tiene menos o más frames de los permitidos. |
| `guide.idle_distinct` | `idle` tiene muy pocos frames distintos (`data`: `count`). |
| `guide.idle_frame_long` | Un frame de `idle` dura más que su tope. |
| `guide.cycle_range` | El ciclo de `idle` o `walk` está fuera de rango. |
| `guide.duration_range` | La duración total de la acción está fuera de rango. |
| `guide.windup_short` | El windup es más corto que el mínimo de la acción. |
| `guide.windup_long` | El windup supera su tope; `ruleset_version` dice qué versión lo limita, si es el ruleset. |
| `guide.windup_order` | No se cumple el orden de windups entre ligero, pesado y especial. |
| `guide.anticipation` | `impact_frame` es menor que el mínimo de la acción. |
| `guide.impact_hold` | El frame de impacto dura menos que el mínimo. |
| `guide.recovery_short` | La recuperación es menor que `REACTION_WINDOW_MS`. |
| `guide.reaction_slow` | El primer frame de `hit`, `block` o `dodge` es demasiado largo. |
| `guide.frame_empty` | Un frame no tiene ningún píxel visible. |
| `guide.frame_clipped` | Un píxel visible toca el borde de la celda. |
| `guide.feet_off_pivot` | En `idle`, los pies no están sobre el pivote (`data`: `row`). |
| `guide.height_range` | El alto del primer frame de `idle` está fuera de rango (`data`: `height`). |
| `guide.impact_short` | El frame de impacto no alcanza al rival (`data`: `reach`). |
| `guide.too_many_errors` | Se cortó la lista de errores de la guía en `LIMITS.maxErrors`. |

En `import`, un error de píxeles sobre un frame del manifest (`/frames/<i>`) sale con `file` igual al primer PNG del export que produjo ese frame.

### Set
| Código | Qué significa |
|---|---|
| `set.unexpected_entry` | Un archivo, carpeta o enlace que el set no admite (o un archivo suelto en `content/sprites/` que no es el README). |
| `set.missing_file` | Falta `manifest.json`, `provenance.json` o una hoja declarada. |
| `set.file_too_large` | `manifest.json` o una hoja superan su tope de peso. |
| `set.invalid_json` | `manifest.json` no es JSON. |
| `set.sheet_header` | La cabecera de una hoja no coincide con `sheets[]` o no es RGBA de 8 bits (`data`: `width`, `height`, `bit_depth`, `color_type`). |
| `set.sheet_invalid` | Una hoja no se puede leer o decodificar. |

### Procedencia
| Código | Qué significa |
|---|---|
| `provenance.file_too_large` | `provenance.json` supera su tope de peso. |
| `provenance.invalid_json` | `provenance.json` no es JSON. |
| `provenance.unknown_field` | Una llave que el esquema no tiene. |
| `provenance.missing_field` | Falta una llave obligatoria. |
| `provenance.invalid_value` | Un valor fuera del esquema; `path` dice cuál (`/author`, `/date`…). |
| `provenance.set_mismatch` | `set` no es el nombre de la carpeta. |

### Vínculo
| Código | Qué significa |
|---|---|
| `link.invalid` | `sprites` en `character.json` no es un id válido (también `null`). |
| `link.unknown_set` | `sprites` apunta a un set que no existe en `content/sprites/` (`data`: `set`). |
| `link.misplaced_sprites` | Hay una carpeta `sprites/` dentro del personaje; los sets van en `content/sprites/`. |
| `link.character_unreadable` | `character.json` no se puede leer (falta, es un enlace, es muy grande o no es un objeto JSON). |
