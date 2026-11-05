---
id: "016"
title: Botarga del jugador con varios especiales y animaciones
type: feature
status: draft
risk: medium
author: claude
linear: TA-55
depends_on: ["003", "015"]
created: 2026-11-05
updated: 2026-11-05
---

# Botarga del jugador con varios especiales y animaciones

## Qué y por qué
En el flujo de la botarga propia (PvP asíncrono del [backlog](../../backlog.md), no el torneo), el jugador arma su luchador: elige varios ataques especiales y golpes pesados, una animación de victoria y una de idle. Los KO salen al azar. Es la base de la progresión ("al subir se eligen ataques especiales y animaciones de victoria") y tiene que nacer sin una build dominante, porque las builds abiertas rompen el supuesto de hoy: `just sim` solo mide un roster fijo de personajes diseñados a mano.

Este documento es **diseño previo** (consulta al `game-designer`, 2026-11-05). Las decisiones de producto quedan abiertas al final; no se implementa hasta que el owner las conteste.

## Alcance
- Entra: las reglas del motor para builds de jugador (ruleset v2), un catálogo cerrado de habilidades, el modo de builds abiertas de `just sim` y las animaciones cosméticas elegidas (victoria e idle) y al azar (KO).
- No entra: la persistencia de la botarga, la cuenta, el PvP, la experiencia ni los niveles (van en la feature de botarga propia y PvP, que aún no existe), la tienda, las skins y las caras, y clips nuevos (se usan los de 015: 4 especiales, 2 pesados, 2 KO, 3 victorias y 5 idles).

## Diseño

**Ranuras.** Hasta 2 especiales y 2 pesados, con un total de 1 a 4. Cabe en `abilities_max = 4`. El tope por tipo es una regla nueva del ruleset (`specials_max`, `heavies_max`), no de la UI. Sin tope por tipo, 4 especiales con cooldown 2 dan un especial en casi cada acción, a unas 3,7 veces el daño por ms del strike. El roster actual cumple (el fontanero va 2+2).

**Cómo elige el motor (ruleset v2).** Hoy el peso es por habilidad, así que cada ranura de más sube la frecuencia total de habilidades, y la build óptima sería llenar todo. En v2 el peso de un tipo se **reparte** entre las habilidades disponibles de ese tipo: `w_kind / n_disponibles`, división entera, y el resto va a la primera en orden de archivo (determinista). Así una ranura de más da variedad, no frecuencia: "más ataques no te hacen atacar más, te hacen menos predecible". Los cooldowns siguen siendo por habilidad. Descartado: un cooldown compartido por tipo, que cambia más la sensación de los personajes del torneo con varias habilidades.

v2 cambia los goldens, así que es una versión nueva y v1 queda intacta para re-simular peleas viejas (ADR-0001). El torneo y el PvP pasan juntos a v2 para no mantener dos reglas vivas; antes se corre `just sim`, porque el fontanero y el guerrero pierden frecuencia de habilidades.

**Catálogo cerrado.** `content/abilities/<id>.json` (id, `kind`, `power`, `cooldown`, `element`, `min_level`) y nombres en `es` y `en`. El jugador elige ids, nunca números: con números libres el óptimo es trivial (poder máximo, cooldown mínimo) y el espacio no se puede simular. Poder y cooldown se pagan entre sí; por ejemplo, especiales 100/8, 70/4 y 50/2, y pesados 55/2, 40/1 y 28/0. Los valores finales los ajusta el `game-designer` con `just sim`. `crates/content` carga el catálogo y el motor lo valida igual que una habilidad de personaje; la botarga se resuelve a un `CharacterSpec` normal antes de pelear, así que `simulate` no cambia de firma.

**Animaciones.** Victoria e idle son cosméticas: **no van en `CharacterSpec`** ni en el hash canónico, porque no afectan el resultado y cambiarlas no debe re-simular. Van en un tipo nuevo de `crates/protocol`, `FighterCosmetics { idle, victory }` (códigos de clip), junto a `FighterInfo` y fuera de lo que verifica el WASM. Sin cosméticos (torneo, logs viejos), el renderer hace lo de hoy. El clip de cada habilidad sigue saliendo del hash de su id (015); elegirlo a mano queda como pregunta abierta. El KO al azar lo elige el **renderer** con un hash de la pelea y el lado, nunca con el PRNG del motor: todos los espectadores ven el mismo y no se consumen tiradas.

