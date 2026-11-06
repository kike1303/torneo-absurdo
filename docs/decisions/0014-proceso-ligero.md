---
id: "0014"
title: Proceso ligero por carriles
type: adr
status: approved
risk: low
author: owner
supersedes: "0011"
linear: TA-49
created: 2026-11-04
updated: 2026-11-04
---

# ADR-0014: Proceso ligero por carriles

## Contexto
La ADR-0011 adoptó un pipeline intent → spec → plan → implementación → PR con ocho agentes, aprobación cruzada de cada documento y varias revisiones firmadas por PR. Sirvió para asentar las fundaciones, pero el costo resultó desproporcionado para un juego hecho por una persona. Diez features produjeron unas 15 500 líneas de documentos; solo el scheduler (010) sumó unas 2 200 entre intent, spec y cuatro planes. Cada feature tardaba días en ciclos de revisión, y el gasto de tokens crecía con cada agente que arrancaba en frío y releía el mismo contexto.

Lo que de verdad protege al proyecto ya es automático o muy acotado: `just check` y CI, los tests de determinismo y de conservación de monedas, el congelamiento del ruleset y las reglas duras de `CLAUDE.md`.

## Decisión
- Hay **tres carriles** según tamaño y riesgo: directo (sin documento), feature (un `feature.md` corto) y sensible (`feature.md` más revisión de seguridad). Detalle en `docs/process.md`.
- Un solo documento por feature, `feature.md`, en lugar de intent, spec y plan.
- Quedan tres agentes: `reviewer` (un pase por PR que junta lo que hacían architect, qa y devops), `security-reviewer` (solo en el carril sensible) y `game-designer` (contenido y balance). Se retiran product-owner, architect, qa-engineer, backend-engineer, frontend-engineer y devops-engineer.
- La implementación la hace la sesión principal con los estándares de `CLAUDE.md`.
- Se acaba la regla "quien escribe nunca aprueba" como trámite: las decisiones de producto las toma el owner en chat y la revisión independiente queda en el PR.
- Las ideas futuras viven en `docs/backlog.md`.

## Alternativas descartadas
- **Mantener el pipeline y solo saltar etapas en cambios chicos**: ya se hacía de facto y no bajó el costo de las features medianas, que son la mayoría.
- **Quitar toda revisión de agente**: el pase sobre el PR encontró bugs reales (por ejemplo en `check-workflows`), y en auth, monedas y semilla un error cuesta caro.

## Consecuencias
- Las features medianas pasan de días a horas, con muchos menos tokens.
- Se pierde parte de la trazabilidad de las aprobaciones; el registro queda en el PR y en el `feature.md`.
- Hay que vigilar que lo sensible no se cuele por el carril directo. Si pasa, se endurece de nuevo solo esa parte.
