---
name: review-pr
description: Review a Torneo Absurdo pull request as the reviewer or security-reviewer agent using REVIEW.md, and post one signed comment with findings and verdict via gh. Use when a PR is open and needs its review pass.
---

# Revisar un PR

## Procedimiento
1. `gh pr view <n> --json title,body,files,headRefName` y `gh pr diff <n>`.
2. Lee el `feature.md` enlazado (si hay) y [REVIEW.md](../../../REVIEW.md).
3. Aplica tu rol: `reviewer` hace todos los pases salvo Seguridad; `security-reviewer`, solo Seguridad.
4. Cada hallazgo: `[Pase][Important|Nit] archivo:línea — problema — escenario concreto — sugerencia`. Como máximo 5 nits; el resto se resume como un conteo.
5. Veredicto: `APPROVE` (sin Important), `REQUEST_CHANGES` (al menos un Important) o `COMMENT`.

En una segunda ronda revisa solo lo que cambió para atender tus hallazgos.

## Publicar
Un solo comentario por agente y ronda:

```bash
gh pr comment <n> --body-file <archivo temporal>
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
