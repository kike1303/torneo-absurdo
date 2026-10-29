# @torneo/sff-converter

Convertidor **local** de personajes de MUGEN (SFF v1/v2 y AIR) al formato de assets v1 del renderer. Fase B de [002](../../docs/features/002-spike-renderer/spec.md) (RF-B1 a RF-B8). Es una herramienta de desarrollo: no es dependencia de `apps/web` y no entra en ninguna imagen (`tools/` está en `.dockerignore`).

## Regla de oro (ADR-0004)
- Los archivos de MUGEN y lo que sale del convertidor viven **solo** en `content/_local/` (ignorado por git). El convertidor se niega a escribir un archivo que `git check-ignore` no confirme como ignorado.
- Usa ids neutros (`local_1`, `local_2`), nunca el nombre del personaje.
- La evidencia en PRs, commits y Linear es **solo texto**: el reporte JSON (que no lleva rutas ni nombres de archivo) y las estadísticas. Sin capturas ni nombres de personajes de terceros.

## Uso
Desde la raíz del repo. `-s` (silencioso) evita que pnpm imprima la línea del comando, que sí lleva tus rutas.

```sh
# 1. Antes de convertir: solo cabeceras, nodos y AIR, sin decodificar píxeles.
pnpm -s --filter @torneo/sff-converter inspect -- \
  --sff content/_local/mugen/p1/char.sff --air content/_local/mugen/p1/char.air --map content/_local/mugen/p1/map.json

# 2. Convertir (por defecto escribe en content/_local/sprites/<id>/).
pnpm -s --filter @torneo/sff-converter convert -- \
  --sff content/_local/mugen/p1/char.sff --air content/_local/mugen/p1/char.air --map content/_local/mugen/p1/map.json
```
- Las rutas relativas se resuelven contra el directorio desde el que llamas a `pnpm` (`INIT_CWD`). `--out <dir>` cambia la salida, que también tiene que estar ignorada.
- stdout es un JSON. Un error sale en stderr como `{ "error": "<código>", "data": { … } }` con código de salida 1 (2 si es `internal.unexpected`). Si el manifest resultante no pasa `validateManifest`, stdout trae el reporte con `manifest_errors` y no se escribe nada.
- La salida queda marcada con un archivo vacío, `.sff-converter-output`. Al volver a convertir, el convertidor borra `manifest.json` y `sheet-N.png` **solo** en una carpeta con ese marcador. Si la carpeta tiene esos archivos y no tiene el marcador (por ejemplo, una salida de antes de que existiera), falla con `write.foreign_output` sin borrar nada: borra tú esa carpeta y vuelve a convertir. Un marcador que es un enlace simbólico da `write.symlink`, y uno que no es un archivo regular (una carpeta, por ejemplo) da `write.marker_invalid`.
- `inspect` cuenta los sprites por compresión (`<nombre>_unsupported` si no se puede decodificar) y, con el mapa, las acciones que faltan en el AIR y los sprites no soportados que usarían las animaciones mapeadas.

## El mapa
Lo escribes tú, junto a los archivos de MUGEN. Dice qué acción de MUGEN es cada animación nuestra.

```json
{
  "id": "local_1",
  "actions": { "attack_light": 200, "attack_heavy": 210, "special": 1000, "block": 130, "dodge": 700, "ko": 5150 },
  "impact_frames": { "attack_light": 2, "attack_heavy": 3, "special": 4 },
  "scale": 1,
  "palette_sprite": [0, 0]
}
```
- Por defecto: `idle: 0`, `walk: 20`, `hit: 5000`, `knockdown: [5050, 5100, 5110]`, `intro: 190` y `win: 180`. Puedes sobrescribirlos en `actions`.
- `knockdown` es la única que acepta una lista (de 1 a 8 acciones), que se reproducen una detrás de otra. En MUGEN la caída se reparte en varias acciones: la 5050 cae en el aire (la altura la pone la física, no el frame), la 5100 golpea el suelo y la 5110 queda tendido, que es el frame que el renderer sostiene hasta que se levanta. Si a tu personaje le falta alguna, el reporte lo anota y la cadena sigue con las demás.
- Obligatorias (dependen del personaje): `attack_light`, `attack_heavy`, `special`, `block`, `dodge` y `ko`. `impact_frames` da, para cada ataque, el índice del frame del AIR en el que golpea; el windup (la suma de los frames anteriores) no puede pasar de 400 ms.
- `scale` (0.25 a 4, por defecto 1) iguala alturas entre personajes. `palette_sprite` (grupo, imagen) solo cuenta en SFF v1 con paleta compartida: es el sprite que trae la paleta (si ves colores raros, prueba otro, por ejemplo `[9000, 0]`).
- Si `intro`, `walk` o `knockdown` no existen en el AIR, el reporte lo anota y el renderer usa su fallback.

## Qué hace con las animaciones
- `frame_ms = round(ticks × 1000 / 60)`, recortado a 16–2000 ms; `ticks = -1` es el último frame, sostenido (100 ms nominales). `idle` y `walk` hacen bucle desde `Loopstart`.
- El pivote es el eje del sprite (espejado con él si el AIR pide `H` o `V`) menos el desplazamiento del AIR. Se recortan los bordes transparentes y, si el pivote queda fuera, se rellena con transparente.
- Un frame de `knockdown` cuyo dibujo queda por encima de los pies se baja hasta el suelo (nota `convert.grounded`): el renderer no tiene física. Las demás animaciones no se tocan.
- Animaciones de más de 64 frames, o sin bucle de más de 3 s, se recortan con una nota. `Clsn*`, `Interpolate`, mezclas, escala y ángulo se ignoran y se cuentan en el reporte.
- Las hojas son de ≤ 2048×2048 (como mucho 4), con 1 px de margen.

## Formatos y límites
| | Soportado | No soportado (se reporta; error solo si una animación mapeada lo usa) |
|---|---|---|
| SFF v1 | PCX 8 bits, 1 plano, RLE; paleta propia, "misma paleta" y paleta compartida; sprites enlazados | PCX de 24 bits o con varios planos; paletas `.act` externas |
| SFF v2 | raw, RLE8, LZ5, PNG8, PNG24, PNG32; `ldata` y `tdata`; sprites y paletas enlazados | RLE5 y otros códigos; PNG entrelazado o de profundidad ≠ 8 |

Límites: SFF ≤ 64 MiB, AIR ≤ 1 MiB, mapa ≤ 64 KiB, ≤ 10 000 sprites y paletas, sprites de 1 a 1024 px por lado, ≤ 10 000 acciones y ≤ 1 000 frames por acción.

## Verlos en la arena
Con `just dev`, `/es/dev/arena` lista los personajes de `content/_local/sprites/` en el grupo "Locales" del selector. Solo existe en `vite dev`: el build y las imágenes no los sirven.

## Desarrollo
`just check-web` corre `tsc`, ESLint y Vitest de este paquete. Los tests generan sus fixtures (SFF v1/v2, PCX, LZ5, PNG y AIR sintéticos) en código, en `test/support/`, y nunca leen de la carpeta local de contenido. `src/` no usa reloj ni azar (ESLint), salvo `src/cli.ts`; `test/support/elapsed.ts` solo puede usar `performance` para medir el límite de 1 s por caso de RF-B4.
