---
id: "006"
title: Ritmo rápido del motor
type: intent
status: done
risk: high
author: game-designer
reviewers: [architect]
approvals: [{by: architect, date: 2026-10-27}]
linear: TA-21
depends_on: ["002", "003", "005"]
created: 2026-10-27
updated: 2026-10-28
---

# Intent: ritmo rápido del motor

## Problema
En el go/no-go del spike 002 (TA-20; spec de 002, `## Revisión`, "owner — 2026-10-27 — Go con cambios") el owner eligió un ritmo más rápido: **~1.2 s de separación media entre golpes**. Llegó a esa cifra con `?pace=0.8` sobre los 1468 ms del guion escrito a mano. Además pidió que el ritmo siga siempre parametrizado para poder ajustarlo otra vez. "Datos para 003" ya recoge el cambio. Lo demás sigue igual: ráfagas a ≤ 900 ms, pausas de ≥ 2.5 s, ≥ 600 ms entre impactos y 45 a 90 s en total.

Ese ajuste no se puede reproducir con peleas reales. `?log=ko-comeback&pace=0.8` en `/[lang]/dev/arena` termina en `timeline_invalid`: al reescalar por 0.8, los impactos del motor, que nunca están a menos de 700 ms, quedan a 560 ms, y eso rompe la regla de la `Timeline` de ≥ 600 ms entre impactos consecutivos.

Las medidas sobre el ruleset v1 (`just sim`, 10 000 peleas, y los 7 dorados del motor) muestran que el problema no es la velocidad:
- El motor **ya** va a ~1.1 s de media: 1096 ms en el simulador y de 931 a 1136 ms en los dorados. El 0.8 corregía el guion de 002, no el motor. Aplicado a un log real, da ~0.85 s de media, más rápido de lo que pidió el owner.
- Las ráfagas sí existen: entre el 30 y el 50 % de los intervalos de cada dorado son de ≤ 900 ms.
- **Las pausas casi no existen.** Solo hay un intervalo de ≥ 2.5 s en los 7 dorados, y viene de un knockdown. El motor no tiene ningún mecanismo de pausa. Sin pausas, la pelea suena a metrónomo: el owner respondió "sí (creo)" a "¿funcionan los dos ritmos?" mirando un guion que sí las tenía, y las peleas reales no lo tienen.

Hoy el único "parámetro de ritmo" a mano es `?pace`, una herramienta de la ruta de prueba que reescala a ciegas y rompe con logs reales. El ritmo de verdad está en el ruleset, pero repartido entre intervalos, windups, recuperación y jitter. Nadie lo puede ajustar pensando en "media, ráfaga y pausa".

## Resultado propuesto
- **Peleas con dos ritmos.** Una pelea del motor alterna ráfagas (intercambios a ≤ 900 ms) con pausas (≥ 2.5 s sin impactos, en las que los luchadores se estudian), con una separación media de ~1.2 s. Dura de 45 a 90 s en total y nunca deja dos impactos consecutivos a menos de 600 ms. El espectador nota la diferencia entre "se están dando con todo" y "se miden", y el narrador tiene un hueco para hablar (ADR-0010).
- **Ritmo ajustable desde el ruleset.** Las cifras de ritmo son parámetros del ruleset con nombres que se entienden. Para volver a ajustar el ritmo se cambian valores, se regeneran los dorados y se corre el simulador, sin tocar fórmulas.
- **El simulador mide el ritmo.** `just sim` informa la separación media, el % de intervalos en ráfaga y las pausas por pelea, y `--check` falla si el ritmo sale de la banda, igual que hoy con el win rate y los momentos.
- **`?pace` sirve con logs reales.** En la arena de prueba, `pace` solo admite factores que respetan los ≥ 600 ms con el log cargado. Ninguno de los dorados termina en `timeline_invalid` por usar `pace`.

## Usuarios y sistemas afectados
- Usuarios: el espectador y el apostador cuando haya peleas publicadas (la pelea se siente con ritmo); hoy, el owner y los agentes en `/[lang]/dev/arena`.
- Sistemas: `crates/engine` (ruleset y simulación), sus dorados, `crates/sim` (métricas y criterios de ritmo), `apps/web` (la arena de prueba y su `?pace`). Posiblemente `crates/engine-wasm`, si cambian los dorados que consume.
- No debería tocar: `crates/protocol`, `packages/protocol-ts`, el formato de la `Timeline` ni el validador del renderer. Si la spec concluye que una pausa necesita un evento nuevo, eso cambia el alcance y se registra.