**`just sim` con builds abiertas.** Un modo `--builds N` genera N builds válidas al azar desde el catálogo con un PRNG propio de semilla fija (nunca la de una pelea, ADR-0002) y las empareja al azar. Criterios nuevos en `Criteria`:
- Win rate de las builds que incluyen cada habilidad del catálogo: 45 % a 55 %.
- Win rate por número de ranuras (1 a 4) y por mezcla de tipos: 45 % a 55 %. Una sola ranura debe ser una elección legítima, no una trampa.
- Builds dominantes: hill-climb desde 200 builds al azar, 30 pasos cambiando una pieza. Falla si alguna supera el 65 % contra la población.
- Personajes del torneo contra builds de nivel máximo: 40 % a 60 %.
- Duración, momentos y `time_up` dentro de los criterios RF17 de hoy.

`--check` corre una versión chica en `just check` (`crates/sim/tests/balance.rs`); el informe completo, a mano.

**Huecos de una build libre** (para v2, a medir con `just sim`): con `stat_min = 1` se puede volcar todo en pocos stats, y conviene un mínimo de 20 por stat para jugadores (el roster va de 30 a 95). La personalidad y las debilidades libres también tienen óptimos triviales (showmanship 100 con aggression 0; cero debilidades); ver las preguntas al owner.

## Fases
1. Motor: ruleset v2 (tope por tipo, reparto del peso, mínimo de stats), goldens v2 y paso del torneo a v2 con `just sim` en verde.
2. Catálogo de habilidades en `content/abilities/` y su carga en `crates/content`.
3. `just sim --builds` con los criterios nuevos.
4. `FighterCosmetics` en `protocol` (`just gen`), KO al azar por hash y victoria e idle elegidos en el renderer (cuando termine TA-52/TA-53).
5. Server y UI para elegir la build: depende de la feature de botarga propia y PvP.

## Riesgos
- **Balance:** una habilidad o un combo dominante en PvP. Se evita con el catálogo cerrado y los criterios de `--builds` en `just check`.
- **Determinismo:** el reparto del peso con división entera tiene que ser estable; tests de golden en v2 y re-simulación de logs v1 sin cambios.
- **Entrada del usuario:** la build llega del cliente. El server la valida en el borde contra el catálogo, el nivel y los desbloqueos, además de `validate`. Si el owner decide que algo se desbloquea con monedas, la feature pasa a `risk: high` y necesita el pase del `security-reviewer`.
- **El torneo cambia de sensación** al pasar a v2; se mide con `just sim` antes de mergear la fase 1.

## Pruebas
- [ ] `just check` en verde.
- [ ] Motor: v2 rechaza 3 especiales o 3 pesados; el reparto da la misma frecuencia por tipo con 1 o 2 habilidades (test estadístico sobre semillas fijas); los logs v1 se re-simulan idénticos.
- [ ] `just sim --check` (roster en v2) y `just sim --builds N --check` pasan.
- [ ] Renderer: con `FighterCosmetics` sale la victoria y el idle elegidos; sin ellos, lo de hoy; el KO es el mismo en dos cargas de la misma pelea.

## Preguntas abiertas al owner
1. **Ranuras:** ¿2 especiales y 2 pesados (máximo 4) como recomienda el game-designer, u otro número? ¿Se desbloquean por nivel (nivel 1 con un pesado, el máximo con todo)?
2. **Progresión de stats:** (A) el presupuesto sube con el nivel, por ejemplo de 250 a 300, que se siente pero obliga a emparejar también por nivel; o (B) queda fijo en 300 y solo se desbloquean opciones, más justo en PvP. ¿Hay tope de nivel?
3. **Animaciones:** ¿la victoria y el idle se eligen libres entre los clips que hay, o se desbloquean (por nivel, por logros)? ¿El jugador elige también el clip de cada especial o sigue el hash del id (015)?
4. **Desbloqueo:** ¿todo sale de subir de nivel, o algo (animaciones) se compra con monedas en la tienda? Lo segundo hace la feature sensible.
5. **Personalidad:** ¿libre con presupuesto (por ejemplo suma 150), por presets ligados a la cara o un arquetipo, o fija?
6. **Elementos y debilidades:** ¿los jugadores tienen elemento? Si sí, ¿cada habilidad elemental trae una debilidad fija (fuego trae agua) o la asigna el sistema?
7. **Ruleset v2 en el torneo:** ¿de acuerdo con pasar el torneo a v2 a la vez que el PvP, aunque cambie un poco cómo pelean el fontanero y el guerrero?
