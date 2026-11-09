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
updated: 2026-11-08
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

### 2. UV y textura (`projtex.py`) → `final-<slug>-texturizado.blend`

- Desdobla con Smart UV Project en un atlas de 2048 × 2048.
- Separa las tres vistas de la lámina y quita el fondo blanco conectado al borde (conserva el blanco de ojos, dientes y franjas).
- Ajusta cada vista a la silueta de la malla con una búsqueda de escala y posición. El informe trae el solapamiento (`iou`); entre 0.87 y 0.95 es normal.
- Proyecta cada vista sobre la superficie, solo donde la superficie es visible desde esa cámara (z-buffer). Cada punto mezcla las vistas según cuánto las mira de frente. El otro costado usa el perfil reflejado, pero solo donde el cuerpo es simétrico; así un tubo o una cola de un solo lado no recibe la cara del perfil.
- Lo que ninguna vista ve (axilas, plantas, parte de arriba de la cabeza, el interior de la boca) toma el color del punto pintado más cercano.
- Arma un material con el mapa de color y rugosidad 0.65.

La lámina trae la luz del render de la IA, así que la textura conserva algo de sombreado. Para el estilo de juguete actual alcanza.

### 3. Vistas (`views.py`) → `final-<slug>-vistas.png`

Frente, perfil, espalda y tres cuartos del modelo texturizado, debajo de la lámina original. **Revisar antes de seguir**: caras o rasgos repetidos en los costados, manchas de otro color en accesorios de un solo lado, bordes blancos y la parte de arriba de la cabeza. El render de Workbench se ve más oscuro que el GLB en el navegador.

### 4. Rig (`autorig.py`) → `final-<slug>-rig.blend` y `final-<slug>-rig.json`

Crea los 25 huesos `mixamorig:*` de Mixamo "No Fingers" a partir de la silueta, sin pasar por Mixamo, y los pesa con _bone heat_. Detalle del método en el README de `scripts/botargas/` y en la bitácora. Si el pesado falla, usa una copia remallada con vóxeles. Si también falla en esa copia, la rehace con vóxeles más gruesos.

Qué revisar en el informe: `warnings` vacío, y en `skin`, `proxy_unweighted` en 0. `unweighted_by_heat` alto es normal en mallas de Tripo con piezas sueltas.

Si el personaje tiene cola o accesorios en el lado izquierdo (+X), pasa `--side right` al final del comando para que mida el otro lado.

### 5. Clips (`bake_glb.py`) → `final-<slug>.glb`

Carga los 22 clips de `animations/` en el rig y exporta un GLB con la malla, el skin (como máximo 4 influencias por vértice), la textura y todos los clips como acciones a 30 fps.

## Revisión en el navegador

```
cd ~/Documents/Blender && python3 -m http.server 8765 --bind 127.0.0.1
```

Abrir `http://127.0.0.1:8765/visor/`. Para agregar un personaje, sumarlo a `CHARS` en `visor/index.html`. El visor tiene selector de clip, velocidad, pausa, cuadro a cuadro, huesos y pose T. Mirar sobre todo `attack_heavy_*`, `special_*`, `block` y `ko_*` a 0.3× y cuadro a cuadro, en hombros, panza, rodillas y la unión entre cabeza y cuello.

## Después del flujo

Lo que el flujo no hace y se decide por personaje:

- **Brazos cortos.** Los clips de Mixamo están pensados para brazos humanos. Con brazos cortos y manos grandes, los puños se meten en la cara y el pecho. En la botarga se alargó el brazo ×2.1 (`stretch.py` y `armtube.py`) y se corrigieron los clips con IK (`ikfix.py`). Cambia las proporciones, así que lo decide Jesús.
- **Pesos finos.** Si en el visor se ve un hombro, la panza o una rodilla mal, se corrige con `fixw.py` y `smoothw.py`, adaptados a las medidas del personaje.
- **Textura a mano.** Si la proyección deja una zona mal, se retoca el mapa `<Nombre> · Color` en Blender (Texture Paint) dentro de `final-<slug>-rig.blend` y se repite solo el paso 5. Los retoques de textura no tocan el rig.
