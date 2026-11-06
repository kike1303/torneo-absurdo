---
name: write-feature
description: Write the single feature.md (what, why, scope, minimal design, how it is tested) for a Torneo Absurdo feature in docs/features/NNN-slug/feature.md, then implement it and open the PR. Use when the user asks to build, add or design something non-trivial in this repo, or says "next feature" or "continúa con la feature NNN". Small fixes, content and UI tweaks skip this and go straight to a PR.
---

# Escribir y llevar una feature

Sigue los carriles de [docs/process.md](../../../docs/process.md). Si el cambio es del carril directo (fix, contenido, ajuste chico), no escribas documento: ve directo a la rama y al PR.

## 1. Preparar
- Toma el siguiente número libre en `docs/features/` (nunca se reutiliza) y crea `docs/features/NNN-slug/`.
- Crea o reusa el issue de Linear (equipo TA) y pásalo a *In Progress*. Rama `ta-<n>-<slug>`.
- Lee solo lo que necesitas: el código que vas a tocar, los ADRs que aplican y `docs/backlog.md` si la idea viene de ahí.
- Si hay decisiones de producto abiertas (alcance, economía, cómo se ve), pregúntale al owner en chat **antes** de diseñar. No las inventes.

## 2. Escribir `feature.md`
Corto: una o dos pantallas. Si pasa de ~150 líneas, parte la feature.

```markdown
---
id: "NNN"
title: <nombre corto>
type: feature
status: draft
risk: low | medium | high
author: <quien la escribe>
linear: TA-n
depends_on: []
created: YYYY-MM-DD
updated: YYYY-MM-DD
---

# <Nombre>

## Qué y por qué
<El problema y lo que cambia para el usuario, en 3 a 6 líneas.>

## Alcance
- Entra: ...
- No entra: ...

## Diseño
<Lo mínimo para implementarlo sin volver a preguntar: componentes que cambian, tipos de protocol, endpoints o mensajes, pantallas y estados. Decisiones con su porqué en una línea.>

## Fases
<Solo si hay más de un PR: una línea por fase.>

## Riesgos
<Qué se puede romper y cómo se evita. Obligatorio si risk: high.>

## Pruebas
- [ ] `just check` en verde
- [ ] <test concreto o verificación manual>
```

- `risk: high` (carril sensible) si toca auth, monedas o apuestas, la semilla, datos personales, pagos o temas legales. Entonces pide al `security-reviewer` un pase sobre el documento antes de implementar.
- Una decisión que afecta a más de una feature va como ADR (skill `write-adr`), no enterrada aquí.

## 3. Implementar
- Sigue los estándares de `CLAUDE.md`. Si el enfoque cambia, actualiza `feature.md` en el mismo PR.
- Antes de abrir el PR: `just check` en verde.

## 4. PR y cierre
- `gh pr create` con título `feat(scope): ... (TA-n)` y un cuerpo con el enlace al `feature.md`, qué cambió y la salida de `just check`. Linear: *In Review*.
- Un pase del agente `reviewer` (skill `review-pr`); en carril sensible también el `security-reviewer`. Corrige lo Important y re-revisa solo eso.
- Con veredictos en `APPROVE` y CI en verde: merge squash. `feature.md` pasa a `status: done` y Linear a *Done*.
