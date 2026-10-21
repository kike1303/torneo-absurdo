---
name: review-pr
description: Review a Torneo Absurdo pull request as a specific agent (qa-engineer, architect, security-reviewer, devops-engineer) using REVIEW.md, and post a signed comment with findings and verdict via gh. Use when a PR is open and needs that agent's review.
---

# Revisar un PR

## Procedimiento
1. `gh pr view <n> --json title,body,files,headRefName` y `gh pr diff <n>`.
2. Lee el `plan.md` y la `spec.md` enlazados, y [REVIEW.md](../../../REVIEW.md).
3. Aplica **solo los pases de tu rol**:
   - qa-engineer: Bugs, Cumplimiento
   - architect: Cumplimiento, Determinismo, límites entre módulos
   - security-reviewer: Seguridad
   - devops-engineer: infra, CI (incluye la estimación de minutos), Dockerfiles
   - frontend-engineer (si se le pide): i18n y UI
4. Cada hallazgo: `[Pase][Important|Nit] archivo:línea — problema — escenario concreto — sugerencia`. Como máximo 5 nits; el resto se resume como un conteo.
5. Veredicto: `APPROVE` (sin Important), `REQUEST_CHANGES` (al menos un Important) o `COMMENT`.

## Publicar
Un solo comentario por agente y ronda:

```bash
gh pr comment <n> --body-file /tmp/review-<agente>.md
```

Formato del cuerpo:

```markdown
**🤖 <agente>** · review de Torneo Absurdo

<resumen en 1–2 líneas>

### Hallazgos
- ...

### Verifiqué
- ...

Veredicto: APPROVE | REQUEST_CHANGES | COMMENT
```

No uses `gh pr review --approve`: el PR es de la cuenta del owner y GitHub no permite autoaprobar. El veredicto vale en el comentario.
