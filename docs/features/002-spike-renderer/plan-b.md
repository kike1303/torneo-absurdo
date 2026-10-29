---
id: "002"
title: Spike de renderizado de peleas — fase B
type: plan
status: done
risk: medium
author: frontend-engineer
reviewers: [architect]
approvals:
  - by: architect
    date: 2026-10-27
linear: TA-2
depends_on: ["001"]
created: 2026-10-27
updated: 2026-10-28
---

# Plan: Spike de renderizado de peleas — fase B (desde spec.md 2026-10-27, con sus enmiendas técnicas)

> [intent](./intent.md) · [spec](./spec.md) · [plan-a1](./plan-a1.md) · [plan-a2](./plan-a2.md) · ADRs [0004](../../decisions/0004-personajes-homenaje.md), [0007](../../decisions/0007-i18n-desde-cero.md), [0009](../../decisions/0009-ui-design-system.md) (D4 y D5), [0012](../../decisions/0012-logs-y-cabeceras-de-seguridad.md) (B4.2 y B4.5)

**Alcance:** RF-B1 a RF-B9. El convertidor local `tools/sff-converter` (`@torneo/sff-converter`), el servido de `content/_local/sprites/` solo en `vite dev`, los personajes locales en el selector de `/[lang]/dev/arena`, la conversión de dos personajes por el owner con evidencia solo textual y su go/no-go. Rama `ta-2-spike-renderer-b` desde `main` **después de fusionar A2** (usa `FightStage`, `ArenaControls`, `fighters.ts` y `arena.spec.ts` de A2), un PR. **No toca** `crates/**`, `packages/protocol-ts/**`, `apps/server/**`, `infra/**`, `content/**` (salvo `content/_local/`, fuera de git), `packages/renderer/src/**` ni la opción `csp` de `apps/web/vite.config.ts`.

**Lo que ya existe y se reutiliza:** `validateManifest(json, { expectedId })` y los tipos de `@torneo/renderer/manifest` (A1, decisión 14), los imports relativos con `.ts` que Node carga con *type stripping* (A1, decisión 5), `pngjs` 7.0.0 y `@types/pngjs` 6.0.5 ya en el lockfile, el mapa de loaders de `FightStage.svelte` y `ARENA_FIGHTERS` de `#lib/features/arena/fighters.ts` (A2, decisión 10), y las reglas de `.gitignore` (`content/_local/`, `tools/sff-converter/input/`, `tools/sff-converter/output/`) y `.dockerignore` (`content/_local/`).

## Para el architect: precisiones de la spec

_Aceptadas por el architect el 2026-10-27. El texto de la enmienda técnica está en `spec-amendments-b.md` y se aplica a `spec.md` después del commit de A2. Este plan ya está alineado con ese texto. Las dejo como registro._

1. **El índice local solo se pide en desarrollo (RF-B7).** La spec dice que la ruta "pide el índice y, si responde 404 (build), solo ofrece los placeholders". En el build, un `fetch` con 404 deja en la consola de Chromium un error `Failed to load resource`, y el e2e de A2 exige "sin errores en la consola". Propongo que la llamada esté detrás de `import.meta.env.DEV`: en el build el código desaparece (Vite lo reemplaza por `false` y lo elimina), así que no hay petición ni error, y el selector solo lista `dummy_a` y `dummy_b`. El e2e sigue pidiendo `/__local-sprites/index.json` directamente y espera 404, y un chequeo nuevo comprueba que la cadena `__local-sprites` no aparece en `apps/web/build/` (decisión 14). En `vite dev` el 404 sigue tratándose como "sin locales", sin error.
2. **"Paletas de exactamente 256 colores" (RF-B4).** En SFF v1 se cumple tal cual (la paleta del PCX son 768 bytes). En SFF v2 un nodo de paleta declara `numcols`, que puede ser menor (paletas de 32 colores para sprites de 5 bits). Lo interpreto así: `1 ≤ numcols ≤ 256` y la longitud de los datos es exactamente `numcols × 4`. La paleta se completa hasta 256 con negro transparente, y un índice de píxel ≥ `numcols` es error (`sff.palette_index_out_of_range`). Si tiene que ser 256 estricto, rechazaría SFF v2 válidos.
3. **El pivote se mete dentro del frame.** El formato exige que el pivote caiga dentro del frame. En MUGEN, el eje (más el desplazamiento del AIR) suele quedar fuera del sprite, por ejemplo en saltos o en caídas. El convertidor rellena el frame con píxeles transparentes hasta que el pivote quede dentro (decisión 9). No cambia el formato.
4. **Una excepción más al reloj, solo en un archivo de test.** RF-B4 exige que "cada caso tarde < 1 s". Vitest no puede cortar un test síncrono por timeout: el test termina y pasa aunque haya tardado 5 s. Por eso hace falta medir. `test/support/elapsed.ts` es el único archivo que puede usar `performance.now()`, además de `src/cli.ts` (decisión 13).
5. **Dos ayudas para el go/no-go, sin texto nuevo en pantalla.** La primera es `pnpm --filter @torneo/sff-converter inspect`, que solo lee cabeceras. Le dice al owner antes de convertir si un personaje usa algo no soportado (decisión 11). La segunda es `?pace=<factor>` en `/dev/arena`, que reescala el guion para probar otra separación media entre golpes (RF-B9: "puede probar variantes del guion"). La separación media resultante sale en el JSON de "copiar estadísticas" (decisión 17). Si prefieres no tenerlas, se quitan sin tocar el resto.

## Alcance de formatos (SFF v1 y v2)

La spec fija los dos (RF-B3) y los mantengo. El owner elige qué personajes convertir, y los de la comunidad se reparten entre la era de WinMUGEN (v1) y la de MUGEN 1.x/IKEMEN (v2). Con un solo formato, la elección quedaría limitada antes de saber qué personajes sirven para el go/no-go. Dentro de cada formato recorto lo que un spike no necesita:

| Formato | Soportado | Fuera (se reporta con código, no aborta si ninguna animación mapeada lo usa) |
|---|---|---|
| SFF v1 (`1.01`) | PCX 8 bits, 1 plano, RLE; paleta propia o "igual a la anterior"; modo de paleta compartida; sprites enlazados | PCX de 24 bits o con varios planos (`pcx.unsupported`); paletas `.act` externas (ver "Alternativas") |
| SFF v2 (`2.00` y `2.01`) | raw (0), RLE8 (2), LZ5 (4), PNG8 (10), PNG24 (11), PNG32 (12); `ldata` y `tdata`; sprites y paletas enlazados | RLE5 (3) y cualquier otro código (`sff.unsupported_compression`); PNG entrelazado o con profundidad ≠ 8 (`png.unsupported`) |
| AIR | `[Begin Action N]`, frames `grupo, imagen, x, y, ticks[, flip]`, `Loopstart`, `-1` en `ticks` y en `grupo` (frame vacío) | `Clsn*`, `Interpolate *`, mezcla (`A`, `S`, …), escala y ángulo de 1.1: se ignoran y se cuentan en el reporte |

**Referencias de formato** (para quien implemente): la documentación de formatos de Elecbyte que viene con MUGEN 1.0/1.1 (`formats.txt`, sección SFFv2, incluido el pseudocódigo de RLE8 y LZ5) y, solo como lectura para comparar, el decodificador de IKEMEN GO (`src/image.go`, MIT). No se copia código. Las cabeceras que el plan da a continuación son las que hay que verificar contra esas fuentes en el paso 3.

- **SFF v1:** cabecera de 512 bytes: firma `ElecbyteSpr\0` (12), versión en los bytes 12–15 (`verhi` en el 15), grupos (u32 @16), imágenes (u32 @20), offset del primer subarchivo (u32 @24), tamaño del subencabezado (u32 @28, = 32) y tipo de paleta (u8 @32, 1 = compartida). Cada subarchivo empieza con un subencabezado de 32 bytes: siguiente offset (u32), longitud del PCX (u32, 0 = enlazado), eje x/y (i16, i16), grupo (u16), imagen (u16), índice enlazado (u16), "misma paleta que el anterior" (u8) y 13 bytes de comentario. Después vienen los datos PCX: cabecera de 128 bytes (`xmin..ymax` @4–11, `nPlanes` @65, `bytesPerLine` @66), RLE con `(b & 0xC0) == 0xC0 → run = b & 0x3F` y, al final, opcionalmente, `0x0C` más 768 bytes RGB.
- **SFF v2:** cabecera de 512 bytes con `verhi = 2` en el byte 15, y en orden u32: offset de nodos de sprite @36, número de sprites @40, offset de nodos de paleta @44, número de paletas @48, `ldata` offset y longitud @52 y @56, `tdata` offset y longitud @60 y @64. Nodo de sprite de 28 bytes: grupo, imagen, ancho, alto (u16), eje x/y (i16), índice enlazado (u16), formato (u8), profundidad (u8), offset de datos (u32), longitud (u32, 0 = enlazado), índice de paleta (u16) y flags (u16; bit 0: `tdata`). Nodo de paleta de 16 bytes: grupo, imagen, `numcols`, índice enlazado (u16), offset (u32) y longitud (u32). En los formatos comprimidos, los datos empiezan con la longitud descomprimida (u32), que se compara con lo esperado.
- **LZ5:** un byte de control da 8 banderas, de la menos a la más significativa. Con la bandera en 0 viene un paquete RLE: corto (3 bits de corrida y 5 de color) o largo (`d & 0xE0 == 0` → corrida = siguiente + 8). Con la bandera en 1 viene un paquete LZ: corto (6 bits de longitud y offset armado con los 2 bits altos "reciclados" de varios paquetes) o largo (`d & 0x3F == 0` → offset de 10 bits y longitud = siguiente + 3).

