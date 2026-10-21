---
id: "0001"
title: Motor de pelea propio y determinista
type: adr
status: approved
risk: medium
author: owner
approvals:
  - by: owner
    date: 2026-10-20
created: 2026-10-20
updated: 2026-10-20
---

# ADR-0001: Motor de pelea propio y determinista

## Contexto
Las peleas tienen que decidirse en el momento, ser justas para las apuestas, ser baratas de correr y traducirse a animaciones. Alternativas evaluadas: un LLM que decide cada acción, o usar MUGEN o Ikemen GO como motor.

## Decisión
Escribimos nuestro propio motor en Rust (`crates/engine`) como una **función pura**: `simulate(seed, fighter_a, fighter_b, ruleset) -> FightLog`.
- Cada personaje tiene stats, habilidades, debilidades y una "personalidad" (pesos de comportamiento).
- Toda la aleatoriedad sale de un PRNG determinista (ChaCha) alimentado con la semilla.
- El log incluye eventos con offset de tiempo que el renderizador traduce a animaciones.
- Se versiona `ruleset_version`: una pelea guardada siempre se re-simula con su versión.

## Alternativas descartadas
- **LLM decide la pelea**: lento, cuesta dinero en cada pelea, no es reproducible, es difícil de balancear y genera desconfianza en las apuestas.
- **MUGEN / Ikemen GO**: cada personaje trae su propia lógica de pelea (no controlamos el resultado ni el balance) y exigiría transmitir video.

## Consecuencias
- Control total del balance; se puede correr un simulador masivo para medir win rates.
- Reproducibilidad: repeticiones y verificación pública (ADR-0002).
- Hay que evitar fuentes de no-determinismo: orden de `HashMap`, floats inconsistentes entre plataformas (preferir aritmética entera o de punto fijo en la lógica del resultado) y el reloj.
