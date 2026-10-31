---
id: "004"
title: Pipeline de assets originales
type: spec
status: approved
risk: medium
author: architect
reviewers: [product-owner, game-designer]
approvals:
  - {by: product-owner, date: 2026-10-30}
  - {by: game-designer, date: 2026-10-30}
linear: TA-4
depends_on: ["002", "007"]
created: 2026-10-30
updated: 2026-10-30
---

# Spec: Pipeline de assets originales

> Intent: [intent.md](./intent.md) · Decisión: [ADR-0013](../../decisions/0013-pipeline-de-sprites.md) (`proposed`) · Receta: [pipeline-de-sprites.md](../../guides/pipeline-de-sprites.md)

## Resumen
Elige el pipeline híbrido: AI, receta, `normalize` e `import`. Documenta lo ya construido y lo medido con `ogro_del_pantano`; no rediseña. El set sigue en `content/_local/` hasta que el owner cierre sus gates.
**Riesgo: se mantiene `medium` en esta spec y propongo `high` en el ADR.** La spec no toca auth, monedas, datos personales ni producción, y nadie redacta términos. La decisión legal vive en ADR-0013, que el owner ya tiene que aceptar, y publicar el set exige su OK (RF6). No sumo a security-reviewer: no hay superficie de ataque nueva. Si product-owner prefiere `high` también aquí, solo se agrega ese gate, que el owner ya ejerce.

## Requisitos funcionales
- **RF1. Evaluación.** La tabla comparativa vive en esta spec. El repo es público: lleva costos de herramientas, nada de precios ni estrategia propios.
- **RF2. `normalize`** (`src/normalize.ts`, con su propia CLI en `src/normalize-cli.ts`; `src/cli.ts` no lo carga). Convierte frames crudos más receta en un export v1: escala entera sin filtro, alfa binaria, pies sobre el pivote y paleta compartida opcional. No valida la guía: el único juez es `import`. Solo escribe dentro de `content/_local/` (`cli.out_forbidden`, si no), y `--out` no puede solaparse con `--in`. Una animación `null` da `normalize.recipe_invalid`. Sus errores son códigos `normalize.*`, documentados en la guía (ADR-0007). Desviación válida (architect, 2026-10-30): `checkOut` es más estricto que este RF, porque `--out` solo puede ser `content/_local/export/<recipe.id>`, sin symlinks; sigue dentro de RF2 y coincide con el flujo de Diseño.
- **RF3. Receta** (`torneo.sprites.recipe` v1, `tools/sprite-importer/recipes/<id>.json`). Números, modos e identificadores: `validateRecipe` rechaza llaves desconocidas con `normalize.recipe_invalid`, y `id`, `source` y los nombres de animación siguen `LIMITS.idPattern`. No caben textos libres, salvo un identificador corto (hasta 48 caracteres). El prompt va solo en `private/provenance/<id>.md` (ADR-0013 D5). `impact_frame` cuenta sobre la lista `frames` de la receta.
- **RF4. Guía.** `docs/guides/pipeline-de-sprites.md` lleva de idea a set sin depender de quien hizo la prueba. Cubre fijar silueta, paleta y vestimenta con el checklist **antes** de generar, `id` con guion bajo frente a `slug` con guion, mirar que el frame de impacto no quede cortado por el lienzo crudo (Área 7) y describir los ataques a partir de `abilities` y del lore (Área 8). El plan actualiza `docs/architecture.md` (`normalize`, receta y enlace al ADR).
- **RF5. Piloto.** `ogro_del_pantano` pasa `run import` y se reproduce en `/[lang]/dev/arena` desde `content/_local/sprites/`, con las 9 acciones obligatorias.
- **RF6. Publicación.** El set pasa a `content/sprites/<id>/` solo con ADR-0013 aceptado, licencia aceptada por el owner, su visto bueno visual, los ataques corregidos (Áreas 7 y 8) y el checklist aprobado por game-designer o product-owner, distinto del autor, con el OK del owner. Quien aprueba el checklist revisa además `private/provenance/<id>.md` y confirma que ningún prompt nombra un personaje o franquicia y que no hubo material protegido como referencia, img2img o calco; si los prompts no se pueden reconstruir, el set se regenera o se queda en local. Ese PR lleva `provenance.json` según ADR-0013 D4 y pasa `check`. Si falta algo, el set se queda en local y la casilla del intent no se marca.
- **RF7. Vínculo.** `"sprites"` se agrega a `character.json` solo si el set está en `content/sprites/` (si no, `check` falla con `link.unknown_set`). `just sim --check` se corre de todos modos.
- **RF8. Lo privado y lo local.** Antes del PR del set existe `private/provenance/ogro_del_pantano.md`, con prompts, referencias y copia fechada de los términos. Crudos en `content/_local/raw/<id>/` y export en `content/_local/export/<id>/`, con resguardo. Ninguno entra al set.
- **RF9. Independencia.** `run import`, `check`, `just check` y CI funcionan sin generador, servicio ni credencial. `normalize` no los necesita y no deja secretos.