## Decisiones de implementación

1. **Paquete sin build, con *type stripping*.** `tools/sff-converter/package.json`:
   - `@torneo/sff-converter`, `private`, `type: module`, `license: AGPL-3.0-only`;
   - `dependencies`: `pngjs` 7.0.0 y `@torneo/renderer` `workspace:*`;
   - `devDependencies`: las de la tabla de versiones;
   - scripts: `convert` e `inspect` (`node --experimental-strip-types --disable-warning=ExperimentalWarning src/cli.ts <subcomando>`, el mismo flag que `gen:placeholders` de A1, decisión 13, porque `engines` admite 22.17), y `typecheck`, `lint` y `test`.

   Un solo `tsconfig.json` (`src`, `test`, `vitest.config.ts` y `eslint.config.js`) con `types: ["node"]`, `lib: ["ES2023"]` sin DOM, `module`/`moduleResolution: "NodeNext"`, `strict`, `noUncheckedIndexedAccess`, `allowImportingTsExtensions`, `noEmit`, `erasableSyntaxOnly` y `verbatimModuleSyntax`. Es una herramienta de Node: no hace falta separar núcleo y tests, como en el renderer.
2. **Solo `./manifest` del renderer.** ESLint prohíbe `@torneo/renderer` y cualquier subpath que no sea `@torneo/renderer/manifest`, además de `pixi.js`, `pixi.js/*`, `svelte`, `@sveltejs/*` y `@torneo/protocol`. Así el convertidor no carga Pixi aunque esté en el grafo de instalación, porque `@torneo/renderer` lo declara.
3. **Errores como datos y sin red de seguridad.** `ConverterError extends Error` con `code` (`cli.*`, `map.*`, `sff.*`, `pcx.*`, `png.*`, `lz5.*`, `rle8.*`, `air.*`, `convert.*` y `write.*`) y `data: Record<string, string | number>`. `message` es el propio `code`, sin texto para humanos (ADR-0007). Los `ManifestError` de la validación final se devuelven tal cual. **Ningún módulo de `src/` salvo `cli.ts` tiene un `try/catch` genérico.** El único `catch` permitido envuelve `zlib.inflateSync` y traduce su error a `png.corrupt` o `png.too_large`. Si `convert()` atrapara todo y lo convirtiera en `ConverterError`, las mutaciones de RF-B4 pasarían en vacío. `cli.ts` imprime `{"error": code, "data": …}` en stderr y sale con 1; un error que no es `ConverterError` sale con 2 y `internal.unexpected`, sin pila ni rutas.
4. **Lector con límites (RF-B4).** `src/reader.ts` define `ByteReader` sobre un `Uint8Array`, con `u8`, `u16`, `i16`, `u32`, `bytes(n)` y `at(offset)`. Antes de leer comprueba `offset + n ≤ length` con aritmética entera (`n` y `offset` son `Number.isSafeInteger` y ≥ 0) y lanza `sff.out_of_range` con `{ offset, length }`. `slice(offset, length)` crea un sublector acotado, de modo que el decodificador de un sprite nunca ve bytes fuera de sus datos. Los límites viven en `src/limits.ts`: SFF ≤ 64 MiB, AIR ≤ 1 MiB, mapa ≤ 64 KiB, ≤ 10 000 sprites y ≤ 10 000 paletas, sprites de 1 a 1024 px por lado, ≤ 10 000 acciones, ≤ 1 000 frames por acción y líneas del AIR de ≤ 4 096 caracteres. **El tamaño de cada archivo se comprueba con `fs.stat` antes de leerlo.**
5. **Recorridos iterativos y acotados.**
   - La lista enlazada de subarchivos de v1 se recorre exactamente `nImages` veces, con un `Set` de offsets visitados (`sff.offset_cycle`).
   - Los sprites y las paletas enlazados se resuelven con un bucle de como mucho `count` pasos (`sff.link_cycle` si no termina, y `sff.link_out_of_range` si apunta fuera).
   - El AIR se lee línea a línea, sin expresiones regulares con retroceso costoso: anclas simples y `split(',')`.

   No hay recursión sobre datos de entrada.
6. **Decodificadores con la salida preasignada.** Cada decodificador recibe `Uint8Array(w × h)` (o `bytesPerLine × h` en PCX) y un índice de escritura. Una corrida que no cabe se recorta al espacio libre y termina, nunca escribe más allá. Si la entrada se acaba antes de llenar la salida, el error es `*.truncated`. En LZ5, una referencia hacia atrás con `offset > escrito` es `lz5.bad_offset`. Los bytes que sobran cuando la salida ya está llena se ignoran, porque los codificadores de Elecbyte a veces rellenan. En v2, la longitud descomprimida declarada tiene que coincidir con `w × h` (`sff.size_mismatch`).
7. **PNG decodificado en casa, `pngjs` solo para escribir.** `pngjs` expande la paleta a RGBA y pierde los índices. Sin índices no se sabe qué píxel es el índice 0, que es el transparente en MUGEN, y no se puede aplicar la paleta del nodo de sprite.
   - `src/png.ts` recorre los chunks con el lector acotado e ignora el CRC. Exige `IHDR` primero, con ancho y alto iguales a los del nodo, profundidad 8, sin entrelazado y tipo de color 3, 2 o 6 según el formato 10, 11 o 12 (`png.format_mismatch`).
   - Concatena los `IDAT` y los infla con `zlib.inflateSync(idat, { maxOutputLength: h × (1 + w × bpp) })`.
   - Exige esa longitud exacta y deshace los filtros 0 a 4 (`png.bad_filter` para cualquier otro).
   - En PNG8, aplica la paleta del nodo de sprite y descarta la `PLTE` embebida (riesgo 3).
8. **Paletas.**
   - **v1:** cada sprite usa su paleta propia (el `0x0C` más 768 bytes al final del PCX) si la trae y no tiene "misma paleta". Si no, usa la última resuelta en orden de archivo. En **modo compartido** (tipo 1), los sprites con "misma paleta" usan la del sprite que el mapa nombra en `palette_sprite` (por defecto `[0, 0]`). Un sprite sin paleta posible da `sff.missing_palette`.
   - **v2:** el índice de paleta del nodo de sprite (decisión de precisión 2).
   - En los formatos indexados, el índice 0 da alfa 0 y el resto, alfa 255. PNG24 es opaco y PNG32 conserva su alfa. La salida es RGBA sin premultiplicar.
9. **Del AIR al manifest (RF-B5).**
   - **Decodificación perezosa:** solo se decodifican los sprites que usan las acciones mapeadas, uno a la vez. Un sprite no soportado que nadie usa queda como nota en el reporte.
   - **Frames:**
     - el pivote dentro del sprite es `eje − desplazamiento del AIR`;
     - `H`/`V` del AIR se aplican espejando píxeles y pivote;
     - después se recortan los bordes transparentes;
     - por último se rellena con transparente hasta que el pivote quede dentro (`convert.pivot_too_far` si el frame pasa de 1024 px por lado);
     - un frame con grupo `-1`, o sin píxeles visibles, queda en 1×1 transparente con el pivote en (0, 0).
   - **Deduplicación:** la clave es `(sprite, flipH, flipV, pivote final)`, así que dos frames del AIR que solo difieren en `ticks` comparten frame.
   - **Tiempo:** `frame_ms = round(ticks × 1000 / 60)`, recortado a `[16, 2000]`, con la nota `convert.frame_ms_clamped`. Con `ticks = 0`, se usa un tick y se agrega una nota.
   - **Bucles:**
     - `idle` y `walk` llevan `loop: true` y, si la acción tiene `Loopstart`, usan los frames desde ahí;
     - el resto lleva `loop: false` con todos los frames hasta el primer `ticks = -1` inclusive, que se sostiene con 100 ms nominales (los frames siguientes son inalcanzables y se descartan con una nota);
     - una animación sin bucle que pasa de 3 000 ms, o cualquiera de más de 64 frames, se recorta con la nota `convert.animation_truncated`.
   - **Ataques:** `impact_frame` sale de `impact_frames` del mapa. Si el windup pasa de 400 ms, el error es el `ManifestError` de `validateManifest`, y el owner ajusta el mapa.
   - **Salida:** `pixel_art: true` y `scale` del mapa. Una acción mapeada que no está en el AIR es error si la animación es obligatoria (`air.missing_action`). Si es opcional (`intro`, `walk` o `knockdown`, que el formato resuelve con su fallback), solo deja una nota.
