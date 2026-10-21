---
id: "002"
title: Spike de renderizado de peleas
type: intent
status: in-review
risk: medium
author: product-owner
reviewers: [architect]
approvals: []
linear: TA-2
depends_on: ["001"]
created: 2026-10-20
updated: 2026-10-20
---

# Intent: Spike de renderizado de peleas

## Problema
La experiencia depende de que una pelea **se vea bien** en el celular. Es el mayor riesgo técnico y visual del proyecto, y todavía no sabemos si un log de eventos animado con sprites se siente emocionante o plano.

## Resultado propuesto
Una página de prueba donde dos personajes pelean siguiendo un **guion falso** (un `FightLog` escrito a mano): intro, ataques, golpes, esquivas, especial, barras de vida, KO y pose de victoria, con efectos básicos (screen shake, números de daño, partículas) a 60 fps en un celular medio.

Además, queda definido el **formato de assets** del renderer (spritesheet + manifest JSON con animaciones estándar) y un convertidor **solo local** de `.sff`/`.air` de MUGEN a ese formato, para prototipar con sprites existentes sin commitearlos (ADR-0004).

## Usuarios y sistemas afectados
- Owner (decisión de go o no-go sobre el enfoque visual).
- `packages/renderer`, `tools/sff-converter`, una ruta de prueba en `apps/web`.

## Restricciones
- ADR-0004: los sprites de MUGEN solo en `content/_local/`; el repo incluye placeholders originales (aunque sean figuras simples).
- Renderer agnóstico de framework; sin Phaser (ADR-0003).
- `prefers-reduced-motion` respetado.

## Criterios de éxito
- [ ] El guion se reproduce completo y bien sincronizado, en móvil (375 px) y desktop.
- [ ] ≥ 55 fps sostenidos en un celular de gama media.
- [ ] El formato de assets está documentado y lo usan los placeholders del repo.
- [ ] El convertidor de MUGEN genera assets jugables en local para al menos 2 personajes.
- [ ] El owner ve la demo y decide: ¿el enfoque cinemático coreografiado funciona?

## Fuera de alcance
Motor real, WebSockets, apuestas, narrador, arte final.

## Preguntas abiertas
- ¿Lista mínima de animaciones por personaje? (propuesta: idle, walk, attack_light, attack_heavy, special, hit, block, dodge, knockdown, ko, win)
- ¿Cámara fija o con zoom dinámico?
- ¿El convertidor en Rust (reutiliza el workspace) o en TS (más rápido de iterar)?
