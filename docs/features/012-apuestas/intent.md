---
id: "012"
title: Apuestas parimutuel, bono diario y rescate
type: intent
status: approved
risk: high
author: game-designer
reviewers: [architect]
approvals: [{by: architect, date: 2026-11-02}]
linear: TA-27
depends_on: ["008", "010"]
created: 2026-11-02
updated: 2026-11-02
---

# Intent: Apuestas parimutuel, bono diario y rescate

## Problema
El motor, el renderer, la auth, el ledger base y las peleas en vivo en la home ya existen, pero no se puede apostar: el usuario mira una pelea sin nada en juego. Falta el pilar "Algo en juego" de la visión, la pieza principal que le queda al MVP.

## Resultado propuesto
- **Apostar:** con sesión, el usuario apuesta `Coins` a un lado mientras la ventana está abierta y ve cómo se mueve el pozo.
- **Cobrar:** al liquidarse la pelea ve su pago, su pérdida o su reembolso. Un empate o una pelea anulada le devuelven lo apostado.
- **Seguir jugando:** reclama el bono de cada día UTC y, si se queda sin monedas, el rescate.
- **Economía protegida:** la carencia y el tope por pelea rigen desde la primera apuesta, y cada apuesta deja lo necesario para clasificarla después (`RankedProfit`).
- **Pelea irrecuperable:** el operador tiene una salida acotada que reembolsa a todos, incluso si el secreto se perdió.

## Usuarios y sistemas afectados
- Usuarios: apostadores con sesión, incluidas las cuentas en carencia. El espectador anónimo sigue viendo la pelea, pero no apuesta.
- Operador: el comando de anulación.
- Sistemas: `apps/server`, `apps/web`, `crates/protocol` y `docs/decisions/` (ADR-0002).

## Restricciones
- ADR-0005: sin dinero real, la casa no arriesga nada, todo movimiento es un asiento del ledger y la liquidación es idempotente.
- ADR-0008 D6, D7 y D8 fijan controles, códigos de error y límites. Esta feature los cumple, no los rediseña.
- 010 RF29 a RF33 y RF59 fijan el contrato con el ciclo de la pelea.
- ADR-0002: el resultado y la semilla siguen ocultos hasta `Settled`, sin excepciones. En `Settled` se publica la semilla, salvo en `voided(secret_lost)`, la enmienda de abajo.
- **Enmienda de ADR-0002** para `voided(secret_lost)`: aceptarla pasa por el gate de ADRs de `docs/process.md`, donde se le muestra el riesgo al owner, y por security-reviewer. Una anulación sin semilla no se puede refutar nunca y saca una pérdida del ranking.
- UI con i18n en/es, mobile-first y sobre el design system (ADR-0007, ADR-0009).
- Todo se valida en local.

## Criterios de éxito
Todos bloquean el merge. Los números son los tests mínimos de ADR-0008.

**Criterio del owner**
- [ ] En local, un usuario apuesta en una ventana abierta, ve su pago o reembolso al liquidarse la pelea y reclama bono y rescate.

**Apuestas y economía (ADR-0008)**
- [ ] Test 7: reclamos concurrentes dan exactamente un bono, e igual con el rescate.
- [ ] Test 8: una apuesta abierta que deja el saldo libre en 0 impide el rescate.
- [ ] Test 18, parte de TA-27: la reserva de capacidad del socket según la carencia.
- [ ] Test 23: dos apuestas concurrentes que juntas pasan el tope, se acepta exactamente una.
- [ ] Test 24: la carencia limita a `1 B` por pelea, y los reembolsos no ayudan a salir de ella.
- [ ] Test 26: sin el bono del día no hay rescate.
- [ ] `bets.ranked_stake` es una foto al crear cada apuesta, probada con una cuenta en carencia y otra fuera de ella.
- [ ] Test 25 completo, de (a) a (g), sobre el `RankedProfit` de cada apuesta, calculado al liquidar y guardado de forma inmutable (ver "Preguntas abiertas", 2).

**Liquidación y anulación (010)**
- [ ] Reembolso real con asientos del ledger para `draw`, para la anulada y, si la spec adopta la recomendación de la pregunta 3, para todo pozo con un lado vacío. Ninguno cuenta como apuesta liquidada ni suma `RankedProfit`.
- [ ] RF31: el gancho no tiene fallos alcanzables por la API ni depende de datos que cambien tras apostar, con pruebas de cuenta baneada, apuestas en los topes y pozo de un solo lado.
- [ ] RF59 completo, disponible antes de la primera apuesta, con cada negativa, cada razón, el reembolso y ningún log con la semilla.
- [ ] `log_without_end` se niega.
- [ ] Antes de anular por `engine_error` se prueba el binario anterior (preocupación 3 de 010).

