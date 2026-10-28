---
id: "007"
title: Estándar de assets para personajes propios
type: intent
status: approved
risk: medium
author: product-owner
reviewers: [architect, game-designer]
approvals:
  - by: architect
    date: 2026-10-27
  - by: game-designer
    date: 2026-10-27
linear: TA-22
depends_on: ["002", "003", "006"]
created: 2026-10-27
updated: 2026-10-27
---

# Intent: Estándar de assets para personajes propios

## Problema
El producto no usa nada de MUGEN. El convertidor `tools/sff-converter` (002) solo escribe en `content/_local/`, que está ignorado por git, y los únicos sprites commiteados son los placeholders generados `dummy_a` y `dummy_b`. El renderer ya define qué acepta (manifest `torneo.sprites` v1: nueve animaciones obligatorias, pivote en los pies, `impact_frame` en los ataques, windup ≤ 400 ms, personajes mirando a la derecha). Pero nada le dice a quien dibuja **cómo entregar** un personaje para que encaje sin retoques, y nada impide commitear un set sin saber de dónde salió.

Sin ese contrato, cada personaje propio (dibujado en casa o por encargo) se convierte a mano, con errores de pivote o de ritmo que solo se ven en pantalla. Además, un set sin procedencia en un repo público es el mismo riesgo que ADR-0004 quiere evitar. Para el espectador, un golpe que conecta antes o después de que se mueva la barra de vida le quita credibilidad a la pelea en la que apostó.

Hay además un dato duplicado: el tope de windup del manifest (`windupMaxMs: 400` en el renderer) copia a mano el `windup_strike_ms` del ruleset v1. Hoy coinciden, pero nada los ata. Si el ruleset cambia en una versión futura, los sets se validarían contra un número viejo sin que nadie se entere.

## Resultado propuesto
Existe un **estándar de assets** que cualquier pipeline de producción cumple, más las herramientas para comprobarlo:
1. **Guía de arte** en `docs/`: tamaño de frame, pivote en los pies, dirección (facing), acciones obligatorias y opcionales (las del manifest), frames mínimos y máximos por acción, frame de impacto, fps, paleta, contorno y formato de exportación.
2. **Formato de export abierto e importador**: 007 fija un formato de entrada abierto y documentado (imágenes más metadatos legibles), independiente de cualquier herramienta de autoría. El importador lo convierte en sheet + `manifest.json` (`format: torneo.sprites`), validado con el validador del renderer. La salida va a `content/` y se commitea. 007 no elige ni recomienda herramienta.
3. **Vínculo y procedencia**: `character.json` apunta a su set de sprites, y cada set lleva un archivo de procedencia y licencia (identificador público del autor, origen, fecha y bajo qué términos entra en `content/`).
4. **Validación en `just check`** de todo set commiteado: falla si falta la procedencia, si el manifest no pasa el validador del renderer o si el set no cumple la guía.

Frontera con 004: **007 define el contrato** (formato de export, guía, importador y validación). **004 elige cómo producir** el arte (herramienta, AI, esqueletal…) y exporta al estándar de 007. La enmienda del intent de 004 (`depends_on: ["002", "007"]` y "exporta al estándar de 007") se hace en su propia revisión, no desde aquí.

## Usuarios y sistemas afectados
- Usuarios: el owner, game-designer y artistas externos (quien produce sprites). El espectador, de forma indirecta: golpes legibles y sincronizados con lo que pasa en la pelea.
- Sistemas: `docs/` (guía), `content/characters/` (sets, vínculo, procedencia), `packages/renderer` (validador reutilizado y fuente única de los límites), `tools/` (importador), `crates/content` (vínculo en `character.json`) y `just check`.
- No toca: `crates/engine` ni `CharacterSpec`.

