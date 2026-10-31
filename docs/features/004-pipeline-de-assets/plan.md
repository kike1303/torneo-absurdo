---
id: "004"
title: Pipeline de assets originales
type: plan
status: done
risk: medium
author: frontend-engineer
reviewers: [architect]
approvals:
  - {by: architect, date: 2026-10-30}
linear: TA-4
depends_on: ["002", "007"]
created: 2026-10-30
updated: 2026-10-31
---

# Plan: Pipeline de assets originales (desde spec.md 2026-10-30)

> [intent](./intent.md) · [spec](./spec.md) · [ADR-0013](../../decisions/0013-pipeline-de-sprites.md) (`approved`) · [receta](../../guides/pipeline-de-sprites.md)

El código precedió a este plan (Área 6 de la spec). Son dos PR. **PR A está fusionado** (PR #62, squash `10d03369`). **PR B no publica arte, por decisión del owner:** lleva la receta y los docs, y el set se queda en local (ver "Desviaciones").

**Prerrequisitos cumplidos:** spec y plan `approved`, y ADR-0013 aceptado por el owner.

## Estado medido (2026-10-31)
- Vitest de `tools/sprite-importer`: 14 archivos y 243 tests en verde. `check`: `{"sets":0,"characters":4,"links":0}`. Los dos corrieron sin red remota y sin variables de credenciales. Con la máquina muy cargada (load average de 140), dos corridas más fallaron solo por el límite de 5 s de `export.too_many_frames` en `import.test.ts`, y con `--testTimeout=60000` pasan los 243. `just check` pasó sobre PR B el 2026-10-31.
- Regeneración, todo con `v3`: 8 llamadas, 7 cobradas del saldo, $0, unos 15 min en serie. Diferencia de histograma contra `idle`: 4.8–9.4 %, frente a 19–25 % con plantillas mezcladas con `v3`. Alcance 68/77/48 px y anticipación 130/260/310 ms (`attack_light`, `attack_heavy`, `special`).
- `normalize` da `{"frames":51,"colors_in":277,"palette":32}` en 3.8 s. `run import` da `{"frames":46,"sheets":1,"colors":32,"partial_alpha_pixels":0}` en 3.9 s, sin códigos, e idéntico byte a byte al repetirlo. Ningún frame que elige la receta toca el borde de su lienzo crudo de 88×88.
- Costo acumulado del personaje de prueba: 19 generaciones, con las animaciones de plantilla abandonadas y las repeticiones. Una corrida limpia costaría 11 (ESTIMADO con el costo medido por llamada: 2 del personaje y 9 animaciones a 1, más repeticiones); ningún personaje se hizo así desde cero.
- `private/` y `content/_local/` están ignorados por git. En el checkout principal están `private/provenance/ogro_del_pantano.md` y el resguardo de `raw` y `export`, idénticos a los de este worktree. La copia de los términos es `private/provenance/pixellab-terminos-2026-10-31.html` ("Last Updated: 2026-10-30", tomada el 2026-10-31) y no cubre todo el set (ver "Riesgos").

## Hallazgos corregidos
Se comprobaron con `validateRecipe` y `runNormalize` sobre carpetas temporales, y cada corrección de código tiene su test.

| # | Requisito | Qué pasaba | Corrección | Estado |
|---|---|---|---|---|
| H1 | RF3 "solo números y modos" | `validateRecipe` solo revisaba las llaves: aceptaba texto libre en `id`, `source`, `loop`, `frame_ms`, `impact_frame`, `ground` y `offset`, así que un prompt cabía. | Cada campo tiene su tipo (paso 2). | Corregido |
| H2 | RF2 "solo escribe dentro de `content/_local/`" | Una animación llamada `../../../../x` con `source` válido escribía sus PNG fuera de `--out`. `source` con `../` leía fuera de `--in`. | Nombres de animación y `source` con `LIMITS.idPattern`, y `source` tiene que ser cadena: una lista pasaba el patrón al convertirse en texto. | Corregido |
| H3 | RF8 y Área 4 (los crudos no tienen copia) | Con `--out` igual a `--in`, `rmSync` borraba los crudos y salía con 0. Tampoco se comprobaban symlinks en la ruta de `--out`, a diferencia de `checkDestination` (`write.ts`). | `checkOut` solo acepta `content/_local/export/<recipe.id>`, sin symlinks ni solape con `--in`, también a través de un symlink. | Corregido |
| H4 | Código de error | `animations: { idle: null }` lanzaba `TypeError` y salía con 2 (`internal`), no con `normalize.recipe_invalid`. | Una animación que no es objeto da `recipe_invalid`. | Corregido |
| H5 | RF6 y ADR-0013 D6 | La sección 6 de la receta pedía solo el visto bueno y el checklist. Faltaban ADR aceptado y licencia aceptada por el owner. | Los gates de RF6, la revisión de `private/provenance/<id>.md` y los chequeos de cierre están en la receta. Los campos de `provenance.json` siguen en la guía de arte, no en la receta. | Corregido |
| H6 | RF2 y Área 1 de la spec | La spec decía que `normalize` vivía en `cli.ts` y que este lo importaba de forma estática. | La spec aprobada cita `normalize-cli.ts`, y `cli.ts` no lo carga. El código no cambió por esto. | Corregido por architect |
| H7 | RF2 y RF8 (revisión de architect) | En un disco que no distingue mayúsculas, `--in <root>/CONTENT/_local/export/<id>` no se veía como solape con `--out`, que se habría borrado. | `checkOut` compara rutas con `realpathSync.native`, y lo que aún no existe cuelga de su carpeta existente más cercana. | Corregido |

## PR A: documentación y `normalize`, sin arte (fusionado: PR #62, squash `10d03369`)

### Archivos que cambian
| Archivo | Cambio | Estado |
|---|---|---|
| `docs/features/004-pipeline-de-assets/{intent,spec,plan}.md` | Intent y spec aprobados, y este plan. | Fusionado |
| `docs/decisions/0013-pipeline-de-sprites.md` | Nuevo, `proposed`. El owner lo acepta después, fuera de este PR. | Fusionado |
| `tools/sprite-importer/src/normalize.ts` | Nuevo: receta y frames crudos → export v1. | Listo (H1, H2 y H4) |
| `tools/sprite-importer/src/normalize-cli.ts` | Nuevo: CLI aparte de `cli.ts`. | Listo (H3 y H7) |
| `tools/sprite-importer/src/codes.ts`, `package.json` | `NORMALIZE_CODES` y script `normalize`. | Listo |
| `tools/sprite-importer/test/normalize.test.ts` | Nuevo, con los casos de H1 a H4 y H7. | Listo |
| `tools/sprite-importer/test/guide-doc.test.ts` | Exige los códigos `normalize.*` en la guía. | Listo |
| `tools/sprite-importer/recipes/ogro_del_pantano.json` | Nuevo: receta del piloto, solo números y modos. | Listo |
| `tools/sprite-importer/README.md` | Comando `normalize`. | Listo; `--out` ya es `content/_local/export/<id>` |
| `docs/guides/sprites.md` | Tabla de códigos `normalize.*`. | Listo |
| `docs/guides/pipeline-de-sprites.md` | Nueva receta de idea a set (RF4). | Listo (H5) |
| `docs/architecture.md` | Entrada "Pipeline de sprites (004)" (RF4). | Listo |

### Orden de trabajo
1. **Hecho:** architect corrigió H6 y la spec se aprobó. También aprobó este plan.
2. **Hecho.** Tests primero, que fallaban con el código anterior, y luego `normalize.ts`:
   - `id`, nombre de animación y `source`: cadena con `LIMITS.idPattern`.
   - `loop`: booleano y obligatorio. `frame_ms`: enteros positivos. `impact_frame`: entero ≥ 0. Su rango lo juzga `import`.
   - `ground`: `first` o `each`. `offset`: dos enteros.
   - Una animación que no es objeto da `recipe_invalid`.
3. **Hecho.** Tests primero y luego `normalize-cli.ts`: `checkOut(root, out, id, inDir)`, con `root` inyectado para probarla sobre una carpeta temporal.
   - Exige `content/_local/export/<recipe.id>`, sin symlinks en la ruta y sin solape con `--in` en ningún sentido, comparando rutas reales con `realpathSync.native` (H7).
   - Reusa `cli.out_forbidden`, `cli.out_id_mismatch`, `cli.out_symlink` y `cli.out_not_a_directory`, sin códigos nuevos. El CLI valida la receta antes de comprobar el destino.
4. **Hecho.** H5 corregido en la receta.
5. **Hecho.** Se guardó el hash del export antes del cambio, se volvió a correr `normalize` con la receta del piloto y `run import` hacia `content/_local/sprites/`, y la salida es idéntica byte a byte.
6. **Hecho.** Issue del número de daño, que cuesta leer en tema claro a 375 px, registrado en Linear: TA-36, del renderer y fuera de 004 (Área 5).
7. **Hecho, con un límite:** la arena se verificó en oscuro a 1024 px y en los dos temas a 375 px (ver "Pruebas"). `just check` terminó con exit 0 sobre `5b3d2e1`.
8. **Hecho.** Commits con `(TA-4)` y PR #62. La descripción lleva resultados en texto, **sin capturas ni sprites**: subir una imagen del set a un PR público ya es publicarlo. Las capturas se quedan en `content/_local/review/`.

## PR B: receta y docs, sin el set

### Antes de los gates
1. **Hecho.** `private/provenance/ogro_del_pantano.md` y el resguardo de `raw` y `export` están en el checkout principal (RF8 y ADR-0013 D5). La copia de los términos está en `private/provenance/pixellab-terminos-2026-10-31.html`, tomada después de generar: cubre la regeneración del 2026-10-31 si los términos no cambiaron ese día, pero no lo generado el 2026-10-28 y el 2026-10-29 (ver "Riesgos").
2. **Hecho.** OK del owner registrado en el archivo privado. El estimado de este paso era de 2 generaciones para dos animaciones; se cobraron 7 porque el alcance creció (ver "Desviaciones").
3. **Hecho, ampliado.** Se regeneraron con `v3` `idle`, `attack_light`, `attack_heavy`, `special`, `hit` y `ko`; `block`, `dodge` y `win` ya eran `v3`. `attack_heavy` y `special` salen de `abilities` y del lore, y se repitieron una vez para quitar un objeto que no pertenece al personaje. Los prompts están en el archivo privado.
4. **Hecho.** La receta elige cada carpeta con `source`, y `normalize` y `run import` hacia `content/_local/sprites/` no dan códigos (ver "Estado medido").
5. **Hecho.** Sin errores de consola en `/es/dev/arena` a 1024 px en oscuro y a 375 px en los dos temas, revisando capturas, no el movimiento (fila 5 de la spec). game-designer confirmó que cada ataque corresponde a su habilidad y que se distinguen. El owner dio su visto bueno visual.

### Gates
Rigen para **publicar** el set (RF6). PR B no publica, así que se abre sin tener los cinco. Estado al 2026-10-31:

- [x] ADR-0013 aceptado por el owner, con la lectura de D4 confirmada. El OK al uso de PixelLab, regeneración incluida, consta en el ADR (Área 3).
- [ ] Términos de licencia aceptados por el owner, incluido que la prueba se hizo con trial (Área 2). **No aceptados:** el owner no acepta la licencia para publicar por ahora, y aceptar el ADR no la incluye. D6 sigue cerrada. La copia fechada existe, pero no cubre todo el set (ver "Riesgos").
- [x] Ataques corregidos (Áreas 7 y 8). Se regeneraron a partir de sus habilidades, el puño cortado no se repite y game-designer confirmó que corresponden y se distinguen. Sus puntos débiles están en la fila 5 de la spec.
- [x] Visto bueno visual del owner, sobre el set regenerado.
- [ ] Checklist "¿se parece demasiado?" aprobado por game-designer o product-owner, distinto del autor, con el OK del owner. Quien lo aprueba revisa `private/provenance/ogro_del_pantano.md` y confirma que ningún prompt nombra un personaje o franquicia y que no hubo material protegido como referencia, img2img o calco. Si los prompts no se pueden reconstruir, el set se regenera o se queda en local. **Sin marcar, porque este gate rige para publicar:** game-designer dio `APPROVE` como personaje de prueba local y el owner dio su OK, transmitido por el coordinador, solo con ese alcance. Ese OK no autoriza publicar.

Falta la licencia y el checklist tiene OK solo para uso local, así que el set se queda en `content/_local/`. En el intent, product-owner marcó 7 de las 8 casillas; la de commitear el set queda sin marcar.

### Archivos que cambian
| Archivo | Cambio | En PR B |
|---|---|---|
| `tools/sprite-importer/recipes/ogro_del_pantano.json` | Todas las animaciones con crudos `v3`. | Sí, ya commiteado |
| `docs/guides/pipeline-de-sprites.md` | `v3` para las 9 animaciones, lecciones de la regeneración y cómo quedarse en local. | Sí |
| `docs/features/004-pipeline-de-assets/plan.md` | Este estado. | Sí |
| `docs/features/004-pipeline-de-assets/spec.md` | Fila 5 de la Evaluación medida de nuevo, con tiempo y costo de la regeneración (Área 7). La edita su autor. | Sí |
| `docs/decisions/0013-pipeline-de-sprites.md` | Aceptación del owner y costo actualizado. Lo actualiza su autor. | Sí |
| `docs/architecture.md` | Lo que corresponda a esta decisión. Lo edita architect. | Sí |
| `docs/features/004-pipeline-de-assets/intent.md` | 7 de 8 casillas marcadas por product-owner; la de commitear el set, sin marcar. | Sí |
| `private/provenance/ogro_del_pantano.md` | **Fuera de git**: ya existe. | No |
| `content/sprites/ogro_del_pantano/{manifest.json,sheet-0.png}` | Salida de `run import`. | No, por decisión del owner |
| `content/sprites/ogro_del_pantano/provenance.json` | A mano; ver el paso 7. | No, por decisión del owner |
| `content/characters/ogro-del-pantano/character.json` | `"sprites": "ogro_del_pantano"` (RF7). | No: `check` fallaría sin el set |

### Solo si el owner decide publicar más adelante
Estos pasos **no se ejecutan en PR B**. Exigen antes un nuevo OK expreso del owner sobre la licencia (D6), su OK al checklist para publicar y resolver los términos de lo generado el 2026-10-28 y el 2026-10-29: regenerarlo o tener confirmación escrita de PixelLab.

6. `run import --in content/_local/export/ogro_del_pantano --out content/sprites/ogro_del_pantano`.
7. Escribir `provenance.json` con `format: "torneo.provenance"`, `format_version: 1`, `set: "ogro_del_pantano"`, `author: "owner"`, `origin: "original"`, `date` (la de generación), `ai_assisted: true` y `license: "content"`.
8. Agregar `"sprites"` a `character.json`.
9. Correr `check` (espera `{"sets":1,"characters":4,"links":1}`), `just sim --check` y `just check`.
10. PR: declara que existe el archivo privado, sin detalles, e incluye el checklist.

## Riesgos
- **Cierre de TA-4:** decisión del owner, transmitida por el coordinador: TA-4 se cierra en Done con el set en local. `just check` y `just sim --check` pasaron el 2026-10-31. Al fusionar PR B, Linear puede cerrar el issue solo → comprobar que quede en Done. La parte de RF5 sin cubrir está en "Pruebas".
- **Se pierden crudos o prompts:** mitigado, porque el archivo privado y el resguardo ya están en el checkout principal, fuera de este worktree. Sigue siendo una sola copia, en la máquina del owner.
- **Publicar con términos que no cubren todo el set:** D5 pide los términos vigentes al generar. La copia lleva "Last Updated: 2026-10-30", posterior a lo generado el 2026-10-28 y el 2026-10-29 (personaje base, `block`, `dodge` y `win`), y esa versión anterior no la tenemos. Solo vale para la regeneración, si los términos no cambiaron ese día → antes de publicar, regenerar eso o tener confirmación escrita de PixelLab, y comparar los términos de ese momento con esta copia.
- **Regenerar no corrige los ataques o aumenta el salto de cuerpo:** no ocurrió en esta regeneración. `import` no detecta ninguno de los dos, así que en la próxima lo sigue mirando una persona.
- **El endurecimiento rechaza la receta real o cambia la salida** → el paso 5 lo detecta con el hash.
- **Se cuela arte en PR A o PR B** → `content/_local/` y `private/` están ignorados; revisar la lista de archivos del PR antes de abrirlo.
- **Un error de `normalize` tumba `just check`:** sus tests corren ahí (Área 1), pero `import` y `check` no lo cargan → riesgo solo de CI del paquete, sin dependencia de servicios (RF9).

## Alternativas descartadas
- **`normalize` como subcomando de `cli.ts`:** cargaría `normalize` en cada `import` y `check`, contra la independencia que pide RF9.
- **Un paquete propio en `tools/`:** duplicaría configuración y `pngjs`, y no quita nada que corra en `just check` salvo sus tests.
- **Validar la guía en `normalize`:** tendría dos jueces del estándar. `import` es el único (RF2).
- **Limitar H3 a `--out` distinto de `--in`:** seguiría pudiendo borrar `content/_local/raw` entero o un set local ajeno. Esta fue la primera corrección; `checkOut` la absorbe.

## Pruebas (cómo se demuestra que funciona)
PR A:
- [x] `normalize.test.ts`: texto en cada campo de H1 da `recipe_invalid` (RF3).
- [x] `normalize.test.ts`: animación o `source` con `../` dan `recipe_invalid` y no se escribe nada (RF2). Una animación `null`, igual.
- [x] `checkOut`: rechaza fuera de `content/_local/export/<id>`, un `id` distinto, un symlink en la ruta y el solape con `--in` en ambos sentidos, también con otras mayúsculas donde el disco no las distingue (RF2, RF8).
- [x] `guide-doc.test.ts`: los códigos `normalize.*` están en la guía (RF2).
- [x] TA-36 registrado para el número de daño.
- [x] Salida de `normalize` idéntica a la anterior y `run import` sin códigos (RF5).
- [ ] Manual, en texto en el PR: `/es/dev/arena` a 375 px y en escritorio, en los dos temas, con los tres ataques y las 9 acciones (RF5). **Incompleta:** PR #62 registra oscuro a 1024 px y los dos temas a 375 px; las 9 acciones se revisaron en la hoja, frame por frame. El escritorio en tema claro se cubrió el 2026-10-31, ya con el set regenerado (ver PR B).
- [x] Búsqueda en la receta: sin secretos ni servicios; `check` y Vitest corren sin red ni credenciales (RF9). Repetido el 2026-10-31 con la red remota bloqueada y sin variables de credenciales.
- [x] `just check` en verde: exit 0 sobre `5b3d2e1`, según el comentario de PR #62. La primera corrida falló solo en un test de tiempo de `sanitize.test.ts`, ajeno al PR, y la repetición pasó.

PR B:
- [x] `normalize` y `run import` sin códigos, y salida idéntica byte a byte al repetir (RF5).
- [x] Ningún frame que elige la receta toca el borde de su lienzo crudo (Área 7).
- [x] `check` con 0 sets y 0 vínculos, porque el set no se publica.
- [x] Fila 5 de la spec medida de nuevo con la regeneración (Área 7), por architect.
- [x] `just sim --check` en verde (exit 0, RF17: all criteria pass, 2026-10-31). RF7 lo pide de todos modos.
- [x] `just check` en verde (exit 0, 2026-10-31; en la misma corrida los documentos aún recibían texto, sin efecto en sus chequeos).
- [x] `/es/dev/arena` sigue reproduciendo el set desde local, sin errores de consola a 1024 px en oscuro y a 375 px en los dos temas; se revisaron capturas, no el movimiento (fila 5 de la spec). La entrega al cliente queda fuera de alcance.
- [x] Manual, escritorio en tema claro (RF5), por el coordinador el 2026-10-31, con Chromium headless a 1024 px y este set: una pelea completa (718 capturas en 100 s, 381 distintas), sin errores de consola. En los frames se ven el ataque ligero, el panzazo, el eructo, `hit`, `ko` y `win`. Los frames los muestreó un script: ninguna persona los revisó como movimiento en vivo. El visto bueno del owner fue sobre imágenes fijas.
- [x] Las 9 acciones se reproducen sin errores de consola (RF5). Por el coordinador el 2026-10-31, con Chromium headless a 1024 px, tema oscuro, `/es/dev/arena?log=ko-knockdown` y este set en los dos lados: el log tiene `dodge` en t≈12.6 s y `block` en t≈39.3 s, y en los frames que muestreó un script alrededor de ambos se ven `idle` y la pelea que continúa, con un 7 de daño bajo en el `block`.
- [ ] Calidad visual de las poses de `block` y `dodge` (RF5). **Sin juzgar:** esos montajes cortan la parte baja de los luchadores. No consta que el owner acepte este límite.
- No aplica, porque el set no se publica: gates de RF6 enlazados en el PR como condición de publicar; `check` con 1 set y 1 vínculo; `git diff` de `character.json` con solo la línea de `sprites`. Ese archivo no tiene cambios frente a `origin/main`.

## Estimación de CI
Sin cambios en workflows.

## Desviaciones durante la implementación
- `LIMITS.idPattern` en lugar de `^[a-z0-9_]+$` para nombres de animación y `source`: es el patrón que ya rige los `id`, y además acota el largo.
- Un `id` distinto da `cli.out_id_mismatch`, como en `import`, no `cli.out_forbidden`.
- `checkOut` también rechaza un archivo en la ruta (`cli.out_not_a_directory`) y resuelve symlinks y mayúsculas antes de comparar.
- **PR B no publica arte, por decisión del owner (2026-10-31).** `ogro_del_pantano` es un personaje de prueba, no arte final, y la licencia no queda aceptada para publicar, así que el set se queda en local. PR B no lleva `content/sprites/ogro_del_pantano`, `provenance.json` ni `sprites` en `character.json`. Lleva la receta, ya commiteada, y los docs. Los pasos 6 a 10 quedan para cuando se decida publicar.
- **Se regeneraron seis animaciones, no dos.** El plan pedía solo `attack_heavy` y `special`. Mezclar plantillas con `v3` daba 19–25 % de diferencia de histograma contra `idle`, y todo `v3` da 4.8–9.4 %. Costo: 7 generaciones cobradas sobre 8 llamadas, no 2.
- **La receta elige crudos con `source`** (`idle_v3`, `attack_heavy_v3b`, etc.). Las animaciones viejas se conservan en `content/_local/raw/`, y no hizo falta borrarlas.

## Revisión
### architect — 2026-10-30 — REQUEST_CHANGES
- Verifiqué: Vitest del paquete (14 archivos, 236 tests), `typecheck` y `check` (`{"sets":0,"characters":4,"links":0}`), como dice "Estado medido". Corrí `runNormalize` con la receta del piloto hacia una carpeta temporal: `{"frames":49,"colors_in":222,"palette":32}` y `diff -r` idéntico al export. H1 a H4 coinciden con `normalize.ts` y `normalize-cli.ts`. `src/cli.ts` no menciona `normalize`. Los campos del paso 3 de PR B coinciden con `provenance.ts`. PR A no toca motor, `protocol` ni monedas: sin choque con ADR-0001, 0002, 0005 ni 0007.
- **`checkOut` es una desviación válida.** Es más estricto que RF2, sigue dentro de él y coincide con el flujo de Diseño. Quedó anotado en RF2 de la spec.
- Cambios pedidos:
  1. **PR B permite publicar el defecto conocido.** RF6 exige "los ataques corregidos (Áreas 7 y 8)", pero ese gate no está en la lista y ningún paso regenera. Agregar el gate y, antes del visto bueno y del checklist, los pasos: OK del owner con el costo, regenerar `attack_heavy` y `special` a partir de `abilities` y del lore, ajustar la receta, repetir `normalize` y `run import`, y mirar la arena. Sumar a "Archivos que cambian" `tools/sprite-importer/recipes/ogro_del_pantano.json`, y cambiar la fila de `spec.md` para que vuelva a medir la fila 5 con tiempo y costo de esa regeneración (Área 7), aunque no haya retoque. ADR-0013 también cita ese costo como pendiente: si cambia, lo actualiza su autor.
  2. **El orden de PR B se contradice.** El gate del checklist va antes de abrir el PR, pero RF6 pide que quien lo aprueba revise `private/provenance/ogro_del_pantano.md`, y el plan lo escribe en el paso 1, después de los gates. Mover ese archivo y el resguardo antes de los gates, y agregar al gate del checklist la revisión de prompts y referencias, con la salida de RF6 (regenerar o quedarse en local) y que lo apruebe alguien distinto del autor.
  3. **Falta el issue del número de daño.** product-owner y game-designer lo pidieron en la spec, y sigue pendiente. Agregar a PR A un paso para registrarlo en Linear, con el identificador en este plan.
- No bloquea el plan, pero hay que corregirlo en PR A: `checkOut` no ve el solape si `--in` cambia mayúsculas en un disco que no las distingue. En esta Mac, con `--in <root>/CONTENT/_local/export/ogro` devolvió `undefined`, y `--out` se habría borrado. Solo afecta a una entrada dentro de `export/<id>`. `realpathSync.native` devuelve la ruta con sus mayúsculas reales; hay que usarlo y agregar un test que corra solo donde el disco no las distinga.

**Respuesta del autor (frontend-engineer, 2026-10-30):**
- (1) PR B suma el gate de ataques corregidos y, antes del visto bueno y del checklist, los pasos 2 a 5: OK del owner con el costo, regenerar `attack_heavy` y `special` desde `abilities` y lore, ajustar la receta, repetir `normalize` y `run import`, y mirar la arena. "Archivos que cambian" suma la receta, la fila 5 de `spec.md` se mide de nuevo aunque no haya retoque, y ADR-0013 lo actualiza su autor si cambia el costo.
- (2) El archivo privado y el resguardo pasan al paso 1, antes de los gates. El gate del checklist incluye la revisión de prompts y referencias, la salida de RF6 y que lo apruebe alguien distinto del autor. Quité de ese gate evaluar `arena-375-claro.png`: el Área 5 ya lo da por evaluado, y el set regenerado se mira en el paso 5.
- (3) El issue del número de daño es TA-36 (paso 6 de PR A). Lo creó el coordinador, no yo.
- Lo no bloqueante también: `checkOut` usa `realpathSync.native` (H7). Reproduje el fallo primero: tres tests que corren solo donde el disco no distingue mayúsculas fallaban y ahora pasan. El CLI real rechaza `--in CONTENT/_local/export/ogro_del_pantano` con `cli.out_forbidden`, y export y set siguen idénticos byte a byte.

### architect — 2026-10-30 — APPROVE (re-revisión)
- Verifiqué mis cambios: (1) PR B tiene el gate de ataques corregidos, los pasos 2 a 5 antes del visto bueno y del checklist, la receta en "Archivos que cambian" y la fila 5 medida de nuevo; (2) el archivo privado y el resguardo van antes de los gates, y el gate del checklist lleva la revisión de prompts, la salida de RF6 y alguien distinto del autor; (3) TA-36 está en el paso 6. H7: `normalize-cli.ts` usa `realpathSync.native` y hay test con `CONTENT`. Vitest del paquete: 14 archivos y 239 tests; Prettier sin errores.
- No bloquea, para su autor: `private/provenance/ogro_del_pantano.md` y los crudos ya están en el checkout principal, así que "Estado medido", el paso 1 de PR B y el riesgo de crudos quedaron viejos. El paso 1 ya no escribe el archivo: falta la copia fechada de los términos y el resguardo del export, que no está en el checkout principal.

**Respuesta del autor (frontend-engineer, 2026-10-31):**
- Nota no bloqueante de architect: "Estado medido", el paso 1 de PR B y el riesgo de crudos ya reflejan que el archivo privado y los crudos están en el checkout principal, y que el export tiene resguardo. Sigue faltando la copia fechada de los términos.
- El plan refleja PR A fusionado, el estado de cada gate y que PR B no publica arte.
- El `status` sigue en `approved`: `docs/process.md` y `review-doc` no piden volver a `in-review`. Aun así, **estos cambios no están revisados**: se pide re-revisión a architect, sobre todo de las desviaciones y del estado de los gates.

### architect — 2026-10-31 — REQUEST_CHANGES (re-revisión de los cambios posteriores a la aprobación)
- Verifiqué, con `git diff` contra HEAD y sin compilar ni correr `just check`: `10d0336` es el squash de PR #62 y su comentario registra el exit 0 sobre `5b3d2e1`. PR B frente a `origin/main` solo lleva la receta, y este árbol suma los cinco docs. `content/sprites/` solo tiene `README.md`, y `character.json` no lleva `sprites` ni tiene cambios. La receta suma 51 frames y usa `source` en las seis animaciones que dice el paso 3. Los crudos elegidos miden 88×88, sin píxeles opacos en el borde. `private/` está ignorado. Las desviaciones son válidas: dejar el set en local es la rama prevista de RF6 y RF7, y no toca ADR-0001, 0002, 0005 ni 0007. Los gates marcados coinciden con ADR-0013 y la spec. No volví a correr Vitest ni `normalize`.
- Hechos nuevos, transmitidos por el coordinador; no los oí del owner: OK del owner al checklist como personaje de prueba local, y TA-4 se cierra en Done con el set en local. Copia de los términos en `private/provenance/pixellab-terminos-2026-10-31.html`: la leí, dice "Last Updated: 2026-10-30" y contiene §1.2, §1.3, §2.2, §3.3 y §5.1.
- Cambios pedidos, todos de hechos:
  1. **La copia de los términos ya existe, pero no cumple D5 para todo el set.** Corregir "Estado medido", el paso 1 de PR B, el gate de licencia y el riesgo de publicar, que dicen que falta. Se tomó el 2026-10-31, después de generar, y su "Last Updated" es posterior a lo generado el 2026-10-28 y el 2026-10-29: el personaje base y `block`, `dodge` y `win`, según el archivo privado. Vale como términos vigentes para la regeneración del 2026-10-31, siempre que no cambiaran ese día, pero no para lo anterior: D5 pide los vigentes al generar y esa versión no la tenemos. Decirlo, y dejar como condición de publicar resolverlo con el owner (regenerar lo anterior o confirmación escrita de PixelLab). También sobra "comparar con lo leído el 2026-10-30": comparar con esta copia.
  2. **Gate del checklist.** Registrar el OK del owner, transmitido por el coordinador y solo como personaje de prueba local. La casilla sigue sin marcar, porque el gate rige para publicar y ese OK no lo autoriza; "Solo si el owner decide publicar" ya pide uno nuevo. Cambiar "Como faltan dos" para que diga que la licencia falta y que el checklist tiene OK solo para uso local.
  3. **Casillas del intent.** La frase del mismo párrafo ya no vale para el checklist: su casilla se puede marcar con ese alcance escrito, y la decide su autor. La de publicar sigue sin marcarse. Con TA-4 en Done, la fila de `intent.md` deja de ser "Si corresponde" y pasa a "Sí".
  4. **Cierre de TA-4.** El riesgo dice que PR B "puede cerrarlo": escribir la decisión del owner. Decir además qué falta antes de cerrar: `just check` y `just sim --check` en verde, y la prueba manual de RF5 (escritorio en tema claro y las 9 acciones en movimiento), hecha o registrada como desviación aceptada por el owner. Hoy las tres están abiertas, y la manual también sigue abierta en PR A.
- No bloquea: `private/provenance/ogro_del_pantano.md` aún dice "Falta guardar aquí la copia fechada"; conviene apuntarlo a la copia, con la misma salvedad. El riesgo "Se cuela arte en PR A" también vale para PR B.

**Respuesta del autor (frontend-engineer, 2026-10-31):**
- (1) "Estado medido", el paso 1, el gate de licencia y el riesgo de publicar citan la copia de los términos y lo que no cubre, con la condición de regenerar o tener confirmación escrita de PixelLab antes de publicar.
- (2) El gate del checklist registra el OK del owner, transmitido por el coordinador, solo para uso local. La casilla sigue sin marcar y "Como faltan dos" se reescribió.
- (3) La fila de `intent.md` pasa a "Sí": product-owner marcó 7 de 8, sin la de commitear el set.
- (4) El riesgo de TA-4 registra la decisión del owner y lo que falta: `just check` y `just sim --check`. Tu pedido también exigía la prueba manual, hecha o aceptada como desviación. La prueba se hizo: las 9 acciones se reproducen sin errores, pero la calidad de las poses de `block` y `dodge` queda sin juzgar y **no consta que el owner acepte ese límite**. Lo dejo escrito para que lo decidas.
- No bloqueante: el riesgo de arte cubre también PR B. No toqué `private/provenance/ogro_del_pantano.md`, porque este encargo solo abarca la guía y el plan.
- Las casillas de `just check` y `just sim --check` siguen sin marcar. Falta tu re-revisión.

### architect — 2026-10-31 — APPROVE (re-revisión de mis cuatro cambios)
- Verifiqué, sin compilar ni correr nada: (1) "Estado medido", el paso 1, el gate de licencia y los riesgos citan la copia y lo que no cubre, y exigen regenerar o una confirmación escrita de PixelLab antes de publicar. ADR-0013 (Consecuencias) y el Área 4 de la spec dicen lo mismo. Esos dos documentos son míos: compruebo que coinciden, no los apruebo. (2) El gate del checklist registra el OK solo para uso local y sigue sin marcar. (3) En `intent.md`, la casilla del checklist está marcada y la de commitear el set no. (4) El riesgo de TA-4 registra la decisión del owner y lo que falta antes de cerrar.
- **Decisión sobre `block` y `dodge`: el hueco no bloquea cerrar TA-4 con el set en local.** RF5 pide que las 9 acciones se reproduzcan, y la calidad de sus poses no está entre los criterios del intent. El owner vio esas poses en la hoja de contactos de las nueve animaciones, dio su visto bueno y aceptó cerrar como personaje de prueba; lo sé por el coordinador, no por él.
- **Límite que queda abierto:** la casilla de las 9 acciones solo prueba que no hubo errores de consola. Los frames muestreados alrededor de `block` y `dodge` muestran `idle` y la pelea que continúa, así que nadie vio esas dos poses en la arena, ni quieta ni en movimiento. El owner tampoco aceptó este límite en concreto. La casilla de calidad sigue sin marcar y hay que mirarlas en la arena antes de publicar el set o de usarlas como referencia para otro personaje.
- Este `APPROVE` no da por cumplidos `just check` ni `just sim --check`: siguen abiertos y TA-4 no se cierra sin ellos en verde. No bloquea: "No consta que el owner acepte este límite" sigue siendo cierto para la arena, pero conviene sumar lo de la hoja de contactos.
