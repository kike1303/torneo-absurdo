---
id: "014"
title: Cámara cinemática y caminatas de la arena 3D
type: feature
status: draft
risk: low
author: claude
linear: TA-51
depends_on: ["013"]
created: 2026-11-05
updated: 2026-11-05
---

# Cámara cinemática y caminatas de la arena 3D

## Qué y por qué
La arena 3D (013) se ve siempre de frente, como el 2D, y las botargas se deslizan al caminar porque `walk` usa el clip `idle`. Esta feature aprovecha el 3D con una cámara que se mueve según lo que pasa en la pelea y agrega clips de caminar hacia adelante y hacia atrás.

Decisión del owner (2026-11-05): cámara "viva pero legible". Intro con acercamiento, intercambio en 3/4 que oscila, primer plano en los especiales, órbita en el KO y en la victoria.

## Alcance
- Entra: el plano de cámara como función pura de la escena; los clips `walk_forward` y `walk_back` en el GLB, elegidos según hacia dónde se mueve el luchador; cuatro idles más (`idle_2` a `idle_5`) que se alternan; movimiento reducido con la cámara fija de frente.
- No entra: el motor (no cambia), el ritmo de la pelea (TA-53), el movimiento lateral (TA-54) ni una animación por habilidad (TA-52).

## Diseño
**Plano (`src/model/shot.ts`, puro).** `shotFor(scene)` devuelve `{yaw, distance, focusX}`: el giro alrededor del eje vertical del foco (0 es de frente; positivo hacia +x), la distancia como múltiplo del encuadre que muestra a los dos y el x lógico al que mira (`null` usa el centro y el zoom de `Scene.camera`). Como solo depende de la escena, que depende de `t`, todos los espectadores ven el mismo plano.
- Intro: acercamiento desde 1,45 veces la distancia con un giro de −20°, en 2,6 s.
- Intercambio: oscila ±17° alrededor del frente con un período de 14 s, igual hacia cada lado. Un 3/4 fijo dejaba a un luchador delante del otro y de espaldas.
- Especial: corte a primer plano del atacante (0,62 de la distancia), girado 23° hacia su frente. Más giro ponía al rival en medio.
- KO: órbita lenta (0,18 rad/s) alrededor del caído. Arranca donde estaba el intercambio y se acerca en 0,9 s. Victoria: la misma órbita alrededor del ganador.
- Movimiento reducido: `Scene.reducedMotion` (nuevo) da el plano de frente de siempre.

`placeCamera` combina el plano con el zoom y el temblor que ya había. El HUD sigue en DOM y no depende de la cámara.

**Caminatas.** `FighterScene.travel` (nuevo: 1 hacia el rival, −1 alejándose, 0 quieto) sale del segmento de la coreografía. Con `walk`, la pose usa `walk_forward` o `walk_back`. En la coreografía, `walk` es solo la vuelta a la marca y dura `RETURN_MS` (300 ms): el clip da un paso (medio ciclo) en ese tiempo, porque a su propio ritmo apenas daría un tercio de paso. El ritmo general es de TA-53.

**Idles.** Cada luchador cambia de idle cada 8 s, por lado, con un hash del lado y del tramo (igual para todos los espectadores) y el fundido de 200 ms entre clips.

**Clips opcionales.** Las caminatas y los idles nuevos son opcionales: si el GLB no los trae, la pose usa `idle`. Vienen de Mixamo (In Place, sin piel, 30 fps). Se cargaron en el archivo de animaciones de Blender, se les aplicó el `fix` de `ikfix.py` (manos fuera del cuerpo) y se exportó con `glb2.py`. Las texturas pasan a JPEG como en 013. El GLB queda en 1,86 MB con 22 clips.

## Riesgos
- Mareo o poca legibilidad en el celular: oscilación lenta y con poca amplitud; movimiento reducido la apaga.
- Los cortes de cámara pueden ocultar un golpe: el primer plano del especial deja al rival a un lado.

## Pruebas
- [x] `just check` en verde.
- [x] Unit: plano con movimiento reducido, acercamiento de la intro, oscilación simétrica, primer plano del especial por lado, órbita del KO continua al empezar, órbita del ganador y planos finitos en todas las peleas golden.
- [x] Unit: clip de caminar por `travel`, un paso por vuelta a la marca, idle que cambia por tramo y respaldo a `idle` sin los clips.
- [x] e2e general (496 pasan) y `e2e-fights` (los 4 proyectos) en local.
- [x] Revisión visual del owner en local (`just dev`).