## Restricciones
- **ADR-0004 y CLAUDE.md:** nunca sprites de MUGEN ni assets de franquicias en git. El importador no lee SFF/AIR; eso sigue siendo del convertidor local de 002, que solo escribe en `content/_local/`.
- **Procedencia obligatoria:** ningún set entra en `content/` sin su archivo de procedencia y licencia. `content/` está bajo `LICENSE-CONTENT.md` (todos los derechos reservados), así que el registro tiene que dejar claro que el set puede publicarse bajo esos términos. Esta feature no redacta términos legales de encargos; si la spec lo intenta, el riesgo sube a `high` y pasa por el owner.
- **Sin datos personales en el repo:** la procedencia commiteada solo lleva un identificador público con consentimiento del autor (un alias o "encargo #N"). Los datos que lo identifiquen y la prueba de cesión de derechos van a `private/`, nunca a git.
- **Un solo validador:** se reutiliza `packages/renderer/src/manifest/validate.ts`. No se crea un validador paralelo del manifest; las reglas de la guía que el manifest no cubre se agregan como una capa encima, con códigos y sin texto (ADR-0007).
- **Una sola fuente por número:** los **tiempos** que vienen del ruleset o de la timeline (windup, recuperación) no se copian a mano en la guía ni en el renderer: se derivan de su fuente y una prueba los ata. Los rangos de frames por acción y los límites del manifest (`LIMITS`) se eligen a mano, pero viven en un solo lugar en código. La guía los cita o se genera a partir de él.
- **Independiente de la herramienta:** `just check` y el importador funcionan sin ninguna herramienta de autoría instalada. Solo leen el formato de export abierto y los archivos commiteados.
- **Vínculo solo en `crates/content`:** el campo que apunta al set en `character.json` lo maneja `crates/content` y nunca entra en `CharacterSpec` ni en el motor (ADR-0001: el motor no sabe de sprites).
- **Sin dependencias de pago ni servicios externos** (gate del owner en `docs/process.md`).
- **Formato de assets estable:** nada de animaciones obligatorias nuevas. Si hace falta una, entra como opcional con fallback, igual que en 005.
- **Mobile-first:** la guía fija tamaños que se lean bien en 375 px dentro de la escena lógica de 640×360 (RF-A15 de 002).

## Criterios de éxito
- [ ] La guía de arte existe en `docs/`, con frontmatter, y cubre todos los puntos del resultado propuesto. Cada tiempo cita el parámetro del ruleset o de la timeline del que sale, y cada rango o límite cita su fuente única en código.
- [ ] El formato de export abierto está documentado. Un export de prueba en ese formato pasa por el importador y produce un sheet + `manifest.json` que el validador del renderer acepta y que se reproduce en `/[lang]/dev/arena`.
- [ ] El importador rechaza, con códigos, un export que no cumple la guía (por ejemplo, falta una acción obligatoria o un ataque no tiene frame de impacto).
- [ ] `just check` falla si un set commiteado no tiene procedencia, si su manifest no pasa el validador o si no cumple la guía, y pasa con los sets válidos. Cada caso tiene un test.
- [ ] `character.json` puede apuntar a un set, `CharacterSpec` no cambia, y la carga del roster (`load_roster`) y `just sim` siguen pasando con y sin sprites.
- [ ] El tope de windup del manifest ya no es un número escrito a mano: si el `windup_strike_ms` del ruleset cambia, una prueba falla de forma explícita, nunca en silencio.
- [ ] Ningún archivo de procedencia commiteado contiene datos personales.
- [ ] game-designer aprueba en la spec los rangos de frames por acción y su relación con el ritmo.

## Fuera de alcance
- Producir personajes reales o el roster del MVP.
- Elegir o recomendar el pipeline o la herramienta de producción, y comparar costos: eso es 004.
- Arte final, estilo visual definitivo e identidad visual.
- Animación esqueletal y cualquier formato que no sea sheet + manifest.
- Cambiar el motor o el ruleset.
- Cómo llega el set de sprites al cliente (server, protocolo o build de la web).
- Cambios al convertidor de MUGEN de 002.

## Preguntas abiertas
- **¿Dónde vive el set?** `content/characters/<slug>/sprites/` (como anticipan el README de personajes y la nota 8 de la spec de 002) o una ruta propia de sprites que varios personajes compartan (por ejemplo, skins).
- **¿Tamaño de frame único o por personaje?** Un tamaño fijo simplifica la guía y el encuadre; uno por personaje deja que un ogro se vea más grande que una rata. ¿O un tamaño fijo con `scale` del manifest?
- **¿Formato de la procedencia?** Un archivo estructurado validable (alias, origen, fecha, licencia) o markdown con campos obligatorios. (La prueba de cesión ya está resuelta: va a `private/`.)
- **¿Se migran `dummy_a` y `dummy_b`?** Regenerarlos bajo el estándar los volvería ejemplo de referencia. Pero hoy son fixtures AGPL del renderer (nota 8 de 002), y moverlos a `content/` los pasaría a la licencia de contenido.
- **¿Cómo llega el ruleset al validador de sprites, y contra qué `ruleset_version` se valida un set?** El ruleset vive en Rust y el validador en TypeScript. El precedente directo es el de 006: `engine::TIMELINE_CONTRACT`, un JSON dorado (`tests/golden/timeline-contract.json`) y un Vitest que los ata. El tope de windup no es una regla de la `Timeline` sino un límite del manifest atado a `windup_strike_ms`, que es un campo versionado del `Ruleset`. La spec decide si un set declara su `ruleset_version`, si se valida contra todas las versiones vigentes o solo contra la actual, y qué pasa con los sets ya commiteados cuando sale una versión nueva.
- **¿Paleta y contorno son reglas que se validan** (máximo de colores, contorno obligatorio) **o solo recomendaciones** de la guía hasta que exista la identidad visual?

