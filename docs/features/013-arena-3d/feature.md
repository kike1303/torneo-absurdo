---
id: "013"
title: Arena 3D con la botarga genérica
type: feature
status: done
risk: low
author: claude
linear: TA-50
depends_on: []
created: 2026-11-05
updated: 2026-11-05
---

# Arena 3D con la botarga genérica

## Qué y por qué
Las peleas de la web se ven con sprites 2D de relleno. La botarga genérica ya existe en 3D (GLB con rig de Mixamo, 16 clips, tinte por material y ranura de cara; ver la [bitácora](../../guides/bitacora-personajes-3d.md#botarga-genérica)) y el piloto fuera del repo la validó en el navegador. Esta feature lleva ese modelo al juego para ver cómo quedan las peleas en 3D: dos botargas enfrentadas, animadas según el `FightLog`, con la cara cambiando según lo que pasa.

Decisiones del owner (2026-11-05): **todo se ve en 3D**. El renderer 2D de sprites queda oculto, sin borrarse (solo `/dev/arena?renderer=2d` lo usa). Los clips de Mixamo pueden ir en el repo dentro del GLB.

## Alcance
- Entra: una capa Three.js en `packages/renderer` que implementa el mismo `Stage` que la de Pixi; el GLB y las caras en `content/models/botarga-base/`; la web pasa a 3D en vivo, en la portada y en `/dev/arena`; tinte por lado; caras por animación; HUD, banners y números de daño en DOM sobre el canvas; medición de fps a 375 px.
- No entra: un modelo por personaje (los cuatro del roster usan la botarga genérica con otro tinte), escenarios, compresión del GLB (meshopt o KTX2; KTX2 además choca con la CSP), apartar el código 2D a una carpeta de archivo, botarga propia del jugador.

## Diseño
**Mismo reproductor, otra capa.** `createPlayer(options, stage)` ya separa el reloj, los cues y `sceneAt` (puros) del dibujo. Se agrega `createRenderer3d(container, options)` en el subpath nuevo `@torneo/renderer/three`, que es `createPlayer(options, createThreeStage(container, model))`. La coreografía, el HP, los banners y los efectos salen del mismo `Scene`, así que `seek`, la pelea en vivo y el determinismo no cambian. Los manifests de sprites se siguen validando, porque dan los tiempos de cada animación (`windup`, `total`), pero la capa 3D no carga sus hojas.

**`FighterScene`** suma `elapsed` (ms desde el inicio de la animación), `windup` y `total`, para muestrear los clips como función pura de `t`.

**Clips.** Cada `AnimationCode` va a un clip. Las variantes (`attack_heavy_1/2`, `special_1-4`, `ko_1/2`, `win_1/2/3`) se eligen con un hash de `(lado, t − elapsed)`, igual para todos los espectadores. El tiempo del clip se ajusta por tramos para que el impacto del clip (`impact`, fracción medida por clip con el máximo alcance de la muñeca) caiga en el `windup` del sprite. Las reacciones se estiran a su `total`, `idle` hace bucle, y `ko` y `win` corren a velocidad natural y se quedan en el último cuadro. Al cambiar de clip hay un fundido de 200 ms (solo visual).

**Caras.** Un `onBeforeCompile` en `Botarga_Cara` mezcla una segunda textura dentro de `face_uv_rect` (con `1 − v`, porque glTF invierte la V). Expresión por animación: `guardia` en idle (`cansado` con HP < 25 %), `preparacion` en el windup y `golpe` tras el impacto, `enfadado` en los especiales, `dano`, `concentrado`, `alerta`, `ko` y `victoria`; `neutral` en la intro.

**Tinte.** Cada material toma `extras.tint_default`; `content/models/botarga-base/tints.json` da el tinte de cada lado (cuerpo y cara juntos, short aparte), así que `a` y `b` no se confunden.

**Mundo.** 1 px lógico = 1/140 m; `a` mira a +x y `b` a −x. La cámara de `Scene.camera` (zoom y temblor) mueve una cámara perspectiva que encuadra a los dos. Un solo GLB cargado; el segundo luchador es un `SkeletonUtils.clone` con materiales propios. El destello del golpe es `emissive`; las partículas, un `InstancedMesh`.

**Costo de dibujo.** Solo se dibuja cuando cambió algo, y nunca mientras el escenario está oculto (en `pre_live`): WebKit guarda lo que se dibuja en un canvas que no muestra y lo paga todo en el primer cuadro visible (3,7 s en WebKit móvil). Al cargar, los shaders se compilan y un pixel leído obliga a subir todo antes de mostrarse. Una mancha suave bajo cada botarga reemplaza al shadow map. Sin MSAA a 2× o más. Si el contexto es por software (SwiftShader, llvmpipe: sin GPU usable), arranca sin MSAA, a pixel ratio 0,5 y con las botargas en `MeshLambertMaterial` (solo difuso, sin normal map): el shader PBR con skinning tarda decenas de segundos en compilarse por software. Si el intervalo de cuadro suavizado pasa de 40 ms, el pixel ratio baja un 25 % (como mucho una vez por segundo, hasta 0,5). Al destruirse, la capa suelta su contexto WebGL como Pixi, y `FightStage` ignora la pérdida de contexto de un renderer que ya no es el actual.

**CSP (ADR-0012).** Sin workers, sin Draco ni KTX2, sin `eval`. `GLTFLoader` lee las imágenes embebidas con `fetch` de una URL `blob:`, que `connect-src 'self'` bloquea: un plugin cambia su cargador por `TextureLoader` (`<img>`, permitido por `img-src blob:`). Three llega solo por `import()`; `check-renderer-split` también lo exige.

**Assets.** `content/` (derechos reservados) es el lugar del arte propio. El GLB pesa 1,6 MB (texturas JPEG: color y normal a 1024, rugosidad a 512) y las caras se guardan a 256 × 256 (las 11 que se precargan suman unos 300 KB). Así llega antes de `t = 0` con la Fast 3G del e2e. La web los importa con `?url` (Vite les pone hash) y el `Dockerfile` copia `content/models`.

## Riesgos
- Peso de descarga en móvil (~1,9 MB): la compresión de la malla y los clips (meshopt) queda como siguiente paso. _Hecho en TA-70 (2026-11-09): 1,07 MB con `EXT_meshopt_compression` sin pérdida visible; receta en la [guía del flujo 3D](../../guides/flujo-personajes-3d.md)._
- WebGL sin contexto: mismo `webgl_unavailable` y la misma vigilancia de contexto perdido.

## Pruebas
- [x] `just check` en verde.
- [x] Unit: mapeo de animación a clip y tiempo (impacto alineado, variantes deterministas, clamp de `ko`/`win`), caras por animación y HP, `FighterScene.elapsed`.
- [x] e2e: las peleas existentes pasan con la capa 3D (mismo `data-state`, HP y cues).
- [x] Capturas a 375 px y en escritorio, claro y oscuro; fps en modo móvil (Mac, 375 px: 120 fps de media, p95 10,3 ms, 0 cuadros > 33 ms).
- [ ] fps en un celular medio real (pendiente del owner, método de `packages/renderer/README.md`).
