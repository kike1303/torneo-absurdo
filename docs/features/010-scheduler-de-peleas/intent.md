---
id: "010"
title: Scheduler de peleas en vivo
type: intent
status: approved
risk: high
author: product-owner
reviewers: [architect]
approvals: [{by: architect, date: 2026-10-30}]
linear: TA-13
depends_on: ["002", "003", "005", "008"]
created: 2026-10-30
updated: 2026-10-30
---

# Intent: Scheduler de peleas en vivo

## Problema
El motor, el ledger, la auth y el `/ws` existen, pero nada programa peleas: ninguna corre en vivo y nadie tiene algo que ver. Sin esto no hay pilar "Ver juntos", y las apuestas (TA-27) no tienen sobre qué montarse. Una web "en vivo" sin peleas no da ninguna razón para volver mañana.

Además, el commit de ADR-0002 cubre solo la semilla. Con la semilla en la base y un motor barato (10 000 peleas en menos de 10 s), quien tenga acceso podría probar ajustes a un personaje después del commit hasta obtener el resultado que quiere, y la verificación de la semilla seguiría pasando (RF22 y preocupación 2 de la spec de 003). La integridad de la pelea tiene que quedar resuelta antes de que exista la primera apuesta.

## Resultado propuesto
1. **Siempre hay algo pasando.** El server programa peleas rápidas solo, cada cierto tiempo (la visión dice 10 a 15 min), con su ventana de apuestas anunciada, aunque nadie esté conectado.
2. **Todos ven la misma pelea.** Cada espectador, con sesión o anónimo, ve lo mismo y al mismo tiempo. Quien llega tarde, o pierde la red, ve lo que ya pasó y sigue en vivo.
3. **Un reinicio no cambia nada.** Si el server se reinicia, ninguna pelea se pierde ni cambia de semilla, personajes, reglas o resultado.
4. **La pelea queda fijada y es comprobable.** Ninguna pelea abre apuestas si los personajes exactos y las reglas no quedan fijados y públicos desde ese momento. Después de `Settled`, cualquiera puede comprobar que la pelea fue la anunciada, aunque el contenido haya cambiado desde entonces.
5. **El final no se conoce antes de tiempo.** Antes de `Live` no sale ningún evento ni dato que delate el resultado o la duración. Durante `Live`, ningún evento sale antes de su hora, menos el margen de reproducción que fije la spec; el KO y `end` salen a su hora, y eso es lo esperado. El `FightLog` completo como objeto y la semilla no salen antes de `Settled`.
6. **Un ciclo en el que las apuestas puedan confiar.** Esta feature fija lo que TA-27 puede asumir, sin diseñar apuestas:
   - cada pelea anuncia el cierre de su ventana, y ese cierre queda persistido y lo hace cumplir el server;
   - cada pelea termina una sola vez y su final no cambia: gana un lado, empate o cancelada;
   - un empate devuelve lo apostado, con asientos del ledger (ADR-0005). Una pelea cancelada también (ADR-0008 D8). Los asientos los implementa TA-27, porque antes no existen apuestas que devolver.
7. **Una vista web mínima de la pelea en vivo.** Sin ella, el resultado no es observable para un usuario, y ninguna otra issue la cubre. Muestra la pelea actual o la próxima (con su cuenta atrás), la pelea en vivo y el resultado, para que entrar entre dos peleas no se sienta vacío. La spec decide si es la última fase.

## Usuarios y sistemas afectados
- Usuarios: el espectador, anónimo o con sesión, casi siempre en el celular; el apostador, desde TA-27; quien quiera comprobar una pelea; el owner, que opera el server.
- Sistemas: `apps/server` (programación, ciclo y persistencia), `crates/protocol` (con `packages/protocol-ts` regenerado), `apps/web` y `packages/renderer` (la vista mínima, porque hoy el renderer exige una `Timeline` completa), `crates/content` como fuente de personajes y `apps/server/Dockerfile` (la imagen no incluye `content/characters/`, y tiene que funcionar al menos en `just prod-local`).
- No debería cambiar: `crates/engine`. El ledger no recibe tipos de asiento ni movimientos en esta feature.

