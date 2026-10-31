---
id: "004"
title: Pipeline de assets originales
type: intent
status: done
risk: medium
author: product-owner
reviewers: [architect, game-designer]
approvals:
  - {by: architect, date: 2026-10-30}
  - {by: game-designer, date: 2026-10-30}
linear: TA-4
depends_on: ["002", "007"]
created: 2026-10-20
updated: 2026-10-31
---

# Intent: Pipeline de assets originales

## Problema
No podemos publicar sprites de MUGEN (ADR-0004) y el owner no es artista. Necesitamos una forma **repetible, barata y de calidad decente** de crear personajes originales con todas sus animaciones, en el formato del renderer.

## Resultado propuesto
Un pipeline evaluado y documentado (más un ADR) que diga cómo pasar de "idea de personaje" a "carpeta de sprites lista", con tiempos y costos reales medidos al crear **un personaje completo** de prueba.

Opciones a evaluar (como mínimo):
1. **Generadores de pixel art con AI** (tipo PixelLab o Retro Diffusion): calidad, consistencia entre frames y animaciones, costo y licencia de uso comercial.
2. **Aseprite o LibreSprite con scripting en Lua** y MCPs comunitarios, para que Claude automatice partes (paletas, recortes, exportar spritesheets).
3. **Animación esqueletal** (Spine, DragonBones, Rive): un solo dibujo en piezas y muchas animaciones baratas; candidato para la fase 3.
4. **Híbrido**: la AI genera la base, se retoca a mano y la exportación es automática.

## Usuarios y sistemas afectados
- Game-designer y owner (producción de contenido).
- `content/sprites/<id>/` (el set, con su `provenance.json`) y `content/characters/` (el personaje lo enlaza con `"sprites": "<id>"` en `character.json`).
- `tools/`: `tools/sprite-importer` ya existe (007); lo que agregue 004 también va aquí.

## Restricciones
- Presupuesto cero o casi cero en esta etapa; los costos recurrentes se documentan.
- La licencia de lo generado debe permitir uso comercial futuro y publicar el set en un repo público bajo `LICENSE-CONTENT.md` (la restricción de procedencia de 007).
- Exporta al estándar de 007: formato abierto `torneo.sprites.export` v1, entra por `tools/sprite-importer` (`run import` y `check`) y cumple `docs/guides/sprites.md`.
- ADR-0004: nunca sprites de MUGEN ni assets de franquicias en git. Tampoco en las entradas: ningún prompt nombra un personaje o franquicia, y ningún material protegido sirve de referencia, img2img o calco de un set que se publica, ni siquiera en local, ni se sube a un servicio externo.
- Procedencia obligatoria y sin datos personales: `provenance.json` en el set; los datos identificables, la prueba de derechos, los prompts y las referencias van en `private/provenance/<id>.md`, para que la regla se pueda auditar.
- Independiente de la herramienta (007): `just check`, CI y el importador siguen funcionando sin herramienta de autoría, servicio externo ni credenciales. Lo que 004 agregue en `tools/` es opcional, no los hace depender de una herramienta y no deja secretos en el repo. Ni las fuentes de la herramienta ni los exports van en el set; si hacen falta para repetir el pipeline, la spec dice dónde viven.
- Quién decide: son del owner el visto bueno visual del personaje, aceptar el ADR y aceptar los términos de licencia de lo generado (tema legal). Adoptar una dependencia de pago o un servicio externo, aunque sea gratuito o un MCP comunitario, requiere su OK previo (gate de `docs/process.md`).

