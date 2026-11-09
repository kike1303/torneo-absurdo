---
id: "flujo-personajes-3d"
title: Flujo de un personaje 3D, de la malla al GLB animado
type: guide
status: draft
risk: low
author: claude
linear: null
depends_on: []
created: 2026-11-08
updated: 2026-11-09
---

# Flujo de un personaje 3D, de la malla al GLB animado

Pasos para llevar a un luchador nuevo desde la malla de Tripo hasta un GLB con textura, esqueleto y los clips comunes, listo para verlo en el navegador. El orden importa: **primero la geometría, después la textura y al final el rig**. Los pesos del rig van por vértice, así que si la malla cambia después del rig, el rig se rehace. La textura no mueve vértices, pero va antes del rig para revisar las deformaciones con el personaje ya pintado.

Los archivos de Blender, los FBX, las imágenes y los scripts viven fuera del repo, en `~/Documents/Blender/`. Los scripts están en `scripts/personajes/` (el flujo) y en `scripts/botargas/` (rig y clips). La historia de cada personaje va en la [bitácora](bitacora-personajes-3d.md).

## Entrada

En la carpeta del personaje (por ejemplo `clown/`), dos archivos con el mismo nombre:

| Archivo | Requisitos |
| --- | --- |
| `final-<slug>.fbx` | Malla de Tripo en pose T, de pie, mirando al frente. Brazos horizontales y separados del torso, piernas separadas desde la entrepierna. |
| `final-<slug>.png` | Lámina de tres vistas sobre fondo blanco, en este orden: frente, perfil (con la cara hacia la derecha) y espalda. La misma pose T y el personaje entero en cada vista. |

La carpeta se llama igual que el slug (`clown/final-clown.png`), en minúsculas. Las vistas sueltas se sacan de la lámina con `scripts/personajes/split_views.py`, que corta por el centro del blanco entre vistas, sin escalar, y escribe `final-<slug>-frente.png`, `-perfil.png` y `-espalda.png`. Lo que deja de usarse (láminas y mallas viejas, versiones previas del rig, respaldos `.blend1`) va a `_anterior/` dentro de la misma carpeta.

```
Blender -b --factory-startup --python scripts/personajes/split_views.py -- <carpeta>/final-<slug>.png
```

La lámina es la fuente de la textura, así que el modelo de Tripo debe salir de ella. Si la lámina y la malla difieren mucho (por ejemplo, un accesorio que solo está en una de las dos), ese accesorio queda mal pintado.

## Un solo comando

```
~/Documents/Blender/scripts/personajes/personaje.sh <carpeta> final-<slug> <Nombre>
```

Por ejemplo, `personaje.sh clown final-clown Payaso`. Corre los cinco pasos en Blender sin interfaz y escribe todo en la misma carpeta. Imprime una línea `RESULT` por paso, con las medidas que hay que revisar. Cada paso también se puede correr suelto.

## Pasos

### 1. Malla (`prep.py`) → `final-<slug>-malla.blend`

- Deja solo la malla del personaje. Los FBX de Tripo traen además el cubo, la cámara y la luz de la escena por defecto.
- Une vértices duplicados, quita caras degeneradas, cierra los huecos y recalcula las normales.
- Normaliza: alto 1.0, pies en Z=0, origen entre los pies, mirando a -Y y con las transformaciones aplicadas.
- Divide las aristas de brazos y piernas que miden más de 3 cm (`--max-edge`). Los triángulos largos se pliegan en el codo y la rodilla y dejan ver el interior. Con las cuatro primeras mallas, el total pasó de unos 10 500 a entre 11 800 y 14 600 triángulos.
- `--sym` refleja el lado +X sobre el -X. Solo sirve para personajes simétricos: no se usa con la rata (cola y orejas distintas), el fontanero (tubo de un lado) ni el payaso (conos).

Qué revisar en el informe: `nonmanifold_e` y `boundary_e` deberían quedar en 0 o cerca. Si quedan muchos, la malla tiene zonas enredadas de Tripo; el paso 4 lo tolera, pero conviene mirar dónde están. `islands` lista las piezas sueltas (ojos, dientes, conos), que son normales.

### 1b. Arreglos de malla (`meshfix.py`, opcional) → el mismo `final-<slug>-malla.blend`

Si en la carpeta hay un `mesh-fixes.json`, el flujo aplica sus operaciones a la malla antes de la textura. Hoy existe `mirror`: copia la pieza suelta más cercana a un punto reflejada en un plano, con escala y desplazamiento opcionales. Sirve para lo que Tripo no modeló y la lámina sí tiene (los dientes de arriba del payaso salen de reflejar los de abajo sobre el centro de la boca). Como cambia la malla, va antes del rig.