10. **Empaquetado (RF-B5).** `src/pack.ts`, puro: algoritmo de estantes con ancho máximo de 2048, 1 px de margen transparente entre frames, orden determinista (alto descendente, después ancho descendente y después clave), hoja nueva cuando no cabe la altura, y el alto y el ancho de cada hoja recortados a lo usado. Más de 4 hojas da `convert.too_many_sheets`. Antes de decodificar, si la suma de las áreas de los sprites mapeados (con su tamaño de cabecera) pasa de 4 × 2048², el error es `convert.too_large` (cota de memoria: ≤ 64 MiB de RGBA en el peor caso).
11. **CLI (RF-B2).** `src/cli.ts`, con `node:util` `parseArgs` en modo estricto:
    - Descarta un `--` inicial, porque `pnpm run x -- …` lo pasa al script.
    - Antes de leer, `cli.ts` hace `fs.stat` de cada entrada (este es el `fs.stat` de la decisión 4). ENOENT da `cli.file_not_found` y EACCES o EPERM dan `cli.file_unreadable`, los dos con `{ arg: 'sff' | 'air' | 'map' }` y sin la ruta. Una entrada que no es un archivo regular da `cli.not_a_file`, y una que supera su límite, `cli.file_too_large` con `{ arg, limit }`. Va dentro de `cli.ts`, el único módulo que admite `catch` además de `png.ts`.
    - Las rutas relativas se resuelven contra `process.env.INIT_CWD` (el directorio desde el que el owner llamó a `pnpm`) y, si no existe, contra `process.cwd()`. `--filter` ejecuta el script en `tools/sff-converter`, y sin esto `--sff content/_local/…` apuntaría a otro lugar.
    - `convert --sff --air --map [--out]`. `--out` por defecto es `<raíz del repo>/content/_local/sprites/<id>/`, con la raíz sacada de `import.meta.url`. Imprime en stdout un reporte JSON: `id`, versión de SFF, número de sprites leídos y decodificados, frames, hojas con su tamaño, frames por animación, notas `{ code, data }` agregadas por código y errores de manifest si los hubo. **Sin rutas ni nombres de archivo**: es lo que el owner pega en el PR (RF-B8).
    - `inspect --sff [--air] [--map]` solo lee cabeceras, nodos y el AIR, y no decodifica píxeles. Reporta la versión, la cuenta de sprites por formato de compresión, las paletas y, si recibe el mapa, qué acciones mapeadas faltan y qué sprites no soportados usarían.
12. **Escritura (RF-B5).** `src/write.ts`, **sin ningún `try/catch`** (decisión 3). Todas las comprobaciones van antes de tocar el disco, en este orden:
    1. Valida con `validateManifest(manifest, { expectedId: id })`.
    2. Busca el **ancestro existente más cercano** de la salida, subiendo con `fs.lstatSync(p, { throwIfNoEntry: false })`, que no lanza. Si ese ancestro o la salida, cuando ya existe, es un enlace simbólico, el código es `write.symlink`. Si el ancestro no es un directorio, es `write.not_a_directory`.
    3. **Por cada archivo que va a escribir** (`manifest.json` y `sheet-<n>.png`, con su ruta absoluta, aunque todavía no exista; `check-ignore` trabaja sobre rutas que no existen), corre `spawnSync('git', ['check-ignore', '-q', '--', <ruta>], { cwd: <ancestro existente>, env: <entorno sin GIT_DIR, GIT_WORK_TREE, GIT_INDEX_FILE, GIT_COMMON_DIR ni GIT_OBJECT_DIRECTORY>, shell: false, stdio: 'ignore' })`. `spawnSync` no lanza. Con `status === 0` el archivo está ignorado. Con `status === 1`, el código es `write.not_ignored`. Con cualquier otro `status`, con `signal` o con `error` (por ejemplo ENOENT si no hay `git`), es `write.git_unavailable`. El entorno se limpia porque el hook `pre-push` corre `just check` con variables `GIT_*` que apuntarían al repo real.
    4. Solo si todo pasó: `mkdirSync(salida, { recursive: true })`, borra los `manifest.json` y `sheet-*.png` viejos de esa carpeta (solo esos nombres) y escribe las hojas con `PNG.sync.write` y el manifest con `JSON.stringify(_, null, 2)` y un salto de línea al final.

    Así, una negativa nunca deja un directorio ni un archivo fuera de lo ignorado.
13. **ESLint del paquete.** `tools/sff-converter/eslint.config.js` exporta `createConfig()` como el renderer, con las mismas prohibiciones de azar y reloj de RF-A3 (`Date`, `performance`, `crypto`, `Math.random`, `getRandomValues`, `randomUUID`, más los imports `node:crypto` y `crypto`), y las listas completas por grupo:
    - todo el paquete: lo anterior más las restricciones de imports de la decisión 2;
    - `src/cli.ts`: sin las prohibiciones de reloj y azar (spec), aunque hoy no las usa;
    - `test/support/elapsed.ts`: solo se le permite `performance` (precisión 4).
14. **Servido solo en desarrollo (RF-B7).** `apps/web/vite/local-sprites.ts`:
    - `resolveLocalSpriteRequest(rawUrl): { kind: 'index' } | { kind: 'file'; id; file } | null` es pura y no toca el disco.
      - Corta la query y el fragmento.
      - Rechaza cualquier `%` y cualquier `\`: ningún id ni archivo válido necesita codificarse, así que `%2e%2e`, `%2f` y `%5c` mueren aquí sin decodificar nada.
      - Exige `^/__local-sprites/(?:index\.json|([a-z][a-z0-9_]{0,47})/(manifest\.json|sheet-[0-9]{1,3}\.png))$`.
    - `createLocalSpritesHandler(baseDir)` es el middleware de connect.
      - Lo que no empieza con `/__local-sprites/` pasa a `next()`.
      - Con `null`, un método distinto de `GET`/`HEAD` o un archivo que no existe, responde 404 con el cuerpo vacío.
      - Con un archivo, resuelve `path.join(baseDir, id, file)` y comprueba con `fs.realpath` que la ruta real queda dentro de `realpath(baseDir)`. Así un enlace simbólico dentro de `content/_local/sprites/` no saca nada de ahí. También comprueba que es un archivo regular de ≤ 32 MiB.
      - Responde con `Content-Type` (`application/json; charset=utf-8` o `image/png`), `X-Content-Type-Options: nosniff` y `Cache-Control: no-store`. El handler no agrega cabeceras CORS: es del mismo origen, como pide ADR-0012. En Vite 8.3.5, el middleware registrado en `configureServer` corre **después** de `rejectInvalidRequestMiddleware`, `cors` y `hostValidationMiddleware`, que se registran antes. Es lo que queremos: un `Host` ajeno recibe 403 antes de llegar al handler (protección contra DNS rebinding), y el `cors` de Vite solo responde `Access-Control-Allow-Origin` a orígenes de loopback.
      - `index.json` lista los subdirectorios con id válido (hasta 100, en orden) cuyo `manifest.json` (≤ 1 MiB) pasa `validateManifest(_, { expectedId })`, como `{ "ids": [...] }`. Si `baseDir` no existe, responde `{ "ids": [] }`.
    - `localSprites()` devuelve `{ name: 'torneo:local-sprites', apply: 'serve', configureServer(server) { server.middlewares.use(handler) } }`. El middleware se registra directamente, no en el *post hook*, para correr antes que Kit y que Vite, y el plugin va **primero** en `plugins` de `vite.config.ts`. No implementa `configurePreviewServer`, así que tampoco existe en `vite preview`. `baseDir` es `<raíz>/content/_local/sprites`, calculado desde `import.meta.url`. La CSP no cambia: `connect-src 'self'` e `img-src 'self'` ya cubren el mismo origen.
    - **En el cliente:** `#lib/features/arena/local-fighters.ts` exporta `loadLocalFighterIds(fetchFn)` y `localAssetUrl(id, file)`. Todo el cuerpo está detrás de `if (!import.meta.env.DEV) return []` (precisión 1), con las URL `/__local-sprites/…` dentro del bloque de desarrollo, para que el build las elimine. Un 404 o un JSON que no es `{ ids: string[] }` con ids válidos da `[]`, sin error.
    - **Chequeo de que no llega al build:** `scripts/check-no-local-sprites.mjs` recorre `apps/web/build/` y falla si algún archivo contiene `__local-sprites` o `torneo:local-sprites`. Corre en `just e2e` justo después del build, junto a `check-renderer-split`, y tiene un test con fixtures en `scripts/__tests__/`.
