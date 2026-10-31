---
id: "004"
title: Pipeline de assets originales
type: spec
status: done
risk: medium
author: architect
reviewers: [product-owner, game-designer]
approvals:
  - {by: product-owner, date: 2026-10-30}
  - {by: game-designer, date: 2026-10-30}
linear: TA-4
depends_on: ["002", "007"]
created: 2026-10-30
updated: 2026-10-31
---

# Spec: Pipeline de assets originales

> Intent: [intent.md](./intent.md) · Decisión: [ADR-0013](../../decisions/0013-pipeline-de-sprites.md) (`approved`) · Receta: [pipeline-de-sprites.md](../../guides/pipeline-de-sprites.md)

## Resumen
Elige el pipeline híbrido: AI, receta, `normalize` e `import`. Documenta lo ya construido y lo medido con `ogro_del_pantano`; no rediseña.
**Resultado (owner, 2026-10-31):** ADR-0013 aceptado y visto bueno visual al set regenerado, pero la licencia no queda aceptada para publicar por ahora. `ogro_del_pantano` sigue como personaje de pruebas, no es una versión final y se queda en `content/_local/`.
**Riesgo: `medium` en esta spec y `high` en el ADR.** La spec no toca auth, monedas, datos personales ni producción, y nadie redacta términos. La decisión legal vive en ADR-0013, que el owner ya aceptó, y publicar un set exige su OK (RF6). No sumo a security-reviewer: no hay superficie de ataque nueva. Si product-owner prefiere `high` también aquí, solo se agrega ese gate, que el owner ya ejerce.

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

**Resultado de RF6 y RF7 (2026-10-31; nota, los requisitos no cambian):** se aplica la rama "se queda en local". Falta la licencia aceptada por el owner. El owner dio su OK al checklist el 2026-10-31 (transmitido por el coordinador), solo para el personaje de prueba local; no vale para publicar. El set no se commitea, no hay `provenance.json`, `character.json` no lleva `sprites`, `check` da `{"sets":0,"characters":4,"links":0}` y la casilla del intent no se marca. El owner decidió cerrar TA-4 como Done con el set en local.

**RF5, prueba manual (2026-10-31):** en tema claro a 1024 px, una pelea completa sin errores de consola. Ataques, `hit`, `ko` y `win` se vieron en frames muestreados por script; `block` y `dodge` no se aislaron.

## Requisitos no funcionales
Trabajo local, sin carga en el VPS ni imagen de Docker (`tools/` queda fuera). Motor, `protocol`, `CharacterSpec` y balance no cambian. Sin dependencias nuevas: `normalize` usa `pngjs` y `@torneo/renderer/guide`, que el paquete ya tiene.

## Diseño
**Flujo:** idea + checklist → generador (crudos en `_local/raw`) → receta + `normalize` (`_local/export`) → `run import` (`_local/sprites`) → arena → gates del owner → `run import` (`content/sprites`) + `provenance.json`.
**Componentes:** `tools/sprite-importer`, `docs/guides/`, `content/sprites/`, `content/characters/ogro-del-pantano/`. Sin cambios en `protocol`, API, WS ni UI.

## Evaluación
Medido el 2026-10-30, salvo lo marcado; la fila 5 se volvió a medir el 2026-10-31. Personaje de prueba: `ogro_del_pantano`. No es asesoría legal. Criterios de calidad del intent: silueta a 375 px en dos temas; consistencia; tres ataques distinguibles; `colors` y `partial_alpha_pixels`.