### 2. UV y textura (`zonetex.py`) → `final-<slug>-texturizado.blend`

Pinta colores planos por pieza. No copia los píxeles de la lámina: el dibujo nunca calza exacto con la malla, y copiarlo corría la nariz a los ojos y embarraba cejas, costados y piernas (fue el primer método, `projtex.py`, que quedó descartado).

- Desdobla con Smart UV Project en un atlas de 2048 × 2048.
- Las pinturas salen de `paints.json`, en la carpeta del personaje: `{"piel": ["#fdb785", "#e48b57", ...], "cono": [...]}`. El primer color de cada pintura es el que se pinta; los demás son sus tonos con luz y con sombra en la lámina, y cada píxel va a la pintura del tono más cercano. Sin el archivo, el script agrupa colores parecidos él solo, pero con láminas como la del payaso (piel y conos del mismo tono) parte la piel en dos o la junta con los conos; por eso la paleta se escribe a mano, a partir de los 16 colores más frecuentes de la lámina.
- Dos llaves opcionales de `paints.json` dan la estructura que la lámina no dice sola. `_big` lista las pinturas que pueden caer sobre las piezas grandes (cuerpo, cabeza); las otras (conos, blanco de ojos, dientes) solo existen en piezas sueltas y sus votos sobre el cuerpo se descartan, porque el dibujo de un cono siempre se pasa unos píxeles sobre la cabeza. `_spikes` describe las puntas con franjas: las dos pinturas, el perfil de base a punta (`"aaaaaaaaaabbbaaaabbbaaaa"`) y los rangos de largo y radio en metros; todas las piezas lisas (pliegue de 50°) de ese tamaño se pintan con el perfil, estén sueltas o pegadas a la cabeza. La lengua no se pinta de cono porque sus votos no tienen esa pintura.
- Ajusta cada vista a la silueta (`fit`, entre 0.87 y 0.95 es normal) y después la corrige por zonas: compara los bordes del modelo con los del dibujo en cuadros de 40 px y corre cada cuadro lo que haga falta (`shift_px`, hasta unos 20 px).
- Corta la malla en piezas por sus pliegues (`--crease`, 20° por defecto) y por las partes sueltas. Cada pieza toma la pintura que más votos recibe de las vistas que la ven de frente. Las piezas grandes con más de una pintura (cuerpo y short, cabeza y parches) se pintan punto por punto, sin las pinturas con menos del 3 % de sus votos; después se borran las manchas de menos de 2 × 2 cm (`blobs_removed`, ruido de desalineación), un desenfoque gaussiano de 1 cm sobre las etiquetas redondea los bordes (sin escalones de vóxel) y se repite la limpieza de manchas (`blobs_removed_2`).
- Más llaves opcionales de `paints.json` para los casos que lo anterior no resuelve. `_min_blob` da el tamaño mínimo en m² por pintura (`{"spot": 0.0007, "fur": 0.003}`): sube el del pelo para que las manchas no tengan huecos del color del cuerpo y baja el de la mancha para conservar flores pequeñas. `_fine` (número de triángulos) une a las piezas mixtas de su isla las piezas pequeñas que salen de una superficie rugosa (los bultos del pelo del ogro), para que una mancha dibujada corra por encima de ellos en vez de pintarse bulto a bulto; **no sirve en personajes lisos**: en la rata borra el borde de la boca y de las orejas. `_close` aplica un cierre morfológico (radio en metros) a una pintura rota en trozos. `_band` lista las pinturas que son una banda de alturas alrededor del cuerpo (por defecto `shorts`, si existe): por cada azimut se busca el intervalo de alturas que mejor explica las etiquetas, se suavizan sus bordes alrededor y el short queda con una cintura y unas boca-piernas continuas (`band` en el informe, con las alturas medias). Solo actúa cuando el short comparte pieza con el cuerpo; si es pieza propia, `band` sale `null`.
- Las piezas sueltas pequeñas (cejas, dientes, lengua, hasta el 2 % de los triángulos) se pintan enteras con la pintura que más votos recibe: pintarlas punto por punto dejaba que el blanco del ojo o la piel se colaran en sus bordes.
- `_bumps` (`{"size": 0.03, "paint": "#e9a070"}`): las piezas sueltas de hasta `size` metros pegadas a una pieza grande (las costuras de la cara) toman esa pintura, un nombre de la paleta o un color hex propio que ningún píxel de la lámina vota. Sus propios votos son ruido: una costura mide unos píxeles en la lámina. Las pupilas, que flotan sobre el ojo y no sobre la cabeza, no cuentan.
- Con `_spikes`, un cono que es pieza suelta se pinta entero por su eje, anillo de la base y tapa incluidos; si una pieza suelta tiene dos conos, cada texel va al cono más cercano. La loma de cabeza sobre la que se apoya también es base: lo que sobresale más de 1 cm de la esfera ajustada a la cabeza, cerca del cono y conectado a él, más la grieta de 4 mm donde el cono toca la cabeza. Un umbral menor mancha la cabeza (la cabeza no es una esfera), así que las lomas que se escapan se agregan a mano en `_spikes.mounds` (`[[x, y, z, r], ...]`, en metros). En un cono que nace de la cabeza, su pieza a 50° entera es cono, el bulto de la base incluido (Tripo modela la base del cono como un bulto, y en el diseño es del color del cono), igual que las piezas chicas a 20° que la tocan (el anillo de la base, la tapa). La cabeza es una pieza grande y no se toca.
- La boca: una pintura `mouth` con el rojo muy oscuro del fondo de la boca (y sus tonos casi negros), en `_big`, para que el fondo visible desde el frente vote boca y no negro, y `_cavity` (`{"paint": "mouth", "box": [[x0, y0, z0], [x1, y1, z1]]}`, en metros) para el piso y los lados, que las vistas no ven y tomaban la piel de al lado. La caja se mide en la malla: el perfil frontal de la cabeza dice dónde abre la boca.
- Tres llaves más para lo que la votación no resuelve y conviene decir a mano, medido en la malla: `_parts` (`[{"at": [x, y, z], "paint": "black", "crease": 35}]`) pinta entera la pieza que contiene el punto, como la nariz de la rata, que salía mitad negra y mitad pelaje; `_zones` (`[{"box": [...], "replace": {"tan": "fur"}}]`) cambia una pintura por otra dentro de una caja, en las piezas grandes, para los dibujos que se proyectan donde no van (el parche de la rata, pieza suelta de atrás, aparecía también sobre la frente); y `_bumps.except_on` lista las pinturas sobre las que una costura toma el color de la pieza de abajo (las del parche rojo).
- Ojos: entre las piezas blancas con algo oscuro que miran al frente, las dos con más blanco son los ojos (`MAX_EYES`), aunque estén hundidas en la cabeza como un casquete. Si sobre la bola hay una pieza suelta chica (el bulto de la pupila que dejó el modelador), la pupila va ahí y esa pieza se pinta oscura; si no, hacia donde el dibujo tiene lo oscuro.
- Lo que ninguna vista ve toma la pintura de su vecino.
- Arma un material con el mapa de color y rugosidad 0.6.