**Secreto perdido (preocupación 13 de 010)**
- [ ] La enmienda de ADR-0002 está aceptada antes del merge.
- [ ] `secret_lost` solo procede si la fila del secreto no existe.
- [ ] El comando se niega mientras pueda haber emisión en curso, hasta el plazo máximo de la pelea más un margen.
- [ ] Hay un contador público de estas anulaciones, y cada caso reabre la aceptación de la preocupación 1.
- [ ] "No verificable" es un estado propio en RF7 y en la vista: una semilla nula nunca pasa como comprobación omitida, y una `voided` de otra razón sin semilla sigue fallando.
- [ ] Solo se llega tras las negativas: no adelanta la de versión ausente, y un secreto que no coincide con el commit nunca es `secret_lost`.

## Fuera de alcance
- Temporadas, `season_stats`, leaderboard, `farm-report` y `season close` (TA-28).
- Despliegue.
- Tope propio para el main event: esta feature no lo sube.

## Preguntas abiertas
1. **Valores de `B`, `m` y `r`** (`R = 0.5 B` ya está fijado). Recomendación, a validar en la spec: `B = 100`, `m = 5`, `r = 5 %`. Da `R = 50`, tope de 500 (100 en carencia) y unas 30 apuestas mínimas al día para quien va en quiebra. El sumidero solo compensa un bono diario si el usuario apuesta unas 2000 monedas al día, así que el tope por pelea es lo que acota a quien acumula.
2. **¿Quién escribe `season_stats`? Decidida por architect el 2026-11-02: TA-28.** El test 25 bloquea a las dos features, y ADR-0008 no cambia.
   - **TA-27** no crea `season_stats` ni temporadas. Lo adelanta de forma voluntaria y pasa el test 25 sobre un `RankedProfit` inmutable por apuesta.
   - **TA-28** vuelve a pasar el test 25 sobre el agregado, sin sumar dos veces al reliquidar, y hereda la parte de carencia del 27. Una pelea sin temporada no hace fallar el gancho (RF31). Si la primera temporada cubre peleas ya liquidadas, las suma desde la cifra guardada.
3. **¿Qué pasa con un pozo de un solo lado?** La decisión final queda para la spec.
   - **El vacío:** ADR-0008 D8 solo cubre `L = 0` sin reembolso, donde el ganador paga el rake sin ganar nada, y deja abierto el pozo reembolsado. Nada define el caso en que nadie apostó al ganador, y la casa no puede pagar ni debería quedarse esas monedas.
   - **Recomendación:** reembolsar todo pozo con un lado vacío, sin rake.
   - **Encaje con D8:** su párrafo "Por qué `min`" considera el pozo de un solo lado sin reembolso y advierte contra cobrar el rake solo del lado perdedor y obligar a reembolsar esos pozos. Esta recomendación no cobra así el rake, y D8 admite el "pozo reembolsado", pero la spec tiene que justificarlo.
   - **Test 25(e):** la liquidación ya no tendría un camino real para ese caso, así que se prueba sobre la fórmula aislada.
   - **Costo:** con pocos usuarios, muchas apuestas no tendrán riesgo real ni contarán para salir de la carencia (riesgo de arranque en frío de la visión). Para ver un pago en local hacen falta dos cuentas en lados opuestos.

## Revisión
### architect — 2026-11-02 — REQUEST_CHANGES
- Verifiqué:
  - **Viabilidad: sí.** `SettlementHook` y `NoBets` ya están en la transacción de RF18, RF19 y RF59. Las migraciones dejan previstas las ampliaciones de `ledger_entries_kind_ck`, `admin_audit_action_ck` y `admin_audit_actor_ck`. `bets` y `season_stats` todavía no existen, así que no hay datos que migrar. Todo corre en el proceso actual y no suma servicios ni costos.
  - **Criterios heredados: están todos.** Tests 7, 8, 18 (reserva), 23, 24 y 26; `ranked_stake`; test 25 y parte de carencia del 27; reembolso de `draw` y `voided` (una cancelada nunca tiene apuestas, RF33); RF31 con sus tres pruebas; RF59; `log_without_end`; binario anterior; los cinco criterios de la preocupación 13 y el gate de la enmienda. Comparé contra la lista que me pasó quien coordina y contra la spec 010, no contra el issue de Linear, que no leí.
  - **Números de la pregunta 1:** `R = 50`, tope de 500 y 100, 30 apuestas mínimas (`150 / 5`) y 2000 monedas de equilibrio (`100 / 0.05`) son correctos y respetan D8. game-designer los valida en la spec.
  - **Formato:** el frontmatter es válido, usa los términos del glosario y la autora no es architect. El largo es aceptable: los criterios son independientes y resumirlos los perdería.
