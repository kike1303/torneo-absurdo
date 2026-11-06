---
id: "018"
title: Movimiento lateral de la arena 3D
type: feature
status: done
risk: low
author: claude
linear: TA-54, TA-57
depends_on: ["013", "014"]
created: 2026-11-05
updated: 2026-11-05
---

# Movimiento lateral de la arena 3D

## Qué y por qué
En la arena 3D las dos botargas pelean siempre sobre la misma línea, de izquierda a derecha, como en el 2D. El owner pidió aprovechar el 3D con movimientos laterales. Esta feature los agrega como coreografía: los luchadores se rodean y esquivan saliéndose de la línea. El motor es 1D y no cambia, así que el resultado de la pelea tampoco.

## Alcance
- Entra: el eje de la pelea gira despacio alrededor del centro; el que esquiva da un paso fuera del eje y vuelve; movimiento reducido sin ninguno de los dos.
- No entra: que la posición afecte el combate (acorralar, usar el entorno), escenarios, el 2D.

## Diseño
**`src/model/circling.ts` (puro).** Dos funciones de la escena, iguales para todos los espectadores en el mismo `t`:
- `arenaAngle(scene)`: el giro del eje, `0,3 · sin(2π t / 20 s)` (unos ±17°). Con más giro un luchador queda detrás del otro.
- `sidestep(fighter, scene)`: durante un `dodge`, `±0,28 m · sin(π · progreso)` fuera del eje. El lado sale de un hash del lado y del inicio del esquive, y no cambia en todo el esquive.
- Esquive cortado (TA-57): al ritmo 3D el esquive dura 750 ms y el contraataque del que esquiva suele empezar a los ~430 ms, así que el paso volvía al eje de golpe (~0,27 m en un cuadro). La escena expone `FighterScene.dodgeCut` (el último esquive cortado, mientras habría seguido sonando, solo con el pasado y por eso vale en vivo) y el paso vuelve con easing en 250 ms. Ese resto se suma al paso de un esquive nuevo, así que tampoco salta.
- `depthAt(x, scene)`: la profundidad entre los dos luchadores, interpolada por `x`. Los números de daño quedan sobre su luchador y las partículas del golpe, entre los dos.

**`arenaPoint` (`src/three/world.ts`).** Lleva un punto lógico al eje girado, con su paso fuera del eje. Con giro 0 y paso 0 es el mapeo de antes. Lo usan los luchadores (que además giran para seguir mirándose, `facingYaw`), las manchas de sombra, las partículas, los números de daño y el foco de la cámara.

**Cámara.** No sigue el giro: por eso se ve que se rodean. Se aleja hasta un 15 % con el giro máximo, porque el luchador que se acerca crece y se salía del cuadro a 375 px. Crece con el cuadrado del giro, para que no rebote al pasar por el frente. El primer plano del especial sí se mide desde el eje (0,4 rad del eje, no de la pantalla), porque con el giro sumado llegaba a 0,7 rad y ponía al rival delante.

## Riesgos
- Que un luchador tape al otro: el giro está acotado a ±0,3 rad y la cámara también oscila alrededor del frente (014).
- Mareo: giro lento (20 s por ciclo); movimiento reducido lo apaga.

## Pruebas
- [x] `just check` en verde.
- [x] Unit: el giro, acotado, simétrico y sin saltos entre cuadros, y 0 con movimiento reducido; el paso del esquive sale y vuelve, no cambia de lado y aparecen los dos lados; nadie más se mueve fuera del eje en las peleas reales; `arenaPoint` coincide con el mapeo 2D sin giro, gira alrededor del centro manteniendo la distancia y da el paso perpendicular al eje.
- [x] Visual en `/dev/arena` con el eje girado a cada lado, en escritorio y a 375 px.
- [x] e2e general (496 pasan) y `e2e-fights` (4 proyectos) en local.
- [x] TA-57: el paso no salta más de 4 cm entre cuadros de 16 ms en las peleas reales al ritmo 2D y 3D (sin el arreglo, 27 cm); el esquive cortado vuelve al eje; `depthAt`; el frente de cada luchador apunta al otro con el eje girado a cada lado (`Object3D`, sin WebGL); el especial queda a 0,4 rad del eje; el alejamiento sin quiebre en 0.