El informe trae las pinturas (`paints`, con sus nombres en `paint_names`), cuántos ojos y franjas encontró (`eyes`, `spikes`) y las manchas borradas (`blobs_removed`). Las vistas de Workbench se ven más oscuras que la lámina; en el visor la exposición está subida para que los colores planos se lean como en el dibujo.

### 3. Vistas (`views.py`) → `final-<slug>-vistas.png`

Frente, perfil, espalda y tres cuartos del modelo texturizado, debajo de la lámina original. **Revisar antes de seguir**: caras o rasgos repetidos en los costados, manchas de otro color en accesorios de un solo lado, bordes blancos y la parte de arriba de la cabeza. El render de Workbench se ve más oscuro que el GLB en el navegador.

### 4. Rig (`autorig.py`) → `final-<slug>-rig.blend` y `final-<slug>-rig.json`

Crea los 25 huesos `mixamorig:*` de Mixamo "No Fingers" a partir de la silueta, sin pasar por Mixamo, y los pesa con _bone heat_. Detalle del método en el README de `scripts/botargas/` y en la bitácora. Si el pesado falla, usa una copia remallada con vóxeles. Si también falla en esa copia, la rehace con vóxeles más gruesos.

Qué revisar en el informe: `warnings` vacío, y en `skin`, `proxy_unweighted` en 0. `unweighted_by_heat` alto es normal en mallas de Tripo con piezas sueltas.

Si el personaje tiene cola o accesorios en el lado izquierdo (+X), pasa `--side right` al final del comando para que mida el otro lado.

### 4b. Piezas rígidas (`rigidparts_rig.py`) → el mismo `final-<slug>-rig.blend`

