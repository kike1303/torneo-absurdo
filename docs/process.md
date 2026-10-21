---
title: Proceso de desarrollo (SDLC con agentes)
type: process
status: approved
author: owner
created: 2026-10-20
updated: 2026-10-20
---

# Proceso de desarrollo

Es una adaptación ligera del *AI-native SDLC playbook* para un proyecto personal. Cada etapa termina con un artefacto versionado en git, y la siguiente etapa empieza leyéndolo. Los roles los cubren **agentes especializados** ([.claude/agents/](../.claude/agents/)), y el owner (Jesús) supervisa y puede vetar en cualquier momento.

## Roles (agentes)

| Agente | Escribe | Aprueba / revisa |
|---|---|---|
| `product-owner` | intent.md | spec.md (que resuelva el problema) |
| `architect` | spec.md, ADRs técnicos | intent.md (viabilidad), plan.md, PRs (arquitectura) |
| `game-designer` | contenido (personajes, balance, narrador), intents de gameplay | specs de gameplay y economía |
| `backend-engineer` | plan.md e implementación en Rust | — |
| `frontend-engineer` | plan.md e implementación en web y renderer | — |
| `qa-engineer` | tests y verificación | PRs (pase de bugs y cumplimiento del plan) |
| `security-reviewer` | — | specs y PRs con `risk: high` (pase de seguridad) |
| `devops-engineer` | plan.md e implementación de infra/CI | PRs de infra |

**Reglas:**
1. **Quien escribe un artefacto nunca lo aprueba.**
2. Toda aprobación queda registrada en el frontmatter (`approvals`) y en la sección `## Revisión` del documento.
3. Un rechazo cambia el `status` a `changes-requested` con comentarios concretos; el autor corrige y se vuelve a pedir revisión.

## Pipeline

```
idea / bug / incidente
   │
   ▼  product-owner escribe            architect aprueba
intent.md ──────────────────────────────────────────▶ approved
   │
   ▼  architect escribe                product-owner aprueba (+ security si risk: high,
spec.md ────────────────────────────── + game-designer si toca gameplay o economía)
   │
   ▼  engineer escribe (plan mode)     architect aprueba
plan.md ────────────────────────────────────────────▶ approved
   │
   ▼  engineer implementa; qa-engineer verifica (just check + pruebas del plan)
PR ── qa-engineer + architect (+ security si risk: high) comentan firmados
   │
   ▼  merge  →  Linear: Done
```

Punto de entrada: skill **`feature-pipeline`**. Formato de cada artefacto: skills `write-intent`, `write-spec`, `write-plan`, `write-adr`. Cómo aprobar: `review-doc`. Cómo revisar PRs: `review-pr` y [REVIEW.md](../REVIEW.md).

## Gates humanos (owner)
Por defecto los agentes avanzan solos. El owner tiene que dar su OK explícito en chat cuando:
- El artefacto tiene `risk: high` (auth, economía/monedas, seguridad, datos personales, temas legales).
- Se va a desplegar a producción.
- Se va a aceptar o reemplazar un ADR.
- Se va a agregar una dependencia de pago o un servicio externo.

## Esquema de frontmatter

```yaml
---
id: "003"                 # número de la feature (o del ADR)
title: Motor de pelea v0
type: intent | spec | plan | adr | vision | architecture | process | glossary | character
status: draft | proposed | in-review | changes-requested | approved | rejected | superseded | done
                          # ADRs: proposed → approved | rejected | superseded
risk: low | medium | high
author: <agente u owner>
reviewers: [architect]     # quiénes deben aprobar
approvals:                 # se llena al aprobar
  - by: architect
    date: 2026-10-21
linear: TA-3               # issue de Linear (o null)
depends_on: ["001"]
created: 2026-10-20
updated: 2026-10-20
---
```

## Estructura de una feature
```
docs/features/NNN-slug/
  intent.md   # qué y por qué
  spec.md     # requisitos y diseño
  plan.md     # archivos, orden, riesgos y pruebas
```
Los números son consecutivos y nunca se reutilizan. Las decisiones transversales salen como ADR en `docs/decisions/NNNN-slug.md`.

## Público vs privado
- El repo es **público**: todo lo de `docs/`, `.claude/` y el código es visible.
- **Va en `private/`** (ignorado por git): modelo de negocio, monetización, precios, estrategia y cualquier cosa que no queramos mostrar. Si una feature es de negocio, su intent/spec/plan vive en `private/features/`.
- Ante la duda, va en `private/` y se pregunta al owner.

## Linear ↔ repo
- **El repo es la fuente de verdad** del *qué* y el *por qué* (intent, spec, plan, ADR).
- **Linear es la fuente de verdad** del *cuándo* y el *estado*.
- Enlace en los dos sentidos: el frontmatter lleva `linear: TA-n`, y el issue lleva la ruta del intent.
- Estados: intent en draft → *Backlog* · intent aprobado → *Todo* · plan aprobado → *In Progress* · PR abierto → *In Review* · merge → *Done*.
- Ramas `ta-<n>-<slug>` y commits con `(TA-n)` para que Linear los enlace solo.

## Reviews de PR
Los PRs se abren con la cuenta del owner. Cada agente revisor deja **un comentario firmado** (`**🤖 qa-engineer**` …) con su veredicto. GitHub no permite que el autor apruebe su propio PR, así que el veredicto vive en el comentario. El merge procede cuando todos los revisores requeridos dejaron `APPROVE` y CI está en verde.

## Mejora continua
- Si un agente comete el mismo error dos veces, la corrección se agrega a `CLAUDE.md` (sección "Cosas que Claude hace mal").
- Si un revisor detecta algo repetido, se ajusta `REVIEW.md` o el agente correspondiente.