## Restricciones
- **ADR-0002:** semilla con CSPRNG y solo en el server hasta `Settled`; cada evento sale a su hora y el log completo no sale antes de `Settled`. Si la spec cambia lo que cubre el commit, es una enmienda del ADR y del glosario, con el OK del owner.
- **ADR-0005:** nada de dinero real. Las monedas solo se mueven con asientos del ledger.
- **ADR-0007:** el server y el protocolo emiten solo códigos. Todo texto de la vista existe en en y es.
- **ADR-0008:** las peleas y la programación van por canales públicos de `/ws`, iguales para todos y bajo ADR-0002 sin importar la auth. El cliente solo se suscribe. Ningún rol lee la semilla antes de `Settled`.
- **ADR-0012 A4:** semillas y contenido del `FightLog` no se registran en logs **nunca**, tampoco después de `Settled`. Los campos nuevos siguen la allowlist.
- **ADR-0004:** la vista no usa assets de franquicias. Hoy ningún personaje tiene un set de sprites propio, así que se ve con los placeholders del renderer.
- **Recursos y local:** VPS de 1 vCPU y 4 GB, compartido, sin servicios externos. Todo funciona en local con `just dev`, y `just check` y `just e2e` pasan sin red.
- **Mobile-first y pocos usuarios:** cómoda a 375 px y con sentido con un solo espectador.
- **Repo público:** sin secretos ni datos personales en docs y tests.

## Criterios de éxito
- [ ] **Sola:** con el server arrancado y nadie conectado, varias peleas seguidas completan el ciclo `Scheduled` → `BettingOpen` → `BettingClosed` → `Live` → `Settled`, a la cadencia de la spec y sin intervención.
- [ ] **Sincronizada:** un espectador anónimo y uno con sesión reciben los mismos eventos, en el mismo orden y cuando les toca, dentro de la tolerancia que fije la spec.
- [ ] **Llegar tarde:** quien entra o se reconecta a mitad de pelea recibe los eventos ya ocurridos y sigue en vivo. Ningún evento le llega antes de su hora, menos el margen que fije la spec.
- [ ] **Reinicio:** al reiniciar el server en cada estado del ciclo, la pelea conserva su semilla, personajes, reglas y `FightLog` byte a byte, y termina una sola vez.
- [ ] **Personajes fijados:** si un personaje cambia en `content/` después de abrir las apuestas, la pelea corre con lo que se fijó y sigue siendo re-simulable. Si no se pueden fijar y publicar, la pelea no abre apuestas.
- [ ] **Comprobable:** con solo los datos públicos, una prueba verifica el commit y reproduce con el motor el mismo `FightLog` byte a byte, también después de cambiar el contenido.
- [ ] **Sin fugas (ADR-0008, test 12):** ningún endpoint ni mensaje WS expone la semilla ni el `FightLog` completo antes de `Settled`, sin sesión, con sesión ni con sesión de admin. La misma prueba comprueba que antes de `Live` no sale ningún evento ni dato que delate el resultado o la duración, y que durante `Live` ningún evento sale antes de su hora, menos el margen de la spec. El KO y `end` a su hora no cuentan como fuga.
- [ ] **Logs:** en una corrida de pelea completa, los canarios confirman que ningún log contiene la semilla ni contenido del `FightLog` (ADR-0012 A4).
- [ ] **Final único e inmutable:** una prueba intenta reescribir el final y pone a dos procesos a competir por cerrarlo, y queda un solo final, sin cambios. Empate y cancelación se distinguen de una victoria en los datos públicos.
- [ ] **Cierre consultable:** el cierre de la ventana queda en un estado persistido que otra transacción puede consultar de forma serializada. Una prueba, sin apuestas, demuestra que ninguna transacción lo ve abierto una vez calculado el `FightLog`.
- [ ] **Reembolso delegado:** antes de cerrar 010, la issue TA-27 lleva como criterio bloqueante el reembolso de empate y cancelación con asientos del ledger, y TA-13 dice que lo delega allí. Esta feature no crea asientos.
- [ ] **Imagen:** en `just prod-local`, el server programa y corre peleas con los personajes disponibles en la imagen.
- [ ] **Vista:** el e2e cubre la próxima pelea, la pelea en vivo, entrar tarde y el resultado (victoria y empate), en en y es, a 375 px, en Chromium y WebKit y sin sesión.
- [ ] **Prerrequisito 5** de `docs/architecture.md` queda cerrado. `check-i18n`, `just check` y `just e2e` pasan.

