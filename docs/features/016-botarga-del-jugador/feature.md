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
En el flujo de la botarga propia (PvP asíncrono del [backlog](../../backlog.md), no el torneo), el jugador arma su luchador con lo que tiene desbloqueado: varios ataques especiales y golpes pesados, accesorios que ajustan sus stats y una animación de victoria y otra de idle. Los KO salen al azar. Al subir de nivel gana ataques nuevos (al azar, en niveles fijos) y otras recompensas. Todo tiene que nacer sin una build dominante, y las builds abiertas rompen el supuesto de hoy: `just sim` solo mide un roster fijo de personajes diseñados a mano.

Diseño previo con el `game-designer` y respuestas del owner (2026-11-05). Quedan preguntas abiertas al final; no se implementa hasta cerrarlas.

## Alcance
- Entra: las reglas del motor para builds de jugador (ruleset v2), el catálogo de habilidades y el de accesorios con su efecto en stats, la tabla de recompensas por nivel, el modo de builds abiertas de `just sim` y las animaciones cosméticas (victoria e idle elegidas, KO al azar).
- No entra: la persistencia de la botarga, la cuenta, el PvP, el cálculo de experiencia y el ranking (van en la feature de botarga propia y PvP, que aún no existe), las caras, los clips nuevos (se usan los de 015) y **la tienda**: en esta primera versión nada se compra, todo sale de subir de nivel (decisión del owner).

## Diseño

**Dos flujos, dos rulesets.** El torneo y las peleas del jugador son flujos separados que comparten el motor. Cada `FightLog` lleva su `ruleset_version`, así que el torneo sigue en v1 sin cambios y el PvP nace en v2. Migrar el torneo a v2 después es opcional; el owner está de acuerdo si hace falta.

**Ranuras de ataque.** Además del strike básico, que tienen todos, hay **4 ranuras de ataque: hasta 2 especiales y 2 pesados**. Cabe en `abilities_max = 4`. El tope por tipo es una regla nueva del ruleset (`specials_max`, `heavies_max`), no de la UI. Sin él, 4 especiales con cooldown 2 dan un especial en casi cada acción, con unas 3,7 veces el daño por ms del strike. Las ranuras se abren con el nivel (tabla de abajo).

**Cómo elige el motor (v2).** En v1 el peso es por habilidad: cada ranura de más sube la frecuencia total de habilidades, y la build óptima sería llenar todo. En v2 el peso de un tipo se **reparte** entre las habilidades disponibles de ese tipo: `w_kind / n_disponibles`, con división entera y el resto a la primera en orden de archivo (determinista). Una ranura de más da variedad, no frecuencia. Los cooldowns siguen por habilidad. Se descartó un cooldown compartido por tipo.

**Catálogo de habilidades.** `content/abilities/<id>.json` (`kind`, `power`, `cooldown`, `element`, `tier`) con nombres en `es` y `en`. El jugador elige ids de las que tiene desbloqueadas, nunca números: con números libres el óptimo es trivial y el espacio no se puede simular. Poder y cooldown se pagan entre sí; por ejemplo, especiales 100/8, 70/4 y 50/2, y pesados 55/2, 40/1 y 28/0. Cada habilidad trae su clip; mientras no haya uno propio, sale del hash de su id (015).

**Accesorios con stats.** Cada ranura de prenda (gorro, guantes, chaleco, pantalón y botas) puede traer un ajuste de stats, por ejemplo guantes con +8 de ataque y −8 de velocidad. Son **intercambios de suma cero**: un accesorio mueve puntos y no da puntos nuevos, y el total sigue bajo el presupuesto del nivel. Así cambian el estilo de pelea sin volverse poder que se compra el día que haya tienda (backlog: "nunca se venden stats"). Hay tope por stat (por ejemplo ±10 por accesorio y ±20 en total), y el mínimo de stats de v2 se aplica después de sumar los accesorios. El server resuelve la botarga con habilidades y accesorios a un `CharacterSpec` normal antes de pelear, así que `simulate` no cambia de firma.

**Niveles y recompensas.** Sin tope de nivel, pero con **tope de poder**: el presupuesto de stats sube de 250 a 300 hasta el nivel 10 (+5 por nivel) y ahí se queda. Desde ese nivel se sube por cosméticos y ranking, no por fuerza. Así los veteranos no aplastan a los nuevos, y el emparejamiento por rating basta. Boceto, a ajustar con `just sim`:

| Nivel | Recompensa |
|---|---|
| 1 | Botarga genérica, strike y 1 pesado inicial, 1 ranura abierta |
| 2, 4, 6, 8, 10 | Un ataque nuevo al azar del catálogo (del tier de ese nivel) |
| 3 / 5 / 7 | Se abre la 2.ª / 3.ª / 4.ª ranura |
| 9, 12, 15… | Accesorio, animación de victoria o de idle, o skin |

