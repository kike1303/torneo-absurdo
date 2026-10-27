---
id: "003"
title: Motor de pelea v0
type: intent
status: approved
risk: high
author: game-designer
reviewers: [architect]
approvals:
  - by: architect
    date: 2026-10-26
linear: TA-3
depends_on: ["001"]
created: 2026-10-20
updated: 2026-10-26
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

## Revisión
### architect — 2026-10-26 — APPROVE
- Verifiqué: frontmatter completo según `docs/process.md` (`depends_on: ["001"]` cerrada; `linear: TA-3`); términos del glosario (`Fight`, `FightLog`, `Seed`); coherencia con ADR-0001 (firma `simulate(seed, fighter_a, fighter_b, ruleset) -> FightLog`, personalidad como pesos), ADR-0002 (32 bytes, ChaCha; el CLI no toca semillas de peleas reales), ADR-0007 (eventos como códigos) y ADR-0010 (los triggers del narrador coinciden con los eventos propuestos). Viabilidad: el motor corre una vez por pelea al cerrar apuestas, costo despreciable en el VPS de 1 vCPU / 4 GB; 10k simulaciones en < 10 s es holgado para un motor entero sin I/O; sin dependencias de pago. `crates/engine` y `crates/engine-wasm` están vacíos y fuera del `[workspace]`, así que no hay deuda previa.
- Arrastro a la spec (la escribe architect), sin bloquear el intent:
  1. **Pureza vs. "leído de `content/characters/`"**: el motor no lee archivos (ADR-0001). La spec define un cargador fuera de `crates/engine` (crate o módulo aparte, usado por el server y el CLI) que valida el YAML y entrega structs ya parseados; el motor solo recibe datos.
  2. **Dirección de dependencias motor ↔ protocolo**: decidir si `FightEvent` vive en `engine` y `protocol` lo reexporta, o al revés, sin arrastrar `ts-rs` al motor y respetando "sin enteros de 64 bits" en tipos exportados (offsets en ms como `u32`).
  3. **WASM**: agregar `wasm32-unknown-unknown` a `rust-toolchain.toml` y un test en CI que compare el log nativo y el WASM; definir la serialización canónica para "idéntico byte a byte".
  4. **Determinismo**: aritmética entera o de punto fijo, sin `HashMap` en el camino del resultado, orden de consumo del PRNG fijado y versionado en `ruleset_version`.
  5. **Criterios medibles**: definir "momento" (remontada, crítico decisivo, sorpresa) con umbrales concretos, el caso de empate o tiempo límite, y que el 40–60 % se mide por matriz de enfrentamientos con lados alternados.
  6. **Duración 45–90 s**: parámetro del ruleset, porque el spike 002 sigue `in-review`.
  7. **Preguntas abiertas** (turnos con tiempo simulado vs. ticks, pesos vs. árbol de comportamiento, debilidades por tipo en v0) se resuelven en la spec, con revisión de game-designer.
  8. El CLI necesita `allow(clippy::print_stdout)` local, por los lints del workspace.
- `risk: high`: según `docs/process.md`, hace falta el OK explícito del owner en chat antes de avanzar a la spec.