## Fuera de alcance
- **Apuestas (TA-27):** pozos, cuotas, tope por pelea, liquidación y sus reembolsos, rescate y cambios al bono diario.
- Main event y torneos.
- Rating de personajes y leaderboards (TA-28), y cualquier consecuencia de la pelea en ellos.
- Narrador.
- UI de repeticiones y pantalla de verificación en el navegador. Los datos para verificar sí se publican.
- Arte definitivo de los personajes y contenido nuevo.
- Deploy (TA-8): todo funciona en local por ahora.

## Preguntas abiertas
Las resuelve la spec antes de aprobarse, salvo donde se indica.

1. **Cadencia y ventana:** ¿cada cuánto hay una pelea dentro de los 10 a 15 min, y cuánto dura la ventana de apuestas? ¿Cuánto espera un espectador sin nada que ver?
2. **Cruces:** ¿cómo se eligen, con 4 personajes hoy (6 cruces) y 6 a 8 en el MVP? ¿Hace falta al game-designer? ¿Cuánto se anticipa el cruce antes de abrir apuestas?
3. **Mecanismo de fijación (RF22):** snapshot publicado o commit extendido. El commit extendido solo liga los personajes con un hash, así que no basta por sí solo para que sean públicos desde la apertura. Si cambia el commit, requiere enmendar ADR-0002 y el glosario con el OK del owner.
4. **Si una pelea no puede correr:** ¿qué la cancela (contenido inválido, caída del server más larga que la pelea)? ¿Qué se hace con una pelea cuyo horario pasó con el server caído? ¿Una pelea se puede cancelar después de calcular su `FightLog`? Si se puede, quien conoce el resultado podría anular las que no le convienen. ¿Se publica la semilla de una pelea cancelada?
5. **Enganche con TA-27, también con una cancelación con apuestas abiertas:** ADR-0008 D8 ya dice que una pelea cancelada se liquida como reembolso. La spec tiene que definir, sin que TA-27 rediseñe el ciclo:
   - dónde entra la liquidación idempotente (ADR-0005) y su orden con el reveal en `Settled`;
   - qué pasa si el server cae entre el final y la liquidación;
   - cómo se sabe qué pelea queda pendiente (`farm-report` falla si queda alguna sin liquidar ni reembolsar);
   - qué hora de cierre usa la temporada de la pelea (ADR-0008 D8);
   - qué garantiza el ciclo al cancelar, por ejemplo que no se acepten más apuestas, y cómo se entera TA-27.
6. **Qué se anuncia sin delatar el final:** si el horario de la siguiente pelea depende de lo que dure la actual, anunciarlo antes de tiempo revela la duración de esta. ¿Cómo se programa y qué se publica?
7. **Riesgo residual del operador:** la fijación no impide que quien tiene acceso a la base lea la semilla durante la ventana, ni que repita el sorteo antes del commit hasta obtener un resultado. La spec dice qué se mitiga y qué se deja escrito como riesgo, para que lo acepte el owner.
8. **Quien queda fuera por `ws.capacity`:** ¿qué ve? Una revisión de ADR-0008 pidió una salida con un mensaje claro y, si cabe, la pelea con retraso, sin mostrar nunca eventos futuros.
9. **Reproducir una pelea de la que solo se conoce el pasado:** hoy el renderer exige una `Timeline` completa, con cierre y duración, y ADR-0002 no deja que el cliente los tenga al empezar. ¿Cómo se reproduce en vivo, incluido entrar tarde? La respuesta define el costo real de la vista mínima, y la spec puede partirla en fases.
10. **Margen de reproducción:** ¿cuánto se adelanta un evento a su hora? La preocupación 2 de la spec de 002 pide un buffer de al menos 400 ms más la latencia, porque el windup necesita conocer el impacto por adelantado.

