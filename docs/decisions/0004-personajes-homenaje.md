---
id: "0004"
title: Personajes homenaje originales
type: adr
status: approved
risk: high
author: owner
approvals:
  - by: owner
    date: 2026-10-20
created: 2026-10-20
updated: 2026-10-20
---

# ADR-0004: Personajes homenaje originales

## Contexto
Los cruces absurdos (tipo "Pikachu vs Goku") son el gancho, pero usar personajes con copyright (o sprites sacados de juegos, como casi todos los de MUGEN) nos deja a merced de terceros, expone a retiros (DMCA) y pone en riesgo todo el contenido.

## Decisión
- Todos los personajes publicados son **originales**: arquetipos que evocan sin copiar ("la rata eléctrica", "el guerrero de pelo erizado que grita al cargar"). No se usan nombres, logos, diseños ni sprites protegidos.
- El humor y el reconocimiento vienen del arquetipo y del lore, no de la copia.
- Los sprites de MUGEN **solo** pueden usarse en local para prototipar el renderizado, en `content/_local/` (ignorado por git). Nunca se commitean ni se despliegan.
- El renderizador consume un formato neutro propio (spritesheet + JSON), para que los assets sean intercambiables.

## Consecuencias
- Necesitamos un pipeline para crear arte original (feature 004).
- El game-designer revisa cada personaje nuevo con el checklist "¿se parece demasiado?" (skill `write-character`).
