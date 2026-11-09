---
id: "bitacora-personajes-3d"
title: Bitácora de personajes 3D
type: guide
status: draft
risk: low
author: codex
created: 2026-11-04
updated: 2026-11-09
---

# Bitácora de personajes 3D

Registro de trabajo de Jesús con Codex en el chat de Blender. Reúne los archivos vigentes, las pruebas hechas y las ideas pendientes para que otro agente pueda continuar. La dirección planteada para Torneo Absurdo es renderizar las peleas en 3D en tiempo real, con prendas combinables. La implementación actual de la arena usa sprites. Los FBX y los archivos de Blender viven fuera del repositorio.

## Archivos vigentes

Las rutas de esta tabla son relativas a `~/Documents/Blender/`.

| Personaje | Archivo maestro | FBX para Mixamo | Estado comprobado |
| --- | --- | --- | --- |
| Bruno, ogro del pantano | `ogre/_anterior/Bruno-Ogro-Rig-Cuerpo-Corregido.blend` | `ogre/_anterior/Bruno-Ogro-Mixamo-Cuerpo-Corregido.fbx` | Texturas empaquetadas y rig de Mixamo con 25 huesos. Cabeza, mandíbula y colmillos corregidos. Último ajuste del torso y cierre probado en Blender. |
| Payaso de obra | `clown/_anterior/Payaso-de-Obra-Texturizado.blend` | `clown/_anterior/Payaso-de-Obra-Mixamo.fbx` | UV, texturas empaquetadas y vistas de frente, perfil y espalda. FBX en pose T verificado en Blender. |
| Fontanero | `plumber/_anterior/Fontanero-Texturizado.blend` | `plumber/_anterior/Fontanero-Mixamo.fbx` | UV, texturas empaquetadas y vistas de frente, perfil y espalda. FBX en pose T verificado en Blender. |
| Rata remendada | `rat/_anterior/Rata-Remendada-Texturizado.blend` | `rat/_anterior/Rata-Remendada-Mixamo.fbx` | UV, texturas empaquetadas y vistas de frente, perfil y espalda. FBX en pose T verificado en Blender. |

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

Jesús preparó `ogre/_anterior/Bruno-Ogro-Base-Turnaround.png` como referencia de frente, perfil y espalda. Muestra a Bruno en pose T, con shorts ajustados de cobertura mínima, pies y manos visibles y la cabeza libre para recibir el casco.

La malla `ogre/_anterior/ogre-base.fbx` llegó en pose T, con 5,505 vértices, 6,728 caras y sin UV. El archivo de trabajo de la base modular es `ogre/_anterior/Bruno-Ogro-Base-Texturizado.blend`. Incluye UV y mapas de color, rugosidad y microrelieve de 2048 × 2048 empaquetados. Se revisó de frente, perfil y espalda en Blender. Se conservó la geometría del FBX. La malla tiene picos finos en los costados de la cintura y bordes abiertos que conviene revisar antes del rig.

Se corrigieron los cruces de color en la cara. Los dientes y el interior de la boca usan materiales propios asignados por cara de la malla. El blanco y las pupilas se pintaron sobre las dos piezas de los ojos. Se verificó el resultado en primeros planos de frente, perfil y desde arriba. La geometría y las UV se conservaron durante esta corrección.

Una revisión desde dentro de la boca detectó caras marrones en la parte posterior de los colmillos. Se completó la asignación del marfil hasta la unión con la encía y se comprobó con primeros planos desde atrás y en diagonal.

La versión vestida sigue siendo el maestro vigente del traje clásico y la referencia de proporciones y movimiento. La base evita tener que reconstruir el cuerpo debajo de las botas y el pantalón integrados en la malla vestida. Mantener la cobertura mínima y dejar las prendas y los accesorios como piezas aparte. Favorecer un relieve de pelo discreto donde apoyan guantes, botas, cinturón y casco.

Para el rig, comparar hombros, cadera, rodillas y tobillos con el esqueleto de la versión vestida. Ambas mallas tienen la misma altura total, pero la base tiene más envergadura y la versión vestida incluye el casco y la rana. Ajustar las articulaciones a la base antes de transferir pesos o reutilizar acciones. Revisar mandíbula y torso en movimiento y usar a Bruno como primera prueba del flujo de equipamiento.

### Vestidor combinable en el juego

