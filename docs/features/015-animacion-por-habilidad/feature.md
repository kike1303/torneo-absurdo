---
id: "015"
title: Animación propia por habilidad
type: feature
status: done
risk: low
author: claude
linear: TA-52
depends_on: ["013", "014"]
created: 2026-11-05
updated: 2026-11-05
---

# Animación propia por habilidad

## Qué y por qué
En la arena 3D, la animación de un especial o de un golpe pesado sale de un hash del momento en que empieza: el mismo especial se ve distinto cada vez. Con esta feature cada habilidad tiene siempre la misma animación, así el público reconoce el especial de cada personaje.

Decisión del owner (2026-11-05): por ahora se usan los clips que ya hay (4 especiales, 2 pesados, 2 KO, 3 victorias). No se descargan más.

## Alcance
- Entra: el id de la habilidad en el cue `strike` (timeline v3); la pose elige la variante del especial y del pesado por la habilidad.
- No entra: elegir la animación a mano por personaje en `content/` (va con TA-55), clips nuevos, cambios en el motor o en el protocolo.

## Diseño
**Timeline v3.** Superset estricto de v2: el cue `strike` acepta `ability` opcional, un código (`^[a-z0-9_]{1,64}$`, ADR-0007). v1 y v2 lo siguen rechazando. Los productores emiten v3. El adaptador del `FightLog` ya tiene la habilidad en `attack` y la copia al cue.

**Escena.** El segmento de la coreografía que nace de un strike guarda su `ability`. `FighterScene.ability` (nuevo, `string | null`) la lleva mientras dura esa animación.

**Pose.** Con `ability`, la variante de `special` y de `attack_heavy` es `hash(ability) mod n`. Sin ella (timelines viejos, `/dev/arena`), sigue el hash del lado y del momento. KO y victoria siguen variando por pelea.

**Corrección de CI (2026-11-05).** El `e2e-fights` falló en el runner porque el espectador que llega tarde no veía la pelea en 30 s. Un diagnóstico en el runner (#94, cerrado sin fusionar) mostró pausas de 4 a 8 s entre cuadros sin tareas largas: sin GPU, varias páginas dibujando 3D a 60 fps saturan el rasterizador por software, y el navegador retiene cada `requestAnimationFrame` hasta mostrar el cuadro anterior. `src/three/pacer.ts` (puro) regula cuánto dibuja la arena: primero baja el pixel ratio y después los cuadros dibujados (50, 100 y 200 ms entre cuadros). Con WebGL por software arranca con 100 ms entre cuadros (8 a 10 por segundo). Un cuadro que no se dibuja no carga la GPU, así que la página sigue al día. Antes las pausas de más de 250 ms se ignoraban siempre; ahora cuenta una pausa larga solo si sigue a otra (las del runner vienen seguidas), y nunca las de una página oculta: una pestaña que vuelve, una ventana restaurada o una compilación suelta no degradan la arena (revisión de #93).

## Riesgos
- Un timeline v3 en un cliente viejo: los productores y el renderer salen en el mismo deploy de la web.
- Dos habilidades del mismo personaje pueden caer en el mismo clip (4 especiales): se acepta por ahora; TA-55 permite elegirlo.

## Pruebas
- [x] `just check` en verde.
- [x] Unit: v3 acepta `ability` válido y rechaza uno inválido; v1 y v2 lo rechazan.
- [x] Unit: el adaptador copia la habilidad; la misma habilidad da el mismo clip en distintos momentos y en una pelea real, solo en quien golpea.
- [x] e2e general (496 pasan) y `e2e-fights` (4 proyectos) en local; pelea en vivo en `just dev` sin avisos.