15. **Arena (sobre A2).**
    - `+page.svelte` llama a `loadLocalFighterIds(fetch)` en `onMount` y pasa los ids a `ArenaControls` y a `FightStage`. La página sigue sin texto propio.
    - `ArenaControls` agrupa el `<select>` de cada lado en dos `<optgroup>`: `arena_fighters_placeholders` y, si hay locales, `arena_fighters_local`. Cada local se etiqueta con `arena_fighter_local({ id })`, que en `es` y en `en` da `Local · {id}`. El id es un dato que elige el owner.
    - En `FightStage.svelte`, los loaders de un id local cargan `manifest.json` con `fetch(localAssetUrl(id, 'manifest.json'))`. `resolveAsset(id, file)` devuelve `localAssetUrl(id, file)` para las hojas. El nombre del HUD sale de la misma etiqueta.
    - Un manifest local inválido termina en `error: asset_invalid`, el estado de A2, con reintentar, porque `load` valida.
    - Cambiar un lado a un id local reemplaza `fighters.<lado>.asset` de la timeline antes de `load`, como A2 ya hace con los placeholders.
16. **Dockerfile, contexto e imagen (RF-B1, "Áreas de preocupación" 10).**
    - `.dockerignore` agrega `tools/`: nada de `tools/` entra en el contexto de construcción, tampoco `input/` ni `output/` con material de MUGEN.
    - `scripts/check-compose.mjs` suma `tools/` a `DOCKERIGNORE_REQUIRED`, con su caso de test.
    - `scripts/check-dockerignore.sh` prueba también `tools/sff-converter/input`, con un canario que crea solo si no existe (mismo patrón `set -C` y borrado al salir) y que no puede aparecer en el contexto.
    - El paso 1 construye la imagen `web` (`just prod-local` sin tag y el job `images`) para ver si `pnpm install --frozen-lockfile --filter 'web...'` exige el `package.json` del importer nuevo. Espero que no, porque `web` no depende de él. Si lo exige, el `.dockerignore` reincluye solo ese archivo (`!tools/sff-converter/package.json`), el Dockerfile lo copia antes del `install`, el filtro `images` suma `tools/*/package.json` y lo anoto en "Desviaciones".
17. **Ayuda para RF-B9: `?pace=<factor>`.**
    - `#lib/features/arena/retime.ts` exporta `retimeTimeline(timeline, factor)`, pura. Acepta `factor` en `[0.8, 1.5]` con dos decimales (si no, la ruta lo ignora). Multiplica por `factor` el `at_ms` de todo cue posterior a la `intro`, contando desde el primer `strike`, y también `duration_ms`. Conserva la duración de cada `knockdown` y la separación de `ko` a `win`, y redondea a entero.
    - El resultado pasa por `validateTimeline` en `load`: un factor que rompa una regla termina en `timeline_invalid`, el estado de A2, y no se fuerza.
    - `meanStrikeGapMs(timeline)` entra en el JSON de "copiar estadísticas" como `mean_strike_gap_ms`, sin etiqueta visible.

## Versiones fijadas

Sin paquetes nuevos del registro. `minimumReleaseAge: 1440` y `strictDepBuilds: true` no se tocan, y `onlyBuiltDependencies` sigue en `[]`. El lockfile solo suma el importer `tools/sff-converter`. **El paso 1 lo comprueba:** el diff de `pnpm-lock.yaml` no agrega ninguna entrada a `packages:` ni a `snapshots:`, y lo pego en el PR.

| Paquete | Versión | Dónde | Notas |
|---|---|---|---|
| `pngjs` | `7.0.0` | `dependencies` | JS puro, ya en el lock (A1). Su `PNG.sync.read` acota el inflado, aunque aquí solo se usa `PNG.sync.write` (decisión 7) |
| `@torneo/renderer` | `workspace:*` | `dependencies` | solo `./manifest` (decisión 2) |
| `@types/pngjs` 6.0.5, `@types/node` 22.20.5, `typescript` 6.0.3, `eslint` 10.13.0, `@eslint/js` 10.0.1, `typescript-eslint` 8.72.0, `eslint-config-prettier` 10.1.8, `globals` 17.13.0, `vitest` 5.0.5 | las mismas del lock | `devDependencies` | ninguna versión nueva; `pnpm why @types/node` sigue mostrando una sola |

## Archivos que cambian

| Archivo | Cambio |
|---|---|
| `pnpm-workspace.yaml` | modificado: `tools/*` (RF-B1) |
| `pnpm-lock.yaml` | regenerado: solo el importer nuevo |
| `tools/sff-converter/.gitkeep` | se borra |
| `tools/sff-converter/{package.json,tsconfig.json,eslint.config.js,vitest.config.ts}` | nuevos (decisiones 1, 2 y 13). `vitest.config.ts`: `environment: 'node'`, `include: ['test/**/*.test.ts']` |
| `tools/sff-converter/README.md` | nuevo, en español: uso de `convert` e `inspect`, formato del mapa con un ejemplo, límites, formatos soportados y la regla de ADR-0004 (ids neutros como `local_1`, evidencia solo textual, nada fuera de `content/_local/`) |
| `tools/sff-converter/src/{errors,limits,reader}.ts` | nuevos (decisiones 3 y 4) |
| `tools/sff-converter/src/{sff,sff-v1,sff-v2,pcx,rle8,lz5,png,palette}.ts` | nuevos: detección de versión, nodos, enlaces, decodificadores y paletas (decisiones 5 a 8). `sff.ts` expone `readSff(bytes): SffFile` (cabeceras y nodos, sin píxeles) y `decodeSprite(file, index): IndexedOrRgbaImage` |
| `tools/sff-converter/src/{air,map}.ts` | nuevos: `parseAir(text)` y `parseMap(json)` con los valores por defecto y las acciones obligatorias de RF-B2 (`map.missing_action`, `map.missing_impact_frame`, `map.unknown_field`, `map.invalid_id`, `map.scale_out_of_range`) |
| `tools/sff-converter/src/{frames,pack,manifest,convert}.ts` | nuevos: frames (decisión 9), estantes (decisión 10), construcción del manifest y `convert({ sff, air, map }): { manifest, sheets, report }`, puro, sin disco |
| `tools/sff-converter/src/{write,cli}.ts` | nuevos (decisiones 11 y 12) |
| `tools/sff-converter/test/support/{bytes,sff1-writer,sff2-writer,pcx-writer,lz5-writer,png-writer,air-writer,fixtures,prng,elapsed,git-repo}.ts` | nuevos (RF-B6). Los escritores trabajan **a nivel de paquete**: reciben listas de paquetes RLE o LZ5 explícitos, no comprimen. Así cada rama del decodificador tiene su vector. `png-writer` usa `zlib.deflateSync` y filtros elegidos por fila. `fixtures.ts` arma un personaje sintético mínimo, con las 12 animaciones mapeables, en v1 y en v2 (una variante por formato de sprite). `git-repo.ts` crea un repo temporal con `git init` y su `.gitignore` de prueba, con el entorno `GIT_*` limpio |
| `tools/sff-converter/test/*.test.ts` | nuevos: `reader`, `pcx`, `rle8`, `lz5`, `png`, `sff-v1`, `sff-v2`, `air`, `map`, `frames`, `pack`, `convert`, `write`, `cli`, `robustness` y `eslint` (sección "Pruebas") |
| `apps/web/vite/local-sprites.ts` | nuevo (decisión 14) |
| `apps/web/vite.config.ts` | modificado: `localSprites()` primero en `plugins`. La opción `csp` queda sin diff |
| `apps/web/tsconfig.json` | modificado: `vite` en `include`, para que `svelte-check` revise el plugin |
| `apps/web/tests/local-sprites.test.ts` | nuevo: el resolvedor, el handler con un directorio temporal y un servidor de Vite real (sección "Pruebas") |
| `apps/web/src/lib/features/arena/{local-fighters,retime}.ts` (+ tests) | nuevos (decisiones 14, 15 y 17) |
| `apps/web/src/lib/components/fight/{FightStage,ArenaControls}.svelte` | modificados (decisión 15). `ArenaControls` sigue con solo `import type` del renderer |
| `apps/web/src/lib/components/fight/ArenaStats.svelte` | modificado: `mean_strike_gap_ms` en el JSON copiado (decisión 17) |
| `apps/web/src/routes/dev/arena/+page.svelte` | modificado: ids locales y `?pace` (decisiones 15 y 17) |
| `apps/web/src/routes/dev/components/_sections/ArenaSection.svelte` | modificado: `ArenaControls` con dos ids locales de ejemplo |
| `apps/web/messages/{es,en}.json` | modificados: `arena_fighters_placeholders`, `arena_fighters_local` y `arena_fighter_local` |
| `apps/web/e2e/arena.spec.ts` | modificado: 404 del índice, selector solo con placeholders y `?pace` |
| `scripts/check-no-local-sprites.mjs`, `scripts/__tests__/check-no-local-sprites.test.mjs`, `scripts/__fixtures__/check-no-local-sprites/**` | nuevos (decisión 14) |
| `.dockerignore`, `scripts/check-compose.mjs` (+ su test), `scripts/check-dockerignore.sh` | modificados (decisión 16) |
| `.github/workflows/ci.yml` | modificado: `tools/**` en el filtro `web` (RF-B1). Sin jobs ni actions nuevas. `node scripts/check-workflows.mjs` en verde |
| `justfile` | modificado: `check-web`, `test` y `e2e` (sección siguiente) |
| `docs/architecture.md` | modificado: `tools/sff-converter`, local y fuera de toda imagen, y el servido de `content/_local/sprites/` solo en `vite dev` |
| `CLAUDE.md` | modificado: en "Comandos", `check-web` y `just test` nombran el convertidor, y se agregan `convert` e `inspect` con un enlace al README |
| `docs/features/002-spike-renderer/spec.md` | modificado **por el owner** en RF-B9 (`## Revisión`). El engineer no escribe ahí |

