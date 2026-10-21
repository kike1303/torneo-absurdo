---
name: write-intent
description: Write or update an intent.md for a Torneo Absurdo feature in the standard format. Use whenever drafting the "what and why" of a new feature, bug or idea in docs/features/NNN-slug/intent.md.
---

# Escribir un intent.md

Un intent dice **qué** se quiere y **por qué**, nunca el **cómo**. Tiene que caber en una pantalla y media. Escríbelo en español, con los términos del glosario (`docs/glossary.md`).

## Antes de escribir
- Lee `docs/vision.md` y los ADRs relevantes.
- Si falta información esencial (usuarios, éxito, restricciones), pregúntala. Lo que no se pueda resolver va en "Preguntas abiertas".

## Plantilla

```markdown
---
id: "NNN"
title: <nombre corto>
type: intent
status: draft
risk: low | medium | high
author: <agente>
reviewers: [architect]
approvals: []
linear: null
depends_on: []
created: YYYY-MM-DD
updated: YYYY-MM-DD
---

# Intent: <nombre>

## Problema
<Qué duele hoy o qué falta, y para quién. Con evidencia si existe.>

## Resultado propuesto
<Qué cambia para el usuario u owner cuando esto exista. Observable.>

## Usuarios y sistemas afectados
- Usuarios: ...
- Sistemas: apps/web, apps/server, crates/engine, ...

## Restricciones
- <ADRs que aplican, presupuesto, VPS, i18n, mobile-first, legal...>

## Criterios de éxito
- [ ] <verificable>

## Fuera de alcance
- ...

## Preguntas abiertas
- ...
```

## Reglas
- `risk: high` si toca auth, monedas o apuestas, la semilla, datos personales o temas legales.
- Nada de detalles de implementación (nombres de tablas, librerías); eso va en la spec.
- Al terminar: `status: in-review` y pedir la revisión con el skill `review-doc`.
