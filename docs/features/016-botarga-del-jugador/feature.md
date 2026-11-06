---
id: "016"
title: Botarga del jugador con varios especiales y animaciones
type: feature
status: approved
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

Diseño previo con el `game-designer` y respuestas del owner (2026-11-05), aprobado por el owner el mismo día.

## Alcance
- Entra: las reglas del motor para builds de jugador (ruleset v2), el catálogo de habilidades y el de accesorios con su efecto en stats, la tabla de recompensas por nivel, el modo de builds abiertas de `just sim` y las animaciones cosméticas (victoria e idle elegidas, KO al azar).
- No entra: la persistencia de la botarga, la cuenta, el PvP, el cálculo de experiencia y el ranking (van en la feature de botarga propia y PvP, que aún no existe), las caras, los clips nuevos (se usan los de 015) y **la tienda**: en esta primera versión nada se compra, todo sale de subir de nivel (decisión del owner).

## Diseño

**Dos flujos, dos rulesets.** El torneo y las peleas del jugador son flujos separados que comparten el motor. Cada `FightLog` lleva su `ruleset_version`, así que el torneo sigue en v1 sin cambios y el PvP nace en v2. Migrar el torneo a v2 después es opcional; el owner está de acuerdo si hace falta.

**Ranuras de ataque.** Además del strike básico, que tienen todos, hay **4 ranuras de ataque: hasta 2 especiales y 2 pesados**. Cabe en `abilities_max = 4`. El tope por tipo es una regla nueva del ruleset (`specials_max`, `heavies_max`), no de la UI. Sin él, 4 especiales con cooldown 2 dan un especial en casi cada acción, con unas 3,7 veces el daño por ms del strike. Las ranuras se abren con el nivel (tabla de abajo).

**Cómo elige el motor (v2).** En v1 el peso es por habilidad: cada ranura de más sube la frecuencia total de habilidades, y la build óptima sería llenar todo. En v2 el peso de un tipo se **reparte** entre las habilidades disponibles de ese tipo: `w_kind / n_disponibles`, con división entera y el resto a la primera en orden de archivo (determinista). Una ranura de más da variedad, no frecuencia. Los cooldowns siguen por habilidad. Se descartó un cooldown compartido por tipo.

**Catálogo de habilidades.** `content/catalog/abilities.json` (`kind`, `power`, `cooldown`, `element`, `tier` y un `starter`), junto a `accessories.json`, `styles.json` y los nombres en `names.es.json` y `names.en.json`. Lo carga `content::load_catalog`; las reglas del armado (nivel, presupuesto, ranuras, accesorios y debilidades) viven en `engine::loadout`, que resuelve una botarga a un `CharacterSpec`. Esas reglas no son parte del ruleset: la pelea solo necesita el spec resuelto. El jugador elige ids de las que tiene desbloqueadas, nunca números: con números libres el óptimo es trivial y el espacio no se puede simular. Poder y cooldown se pagan entre sí; por ejemplo, especiales 100/8, 70/4 y 50/2, y pesados 55/2, 40/1 y 28/0. Cada habilidad trae su clip; mientras no haya uno propio, sale del hash de su id (015).

**Accesorios con stats.** Cada ranura de prenda (gorro, guantes, chaleco, pantalón y botas) puede traer un ajuste de stats, por ejemplo guantes con +8 de ataque y −8 de velocidad. Son **intercambios de suma cero**: un accesorio mueve puntos y no da puntos nuevos, y el total sigue bajo el presupuesto del nivel. Así cambian el estilo de pelea sin volverse poder que se compra el día que haya tienda (backlog: "nunca se venden stats"). Hay tope por stat (por ejemplo ±10 por accesorio y ±20 en total), y el mínimo de stats de v2 se aplica después de sumar los accesorios. El server resuelve la botarga con habilidades y accesorios a un `CharacterSpec` normal antes de pelear, así que `simulate` no cambia de firma.

**Niveles y recompensas.** **Tope en el nivel 100** (owner), con la progresión repartida en todo el camino: el presupuesto de stats sube de 200 en el nivel 1 a 300 en el 100 (+1 por nivel, más el nivel 1), así cada nivel se nota y el máximo iguala al roster del torneo. Los ataques, las ranuras y los cosméticos se reparten hasta el 100. Boceto, a ajustar con `just sim`:

| Nivel | Recompensa |
|---|---|
| 1 | Botarga genérica, strike, 1 pesado inicial y 1 ranura abierta |
| 5 / 15 / 30 | Se abre la 2.ª / 3.ª / 4.ª ranura |
| Cada 5 (salvo los de ranura) | Elegir 1 de 3 ataques nuevos al azar del tier del nivel |
| Cada 10 | Un cosmético: accesorio, victoria, idle o skin |
| 25, 50, 75, 100 | Hito: un cosmético especial del tier |