- Cambios pedidos:
  1. **Restricción de ADR-0002 (línea 37).** Tal como está, la enmienda sería una excepción a "oculto hasta `Settled`", y no lo es: nada se revela antes de tiempo. Lo que exceptúa es la publicación de la semilla en `Settled` (punto 4). Redacción: "El resultado y la semilla siguen ocultos hasta `Settled`, sin excepciones. En `Settled` se publica la semilla, salvo en `voided(secret_lost)`, la enmienda de abajo".
  2. **Criterio de la línea 56 y pregunta 2.** Reemplazarlos por la decisión de abajo. Sin esto, el intent deja abierto quién carga con cada test.
  3. **Pregunta 3 y test 25(e).** Agregar que reembolsar todo pozo con un lado vacío deja 25(e) sin camino real por la liquidación, y que entonces se prueba sobre la fórmula aislada. Citar también el párrafo "Por qué `min`" de D8, que considera el pozo de un solo lado sin reembolso y advierte contra obligar a reembolsarlo junto con un rake solo del lado perdedor. La recomendación no cobra así el rake y D8 admite el "pozo reembolsado", así que no veo contradicción, pero la spec tiene que justificarlo. Si se adopta, la línea 59 suma ese reembolso: no cuenta como apuesta liquidada ni suma `RankedProfit`.
- **Decisión sobre `season_stats` (pregunta 2):**
  - **Quién la escribe: TA-28.** TA-27 no crea la tabla ni temporadas. Se confirma el punto medio, con una corrección: el test 25 bloquea a **las dos** features. Tal como lo pide la nota del 2026-10-28, sigue atado a TA-28 como primera en escribir `season_stats`, y TA-27 lo adelanta de forma voluntaria.
  - **TA-27** calcula al liquidar, en la transacción del gancho, el `RankedProfit` de cada apuesta y lo guarda de forma inmutable. Pasa el test 25 completo sobre esa cifra, (a) a (g).
  - **TA-28** suma esa cifra en `season_stats` en la misma transacción. Vuelve a pasar el test 25 sobre el agregado (reliquidar no suma dos veces) y la parte de carencia del 27. Además, una pelea sin temporada no hace fallar el gancho (RF31), y si la primera temporada cubre peleas ya liquidadas, las suma desde la cifra guardada.
  - **Sin cambios en ADR-0008:** el reparto cumple la nota, y `season_stats` sigue siendo la fuente del leaderboard. La carencia no necesita `season_stats`, porque se deriva de las apuestas. Un criterio de TA-28 no bloquea a TA-27, pero su issue de Linear tiene que recoger estos tres puntos antes de aprobar este intent.
- Arrastro a la spec, sin pedir cambios aquí:
  - Cómo se verifica "probar el binario anterior" y "reabrir la aceptación", que son pasos humanos.
  - Qué plazo rige el comando de `secret_lost` en `betting_closed`, donde no hay `live_started_at`.
  - A quién van los residuos del redondeo del pago y del rake con importes chicos (`m = 5`), sin que la casa arriesgue nada.
  - `lock_timeout` de RF29.
  - El ajuste de RF7, RF9 ("sin excepciones") y RF25, y del glosario ("Anulada" revela la semilla).
- **Gates del owner al volver a revisar:** `risk: high` pide su OK explícito en chat además de esta aprobación, y la enmienda de ADR-0002 tiene su propio gate.

### architect — 2026-11-02 — APPROVE (segunda ronda)
- Verifiqué, solo en lo afectado:
  - **Cambio 1:** la línea 37 lleva la redacción pedida.
  - **Cambio 2:** la línea 56, la pregunta 2 y "Fuera de alcance" recogen la decisión sobre `season_stats`, sin dejar abierto quién carga con cada test.
  - **Cambio 3:** la pregunta 3 trata 25(e) y el párrafo "Por qué `min`" de D8, y la línea 59 suma el reembolso de forma condicionada. La decisión final queda arrastrada a la spec.
  - **Linear:** quien coordina comunica que el reparto ya está como comentario en TA-28. No lo leí: lo tomo de ese mensaje.
- **Sigue pendiente, y esta aprobación no lo cubre:** el OK explícito del owner por `risk: high` y el gate de la enmienda de ADR-0002. Lo arrastrado a la spec en la primera ronda sigue vigente.
