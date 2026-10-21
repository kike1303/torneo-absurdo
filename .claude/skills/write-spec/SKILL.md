---
name: write-spec
description: Write a spec.md (requirements + design) for a Torneo Absurdo feature from its approved intent.md, in the standard format. Use when an intent is approved and needs a requirements/design document in docs/features/NNN-slug/spec.md.
---

# Escribir un spec.md

La spec traduce el intent aprobado en **requisitos verificables y un diseño** que encaje en la arquitectura actual. Debe quedar lista para que un ingeniero escriba el plan sin volver a preguntar.

## Antes de escribir
- El intent debe estar `approved`. Lee el intent, `docs/architecture.md`, `CLAUDE.md` y los ADRs aprobados.
- Revisa el código existente que se tocará.

## Plantilla

```markdown
---
id: "NNN"
title: <nombre>
type: spec
status: draft
risk: <hereda del intent o sube>
author: architect
reviewers: [product-owner]   # + security-reviewer si risk: high, + game-designer si es gameplay/economía
approvals: []
linear: TA-n
depends_on: []
created: YYYY-MM-DD
updated: YYYY-MM-DD
---

# Spec: <nombre>

> Intent: [intent.md](./intent.md)

## Resumen
<2–4 líneas.>

## Requisitos funcionales
- RF1: ...

## Requisitos no funcionales
- Rendimiento, memoria (VPS de 1 vCPU / 4 GB), i18n, accesibilidad, seguridad.

## Diseño
### Componentes afectados
### Modelo de datos / tipos (protocol)
### API / mensajes WS
### Flujo (diagrama ASCII si ayuda)
### UI (si aplica): pantallas, estados (cargando, vacío, error), móvil y desktop

## Áreas de preocupación
- <Riesgos, conflictos entre ADRs o políticas, cosas que no se pudieron satisfacer.>

## Respuestas a preguntas abiertas del intent
- P: ... → R: ... (o "se arrastra a ...")

## Estrategia de pruebas
- <Qué se prueba y a qué nivel: unit, propiedad, integración, e2e.>

## Alternativas consideradas
```

## Reglas
- Cada RF se puede verificar.
- Si la spec obliga a una decisión transversal, propón un ADR (skill `write-adr`) en vez de enterrarla aquí.
- Al terminar: `status: in-review` y pedir revisores con `review-doc`.