## Restricciones
- **ADR-0001:** determinismo intacto. Toda variación de ritmo sale del PRNG con semilla, misma semilla → mismos bytes. Ni reloj ni aleatoriedad fuera del motor.
- **Reglas de la `Timeline` (002):** ≥ 600 ms entre impactos consecutivos, intro de ≥ 2 s, ≥ 1.5 s de KO a victoria y ≥ 2 s de pose; la v2 de 005 para los fines por tiempo. El motor se adapta a esas reglas. No se relajan para que quepa el motor.
- **Balance (003 RF17):** win rate de cada personaje entre 40 y 60 % en 10 000 peleas, momentos en ≥ 15 % de las peleas y cada `kind` en ≥ 3 %, como mucho un 10 % de `time_up` y menos de un 1 % de empates. Más pausas alargan las peleas, y eso empuja al `time_up` y cambia qué personajes rinden.
- **Roster actual:** el contenido de `content/characters/` no debería necesitar retoques. Si hace falta alguno, entra en el mismo PR con su informe de balance.
- **ADR-0004 y ADR-0007:** nada nuevo de franquicias, y nada de texto visible fuera de Paraglide (la arena sigue usando solo códigos).
- **Repo público:** el ritmo se justifica como experiencia del espectador, nada más.

## Criterios de éxito
- [ ] Con `just sim` (10 000 peleas, semilla por defecto), la separación media entre impactos queda en ~1.2 s (la spec fija la banda; propuesta inicial: 1100–1300 ms), con ráfagas a ≤ 900 ms y al menos una pausa de ≥ 2.5 s en la gran mayoría de las peleas (la spec fija la cifra; propuesta: ≥ 2 pausas en la pelea mediana).
- [ ] Ninguna pelea simulada tiene dos impactos consecutivos a menos de 600 ms. Lo comprueba un test de propiedad sobre muchas semillas, además del test de cotas del ruleset.
- [ ] El ≥ 90 % de las peleas dura de 45 a 90 s en total, con un `time_up` de ≤ 10 %.
- [ ] `just sim --check` sigue pasando: win rates de 40 a 60 %, momentos en ≥ 15 % y cada `kind` en ≥ 3 %. Si la spec lo decide, también pasan los criterios de ritmo nuevos.
- [ ] Los dorados del motor se regeneran en el mismo PR, y el adaptador y `validateTimeline` los aceptan todos.
- [ ] Toda cifra de ritmo de los criterios anteriores es un parámetro del ruleset. Cambiar el ritmo no exige tocar fórmulas.
- [ ] En `/[lang]/dev/arena`, cada dorado se reproduce hasta `finished` con `pace=1` y con el factor mínimo que acepte la arena, en Chromium y en WebKit. Un `pace` fuera del rango válido para ese log nunca termina en `timeline_invalid`.
- [ ] El owner mira un dorado nuevo en la arena y confirma que se notan los dos ritmos (se registra en la spec).

## Fuera de alcance
- Animaciones nuevas, el cue `taunt` y cualquier coreografía de la pausa más allá de `idle` (o `walk` con su fallback). Es trabajo de 004 y de futuras `timeline_version`.
- Cambios de daño, críticos o momentos que no sean consecuencia del ritmo.
- `pace` en una ruta pública o como ajuste del espectador: sigue siendo una herramienta de `/dev/**`.
- El narrador y sus frases para las pausas (ADR-0010). Esta feature solo deja el hueco.
- La sincronización en vivo y el buffer de look-ahead del server.

