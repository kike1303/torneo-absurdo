---
id: "0013"
title: Pipeline de sprites híbrido (AI + receta + importador) y cómo se declara un set hecho con AI
type: adr
status: proposed
risk: high
author: architect
reviewers: [game-designer, owner]
approvals:
  - {by: game-designer, date: 2026-10-30}
supersedes: null
linear: TA-4
depends_on: ["0004"]
created: 2026-10-30
updated: 2026-10-30
---

# ADR-0013: Pipeline de sprites híbrido (AI + receta + importador) y cómo se declara un set hecho con AI

## Contexto
ADR-0004 obliga a publicar arte original y el owner no es artista. La feature 007 fijó el estándar y el formato abierto `torneo.sprites.export` v1, sin elegir herramienta, y dejó la procedencia con `origin` (`original`, `commission`, `generated`) más `ai_assisted`. La feature 004 evaluó cinco opciones con `ogro_del_pantano` como prueba. Los datos, con lo medido y lo estimado marcado, están en la [sección Evaluación de la spec de 004](../features/004-pipeline-de-assets/spec.md#evaluación). Este ADR no es asesoría legal.

## Decisión
- **D1. Pipeline.** Híbrido: un generador de AI produce frames crudos, una receta los ajusta y `tools/sprite-importer` hace de único juez del estándar. Pasos en la [receta del pipeline](../guides/pipeline-de-sprites.md); ejemplo real en [`ogro_del_pantano.json`](../../tools/sprite-importer/recipes/ogro_del_pantano.json).
- **D2. Generador recomendado: PixelLab.** Es una recomendación, no una dependencia: la salida obligatoria es `torneo.sprites.export`. Cambiar de generador solo exige actualizar la guía y el OK del owner. El uso programático va por su vía oficial (§2.2 de sus [términos](https://pixellab.ai/termsofservice)).
- **D3. `normalize` es opcional y agnóstico.** Solo conoce archivos PNG y la receta. `import` y `check` no lo cargan. Un export hecho con otra herramienta entra directo por `run import`.
- **D4. Un set con AI declara `origin: "original"` y `ai_assisted: true`, sin cambiar el enum de 007.** `author` es `owner`, si es quien dirige y responde por el set.
  - `generated` significa "lo genera código de este repo" y `commission` un encargo a una persona que cede derechos; un servicio de AI de terceros no es ninguna de las dos cosas. `original` es el valor que dice quién dirige el set y responde por él.
  - `ai_assisted` existe justo para separar estos sets y permitir revisarlos o retirarlos. Un valor nuevo obligaría a enmendar 007 (código, guía y `check`) sin aportar información.
  - **Límite:** `original` no afirma autoría humana ni que exista copyright, y `ai_assisted: true` no distingue un set retocado de uno solo generado; eso consta en `private/provenance/<id>.md`.
  - **Se pide al owner que confirme esta lectura de forma expresa** al aceptar este ADR.
- **D5. Lo que no va al repo.** Prompts, referencias y prueba de derechos van en `private/provenance/<id>.md`, junto con una copia fechada de los términos vigentes al generar. Los crudos y el export se quedan en `content/_local/raw/<id>/` y `content/_local/export/<id>/`, con copia de resguardo fuera de un worktree desechable. Al repo van solo el set y la receta, que tiene números, modos e identificadores cortos, sin textos libres.
- **D6. Puerta de publicación.** Un set con AI se commitea solo con el OK expreso del owner sobre la licencia, además de su visto bueno visual y el checklist de `write-character`. Sin eso, se queda en `content/_local/`.
  - **Regla de ADR-0004 en las entradas, para cualquier generador:** ningún prompt nombra un personaje o franquicia, y ningún material protegido sirve de referencia, img2img o calco, ni siquiera en local.
  - **Comprobación:** quien aprueba el checklist revisa `private/provenance/<id>.md` y lo confirma. Si los prompts no se pueden reconstruir, el set se regenera o se queda en `content/_local/`.
- **D7. Esqueletal descartado para esta fase.** Ver abajo.

## Alternativas descartadas
- **AI sola, sin receta:** su salida cruda no cumple el estándar (lienzos mezclados, sin celda de 192 y 237 colores).
- **Retro Diffusion como recomendado:** no medido, porque exige cuenta y un servicio externo pasa por el owner. Lo generado con uso gratuito pueden mostrarlo, y entrenan con los outputs ([términos](https://www.retrodiffusion.ai/terms)). Sigue siendo un sustituto posible por D2.
- **Aseprite o LibreSprite con scripting:** automatizan el armado pero no producen arte. Quien dibuja es una persona, y el estimado para un no artista es de 12–24 h por personaje.
- **Esqueletal (Spine, Rive, DragonBones):** un sprite plano no trae lo que hay detrás de una pieza, así que habría que dibujar piezas, que es justo lo que el owner no puede hacer. 007 solo admite frames, de modo que además tendría que hornear. Se puede reabrir si aparecen piezas hechas por una persona.
- **Valor nuevo de `origin` (por ejemplo, `ai`):** ver D4.
- **`origin: "generated"`:** rompería su definición y haría creer que el set se puede regenerar desde el repo.

## Consecuencias
**Se vuelve fácil:** un personaje completo cuesta unos 30 min de agente y 12 generaciones, sin saber dibujar y sin contar aún la regeneración pendiente de `attack_heavy`, y el generador se puede cambiar sin tocar código.

**Riesgo legal, que el owner acepta o no al aprobar:**
- **El trial no está nombrado.** Los [términos de PixelLab](https://pixellab.ai/termsofservice) (§1.3) dan propiedad y uso comercial sin condición de plan, pero ninguna cláusula nombra el trial, con el que se hizo la prueba. Tampoco se dice qué pasa al cancelar. Lo sólido es una confirmación escrita de PixelLab.
- **Los términos cambian.** Los leídos llevan fecha del mismo 2026-10-30 y no se compararon con versiones anteriores. Por eso se guarda una copia fechada (D5).
- **El usuario responde.** Por no infringir derechos (§3.3), con un servicio sin garantías (§5.1). El checklist de `write-character` mitiga, pero no elimina, que lo generado se parezca a algo protegido. También se prohíbe entrenar modelos con lo generado (§1.2).
- **"Todos los derechos reservados" puede no ser exigible.** [`LICENSE-CONTENT.md`](../../LICENSE-CONTENT.md) lo declara sobre todo `content/`, pero un contrato que dice "es tuyo" no crea copyright frente a terceros. Según el [informe de la USCO, Part 2](https://www.copyright.gov/ai/Copyright-and-Artificial-Intelligence-Part-2-Copyrightability-Report.pdf) (enero de 2025), el prompt solo no basta. Selección, ajuste y retoque tienen mejor base, en una decisión caso por caso y solo para EE. UU.; otras jurisdicciones no se revisaron. Un sprite solo generado podría copiarse sin que podamos impedirlo. El nombre, el lore y la marca no dependen de esto.

**Calidad, que hay que vigilar:**
- **Inconsistencia de cuerpo.** Las animaciones `v3` salen más robustas que las de plantilla (19–25 % de diferencia de histograma, con paleta compartida). La receta reduce el salto de pose, no este. Si el owner no lo acepta, hace falta retoque a mano o regenerar, y el tiempo medido sube.
- **El lienzo crudo puede cortar el golpe, y el importador no lo ve.** En el piloto, el puño del frame de impacto de `attack_heavy` queda cortado por el lienzo de 64 px y el golpe se parece al ligero (game-designer, 2026-10-30), aunque `import` no da ningún código. La revisión humana sigue siendo obligatoria. Se corrige regenerando, con el OK del owner.
- **Probado con un solo personaje.** No demuestra que el estilo se sostenga entre personajes. El estilo final sigue pendiente de la identidad visual.

**Costos a revisar:** el precio de los planes de PixelLab está sin confirmar en su página de precios, y sostener un roster con el bono del trial no está demostrado.

## Revisión
### game-designer — 2026-10-30 — REQUEST_CHANGES
- Verifiqué: D4 contra RF-B4 de 007 (`origin`, `ai_assisted` y el patrón de `author`, que admite `owner`), y es la lectura que propuse en el intent, con su límite bien dicho; D3 contra `normalize-cli.ts` y `src/cli.ts`; D5 contra el esquema de receta y `.gitignore`; las alternativas contra la tabla de la spec; el riesgo de `attack_heavy` contra los crudos. Sin choque con ADR-0001, ADR-0005 ni ADR-0010: no toca motor, monedas ni narrador. No releí los términos de licencia: esa aceptación es del owner. El `status` sigue `proposed`: aceptar este ADR es del owner.
- Cambios pedidos:
  1. **ADR-0004 en las entradas, como decisión.** D5 dice dónde se guardan los prompts, pero ninguna decisión prohíbe nombrar personajes o franquicias en ellos ni usar material protegido como referencia, img2img o calco. Hoy eso solo vive en la guía, que D2 permite cambiar al cambiar de generador, y el checklist mitiga solo sobre el resultado. Agregar la regla a D6, con su comprobación: quien aprueba el checklist revisa `private/provenance/<id>.md`.
- No bloquea: `depends_on` podría sumar `"007"`, porque D4 interpreta su enum.

**Respuesta del autor (architect, 2026-10-30):**
- (1) D6 lleva la regla de ADR-0004 sobre las entradas y su comprobación.
- No bloqueante: no sumo `"007"` a `depends_on`, porque ese campo lleva números de ADR y el Contexto ya cita 007.

### game-designer — 2026-10-30 — APPROVE (re-revisión)
- Verifiqué: D6 lleva la regla de ADR-0004 sobre las entradas, para cualquier generador, y su comprobación, con la misma salida que RF6 de la spec y el paso 6 de la guía. Acepto dejar `depends_on` como está.
- El `status` sigue `proposed`: aceptar este ADR es del owner.