PixiJS es el renderer 2D de la arena actual. Three.js es un candidato para mostrar modelos 3D animados en el navegador. El piloto puede partir de la separación entre `FightLog` y la línea de tiempo de presentación para reproducir los eventos con modelos y clips 3D. La elección del renderer y el alcance de la migración quedan pendientes. [Documentación de PixiJS](https://pixijs.com/8.x/guides/getting-started/intro) · [Documentación de Three.js](https://threejs.org/docs/pages/GLTFLoader.html)

- Preparar el cuerpo y cada pieza por separado en Blender. Los accesorios rígidos siguen un hueso, por ejemplo el casco sigue la cabeza. Las prendas deformables se ajustan a la pose de descanso y se pesan para el mismo esqueleto que mueve al cuerpo. Usar FBX para el intercambio con Mixamo y probar GLB como formato para el navegador. [Documentación de mallas con esqueleto](https://threejs.org/docs/pages/SkinnedMesh.html)
- Guardar los identificadores de las piezas equipadas por ranura. El juego arma el conjunto en tiempo real y reproduce la animación del personaje con las piezas puestas. Cinco cascos, seis chalecos, cuatro pantalones y tres pares de botas dan 360 combinaciones si todas las piezas son compatibles. Para un cuerpo compatible se preparan 18 piezas más el cuerpo base.
- Registrar qué prendas encajan en cada cuerpo. Una prenda compartida puede requerir versiones ajustadas a proporciones distintas. Probar el cruce entre mallas en reposo, bloqueo y ataque, y ocultar zonas del cuerpo bajo la ropa cuando el corte de la prenda lo permita.
- Probar primero dos cascos y dos chalecos con Bruno. Confirmar que las cuatro combinaciones se pueden cambiar en el navegador y siguen el movimiento del mismo esqueleto antes de ampliar el vestidor.

## Botarga genérica

Es el personaje por defecto del jugador y el molde de la contextura "normal". Sus proporciones no deben cambiar, porque las prendas deformables se modelan primero sobre ella. Las referencias son `base/_anterior/Sky-Blue Faceless Character Turnaround.png` (la principal) y `base/_anterior/Plush Character Turnaround Sheet.png`. La entrada fue `base/base.fbx` de Tripo, que no se modificó.

| Archivo | Contenido |
| --- | --- |
| `base/Botarga-Base-Texturizado.blend` | Maestro. Malla `Botarga_Base` con UV, tres materiales y mapas empaquetados. |
| `base/Botarga-Base-Mixamo.fbx` | FBX Binary mínimo en pose T: solo la malla, sin armature ni texturas. Subirlo a Mixamo con Skeleton LOD "No Fingers" (25 huesos, igual que Bruno). Ya tiene los brazos largos. |
| `base/Botarga-Base-Plantilla-Cara.png` | Plantilla de 1024 × 1024 de la zona de la cara. |
| `base/Botarga-Base-Vistas.png` | Frente, perfil y espalda con el tinte por defecto y una cara provisional. |
| `base/Botarga-Base-Turnaround.png` | Referencia de diseño: pose T de frente, perfil y espalda, sin cara, sobre fondo blanco y con los brazos largos. Sirve para pedir los próximos personajes con estas proporciones. |

**Primera pasada en Blender (2026-11-04).** Se cerraron los tres huecos de los pies, que eran las 12 aristas no-manifold. La malla se simetrizó en X, porque las dos mitades diferían hasta 1.8 cm, sobre todo en las orejas. Mide 1.0 de alto, con los pies en Z=0 y el origen entre los pies, mira a -Y y tiene las transformaciones aplicadas. Tripo entregó 10 706 triángulos. Se decimó con simetría hasta 9 704 triángulos y 4 854 vértices, y se conservan casi todos los quads. No había picos ni caras degeneradas. Brazos y piernas ya venían bien para Mixamo. Los brazos están horizontales y separados del torso, y las piernas están separadas desde la entrepierna (Z≈0.24).

**UV.** Son 24 islas con distorsión baja: el ángulo cambia 3° en la mediana y 12° en el percentil 95, y ninguna cara está invertida. Los cortes entre frente y espalda, arriba y abajo y entre cada extremidad se hicieron con planos rectos, para que las costuras sean líneas limpias. La cabeza se parte por la línea media. La nariz, cada oreja y las plantas tienen islas propias. El short sigue el pliegue de la cintura y del borde de las piernas. La espalda del torso no tiene costura central, porque allí va el cierre.

**Ranura de la cara.** Es una isla propia, plana y proyectada de frente sobre el cuadrado X ∈ [-0.14, 0.14] y Z ∈ [0.625, 0.905] de la malla. La nariz queda en el centro y las orejas quedan fuera. En la UV, ese cuadrado ocupa la esquina superior izquierda del atlas, con u0 = 0.004 y un lado de 0.265. El valor exacto está en la propiedad `face_rect` del objeto. La cara es otra textura RGBA del mismo encuadre que la plantilla. El material `Botarga_Cara` la lee con un nodo Mapping que lleva ese rectángulo a 0…1 (extensión `CLIP`) y la mezcla sobre el fieltro según su alfa. En Three.js se logra lo mismo con `offset` y `repeat` en la textura de la cara. En la plantilla, el blanco con borde rojo es toda la zona dibujable, el gris oscuro es la nariz, que no se pinta, y la elipse verde punteada es el área segura recomendada para ojos, cejas y boca. Las guías azules marcan el centro y la altura de la nariz. La cara provisional va empaquetada como `Cara_Provisional`. En el maestro está desactivada con el valor "Mostrar cara" en 0.

**Materiales.** `Botarga_Cuerpo`, `Botarga_Short` y `Botarga_Cara` comparten los mapas de 1024 × 1024 `Botarga_Base_Color` (blanco hueso), `Botarga_Base_Roughness` y `Botarga_Base_Normal`. Se hornearon de un fieltro procedural en coordenadas de objeto, así que el ruido sigue sin cortes a través de las costuras. Cada material multiplica el color por su nodo "Tinte". El valor por defecto es celeste pastel en el cuerpo y la cara, y azul marino en el short. El cierre de la espalda no tiene geometría. Va en los mapas, con cinta gris, dientes alternados y rugosidad más baja, desde la cintura del short hasta debajo de la nuca. Como el cierre es gris en el mapa de color, el tinte del jugador también lo colorea. Si debe quedar metálico, hará falta una máscara de tinte.

**Verificado en Blender.** Al reimportar el FBX llegan una sola malla, 9 704 triángulos, 0 aristas no-manifold, `UVMap` y los tres materiales. La malla mantiene el alto de 1.0 y mira a -Y. Se revisaron los renders de frente, perfil y espalda y los primeros planos de la cara y del cierre. Falta la prueba en Mixamo.

**Exportar para Mixamo.** La primera exportación usó las opciones por defecto: `UnitScaleFactor` 1 y un nodo con rotación de -90° y escala 100. Mixamo la rechazó con "Sorry, unable to map your existing skeleton", aunque el archivo no tenía huesos. Se volvió a exportar como el FBX de Tripo, que Mixamo acepta: `apply_scale_options='FBX_SCALE_UNITS'` y `bake_space_transform=True`. Así quedan `UnitScaleFactor` 100 y un nodo sin transformación. Tampoco lleva texturas ni grupos de suavizado. Mixamo volvió a fallar con el mismo error. Según la [guía de Mixamo](https://helpx.adobe.com/mx/creative-cloud/help/mixamo-rigging-animation.html), un archivo sin esqueleto debería abrir los marcadores, así que Mixamo está leyendo algo del FBX como si fuera un rig. La tercera exportación deja el FBX tan mínimo como el de Tripo: solo `Geometry`, `Model` y 3 materiales simples, sin nodos `Texture` ni `Video`, sin aristas y sin propiedades personalizadas. Como alternativa se preparó un .zip con OBJ, MTL y el mapa de color, que la guía también acepta. No hizo falta y se borró.

**Mixamo.** El FBX mínimo funcionó. Jesús descargó `base/Botarga-Base-Mixamo-T-pose.fbx` (FBX Binary, pose T, con malla). Trae los 25 huesos `mixamorig:*`, igual que Bruno, y conserva las UV y los tres materiales. Se ve gris porque el FBX subido no llevaba texturas.

**Segunda pasada en Blender (2026-11-04).** Siguiendo el método de Bruno, se partió del FBX de Mixamo y solo se cambiaron los pesos. Las texturas y los materiales se recuperaron del maestro, y la malla coincide vértice por vértice.

| Archivo | Contenido |
| --- | --- |
| `base/Botarga-Base-Rig.blend` | Maestro del rig: malla `Botarga_Base` con los materiales completos y armature `Botarga_Esqueleto`, sin acciones. |
| `base/Botarga-Base-Animaciones.blend` | El rig con los 16 clips corregidos como acciones (nombre del archivo) y los originales como `<clip>_mixamo` para comparar. La cara provisional está visible. |
| `base/Botarga-Base.glb` | Para el navegador (3.45 MB). Malla con skin de 25 huesos y como máximo 4 influencias por vértice, y las 16 animaciones corregidas a 30 fps, con el nombre de cada archivo. Lleva los mapas de color, rugosidad y normal sin teñir. En `extras` de cada material va `tint_default` (`#9FD3F0` en el cuerpo y la cara, `#1F2F5A` en el short), y el material de la cara lleva `face_uv_rect`. |

Correcciones de pesos sobre lo que entregó el auto-rig:

- Cabeza, cara, nariz y orejas quedan rígidas en `Head`. Mixamo dejaba en la base de la cabeza un 38 % del peso repartido entre `Spine2` y los hombros, que la aplastaba al girarla. El cuello (`Neck`, que tenía casi cero) toma el anillo superior del torso, con una transición desde `Spine2`.
- La rodilla no se usaba: `Leg` no pasaba de 0.12. Ahora la espinilla, entre Z 0.115 y 0.15, es de `Leg`, con transiciones a `UpLeg` arriba y a `Foot` en el tobillo. El empeine y los dedos conservan los pesos de Mixamo.
- Se quitó la influencia de `Head` en hombros y torso. Se suavizaron los pesos de hombros, axilas y panza en 8 pasadas, y se limitó cada vértice a 4 influencias normalizadas.

Se verificó en Blender con poses de prueba: brazos abajo, golpe, guardia, giro de cabeza, torsión de torso, patada y agachado. El GLB se reimportó y se posó con las texturas. Pendiente: un pliegue leve junto al ombligo en torsiones fuertes de torso, y la prueba con los clips reales y en el navegador.

**Animaciones.** El esqueleto es común, así que los clips genéricos se descargan una vez (FBX Binary, Without Skin, 30 fps, In Place cuando aplique) y sirven para todos. Siete acciones son genéricas: `idle`, `attack_light`, `hit`, `block`, `dodge`, `ko` y `win`. `attack_heavy` y `special` son propias de cada habilidad de cada personaje, porque cada uno conserva sus ataques especiales (por ejemplo `mud_belly_flop` y `swamp_burp` de Bruno). La botarga genérica necesita también las suyas.

**Primeros clips sobre la botarga (2026-11-04).** `~/Documents/Blender/animations/` tiene 16 clips que cubren las 9 acciones, con variantes de `attack_heavy`, `special`, `ko` y `win`. Todos tienen los 25 huesos, la escala del rig y 30 fps. Se aplicaron al rig en `base/Botarga-Base-Animaciones.blend`, con una acción por archivo y la cara provisional visible para revisarlos. `dodge` termina 16 cm atrás y `win_2` recorre unos 40 cm. Los demás vuelven al punto de partida.

Un detector de autointersección por cuadro encontró choques en los 16 clips. Los puños de guardia se hunden en el mentón y, en `block`, `hit`, `dodge`, `special` y `ko`, las manos entran en el pecho y la panza. `win_2` además cruza las piernas. La causa es la proporción: del hombro a la muñeca hay unos 12 cm, el 12 % de la altura, mientras que la cabeza mide 36 cm y el pecho 27 cm de fondo. Los clips de Mixamo están hechos para brazos humanos, que son unas tres veces más largos que la cabeza. Se probó una corrección automática que gira el brazo desde el hombro hasta sacar las manos de la cabeza y el torso. Elimina buena parte de los choques con el torso, pero cambia la intención de la pose (el bloqueo queda con los brazos abiertos), así que no se adoptó. Queda por decidir entre alargar los brazos del molde, ajustar los clips en Mixamo (Character Arm-Space) o retocarlos a mano por clip.

**Prueba de brazos largos.** Se estiró el tramo entre el hombro y la muñeca en la malla y los huesos del rig, sin pasar otra vez por Mixamo. Los clips solo traen rotaciones y la posición relativa al reposo, así que conservan el brazo más largo. Estirar el brazo no basta: los ángulos de los clips siguen dirigiendo la mano hacia la cara, solo que más lejos. Para que el puño quede delante del mentón, la muñeca debe quedar a unos 19–23 cm del hombro, y el brazo actual mide 12 cm. Sí funciona combinar brazos más largos con un ajuste IK automático. En cada cuadro, el ajuste adelanta la mano, hacia el frente del pecho, hasta sacarla de las elipsoides de cabeza, pecho y panza (que se calculan de la propia malla). Luego resuelve el brazo con IK de dos huesos, conserva el plano del codo y el giro de la mano respecto al antebrazo, y suaviza el desplazamiento en el tiempo. El conteo de cuadros con choques (cabeza / torso), sumado en los 16 clips, fue:

| Brazos | Solo los clips | Con ajuste IK |
| --- | --- | --- |
| Actuales (×1) | 382 / 443 | 284 / 402 |
| ×1.8 | — | 286 / 85 |
| ×2.1 | — | 208 / 116 |

Con ×2.1 la guardia queda con los puños delante del mentón y el bloqueo con los antebrazos cruzados delante del pecho, sin perder la intención del clip. Los choques que quedan se concentran en `ko_1` (en el suelo) y `win_1` (manos en la cabeza), donde algo de contacto es natural. El ajuste no requiere configurar nada por clip ni por personaje, porque las elipsoides salen de la malla de cada botarga.

**Brazos largos adoptados (2026-11-05).** Jesús aprobó el cambio y los brazos ×2.1 pasan a ser parte del molde. El tramo entre el hombro y la muñeca va de 12 a unos 24 cm, la mano se mueve rígida y la envergadura pasa de 0.72 a 0.94. Se estiró la malla etiquetada antes de desdoblar, y se rehicieron las UV (siguen siendo 24 islas con la misma distorsión) y el horneado. En el rig se estiraron igual la malla y los huesos, se conservaron los pesos corregidos y se copiaron las UV y los materiales del maestro nuevo (las caras coinciden exactamente). La plantilla de la cara no cambió, porque usa el mismo cuadrado frontal. Solo cambió el `face_rect` del atlas: v0 = 0.740 y lado 0.256. No hace falta volver a Mixamo: el esqueleto es el mismo y los clips se reutilizan. Con el ajuste IK aplicado, el total de cuadros con choques bajó de 382 / 443 a 208 / 116. `idle` quedó en 0 / 0 y el torso casi limpio en golpes y especiales. Lo que queda está en `ko_1`, `ko_2`, `win_1` y `win_3`. El GLB se reimportó en Blender y reproduce los clips. Falta la prueba en el navegador.

**Brazos rehechos (2026-11-05).** En el piloto los brazos se veían flacos y en punta, se doblaban feo y a veces mostraban huecos. Había tres causas:

- La decimación había dejado el tramo entre hombro y muñeca con 112 caras por brazo, y 110 aristas medían más de 4 cm (hasta 17 cm). Estirarlo ×2.1 alargó esos triángulos, que no pueden doblarse en el codo: se pliegan y dejan ver el interior.
- Al estirar los huesos conectados, el codo y la muñeca se movieron dos veces, porque comparten punta con el hueso siguiente. El hueso de la mano quedó 11 cm más afuera que la muñeca real.
- El estiramiento alargó sobre todo el antebrazo, y el codo quedó a 3 cm del hombro.

Corrección:

- El tramo se reconstruyó como un tubo de 16 lados con 14 anillos (uno cada ~1.4 cm). Va cosido a los anillos existentes del hombro y la muñeca, sigue el radio de cada punta con un leve abultamiento al centro y queda cerrado (0 aristas no-manifold).
- Para no pasar de 10 k, el cuerpo se decimó al 0.75 en vez del 0.80. El total quedó en 9 828 triángulos.
- `stretch.py` ahora calcula todas las posiciones antes de mover los huesos. El codo quedó a mitad del brazo (hombro 0.118, codo 0.227, muñeca 0.336).
- El rig se rehízo desde el de Mixamo: pesos pasados por superficie, pesos por tramo en los tubos (brazo, codo y muñeca) y luego las correcciones de cabeza, cuello y rodilla y el suavizado.

En la lámina de referencia se veía un escalón en el hombro y otro en la muñeca, como una manga metida en el brazo. El tubo se había armado como una elipse con el radio máximo de cada punta, y los anillos reales no son elipses. Ahora cada anillo del tubo interpola la forma real del anillo del hombro y del de la muñeca, ángulo por ángulo, con una transición suave. Luego se suavizan con Laplaciano (6 pasadas, conservando el volumen) los vértices a ±3 cm de cada unión. Así el brazo queda continuo y sin escalones. Con el ajuste IK, los cuadros con choques en los 16 clips quedaron en 190 / 14 (cabeza / torso). La plantilla de la cara cambió un poco por la nueva decimación de la cabeza y se volvió a exportar. Pendiente: la rodilla, con la decimación al 0.75, se ve algo facetada al doblarse mucho.

**Ranura de la cara ampliada y caras de referencia (2026-11-05).** Jesús pasó una lámina con 20 expresiones (`base/Caras-Referencia.png`): neutral, guardia, preparación, golpe, daño, KO, aturdido, victoria, derrota, celebración, concentrado, alerta, vigilando, cansado, recuperándose, enfadado, decidido, entusiasmado, dudoso y dormido. Sus rasgos (ojos grandes y cejas altas) no cabían en la ranura original, así que se amplió. La elipse de la cara pasó de 25 × 22 cm a 30 × 28 cm, centrada en Z 0.80, y el cuadrado frontal de la plantilla pasó a X ∈ [-0.17, 0.17] y Z ∈ [0.63, 0.97]. En el atlas, `face_rect` quedó con v0 = 0.679 y lado 0.317. Se rehicieron la plantilla, las UV, el horneado, el rig, los clips y el GLB.

Las caras se extrajeron con un script, sin dibujarlas a mano:

1. Cada cabeza se detecta con un círculo (RANSAC sobre el perfil superior, con radio común).
2. La piel celeste se vuelve transparente por color, así que párpados y mejillas toman el tinte del jugador. Los bordes se limpian del celeste para que no quede halo al teñir.
3. Cada cara se alinea con la nariz 3D según un punto de anclaje propio. Un primer intento asumía la nariz en el mismo punto de todas las cabezas y dejaba la nariz fuera de lugar: la posición varía y varias cabezas están ladeadas. El anclaje es la nariz dibujada cuando se ve (8 caras), y si no, el punto medio entre los ojos a 0.4 del camino hacia la boca. Ese factor se calibró con las caras de nariz visible. Seis caras en las que la detección confundía dientes con ojos o marcas con la boca se midieron a mano en una cuadrícula (`anchors.json`).
4. Se descartan los restos: la línea del cuello, el aro de estrellas del KO, nubes, el símbolo de enojo y las manos de `golpe` y `dudoso`.

Las 20 PNG están en `piloto/caras/` y se revisaron sobre el modelo en `base/Botarga-Base-Caras.png`. El piloto las carga todas y cambia de cara según el clip: guardia en `idle`, golpe en los ataques, enfadado en los especiales, concentrado en el bloqueo, alerta al esquivar, daño al recibir un golpe, KO y victoria. `dudoso` quedó sin boca porque en la referencia la tapa la mano.

**Scripts.** El flujo completo vive en `~/Documents/Blender/scripts/botargas/`, fuera del repo y con un README: limpieza, UV, horneado, pesos, alargamiento de brazos, ajuste IK de clips, detector de choques y exportación. Varios usan medidas de la genérica, así que para otra botarga hay que medirlas primero.

**Para las próximas botargas.** Si comparten las proporciones base, los brazos ya deben salir largos desde la imagen. Al pedirla a la IA, conviene describir brazos que lleguen a medio muslo. Después de Mixamo se corre `ikfix.py` sobre los clips comunes, sin ajustes manuales. Bruno, el payaso, el fontanero y la rata tienen sus propias mallas: hay que medir sus brazos y, si son cortos, alargarlos con `stretch.py` antes de animar.

**Piloto en el navegador (2026-11-05).** Es un prototipo desechable fuera del repo, en `~/Documents/Blender/piloto/`: un `index.html` con Three.js r170 desde CDN y cinco caras de prueba en `caras/` (`neutral`, `guard`, `hit`, `ko` y `win`, RGBA de 1024 × 1024 en el encuadre de la plantilla). Para abrirlo hay que servir `~/Documents/Blender` (por ejemplo `python3 -m http.server 8765 --bind 127.0.0.1`) y entrar a `http://127.0.0.1:8765/piloto/`. Se verificó en el navegador:

- El GLB carga sin errores de consola, con 9 704 triángulos, 4 draw calls y las 16 animaciones.
- El tinte se aplica con `material.color`, que multiplica el mapa blanco hueso y toma los valores por defecto de `extras.tint_default`. El cuerpo y la cara comparten tinte, y el short se tiñe aparte.
- La cara se cambia sin tocar el atlas. Un `onBeforeCompile` en `Botarga_Cara` lee una segunda textura dentro de `face_uv_rect` y la mezcla por su alfa. glTF invierte la V, así que se usa `1 - v`. La cara también puede cambiar sola según el clip: guardia en `idle`, ataques y bloqueo, `hit` al recibir un golpe, `ko` y `win`. Las caras quedaron alineadas: ojos sobre la nariz y boca debajo.
- Los clips se reproducen con `AnimationMixer` y un fundido de 0.2 s. `ko` y `win` se detienen en su último cuadro.
- A 375 px de ancho, la cámara se aleja y el panel baja, así que se ve el cuerpo entero.
- El costo por cuadro medido en la Mac (con animación y sombras) fue de 0.75 ms a 2048 × 1536 y 0.25 ms a 375 × 667. Falta medirlo en un celular medio.

Si se integra al juego, iría como feature del renderer: cargar el GLB, teñir por `extras`, poner la cara con esta ranura y mapear `FightEvent` a clips y expresiones.

**Riesgos para Mixamo.** Los brazos son cortos y las manos grandes. El hombro está en X≈0.15 y la muñeca en X≈0.245, así que conviene colocar a mano los marcadores de codo y muñeca. La cabeza es grande y apoya casi sin cuello (barbilla en Z≈0.645). Por la experiencia con Bruno, conviene revisar en la segunda pasada los pesos entre cabeza, cuello y torso y los de la panza. La ingle está baja (Z≈0.24) y las piernas son cortas, así que hay que confirmar los marcadores de rodilla.

**Auto-rig sin Mixamo (2026-11-08).** Mixamo no tiene una API pública de auto-rig, así que `scripts/botargas/autorig.py` arma el rig en Blender. Toma una malla en pose T con los pies en Z=0, mirando a -Y. Detecta las articulaciones en la silueta frontal y con rayos de profundidad, y crea los 25 huesos `mixamorig:*` con los mismos nombres, jerarquía y ejes que los rigs de Mixamo. Por eso los clips de `animations/` se aplican sin cambios. Pesa con _bone heat_ de Blender. Los vértices donde falla, o la malla entera si falla en más de la mitad, toman los pesos de una copia remallada con vóxeles. Al final deja la cabeza rígida y como máximo 4 influencias por vértice. Escribe `.blend` y, si se pide, FBX y GLB.

Las reglas se ajustaron con los dos rigs de Mixamo que hay, el de la botarga (antes de alargar los brazos) y el de Bruno. La axila coincide con el hombro de Mixamo. La columna usa las mismas fracciones entre `Hips` y `Neck`. El tobillo sale del salto del perfil del pie. El error medio por articulación fue de 0.7 cm en la botarga y de 1.4 cm en Bruno, con un máximo de 3.6 cm en la cabeza de Bruno por la barba. Como las reglas salen de esos mismos dos rigs, no es una prueba independiente. En la botarga, los clips reproducen la altura de la cadera del rig de Mixamo con 2 a 3 mm de diferencia. También se probó con el payaso, el fontanero y la rata, que no tenían rig. En los cinco, `idle`, `attack_light`, `block` y `hit` (más `ko_1` en la botarga) se ven sin rasgaduras en hojas de 260 px (`autorig_sheet.py`). El FBX y el GLB se reimportan con 25 huesos, y el GLB tiene como máximo 4 influencias por vértice. Falta revisar de cerca hombros, panza y rodillas, y probar los rigs en el navegador. Las correcciones por personaje, como `fixw.py` o `stretch.py`, se siguen corriendo después.

## Bases finales (2026-11-08)

Jesús dejó en cada carpeta la malla base definitiva y su lámina de tres vistas: `clown/final-clown`, `plumber/final-plumber`, `rat/final-rat` y `ogre/final-ogre` (`.fbx` y `.png`). Ninguna traía UV ni textura. Se pasaron por el flujo nuevo, documentado paso a paso en [Flujo de un personaje 3D](flujo-personajes-3d.md): malla, textura proyectada desde la lámina, rig automático y GLB con los 22 clips. Cada carpeta tiene ahora `final-<slug>-malla.blend`, `-texturizado.blend`, `-vistas.png`, `-rig.blend`, `-rig.json` y `final-<slug>.glb`. Los cuatro se ven en `visor/` junto a la botarga base.

| Personaje | Triángulos (antes → después) | Ajuste de la lámina (`iou` frente) | Notas |
| --- | --- | --- | --- |
| Payaso | 10 354 → 12 694 | 0.88 | Los costados de la cara quedan algo manchados: la malla tiene la boca más abierta que la lámina. El cono de la nuca salió color piel. |
| Fontanero | 10 686 → 14 622 | 0.92 | 57 aristas no-manifold de Tripo en los dedos del pie izquierdo, la nuca y el tubo. El pesado necesitó la copia de vóxeles gruesos (0.008). La boca del tubo y su borde superior quedan claros. |
| Rata | 10 552 → 11 806 | 0.92 | Malla limpia. La cola no tiene huesos propios y sigue al cuerpo. |
| Ogro | 11 088 → 13 902 | 0.95 | Malla limpia. El relieve del pelo viene en la geometría. |

Falta: revisar de cerca las deformaciones en todos los clips, decidir si se alargan los brazos (del hombro a la muñeca miden entre 17 y 18 cm, con manos grandes) y retocar a mano las zonas de textura de la tabla.

## Orden de carpetas y personajes nuevos (2026-11-08)

Cada carpeta se llama como su slug: `Ogre` pasó a `ogre` y `cone` a `clown`. En la raíz de cada una quedan solo los `final-<slug>.*` vigentes y las tres vistas de la lámina (`final-<slug>-frente.png`, `-perfil.png`, `-espalda.png`). Lo anterior (los trabajos con Mixamo y el primer auto-rig, las láminas y mallas viejas de Tripo y los respaldos `.blend1`) está en `_anterior/`. El `ogre.blend` suelto de la raíz pasó a `ogre/_anterior/`. La botarga base conserva los nombres `Botarga-Base-*`, que usan los scripts de `scripts/botargas/`. Sus `pre-014` y las dos láminas de diseño están en `base/_anterior/`.

Llegaron 11 personajes con su lámina, sin malla todavía: `washer` (robot lavadora), `gnome` (gnomo de plastilina), `astronaut` (astronauta pecera), `yeti` (yeti helado), `mop` (mago trapeador), `hedgehog` (erizo cepillo), `ninja` (ninja calcetín), `pinata` (luchador piñata), `gorilla` (gorila luchador), `goo` (baba rosa) y `eggplant` (berenjena vampiro). Para pasarlos por el flujo falta el `final-<slug>.fbx` de Tripo de cada uno.

## Brazos y ajuste IK del payaso (2026-11-09)

El payaso fue el primero de las bases finales en pasar por la corrección de choques. Su cabeza mide 56 cm de ancho y arranca a la altura del hombro (barbilla en Z≈0.54, brazo en Z≈0.51), y del hombro a la muñeca hay 18 cm. Con los clips de Mixamo tal cual, las manos entraban en la cabeza en los 22 clips, incluso en `idle` (los puños de guardia quedaban dentro de la cara) y en `block`, y en `hit`, `ko_*`, `special_*` y `win_*` también en el torso.

Los scripts de la botarga (`stretch.py`, `ikfix.py`, `collide.py`) dependían del atributo `part` del maestro, que las mallas del flujo nuevo no tienen. Se generalizaron en `scripts/botargas/` para cualquier rig de `autorig.py`, tomando la región de cada vértice de su grupo dominante:

- `stretch_rig.py out.blend K`: alarga el tramo hombro→muñeca ×K en la malla y los huesos (la mano se mueve rígida, el codo queda a la mitad) y parte las aristas del brazo que quedan de más de 3 cm, con pesos y UV interpolados.
- `ikfix_rig.py out.blend`: por cuadro, adelanta cada mano hasta sacarla de tres elipsoides (cabeza, pecho y panza, medidas de los vértices de `Head`/`Neck`, `Spine1`/`Spine2` y `Spine`/`Hips`), resuelve el brazo con IK de dos huesos y suaviza en el tiempo. Guarda los originales como `<clip>_mixamo`.
- `collide_rig.py out.json`: cuenta por clip los cuadros con vértices de la mano dentro de la isla cerrada de la cabeza (paridad de cuatro rayos casi horizontales, para que una isla abierta por el cuello no dé falsos positivos) y dentro de las elipsoides del torso, con la profundidad máxima. Un primer intento medía la profundidad con la normal de la cara más cercana sobre superficies abiertas por grupo y daba choques de 20 cm en cuadros limpios; no sirve.
- `bake_glb_actions.py out.glb`: exporta el GLB con las acciones ya corregidas del archivo (sin las `_mixamo`), en vez de volver a cargar los FBX.
- `frames_sheet.py out.png clip:cuadro,...`: hoja de revisión de cuadros concretos desde cuatro ángulos.

Se probaron cuatro variantes sobre `clown/final-clown-rig.blend`, con el ajuste IK en todas. Cuadros con la mano dentro de la cabeza / del torso, sumados en los 22 clips (muestreo cada 2 cuadros):

| Brazos | Sin ajuste | Con ajuste IK |
| --- | --- | --- |
| ×1.0 (18 cm) | 627 / 675 | 151 / 328 |
| ×1.3 (23 cm) | — | 86 / 312 |
| ×1.5 (26 cm) | — | 91 / 189 |
| ×1.7 (30 cm) | — | 118 / 195 |

Se adoptó **×1.3**: es el cambio de proporción más chico que deja la cabeza limpia (la envergadura pasa de 0.93 a 1.03 y en pose T casi no se nota) y ×1.5 o ×1.7 no mejoran la cabeza. Lo que queda son roces de menos de 2,5 cm y contacto natural: las manos sobre los conos en `win_1`, entre las rodillas en `dodge` y en el suelo en `ko_*`; `idle_4` (agachado) y `win_2` concentran el resto. Archivos: `clown/final-clown-rig.blend` (brazos ×1.3, 22 clips corregidos y sus `_mixamo`), `clown/final-clown.glb` y la entrada `arms` de `clown/final-clown-rig.json`. Lo anterior quedó en `clown/_anterior/*.pre-arms.*`. El GLB se recargó en el visor con los 22 clips.

Para los demás personajes, el mismo orden: `stretch_rig.py` (si hace falta), `ikfix_rig.py`, `collide_rig.py` para comparar y `bake_glb_actions.py`. Cada paso tarda menos de un minuto en la Mac.

## Piezas rígidas de la cara y ajuste IK en los otros tres (2026-11-09)

Jesús vio que las costuras de las mejillas del payaso se alargaban en las animaciones. Son piezas sueltas de 22 vértices junto al borde cabeza-hombro, y el pesado por calor les había dejado hasta un 58 % de `Neck` y un 10 % de los hombros, igual que al borde inferior de la propia cabeza (la regla de cabeza rígida de `autorig.py` solo cubre `|x|` menor que la axila, y la cabeza del payaso es más ancha que los hombros). `scripts/botargas/rigidparts_rig.py` lo corrige para cualquier rig: cada pieza suelta chica (menos de 500 vértices) copia los pesos del vértice más cercano de la pieza grande cuyo hueso dominante coincide con el suyo (una costura es de `Head`, así que copia de la cabeza aunque el hombro esté más cerca), y después la pieza de la cabeza (la isla con más vértices de `Head`, o los vértices dominados por `Head` cuando la cabeza va unida al cuerpo) se queda solo con `Head` y `Neck`. Dos tropiezos que conviene no repetir: la cercanía por centroide elegía el cuerpo, y quitar grupos iterando `v.groups` reindexa la lista y deja uno sin borrar (los vértices quedaban con `Head` 1.0 y `LeftShoulder` 0.26 a la vez); hay que sacar el vértice de todos los grupos y volver a asignar. Verificado en primeros planos de `hit` e `idle`: las costuras quedan pegadas a la mejilla.

El fontanero, la rata y el ogro pasaron por el mismo flujo con `stretch_rig.py` ×1.0, ×1.3 y ×1.5. Cuadros con la mano dentro de la cabeza / del torso, sumados en los 22 clips:

| Personaje | Sin ajuste | ×1.0 + IK | ×1.3 + IK | ×1.5 + IK |
| --- | --- | --- | --- | --- |
| Fontanero (brazo 18 cm) | 555 / 648 | 5 / 323 | 24 / 315 | 67 / 206 |
| Rata (18 cm) | 628 / 639 | 185 / 298 | 232 / 348 | 198 / 249 |
| Ogro (17 cm) | 688 / 678 | 236 / 445 | 285 / 427 | 301 / 405 |

En los tres, alargar el brazo no mejora la cabeza (sus cabezas no son más anchas que los hombros, a diferencia del payaso), así que quedaron **sin alargar**, solo con el ajuste IK y las piezas rígidas. Lo que queda son roces de menos de 1 cm en el fontanero, y en la rata y el ogro de 2 a 9 cm en `win_1` (manos en la cabeza), `attack_heavy_1`, `idle_4` (agachado) y `ko_*`, que en las hojas se ven como contacto natural. La cabeza de la rata y la del ogro van unidas al cuerpo en una sola isla, así que `collide_rig.py` mide la cabeza con una elipsoide en vez de la isla cerrada. Archivos: `final-<slug>-rig.blend` y `final-<slug>.glb` de los tres, con los anteriores en `_anterior/*.pre-ik.*`, y la entrada `arms` de cada `-rig.json`.

`personaje.sh` ya incluye los pasos: 4b piezas rígidas, 4c brazos si hay `arms.json` (`{"stretch": 1.3}`, solo el payaso) y 4d ajuste IK, y el GLB sale de `bake_glb_actions.py`. Si otro agente vuelve a correr el flujo sobre un personaje, estas correcciones se reaplican solas.

**Codos rectos (2026-11-09, tarde).** Jesús vio que en muchos clips el brazo quedaba recto, tipo zombi: el ajuste IK empujaba la mano hacia adelante hasta sacarla del cuerpo y, con brazos cortos, llegaba al tope del alcance. `ikfix_rig.py` ahora limita la distancia hombro→mano al 92 % del brazo (unos 45° de codo como mínimo) y, al llegar a ese radio, el empuje sigue tangencial: la mano se desliza alrededor de la cabeza o el pecho en vez de estirar el codo (parámetro `alcance`, tercer argumento). Además de verse mejor, deslizar saca más manos que estirar. Cuadros con la mano dentro de la cabeza / del torso, brazo recto → alcance 0.92: payaso 86 / 312 → 72 / 232, fontanero 5 / 323 → 3 / 237, rata 185 / 298 → 86 / 81 y ogro 236 / 445 → 110 / 197. Los cuatro rigs y GLB se rehicieron; `ik_reach` queda en la entrada `arms` de cada `-rig.json`.

**Brazos abiertos en las caminatas (2026-11-09, tarde).** Jesús vio que en `walk_forward` y `walk_back` el brazo derecho de la rata, el ogro y el fontanero se doblaba hacia afuera. Causas, en orden de hallazgo: la cola de la rata pesa con `Hips` y estiraba la elipsoide de la panza hasta su lado derecho; las elipsoides de la cabeza se ajustaban a la caja de todos los vértices de `Head` (orejas, conos), llegaban hasta el hombro y, al inclinarse la cabeza en la caminata, barrían la zona del pecho; y la normal de una elipsoide al costado del pecho apunta hacia afuera y abajo, así que el empuje abría el brazo, y cuando el brazo cuelga casi recto el límite de alcance convierte cualquier empuje en un deslizamiento lateral. En las caminatas la mano sí entra 1,5–3 cm en la parte alta del pecho, pero eso no se nota en un peluche y la corrección sí.

`ikfix_rig.py` quedó así: la cabeza se prueba contra la **malla posada** (las caras con peso dominante de `Head`, con los bordes abiertos tapados por abanicos en cada cuadro para que la paridad de seis rayos casi horizontales sea exacta aunque la cabeza vaya unida al cuerpo o tenga la boca abierta), con una holgura de 3 cm: el puño queda delante de la cara, no pegado. Las elipsoides del torso se miden solo sobre la pieza grande del cuerpo, simétricas en x y un 15 % más angostas a los lados, y una mano al costado del cuerpo no cuenta. Los contactos bajo la articulación de la cabeza (la parte alta del pecho, que en mallas unidas pesa como cabeza) y los del torso se toleran hasta unos 2 cm de penetración y se empujan solo hacia adelante. El desplazamiento nunca tiene componente lateral respecto al torso, y el plano del codo es el del clip cuando el brazo viene flexionado y el natural (codo hacia atrás) cuando viene casi recto. Un intento intermedio con el rayo hacia arriba se engañaba con la boca abierta del payaso y el tubo del fontanero; otro con tolerancias de 6 cm volvía a dejar los puños pegados a la cara en `idle`. Los cuatro rigs y GLB se rehicieron: en las caminatas los brazos van flexionados hacia adelante sin abrirse y en `idle` y `block` los puños quedan delante de la cara con el codo doblado. Los ajustes quedan en la entrada `arms` de cada `-rig.json`.

## Cómo mantener esta bitácora

Actualizar la fecha, el archivo maestro si cambia, las pruebas verificadas y el siguiente paso cuando se termine una sesión de trabajo. Distinguir las pruebas de Blender de las confirmadas en Mixamo o en la arena. Registrar las propuestas de skins como ideas hasta que haya un piloto aprobado.
