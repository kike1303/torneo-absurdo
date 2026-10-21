---
id: "003"
title: Motor de pelea v0
type: intent
status: in-review
risk: high
author: game-designer
reviewers: [architect]
approvals: []
linear: TA-3
depends_on: ["001"]
created: 2026-10-20
updated: 2026-10-20
---

# Intent: Motor de pelea v0

## Problema
Sin motor no hay peleas. Necesitamos que el resultado sea impredecible para el público, pero **reproducible** a partir de la semilla, **balanceable** y lo bastante rico en eventos para que el renderizado y el narrador tengan material.

## Resultado propuesto
`crates/engine` expone una función pura que, dada una semilla, dos personajes y un ruleset, produce un `FightLog`: eventos con offset de tiempo (intro, ataques, golpes, esquivas, bloqueos, críticos, especiales, HP bajo, remontada, KO) y el resultado final. Incluye:
- Esquema de personaje (stats, habilidades, personalidad, debilidades) leído de `content/characters/`.
- Un **simulador masivo** (CLI) que corre N peleas y reporta win rates, duración y % de remontadas.
- 2–4 personajes homenaje de prueba (lore mínimo) para balancear.

## Usuarios y sistemas afectados
- Espectadores y apostadores (indirectamente: la calidad y justicia de cada pelea).
- `crates/engine`, `crates/protocol` (tipos `FightEvent`), `content/characters/`.

## Restricciones
- ADR-0001 (puro y determinista), ADR-0002 (semilla de 32 bytes, PRNG ChaCha), ADR-0007 (eventos como códigos).
- Debe compilar a WASM sin cambios (verificación en el cliente).
- Duración objetivo de una pelea: 45–90 s de reproducción (a confirmar con el spike 002).

## Criterios de éxito
- [ ] Misma semilla y mismos personajes dan un log idéntico byte a byte, en nativo y en WASM (test de propiedad).
- [ ] Ningún personaje de prueba fuera de 40–60% de win rate global en 10k simulaciones.
- [ ] ≥ 15% de las peleas tienen al menos un "momento" (remontada, crítico decisivo o sorpresa).
- [ ] 10k simulaciones corren en < 10 s en la Mac del owner.

## Fuera de alcance
Servidor, scheduler, apuestas, torneos, rating.

## Preguntas abiertas
- ¿Por turnos con tiempo simulado o con ticks en tiempo continuo?
- ¿Cómo influye la "personalidad" en las decisiones (pesos sobre acciones o árbol de comportamiento)?
- ¿Las debilidades por tipo (eléctrico/tierra…) entran en v0 o después?
