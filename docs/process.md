---
title: Proceso de desarrollo
type: process
status: approved
author: owner
created: 2026-10-20
updated: 2026-11-04
---

# Proceso de desarrollo

Es un juego hecho por una persona con ayuda de agentes, no un banco. El proceso existe para no romper lo que importa (el motor determinista, la semilla, las monedas y la auth), no para generar papeles. Ante la duda, se elige el carril más ligero y se sube solo si aparece riesgo. Decisión de fondo: [ADR-0014](decisions/0014-proceso-ligero.md), que reemplaza a la ADR-0011.

## Tres carriles

| Carril | Cuándo | Documento | Revisión |
|---|---|---|---|
| **Directo** | Fixes, contenido, ajustes de UI, refactors chicos, docs, tooling | Ninguno: basta la descripción del PR | CI en verde. Revisión de agente opcional |
| **Feature** | Algo nuevo de varios archivos o con decisiones de diseño | Un `feature.md` corto | Un pase del agente `reviewer` sobre el PR |
| **Sensible** | Auth, sesiones, monedas o ledger, apuestas, semilla, datos personales, pagos, legal | Un `feature.md` con la sección de riesgos completa | `security-reviewer` sobre el doc y el PR, más el pase de `reviewer` |

Quien decide el carril es la sesión que hace el trabajo; si se equivoca hacia lo ligero y aparece riesgo, sube de carril.

## El `feature.md`

Un solo archivo en `docs/features/NNN-slug/feature.md` que reemplaza a intent, spec y plan. Formato en el skill `write-feature`. Reglas:
- **Corto**: apunta a una o dos pantallas. Si pasa de ~150 líneas, la feature es demasiado grande y se parte.
- Dice qué y por qué, el alcance, el diseño mínimo para que otra sesión lo implemente y cómo se prueba.
- Lo que es decisión de producto se le pregunta al owner en chat. No hay ciclos de aprobación entre agentes.
- Si cambia el enfoque al implementar, se actualiza en el mismo PR. No hay planes por fase: si hay fases, se listan dentro del mismo archivo y cada una es un PR.

Las features 001 a 010 conservan sus intent, spec y plan como registro histórico.

## Agentes

| Agente | Para qué |
|---|---|
| `reviewer` | Un pase por PR: bugs, determinismo, arquitectura, cumplimiento del `feature.md`, i18n e infra. Reporta; no arregla |
| `security-reviewer` | Solo el carril sensible: semilla, monedas, auth, entrada, secretos y datos personales |
| `game-designer` | Personajes, balance, economía y narrador (skill `write-character`) |

La implementación la hace la sesión principal, siguiendo los estándares de `CLAUDE.md`. Los subagentes se usan para trabajo de verdad paralelo o para una revisión con ojos frescos, no por costumbre.

## Gates humanos (owner)
El owner da su OK explícito en chat para:
- Desplegar a producción o crear un tag `v*`.
- Aceptar o reemplazar un ADR.
- Agregar una dependencia de pago o un servicio externo.
- Decisiones de producto de verdad (alcance, economía, monetización).

Todo lo demás avanza solo y se reporta.

## Esquema de frontmatter

```yaml
---
id: "011"                 # número de la feature (o del ADR); en una guía, su slug
title: Renderer 3D
type: feature | adr | vision | architecture | process | glossary | character | legal | guide | backlog
                          # históricos: intent | spec | plan
status: draft | approved | done | superseded
                          # ADRs: proposed → approved | rejected | superseded
risk: low | medium | high # high = carril sensible
author: <agente u owner>
linear: TA-n              # issue de Linear (o null)
depends_on: []
created: 2026-11-04
updated: 2026-11-04
---
```

Los campos `reviewers` y `approvals` ya no se usan; los docs viejos los conservan.

Los números de feature son consecutivos y nunca se reutilizan. Las decisiones transversales van como ADR en `docs/decisions/NNNN-slug.md` (skill `write-adr`). Las guías transversales viven en `docs/guides/<slug>.md`, con `type: guide` e `id` igual al slug.

## PRs
- Rama `ta-<n>-<slug>`, commits `tipo(scope): mensaje (TA-n)`.
- La descripción dice qué cambia, cómo se probó (con la salida de `just check` si se tocó código) y enlaza el `feature.md` si existe.
- La revisión de agente es **un comentario firmado** (`**🤖 reviewer**` …) con veredicto. Si pide cambios, se corrigen y solo se revisa lo afectado; no hay segunda ronda completa.
- Con CI en verde y los veredictos en `APPROVE`, merge (squash).

## Linear ↔ repo
- El repo guarda el *qué* y el *por qué*; Linear, el *cuándo* y el estado.
- Toda feature o PR lleva un issue `TA-n`. Estados: *Todo* → *In Progress* (rama creada) → *In Review* (PR abierto) → *Done* (merge).
- Las ideas que aún no se van a hacer viven en [backlog.md](backlog.md), no en Linear.

## Público vs privado
- El repo es **público**: todo lo de `docs/`, `.claude/` y el código es visible.
- **Va en `private/`** (ignorado por git): modelo de negocio, monetización, precios, estrategia y cualquier cosa que no queramos mostrar.
- Ante la duda, va en `private/` y se pregunta al owner.

## Mejora continua
- Si un agente comete el mismo error dos veces, la corrección se agrega a `CLAUDE.md` (sección "Cosas que Claude hace mal").
- Si el proceso vuelve a sentirse pesado, se recorta. La pregunta es siempre: ¿esta revisión protege algo que importe?
