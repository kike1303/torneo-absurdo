---
id: "0011"
title: SDLC con agentes especializados
type: adr
status: approved
risk: medium
author: owner
approvals:
  - by: owner
    date: 2026-10-20
created: 2026-10-20
updated: 2026-10-20
---

# ADR-0011: SDLC con agentes especializados

## Decisión
- Adoptamos una versión ligera del *AI-native SDLC playbook*: intent → spec → plan → implementación → PR, con artefactos en Markdown y frontmatter dentro del repo.
- Los roles los cubren agentes definidos en `.claude/agents/` (product-owner, architect, game-designer, backend-engineer, frontend-engineer, qa-engineer, security-reviewer, devops-engineer), y los formatos se estandarizan con skills en `.claude/skills/`.
- Los agentes **escriben y aprueban** los artefactos, con separación de funciones: el autor nunca aprueba lo suyo.
- Las reviews de PR se publican como comentarios firmados por agente, desde la cuenta del owner.
- Hay gates humanos para lo de alto riesgo, producción y ADRs (ver docs/process.md).
- Linear lleva el estado; el repo lleva el contenido.

## Fuera del alcance por ahora
Evals en CI, monitoreo con bandas, managed settings y release manager. Se revisará cuando haya producción.

## Riesgo conocido
Que los agentes "se aprueben entre ellos" por inercia. Mitigaciones: cada revisor tiene su propio checklist, un rechazo debe señalar algo concreto, y el owner audita aprobaciones al azar.
