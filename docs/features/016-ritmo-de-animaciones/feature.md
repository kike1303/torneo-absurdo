---
id: "016"
title: Ritmo propio de las animaciones 3D
type: feature
status: draft
risk: low
author: claude
linear: TA-53
depends_on: ["013", "015"]
created: 2026-11-05
updated: 2026-11-05
---

# Ritmo propio de las animaciones 3D

## Qué y por qué
Los movimientos de la arena 3D se ven muy rápidos. La causa: la capa 3D toma los tiempos de los manifests de sprites (por ejemplo, `attack_light` dura 330 ms con 120 ms de preparación), pensados para pocos cuadros dibujados. Un clip de Mixamo de 1,8 s metido en ese tiempo corre 4 a 6 veces más rápido que su velocidad natural.

Decisión del owner (2026-11-05): solo ajustar las animaciones, sin tocar el motor ni la duración de las peleas.

## Alcance
- Entra: un ritmo propio de la botarga para los ataques y las reacciones, aplicado por la capa 3D a los manifests que recibe.
- No entra: el motor, el ruleset, la duración de las peleas, el renderer 2D y los manifests de `placeholders/`.

## Diseño
**`src/model/pacing.ts` (puro).** `paceManifest` reescribe solo `frame_ms` de las acciones de `BOTARGA_PACE`: la parte antes del impacto suma el `windup` y la de después, `total − windup`. Las demás animaciones, y lo que no reconoce, pasan igual, y `validateAssets` sigue revisando todo.

| Acción | Sprite (antes) | Botarga |
|---|---|---|
| `attack_light` | 120 / 330 ms | 260 / 800 ms |
| `attack_heavy` | 300 / 670 ms | 360 / 1100 ms |
| `special` | 320 / 740 ms | 400 / 1300 ms |
| `hit`, `block`, `dodge` | 400, 350, 400 ms | 750, 650, 750 ms |

Los windups quedan en `LIMITS.windupMaxMs` (400 ms) o menos: así se mantiene la garantía de la coreografía de 002 (la reacción a un golpe siempre se ve) y todos los timelines válidos lo siguen siendo. El impacto del clip sigue cayendo en el impacto del cue (013), así que el daño y los números no se mueven.

**Dónde se aplica.** `createRenderer3d` envuelve `load` y `loadLive` con `paceAssets`. La web y el 2D no cambian.

## Riesgos
- Una recuperación más larga puede quedar cortada por el siguiente golpe del rival: la coreografía ya lo resuelve (gana el segmento más nuevo) y el fundido de 200 ms lo suaviza.
- Los clips siguen siendo más largos que el tiempo disponible (un `attack_light` de 1,8 s en 800 ms): van a ~2× en vez de 4–6×. Ir más lento ya necesita peleas más largas (motor).

## Pruebas
- [ ] `just check` en verde.
- [x] Unit: cada acción suma su ritmo y su windup, los windups ≤ 400 ms, los manifests siguen válidos, es más lento que el sprite, lo demás pasa igual y los timelines golden y el spike siguen válidos.
- [ ] e2e general y `e2e-fights` en local; revisión visual en `just dev`.