## Criterios de éxito
- [x] Tabla comparativa con calidad, tiempo por personaje, costo y licencia. Los términos de licencia se leen en la fuente y llevan fecha; los datos se miden donde se pueda, y lo no medido se marca como estimado. Esto último no vale para la opción ganadora: su tiempo y su costo por personaje, retoque incluido, salen medidos del personaje de prueba.
  - Estado (2026-10-31): la tabla está en la [Evaluación de la spec](./spec.md#evaluación). De la ganadora, costo medido de 19 generaciones cobradas y 0 min de retoque. Tiempo: ~45 min como mínimo, porque el ajuste de la receta en la regeneración no se cronometró aparte.
  - La marco porque todo lo que la fila 5 afirma sale del personaje de prueba, y lo que falta está declarado y solo puede subir el total; ~45 min es un mínimo de esta prueba, no el tiempo medido de un personaje completo ni de una corrida limpia.
  - Casi todas las licencias se leyeron con un lector web que resume. Sirve para comparar, no para aceptarlas.
- [x] La columna de calidad mide, como mínimo: silueta legible a 375 px sobre los dos temas; silueta y paleta consistentes entre frames y animaciones; los tres ataques se distinguen entre sí; y `colors` y `partial_alpha_pixels` del resumen del importador.
  - Estado (2026-10-31): fila 5 de la tabla. La revisión fue con imágenes y frames fijos, no con movimiento. Los tres ataques a 375 px se compararon con frames escalados por script, no en el renderer.
- [x] `ogro-del-pantano`, del roster existente, queda como personaje de prueba completo (todas las animaciones obligatorias de `docs/guides/sprites.md`) hecho con la opción ganadora, que pasa `run import` y se reproduce en `/[lang]/dev/arena`. Solo se le agrega el campo `sprites`, sin tocar su balance, y `just sim --check` se corre de todos modos.
  - Estado (2026-10-31): `sprites` no se agrega porque RF7 de la spec y la casilla siguiente lo condicionan a que el set esté en `content/sprites/`, y se queda en local.
  - `just sim --check` pasó el 2026-10-31 (exit 0).
  - Las 9 acciones están en el set y `run import` pasa. La arena reprodujo las 9 acciones sin errores, también en escritorio con tema claro; la calidad de las poses de `block` y `dodge` queda sin juzgar (ver el plan).
- [ ] El set se commitea en `content/sprites/<id>/` y pasa `check`, si la licencia lo permite sin duda y el owner da su OK. Si falta alguna de las dos, se queda en `content/_local/` y esta casilla no se marca.
  - Estado (2026-10-31): no se marca. El owner no acepta la licencia para publicar, por ahora, porque es un personaje de prueba y no arte final. El set se queda en `content/_local/`.
- [x] Ese personaje pasa el checklist "¿se parece demasiado?" de `write-character`, con su ítem de diseño visual ya aplicado a los sprites. Lo aprueba game-designer o product-owner, distinto del autor, con el OK del owner.
  - Estado (2026-10-31): `APPROVE` de game-designer, prompts incluidos, y OK del owner transmitido por el coordinador. Vale **solo como personaje de prueba local**: no sirve para publicar el set ni para llevarlo a arte final.
  - El motivo del lore se corrige en TA-41, por decisión del owner. El comentario firmado que pide `write-character` está en el PR de cierre (#72).
- [x] El owner da su visto bueno visual al personaje.
  - Estado (2026-10-31): dado sobre el set regenerado, transmitido por el coordinador.
- [x] Un ADR con el pipeline elegido, que incluye cómo se declara `origin` en un set hecho con AI.
  - Estado (2026-10-31): [ADR-0013](../../decisions/0013-pipeline-de-sprites.md), aceptado por el owner con D4.
- [x] Una receta paso a paso de idea a export, en el ADR o en un doc que él enlace, que se pueda seguir sin depender de lo que recuerde quien hizo la prueba.
  - Estado (2026-10-31): [receta del pipeline](../../guides/pipeline-de-sprites.md). Sus cambios de esta fecha los aprobaron game-designer y architect. Nadie la ha seguido todavía desde cero con otro personaje.

## Fuera de alcance
Producir el roster completo del MVP.

## Preguntas abiertas
- ¿Estilo visual objetivo? Respondida en parte: 007 fijó la celda de 192×192 y recomendaciones de paleta y contorno. El estilo final sigue pendiente y se define con la identidad visual: la spec la arrastra allí.
- ¿Vale la pena saltar directo a esqueletal? 007 dejó esqueletal fuera del formato (solo sheet + manifest), así que esqueletal solo sirve si hornea a frames. **Respondida:** no en esta fase (ADR-0013 D7). Se puede reabrir si aparecen piezas hechas por una persona.
- ¿Qué `origin` declara en `provenance.json` un set hecho con AI (la opción híbrida y la de solo AI)? RF-B4 de 007 define `generated` como "lo genera código de este repo", `original` como "owner o equipo" y `commission` como encargo, y lo que sale de un servicio de AI de terceros no encaja claro en ninguno. Propongo resolverla en el ADR de esta feature, partiendo de la propuesta de game-designer (`original` con `ai_assisted: true`). Si hace falta cambiar el enum, es una enmienda al estándar de 007, no un parche de 004. **Respondida:** `original` con `ai_assisted: true`, sin cambiar el enum (ADR-0013 D4, lectura confirmada por el owner el 2026-10-31).

## Revisión
### game-designer — 2026-10-30 — REQUEST_CHANGES
- Verifiqué: la enmienda coincide con 007 y la guía: formato `torneo.sprites.export` v1, `run import` y `check`, las 9 acciones obligatorias, celda de 192×192, set en `content/sprites/<id>/` con `provenance.json`, vínculo `"sprites"` y datos identificables en `private/provenance/<id>.md`. `/[lang]/dev/arena` existe (`reroute` sobre `routes/dev/arena`). Lo de esqueletal es correcto: solo sirve horneado a frames. El frontmatter es válido y las preguntas abiertas están arrastradas con razón. Lo que falla es que un pipeline con AI cumpla ADR-0004 de forma comprobable y que "calidad" y "repetible" se puedan verificar.
- Cambios pedidos:
  1. **ADR-0004 también en las entradas.** La restricción se queda en "nunca sprites de MUGEN ni assets de franquicias en git", pero el ADR prohíbe además nombres y diseños protegidos, y el checklist solo mira el resultado. Agregar: ningún prompt nombra un personaje o franquicia, y ningún material protegido sirve de referencia, img2img o calco, ni siquiera en local, para un set que se publica. Los prompts y las referencias quedan en `private/provenance/<id>.md`, y así la regla se puede auditar.
  2. **Licencia y servicios.** "Uso comercial futuro" no alcanza: el criterio tiene que cubrir también publicar los PNG en un repo público y bajo `LICENSE-CONTENT.md` (`license` es siempre `content`). Además, el gate de `docs/process.md` cubre "una dependencia de pago o un servicio externo", y el intent solo habla de pago: usar un servicio externo, aunque sea gratis, también requiere el OK previo del owner.
  3. **"Calidad" verificable y visto bueno como criterio.** Definir qué mide la columna, como mínimo: silueta legible a 375 px sobre los dos temas; silueta y paleta consistentes entre frames y animaciones; los tres ataques se distinguen entre sí; y `colors` y `partial_alpha_pixels` del resumen del importador. El visto bueno visual del owner hoy es una restricción: tiene que ser también una casilla de los criterios de éxito.
  4. **Medido y repetible.** "Lo no medido se marca como estimado" no puede valer para la opción ganadora: su tiempo y su costo por personaje, retoque incluido, salen del personaje de prueba. El Problema pide algo "repetible" y ningún criterio lo comprueba: agregar una receta paso a paso de idea a export, en el ADR o en un doc que él enlace, sin depender de lo que recuerde quien hizo la prueba.
  5. **Qué personaje y dónde queda.** Decir si es uno del roster o uno nuevo. Recomiendo uno del roster: `sprites` no entra en la forma que ve el motor, así que no toca el balance y el lore ya existe. Si es nuevo, entra por `write-character` y además pasa `just sim --check`. Decir también que el set se commitea, porque `check` solo valida `content/sprites/` y la arena prueba desde `content/_local/sprites/`. Y que el checklist lo aprueba quien fija el skill (un agente distinto del autor, más el OK del owner), con su ítem de diseño visual ya aplicado a los sprites.
  6. **`origin` de un set con AI.** Ningún valor encaja claro: la spec de 007 define `generated` como "lo genera código de este repo", así que un set salido de un generador externo no cabe ahí. Agregar como pregunta abierta o como exigencia del ADR cómo se declara (mi propuesta: `original` con `ai_assisted: true`), o `ai_assisted` pierde su utilidad de trazabilidad.
- No bloquean: (a) el `id` del set usa guion bajo y el `slug` guion (`rata_electrica` frente a `rata-electrica`); conviene que la receta lo diga. (b) Un solo personaje no prueba que el estilo se sostenga entre personajes; la spec puede decidir si basta con un boceto de un segundo personaje, sin producirlo completo. (c) Conviene que la spec reevalúe `risk: medium`: leer términos de licencia de un generador para publicar su salida roza "temas legales" de `docs/process.md`.

### architect — 2026-10-30 — REQUEST_CHANGES
- Verifiqué: la enmienda delegada por 007 está (`depends_on: ["002", "007"]` y "exporta al estándar de 007"); frontmatter válido, autor distinto de los revisores. Las rutas y comandos coinciden con la guía, `content/sprites/README.md` y `tools/sprite-importer/README.md` (`content/sprites/<id>/`, `"sprites": "<id>"`, `run import`, `check`, `private/provenance/<id>.md`). La celda de 192×192 y la exclusión de esqueletal coinciden con 007. El criterio de la arena es viable: `/[lang]/dev/arena` sirve `content/_local/sprites/` con `just dev`, que es el paso 2 del flujo de la guía. Es viable con el stack y sin carga en el VPS: producir arte es trabajo local. Sin choque con ADR-0001, 0002, 0005 ni 0007.
- Cambios pedidos:
  1. **ADR-0004 queda acotado de menos.** La restricción solo habla de git, pero el ADR también dice que los sprites de MUGEN se usan solo en local para prototipar el renderizado y que no se usan nombres, diseños ni sprites protegidos. Con generadores de AI eso importa: agregar que ningún sprite de MUGEN ni asset de franquicia se usa como entrada o referencia de un generador ni se sube a un servicio externo, y que los prompts no llevan nombres de personajes ni franquicias. `content/_local/` tiene justo ese material a mano.
  2. **Licencia insuficiente.** "Uso comercial futuro" no alcanza. El piloto se commitea (`check` solo valida `content/sprites/`), y `LICENSE-CONTENT.md` declara ese contenido "© Torneo Absurdo, todos los derechos reservados". Exigir también lo que pide la restricción de procedencia de 007: que los términos permitan publicar el set en un repo público bajo esa licencia. Agregar a "Quién decide" que aceptar esos términos es del owner: es tema legal y `docs/process.md` lo manda a su gate. Con eso `risk: medium` se sostiene, porque nadie redacta términos legales.
  3. **Gate más angosto que el del proceso.** "Gastar en un servicio de pago" no cubre lo que dice `docs/process.md`: "una dependencia de pago o un servicio externo". Un servicio externo gratuito o un MCP comunitario también pasan por el OK del owner antes de adoptarse en el pipeline.
  4. **Falta la independencia de la herramienta de 007.** Agregar como restricción que `just check`, CI y el importador siguen funcionando sin herramienta de autoría, servicio externo ni credenciales: lo que 004 agregue en `tools/` es opcional, no entra en esos chequeos y no deja secretos en el repo. Y que ni las fuentes de la herramienta ni los exports van en el set (la spec dice dónde viven, si hacen falta para repetir el pipeline).
  5. **Pregunta abierta nueva: `origin` en `provenance.json`.** RF-B4 de 007 define `generated` como "lo genera código de este repo", `original` como "owner o equipo" y `commission` como encargo. Lo que sale de un servicio de AI de terceros no encaja claro en ninguno, y la guía no repite esa definición. Arrastrarla para que la spec decida el valor de la opción híbrida y la de solo AI. Si hace falta cambiar el enum, es una enmienda al estándar de 007, no un parche de 004.
- Para la spec (no bloquean): (a) decidir si el piloto es uno de los cuatro personajes existentes o uno nuevo; uno nuevo además entra en `just sim --check`. (b) Decidir dónde vive la tabla comparativa, sabiendo que el repo es público: costos de herramientas sí, nada de precios ni estrategia propios. (c) Dejar explícito en Fuera de alcance que la entrega del set al cliente sigue fuera, como en 007.

**Respuesta del autor (product-owner, 2026-10-30):**
- game-designer: (1) la restricción de ADR-0004 cubre también las entradas (prompts sin nombres, nada de material protegido como referencia, img2img o calco, ni en local) y prompts y referencias van en `private/provenance/<id>.md`. (2) La licencia exige publicar en un repo público bajo `LICENSE-CONTENT.md`, y el gate cubre también servicios externos gratuitos. (3) Criterio nuevo que define la columna de calidad y casilla para el visto bueno visual del owner. (4) Lo no medido ya no vale para la opción ganadora, y hay criterio nuevo de receta paso a paso. (5) Decisión tomada: el personaje de prueba es `ogro-del-pantano`, del roster; solo se le agrega `sprites` y `just sim --check` se corre de todos modos. El set se commitea en `content/sprites/<id>/` si la licencia lo permite sin duda y el owner da su OK. El checklist lo aprueba game-designer o product-owner, distinto del autor, con el OK del owner. (6) Pregunta abierta nueva sobre `origin`, resumida abajo.
- architect: (1) mismo cambio que el punto 1 de game-designer, con la prohibición de subir ese material a un servicio externo. (2) Licencia como arriba, y aceptar los términos pasa a "Quién decide" como tema legal del owner; `risk: medium` se mantiene porque nadie redacta términos. (3) Gate alineado con `docs/process.md`, MCP comunitarios incluidos. (4) Restricción nueva de independencia de la herramienta; fuentes y exports fuera del set. (5) La pregunta de `origin` queda abierta. Difiero en dónde se resuelve: propongo el ADR y no la spec, y el ADR lo exige un criterio de éxito. Si hace falta cambiar el enum, sigue siendo una enmienda a 007.
- No bloqueantes: se arrastran a la spec dónde vive la tabla, la entrega al cliente en Fuera de alcance, el `id` con guion bajo frente al `slug` con guion en la receta, si hace falta un boceto de un segundo personaje y reevaluar `risk`. La nota (a) de architect queda resuelta con `ogro-del-pantano`.
- No hay dos secciones `## Revisión`: solo había una, así que no unifiqué nada.

### game-designer — 2026-10-30 — APPROVE (re-revisión)
- Verifiqué mis seis cambios contra el texto actual, ADR-0004, `docs/process.md`, `write-character`, la guía y RF-B4 a RF-B6 de 007: (1) las entradas quedan cubiertas (prompts, referencia, img2img, calco, también en local) y lo auditable va en `private/provenance/<id>.md`; (2) la licencia exige repo público bajo `LICENSE-CONTENT.md` y el gate coincide con "una dependencia de pago o un servicio externo"; (3) la columna de calidad mide lo pedido, `colors` y `partial_alpha_pixels` existen en el resumen del importador y el visto bueno del owner es casilla; (4) el tiempo y el costo de la ganadora salen medidos y hay criterio de receta; (5) `ogro-del-pantano` existe en el roster, `sprites` no entra en `CharacterSpec` y quién aprueba el checklist coincide con el skill; (6) `origin` queda abierta y el ADR la exige. Acepto resolverla en el ADR.
- Para la spec (no bloquean): (a) el campo `sprites` se agrega solo si el set queda en `content/sprites/`. Tal como está escrito, el criterio del personaje de prueba lo agrega siempre, pero si el set se queda en `content/_local/`, `check` falla con `link.unknown_set`. La arena no lo necesita: lista `content/_local/sprites/` por su cuenta. `just check` lo atrapa, así que no hay daño silencioso. (b) Un ogro de pantano es el arquetipo del roster con el referente visual más obvio: la spec debe pedir que silueta, paleta y vestimenta se fijen con el checklist antes de generar, no al final, para no gastar la prueba en un set que luego no pase.

### architect — 2026-10-30 — APPROVE (re-revisión)
- Verifiqué mis cinco cambios contra el texto actual, ADR-0004 y `docs/process.md`: (1) la restricción de ADR-0004 cubre prompts, referencias, img2img, calco y subida a servicios externos; (2) la licencia exige publicar en repo público bajo `LICENSE-CONTENT.md` y aceptar los términos es del owner; `risk: medium` se sostiene porque nadie redacta términos; (3) el gate coincide con "dependencia de pago o servicio externo" e incluye MCP comunitarios; (4) la independencia de la herramienta está, con fuentes y exports fuera del set; (5) la pregunta de `origin` sigue abierta y RF-B4 de 007 está bien citado.
- Acepto el cambio de lugar: `origin` se resuelve en el ADR y no en la spec. Es mejor sitio, porque ese valor marcará a todo set futuro y no solo a esta feature; un criterio de éxito lo exige y cambiar el enum sigue siendo enmienda a 007.
- Para la spec (no bloquean): (a) ordenar el ADR antes de commitear el set, porque `provenance.json` exige `origin` y `check` solo acepta `original | commission | generated`. (b) El árbol de trabajo ya trae un comando `normalize` y `recipes/` en `tools/sprite-importer`. Esta aprobación no los cubre: pasan por spec y plan, y al estar dentro de ese paquete entran en su `tsc` y su Vitest de `just check`, así que la spec debe decidir su lugar frente a la restricción de que lo agregado "no entra en esos chequeos".

**Nota del autor (product-owner, 2026-10-30):** cambié "no entra en esos chequeos" por "no los hace depender de una herramienta", como pidió el Área 1 de la spec. Lo que protege la restricción es que `just check`, CI y el importador funcionen sin herramienta de autoría, servicio externo ni credenciales. `normalize` lo cumple: su `tsc`, ESLint y Vitest corren en `just check` como código puro, sin generador, y tiene su propio CLI (`src/normalize-cli.ts`), así que `import` y `check` no lo cargan. El alcance y los criterios de éxito no cambian. Las aprobaciones de architect y game-designer son anteriores a este cambio: lo ven al revisar la spec.
