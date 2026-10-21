---
id: "004"
title: Pipeline de assets originales
type: intent
status: in-review
risk: medium
author: product-owner
reviewers: [architect, game-designer]
approvals: []
linear: TA-4
depends_on: ["002"]
created: 2026-10-20
updated: 2026-10-20
---

# Intent: Pipeline de assets originales

## Problema
No podemos publicar sprites de MUGEN (ADR-0004) y el owner no es artista. Necesitamos una forma **repetible, barata y de calidad decente** de crear personajes originales con todas sus animaciones, en el formato del renderer.

## Resultado propuesto
Un pipeline evaluado y documentado (más un ADR) que diga cómo pasar de "idea de personaje" a "carpeta de sprites lista", con tiempos y costos reales medidos al crear **un personaje completo** de prueba.

Opciones a evaluar (como mínimo):
1. **Generadores de pixel art con AI** (tipo PixelLab o Retro Diffusion): calidad, consistencia entre frames y animaciones, costo y licencia de uso comercial.
2. **Aseprite o LibreSprite con scripting en Lua** y MCPs comunitarios, para que Claude automatice partes (paletas, recortes, exportar spritesheets).
3. **Animación esqueletal** (Spine, DragonBones, Rive): un solo dibujo en piezas y muchas animaciones baratas; candidato para la fase 3.
4. **Híbrido**: la AI genera la base, se retoca a mano y la exportación es automática.

## Usuarios y sistemas afectados
- Game-designer y owner (producción de contenido).
- `content/characters/*/sprites/`, `tools/`.

## Restricciones
- Presupuesto cero o casi cero en esta etapa; los costos recurrentes se documentan.
- La licencia de lo generado debe permitir uso comercial futuro.
- Exporta al formato definido en el spike 002.

## Criterios de éxito
- [ ] Tabla comparativa con calidad, tiempo por personaje, costo y licencia.
- [ ] Un personaje homenaje completo (todas las animaciones mínimas) hecho con la opción ganadora y reproduciéndose en el renderer.
- [ ] Un ADR con el pipeline elegido.

## Fuera de alcance
Producir el roster completo del MVP.

## Preguntas abiertas
- ¿Estilo visual objetivo (resolución del pixel art, paleta, proporciones)? Se define con la identidad visual.
- ¿Vale la pena saltar directo a esqueletal?