## Revisión
### architect — 2026-10-27 — REQUEST_CHANGES
- Verifiqué: viabilidad con el stack y presupuesto cero (importador y chequeo en Node dentro de `check-web`, `pngjs` ya está en el renderer, sin servicios externos ni carga en el VPS); `LIMITS` y `windupMaxMs: 400` en `packages/renderer/src/manifest/types.ts` y su uso en `validate.ts`; `windup_strike_ms: 400` en `crates/engine/src/ruleset.rs`; `CharacterFile` con `deny_unknown_fields` en `crates/content/src/file.rs` (un campo nuevo es viable porque `into_parts` lo separa de `CharacterSpec`); `content/characters/README.md` ya anticipa `sprites/`; `LICENSE-CONTENT.md` cubre sprites en `content/`; intent, spec y plan de 006 (rama `ta-21`, `approved`) e intent de 004. Frontmatter válido. No contradice ADR-0001, 0002, 0004, 0005 ni 0007. `risk: medium` es correcto mientras se cumpla el punto 3; la cláusula que lo sube a `high` si la spec redacta términos legales está bien.
- Cambios pedidos:
  1. **Premisa de 006 inexacta.** La spec aprobada de 006 dice que los windups (400/700/1100) y `recovery_ms` **no cambian**. Reescribir el último párrafo de Problema: el problema es la duplicación (400 a mano en el renderer), no un cambio inminente. `depends_on: "006"` puede quedarse, pero por otra razón: 006 introduce `engine::TIMELINE_CONTRACT` + `tests/golden/timeline-contract.json` + un Vitest de atadura, que es el precedente directo para "¿cómo llega el ruleset a la validación de sprites?". Citarlo en esa pregunta abierta. Además, el tope de windup del manifest no es una regla de la `Timeline` sino un límite del manifest atado a `windup_strike_ms`, que es campo del `Ruleset` y está versionado: la pregunta debe incluir contra qué `ruleset_version` se valida un set.
  2. **Contradicción interna.** Restricciones dice "ningún número de frames ni de milisegundos se fija a mano en la guía", pero Resultado pide "frames mínimos y máximos por acción" y un criterio pide que game-designer apruebe esos rangos, que son cifras elegidas a mano. Acotar la regla: los **tiempos** derivados del ruleset o de la timeline no se copian; los rangos de frames y los límites del manifest (`LIMITS`) tienen una sola fuente en código y la guía los cita o se genera desde ella.
  3. **Datos personales en un repo público.** El archivo de procedencia pide "autor" de artistas externos. CLAUDE.md prohíbe datos personales en el repo. Añadir como restricción que la procedencia commiteada solo lleva un identificador público con consentimiento (alias o "encargo #N") y que cualquier dato identificable y la prueba de cesión van a `private/`. Con eso la pregunta abierta sobre la cesión queda respondida y el riesgo sigue en `medium`.
  4. **Frontera con 004.** "Un tag por acción" y "la herramienta recomendada" eligen herramienta, lo que es terreno de 004 (opción Aseprite/Lua). Dejar explícito que 007 define un **formato de export abierto** independiente de la herramienta y no recomienda ninguna, y que el intent de 004 se enmienda (`depends_on: ["002", "007"]` y "exporta al estándar de 007") en su propia revisión, no desde aquí.
  5. **Límite de módulo del vínculo.** Agregar en Restricciones que el vínculo a sprites en `character.json` es solo de `crates/content` y nunca entra en `CharacterSpec` ni en el motor (ADR-0001), y que cómo llega el set al cliente (server o protocolo) queda fuera de alcance.