| Opción | Calidad | Tiempo por personaje | Costo | Licencia (leída 2026-10-30) |
|---|---|---|---|---|
| **1. AI sola, PixelLab** (MEDIDO) | El crudo no entra al estándar: mezcla lienzos de 64 y 88 px, sin celda de 192, 237 colores (recomendado ≤ 32), alfa binaria (`partial_alpha_pixels` 0). Silueta legible; los tres ataques se distinguen (medición inicial; ver el hallazgo de `attack_heavy` en la fila 5, que nace en este crudo). Consistencia: diferencia de histograma de color entre el primer frame de cada animación e `idle` de 14 a 25 %; cuerpo más robusto en las animaciones `v3`. | 12 min de generación en la primera tanda (2026-10-28 05:36–05:48 UTC) y 8 min entre el primer y el último envío de la segunda; sin tiempo del owner. | 12 generaciones (2 personaje, 6 plantilla con 1 repetición, 4 `v3`). $0 con bono diario del trial (`generations_used` de 7 a 19). Apprentice $12/mes por 2 000 generaciones equivaldría a ~$0.07; precio sin confirmar en su página de precios. | [Términos](https://pixellab.ai/termsofservice), con fecha 2026-10-30. §1.3: propiedad y uso comercial del usuario, sin condición de plan; ninguna cláusula nombra el trial. Prohíbe entrenar modelos con lo generado (§1.2). El usuario responde por no infringir (§3.3). |
| **2. AI sola, Retro Diffusion** (NO MEDIDO: exige cuenta, y un servicio externo pasa por el owner) | No medida. | No medido. | Sus cifras: $0.03–0.18 por imagen, animaciones desde $0.07, $0.50 gratis. ESTIMADO: del orden de $0.70–2 por personaje. | [Términos](https://www.retrodiffusion.ai/terms): lo generado es del usuario; lo hecho con uso gratuito pueden mostrarlo, y entrenan con outputs. |
| **3. Aseprite/LibreSprite con scripting** (MEDIDO solo la automatización) | No produce arte: quien dibuja es una persona, y el owner no es artista. ESTIMADO: calidad dependiente del dibujante; el `ref_a` dibujado por código es geométrico y no sirve como personaje. | MEDIDO: LibreSprite 1.0 instalado arma una hoja de 4 frames en 0.49 s por CLI. ESTIMADO: 12–24 h por personaje de ~47 frames, a 15–30 min por frame para un no artista. | LibreSprite $0. Aseprite $19.99 en Steam. MCPs comunitarios de Aseprite: requieren Aseprite, no instalados. | [LibreSprite](https://github.com/LibreSprite/LibreSprite): GPL-2.0. [EULA de Aseprite](https://github.com/aseprite/aseprite/blob/main/EULA.txt) y [FAQ](https://www.aseprite.org/faq/): permite uso comercial de lo creado, también compilado. MCPs ([diivi](https://github.com/diivi/aseprite-mcp), [MalloyTheDev](https://github.com/MalloyTheDev/aseprite-mcp)): MIT. |
| **4. Esqueletal** (MEDIDO la sonda; lo demás ESTIMADO) | Sonda de 15 líneas (brazo recortado, rotado con vecino más cercano): huecos y dientes de sierra visibles, porque un sprite plano no trae lo que hay detrás del brazo. Haría falta dibujar piezas: vuelve al problema del owner. ESTIMADO: viable solo con piezas hechas por una persona. 007 solo admite frames, así que tendría que hornear. | MEDIDO: sonda de 0.14 s y 4 frames. Sin cifra por personaje: depende de quien dibuje las piezas. | Spine Essential $69 (sin mallas ni IK). Rive: pago para exportar PNG (plan exacto sin confirmar). DragonBones: gratis. | [Spine](https://esotericsoftware.com/spine-editor-license): Essential bajo $500 000 de ingresos en 12 meses. [Rive](https://rive.app/pricing). [DragonBones](https://dragonbones.github.io/en/download.html): sin licencia de editor hallada y con poco mantenimiento aparente. |
| **5. Híbrido, ganador: AI + receta + `normalize` + `import`** (MEDIDO; re-medido 2026-10-31, con todas las animaciones en `v3`) | Pasa `run import` sin códigos y una segunda corrida es idéntica byte a byte. En `/es/dev/arena`, sin errores de consola a 1024 px en oscuro y a 375 px en oscuro y claro; se revisaron capturas, no el movimiento, y el owner dio su visto bueno al set. El número de daño en claro sigue en TA-36 (renderer). 51 frames de receta, 46 distintos, 1 hoja; `colors` 32 (277 de entrada); `partial_alpha_pixels` 0. Consistencia: 4.8–9.4 % de diferencia de histograma frente a `idle` (game-designer). Alcance en impacto: 68, 77 y 48 px (mínimo 32); windups 130/260/310 ms. Ningún frame que usa la receta toca el borde de su lienzo crudo de 88×88. **Ataques** (game-designer, re-checklist 2026-10-31, APPROVE como personaje de prueba local): `attack_heavy` es un panzazo, `special` un eructo inclinado con la boca abierta y `attack_light` un golpe recto; corresponden a `abilities` y se distinguen. Puntos débiles: en capturas, `special` se parece un poco a un grito; el perfil boca abajo de `ko` recuerda al panzazo; el último frame de `attack_heavy` termina 2 px sobre el suelo; `attack_light` deja ver un calzón café. **Antes (2026-10-30, plantillas y `v3` mezcladas):** 49 frames de receta y 47 distintos; 19–25 % de diferencia; alcance de 56, 68 y 47 px; windups 120/300/320 ms; el puño de `attack_heavy` cortado por el lienzo de 64 px, y `attack_heavy` y `special` sin corresponder a sus habilidades. | **Acumulado de las dos tandas:** ~35 min de generación (los 20 iniciales + 878 s, ~15 min, de la regeneración, con trabajos en serie, de uno en uno en el trial) y ~10 min de receta de la primera tanda (2 iteraciones). El ajuste de la receta para la regeneración no se cronometró aparte, así que ~45 min es un mínimo. `normalize` 3.8 s, `import` 3.9 s. Retoque a mano: 0 min, final. Antes: ~30 min, sin regeneración. | **19 generaciones cobradas por el saldo** (12 + 7; `generations_used` de 7 a 26), o 20 si se cuenta lo que informan las llamadas: la regeneración hizo 8 llamadas de 1 generación cada una, y el saldo bajó 7. Dos de las 8 se descartaron: el primer panzazo y el primer eructo traían un artefacto, un saco beige o blanco frente a la panza, que se corrigió con el prompt. $0 con bono diario del trial; con Apprentice equivaldría a ~$0.11 (precio sin confirmar). Incluye lo descartado: un personaje hecho con `v3` desde el principio no está medido. | La de PixelLab, también en trial para la regeneración. El owner no la acepta para publicar, por ahora (Área 2). Selección y ajuste tienen algo más de base de autoría según [USCO Part 2](https://www.copyright.gov/ai/Copyright-and-Artificial-Intelligence-Part-2-Copyrightability-Report.pdf) (enero 2025), que dice que el prompt solo no basta; decisión caso por caso y solo para EE. UU. Este set no tiene retoque a mano. |

- La fila 1 es la medición inicial del crudo y no se actualiza; la vigente de la opción ganadora es la fila 5.
- El intent hablaba de Lua: el scripting de LibreSprite es JavaScript. La versión medida (1.0) no es la última estable (v1.3).
- Casi todas las licencias se leyeron con un lector web que resume; solo §1.3 de PixelLab se comprobó además en el HTML crudo. Hay que revisar el original antes de aceptar.

## Áreas de preocupación
1. **Aclaración del intent, confirmada por product-owner (2026-10-30).** "No entra en esos chequeos" debe leerse como "no los hace depender de una herramienta". `normalize` está en el paquete, así que su `tsc`, ESLint y Vitest corren en `just check`, solo como código puro con tests. `import` y `check` no lo cargan.
2. **"Sin duda" frente a la duda real.** Resuelta (owner, 2026-10-31): no acepta la licencia para publicar por ahora, porque el ogro sigue en pruebas y no es una versión final, así que se queda en `content/_local/`; la confirmación escrita de PixelLab sigue siendo una opción si algún día se publica.
3. **Gate de servicio externo.** Resuelta (owner, 2026-10-31): deja constar su OK al uso de PixelLab, regeneración incluida (ADR-0013, Revisión).
4. **Faltan piezas privadas.** Resuelta para el uso local (2026-10-31): `private/provenance/ogro_del_pantano.md` lleva los prompts de la regeneración, revisados por game-designer (ninguno nombra un personaje o franquicia; la ausencia de referencias e img2img consta en ese registro), y crudos y export tienen copia idéntica en el checkout principal; la copia de los términos está en `private/provenance/pixellab-terminos-2026-10-31.html` ("Last Updated: 2026-10-30", tomada el 2026-10-31), pero no prueba los del 2026-10-28 y el 2026-10-29, cuando se generaron el personaje base, `block`, `dodge` y `win`: publicar exige regenerarlos o una confirmación escrita de PixelLab.
5. **Medidas provisionales.** Resuelta con el set regenerado (2026-10-31): retoque final de 0 min, inconsistencia de cuerpo de 4.8–9.4 % y número de daño en TA-36, fuera de 004; lo que sigue sin probarse es el estilo entre personajes.
6. **Orden.** El código precede a esta spec. Hay plan aprobado que lo cubre, y ADR-0013 ya está aceptado (owner, 2026-10-31); eso no abre la publicación, que sigue dependiendo de RF6.
7. **`attack_heavy` no cumple el criterio de los tres ataques.** Resuelta (2026-10-31): regenerado en `v3` con el OK del owner, ya no se corta, los tres ataques se distinguen (game-designer) y la fila 5 está medida de nuevo, con tiempo y costo.
8. **Los ataques no corresponden a sus habilidades.** Resuelta (2026-10-31): `attack_heavy` es un panzazo y `special` un eructo con la boca abierta, regenerados con el OK del owner (game-designer, APPROVE como personaje de prueba; OK del owner al checklist el 2026-10-31, solo para ese uso local; los puntos débiles están en la fila 5).

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

**Nota del autor (architect, 2026-10-31):**
- Hechos posteriores a las aprobaciones, que **piden la re-revisión de product-owner y game-designer**: Resumen, nota de resultado de RF6 y RF7, fila 5 y Áreas 2, 3, 4, 5, 7 y 8. Los requisitos no cambian. Las aprobaciones se conservan a la espera de esa re-revisión.
- Comprobé: 51 frames de receta, 46 en el manifest, los windups contra `frame_ms` e `impact_frame`, el lienzo de 88×88 sin píxeles opacos en el borde, `check`, y crudos y export idénticos en el checkout principal. No volví a medir el alcance, el histograma, los tiempos ni el saldo: vienen de la medición de esa corrida y de game-designer. Tampoco releí los términos.

### product-owner — 2026-10-31 — APPROVE (re-revisión)
- Verifiqué:
  - **Alcance.** RF1 a RF9, Fuera de alcance y las respuestas a las preguntas abiertas coinciden con la versión del 2026-10-30. Comparé con una copia de esa versión en otro worktree, no con `git diff`, porque aquí no tengo shell. Los cambios son resultados, no requisitos.
  - **El problema sigue resuelto.** La spec deja un pipeline repetible y medido aunque el set no se publique, y RF6 ya preveía la rama local. Resumen y Áreas 2 y 3 coinciden con ADR-0013 y con las decisiones del owner, que conozco por el coordinador. Sigo aceptando `risk: medium`: el owner ya ejerció el gate legal en el ADR.
  - **ADRs.** 0004: leí los prompts del archivo privado y ninguno nombra un personaje o franquicia; la ausencia de referencias e img2img es lo que declara el registro, sin poder auditarla en el generador. 0005: solo costos de herramientas, sin dinero real. 0007: sin texto de UI.
  - **Fila 5.** Recontadas en la receta: 51 frames, 46 distintos, windups de 130/260/310 ms. Las sumas cuadran: 12 + 7 = 19 generaciones, 20 + 15 = 35 min y ~45 min con la receta, declarado como mínimo. El manifest trae las 9 acciones, y `arena-375-claro.png` muestra el set en la arena. Alcance e histograma coinciden con el checklist de game-designer, sin medirlos yo. Tampoco medí los tiempos ni el saldo.
- Para el autor antes del PR. Son textos que hoy quedaron viejos y no cambian este veredicto ni piden otra re-revisión mía, si se limitan a esto:
  1. **Nota de RF6 y RF7 y Área 8:** dicen que el OK del owner al checklist sigue pendiente. Ya lo dio, para el personaje de prueba local. El set se queda en local por la licencia.
  2. **Área 4:** dice que la copia fechada de los términos no se guardó. Ya está en `private/provenance/`, tomada el 2026-10-31, después de generar. Lleva "Last Updated: 2026-10-30", así que no prueba qué términos regían el 2026-10-28 y 2026-10-29, cuando se generaron el personaje base, `block`, `dodge` y `win`. No afecta al set local; ADR-0013 D5 pide los vigentes al generar, así que hay que resolverlo antes de publicar. ADR-0013 (Consecuencias) y el plan repiten lo de la copia y el plan lo del OK.
  3. **Área 6:** sigue pidiendo plan aprobado y ADR aceptado, y ya existen.
- Sigue abierto, sin bloquear esta spec: RF5 pide las 9 acciones en la arena y solo hay capturas. La fila 5 lo declara y el plan lo mantiene sin marcar. La estrategia de pruebas lo pide registrado en el PR.

### game-designer — 2026-10-31 — APPROVE (re-revisión)
- Verifiqué con `git diff HEAD`:
  - **Puertas:** RF6, RF7 y RF8 no cambian. La nota de resultado solo aplica la rama "se queda en local".
  - **Fila 5, frente a mi checklist del 2026-10-31 y a la receta:** 51 frames de receta (sumados por animación), 46 distintos, 277 → 32 colores, 0 de alfa parcial, 68, 77 y 48 px, windups 130/260/310 ms (sumados de `frame_ms`) y 4.8–9.4 %. Los puntos débiles también coinciden.
  - **Costo:** 12 + 7 = 19; `generations_used` de 7 a 26; ~$0.11 a $12 por 2 000; 8 llamadas contra el registro privado. Tiempo: 35 + 10 = ~45 min.
  - **Estado:** `check` da `{"sets":0,"characters":4,"links":0}`, `character.json` no lleva `sprites`, y crudos y export son idénticos en el checkout principal (`diff -rq`). Las Áreas 7 y 8 y el OK pendiente del owner al checklist dicen lo mismo que mi checklist.
  - **Texto:** no lleva prompts del personaje de prueba.
- No bloquean:
  1. **Área 4:** "la copia fechada de los términos no se guardó" quedó vieja. `private/provenance/pixellab-terminos-2026-10-31.html` apareció a las 15:16 de hoy.
  2. **Área 6:** sigue pidiendo "ADR-0013 aceptado", que ya lo está.
  3. **Registro privado, pendiente de mi checklist:** `private/provenance/ogro_del_pantano.md` todavía dice 12 generaciones, no incluye la regeneración en su cabecera de fechas y dice que falta la copia. La fila 5 ya se cerró con el valor correcto, así que falta actualizar ese archivo.
- Límites: el saldo, los tiempos y la segunda corrida idéntica byte a byte son del autor; yo no pude repetir `import`. Esta aprobación cubre gameplay y diseño, no la licencia. La casilla del checklist en el intent sigue abierta hasta el OK del owner.