El sorteo del ataque lo hace el **server** (no el motor), entre las del tier que el jugador aún no tiene, y se guarda en el mismo paso que sube el nivel: reintentar no vuelve a sortear.

**Animaciones cosméticas.** Victoria e idle **no van en `CharacterSpec`** ni en el hash canónico: no afectan el resultado y cambiarlas no re-simula. Van en un tipo nuevo de `crates/protocol`, `FighterCosmetics { idle, victory }` (códigos de clip), junto a `FighterInfo` y fuera de lo que verifica el WASM. Sin cosméticos (torneo, logs viejos), el renderer hace lo de hoy. El KO al azar lo elige el **renderer** con un hash de la pelea y el lado, nunca con el PRNG del motor: todos los espectadores ven el mismo.

**`just sim` con builds abiertas.** Un modo `--builds N` genera N builds válidas al azar (habilidades, accesorios y stats dentro del presupuesto) con un PRNG propio de semilla fija (nunca la de una pelea, ADR-0002), corre en v2 y las empareja al azar. Criterios nuevos en `Criteria`:
- Win rate de las builds que incluyen cada habilidad y cada accesorio: 45 % a 55 %.
- Win rate por número de ranuras (1 a 4) y por mezcla de tipos: 45 % a 55 %.
- Builds dominantes: hill-climb desde 200 builds al azar, 30 pasos cambiando una pieza. Falla si alguna supera el 65 % contra la población.
- Duración, momentos y `time_up` dentro de los criterios RF17 de hoy.

`--check` corre una versión chica en `just check`; el informe completo, a mano.

**Huecos de una build libre** (para v2, a medir): con `stat_min = 1` se puede volcar todo en pocos stats; conviene un mínimo de 20 por stat (el roster va de 30 a 95). La personalidad y las debilidades libres también tienen óptimos triviales: ver las preguntas abiertas.

## Fases
1. Motor: ruleset v2 (tope por tipo, reparto del peso, mínimo de stats) y sus goldens. El torneo no cambia.
2. Catálogos de habilidades y de accesorios en `content/` y su carga en `crates/content`.
3. `just sim --builds` con los criterios nuevos.
4. `FighterCosmetics` en `protocol` (`just gen`), KO al azar por hash y victoria e idle elegidos en el renderer.
5. Server y UI (niveles, sorteo, inventario y armado de la build): depende de la feature de botarga propia y PvP.

## Riesgos
- **Balance:** una habilidad, un accesorio o un combo dominante. Se evita con catálogos cerrados, accesorios de suma cero y `--builds` en `just check`.
- **Determinismo:** el reparto con división entera tiene que ser estable; goldens de v2 y re-simulación de logs v1 idéntica.
- **Entrada del usuario:** la build llega del cliente. El server la valida en el borde contra lo que el jugador tiene desbloqueado, además de `validate`. El sorteo de recompensas no se puede repetir.
- **Tienda futura:** cuando se compren accesorios o animaciones con monedas, esa feature es `risk: high` y pasa por el `security-reviewer`. Los accesorios en venta tienen que seguir siendo de suma cero.

## Pruebas
- [ ] `just check` en verde.
- [ ] Motor: v2 rechaza 3 especiales o 3 pesados; con 1 o 2 habilidades de un tipo, ese tipo sale con la misma frecuencia (semillas fijas); los logs v1 se re-simulan idénticos.
- [ ] Un accesorio que suma puntos netos o pasa el tope por stat se rechaza.
- [ ] `just sim --check` (torneo, v1) y `just sim --builds N --check` (v2) pasan.
- [ ] Renderer: con `FighterCosmetics` salen la victoria y el idle elegidos; sin ellos, lo de hoy; el KO es el mismo en dos cargas de la misma pelea.

## Respuestas del owner (2026-11-05)
- Animaciones de victoria e idle: se desbloquean como los accesorios (en el futuro también se compran). Los ataques se ganan al subir de nivel, uno al azar en niveles fijos. Otros niveles dan otras recompensas.
- Tienda: fuera de esta versión.
- El jugador equipa lo que tiene disponible: accesorios, skins y ataques.
- Los accesorios afectan los stats.
- Torneo y PvP son flujos separados; de acuerdo con v2.

## Preguntas abiertas al owner
1. **Accesorios de suma cero:** ¿de acuerdo con que muevan stats sin sumar (por ejemplo +8 ataque y −8 velocidad)? Si suman puntos, la tienda vendería poder.
2. **Tope de poder:** ¿presupuesto de 250 a 300 hasta el nivel 10 y después solo cosméticos, sin tope de nivel?
3. **Sorteo de ataques:** ¿uno al azar sin elegir, o elegir 1 de 3 al azar (evita quedarse con algo que no le sirve)?
4. **Personalidad:** ¿libre con presupuesto, por presets ligados a la cara, o fija?
5. **Elementos y debilidades:** ¿cada habilidad elemental trae una debilidad fija (fuego trae agua), o la asigna el sistema?
