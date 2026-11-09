---
title: Backlog de ideas
type: backlog
status: draft
author: owner
created: 2026-11-04
updated: 2026-11-05
---

# Backlog de ideas

Ideas acordadas con el owner que todavía no se implementan. No son requisitos: cuando una entra en trabajo, sale de aquí y pasa a un `feature.md`. Lo de negocio y monetización vive en `private/`, nunca aquí.

## Orden acordado (2026-11-04)
1. Proceso ligero (ADR-0014).
2. Piloto 3D: Bruno animado en Three.js reproduciendo un `FightLog` real, con criterio de "seguimos o no" (fps y MB en un celular de gama media).
3. Cerrar el torneo de espectadores en 3D: peleas en vivo, apuestas, leaderboards y repeticiones, sin accesorios.
4. Botarga propia y PvP asíncrono con ranking.
5. Amigos y torneos privados.
6. Tienda de skins.

**El torneo va primero.** El PvP arranca cuando el torneo esté jugable de punta a punta.

## Peleas en 3D con botargas
- Los personajes son **botargas** 3D graciosas. Trabajo en Blender: [bitácora de personajes 3D](guides/bitacora-personajes-3d.md).
- El motor no cambia: el renderer 3D (candidato Three.js) reproduce el mismo `FightLog`. Cuando el 3D sea viable, el renderer 2D y las herramientas de sprites (`packages/renderer`, `tools/sff-converter`, `tools/sprite-importer`) se apartan a una carpeta de archivo; no se borran.
- **Escenarios** distintos, solo decorativos al principio.
- Ya se ve en 3D (feature 013) y el GLB va comprimido con meshopt (TA-70; KTX2 choca con la CSP). Queda: fps en un celular real. Los estáticos ya salen precomprimidos (brotli) por `adapter-node`; el GLB es la excepción (Kit no comprime `.glb`) y se acepta hasta que haya `compress` en Traefik (TA-71, cerrada). Los clips de Mixamo van en el repo dentro del GLB (decisión del owner, 2026-11-05).

### Un solo "humano por dentro"
Todas las botargas comparten **un esqueleto** y unas proporciones base. Los personajes del torneo son disfraces completos sobre ese cuerpo; cada uno conserva sus propios ataques especiales en el torneo. Así una prenda le queda a todos sin hacer N versiones.

Cómo se ajustan prendas a cabezas y cuerpos de distinto tamaño (a validar en el piloto):
- **Accesorios rígidos** (gorro, casco, guantes rígidos): se pegan a un hueso. Cada personaje declara un **punto de anclaje** (posición, rotación y escala) por ranura, por ejemplo `head_top`. El mismo gorro se escala y se coloca según la cabeza de cada uno.
- **Prendas deformables** (chaleco, pantalón): se pesan sobre el esqueleto común. Las diferencias de volumen se cubren con pocos **tipos de cuerpo** (por ejemplo flaco, normal y panzón) hechos como shape keys del cuerpo base; cada prenda trae las mismas shape keys, y el personaje elige su tipo.
- Bajo una prenda se oculta la parte del cuerpo que tapa, para evitar cruces de mallas.
- Si un personaje se sale del rango (cabeza enorme, cola), su ranura se marca incompatible en lugar de forzar el ajuste.

### Skins
Ranuras: gorro, guantes, chaleco, pantalón y botas, para todos los personajes. También disfraces completos de los participantes del torneo. Arte de las prendas: concepto con IA de imagen y modelado con Tripo AI.

### Botarga genérica y caras
- La botarga genérica (la de cada jugador nuevo) es celeste pastel con short azul marino, sin pelo y con un cierre en la espalda del torso. Es el molde de la contextura "normal".
- Se modela **sin cara**: cabeza lisa con orejas y nariz, y una zona fija de la cabeza (con su propia isla de UV) reservada para la cara.
- **La cara es una ranura más**: una textura pintada (ojos, cejas y boca) sobre esa zona. Cambiar de cara es cambiar una imagen; no hay que modelar ni riggear. Todas las caras respetan la misma plantilla para encajar y no quedar tapadas por gorros o cascos.
- Cara por defecto: "novato entusiasta" (ojos grandes, boca abierta y diente separado). Las demás de la hoja de exploración del 2026-11-04 (decidido, juguetón, nervioso valiente, mascota clásica, relajado) quedan como caras desbloqueables.
- **Expresiones en la pelea**: la cara cambia según el `FightLog` (guardia, golpe recibido, victoria, KO). Al inicio cada cara trae solo su expresión base, y el golpe y el KO usan expresiones genéricas compartidas. Más adelante, las caras de la tienda pueden traer sus cuatro expresiones propias.
- Los personajes del torneo tienen su cara como parte del diseño y no se cambia. Un disfraz completo trae su propia cabeza, y entonces su cara manda: la ranura de cara solo existe con la cabeza genérica.

## Acceso
Las peleas del torneo se ven sin iniciar sesión. Para apostar, cobrar las monedas diarias o tener botarga propia se inicia sesión con Google o Discord.

## Botarga propia y PvP asíncrono
- Cada usuario nuevo recibe una **botarga genérica** con stats estándar.
- **Buscar pelea** contra la botarga de otro usuario real, esté o no conectado: la pelea la resuelve el server con el motor. Al volver, el usuario ve cuántas peleas tuvo, el resultado y cuánto subió.
- **Todas las peleas se pueden ver** como repetición, porque se regeneran desde su semilla.
- **Sin límite duro de peleas por día**, con **rendimientos decrecientes**: después de cierto número de peleas en el día, ganan cada vez menos experiencia y monedas, y deja de moverse el ranking. Así nadie se va por toparse con un muro, y nadie domina el ranking por volumen.
- **Al ganar**: experiencia, monedas y puntos de ranking. **Al perder**: poca experiencia y se pierden puntos de ranking; nunca se pierden monedas.
- **Ranking** por rating (tipo Elo o Glicko), emparejando por rating, no solo por nivel.
- **Niveles**: al subir se reparten puntos de stats con un techo de presupuesto, se eligen ataques especiales (el básico lo tienen todos) y animaciones de victoria. Las builds abiertas exigen extender `just sim` para medir balance con builds de jugadores.
- **Nunca se venden stats**: lo que se compra es solo cosmético.

## Amigos y torneos privados
- Lista de amigos, retos directos y torneos personalizados con brackets.
- Los retos y torneos entre amigos **no dan puntos de ranking** ni cuentan para la progresión (así nadie se deja ganar para subir a otra cuenta), y no tienen límite.
- Apuestas internas con monedas de torneo de amigos, o torneos sin monedas, solo por diversión.

## Monedas
Dos monedas: la de **apuestas** (la del torneo público) y la de **torneos de amigos**. Las skins de la tienda se pueden comprar con cualquiera de las dos, a precios distintos. La de apuestas nunca se compra ni se vende. Lo demás sobre economía de pago está en `private/`.

## Preguntas abiertas
- ¿Cuántas peleas por día con recompensa completa antes de que empiece el decrecimiento?
- ¿Cómo se ganan las monedas de torneo de amigos sin pagar?
- ¿Los niveles tienen tope? ¿Hay temporadas que reinician el ranking?
- ¿Se puede apostar en peleas PvP de otros?
- ¿Las caras se pueden recolorear (cejas y boca) o cada una trae colores fijos?