## `just check` en B

```
check-web: _web-prepare
    …   (pasos de apps/web y de @torneo/renderer sin cambios)
    pnpm --filter @torneo/sff-converter exec tsc -p tsconfig.json --noEmit
    pnpm --filter @torneo/sff-converter exec eslint .
    pnpm --filter @torneo/sff-converter exec vitest run
    pnpm exec vitest run --config scripts/vitest.config.mjs
    …   (check-tokens, check-i18n y el resto sin cambios)

test: _web-prepare
    …
    pnpm --filter @torneo/sff-converter exec vitest run
    …

e2e: _web-prepare
    pnpm --filter web build
    node scripts/check-renderer-split.mjs
    node scripts/check-no-local-sprites.mjs
    pnpm --filter web exec playwright test
```

- Prettier ya cubre `tools/` desde la raíz. Prettier 3 respeta `.gitignore`, así que `input/` y `output/` quedan fuera.
- `svelte-check` cubre `apps/web/vite/` por el cambio de `tsconfig.json`, y el ESLint de `apps/web` (`eslint .`) ya lo recorre.
- El job `web` de CI corre `just check-web` y, con el filtro nuevo, se dispara al tocar `tools/**`.

## Orden de trabajo

Commits `build(tools): … (TA-2)`, `feat(tools): … (TA-2)`, `feat(web): … (TA-2)`, `test(…): … (TA-2)` y `docs(002): … (TA-2)`.

1. **Esqueleto y cadena de suministro (RF-B1).** Workspace, `package.json`, configuraciones, un `src/errors.ts` mínimo con su test, la parte de `justfile` y el filtro de `ci.yml`.
   → `pnpm install`: el diff del lock es solo el importer. Después `just check` en verde, `node scripts/check-workflows.mjs` en verde y `just prod-local` (build local de `web`) en verde. **Punto de corte:** si el `install` del Dockerfile exige el `package.json` nuevo, aplico la alternativa de la decisión 16 antes de seguir.
2. **Lector, límites y escritores de fixtures (RF-B4, RF-B6).** `reader.ts`, `limits.ts` y `test/support/*` (bytes, PRNG y escritores a nivel de paquete).
   → `reader.test.ts`: lecturas en el borde exacto, uno más allá, offsets negativos y no enteros, y sublectores que no ven fuera de su ventana.
3. **SFF v1 (RF-B3).** `sff.ts`, `sff-v1.ts`, `pcx.ts` y `palette.ts`.
   → `pcx.test.ts` y `sff-v1.test.ts`: corridas literales y RLE, `bytesPerLine > w`, una corrida que cruza el final de la salida, paleta propia, "misma paleta", modo compartido con `palette_sprite`, sprite enlazado, ciclo de enlaces, lista de offsets cíclica y PCX de 24 bits (`pcx.unsupported`). Antes de escribir los decodificadores, comparo las cabeceras de "Alcance de formatos" con `formats.txt` y con IKEMEN GO, y corrijo el plan si algo no coincide. **Paralelizable con el paso 4.**
4. **AIR y mapa (RF-B2, RF-B3).** `air.ts` y `map.ts`.
   → `air.test.ts`:
   - mayúsculas y minúsculas mezcladas;
   - comentarios `;`;
   - `Clsn*` e `Interpolate` ignorados;
   - `Loopstart`;
   - flips `H`, `V` y `HV`;
   - campos de mezcla ignorados y contados;
   - `ticks = -1`;
   - grupo `-1`;
   - acción duplicada (gana la primera, con nota);
   - línea demasiado larga;
   - demasiados frames.

   → `map.test.ts`: valores por defecto, acciones obligatorias que faltan, `impact_frames` que falta, campo desconocido, id inválido y `scale` fuera de rango.
5. **SFF v2 (RF-B3).** `sff-v2.ts`, `rle8.ts`, `lz5.ts` y `png.ts`.
   → `sff-v2.test.ts`: un sprite por formato (raw, RLE8, LZ5, PNG8, PNG24 y PNG32), `ldata` y `tdata`, paleta enlazada, `numcols < 256` con un índice fuera de rango, RLE5 reportado sin abortar cuando ninguna animación lo usa y con error cuando una lo usa, y una longitud declarada que no coincide.
   → `lz5.test.ts`: un vector por tipo de paquete (RLE corto y largo, LZ corto con bits reciclados y LZ largo), más `offset > escrito` y la entrada agotada.
   → `png.test.ts`: los cinco filtros, CRC ignorado, `IHDR` que no coincide, entrelazado, profundidad 16 y un `IDAT` que infla más de lo esperado (`png.too_large`).
6. **`inspect` y primera prueba real (punto de corte para B8).** `cli.ts inspect`. El owner lo corre sobre los personajes que piensa usar y pega en el PR solo la cuenta por formato, sin nombres. **Si alguno depende de RLE5, de paletas `.act` o de PCX de 24 bits**, se decide en ese momento: elegir otro personaje o abrir una desviación. Así no se descubre al final.
7. **Frames, empaquetado y conversión (RF-B5).** `frames.ts`, `pack.ts`, `manifest.ts` y `convert.ts`.
   → `frames.test.ts`: pivote con desplazamiento, espejo H y V, recorte, relleno hasta el pivote, `pivot_too_far` y frame vacío.
   → `pack.test.ts`: frames sin solapes y dentro de su hoja, margen de 1 px, orden determinista, cambio a una segunda hoja y `too_many_sheets`.
   → `convert.test.ts`: el personaje sintético v1 y v2 pasa `validateManifest(_, { expectedId })`, los píxeles de cada frame en la hoja coinciden con los del sprite de origen (con la paleta y el espejo aplicados), `frame_ms` y los recortes llevan sus notas, y un windup > 400 ms termina con el `ManifestError` de `validateManifest`.
8. **Escritura y CLI (RF-B2, RF-B5).** `write.ts` y `cli.ts convert`.
   → `write.test.ts`, en un repo temporal (`git-repo.ts`):
   - escribe en un directorio ignorado por el `.gitignore` de prueba;
   - se niega en uno no ignorado (`write.not_ignored`), y después **el directorio de salida no existe** (ni archivos ni directorios intermedios nuevos);
   - se niega con la salida como enlace simbólico;
   - se niega fuera de un repo (`write.git_unavailable`, `status` 128) y con un `PATH` sin `git` (`write.git_unavailable` por `error` ENOENT), sin crear nada en ninguno de los dos casos;
   - se niega si el ancestro existente más cercano es un enlace simbólico;
   - con `GIT_DIR` apuntando a otro repo en el entorno del test, el resultado no cambia.

   → `cli.test.ts`: lanza el CLI real con `process.execPath --experimental-strip-types src/cli.ts convert -- …` sobre los fixtures escritos al directorio temporal. Prueba que Node carga el paquete y `@torneo/renderer/manifest` con *type stripping*, que `INIT_CWD` resuelve las rutas relativas, que stdout es el JSON del reporte sin rutas (busca el nombre del directorio temporal en la salida y no debe estar) y que los errores salen como código con salida 1: un `--sff` inexistente da `cli.file_not_found` con `{ arg: 'sff' }` y sin la ruta en stderr. **Paralelizable con el paso 9.**
9. **Robustez (RF-B4).** `robustness.test.ts`:
   - truncado en cada byte de la cabecera y del primer sprite, en v1 y en v2;
   - 2 000 mutaciones con `mulberry32(0x5ff)` de `test/support/prng.ts` sobre los fixtures v1, v2 y AIR: cambiar un byte, poner un u16 o u32 en 0, `0xFFFF` o `0xFFFFFFFF`, truncar, duplicar un tramo e intercambiar dos nodos.

   Cada caso llama a `convert()`. Solo vale que termine bien o con `ConverterError`, y `elapsed()` < 1 000 ms por caso. Si falla, el mensaje imprime la semilla y el índice del caso para reproducirlo. → el test en verde y su duración total en el PR (objetivo < 10 s).
10. **Servido local (RF-B7).** `vite/local-sprites.ts`, `vite.config.ts`, `tsconfig.json` y `tests/local-sprites.test.ts`. **Paralelizable con los pasos 2 a 9** (no comparte archivos).
    → los tests de la sección "Pruebas", y a mano: `just dev` con un `content/_local/sprites/` que solo tiene una copia de los placeholders con otro id. `/__local-sprites/index.json` la lista.
