---
id: "004"
title: Pipeline de assets originales
type: plan
status: approved
risk: medium
author: frontend-engineer
reviewers: [architect]
approvals:
  - {by: architect, date: 2026-10-30}
linear: TA-4
depends_on: ["002", "007"]
created: 2026-10-30
updated: 2026-10-30
---

# Plan: Pipeline de assets originales (desde spec.md 2026-10-30)

> [intent](./intent.md) · [spec](./spec.md) · [ADR-0013](../../decisions/0013-pipeline-de-sprites.md) (`proposed`) · [receta](../../guides/pipeline-de-sprites.md)

El código precede a este plan (Área 6 de la spec): la mayor parte de PR A ya está en el árbol de trabajo, sin commitear. Este plan lo cubre tal como está y agrega lo que falta. Son dos PR. PR A no acepta el ADR ni publica arte, así que se puede fusionar con los revisores en `APPROVE`. PR B espera los gates del owner (RF6).

**Prerrequisito cumplido:** la spec ya está `approved`. Falta la aprobación de este plan por architect.

## Estado medido (2026-10-30)
- Vitest de `tools/sprite-importer`: 14 archivos y 239 tests en verde; `typecheck`, `lint` y Prettier del paquete, sin errores. `check`: `{"sets":0,"characters":4,"links":0}`. `just check` aún no se corrió.
- Paso 5: `normalize` da `{"frames":49,"colors_in":222,"palette":32}` y `run import`, `{"frames":47,"sheets":1,"colors":32,"partial_alpha_pixels":0}`, sin códigos. El SHA-256 de `content/_local/export/ogro_del_pantano` y el de `content/_local/sprites/ogro_del_pantano` son iguales antes y después de los pasos 2 y 3.
- `private/` y `content/_local/` están ignorados por git. `private/provenance/` no existe. `content/_local/` tiene `raw`, `export`, `sprites` y `review` de `ogro_del_pantano`, solo en este worktree.

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

## PR A: documentación y `normalize`, sin arte

### Archivos que cambian
| Archivo | Cambio | Estado |
|---|---|---|
| `docs/features/004-pipeline-de-assets/{intent,spec,plan}.md` | Intent y spec aprobados, y este plan. | En el árbol; falta aprobar este plan |
| `docs/decisions/0013-pipeline-de-sprites.md` | Nuevo, `proposed`. El owner lo acepta después, fuera de este PR. | En el árbol |
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
1. **Hecho:** architect corrigió H6 y la spec se aprobó. **Pendiente:** architect revisa este plan.
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
7. **Pendiente.** Verificar en `/es/dev/arena` según "Pruebas". Correr `just check`.
8. **Pendiente.** Commits con `(TA-4)` y PR. La descripción lleva resultados en texto, **sin capturas ni sprites**: subir una imagen del set a un PR público ya es publicarlo. Las capturas se quedan en `content/_local/review/`.

## PR B: el set, tras los gates del owner

### Antes de los gates (nada de esto entra al repo todavía)
1. Escribir `private/provenance/ogro_del_pantano.md` en el checkout principal, no en un worktree desechable, con prompts, referencias y copia fechada de los términos. Copiar `raw` y `export` a un resguardo fuera del worktree (RF8 y ADR-0013 D5). Va primero porque quien aprueba el checklist lo revisa, y porque regenerar reemplaza crudos.
2. Pedir el OK del owner con el costo. ESTIMADO: 2 generaciones si salen a la primera, porque cada animación `v3` costó 1 en el piloto; repetir cuesta más.
3. Regenerar `attack_heavy` y `special` a partir de `abilities` y del lore (Áreas 7 y 8): `mud_belly_flop` es un panzazo y `swamp_burp`, un eructo. Se recomienda `v3` con `action_description`, sin nombres protegidos, y anotar los prompts nuevos en el archivo privado. Medir tiempo y costo.
4. Ajustar la receta y repetir `normalize` y `run import` hacia `content/_local/sprites/`.
5. Mirar `/es/dev/arena` a 375 px y en escritorio, en los dos temas: cada ataque corresponde a su habilidad, ninguno se corta y los tres se distinguen. Si no, volver al paso 3 con un nuevo OK si el costo supera lo aprobado.

### Gates
**PR B no se abre hasta tener todos registrados (RF6):**
- [ ] ADR-0013 aceptado por el owner, con la lectura de D4 confirmada y el uso de PixelLab que ya se hizo registrado (Área 3).
- [ ] Términos de licencia aceptados por el owner, incluido que la prueba se hizo con trial (Área 2).
- [ ] Ataques corregidos (Áreas 7 y 8), según el paso 5.
- [ ] Visto bueno visual del owner, sobre el set regenerado.
- [ ] Checklist "¿se parece demasiado?" aprobado por game-designer o product-owner, distinto del autor, con el OK del owner. Quien lo aprueba revisa `private/provenance/ogro_del_pantano.md` y confirma que ningún prompt nombra un personaje o franquicia y que no hubo material protegido como referencia, img2img o calco. Si los prompts no se pueden reconstruir, el set se regenera o se queda en local.