**Respuesta del autor (product-owner, 2026-10-27):** (1) el Problema ya no dice que 006 cambie tiempos, sino que el 400 está duplicado a mano en el renderer; la pregunta sobre el ruleset cita `TIMELINE_CONTRACT` + JSON dorado + Vitest de 006 y suma contra qué `ruleset_version` se valida un set; mantengo `"006"` en `depends_on` como precedente técnico de ese camino Rust→TS, no por los tiempos. (2) La regla "no a mano" queda solo para tiempos; rangos de frames y `LIMITS` con una sola fuente en código que la guía cita o de la que se genera. (3) Restricción nueva: en el repo solo alias o "encargo #N" con consentimiento; datos identificables y prueba de cesión en `private/`; la pregunta de la cesión queda resuelta y hay un criterio de éxito nuevo. (4) Quité "tag por acción" y "herramienta recomendada"; 007 fija un formato de export abierto, no recomienda herramienta, y la enmienda de 004 se hace en su propia revisión. (5) Restricción nueva: el vínculo vive solo en `crates/content` y no entra en `CharacterSpec` ni en el motor; la entrega del set al cliente pasa a Fuera de alcance.

### architect — 2026-10-27 — APPROVE
- Verifiqué los 5 puntos contra el cuerpo actual: (1) Problema habla solo de la duplicación del 400 y la pregunta del ruleset cita el precedente de 006 e incluye `ruleset_version`; (2) "no a mano" acotado a tiempos, con rangos y `LIMITS` de fuente única; (3) restricción y criterio sin datos personales, con la cesión en `private/`; (4) formato de export abierto sin herramienta recomendada y la enmienda de 004 delegada a su revisión; (5) vínculo solo en `crates/content`, `CharacterSpec` intacto (también en criterios) y la entrega al cliente fuera de alcance. Sin cambios de alcance ni ADRs contradichos; `risk: medium` se mantiene.
- Queda `in-review` hasta que apruebe game-designer.

### game-designer — 2026-10-27 — APPROVE
- Verifiqué: `Ruleset::v1()` en `crates/engine/src/ruleset.rs` (windups 400/700/1100, `recovery_ms` 300, `knockdown_ms` 1500); `LIMITS`, `REQUIRED_ANIMATIONS`, `ATTACK_ANIMATIONS` y fallbacks en `packages/renderer/src/manifest/types.ts`; el cálculo de windup hasta `impact_frame` en `validate.ts`; la coreografía y la precedencia de 002 (impacto en `at_ms`, windup hacia atrás, combo a 600 ms, recuperación interrumpible); la spec de 006 (windups y `recovery_ms` sin cambios, `strike_gap_min()` = 700, respiros de 2.5 a 4.6 s, `TIMELINE_CONTRACT`). Las nueve acciones obligatorias más los tres opcionales con fallback alcanzan para una pelea legible y no suben el costo por personaje; "nada obligatorio nuevo" protege a 004. El frame de impacto queda cubierto por la guía, el importador (rechazo con códigos) y `just check`. La frontera con 004 sirve para producir contenido: 007 fija qué se entrega y 004 cómo se dibuja, y el criterio de que yo apruebe los rangos de frames en la spec me deja controlar el costo y el ritmo. Sin choques con ADR-0001, 0004 ni 0005; `risk: medium` es correcto.
- Para la spec (no bloquean el intent):
  1. **Windup por golpe.** Hoy el manifest topa los tres ataques en 400 ms (`windup_strike_ms`), aunque el motor da 700 y 1100 a `attack_heavy` y `special`. La spec decide, con números, si cada ataque se ata a su `windup_*` (un pesado que se ve venir es más legible y la separación del motor lo permite) o si los tres quedan en el de strike. En ese caso, debe justificarlo con el look-ahead del vivo (nota 2 de 002) y con el margen de "windup del segundo ≥ 200 ms después del impacto del primero", que depende de `strike_gap_ms` y no solo de `windup_strike_ms`. Que la atadura sea a la restricción real, no solo una copia del campo.
  2. **Recuperación y reacciones.** La guía tiene que relacionar los frames después de `impact_frame`, y la duración de `hit`, `block` y `dodge`, con `recovery_ms` y `strike_gap_ms`, porque en un combo a 600–700 ms se cortan. Lo que importa tiene que leerse antes del corte.
  3. **`idle` en los respiros de 006.** Con pausas de hasta ~4.6 s solo con `idle`, la guía debería pedir un loop que aguante esa duración sin verse congelado (frames mínimos o duración mínima del ciclo).