11. **Arena (RF-B7, decisiones 15 y 17).** `local-fighters.ts`, `retime.ts`, los cambios de `FightStage`, `ArenaControls`, `ArenaStats` y `+page.svelte`, los mensajes y el catálogo.
    → `just check` (`check-i18n`, `check-tokens`) y los unit de `local-fighters` (con un `fetch` falso: 404, JSON inválido, ids inválidos) y de `retime` (factor 1 da la misma timeline, 1.25 pasa `validateTimeline`, la separación media escala con el factor, y fuera de rango se ignora). Capturas del catálogo a 375 y 1280 px en los dos idiomas y los dos temas, con el `<select>` agrupado.
12. **Build sin rastro y e2e.** `check-no-local-sprites.mjs` con sus fixtures, la línea de `just e2e` y `arena.spec.ts`.
    → `just e2e` en verde. **Paralelizable con el paso 11** hasta que haga falta el selector.
13. **Contexto de Docker (decisión 16).** `.dockerignore`, `check-compose.mjs` y `check-dockerignore.sh`.
    → `just check-dockerignore` en verde con `content/_local/` y `tools/sff-converter/input/` presentes en local, y `node scripts/check-compose.mjs` con su test.
14. **Docs.** README del convertidor, `architecture.md` y `CLAUDE.md`. → revisión del PR.
15. **Conversión y medición del owner (RF-B8, RNF).** El owner convierte al menos 2 personajes con `just dev` y los reproduce en `/es/dev/arena`. En el teléfono de referencia y en la Mac, en la misma sesión de `just dev`, corre una vez de calentamiento y tres corridas con `dummy_a` contra `dummy_b` (el control) y tres con los dos locales. Pega en el PR el reporte JSON de cada conversión (sin rutas ni nombres), las `ManifestError` si hubo y los seis JSON de estadísticas, con el modelo, el SO y el navegador. **Sin capturas.** Antes de publicar el PR, reviso yo que ningún texto del PR, de los commits ni de Linear nombre un personaje de terceros.
16. **Go/no-go (RF-B9).** El owner ve la demo, con `?pace` si quiere probar otras separaciones, y escribe su nota en `## Revisión` de la spec con las seis respuestas, la separación media preferida y los ajustes a "Datos para 003". Yo no escribo ni preparo esa nota más allá de recordarle las preguntas.
17. **Cierre.** `just check`, `just e2e` y `just check-dockerignore` desde un worktree limpio, `pnpm audit --audit-level moderate` y `git status --ignored content/_local tools/sff-converter`: lo ignorado sigue ignorado y nada aparece como rastreable. Las salidas van al PR.

## Riesgos