## Revisión
### architect — 2026-10-30 — REQUEST_CHANGES
- Verifiqué:
  - **Viabilidad en general: sí.** El motor corre una vez por pelea (RNF de 003), `Channel` es un enum cerrado que espera los canales públicos ("arrive with TA-13"), y `jobs.rs` da la forma de una tarea de fondo con parada. El `edge.mjs` del e2e ya hace upgrade de WS, y `connect-src 'self'` cubre el WS del mismo origen (ADR-0012 B4.4). El server solo depende hoy de `protocol`: sumar `engine` y `content` ya está previsto en `docs/architecture.md`.
  - **ADRs.** No contradice ADR-0002 (semilla, commit y enmienda condicionada), ADR-0005, ADR-0008 (D6, D8, D9 y test 12) ni ADR-0012 (A4.8, A4.9 y canarios), salvo por la redacción del cambio 2. Exigir personajes "públicos" es más estricto que RF22, no lo contradice.
  - **No impone el cómo:** el mecanismo de fijación, la cadencia, la persistencia y la fase de la vista quedan para la spec.
- Cambios pedidos:
  1. **`packages/renderer` falta en "Sistemas", y la vista no sale solo con `apps/web`.** El renderer solo recibe una `Timeline` completa: `load` la valida una vez, `duration_ms` es obligatorio, `validateTimeline` rechaza una sin cierre (`timeline.ending_missing`), `prepareScene` resuelve todo por carga y no hay forma de agregar cues. `timelineFromFightLog` también pide el log completo. Eso choca con ADR-0002: el cliente no puede tener el final ni la duración al empezar. *Cambio:* agregar `packages/renderer` a "Sistemas" y una pregunta abierta sobre cómo se reproduce una pelea de la que solo se conoce el pasado, incluido entrar tarde, sin decidir la solución. Con eso, "vista mínima" tiene un costo honesto y la spec puede partirla en fases. Agregar también `depends_on: "002"` y `"005"`. De paso, sumar `apps/server/Dockerfile` a "Sistemas": la imagen final solo lleva el binario y `content/characters/` no entra, pero `load_roster` lee del disco. Eso debe funcionar al menos en `just prod-local`.
  2. **El criterio "Sin fugas" y el punto 5 no se pueden cumplir como están escritos.** Dicen que el resultado no sale antes de `Settled`, pero durante `Live` el `Hit` del KO y el evento `end` lo revelan, y ADR-0002 punto 3 manda emitirlos a su tiempo: solo la semilla espera a `Settled`. Además, "nunca uno futuro" choca con la preocupación 2 de la spec de 002, que pide a esta feature un buffer ≥ 400 ms más la latencia, porque el windup necesita conocer el impacto por adelantado. *Cambio:* redactar la regla como: "antes de `Live` no sale ningún evento ni dato que delate resultado o duración; durante `Live`, ningún evento sale antes de su hora, menos el margen que fije la spec; el log completo como objeto no sale antes de `Settled`". Agregar el margen a las preguntas abiertas.
  3. **Frontera con TA-27: el reparto es correcto, pero no está bien amarrado.** Dejar los asientos a TA-27 es lo correcto: antes de TA-27 no existen stakes, así que ningún empate deja monedas sin devolver; `ledger_entries_kind_ck` solo acepta `grant`; 008 ya reserva `refund` y `fight_id` para esa migración, y ADR-0008 D8 dice que lo implementa la spec de apuestas. Un `refund` en 010 sería código muerto. Pero el requisito de TA-13 no se cumple solo con 010, y faltan tres cosas:
     - **(a) Reasignación explícita.** "Su intent cita esta regla" no se puede verificar al cerrar 010, porque ese intent no existe. El test 25(f) de ADR-0008, el del empate, tampoco lo asegura: la nota de reparto lo ata a la primera feature que escriba `season_stats`, que puede ser TA-28, y no está en la fila de TA-27. *Cambio:* antes de cerrar 010, el reembolso de empate y cancelación tiene que quedar como criterio bloqueante en la issue TA-27, y TA-13 tiene que decir que lo delega allí. El criterio del intent verifica esas dos cosas.
     - **(b) Garantías que 010 prueba, no solo escribe.** Hoy "Final para las apuestas" solo pide que "la spec deje escrito". *Cambio:* criterios verificables sin apuestas: el final es único e inmutable aunque se intente reescribir o haya dos procesos compitiendo, y el cierre de la ventana queda en un estado persistido que otra transacción puede consultar de forma serializada. Una apuesta aceptada después de calcular el `FightLog` rompe ADR-0002, y TA-27 no puede arreglarlo solo por su cuenta.
     - **(c) Punto de enganche de la liquidación.** ADR-0005 exige una transacción idempotente, y la pregunta 5 lo trata como un detalle de orden. *Cambio:* que la pregunta 5 exija que la spec defina dónde entra la liquidación sin rediseñar el ciclo, incluida una caída entre el final y la liquidación, y que el ciclo sepa qué pelea queda pendiente (`farm-report`, ADR-0008 D8, falla si queda alguna sin liquidar ni reembolsar). También el dato de la hora de cierre, del que sale la temporada de la pelea (D8).