## Preguntas abiertas
- **¿Mecanismo de pausa en el motor, o basta con ampliar el jitter?** Mi recomendación como game-designer: **un mecanismo de pausa explícito** ("respiro" o estudio: los dos luchadores dejan de atacar un tiempo sorteado con el PRNG, por ejemplo tras una ráfaga o tras un golpe fuerte). Ampliar el jitter reparte el ruido de forma pareja: sube la media, convierte los intervalos cortos en medianos (1.5–2 s) y casi nunca da pausas de ≥ 2.5 s sin pasarse de los 90 s. El ritmo dramático necesita una distribución con dos picos, ráfaga y pausa, y eso no sale de un solo jitter uniforme. Además, un respiro con regla propia se puede balancear (cuándo ocurre, cuánto dura) y le da al narrador un momento reconocible. Mejor si la pausa es solo ausencia de impactos y no un evento nuevo del log; la spec lo confirma.
- **¿Bump de `ruleset_version` o reescribir la v1?** Recomiendo **reescribir la v1**. `ruleset.rs` lo permite antes de la primera pelea de producción si los dorados se regeneran en el mismo PR, y no hay ninguna pelea en producción. Una v2 obligaría a mantener para siempre una v1 que ninguna pelea real usó. El architect lo confirma contra ADR-0001.
- **¿Cómo se acota `pace`?** Recomiendo un **mínimo dinámico por log**: el factor más bajo que deja todos los intervalos entre impactos en ≥ 600 ms (600 / intervalo mínimo del log, redondeado hacia arriba), con piso y techo fijos como red de seguridad. Un valor fuera de rango se lleva al límite válido más cercano, y el factor efectivo se informa en "copiar estadísticas", en vez de terminar en error. Un mínimo fijo vale para un solo log y vuelve a romperse en cuanto cambie el ruleset. La spec decide entre llevar al límite y rechazar con código.
- **¿Las pausas cuentan en la separación media?** La "~1.2 s" del owner salió de un guion con pausas incluidas. Propongo medir la media sobre todos los intervalos, pausas incluidas, como hacen hoy el simulador y `mean_strike_gap_ms`, para que la cifra sea comparable.
- **¿Qué intervalo cuenta para las reglas?** Desde el knockdown, el impacto siguiente puede quedar lejos. La spec define si un knockdown cuenta como pausa o se mide aparte.

## Revisión
### architect — 2026-10-27 — APPROVE
- Verifiqué: viabilidad con el stack (solo motor, simulador y arena de `/dev/**`; sin server, protocolo ni infraestructura, coste nulo en el VPS). Contra `crates/engine/src/sim.rs`: las acciones son secuenciales (`t_res = t + windup`, `def.next_at >= t_res + recovery_ms`), así que el mínimo entre impactos es estructural, `recovery_ms + windup_strike_ms` = 700 ms, y una pausa explícita sorteada con el PRNG se puede añadir sin romperlo ni tocar el determinismo (ADR-0001). La semilla y el commit-reveal (ADR-0002) no cambian. Reescribir la v1 es compatible con ADR-0001 y con el comentario de `ruleset.rs`: `apps/server` no usa el motor ni guarda peleas, no hay ninguna en producción. Hay holgura de duración: p90 de 63.5 s contra el límite de 86 s y `time_up` en 0 %. `retime.ts` tiene `PACE_MIN` fijo en 0.8, que es la causa del `timeline_invalid` descrito. El alcance deja fuera `crates/protocol` y el formato de la `Timeline`. El frontmatter es válido. No contradice ningún ADR `approved`.
- Arrastrado a la spec (no bloquea):
  - Criterio "toda cifra de ritmo es un parámetro del ruleset": lo interpreto como *entradas* en el ruleset (probabilidad, disparador y duración de la pausa, intervalos, jitter). Las *bandas de salida* (media de 1100 a 1300 ms, pausas por pelea, % de ráfaga) son criterios de `crates/sim`. Las reglas de la `Timeline` (≥ 600 ms, intro, KO, pose) siguen siendo de 002/005 y del validador del renderer, no del ruleset. La spec nombra una sola fuente para cada cifra y cómo el test de cotas del motor se ata al 600 sin duplicarlo en silencio.
  - "Con ráfagas a ≤ 900 ms" no tiene cifra. La spec fija qué % mínimo de intervalos tiene que estar en ráfaga.
  - Los sorteos de la pausa cambian el orden del PRNG y, con él, todos los dorados y las cifras de balance. La spec fija dónde se consumen esos sorteos y vuelve a medir RF17 entero, no solo el ritmo.
  - La pausa no es un evento nuevo del log (lo confirmo como architect), salvo que la spec demuestre lo contrario y registre el cambio de alcance.
  - Knockdown: la spec define si cuenta como pausa. El `pace` dinámico se calcula sobre el log ya adaptado (strikes de la `Timeline`) y se decide entre llevar al límite y rechazar con código.