Tiers del catálogo cada 20 niveles (1-19, 20-39, …, 80-100). Los ataques de tier alto no son más fuertes: tienen efectos más vistosos o combinaciones raras de poder y cooldown, porque la fuerza ya la da el presupuesto. Con unas 17 tiradas hasta el 100, el catálogo necesita al menos 3 ataques que el jugador no tenga en cada tirada, unos 24 a 30 en total.

**Sorteo de ataques.** Lo hace el **server** (no el motor): saca 3 ataques **que el jugador no tiene**, del tier de su nivel o de uno inferior; el jugador elige 1. Si quedan menos de 3 sin tener, se ofrecen los que queden; si no queda ninguno, la tirada da un cosmético. Las 3 opciones se guardan en el mismo paso que sube el nivel y quedan pendientes hasta que elija: recargar o reintentar no vuelve a sortear.

**Personalidad (estilo de pelea).** En el motor todo luchador tiene tres rasgos de 0 a 100 que deciden cómo pelea: `aggression` (más golpes y pesados), `caution` (bloquea más) y `showmanship` (más especiales). Libres tienen un óptimo trivial, así que la botarga elige un **estilo** de una lista fija (equilibrado, agresivo, defensivo, showman), cada uno con valores ya balanceados. Se cambia libre, como un ataque equipado.

**Elementos y debilidades.** La botarga no tiene elemento propio: lo dan sus ataques. Cada ataque elemental equipado trae una debilidad fija según un ciclo: fuego es débil a agua, agua a eléctrico, eléctrico a tierra, tierra a aire, aire a hielo y hielo a fuego. Se quitan las repetidas, y una build con más de 2 debilidades no es válida (`weaknesses_max = 2`), así que se combinan como mucho 2 elementos. Es un intercambio: el ataque elemental pega más a quien es débil a él, pero te expone. Los ataques sin elemento no traen debilidad. El jugador cambia sus debilidades cambiando sus ataques equipados.

La regla de daño es la de hoy y no cambia: si el elemento del ataque está entre las debilidades del rival, el daño se multiplica por 5/4 (`weak_num`/`weak_den`), golpee o sea bloqueado. No hay bonus por mismo elemento (empujaría a builds de un solo elemento) ni resistencias o daño reducido; un ataque contra un elemento que no es debilidad pega normal. Los personajes del torneo no cambian: no tienen elemento propio y sus debilidades están puestas a mano en su `character.json`.

**Animaciones cosméticas.** Victoria e idle **no van en `CharacterSpec`** ni en el hash canónico: no afectan el resultado y cambiarlas no re-simula. Van en un tipo nuevo de `crates/protocol`, `FighterCosmetics { idle, victory }` (códigos de clip), junto a `FighterInfo` y fuera de lo que verifica el WASM. Sin cosméticos (torneo, logs viejos), el renderer hace lo de hoy. El KO al azar lo elige el **renderer** con un hash de la pelea y el lado, nunca con el PRNG del motor: todos los espectadores ven el mismo.

**`just sim` con builds abiertas.** Un modo `--builds N` genera N builds válidas al azar (habilidades, accesorios y stats dentro del presupuesto) con un PRNG propio de semilla fija (nunca la de una pelea, ADR-0002), corre en v2 y las empareja al azar. Criterios nuevos en `Criteria`:
- Win rate de las builds que incluyen cada habilidad y cada accesorio: 45 % a 55 %.
- Win rate por número de ranuras (1 a 4) y por mezcla de tipos: 45 % a 55 %.
- Builds dominantes: hill-climb desde 200 builds al azar, 30 pasos cambiando una pieza. Falla si alguna supera el 65 % contra la población.
- Duración, momentos y `time_up` dentro de los criterios RF17 de hoy.

`--check` corre una versión chica en `just check`; el informe completo, a mano.

Ajustes de la implementación (fase 3):
- Cada build llena todas las ranuras que su nivel abre, como haría un jugador. Por eso el criterio "por número de ranuras" no se juzga: con más ataques se gana más (con 1 ranura, 27 %; con 4, 73 %), y eso es progresión, no un desbalance dentro de un nivel.
- La dominancia se mide entre las builds escaladas, no contra la población al azar, que tiene muchas builds flojas. La ganadora se vuelve a medir con semillas nuevas.
- Con el catálogo ajustado por el `game-designer`, el nivel 100 pasa sin fallos con dos semillas, y la mejor build queda entre 53 % y 65 %.
- En niveles bajos queda dominancia (nivel 30: 66-68 %; nivel 5: 66-72 %) y, en el nivel 5, la mezcla de tipos (1 pesado y 1 especial: 58 %; 2 pesados: 40 %). El contenido ya no lo arregla; son reglas del motor (ver Preguntas abiertas).

