---
name: write-adr
description: Write an Architecture Decision Record for Torneo Absurdo in docs/decisions/NNNN-slug.md. Use when a decision affects more than one feature (stack, security model, data model, process, legal/economy rules) or when superseding a previous ADR.
---

# Escribir un ADR

## Cuándo
Una decisión que trasciende a una feature o que alguien podría cuestionar en 6 meses: "¿por qué hicimos esto así?".

## Plantilla

```markdown
---
id: "NNNN"
title: <decisión en una línea>
type: adr
status: proposed
risk: low | medium | high
author: <agente u owner>
reviewers: [<agentes>, owner]
approvals: []
supersedes: null        # "NNNN" si reemplaza a otro
created: YYYY-MM-DD
updated: YYYY-MM-DD
---

# ADR-NNNN: <título>

## Contexto
<Fuerzas en juego: requisitos, restricciones, recursos.>

## Decisión
<Qué decidimos, en términos concretos y verificables.>

## Alternativas descartadas
- <opción>: <por qué no>

## Consecuencias
<Qué se vuelve fácil, qué se vuelve difícil y qué hay que vigilar.>
```

## Reglas
- Numeración consecutiva de 4 dígitos; nunca se reutiliza.
- Un ADR `approved` no se edita en lo sustancial: se reemplaza con uno nuevo (`supersedes`) y el viejo pasa a `superseded`.
- **Aceptar o reemplazar un ADR requiere el OK del owner** (gate humano).
