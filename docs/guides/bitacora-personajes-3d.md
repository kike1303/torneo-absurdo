---
id: "bitacora-personajes-3d"
title: Bitácora de personajes 3D
type: guide
status: draft
risk: low
author: codex
created: 2026-11-04
updated: 2026-11-04
---

# Bitácora de personajes 3D

Registro de trabajo de Jesús con Codex en el chat de Blender. Reúne los archivos vigentes, las pruebas hechas y las ideas pendientes para que otro agente pueda continuar. La dirección planteada para Torneo Absurdo es renderizar las peleas en 3D en tiempo real, con prendas combinables. La implementación actual de la arena usa sprites. Los FBX y los archivos de Blender viven fuera del repositorio.

## Archivos vigentes

Las rutas de esta tabla son relativas a `~/Documents/Blender/`.

| Personaje | Archivo maestro | FBX para Mixamo | Estado comprobado |
| --- | --- | --- | --- |
| Bruno, ogro del pantano | `Ogre/Bruno-Ogro-Rig-Cuerpo-Corregido.blend` | `Ogre/Bruno-Ogro-Mixamo-Cuerpo-Corregido.fbx` | Texturas empaquetadas y rig de Mixamo con 25 huesos. Cabeza, mandíbula y colmillos corregidos. Último ajuste del torso y cierre probado en Blender. |
| Payaso de obra | `cone/Payaso-de-Obra-Texturizado.blend` | `cone/Payaso-de-Obra-Mixamo.fbx` | UV, texturas empaquetadas y vistas de frente, perfil y espalda. FBX en pose T verificado en Blender. |
| Fontanero | `plumber/Fontanero-Texturizado.blend` | `plumber/Fontanero-Mixamo.fbx` | UV, texturas empaquetadas y vistas de frente, perfil y espalda. FBX en pose T verificado en Blender. |
| Rata remendada | `rat/Rata-Remendada-Texturizado.blend` | `rat/Rata-Remendada-Mixamo.fbx` | UV, texturas empaquetadas y vistas de frente, perfil y espalda. FBX en pose T verificado en Blender. |

Usar el archivo maestro de Bruno como punto de partida para cambios de malla, materiales o rig. Su FBX es la copia de intercambio para Mixamo. Las versiones anteriores y los PNG de pruebas del ogro se movieron a la Papelera. Se conservaron las fuentes y la referencia.

## Lecciones del rig y las animaciones

- El rig automático de Bruno repartió parte de la mandíbula y los colmillos entre la cabeza, el cuello y el torso. Se corrigieron los pesos para que la mandíbula, los colmillos, la olla y la rana acompañen a la cabeza. La transición queda en el cuello.
- La primera reexportación desde Blender cargó el personaje en Mixamo, pero el visor se quedó en `Loading` al elegir animaciones. El usuario confirmó que funcionó la variante creada a partir del FBX original de Mixamo, cambiando solo los pesos. Para el ajuste posterior del torso se repitió ese método. Los clips de prueba en Blender mostraron menos estiramiento en la barriga, los costados y el cierre. La prueba del último FBX en el visor web queda pendiente de confirmación.
- Para cada personaje, guardar una descarga base de Mixamo en `FBX Binary` y pose T con malla y esqueleto. Con una animación activa, `With Skin` incluye la malla. El cuadro de descarga del personaje en pose T puede mostrar únicamente formato y pose.
- Descargar una vez los clips comunes con `Without Skin` y adaptarlos al rig de cada personaje en Blender. Revisar el apoyo de los pies, la separación de brazos y torso, la claridad de la pose en pantalla y el momento del impacto. Guardar variantes por personaje cuando sus proporciones lo pidan. `Block.fbx` y `Dodging.fbx` ya sirvieron para pruebas locales con Bruno.

## Próximos pasos

1. Revisar la geometría de las nuevas mallas base en pose T y comprobar en Blender si encajan con los rigs existentes o requieren un nuevo rig. La base de Bruno ya está texturizada y queda pendiente su rig. Las demás bases quedan pendientes de recibir.
2. Probar el mismo bloqueo y la misma esquiva en los cuatro personajes antes de ampliar la biblioteca de clips. Ajustar los pesos o el movimiento donde aparezcan deformaciones o cruces.
3. Usar las nueve acciones del manifiesto actual como referencia para el catálogo de animaciones. Son `idle`, `attack_light`, `attack_heavy`, `special`, `hit`, `block`, `dodge`, `ko` y `win`. Decidir cuáles se comparten y qué ataques necesitan una interpretación propia de cada luchador.
4. Preparar un piloto con Bruno animado y prendas intercambiables en una arena 3D. Revisar el encuadre y la legibilidad de las poses, y sincronizar un impacto con la línea de tiempo del combate.

## Ideas para skins

Jesús decidió usar las nuevas mallas base como punto de partida para los personajes modulares. Cada base tendrá cobertura mínima y conservará las proporciones y los rasgos del personaje. Cascos, chalecos, pantalones, botas y accesorios se prepararán como piezas separadas. Los modelos vestidos sirven de referencia para el diseño y los trajes clásicos. Se pueden compartir accesorios entre personajes cuando su forma y escala encajen.

