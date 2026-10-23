---
name: feature-pipeline
description: Drive a Torneo Absurdo feature, bug or idea through the full agent pipeline (intent → spec → plan → implementation → PR review → merge) with the right agent at each step and Linear kept in sync. Use when the user asks to build, add, fix or design something non-trivial in this repo, or says "next feature", "pipeline", "continúa con la feature NNN".
---

# Feature pipeline

Orquestas el pipeline de [docs/process.md](../../../docs/process.md). Tú (la sesión principal) invocas a cada agente con la herramienta Agent (`subagent_type` = nombre del agente), le pasas la ruta de los artefactos y registras el resultado.

## 0. Ubicar o crear la feature
- Si existe `docs/features/NNN-*/`, lee los `status` de su frontmatter y retoma en la etapa pendiente.
- Si no existe, toma el siguiente número libre y crea `docs/features/NNN-slug/`.

## 1. Intent
- `product-owner` lo escribe (skill `write-intent`). Para gameplay o economía, el que lo escribe es `game-designer`.
- `architect` lo revisa (skill `review-doc`).
- **Linear**: crea o actualiza el issue en el equipo de Torneo Absurdo (título = título del intent, descripción con la ruta del archivo y el problema resumido) y escribe `linear: TA-n` en el frontmatter. Si queda aprobado, pasa el issue a *Todo*.

## 2. Spec
- `architect` la escribe (skill `write-spec`).
- Revisores: `product-owner` siempre, `security-reviewer` si `risk: high`, `game-designer` si toca gameplay o economía.
- Si `risk: high`, **detente y pide el OK del owner** en chat, con un resumen de 5 líneas y las áreas de preocupación.

## 3. Plan
- El ingeniero dueño de la feature (`backend-engineer`, `frontend-engineer` o `devops-engineer`) lo escribe (skill `write-plan`) sin editar código.
- `architect` lo revisa.
- **Linear**: el issue pasa a *In Progress*. Si el plan tiene pasos independientes que tocan archivos distintos, se pueden crear sub-issues.

## 4. Implementación
- Rama `ta-<n>-<slug>`. El ingeniero implementa según el plan; si se desvía, actualiza `plan.md` (o `plan-<fase>.md` si la spec define fases; en ese caso las etapas 3 a 5 se repiten por fase).
- `qa-engineer` verifica (`just check` más las pruebas del plan). Si falla, regresa al ingeniero con el reporte.

## 5. PR
- Abre el PR con `gh pr create`: título `feat(scope): ... (TA-n)` y un cuerpo con enlaces a intent, spec y plan y la salida de `just check`.
- Revisores con el skill `review-pr`: `qa-engineer` y `architect` siempre, `security-reviewer` si `risk: high`, `devops-engineer` si toca `infra/` o `.github/`.
- Cada uno publica su comentario firmado. Si alguno pide cambios, el ingeniero corrige y se vuelve a revisar solo lo afectado.
- **Linear**: *In Review*.

## 6. Cierre
- Con todos en `APPROVE` y CI en verde: haz merge (squash). Si `risk: high` o hay deploy a producción, pide antes el OK del owner.
- Marca intent, spec y plan con `status: done`. **Linear**: *Done*.
- Si salió una lección repetida, agrégala a `CLAUDE.md` o `REVIEW.md`.

## Reporte al owner
Al terminar cada etapa, da una línea: etapa, artefacto, veredicto, siguiente paso.