Además, win rate por banda de nivel contra la misma banda (que ningún nivel tenga una build rota) y la ventaja de 10 niveles de diferencia, para fijar cuánta distancia de nivel tolera el emparejamiento.

**Mínimo de stats** (v2, a medir): con `stat_min = 1` se puede volcar todo en pocos stats; conviene un mínimo de 20 por stat (el roster va de 30 a 95). Con 200 de presupuesto en el nivel 1 cabe.

## Fases
1. Motor: ruleset v2 (tope por tipo, reparto del peso, mínimo de stats) y sus goldens. El torneo no cambia.
2. Catálogos de habilidades y de accesorios en `content/` y su carga en `crates/content`.
3. `just sim --builds` con los criterios nuevos.
4. `FighterCosmetics` en `protocol` (`just gen`), KO al azar por hash y victoria e idle elegidos en el renderer.
5. Server y UI (niveles, sorteo, inventario y armado de la build): depende de la feature de botarga propia y PvP.

## Riesgos
- **Balance:** una habilidad, un accesorio o un combo dominante. Se evita con catálogos cerrados, accesorios de suma cero y `--builds` en `just check`.
- **Determinismo:** el reparto con división entera tiene que ser estable; goldens de v2 y re-simulación de logs v1 idéntica.
- **Distancia de nivel:** entre el nivel 1 (200) y el 100 (300) hay 100 puntos de stats. El emparejamiento por rating lo cubre en parte; la feature de PvP tiene que limitar la diferencia de nivel con el dato de `--builds`.
- **Entrada del usuario:** la build llega del cliente. El server la valida en el borde contra lo que el jugador tiene desbloqueado, además de `validate`. El sorteo de recompensas no se puede repetir.
- **Tienda futura:** cuando se compren accesorios o animaciones con monedas, esa feature es `risk: high` y pasa por el `security-reviewer`. Los accesorios en venta tienen que seguir siendo de suma cero.

## Pruebas
- [ ] `just check` en verde.
- [x] Motor: v2 rechaza 3 especiales o 3 pesados; en cada elección, el peso de un tipo es el mismo con 1 o 2 habilidades disponibles; los logs v1 se re-simulan idénticos. A lo largo de una pelea, una segunda habilidad sí cubre el cooldown de la primera y el tipo sale algo más: lo mide `just sim --builds` (win rate por número de ranuras).
- [x] Un accesorio que suma puntos netos o pasa el tope por stat se rechaza; una build con más de 2 debilidades también.
- [ ] Sorteo: las 3 opciones nunca incluyen un ataque que el jugador ya tiene, y reintentar devuelve las mismas.
- [x] `just sim --check` (torneo, v1) y `just sim --builds 2000 --fights 50000 --check` (v2, nivel 100) pasan; una versión chica corre en `just check`.
- [ ] Renderer: con `FighterCosmetics` salen la victoria y el idle elegidos; sin ellos, lo de hoy; el KO es el mismo en dos cargas de la misma pelea.

## Respuestas del owner (2026-11-05)
- Animaciones de victoria e idle: se desbloquean como los accesorios (en el futuro también se compran). Los ataques se ganan al subir de nivel, uno al azar en niveles fijos. Otros niveles dan otras recompensas.
- Tienda: fuera de esta versión.
- El jugador equipa lo que tiene disponible: accesorios, skins y ataques.
- Los accesorios afectan los stats.
- Torneo y PvP son flujos separados; de acuerdo con v2.
- Accesorios de suma cero: sí.
- Tope en el nivel 100, con la progresión (stats, ataques, skins) repartida hasta ahí.
- Sorteo: elegir 1 de 3, siempre ataques que todavía no tiene.
- Estilo de pelea de una lista fija: sí.
- Debilidades según los ataques elementales equipados: sí.
- La regla de daño elemental queda como hoy (×5/4 contra una debilidad).

## Preguntas abiertas al owner
1. **Peso del tipo vacío (v2):** con pocas ranuras, el peso de un tipo sin ataques disponibles se pierde, y tener un ataque de cada tipo gana más. ¿Ese peso pasa al golpe básico o al otro tipo?
2. **Stat especial (v2):** cuenta tres veces (ataque y defensa de los especiales y probabilidad de crítico), y por eso domina en las builds optimizadas. ¿Que el crítico dependa de la velocidad, o que la defensa contra especiales use la defensa?