Las piezas sueltas chicas (costuras, dientes, ojos, botones) copian los pesos de la pieza grande a la que pertenecen, elegida por su hueso dominante, y la cabeza se queda solo con `Head` y `Neck`. Sin esto, el pesado por calor reparte una costura de la mejilla entre la cabeza, el cuello y el hombro y se alarga al mover la cabeza.

### 4c. Brazos (`stretch_rig.py`, opcional) y 4d. ajuste IK de los clips (`ikfix_rig.py`)

Los clips de Mixamo están pensados para brazos humanos: con cabezas grandes y brazos cortos, los puños se meten en la cara y el pecho. `ikfix_rig.py` carga los 22 clips, saca las manos de la cabeza, el pecho y la panza cuadro a cuadro con IK de dos huesos y guarda los originales como `<clip>_mixamo`. La cabeza se prueba contra la malla posada (con los bordes abiertos tapados) y el puño queda a 3 cm de la cara; el pecho y la panza son elipsoides del cuerpo y toleran unos 2 cm de contacto (una mano rozando el pecho en una caminata no se toca). La mano nunca se mueve hacia los lados ni se aleja del hombro más del 92 % del brazo: al llegar a ese radio se desliza, así el codo sigue flexionado y el brazo no se abre. Si además hace falta alargar el brazo, `arms.json` en la carpeta del personaje (`{"stretch": 1.3}`) lo hace antes del ajuste; cambia las proporciones, así que el factor lo decide Jesús. Solo el payaso lo necesitó (su cabeza es más ancha que los hombros); el fontanero, la rata y el ogro quedaron sin alargar. `collide_rig.py out.json` cuenta antes y después los cuadros con la mano dentro de la cabeza o del torso, y `frames_sheet.py out.png clip:cuadro,...` renderiza los cuadros que marque.

### 5. Clips (`bake_glb_actions.py`) → `final-<slug>.glb`

Exporta un GLB con la malla, el skin (como máximo 4 influencias por vértice), la textura y los 22 clips ya corregidos como acciones a 30 fps (sin los `_mixamo`).

### 6. Compresión (`gltfpack`) → el GLB que va a `content/models/`

El GLB de Blender se comprime con meshopt (`EXT_meshopt_compression`, TA-70) antes de entrar al repo. Los ajustes conservan todo lo que el renderer usa y no pierden calidad visible: precisión máxima de 16 bits en posiciones, UV, normales y curvas de animación, sin remuestrear los clips (`-af 0`, se quedan las claves originales a 30 fps y la interpolación `STEP`), con los nombres de nodos y materiales (`-kn -km`) y los `extras` (`-ke`, donde viven `tint_default` y `face_uv_rect`). Las texturas JPEG se copian tal cual (nada de KTX2 ni WebP: KTX2 choca con la CSP).

```
pnpm -s dlx gltfpack@1.3.0 -i final-<slug>.glb -o <slug>.glb -cc -vp 16 -vt 16 -vn 16 -at 16 -ar 16 -as 16 -af 0 -ac -kn -km -ke
```

En la botarga base baja de 1,86 MB a 1,07 MB (las texturas son 0,48 MB de ambos). Los ajustes por defecto de `gltfpack` (14 bits, rotaciones de 12 bits, clips remuestreados) ahorran solo 80 KB más, así que no compensan tocar la calidad. Después de comprimir, revisar en `/es/dev/arena` que la pelea se ve igual y que el tinte y la cara siguen funcionando.

## Revisión en el navegador

```
cd ~/Documents/Blender && python3 -m http.server 8765 --bind 127.0.0.1
```

Abrir `http://127.0.0.1:8765/visor/`. Para agregar un personaje, sumarlo a `CHARS` en `visor/index.html`. El visor tiene selector de clip, velocidad, pausa, cuadro a cuadro, huesos y pose T. Mirar sobre todo `attack_heavy_*`, `special_*`, `block` y `ko_*` a 0.3× y cuadro a cuadro, en hombros, panza, rodillas y la unión entre cabeza y cuello.

## Después del flujo

Lo que el flujo no hace y se decide por personaje:

- **Pesos finos.** Si en el visor se ve un hombro, la panza o una rodilla mal, se corrige con `fixw.py` y `smoothw.py`, adaptados a las medidas del personaje.
- **Textura a mano.** Si la proyección deja una zona mal, se retoca el mapa `<Nombre> · Color` en Blender (Texture Paint) dentro de `final-<slug>-rig.blend` y se repite solo el paso 5. Los retoques de textura no tocan el rig.
