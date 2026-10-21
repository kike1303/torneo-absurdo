---
name: review-doc
description: Review and approve or request changes on a Torneo Absurdo doc artifact (intent.md, spec.md, plan.md, ADR, character) as a specific agent, recording the verdict in frontmatter and a Revisión section. Use whenever an artifact is in-review and an agent must approve it.
---

# Revisar un documento

Revisas **como un agente concreto** (indícalo). Nunca revises algo cuyo `author` seas tú.

## Procedimiento
1. Lee el documento, sus antecedentes (intent → spec → plan) y los ADRs que cita.
2. Aplica el checklist de tu rol (en tu definición de agente) y además el checklist general:
   - ¿El frontmatter está completo y es válido según `docs/process.md`?
   - ¿Usa los términos del glosario?
   - ¿Contradice algún ADR `approved`?
   - ¿Cada requisito o criterio se puede verificar?
   - ¿Las preguntas abiertas están respondidas o arrastradas explícitamente?
3. Decide: **APPROVE** o **REQUEST_CHANGES**. Para pedir cambios necesitas al menos un punto concreto y accionable. Para aprobar necesitas haber revisado de verdad: menciona qué verificaste.

## Registrar el veredicto
- Agrega o actualiza al final del documento:

```markdown
## Revisión
### <agente> — YYYY-MM-DD — APPROVE | REQUEST_CHANGES
- Verifiqué: ...
- Cambios pedidos: ... (si aplica)
```

- Si apruebas, agrega `{by: <agente>, date: YYYY-MM-DD}` a `approvals`.
- Cuando **todos** los `reviewers` aprobaron, el `status` pasa a `approved`. Si alguno pide cambios, pasa a `changes-requested`.
- Actualiza `updated`.