## Requisitos no funcionales
Trabajo local, sin carga en el VPS ni imagen de Docker (`tools/` queda fuera). Motor, `protocol`, `CharacterSpec` y balance no cambian. Sin dependencias nuevas: `normalize` usa `pngjs` y `@torneo/renderer/guide`, que el paquete ya tiene.

## Diseño
**Flujo:** idea + checklist → generador (crudos en `_local/raw`) → receta + `normalize` (`_local/export`) → `run import` (`_local/sprites`) → arena → gates del owner → `run import` (`content/sprites`) + `provenance.json`.
**Componentes:** `tools/sprite-importer`, `docs/guides/`, `content/sprites/`, `content/characters/ogro-del-pantano/`. Sin cambios en `protocol`, API, WS ni UI.

## Evaluación
Medido el 2026-10-30, salvo lo marcado. Personaje de prueba: `ogro_del_pantano`. No es asesoría legal. Criterios de calidad del intent: silueta a 375 px en dos temas; consistencia; tres ataques distinguibles; `colors` y `partial_alpha_pixels`.

| Opción | Calidad | Tiempo por personaje | Costo | Licencia (leída 2026-10-30) |
|---|---|---|---|---|
| **1. AI sola, PixelLab** (MEDIDO) | El crudo no entra al estándar: mezcla lienzos de 64 y 88 px, sin celda de 192, 237 colores (recomendado ≤ 32), alfa binaria (`partial_alpha_pixels` 0). Silueta legible; los tres ataques se distinguen (medición inicial; ver el hallazgo de `attack_heavy` en la fila 5, que nace en este crudo). Consistencia: diferencia de histograma de color entre el primer frame de cada animación e `idle` de 14 a 25 %; cuerpo más robusto en las animaciones `v3`. | 12 min de generación en la primera tanda (2026-10-28 05:36–05:48 UTC) y 8 min entre el primer y el último envío de la segunda; sin tiempo del owner. | 12 generaciones (2 personaje, 6 plantilla con 1 repetición, 4 `v3`). $0 con bono diario del trial (`generations_used` de 7 a 19). Apprentice $12/mes por 2 000 generaciones equivaldría a ~$0.07; precio sin confirmar en su página de precios. | [Términos](https://pixellab.ai/termsofservice), con fecha 2026-10-30. §1.3: propiedad y uso comercial del usuario, sin condición de plan; ninguna cláusula nombra el trial. Prohíbe entrenar modelos con lo generado (§1.2). El usuario responde por no infringir (§3.3). |
| **2. AI sola, Retro Diffusion** (NO MEDIDO: exige cuenta, y un servicio externo pasa por el owner) | No medida. | No medido. | Sus cifras: $0.03–0.18 por imagen, animaciones desde $0.07, $0.50 gratis. ESTIMADO: del orden de $0.70–2 por personaje. | [Términos](https://www.retrodiffusion.ai/terms): lo generado es del usuario; lo hecho con uso gratuito pueden mostrarlo, y entrenan con outputs. |
| **3. Aseprite/LibreSprite con scripting** (MEDIDO solo la automatización) | No produce arte: quien dibuja es una persona, y el owner no es artista. ESTIMADO: calidad dependiente del dibujante; el `ref_a` dibujado por código es geométrico y no sirve como personaje. | MEDIDO: LibreSprite 1.0 instalado arma una hoja de 4 frames en 0.49 s por CLI. ESTIMADO: 12–24 h por personaje de ~47 frames, a 15–30 min por frame para un no artista. | LibreSprite $0. Aseprite $19.99 en Steam. MCPs comunitarios de Aseprite: requieren Aseprite, no instalados. | [LibreSprite](https://github.com/LibreSprite/LibreSprite): GPL-2.0. [EULA de Aseprite](https://github.com/aseprite/aseprite/blob/main/EULA.txt) y [FAQ](https://www.aseprite.org/faq/): permite uso comercial de lo creado, también compilado. MCPs ([diivi](https://github.com/diivi/aseprite-mcp), [MalloyTheDev](https://github.com/MalloyTheDev/aseprite-mcp)): MIT. |
| **4. Esqueletal** (MEDIDO la sonda; lo demás ESTIMADO) | Sonda de 15 líneas (brazo recortado, rotado con vecino más cercano): huecos y dientes de sierra visibles, porque un sprite plano no trae lo que hay detrás del brazo. Haría falta dibujar piezas: vuelve al problema del owner. ESTIMADO: viable solo con piezas hechas por una persona. 007 solo admite frames, así que tendría que hornear. | MEDIDO: sonda de 0.14 s y 4 frames. Sin cifra por personaje: depende de quien dibuje las piezas. | Spine Essential $69 (sin mallas ni IK). Rive: pago para exportar PNG (plan exacto sin confirmar). DragonBones: gratis. | [Spine](https://esotericsoftware.com/spine-editor-license): Essential bajo $500 000 de ingresos en 12 meses. [Rive](https://rive.app/pricing). [DragonBones](https://dragonbones.github.io/en/download.html): sin licencia de editor hallada y con poco mantenimiento aparente. |
| **5. Híbrido, ganador: AI + receta + `normalize` + `import`** (MEDIDO) | Pasa `run import` a la primera sin códigos y se reproduce en `/es/dev/arena` sin errores de consola, a 1024 px y 375 px, en tema oscuro; en tema claro a 375 px también se evaluó y se lee, aunque el número de daño cuesta leerlo (problema del renderer). 49 frames de receta, 47 distintos, 1 hoja; `colors` 32; `partial_alpha_pixels` 0. Mismas diferencias de consistencia (19–25 % con la paleta compartida): la receta reduce el salto de pose (quitando frames neutros), no el de cuerpo entre plantilla y `v3`. Alcance en impacto: 56, 68 y 47 px (mínimo 32); windups 120/300/320 ms. A 375 px se lee la silueta, pero se pierde el detalle de la cara. **Los tres ataques no se distinguen del todo** (game-designer, checklist, 2026-10-30): en `attack_heavy` el puño del frame de impacto queda cortado por el lienzo crudo de 64 px y el golpe se parece al ligero; los 5-6 px de diferencia de alcance en pantalla no bastan. Se corrige regenerando, con el OK del owner. | Los 20 min de generación más ~10 min de receta (2 iteraciones); `normalize` 3.1 s, `import` 2.9 s. Retoque a mano: 0 min (el visto bueno del owner está pendiente y puede añadir retoque). Sin contar aún la regeneración de `attack_heavy`. | El de PixelLab; la regeneración de `attack_heavy` aún no está sumada. | La de PixelLab. Selección y ajuste tienen algo más de base de autoría según [USCO Part 2](https://www.copyright.gov/ai/Copyright-and-Artificial-Intelligence-Part-2-Copyrightability-Report.pdf) (enero 2025), que dice que el prompt solo no basta; decisión caso por caso y solo para EE. UU. |

- El intent hablaba de Lua: el scripting de LibreSprite es JavaScript. La versión medida (1.0) no es la última estable (v1.3).
- Casi todas las licencias se leyeron con un lector web que resume; solo §1.3 de PixelLab se comprobó además en el HTML crudo. Hay que revisar el original antes de aceptar.

## Áreas de preocupación
1. **Aclaración del intent, confirmada por product-owner (2026-10-30).** "No entra en esos chequeos" debe leerse como "no los hace depender de una herramienta". `normalize` está en el paquete, así que su `tsc`, ESLint y Vitest corren en `just check`, solo como código puro con tests. `import` y `check` no lo cargan.
2. **"Sin duda" frente a la duda real.** El intent solo commitea el set si la licencia lo permite "sin duda", y la prueba se hizo con trial, que ninguna cláusula nombra. Por defecto se queda en `content/_local/` hasta que el owner decida (ADR-0013, Consecuencias). Puede apoyarse en una confirmación escrita de PixelLab.
3. **Gate de servicio externo.** La evaluación ya usó PixelLab y este repo no registra el OK del owner. Se pide dejarlo constar al aceptar el ADR.
4. **Faltan piezas privadas.** `private/provenance/ogro_del_pantano.md` ya existe en el checkout principal, con los prompts reconstruidos, y los crudos tienen allí una copia idéntica; faltan la copia fechada de los términos y el resguardo del export, y la revisión de RF6 sigue pendiente: si esos prompts no se confirman, el set se regenera o se queda en local.
5. **Medidas provisionales.** El tiempo ganador da 0 min de retoque de forma provisional; si el owner pide retoque, se mide y se actualiza la tabla. El tema claro a 375 px ya se evaluó y se lee; el número de daño cuesta leerlo, pero es del renderer, no de este set, y queda fuera de 004. La inconsistencia de cuerpo tampoco se corrige con la receta, y un solo personaje no prueba el estilo entre personajes.
6. **Orden.** El código precede a esta spec. Sigue haciendo falta un plan aprobado, que lo cubra tal como está, y ADR-0013 aceptado antes de commitear el set.
7. **`attack_heavy` no cumple el criterio de los tres ataques.** game-designer (checklist, 2026-10-30) halló que el puño del frame de impacto queda cortado por el lienzo crudo de 64 px y el golpe se parece al ligero. El alcance medido pasa el mínimo y `import` no da ningún código: este defecto solo lo detecta la revisión humana. Se corrige regenerando, con el OK del owner porque gasta generaciones; después hay que volver a medir esa fila y sumar tiempo y costo, como exige el intent para la opción ganadora.
8. **Los ataques no corresponden a sus habilidades.** Los ataques salen de `abilities` y del lore, y hoy "Panzazo de Lodo" (`mud_belly_flop`, `attack_heavy`) es un puñetazo y `swamp_burp` ("Eructo de Ciénaga", `special`) un golpe a dos manos. Se corrige al regenerar, con el OK del owner. Para `attack_heavy`, game-designer recomienda `v3` con un movimiento de panzazo, porque la plantilla puede repetir el corte del Área 7; no está medido.

## Respuestas a preguntas abiertas del intent
- **¿Estilo visual objetivo?** → Se arrastra a la identidad visual.
- **¿Saltar directo a esqueletal?** → No en esta fase (ADR-0013).
- **¿`origin` de un set con AI?** → `original` con `ai_assisted: true` (ADR-0013 D4), sin enmendar 007.
- **¿Boceto de un segundo personaje?** (nota de game-designer) → No se exige: gasta generaciones sin cerrar ningún criterio; el riesgo queda en el Área 5. game-designer puede pedirlo en su revisión.

## Estrategia de pruebas
- **Unit (Vitest, ya en `just check`):** `test/normalize.test.ts` y `test/guide-doc.test.ts` (códigos `normalize.*` en la guía).
- **Manual, registrada en el PR:** `normalize`, `run import` y arena a 375 px en los dos temas, con los tres ataques. Al publicar: revisión de `private/provenance/<id>.md` (RF6), `check`, `just sim --check` y `just check`.

## Fuera de alcance y alternativas
Fuera: producir el roster del MVP, la entrega del set al cliente (como en 007), la identidad visual y cualquier cambio al enum de `origin`.
Alternativas: ver Evaluación y [ADR-0013](../../decisions/0013-pipeline-de-sprites.md#alternativas-descartadas).

## Revisión
### product-owner — 2026-10-30 — REQUEST_CHANGES
- Verifiqué: cada criterio de éxito del intent tiene RF, fila de la tabla o casilla abierta declarada; las tres preguntas abiertas y las notas no bloqueantes quedan respondidas o arrastradas; ADR-0004 (entradas limpias y set fuera de git hasta los gates), ADR-0005 (solo costos de herramientas) y ADR-0007 (códigos `normalize.*`, documentados en la guía) se respetan; no entra el roster del MVP. Acepto `risk: medium` en la spec: la decisión legal está en ADR-0013 (`high`) y RF6 no publica sin el owner. Confirmo la aclaración del Área 1, y ya corregí esa frase en el intent. En el código, `src/cli.ts` no menciona `normalize`, el script `normalize` apunta a `src/normalize-cli.ts`, ningún otro archivo de `src/` importa `normalize.ts` y `validateRecipe` rechaza llaves desconocidas.
- Cambios pedidos:
  1. **RF2 y Área 1 describen el código de antes.** RF2 debe citar `src/normalize-cli.ts` en lugar de `cli.ts`. El Área 1 debe quitar "`cli.ts` lo importa de forma estática [...] no hay más dependencia que esa": ya es falso, y ahora `import` y `check` no cargan `normalize`. Esa frase sustenta justo lo que se pide confirmar, y el plan va a cubrir el código "tal como está".
  2. **RF3 promete de más.** "Solo números y modos [...] así que no caben prompts" no es exacto: `id` y `source` son cadenas libres, y también los nombres de animación, que son llaves libres. Las llaves desconocidas ya se rechazan, pero una cadena podría llevar texto. Decir "números, modos y nombres" y que las llaves exactas impiden un campo de prompt, o limitar esas cadenas en `validateRecipe` si se quiere mantener la promesa. El prompt sigue yendo solo en `private/provenance/<id>.md` (ADR-0013 D5).
- No bloquea: RF6 podría repetir que el checklist lo aprueba alguien distinto del autor y con el OK del owner, como dice el criterio del intent. Queda vigente el gate del Área 3: el OK del owner sobre PixelLab debe constar al aceptar el ADR.

**Respuesta del autor (architect, 2026-10-30):**
- (1) RF2 cita `src/normalize-cli.ts` y el Área 1 ya no habla de la importación estática. Verifiqué que el script `normalize` apunta a esa CLI y que `src/cli.ts` no lo menciona.
- (2) Tomé la segunda opción, ya aplicada en el código: `validateRecipe` limita `id`, `source` y los nombres de animación con `LIMITS.idPattern`. RF3 solo promete que no caben textos libres, salvo un identificador corto, y remite el prompt a `private/provenance/<id>.md`.
- No bloqueante: RF6 repite que el checklist lo aprueba alguien distinto del autor, con el OK del owner.
- Hechos nuevos, que **se suman a lo ya revisado y piden tu re-revisión**: el defecto de `attack_heavy` (fila 5, nota en la fila 1, Área 7, RF4 y RF6) y el tema claro a 375 px, ya evaluado (fila 5 y Área 5). También ajusté ADR-0013 (D3, D5 y Consecuencias). No había aprobaciones que conservar ni que borrar.

### product-owner — 2026-10-30 — APPROVE (re-revisión)
- Verifiqué: RF2 y el Área 1 coinciden con el código (`package.json` apunta a `src/normalize-cli.ts` y `src/cli.ts` no menciona `normalize`). RF3 es cierto: `validateRecipe` limita `id`, `source` y los nombres de animación con `idPattern` (`^[a-z][a-z0-9_]{0,47}$`, hasta 48 caracteres), y hay un test que rechaza un `source` fuera de la carpeta de entrada. RF6 ya recoge el criterio del intent sobre el checklist. ADR-0013 coincide en D3, D5 y Consecuencias.
- Hechos nuevos: `attack_heavy` no cambia la opción ganadora ni el alcance, porque es un defecto del crudo que se corrige regenerando. Queda bien acotado: RF6 impide publicar sin corregirlo, regenerar pasa por el OK del owner y el Área 7 exige volver a medir tiempo y costo, como pide el intent. Hasta entonces, las casillas de calidad y de tiempo y costo medidos de la ganadora siguen sin marcarse.
- Para el plan (no bloquean): (a) RF4 pide que la guía cubra el frame de impacto cortado y `docs/guides/pipeline-de-sprites.md` todavía no lo dice; el plan debe incluirlo, porque ese defecto solo lo detecta una persona. (b) El número de daño que cuesta leer en tema claro a 375 px queda bien fuera de 004, pero lo ve todo espectador en móvil: hay que registrarlo como issue propio del renderer, no solo dejarlo en esta spec.

### game-designer — 2026-10-30 — REQUEST_CHANGES
- Verifiqué: la tabla contra la receta y `content/_local/sprites/ogro_del_pantano/manifest.json`: 49 frames de receta y 47 distintos; alcance en impacto de 56, 68 y 47 px; windups 120/300/320 ms, dentro de los rangos de la guía. `normalize-cli.ts` y `validateRecipe` coinciden con RF2 y RF3, y `src/cli.ts` no carga `normalize`. Motor y balance no cambian: `sprites` no entra en `CharacterSpec`, así que no corrí `just sim`. No releí los términos de licencia: esa aceptación es del owner.
- **Área 5, corrección a la versión que me llegó:** el tema claro a 375 px sí se evaluó. Lo confirmo con `content/_local/review/arena-375-claro.png`, y el texto actual del Área 5 y de la fila 5 ya coincide: la silueta se lee, el contorno oscuro la separa del fondo y del suelo, y la cara pierde detalle igual que en oscuro. El número de daño cuesta leerlo porque es oscuro sobre la cabeza del sprite. Es una sola captura: vale para la silueta, no para distinguir los tres ataques.
- **Área 7, confirmada con datos:** de todos los crudos, solo `raw/ogro_del_pantano/attack_heavy/05.png` toca el borde del lienzo (9 filas opacas en el borde derecho), justo en el frame de impacto.
- Cambios pedidos:
  1. **RF6 no audita las entradas.** La regla de ADR-0004 sobre prompts y referencias no aparece en ningún RF ni en la estrategia de pruebas: solo en la guía. RF8 pide que el archivo exista, no que alguien lo lea, y el checklist de `write-character` solo mira el resultado. Agregar a RF6 que quien aprueba el checklist revisa `private/provenance/<id>.md` y confirma que ningún prompt nombra un personaje o franquicia y que no hubo material protegido como referencia, img2img o calco. Si los prompts del piloto no se pueden reconstruir, el set se regenera o se queda en local. El Área 4 debe decirlo, porque hoy ese archivo no existe.
- No bloquean:
  - **"Se corrige regenerando" puede repetir el defecto.** El corte sale de la plantilla `cross-punch` en un lienzo de 64 px, y no está medido si pasa siempre. Para el ogro recomiendo regenerar `attack_heavy` con `v3`: su lienzo fue mayor en la prueba y su cuerpo ya coincide con `special`, `block`, `dodge` y `win`. Un movimiento de panzazo, además, se distinguiría del golpe ligero y encajaría con `mud_belly_flop`, hoy un puñetazo recto.
  - **Lore para el visto bueno del owner.** `swamp_burp` se ve como un golpe agachado, y un eructo que no se ve pierde el chiste. No afecta el pipeline, pero conviene mirarlo antes de gastar en la regeneración.
  - **Número de daño.** "Queda fuera de 004" necesita un issue en Linear para que no se pierda: es parte de la emoción de la pelea.
- **Segundo personaje (pregunta que me dejó la spec):** no pido boceto ahora. El primer personaje que entre después de este piloto sirve de prueba de estilo, y se mira junto a Bruno antes de producir más.
- Esta revisión no aprueba el checklist "¿se parece demasiado?" del set: eso va en el PR, con los prompts a la vista.

**Respuesta del autor (architect, 2026-10-30), a game-designer:**
- (1) RF6 incluye la revisión de `private/provenance/<id>.md`, con su salida si los prompts no se pueden reconstruir; el Área 4 y la estrategia de pruebas lo recogen. D6 de ADR-0013 lleva la regla y su comprobación.
- Hechos nuevos: Área 8 y RF4 (ataques según `abilities` y lore; RF6 los exige corregidos) y RF2 (`--out` sin solape con `--in`, animación `null`). Recogí tu recomendación de `v3` en el Área 8. El issue del número de daño sigue pendiente de registrarse.
- No toco `depends_on` del ADR: ese campo lleva números de ADR, y el Contexto ya cita 007.
- La aprobación de product-owner se conserva, pero es anterior a RF6 ampliado y al Área 8; se le avisa por si quiere revisarlos.

### game-designer — 2026-10-30 — APPROVE (re-revisión)
- Verifiqué: RF6, el Área 4 y la estrategia de pruebas recogen la revisión de `private/provenance/<id>.md`, con la salida si los prompts no se pueden reconstruir. El Área 8 y RF4 recogen los ataques según `abilities` y lore. En RF2, `overlaps` de `normalize-cli.ts` y el rechazo de una animación `null` en `validateRecipe` existen.
- **La aprobación de product-owner vale.** Lo que cambió después no altera el problema ni el alcance: la opción ganadora y Fuera de alcance son los mismos. La revisión de prompts aplica una restricción que ya está en el intent, y el Área 8 solo endurece la puerta para publicar el piloto, que el owner ya controla. El costo es regenerar también `special`, con su OK.
- No bloquean: (a) la fila 5 y ADR-0013 hablan solo de la regeneración de `attack_heavy`; al volver a medir hay que sumar también la de `special`. (b) `test/normalize.test.ts` estaba en rojo durante esta re-revisión (31 fallos; por ejemplo, `checkOut` aún no existe). El archivo se había editado segundos antes, así que parece trabajo en curso. Este APPROVE cubre el texto de la spec, no ese estado: el código tiene que pasar `just check` antes del PR.