Si falta alguno, el set se queda en `content/_local/` y las casillas del intent no se marcan.

### Archivos que cambian
| Archivo | Cambio |
|---|---|
| `content/sprites/ogro_del_pantano/{manifest.json,sheet-0.png}` | Nuevos: salida de `run import`. |
| `content/sprites/ogro_del_pantano/provenance.json` | Nuevo, a mano; ver el paso 7. |
| `content/characters/ogro-del-pantano/character.json` | Solo se agrega `"sprites": "ogro_del_pantano"` (RF7). |
| `tools/sprite-importer/recipes/ogro_del_pantano.json` | Frames y tiempos de `attack_heavy` y `special` regenerados. |
| `docs/features/004-pipeline-de-assets/spec.md` | Fila 5 de la Evaluación medida de nuevo, con tiempo y costo de la regeneración (Área 7), aunque no haya retoque; y el retoque, si el owner lo pidió (Área 5). La edita su autor. |
| `docs/decisions/0013-pipeline-de-sprites.md` | Solo si cambia el costo que cita. Lo actualiza su autor. |
| `docs/features/004-pipeline-de-assets/intent.md` | Casillas cumplidas. |
| `private/provenance/ogro_del_pantano.md` | Nuevo, **fuera de git**: ver el paso 1. |

### Orden de trabajo, con los gates cerrados
6. `run import --in content/_local/export/ogro_del_pantano --out content/sprites/ogro_del_pantano`.
7. Escribir `provenance.json` con `format: "torneo.provenance"`, `format_version: 1`, `set: "ogro_del_pantano"`, `author: "owner"`, `origin: "original"`, `date` (la de generación), `ai_assisted: true` y `license: "content"`.
8. Agregar `"sprites"` a `character.json`.
9. Correr `check` (espera `{"sets":1,"characters":4,"links":1}`), `just sim --check` y `just check`.
10. PR: declara que existe el archivo privado, sin detalles, e incluye el checklist.

## Riesgos
- **Fusionar PR A cierra TA-4:** Linear enlaza la rama y puede pasar el issue a Done → tras el merge, comprobarlo y devolverlo a In Progress hasta cerrar PR B o hasta que el owner decida dejar el set en local.
- **Se pierden crudos o prompts antes de PR B:** solo existen en este worktree, si es que los prompts se conservan (Área 4) → hacer el resguardo antes de limpiar el worktree. Si los prompts no se pueden recuperar, RF8 no se cumple y hay que avisar al owner antes de los gates, porque regenerar cuesta generaciones y su OK.
- **Regenerar no corrige los ataques o aumenta el salto de cuerpo:** `import` no detecta ninguno de los dos → lo mira una persona en el paso 5; cada intento repite el OK si supera el costo aprobado y, si no se logra, el set se queda en local.
- **El endurecimiento rechaza la receta real o cambia la salida** → el paso 5 lo detecta con el hash.
- **Se cuela arte en PR A** → `content/_local/` y `private/` están ignorados; revisar la lista de archivos del PR antes de abrirlo.
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
- [ ] Manual, en texto en el PR: `/es/dev/arena` a 375 px y en escritorio, en los dos temas, con los tres ataques y las 9 acciones (RF5).
- [ ] Búsqueda en la receta: sin secretos ni servicios; `check` y Vitest corren sin red ni credenciales (RF9).
- [ ] `just check` en verde.

PR B:
- [ ] Gates de RF6 enlazados en el PR, incluida la revisión de prompts y referencias.
- [ ] Fila 5 de la spec medida de nuevo con la regeneración (Área 7).
- [ ] `check` con 1 set y 1 vínculo, `just sim --check` y `just check` en verde (RF6, RF7).
- [ ] `git diff` de `character.json`: solo la línea de `sprites` (RNF: balance sin cambios).
- [ ] `/es/dev/arena` sigue reproduciendo el set desde local. La entrega al cliente queda fuera de alcance.

## Estimación de CI
Sin cambios en workflows.

## Desviaciones durante la implementación
- `LIMITS.idPattern` en lugar de `^[a-z0-9_]+$` para nombres de animación y `source`: es el patrón que ya rige los `id`, y además acota el largo.
- Un `id` distinto da `cli.out_id_mismatch`, como en `import`, no `cli.out_forbidden`.
- `checkOut` también rechaza un archivo en la ruta (`cli.out_not_a_directory`) y resuelve symlinks y mayúsculas antes de comparar.

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
