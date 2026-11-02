---
id: "0002"
title: Semilla con commit-reveal
type: adr
status: approved
risk: high
author: owner
approvals:
  - by: owner
    date: 2026-10-20
created: 2026-10-20
updated: 2026-11-02
---

# ADR-0002: Semilla con commit-reveal

## Contexto
Con apuestas de por medio, el resultado no puede conocerse antes de que cierren las apuestas, y los usuarios deben poder comprobar que no hubo trampa.

## Decisión
1. Al abrir las apuestas, el servidor genera `seed` (32 bytes) con un CSPRNG del sistema operativo (`OsRng`/`getrandom`) y publica `commit = SHA-256(seed)`.
2. La semilla vive solo en el servidor, nunca en logs, respuestas del API ni mensajes WS, hasta `Settled`.
3. Al cerrar las apuestas se calcula el `FightLog`. Los eventos se emiten **solo cuando les toca** según el tiempo de la pelea; nunca se envía el log completo ni eventos futuros.
4. En `Settled` se publica la semilla. Cualquiera puede verificar `SHA-256(seed) == commit` y re-simular con `engine-wasm`.

## Alternativas descartadas
- Mezclar entropía aportada por los usuarios: es más complejo y no aporta mucho al inicio. Se puede revisar después.
- Enviar el log completo al cliente al empezar: cualquiera vería el final en DevTools.

## Consecuencias
- Hay tests que garantizan que ningún endpoint ni mensaje expone `seed` antes de `Settled`.
- La semilla se guarda en la base de datos (necesaria si el servidor se reinicia a mitad de una pelea), en una columna que el API nunca serializa mientras la pelea no esté liquidada.

## Enmienda propuesta 2026-11-02: pelea anulada con el secreto perdido
**Estado: `proposed`. No está en vigor.** La decisión de arriba rige sin cambios hasta que esta enmienda pase por el gate de ADRs de [`docs/process.md`](../process.md): revisión de security-reviewer y OK explícito del owner. Viene de la preocupación 13 de la [spec de 010](../features/010-scheduler-de-peleas/spec.md) y la implementa la fase A de la [spec de 012](../features/012-apuestas/spec.md).

**Qué cambia.** Solo el punto 4, al que se le suma esta frase:

> Salvo una pelea anulada porque el secreto se perdió (`voided(secret_lost)`), en la que no hay semilla que publicar y la pelea queda sin verificar.

Los puntos 1, 2 y 3 no cambian: el resultado y la semilla siguen ocultos hasta `Settled`, sin excepciones.

**Por qué.** Si la fila de `fight_secrets` desaparece y no se puede restaurar desde un backup, la pelea queda detenida para siempre: no hay semilla que revelar, y `voided` exige revelarla. Mientras dure, no hay peleas nuevas y los stakes de esa pelea quedan bloqueados. La otra salida es SQL manual con los triggers apagados, que rompe las transiciones de la pelea y escribe reembolsos fuera del módulo del ledger.

**Riesgo que acepta el owner.** Quien tiene la credencial de la base o el operador puede usar esta salida a propósito:
1. Ve el `end` de la pelea, o calcula el resultado leyendo el secreto.
2. Detiene el server antes de `settle_at`, para que nada liquide.
3. Apaga los triggers, borra el secreto y los vuelve a encender.
4. Arranca el server y espera el plazo, 215 s con los valores por defecto.
5. Anula la pelea: todos reciben su reembolso.

Sirve para no perder una apuesta propia y **también para anular las victorias de otros.** Una anulación con semilla se puede refutar simulándola. **Esta no se puede refutar nunca**, y además saca una pérdida del ranking. Es peor que las otras formas de abuso de la preocupación 1 de 010, aunque el daño posible sigue siendo sobre monedas ficticias y el ranking (ADR-0005). **El contador público y `admin_audit` detectan, no previenen:** el plazo y las condiciones de abajo frenan errores, no a quien tiene la credencial, y controlar el server o su contenedor ya da esa credencial, porque contiene `DATABASE_URL`. Esa credencial también podría alterar `admin_audit`.

**Condiciones de la enmienda** (criterios de security-reviewer en la revisión del PR #69). Si una deja de cumplirse, la enmienda deja de valer:
1. `secret_lost` solo procede si la fila del secreto no existe. El guard de la base exige `NOT EXISTS` en `fight_secrets`.
2. El comando se niega mientras pueda haber emisión en curso, hasta `live_started_at + max_duration_ms + settle_delay` más un margen. Sin `live_started_at`, cuenta desde `live_at`.
3. Hay un contador público de estas anulaciones, y cada caso reabre la aceptación de la preocupación 1 de 010.
4. "No verificable" es un estado propio en la verificación pública y en la vista. Una semilla nula nunca pasa como una comprobación omitida, y una `voided` de otra razón sin semilla sigue fallando.
5. Solo se llega tras las negativas. No adelanta la de versión ausente, y un secreto que no coincide con el commit nunca es `secret_lost`.

**Alternativas.** Tratarlo como una lectura del ADR, sin cambiar el texto: oculta la excepción. No agregar este final: deja el SQL manual como única salida.