| Variante | Trabajo sobre los modelos actuales |
| --- | --- |
| Paleta, material o estampado | Reutilizar malla, UV, rig y clips. Pintar máscaras por zona para cambiar ropa, botas y parches de forma independiente. |
| Accesorio rígido | Separar una pieza existente si su geometría lo permite, o modelar un reemplazo. Mantener su unión al hueso correspondiente y probarla en las poses que la acerquen al cuerpo. La olla de Bruno y la llave del fontanero son candidatos. |
| Prendas combinables | La ropa actual forma parte de la silueta y está integrada en varias zonas de la malla. Reconstruir solo lo que quedaría visible bajo una prenda intercambiable, conservar cabeza y proporciones, y crear piezas separadas con sus UV y pesos. Reutilizar el rig cuando se conserve la pose de descanso y la estructura del cuerpo. |

**Piloto propuesto.** Validar la nueva base de Bruno con un casco y un chaleco desmontable. Probar reposo, guardia, flexión y ataque. Para el chaleco, transferir pesos desde la malla base como punto de partida y corregirlos en las poses de prueba.

### Base modular de Bruno

Jesús preparó `Ogre/Bruno-Ogro-Base-Turnaround.png` como referencia de frente, perfil y espalda. Muestra a Bruno en pose T, con shorts ajustados de cobertura mínima, pies y manos visibles y la cabeza libre para recibir el casco.

La malla `Ogre/ogre-base.fbx` llegó en pose T, con 5,505 vértices, 6,728 caras y sin UV. El archivo de trabajo de la base modular es `Ogre/Bruno-Ogro-Base-Texturizado.blend`. Incluye UV y mapas de color, rugosidad y microrelieve de 2048 × 2048 empaquetados. Se revisó de frente, perfil y espalda en Blender. Se conservó la geometría del FBX. La malla tiene picos finos en los costados de la cintura y bordes abiertos que conviene revisar antes del rig.

Se corrigieron los cruces de color en la cara. Los dientes y el interior de la boca usan materiales propios asignados por cara de la malla. El blanco y las pupilas se pintaron sobre las dos piezas de los ojos. Se verificó el resultado en primeros planos de frente, perfil y desde arriba. La geometría y las UV se conservaron durante esta corrección.

La versión vestida sigue siendo el maestro vigente del traje clásico y la referencia de proporciones y movimiento. La base evita tener que reconstruir el cuerpo debajo de las botas y el pantalón integrados en la malla vestida. Mantener la cobertura mínima y dejar las prendas y los accesorios como piezas aparte. Favorecer un relieve de pelo discreto donde apoyan guantes, botas, cinturón y casco.

Para el rig, comparar hombros, cadera, rodillas y tobillos con el esqueleto de la versión vestida. Ambas mallas tienen la misma altura total, pero la base tiene más envergadura y la versión vestida incluye el casco y la rana. Ajustar las articulaciones a la base antes de transferir pesos o reutilizar acciones. Revisar mandíbula y torso en movimiento y usar a Bruno como primera prueba del flujo de equipamiento.

### Vestidor combinable en el juego

PixiJS es el renderer 2D de la arena actual. Three.js es un candidato para mostrar modelos 3D animados en el navegador. El piloto puede partir de la separación entre `FightLog` y la línea de tiempo de presentación para reproducir los eventos con modelos y clips 3D. La elección del renderer y el alcance de la migración quedan pendientes. [Documentación de PixiJS](https://pixijs.com/8.x/guides/getting-started/intro) · [Documentación de Three.js](https://threejs.org/docs/pages/GLTFLoader.html)

- Preparar el cuerpo y cada pieza por separado en Blender. Los accesorios rígidos siguen un hueso, por ejemplo el casco sigue la cabeza. Las prendas deformables se ajustan a la pose de descanso y se pesan para el mismo esqueleto que mueve al cuerpo. Usar FBX para el intercambio con Mixamo y probar GLB como formato para el navegador. [Documentación de mallas con esqueleto](https://threejs.org/docs/pages/SkinnedMesh.html)
- Guardar los identificadores de las piezas equipadas por ranura. El juego arma el conjunto en tiempo real y reproduce la animación del personaje con las piezas puestas. Cinco cascos, seis chalecos, cuatro pantalones y tres pares de botas dan 360 combinaciones si todas las piezas son compatibles. Para un cuerpo compatible se preparan 18 piezas más el cuerpo base.
- Registrar qué prendas encajan en cada cuerpo. Una prenda compartida puede requerir versiones ajustadas a proporciones distintas. Probar el cruce entre mallas en reposo, bloqueo y ataque, y ocultar zonas del cuerpo bajo la ropa cuando el corte de la prenda lo permita.
- Probar primero dos cascos y dos chalecos con Bruno. Confirmar que las cuatro combinaciones se pueden cambiar en el navegador y siguen el movimiento del mismo esqueleto antes de ampliar el vestidor.

## Cómo mantener esta bitácora

Actualizar la fecha, el archivo maestro si cambia, las pruebas verificadas y el siguiente paso cuando se termine una sesión de trabajo. Distinguir las pruebas de Blender de las confirmadas en Mixamo o en la arena. Registrar las propuestas de skins como ideas hasta que haya un piloto aprobado.