1. **Leer mal el formato y que los fixtures no lo detecten.** Los escritores sintéticos codifican la misma interpretación que el lector, así que un offset equivocado pasa los tests. → El paso 3 coteja las cabeceras con dos fuentes independientes, y el paso 6 prueba `inspect` con personajes reales **antes** de construir frames y empaquetado. Lo que salga mal en la conversión real se arregla con un test que lo reproduzca con fixtures sintéticos (regla de bugs de `CLAUDE.md`).
2. **Colores equivocados en v1 con paleta compartida.** Muchos personajes v1 guardan la paleta real en archivos `.act`. → `palette_sprite` en el mapa cubre el caso común (la paleta del sprite 0,0). Si el owner ve colores mal, la opción `--act` queda como desviación acotada, y no se adelanta.
3. **PNG8 con la paleta embebida en vez de la del nodo.** Uso la del nodo, que es la convención de SFF v2, pero hay herramientas que solo llenan la `PLTE`. → Si el owner ve colores mal en un PNG8, el mapa gana `png8_palette: "embedded"`, y se anota.
4. **Fuzzing que pasa en vacío.** Un `catch` genérico en `convert()` haría pasar las mutaciones sin probar nada. → Decisión 3. Además, la revisión del PR busca `catch` en `src/` (solo `png.ts` y `cli.ts`), y el test de robustez exige que **al menos** el 20 % de los casos termine bien y el 20 % con `ConverterError`, para ver que las mutaciones ejercen los dos caminos.
5. **Que Vite 8 cargue mal el `.ts` del renderer desde `vite.config.ts`.** El cargador de configuración (rolldown) podría externalizar `@torneo/renderer/manifest`, que es TS. → Si lo externaliza, Node 22.23 lo carga con *type stripping* (la ruta real está en `packages/renderer`, fuera de `node_modules`). Si aun así falla, el plugin lo carga en tiempo de ejecución con `server.ssrLoadModule('@torneo/renderer/manifest')` dentro de `configureServer`. El test de integración del paso 10 lo detecta.
6. **Que el servido local llegue al build o a la imagen.** → `apply: 'serve'` sin `configurePreviewServer`, la eliminación de `import.meta.env.DEV`, `check-no-local-sprites.mjs` sobre `build/`, el e2e del 404, `tools/` y `content/_local/` en `.dockerignore`, y `check-dockerignore.sh`.
7. **Path traversal y DNS rebinding en el middleware.** → El `hostValidationMiddleware` de Vite corre antes del handler (probado con un `Host` ajeno). Rechazo de `%` y de `\` sin decodificar, una expresión regular exacta, contención con `realpath` (también contra enlaces simbólicos) y tests con peticiones HTTP crudas (`node:http`; `fetch` normalizaría `..` antes de enviar).
8. **Sprites de MUGEN en el repo, el PR o Linear (ADR-0004).** → `write.ts` se niega fuera de lo ignorado, el reporte no lleva rutas ni nombres, el README pide ids neutros, el paso 15 exige evidencia solo textual con una revisión mía antes de publicar, y el paso 17 corre `git status --ignored`.
9. **Memoria con personajes grandes.** → Decodificación perezosa y la cota `convert.too_large` antes de decodificar (decisiones 9 y 10).
10. **El hook `pre-push` y las variables `GIT_*`.** `write.ts` y sus tests podrían consultar el repo real. → Entorno limpio en `spawnSync`, con un test explícito (paso 8).
11. **Animaciones de MUGEN que no encajan en el formato** (windups de más de 400 ms, poses de victoria largas). → Los recortes van con nota. Si el windup es demasiado largo, falla con el código de `validateManifest` y el owner elige otra acción o otro `impact_frame`. Si pasa con muchos personajes, es un dato para el go/no-go, no algo para esconder.

## Alternativas descartadas

- **Decodificar PNG con `pngjs`.** Pierde los índices de la paleta: no se puede saber qué píxel es transparente (el índice 0) ni aplicar la paleta del nodo (decisión 7).
- **Solo SFF v1 o solo v2.** Limita qué personajes puede elegir el owner antes de saber cuáles sirven ("Alcance de formatos").
- **RLE5 y paletas `.act` desde el principio.** RLE5 es raro en personajes recientes y `.act` agrega una entrada más. Se reportan con código y se suman solo si los personajes elegidos los necesitan (paso 6).
- **Un `catch` genérico que convierta todo en `ConverterError`.** Haría pasar en vacío la prueba de RF-B4 (riesgo 4).
- **Servir `content/_local/` con `server.fs.allow` o `publicDir` de Vite.** Expone el árbol entero, sin lista blanca de nombres, y `publicDir` se copia al build.
- **Pedir siempre el índice y tratar el 404 en el build** (texto literal de RF-B7). Deja un error en la consola del build y rompe el e2e de A2 (precisión 1).
- **Comprobar `git check-ignore` una sola vez, sobre el directorio.** Una regla de `.gitignore` con `!` podría reincluir un archivo concreto. Por eso se comprueba cada archivo que se va a escribir.
- **Fuzzing con `fast-check` u otra librería.** Sería una dependencia nueva para algo que un PRNG con semilla y cinco mutadores cubren.

## Pruebas (cómo se demuestra que funciona)

- [ ] `just check`, `just e2e` y `just check-dockerignore` en verde, con las salidas en el PR.
- [ ] **RF-B1:** el diff del lock es solo el importer; `pnpm why @types/node` muestra una versión; `eslint.test.ts` con fixtures que fallan (`Math.random`, `Date.now()` y `performance.now()` en `src/`, `import 'pixi.js'`, `import '@torneo/renderer'` sin subpath) y que pasan (`performance.now()` en `test/support/elapsed.ts`, `Date` en `src/cli.ts`); el job `web` se dispara con `tools/**`; el job `images` en verde.
- [ ] **RF-B2:** `map.test.ts` y `cli.test.ts` (rutas relativas con `INIT_CWD`, `--` inicial, errores como código con su salida, `cli.file_not_found` y `cli.file_unreadable` sin la ruta).
- [ ] **RF-B3:** `pcx`, `rle8`, `lz5`, `png`, `sff-v1`, `sff-v2` y `air`, con un fixture sintético por caso de la tabla de formatos, incluido RLE5 reportado sin abortar.
- [ ] **RF-B4:** `reader.test.ts` y `robustness.test.ts`: truncado en cada byte de la cabecera y del primer sprite, 2 000 mutaciones con semilla fija, solo `ConverterError` o éxito, cada caso < 1 s y los dos caminos ejercidos (riesgo 4).
- [ ] **RF-B5:** `write.test.ts` (ignorado: escribe; no ignorado: el directorio de salida no existe después; enlace simbólico en la salida o en el ancestro; fuera de un repo; sin `git`; `GIT_DIR` ajeno; `write.ts` sin `catch`), `convert.test.ts` (sale `validateManifest` ok y los píxeles coinciden) y `pack.test.ts`.
- [ ] **RF-B6:** `grep -rn "content/_local" tools/sff-converter/test` sin resultados (también en un test). CI en verde sin archivos de MUGEN.
- [ ] **RF-B7:**
  - `tests/local-sprites.test.ts`, contra el resolvedor:
    - `..`;
    - `%2e%2e`, `%2E%2E`, `%2f` y `%5c`;
    - `\`;
    - `//`;
    - un id con mayúsculas o de 49 caracteres;
    - `sheet-a.png`, `manifest.json.bak` y `.env`;
    - query y fragmento.
  - El mismo test, contra el handler con un directorio temporal: 404 sin tocar el disco cuando el resolvedor da `null` (con un `fs` espiado), contención frente a un enlace simbólico que sale de la base, `index.json` que lista solo ids con manifest válido, y la base ausente da `{ ids: [] }`.
  - El mismo test, con un servidor de Vite real (`createServer({ configFile: false, plugins: [localSprites({ baseDir })], server: { port: 0, host: '127.0.0.1' } })`) y peticiones crudas con `node:http`: los mismos casos de traversal dan 404, el PNG llega con `nosniff`, con `Origin: http://evil.example` la respuesta no trae `Access-Control-Allow-Origin`, con `Host: evil.example` Vite responde 403 antes de llegar al handler (con un `fs` espiado que no se llama) y `localSprites().apply === 'serve'`.
  - `arena.spec.ts` contra el build: `/__local-sprites/index.json` y `/__local-sprites/dummy_a/manifest.json` dan 404 y el selector de cada lado tiene exactamente `dummy_a` y `dummy_b`.
  - `check-no-local-sprites.mjs` sobre el build real, con sus fixtures en los tests de `scripts/`.
  - `just check-dockerignore` en verde.
- [ ] **Arena:** unit de `local-fighters` y `retime`. e2e: `?pace=1.25` llega a `playing` y, con `?t=<duration_ms>`, a `finished`. Catálogo con `ArenaControls` agrupado; capturas a 375×812 y 1280×800, en `es` y `en`, con los temas claro y oscuro; axe sin violaciones `serious` ni `critical`.
- [ ] **RF-B8:** en el PR, solo texto: el reporte de cada conversión, las `ManifestError` si hubo y las estadísticas de control y con personajes locales en `just dev`, en el teléfono y en la Mac.
- [ ] **RF-B9:** la nota del owner en `## Revisión` de `spec.md`, con las seis respuestas, la separación media y los ajustes a "Datos para 003".

## Estimación de CI

Un solo cambio en `ci.yml`: `tools/**` en el filtro `web`. No hay jobs ni actions nuevas. El job `web` suma unos 30 s: `tsc`, ESLint y Vitest del convertidor, con las 2 000 mutaciones en menos de 10 s y el arranque del servidor de Vite del test del plugin. El job `e2e` suma menos de 30 s (`check-no-local-sprites` y tres casos de `arena.spec.ts`). `images` corre porque cambian el lock y `.dockerignore`, con la duración de siempre. `policy` corre siempre y no cambia.

## Desviaciones durante la implementación

1. **Rama.** `ta-2-sff-converter` en lugar de `ta-2-spike-renderer-b`, a pedido de la coordinación. Mismo alcance y un solo PR.
2. **`parseMap(json)` recibe el JSON ya parseado.** `JSON.parse` y su `catch` van en `cli.ts`, que es el único módulo con `catch` además de `png.ts` (decisión 3). El tamaño del mapa se controla ahí mismo, con `fs.stat`.
3. **`decodeSprite` devuelve siempre RGBA.** La paleta se aplica dentro del decodificador, así que no existe el tipo intermedio `IndexedOrRgbaImage`.
4. **Prefijo u32 en SFF v2.** En RLE8 y LZ5 se compara con `w × h` (`sff.size_mismatch`). En PNG se salta sin comparar, porque no es la cuenta de píxeles. El formato raw no lleva prefijo: sus datos tienen que medir al menos `w × h`.
5. **Pivote con espejo.** Primero se espeja el eje (`w − eje_x` con `H`, `h − eje_y` con `V`) y después se resta el desplazamiento del AIR, que está en el espacio del personaje y no se espeja.
6. **Ancestro existente en `write.ts`.** `lstatSync(_, { throwIfNoEntry: false })` lanza ENOTDIR cuando el camino pasa por un archivo. Por eso el recorrido baja desde la raíz y nunca hace `lstat` a través de un archivo. Un enlace simbólico intermedio se sigue con `existsSync` y `statSync`, que solo corre si `existsSync` dio `true` (macOS tiene `/var` → `/private/var`). Solo se rechaza el enlace que es el ancestro existente más cercano o la salida. Sigue sin haber `catch`.
7. **ADR-0009 D4 en la arena.** Los componentes no hacen `fetch` ni importan valores de `features/`. Por eso `+page.svelte` le pasa a `FightStage`, por props, `local: LocalAssets` (de `createLocalAssets(fetch)`) y `retime`. `FightStage` expone `currentTimeline()`, y la página calcula `meanStrikeGapMs` con ese valor. `retime.test.ts` vive en `apps/web/tests/` porque lee valores del renderer, igual que `renderer-package.test.ts`. `ArenaFighterId` pasa a ser `string`, y `PlaceholderFighterId` guarda los dos placeholders.
8. **`check-no-local-sprites` y los source maps.** Los `.map` del servidor copian el código original en `sourcesContent`, incluida la rama `import.meta.env.DEV` que el build eliminó. Por eso los `.map` se revisan sin ese campo, y el resto del build se revisa entero.
9. **`retimeTimeline`.** Un `knockdown` conserva su separación del `strike` anterior (la regla de los 100 ms). `duration_ms` conserva el tramo posterior a `win`, la pose de victoria de 2 s, en lugar de escalarse. Con 0.8, 1.25 y 1.5, el guion pasa `validateTimeline`.
10. **Arreglo de A2 en `vite dev`.** Kit limita `server.fs.allow` a `apps/web`, así que los `?url` de las hojas de los placeholders respondían 403 y `/dev/arena` terminaba en `asset_load_failed` con `just dev`. La fase B mide ahí, así que se agrega solo `packages/renderer/placeholders/`, con su test en `tests/vite-config.test.ts`. Va en un commit `fix(web)` aparte.
11. **Códigos agregados.** Una línea mal formada del AIR se cuenta en su acción y solo es error (`air.bad_frame`) si una animación mapeada la usa. Se suman `air.empty_action`, `convert.sprite_not_found` (frame vacío con nota), `write.invalid_manifest`, `write.sheet_mismatch`, `cli.missing_argument`, `cli.bad_arguments` y `cli.unknown_command`.
12. **Decisión 16 confirmada sin alternativa.** La imagen `web` se construye (`docker buildx build -f apps/web/Dockerfile`) sin el `package.json` del importer nuevo: `--filter 'web...'` no lo pide. `check-dockerignore.sh` crea `tools/sff-converter/input/` si no existe y lo borra al salir.
13. **`CLAUDE.md` lo actualiza el orquestador, no el engineer.** Las líneas de "Comandos" que pide el plan (`check-web` y `just test` nombran el convertidor, y `convert` e `inspect` enlazan al README) entran en el commit `ebe8625` del orquestador. El engineer no toca `CLAUDE.md`.
14. **`pnpm -s`.** El README pide `pnpm -s …`. Sin `-s`, pnpm imprime la línea del comando con las rutas locales, y eso no puede terminar en el PR (RF-B8).
15. **`packages/renderer/src/stage/textures.ts` fuera del alcance "no toca".** El commit `eb423a0` aplica un comentario de la revisión del PR #40, a pedido de la coordinación: la URL de una hoja se registra para liberarla solo si su `acquire` tuvo éxito. Lo cubre `packages/renderer/test/textures.test.ts`. En el mismo commit van otros comentarios de esa revisión: `just arena-bench` limpia `SOCKET_PATH` y `LISTEN_FDS`, se documenta la caché compartida y se quita un comentario redundante del `justfile`.
16. **`sff.unsupported_compression` en el reporte.** Un sprite no soportado que ninguna animación mapeada usa queda como nota (`sff.unsupported_compression` o `pcx.unsupported`, con `{ sprite, compression }`) y la conversión sigue (RF-B3). `unsupportedCompression()` no lanza: solo lee campos de nodo y, en SFF v1, una cabecera PCX completa. Lo hallado por QA lo cubre `convert.test.ts`.
17. **`Cross-Origin-Resource-Policy: same-origin`** en las respuestas de `/__local-sprites/` (solo en `vite dev`), con su test.
18. **El `knockdown` por defecto encadena acciones y queda en el suelo (TA-24, después del merge de B).** Con `knockdown: 5050`, el personaje convertido se quedaba suspendido en el aire durante todo `duration_ms`. En MUGEN, la 5050 es la caída en el aire: la altura la pone la física (velocidad en y), no el frame, y el aterrizaje (5100) y el estar tendido (5110) son otras acciones. El renderer sostiene el último frame hasta el fin del `knockdown`, como manda la spec, así que sostenía un cuerpo a ~150 px de los pies. Los placeholders no tenían el problema: su `knockdown` está apoyado en el suelo. El arreglo es solo del convertidor:
    - `actions.knockdown` acepta un número o una lista de 1 a 8 acciones que se reproducen una detrás de otra. Por defecto, `[5050, 5100, 5110]`. Las demás animaciones siguen con una sola acción (`map.invalid_action` si llevan lista). Una acción de la cadena que falta en el AIR deja `convert.optional_missing`, y si faltan todas, el renderer usa el fallback (`hit`). El tope de 64 frames y de 3 s se aplica a la cadena completa, y el recorte nunca quita su último frame, que es el que el renderer sostiene (tendido).
    - Un frame de `knockdown` cuyo dibujo termina por encima del pivote se baja hasta que su fila visible más baja toque el pivote, y deja la nota `convert.grounded`. El pivote horizontal y los píxeles no cambian, y un frame que ya llega al suelo (o lo pasa) queda igual. Las demás animaciones no se tocan: un ataque puede despegar del suelo.
    - Lo cubren fixtures sintéticos en `convert.test.ts` y `map.test.ts`: la 5050 del fixture sube el sprite con un desplazamiento del AIR, como la de MUGEN.

## Revisión
### architect — 2026-10-27 — REQUEST_CHANGES
- Verifiqué:
  - **Cobertura de la spec.** RF-B1 a RF-B9 tienen decisión, archivo, paso y prueba. El alcance "no toca" coincide con "Componentes afectados" y deja intacta la opción `csp` (ADR-0012 B6). No hay paquetes nuevos del registro y `strictDepBuilds` no cambia.
  - **ADR-0004.** `content/_local/` y `tools/sff-converter/{input,output}/` están en `.gitignore`, y `content/_local/` en `.dockerignore`. Sumar `tools/` y su canario cierra el contexto de construcción. La evidencia es solo texto, el reporte no lleva rutas y hay `git status --ignored` al cierre.
  - **Límites entre módulos.** El convertidor solo importa `@torneo/renderer/manifest` (con ESLint), y `apps/web` no depende de él. El plugin es `apply: 'serve'` sin `configurePreviewServer`, el cliente queda detrás de `import.meta.env.DEV` y `check-no-local-sprites.mjs` revisa el build. RF-A17 no cambia: `local-fighters.ts` no importa Pixi.
  - **Repo.** `apps/web/tsconfig.json` tiene `include` propio, así que agregar `vite` basta. El filtro `web` de CI también dispara `e2e`, que corre `just e2e`. El Dockerfile de `web` copia `package.json` concretos y `packages`, nunca `tools/`.
  - **Vite 8.3.5** (`dist/node/chunks/node.js`, `_createServer`). `rejectInvalidRequestMiddleware`, `cors` y `hostValidationMiddleware` se registran **antes** de los hooks `configureServer`, así que el middleware directo queda protegido frente a DNS rebinding.
  - Las cinco precisiones: las acepto. El texto de la enmienda está en `spec-amendments-b.md`, y se aplica después del commit de A2.
- Cambios pedidos:
  1. **(Bloqueante) Decisión 12 frente a la decisión 3.** `execFileSync` lanza una excepción cuando el código de salida no es 0, y para distinguir el 1 del 128 habría que poner un `catch` en `write.ts`. La decisión 3 lo prohíbe ("solo `png.ts` y `cli.ts`"), y la revisión del riesgo 4 lo marcaría. Usa `spawnSync('git', [...], { cwd, env, shell: false, stdio: 'ignore' })`, que no lanza. Con `status === 0` se escribe, con `1` el código es `write.not_ignored`, y con cualquier otro `status`, `signal` o `error` (por ejemplo ENOENT) es `write.git_unavailable`. Además, el orden: hoy "crea el directorio de salida" antes de `check-ignore`, y eso deja un directorio en un lugar no ignorado. `check-ignore` funciona con rutas que todavía no existen. Córrelo con `cwd` en el ancestro existente más cercano de la salida, comprueba el enlace simbólico en ese ancestro y en la salida si ya existe, y haz `mkdir` solo cuando todas las comprobaciones pasaron. En `write.test.ts`, "no ignorado" exige que **no exista** el directorio de salida, no solo que no haya archivos.
  2. **(Corrección, no bloqueante) Decisión 14: "el middleware corre antes que el `cors` interno de Vite" es falso en Vite 8.3.5.** Corre después de `cors` y de `hostValidationMiddleware`. Eso es mejor, pero el test "no hay cabeceras CORS" fallaría con un `Origin` de loopback, porque Vite responde con `Access-Control-Allow-Origin` a `localhost` y a `127.0.0.1`. Reescríbelo así: con `Origin: http://evil.example` no hay `Access-Control-Allow-Origin`, y con `Host: evil.example` Vite responde 403 antes de llegar al handler (protección contra DNS rebinding comprobada en el test con el servidor real).
  3. **(No bloqueante) Archivo de entrada inexistente.** Hoy sale como `internal.unexpected` con código 2, que es el error más común del owner. En `cli.ts`, el `fs.stat` de cada entrada traduce ENOENT y EACCES a `cli.file_not_found` y `cli.file_unreadable` con `{ arg: 'sff' | 'air' | 'map' }`, sin la ruta. Va dentro de `cli.ts`, que ya admite `catch`.

### frontend-engineer — 2026-10-27 — RESPUESTA
- **Cambio 1 (bloqueante, decisión 12).** `write.ts` usa `spawnSync('git', ['check-ignore', '-q', '--', <ruta>], { cwd, env, shell: false, stdio: 'ignore' })`, que no lanza, y no tiene ningún `catch`: `status === 0` escribe, `1` da `write.not_ignored`, y cualquier otro `status`, `signal` o `error` da `write.git_unavailable`. El orden ahora es: validar el manifest, buscar el ancestro existente más cercano (con `lstatSync(_, { throwIfNoEntry: false })`), comprobar el enlace simbólico en ese ancestro y en la salida si ya existe, correr `check-ignore` por archivo con `cwd` en ese ancestro, y `mkdir` solo al final. `write.test.ts` exige que, tras una negativa, el directorio de salida no exista. Suma los casos sin `git` en el `PATH` y con el ancestro como enlace simbólico.
- **Cambio 2 (decisión 14).** Corregí el orden: en Vite 8.3.5 el handler corre después de `rejectInvalidRequestMiddleware`, `cors` y `hostValidationMiddleware`. El test con el servidor real comprueba que con `Origin: http://evil.example` no hay `Access-Control-Allow-Origin` y que con `Host: evil.example` hay 403 sin que el handler toque el disco. El riesgo 7 suma DNS rebinding.
- **Cambio 3 (decisión 11).** `cli.ts` hace `fs.stat` de cada entrada y traduce ENOENT a `cli.file_not_found` y EACCES o EPERM a `cli.file_unreadable`, con `{ arg }` y sin la ruta. Agregué `cli.not_a_file` y `cli.file_too_large`. Lo cubre `cli.test.ts`.
- **Enmiendas.** La sección "Para el architect" queda como registro de las cinco precisiones aceptadas en `spec-amendments-b.md`, y las decisiones 7, 8, 9, 11, 13, 14 y 17 ya coinciden con ese texto.
- `status: in-review`.

### architect — 2026-10-27 — APPROVE (segunda revisión)
- Verifiqué:
  - **Cambio 1.** La decisión 12 usa `spawnSync`, sin `catch` en `write.ts`, con el mapeo de `status`, `signal` y `error`. El orden es: manifest, ancestro existente con `lstatSync(_, { throwIfNoEntry: false })`, comprobación de enlace simbólico, `check-ignore` por archivo y `mkdir` al final. `write.test.ts` exige que el directorio no exista después de una negativa, y suma los casos sin `git` y con el ancestro como enlace simbólico.
  - **Cambio 2.** El orden de los middlewares de Vite 8.3.5 está corregido en la decisión 14 y en el riesgo 7. El test con el servidor real comprueba que un `Origin` ajeno no recibe `Access-Control-Allow-Origin` y que un `Host` ajeno recibe 403 sin tocar el disco.
  - **Cambio 3.** `cli.ts` traduce los errores de `fs.stat` a `cli.file_not_found`, `cli.file_unreadable`, `cli.not_a_file` y `cli.file_too_large`, con `{ arg }` y sin la ruta, y lo cubre `cli.test.ts`.
  - Las precisiones del plan coinciden con `spec-amendments-b.md`, que se aplica a `spec.md` después del commit de A2.
- Sin cambios pedidos.
