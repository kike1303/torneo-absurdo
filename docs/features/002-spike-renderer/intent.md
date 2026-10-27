---
id: "002"
title: Spike de renderizado de peleas
type: intent
status: approved
risk: medium
author: product-owner
reviewers: [architect]
approvals:
  - by: architect
    date: 2026-10-26
linear: TA-2
depends_on: ["001"]
created: 2026-10-20
updated: 2026-10-26
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

## Revisión
### architect — 2026-10-26 — APPROVE
- Verifiqué:
  - **Viabilidad y recursos.** Todo corre en el navegador y en la máquina del owner: no toca el server, Postgres ni el VPS (1 vCPU / 4 GB) y no tiene costo. PixiJS en `packages/renderer` y sin Phaser cumple ADR-0003. `packages/renderer` y `tools/sff-converter` existen solo con `.gitkeep`, y `packages/*` ya está en `pnpm-workspace.yaml`.
  - **ADR-0004.** `content/_local/` está en `.gitignore`. El intent exige placeholders originales en el repo y un formato neutro propio, como pide el ADR.
  - **ADR-0001, ADR-0002 y ADR-0005.** No los toca: no hay motor, semilla ni monedas.
  - **Frontmatter.** Es válido según `docs/process.md`. `FightLog` es término del glosario.
  - **Criterios.** Son verificables, salvo los 55 fps, que la spec tiene que acotar (condición 6).
  - **Preguntas abiertas.** Las tres se arrastran a la spec con recomendación (ver abajo).
- Condiciones para la spec (no bloquean el intent):
  1. **Sin tipos en `crates/protocol` ni en `packages/protocol-ts`.** El formato de `FightLog`/`FightEvent` es de la feature 003, que avanza en paralelo. El "guion falso" de este intent **no es** un `FightLog` del protocolo, sino un fixture con una forma propia del renderer: una línea de tiempo de "cues" con offset en ms, actor y código de acción. El renderer solo consume esa forma. Un adaptador estrecho (`FightLog → timeline`) se escribe cuando 003 fije sus tipos, y vive del lado del consumidor, no en el protocolo. Así el renderer no queda acoplado a la forma exacta de los eventos de 003.
  2. **Formato de assets versionado.** La feature 004 depende de él. El manifest lleva `format_version` y define por animación los frames, la duración por frame en ms, el loop o no, y el pivote o ancla de pies. Los nombres de animación son **códigos** `snake_case` sin texto para humanos (ADR-0007). El formato queda documentado en la spec, y un test valida contra él los placeholders del repo.
  3. **Aleatoriedad cosmética.** CLAUDE.md prohíbe la aleatoriedad fuera de un PRNG con semilla. Las partículas, el shake y la dispersión de los números de daño usan un PRNG **local del renderer** con semilla derivada de la línea de tiempo (por ejemplo, del índice del cue), nunca `Math.random`. Así todos los espectadores y las repeticiones ven lo mismo. Ese PRNG no es el del motor ni recibe la semilla de la pelea (ADR-0002).
  4. **Assets de MUGEN fuera de todo build.** La ruta de prueba solo carga lo convertido desde `content/_local/` en modo desarrollo. Ni el `build/` de adapter-node ni la imagen `web` pueden incluir esos archivos, y una prueba o el `.dockerignore` lo garantiza. Con un build de producción, la ruta funciona solo con los placeholders originales.
  5. **Convertidor solo local.**
     - Ni CI ni `just check` dependen de archivos de MUGEN. Sus tests usan fixtures sintéticos creados en el propio test.
     - Por defecto escribe en `content/_local/`.
     - Trata `.sff`/`.air` como entrada no confiable: valida límites y tamaños, y nunca hace lecturas fuera de rango.
  6. **Medición de rendimiento.** La spec fija un dispositivo de referencia de gama media y el método de medición (FPS del ticker de Pixi a lo largo del guion completo, con su percentil). Además, el renderer se carga de forma diferida solo en la ruta de prueba, para no pesar en el resto de la app.
  7. **i18n y movimiento reducido.** Todo texto visible de la ruta de prueba sale de Paraglide (`check-i18n`). Con `prefers-reduced-motion`, se apagan el shake y el zoom y bajan las partículas, sin cambiar la sincronía del guion.
  8. **Duración del guion.** Dura entre 45 y 90 s, para darle a 003 el dato que su intent deja "a confirmar con el spike 002".
- Recomendaciones para las preguntas abiertas (las decide la spec):
  - **Animaciones.**
    - Obligatorias: `idle`, `attack_light`, `attack_heavy`, `special`, `hit`, `block`, `dodge`, `ko` y `win`.
    - Opcionales, con fallback declarado en el formato: `walk`, `knockdown` e `intro` (el guion tiene intro). Por ejemplo, `intro` cae en `idle` y `knockdown` en `hit`.
    - Así baja el costo por personaje de 004 sin cerrar la puerta a más animaciones.
  - **Cámara.** Encuadre fijo que contiene a los dos personajes en 375 px, con un "punch zoom" breve como efecto en especiales y en el KO, apagado con movimiento reducido. Nada de cámara que siga a los personajes en el spike.
  - **Lenguaje del convertidor.**
    - TS (Node), como paquete del workspace de pnpm en `tools/sff-converter`. Es local, no lo usa ni el server ni el motor, y en Rust tendría que entrar al workspace de Cargo, con `crate-age`, `deny` y clippy en `just check`, sin beneficio.
    - Solo dependencias JS puras para decodificar PCX/PNG y empaquetar el spritesheet (por ejemplo `pngjs`), sin `sharp` ni nada con scripts de instalación, que rompería `strictDepBuilds`.