- Para la spec, sin bloquear (los llevo como "Áreas de preocupación"):
  - El e2e de empate y victoria no puede depender de una semilla al azar, y una vía para inyectarla no puede existir en release (como `dev-auth`).
  - "Cancelada" no está en el ciclo de `docs/architecture.md` ni en el glosario; tampoco los estados del ciclo.
  - Falta la política para clientes lentos al difundir a 2000 sockets en 1 vCPU y 256 MB.
- **Gate del owner:** es `risk: high`, así que además de esta aprobación hace falta su OK explícito en chat (`docs/process.md`).

### product-owner — 2026-10-30 — correcciones de la ronda 1
- Apliqué los tres cambios: (1) `packages/renderer` y `apps/server/Dockerfile` en "Sistemas", criterio "Imagen", `depends_on` con "002" y "005" y pregunta 9; (2) punto 5, "Llegar tarde" y "Sin fugas" reescritos, restricción de ADR-0002 alineada y pregunta 10 sobre el margen; (3) criterios "Final único e inmutable", "Cierre consultable" y "Reembolso delegado" (apunta a la issue TA-27), y pregunta 5 ampliada.

### architect — 2026-10-30 — APPROVE
- Verifiqué, solo sobre lo que cambió tras la ronda 1:
  - **Cambio 1:** `packages/renderer` y `apps/server/Dockerfile` están en "Sistemas", con el criterio "Imagen" y la pregunta 9, que no decide la solución. `depends_on` suma "002" y "005", y ambas features existen. El Dockerfile solo copia `content/names`, así que el problema de la imagen es real.
  - **Cambio 2:** el punto 5, "Llegar tarde" y "Sin fugas" usan la regla pedida. El KO y `end` a su hora no cuentan como fuga, y el margen queda en la pregunta 10. Ya no chocan con ADR-0002 punto 3 ni con la preocupación 2 de la spec de 002.
  - **Cambio 3:** (a) "Reembolso delegado" se puede verificar al cerrar 010. (b) "Final único e inmutable" y "Cierre consultable" se prueban sin apuestas. (c) La pregunta 5 cubre el enganche idempotente, la caída entre el final y la liquidación, las peleas pendientes (`farm-report`), la hora de cierre y la cancelación con apuestas abiertas, y coincide con ADR-0008 D8.
  - **Prerrequisito 5:** `docs/architecture.md` lo asigna a TA-13, como dice el criterio.
- No verifiqué las issues de Linear: no tengo acceso desde esta revisión. Me consta por el aviso de quien coordina que TA-27 y TA-13 ya están actualizadas. El criterio "Reembolso delegado" sigue abierto hasta comprobarlas al cerrar 010.
- **Gate del owner:** retiro lo que pedí en la ronda 1. El owner autorizó el 2026-10-29 no esperar su OK en artefactos `risk: high` mientras no haya nada desplegado, aunque `docs/process.md` aún no lo refleja.
- Para la spec, sin bloquear: la restricción de ADR-0002 dice "cada evento sale a su hora" sin el margen; la regla completa es la del punto 5. Sigo llevando las tres preocupaciones de la ronda 1.
