---
name: write-plan
description: Write a plan.md (files that change, order of work, risks, proof) for a Torneo Absurdo feature from its approved spec, working read-only before any code edit. Use when a spec is approved and implementation needs a concrete plan in docs/features/NNN-slug/plan.md (or plan-<phase>.md when the spec defines phases).
---

# Escribir un plan.md

El plan se escribe **antes de editar código**, en modo solo lectura. Si la spec define fases, escribe un plan por fase en `plan-<fase>.md` (ver `docs/process.md`). La prueba de calidad: alguien que nunca vio la conversación tiene que poder implementarlo solo con el plan.

## Antes de escribir
- La spec debe estar `approved`. Lee intent, spec y el código actual de los archivos que se tocarán.
- Pregúntate: ¿qué se puede romper? ¿Cuál es el paso más riesgoso? ¿Qué alternativas descarto y por qué?

## Plantilla

```markdown
---
id: "NNN"
title: <nombre>
type: plan
status: draft
risk: <hereda>
author: <backend-engineer | frontend-engineer | devops-engineer>
reviewers: [architect]
approvals: []
linear: TA-n
created: YYYY-MM-DD
updated: YYYY-MM-DD
---

# Plan: <nombre> (desde spec.md YYYY-MM-DD)

> [intent](./intent.md) · [spec](./spec.md)

## Archivos que cambian
| Archivo | Cambio |
|---|---|
| `crates/engine/src/...` | nuevo / modificado: ... |

## Orden de trabajo
1. ...
2. ...

## Riesgos
- <riesgo> → <mitigación>

## Alternativas descartadas
- ...

## Pruebas (cómo se demuestra que funciona)
- [ ] `just check` en verde
- [ ] <test concreto> cubre RF1
- [ ] <captura móvil y desktop, ambos idiomas> (si es UI)

## Estimación de CI
<Minutos aproximados por corrida si se agregan o cambian workflows; si no, "sin cambios".>

## Desviaciones durante la implementación
_(Se llena si la implementación se aparta del plan.)_
```

## Reglas
- Los pasos que tocan archivos distintos y no dependen entre sí se marcan como **paralelizables**.
- Si durante la implementación se cambia el enfoque, se actualiza este archivo en el mismo commit.
