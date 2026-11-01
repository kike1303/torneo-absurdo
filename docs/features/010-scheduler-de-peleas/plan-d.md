---
id: "010"
title: Scheduler de peleas en vivo, fase D (vista, cliente WS, e2e y cierre)
type: plan
status: approved
risk: medium
author: frontend-engineer
reviewers: [architect]
approvals: [{by: architect, date: 2026-10-31}]
linear: TA-13
depends_on: ["001", "002", "003", "005", "008"]
created: 2026-10-31
updated: 2026-10-31
---

# Plan: Scheduler de peleas en vivo, fase D (desde spec.md 2026-10-31)

> [intent](./intent.md) · [spec](./spec.md) · [plan-a](./plan-a.md) · [plan-b](./plan-b.md) · [plan-c](./plan-c.md) · ADRs [0004](../../decisions/0004-personajes-homenaje.md), [0006](../../decisions/0006-despliegue-y-ci.md) D10, [0007](../../decisions/0007-i18n-desde-cero.md), [0008](../../decisions/0008-autenticacion.md) D6, [0009](../../decisions/0009-ui-design-system.md)

Escrito en solo lectura, sin compilar ni correr nada, sobre `origin/ta-13-fight-events` (`557cf26`), la rama de la fase B en revisión en el PR #73. **Si esa rama cambia, se integra antes de implementar.** Se implementa en `ta-13-fight-view`.

**Alcance:** RF53 a RF58, el RNF de sincronización, los e2e de la estrategia de pruebas de la spec, las preocupaciones 9, 10 y 11, las notas para D de `plan-b.md` y `plan-c.md`, y el trabajo de cierre.

**No toca:** `crates/**`, `apps/server/src/**`, `packages/protocol-ts/**`, el motor ni el ledger. No hay apuestas ni "apuestas abiertas" (TA-27), repeticiones, verificación en pantalla ni selector de personajes. La medición en Android sigue obligatoria antes del primer despliegue público, y no se hace aquí.

**Riesgo `medium`, no el `high` de la spec:** `process.md` reserva `high` para auth, monedas, seguridad, datos personales y temas legales. El cliente solo lee datos públicos y no recibe nada que el server no haya liberado. **devops-engineer revisa** `apps/web/Dockerfile`, `check-compose`, `justfile` y el uso de `ci.yml`.

## Comprobado en el código
Leído, sin ejecutar.

1. **`fight:{id}` desconocido no cierra el socket:** responde `error` con `ws.forbidden_channel` (`session.rs`). Solo `ws.rate_limited` cierra, y el límite se consume después de saber que la pelea existe y que el canal está cerrado. Resuscribir `schedule` nunca gasta.
2. **Una cookie inválida da un socket anónimo** (`handshake.rs`). Una sesión que expira con el socket abierto cierra con `auth.unauthenticated`, y al reconectar entra como anónimo, así que no hay bucle.
3. **`MAX_FIGHT_SECS = 86 400`:** la cadencia máxima es de un día, y la rejilla mínima, de 113 s. Por tanto, ninguna configuración deja `/` quieto para siempre con peleas activas. Ver decisión 15.
4. **El e2e actual ya arranca el server con el roster real** (`playwright.config.ts`, `CONTENT_DIR`), a la cadencia por defecto. Hoy no importa, porque `/` es fijo; con la vista importaría.
5. **`arena.spec.ts` y casi todo el e2e corren con `workers: 1`.** Los proyectos no miran las mismas peleas "en paralelo" como supone la preocupación 9, salvo con una configuración aparte (decisión 16).
6. **`internal-fetch.spec.ts` y el comentario de `playwright.config.ts`** ("it never calls the server without a session cookie") suponen que una visita anónima no llama al API. RF53 lo cambia.
7. **Los dorados usan slugs de prueba** (`striker`, `trickster`, `turtle`, `tank`), que no están en `content/characters`. Con ellos se ve el respaldo de RF56.
8. **La semilla de un dorado es `ROOT_SEED` con `i` en XOR little-endian** (`crates/engine/tests/common/mod.rs`), y los specs están en `crates/engine/tests/golden/spec-*.json`. Los vectores que usa D: `log-ko-knockdown` (`striker`, `trickster`, 0) y `log-draw` (`turtle`, `turtle`, 118).

## Decisiones de implementación

### Datos y reloj
1. **`+page.server.ts`** llama a `getFightsCurrent(api)` en `lib/server/api/reads.ts` (solo `GET`, `no-mutation-in-load` lo vigila). Devuelve `{ board: FightsCurrent | null, debug: boolean }`. Un fallo da `board: null` y 200, nunca 500 (RF53).
2. **`lib/features/fight/live/clock-sync.ts`, puro.** `serverNow() = monotónico + offset`, con `performance.now()` inyectado.
   - El offset es el máximo de `server_time − monotónico` entre lo recibido en la conexión actual (RNF). Toda muestra cuenta: `schedule_update.board.server_time`, `fight_sync` y `fight_events`.
   - Una conexión nueva conserva el offset anterior hasta su primera muestra, y desde ahí empieza de cero.
   - Antes de conectar, la muestra es la del SSR, y con el poll de RF57, cada respuesta.
   - **La sonda de la decisión 5 no trae hora.** Al volver a visible con el socket vivo, se toma además una muestra con `GET /api/fights/current`, con el mismo tope de una cada 2 s. Sin ella, un reloj monotónico pausado dejaría el offset atrasado hasta el siguiente mensaje, que puede tardar minutos entre peleas. Test en `clock-sync.test.ts` y `fight-socket.test.ts`.
   - Usa el reloj monotónico y no `Date`: un cambio de hora del teléfono no mueve la pelea, y si el monotónico se pausa al bloquear la pantalla, el máximo sube con la siguiente muestra.
3. **`event-buffer.ts`, puro (DP-B9).** Un arreglo por pelea. `place(fromIndex, events)`:
   - **Solapa o repite:** descarta lo que ya tiene y agrega el resto.
   - **Hueco** (`fromIndex > length`): no agrega nada y devuelve `gap`. No puede pasar en una misma conexión, porque el cursor del server es contiguo, así que el socket lo trata como error de protocolo y reconecta con backoff. El buffer sobrevive, y las páginas de la conexión nueva, desde 0, se solapan.
   - **`fight_sync` no toca el buffer:** solo actualiza el resumen.
   - Tope de 512 eventos, el de guarda del motor (`max_events`). Pasarlo también es error de protocolo.
4. **`view-state.ts`, puro: una sola función decide qué se ve.** `viewState({ board, watched, serverNow, connection, ssrFailed })`.

   **Definiciones:**
   - **`watched`:** la pelea con sesión abierta. El store conserva su resumen, actualizado por `fight_sync` y por el tablero, así que sigue disponible aunque `current` cambie. Lleva `t`, los cues, `ending` y `end.t` si ya llegó.
   - **`painted`:** primer frame pintado, o modo texto activo (`webgl_unavailable`, 2 pérdidas de contexto, rechazo o `fail`).
   - **Pelea por venir:** `current` mientras no tenga `live_started_at` o `serverNow < live_started_at + delay`; si no, `next`. Su objetivo es `(live_started_at ?? live_at) + playback_delay_ms`.
   - **`final` visible:** de una pelea no mirada, siempre. De la mirada, de inmediato si no es `decided` (`voided`, y lo mismo si `ws.forbidden_channel` la dejó sin eventos), y si es `decided`, desde `t ≥ max_duration_ms`.
   - **Mirada terminada:** se cumple una de estas: tiene `end` y `t ≥ end.t + 10 s`; al estar al día ya había pasado `end.t` (decisión 7, sin Pixi); no tiene `end` y tiene `final` visible; o tiene `final` visible que no es `decided`.

   **Prioridad:**

   | # | Condición | Estado |
   |---|---|---|
   | 1 | `ssrFailed` y sin datos todavía | `error` |
   | 2 | `halted === true` | `halted`, con `last` y el resultado ya visto |
   | 3 | Todo nulo y `halted === null` | `loading` (desviación 2) |
   | 4 | Todo nulo y `halted === false` | `starting` |
   | 5 | Hay `watched`, `t ≥ 0`, sin terminar y `painted` | `live`, con su pose incluida |
   | 6 | `current` en `live`, `serverNow ≥ live_started_at + delay`, y no es una mirada terminada ni una mirada en la fila 5 | `live` sin lienzo, salvo que `watched` espere su primer frame: entonces `about_to_start` |
   | 7 | Hay una pelea por venir y su objetivo ya pasó | `about_to_start`, con `last` si existe |
   | 8 | Hay una pelea por venir | `next`, con `last` si existe |
   | 9 | Solo `last`, o una mirada terminada | `result` |

   - **`last` sigue en las filas 7 y 8** hasta que la siguiente esté en vivo (RF54). `halted === null` nunca da `halted`.
   - **`live` y el rótulo "En vivo":** se quita en cuanto exista `ending`, también durante la pose.
   - **Fin de la pose con la liquidación atrasada:** `current` sigue siendo esa pelea y no hay `final`, pero el resultado sale de sus eventos. Se ve en la fila 8 o 9, con la tarjeta de resultado, y nunca vuelve a la cuenta atrás de esa pelea.
   - **Lienzo con `halted`:** se desmonta si no queda nada que reproducir, es decir, si no hay ningún cue con `at_ms > t`.
   - `connection === 'degraded'` no es un estado propio: suma el aviso de RF57 a cualquiera de estos y oculta el reloj de ronda.
   - **Tests:** una fila de `view-state.test.ts` por cada caso de arriba: `current` en `live` con `t < 0`, sin `next` ni `last`; `Settled` con la pose en curso y con un cliente atrasado; `voided` y `ws.forbidden_channel` de la mirada; fin de pose con la liquidación atrasada; `last` en la fila 7; empate con `Settled` durante la pose; `log-ko-clutch` en `t = 90 000`; entrada con `end.t` ya pasado y `current` en `live`; y `painted` en modo texto. `home.spec.ts` suma el caso de la mirada que pasa a `voided` y el del modo texto en vivo.

### Cliente WS (RF57, DP-B7)
5. **`fight-socket.ts`: `/ws` relativo, sin URL absolutas (ADR-0012), con `WebSocket`, temporizadores y azar inyectados.** Al abrir, suscribe `schedule`. Del tablero deduce un único canal `fight:{id}`, el de `current` en `betting_closed` o `live`. **Un socket nuevo se suscribe también a la pelea mirada que aún no tiene `end`,** para que un corte no la deje sin eventos si ya es `last` (RF57). **Nunca se suscribe a `next` ni a un `last` que no se esté mirando:** el tablero ya trae sus resúmenes, y seguirlos podría pasar el límite de DP-B7 (decisión 6).
   - **Reconexión:** 1, 2, 4… hasta 30 s, con jitter de ±20 % (`Math.random`, que no toca el motor ni una pelea). Un fallo es un cierre o un error antes de recibir el primer mensaje. El contador vuelve a 0 con el primer mensaje.
   - **3 fallos seguidos:** `connection = 'degraded'`, con un aviso único. El primer `GET /api/fights/current` sale al entrar en ese modo, y los siguientes, cada 30 s con jitter, hasta reconectar. Sigue intentando.
   - **`visibilitychange` a visible y `online`: "comprobar el socket".** Si no está `OPEN`, reconecta ya y reinicia el backoff. Si está `OPEN`, puede estar muerto sin saberlo, así que envía una sonda: `subscribe schedule`, que con el canal abierto responde `subscribed` y no gasta DP-B7. Sin respuesta en 3 s, cierra y reconecta. Como mucho, una sonda cada 2 s, y una reconexión inmediata cada 2 s.
   - **Cierres:** `ws.rate_limited`, `auth.*`, `ws.pong_timeout` y cualquier otro van por el mismo camino, con backoff (nota de `plan-b.md`). `pagehide` cierra el socket, y `pageshow` persistido reconecta. El socket vive solo en `/`.
   - **`error` con `ws.forbidden_channel`:** no reintenta ese `id` hasta que un `schedule_update` lo vuelva a nombrar (tras un reinicio, hasta la primera evaluación). No cuenta como fallo.
   - **Canal silencioso (DP-B5):** cuando la pelea mirada ya no es `current` ni `last` y su reproducción terminó, envía `unsubscribe`.
6. **DP-B7: un cliente legítimo no llega al límite, y un test lo fija.** El límite es de 3 `fight_sync` pedidos por socket y minuto. El cliente pide como mucho **uno por pelea y socket**, y 2 peleas solo en un socket nuevo con una mirada sin terminar: nunca desuscribe un canal para volver a abrirlo, y la sonda usa `schedule`.
   - **Solo `current`:** un cliente que siguiera `current`, `next` y `last`, y entrara entre las 9:00 y las 9:35 de una ranura de 10 min, podría pasarse (nota de QA). Por eso `next` y `last` se leen del tablero, sin canal.
   - **Cambio de pelea:** una rejilla no baja de 113 s, así que hay como mucho un pedido cada 113 s.
   - **Volver a primer plano:** no pide nada con el socket vivo, y con uno nuevo abre una ventana nueva.
   - **Varias pestañas:** el límite es por socket.
   - **Prueba:** `fight-socket.test.ts` recorre 1 hora de tableros con la rejilla mínima, sondas y reconexiones con semilla fija, y cuenta los `subscribe fight:*` a canal cerrado por socket en cada ventana de 60 s: nunca más de 2 en un socket nuevo (`current` y esa mirada), y 1 en los demás casos, bajo el límite de 3. Comprueba además que ningún `subscribe` nombra el `id` de `next` ni el de un `last` que no se estuviera mirando, también entrando entre las 9:00 y las 9:35 de una ranura de 10 min. El e2e de peleas cuenta lo mismo en el socket real.
   - Si esto cambiara al implementar, vuelve a security-reviewer. Las reconexiones también gastan el límite de upgrades (30/min con ráfaga de 10): el tope de una inmediata cada 2 s y el backoff quedan por debajo.

### Reproducción
7. **`fight-session.ts`: `LiveSession` sobre el buffer.** Se resuelve cuando el buffer tiene el `intro` en el índice 0 y está al día: ya tiene `end`, o pasaron 300 ms sin otra página. **No depende del tamaño de las páginas,** que es un detalle de la fase B (`PAGE_EVENTS`). Así, **`loadLive` espera al `intro`**, una lista vacía nunca llega al adaptador (`fight_sync` sin eventos, también en `live` antes de `live_started_at`, R4), y se sabe si `end` ya pasó.
   - **Si al estar al día ya pasó `end.t`, no se monta Pixi:** tarjeta de resultado, con lo que dicen los eventos. Con una red tan lenta que las páginas se separen más de 300 ms, el lienzo puede llegar a montarse, y se desmonta al llegar `end`, por la decisión 4. `fight-session.test.ts` usa páginas de distintos tamaños.
   - `clock = serverNow() − live_started_at − playback_delay_ms`.
   - `timeline` y cada `push` salen de `liveTimelineFromEvents(summary, eventos, fighters, summary.max_duration_ms)`.
   - Cada grupo de páginas de un mismo tick se junta en un `push` por `queueMicrotask`.
   - **Quien entra tarde a una pelea en curso** instala con lo ya recibido y extiende con lo que venga. Una página atrasada solo adelanta cues en ráfaga (decisión 10).
8. **`LiveSession.subscribe` suma un segundo argumento, `fail(code)`,** para un adaptador que rechaza. Es un cambio pequeño de la interfaz de la fase C (desviación 4). `log-drip.ts` lo ignora.
9. **RF54 frente a RF50: el resultado sale de `snapshot().ending`, y el host se entera leyendo cada 250 ms.**
   - **Cómo se entera.** Mientras hay `live`, `FightStage` lee `handle.snapshot()` cada 250 ms y llama a una prop `onsnapshot`. `onCue` no sirve para quien entra tarde, porque RF50 no dispara lo saltado, y `onState` llega 2500 ms tarde. El primer `play()` ya pinta `t` (fase C), así que quien entra tras el cierre lo ve en la primera lectura. La lectura para al llegar a `finished`, y no toca el renderer.
   - **Regla para no adelantar el final:** `Settled` llega en `end + 5000` y el cierre se ve en `end + 3500 − 2500`, pero un cliente con la red atrasada puede ir tarde. Para la pelea mirada, un final `decided` no adelanta el resultado antes de `t ≥ max_duration_ms`. Un final que no es `decided` se muestra de inmediato, porque esa pelea nunca traerá `end` y quedaría en vivo para siempre (decisión 4). Tampoco se adelanta en modo degradado si la pelea sigue mirándose: la decisión 6 de producto solo desmonta al llegar a esa regla.
   - **Sin WebGL (`webgl_unavailable`): `text-scene.ts`, puro.** Dados los cues y `t`, da HP, fase y resultado, este solo con `t ≥ end.t`, igual que RF54, y se refresca con el mismo intervalo de 250 ms.
   - **`halted` con el final ya visto:** se conserva lo derivado de los eventos, junto al aviso (RF54). Modo texto del HP: `fight_text_hp`.
10. **Ráfaga de cues vencidos: la vista no depende de `onCue`.** Nada de `/` se engancha a `onCue`: ni el resultado, ni el HP, ni el reloj de ronda. No hay sonido ni efectos por cue. En un corte largo o una entrada tarde, la ráfaga solo llega a listeners inexistentes y la escena es correcta por `t`. El `onCue(refresh)` de `debug` queda solo en la arena.
11. **`live-host.ts`, puro: lo que hoy está dentro de `FightStage`.** Mueve el orden de la fase C (`loadLive`, sin esperar `subscribe`, `await`, `play`) a una función con el `handle` y la sesión inyectados. Esa extracción es lo que hace comprobable el rechazo sin montar Pixi.
    - **Rechazo de `extend`:** estado de error y suscripción cancelada, **tanto en la entrega inmediata de `subscribe` como en una posterior** (criterio del architect). Hoy, `renderer !== handle` cubre el primero; pasa a estar en esta función.
    - **`fail` del adaptador** se prueba igual, también en entrega inmediata, en `live-host.test.ts`.
    - **En `/`, un rechazo o un `fail`** no deja una pantalla rota: pasa al texto de `text-scene.ts` si los eventos sirven, y si no, al estado por tablero, con el aviso `fight_notice_playback_error`.
12. **RF55: `round-clock.ts`, puro.** `max(0, time_limit_ms − t)`, congelado en el `at_ms` del primer `ko` o `time_up` con `at_ms ≤ t`. Se decide por `t`, nunca por haber recibido el evento. `role="timer"`, `aria-live="off"` y etiqueta accesible propia, así que no anuncia cada segundo (RF58). Se oculta con `degraded`. Sin barra de progreso.
13. **`webglcontextlost` y bloqueo de pantalla (sección "UI" de la spec).** `FightStage` escucha el evento en el `canvas` de su contenedor y rehace el renderer con la misma sesión: la escena sale de `t`. Tras 2 pérdidas en 60 s, pasa a texto. El bloqueo de pantalla no necesita más: el reloj sigue, y la decisión 5 repone el socket.

### Vista
14. **Componentes.** Primitivas existentes. Nuevos en `lib/components/fight/`, todos en `/dev/components` (`ArenaSection` o una sección `FightSection`): `FightHome` (compone), `FightBillboard` (personajes, epíteto y cuenta atrás), `FightCommit`, `RoundClock`, `FightResultCard`, `FightNotice`. Solo tokens. `FightStage` suma props opcionales `names`, `onsnapshot` y `showResult` (por defecto, como hoy en la arena). En `/` no hay controles de reproducción ni "Repetir".
    - **`Pixi` solo por `import()`:** `FightStage` se monta al abrir `pre_live`, es decir, con `current` en `betting_closed`, o al entrar en `live`, nunca antes (RF53 y sección "UI"). Hasta recibir el `intro` solo descarga el módulo, sin `canvas`.
    - **Sin lienzo vacío en `pre_live`:** la cartelera y la cuenta atrás siguen visibles hasta `t ≥ 0` con el primer frame pintado (`painted`, filas 5 y 6 de la decisión 4). El escenario se monta oculto, sin `display: none`: con tamaño 0, el lienzo no da una medida válida y el primer frame no significa nada. Un test comprueba que el contenedor tiene tamaño. **Criterio con red móvil limitada** (Chromium, `Network.emulateNetworkConditions` con el perfil Fast 3G, y CPU a 4×, en `home.spec.ts` con eventos simulados y los placeholders de RF56, que son los assets de esta fase, y sin `page.clock`, porque con el reloj falso "antes de `t = 0`" no mide nada): el primer frame llega antes de `t = 0`, con los 3,5 s de margen. Si no llega, las hojas se precargan en `pre_live`, sin más ronda.
    - **`FightCommit`:** un `<details>` cerrado, con el commit completo y copiable (`break-all`, `select-all`) y su explicación. Sin `title` ni truncar (desviación 7). `<summary>` de 44 px o más.
    - **375 px:** la cartelera se ve completa sin scroll a 375×667, con los personajes apilados, porque hay epítetos de 40 caracteres, y `home_lead` debajo. Cuenta atrás y reloj con `tabular-nums`.
    - **Regiones:** `aria-live` en el resultado, y `role="status"` en `halted` y `degraded`, cada una sin anuncios repetidos.
    - **SSR sin saltos:** la cuenta atrás parte de `board.server_time`, igual en servidor y primer render del cliente, y empieza a correr en `onMount`, para que no haya error de hidratación.
    - **RF56:** `apps/web/vite/fighter-lore.ts`, módulo virtual `virtual:fighter-lore`, que extrae solo `name` y `epithet` del frontmatter con `yaml`, y `fighter-names.ts` con el respaldo por slug. `apps/web/Dockerfile` copia `content/characters` en su etapa de build.
    - **`debug` solo con `ENABLE_DEV_CATALOG` y `?debug=1`:** `data-t`, `data-t-at`, `data-events`, `data-event-digest`, `data-view`, `data-subscribes`, `data-connection` y `data-ws-failures`. Solo describen lo ya liberado y mostrado. En producción, `+page.server.ts` da `debug: false` siempre, con un test.

### e2e (preocupaciones 9, 10 y 11)
15. **Estado conocido en `/` para el e2e actual: `starting`.** `playwright.config.ts` cambia `CONTENT_DIR` a un directorio vacío, `e2e/support/no-roster/`. Da `fights.roster_invalid` una vez, sin peleas y con `halted: false`, para siempre y sin depender del reloj. `a11y`, `hydration`, `i18n`, `screenshots` y `auth` ven siempre lo mismo, con el WS real y sin tocar sus aserciones. **Alternativa descartada:** la cadencia de un día deja un cruce con la medianoche UTC en cerca del 0,5 % de las corridas.
    - **Los demás estados, en `e2e/home.spec.ts`:** `page.routeWebSocket` con mensajes simulados y `page.clock`, deterministas y en segundos, como permite RF54. Cubre `next`, `about_to_start`, `live`, `result`, `cancelled`, `voided`, `halted`, RF57 y los rechazos. Va también en el proyecto `webkit`.
16. **Peleas reales: `playwright.fights.config.ts` y `e2e/fights.spec.ts`, una corrida aparte, en el job `e2e` actual.** `_e2e-run` la lanza después de la actual, y repite entre las dos `reset-run.mjs` y `db.mjs reset`.
    - **Por qué aparte:** es la única forma de que Chromium y WebKit, de escritorio y a 375 px, miren **las mismas peleas en paralelo** (`workers: 4`), como pide la preocupación 9, sin subir los workers del resto.
    - **Config:** `globalTimeout` de 8 min, `retries: 0`, `test.setTimeout` de 7,5 min, el mismo `globalTeardown` y `outputDir` propio (`test-results/fights-artifacts`).
    - **Logs en CI:** solo se suben los de la última corrida, porque `serve-with-log.mjs` los vacía al arrancar, y `reset-run.mjs` solo limpia `egress.log` y los archivos de corrida. Un fallo en la primera se conserva: `set -e` detiene la segunda antes de arrancar.
    - **Mismo stack, otras variables:** `playwright.config.ts` exporta sus `webServer` como función. Las diferencias son `FIGHT_SCRIPT_PATH`, `FIGHT_CADENCE_SECS=113`, `FIGHT_BETTING_WINDOW_SECS=5` y `FIGHT_PRE_LIVE_SECS=3`. Base y puertos son los mismos, porque no corren a la vez.
    - **Script:** `e2e/support/fight-script.mjs` escribe `test-results/fight-script.json` con `log-ko-knockdown` y `log-draw` (hallazgo 8). No se commitea, y si los dorados se regeneran, sigue valiendo.
    - **WebKit sin WebGL (preocupación 10):** las aserciones son por estados y `data-*` bajo `debug`, nunca por píxeles, y aceptan `webgl_unavailable` con la anotación `limited`, como `arena-webkit.spec.ts`.
17. **`fights.spec.ts`: un solo flujo por proyecto, con contextos simultáneos.** Un único test con pasos. **La pelea se identifica por sus slugs, nunca por su orden,** porque RF45 elige la fila por las filas previas: `striker` y `trickster` son el KO, y `turtle` y `turtle`, el empate.

    | Paso | Pelea | Contexto | Idioma | Entrada | Qué comprueba |
    |---|---|---|---|---|---|
    | A | KO | `main`, anónimo | es | Antes de empezar | Próxima, por comenzar, en vivo, resultado de KO, y `last` junto a la próxima |
    | B | KO | `late`, anónimo | en | A 20 s de `t` | Entrar a mitad: `fight_sync` y páginas, y el mismo `data-event-digest` que `main` hasta ese punto |
    | C | Empate | `session`, con sesión | es | Antes de empezar | RNF: con `anon`, `t` difiere ≤ 500 ms, y el digest coincide |
    | D | Empate | `anon`, anónimo | en | Antes de empezar | RNF y resultado de empate |
    | E | Empate | `cut`, anónimo | es | Antes de empezar | Corte con `routeWebSocket` y `connectToServer` a 30 s de `t`: se pone al día con páginas |
    | F | Empate | `window`, anónimo | es | En `time_up + 5500`, de tiempo del server | RF54 frente a RF50, con una pelea real |
    | G | Ambas | `request` | | Tras `Settled` | `log` igual byte a byte al dorado, y `SHA-256(seed) == seed_commit` |

    - **Si se entra con una pelea empezada:** cada paso espera a la primera pelea con sus slugs cuyo `t` sea negativo al llegar. Si el arranque se retrasó y el KO ya empezó, A y B usan el KO del siguiente ciclo. Eso cabe en el `globalTimeout`: unos 66 s, 113 s y el KO completo, cerca de 5 min. G acepta cualquier pelea de esos slugs.
    - **Socket por socket:** 6 contextos y una reconexión: 7 upgrades por proyecto y 28 en total, en unos 5 min.
    - **Ráfaga de upgrades:** `WS_ANON_UPGRADE_IP` da 30/min con ráfaga de 10 por IP, y el e2e sale de una sola. Un rechazo contaría como fallo de RF57 y falsearía las aserciones. Sin subir el límite, cada grupo de entradas se escalona 1 s por proyecto, y hay 10 s o más entre grupos: A y C-E se abren en grupos separados. Cada grupo son como mucho 4 upgrades, y en 10 s vuelven 5 fichas. F se escalona 0,5 s, porque su ventana solo dura de `time_up + 5000` a `time_up + 9000`.
    - **`degraded` nunca aparece en esta corrida:** cada contexto mira `data-connection` y `data-ws-failures`, y al final los dos deben seguir en `live` y 0. Si falla, el test falla, y no se reintenta.

## Archivos que cambian
| Archivo | Cambio |
|---|---|
| `apps/web/src/routes/+page.server.ts` | nuevo: RF53 |
| `apps/web/src/routes/+page.svelte` | modificado: `FightHome` |
| `apps/web/src/lib/server/api/reads.ts` | modificado: `getFightsCurrent` |
| `apps/web/src/lib/features/fight/live/clock-sync.ts`, `event-buffer.ts`, `view-state.ts`, `round-clock.ts`, `text-scene.ts`, `live-host.ts`, `fighter-names.ts` | nuevos, puros, con su `.test.ts` |
| `apps/web/src/lib/features/fight/live/fight-socket.ts`, `fight-session.ts` | nuevos, con dependencias inyectadas y su `.test.ts` |
| `apps/web/src/lib/features/fight/live/live-store.svelte.ts` | nuevo: une socket, reloj y buffers en runas |
| `apps/web/src/lib/features/fight/live-session.ts` | modificado: `fail` en `subscribe` |
| `apps/web/src/lib/features/arena/log-drip.ts` | modificado: firma de `subscribe` |
| `apps/web/src/lib/components/fight/FightStage.svelte` | modificado: `live-host`, `names`, `onsnapshot`, `showResult` y `webglcontextlost` |
| `apps/web/src/lib/components/fight/{FightHome,FightBillboard,FightCommit,RoundClock,FightResultCard,FightNotice}.svelte` | nuevos |
| `apps/web/src/routes/dev/components/_sections/*` | modificado: los componentes nuevos con datos de ejemplo |
| `apps/web/messages/{en,es}.json` | modificado: `fight_*` (ver "Textos"); sale `home_coming_soon` |
| `apps/web/vite/fighter-lore.ts`, `apps/web/vite.config.ts`, `apps/web/src/virtual-fighter-lore.d.ts` | nuevo módulo virtual |
| `apps/web/package.json`, `pnpm-lock.yaml` | `yaml` como devDependency, la versión de la raíz, sin versión nueva |
| `apps/web/Dockerfile`, `scripts/check-compose.mjs`, su test y sus fixtures | `COPY content/characters` en la etapa de build (RF56) |
| `.dockerignore`, `.github/workflows/ci.yml` | solo si hace falta: `web` ya filtra `content/characters/**` |
| `apps/web/tests/fighter-lore.test.ts` | nuevo: los dos campos por personaje y locale |
| `apps/web/playwright.config.ts` | modificado: `webServer` exportable y `CONTENT_DIR` vacío; `home.spec.ts` en `webkit` |
| `apps/web/playwright.fights.config.ts`, `e2e/fights.spec.ts`, `e2e/home.spec.ts`, `e2e/support/fight-script.mjs`, `e2e/support/fake-ws.ts`, `e2e/support/no-roster/.gitkeep` | nuevos |
| `apps/web/e2e/{internal-fetch,security,screenshots}.spec.ts`, comentarios del config | solo lo que RF53 cambie (hallazgo 6) |
| `justfile` | `_e2e-run`: `reset-run.mjs`, `db.mjs reset` y segunda corrida |
| Documentos de "Trabajo de cierre" | ver abajo |

## Textos
Con las correcciones de product-owner; `fight_notice_playback_error`, `fight_round_clock_label` y `fight_text_hp` son redacción mía para las claves que pidió. Todo en `messages/{en,es}.json`, y `check-i18n` lo exige.

| Clave | es | en |
|---|---|---|
| `fight_view_starting` | Estamos preparando la cartelera. | We're getting the card ready. |
| `fight_view_loading` | Cargando la cartelera… | Loading the card… |
| `fight_view_error` | No pudimos cargar la cartelera. Seguimos intentando. | We couldn't load the card. We'll keep trying. |
| `fight_view_next` | Próxima pelea | Next fight |
| `fight_view_countdown` | Empieza en {time} | Starts in {time} |
| `fight_view_about_to_start` | Por comenzar | About to start |
| `fight_view_live` | En vivo | Live |
| `fight_view_versus` | vs | vs |
| `fight_view_commit` | Semilla sellada | Sealed seed |
| `fight_view_commit_help` | Se revela al terminar, para que cualquiera pueda comprobar la pelea. | Revealed after the fight so anyone can check it. |
| `fight_view_halted` | Las peleas están detenidas por un problema nuestro. Esta página se actualizará sola cuando vuelvan. | Fights are on hold because of a problem on our end. This page will update on its own when they're back. |
| `fight_view_last` | Pelea anterior | Previous fight |
| `fight_round_clock_label` | Tiempo restante de la ronda | Round time left |
| `fight_text_hp` | {name}: {hp}/{max} de vida | {name}: {hp}/{max} HP |
| `fight_result_ko` | {name} ganó por KO | {name} won by KO |
| `fight_result_decision` | {name} ganó por decisión | {name} won by decision |
| `fight_result_draw` | Empate | Draw |
| `fight_result_cancelled` | No se disputó | Did not take place |
| `fight_result_voided` | Anulada por un problema técnico | Voided due to a technical problem |
| `fight_notice_degraded` | No pudimos conectarte en vivo; seguimos intentando. El resultado aparecerá aquí. | We couldn't connect you live. We'll keep trying. The result will show up here. |
| `fight_notice_playback_error` | No pudimos mostrar la animación. El resultado aparecerá aquí. | We couldn't show the animation. The result will show up here. |
| `fight_notice_no_animation` | Este navegador no puede mostrar la animación. | This browser can't show the animation. |
| `fight_name_unknown` | Luchador {slug} | Fighter {slug} |

## Orden de trabajo
Primero integrar `ta-13-fight-events` si cambió. La Enmienda 4 de la spec ya está aplicada. **No se empieza hasta que product-owner confirme el texto final de RF54 y se fusione el PR #73.** Cada paso es un commit `(TA-13)` con sus tests en verde. Vitest y `just check-web`; Rust solo se compila para el e2e.

1. **Núcleo puro:** `clock-sync`, `event-buffer`, `view-state`, `round-clock`, `text-scene`, con sus tests. *Paralelizable con el paso 2 y el 3.*
2. **RF56:** módulo virtual, `fighter-names`, `Dockerfile`, `check-compose` y su test. *Paralelizable.* Revisa devops.
3. **`fight-socket` y la prueba de DP-B7.** *Paralelizable.* Es el paso con más casos: si el conteo no da, para y vuelve a security-reviewer.
4. **`fight-session`, `live-host` y `FightStage`,** con el test de rechazo en las dos entregas. La arena sigue igual: `arena.spec.ts` no cambia. **Es el paso más riesgoso:** mueve el orden de arranque de la fase C.
5. **Vista:** `+page.server.ts`, `live-store`, componentes, textos y `/dev/components`. Capturas a 375 px y en escritorio, en dark y light, en es y en, comparadas con la spec. Depende de 1 a 4.
6. **`e2e/home.spec.ts`** y el estado conocido (decisión 15). Corre el e2e existente completo: `a11y`, `hydration`, `i18n`, `screenshots`, `auth`, `logs`, `internal-fetch`.
7. **`fights.spec.ts`, su config y `_e2e-run`.** Revisa devops.
8. **Cierre:** documentos y Linear (abajo).
9. **`just check` y `just e2e`,** con la salida en el PR. Nunca `just db-reset` ni `just db-down`.

## Riesgos
- **Un spoiler del resultado en la UI antes de su hora.** Lo más grave de esta fase: `ko` y `end` llegan 3,5 s antes de mostrarse, y `final` puede ganarle a un cliente atrasado. → Nada de la vista sale de haber recibido un evento: todo pasa por `t` (decisiones 9 y 12). `home.spec.ts` entrega `end` con `t` anterior y comprueba que ni el texto, ni el reloj, ni `aria-live`, ni el DOM sin `debug` lo delatan. Lo que viaja por el socket sí se ve en las herramientas de desarrollo, y la spec lo acepta.
- **Mover el arranque de `FightStage` rompe la fase C.** → `arena.spec.ts` y `player-live.test.ts` sin cambios, y `live-host` extraído antes de agregar nada.
- **Más visitas anónimas llaman al API.** → Hallazgo 6: `internal-fetch.spec.ts` compara lo que ve en `/es/login`, no en `/`, así que lo espero sin cambios, sin haberlo corrido. `GET /api/fights/current` sale de memoria, sin base y sin límite (RF35).
- **El WS real en cada visita a `/` del e2e actual agota el límite de upgrades,** que deja pasar 10 de golpe por IP. Un fallo deja un error de consola en un test que los vigile. → El cálculo depende de cuántas veces se visite `/`, y no lo medí. El paso 6 lo comprueba. Si pasa, en esos tests se bloquea `/ws` con `routeWebSocket`, sin subir el límite del server.
- **El primer frame llega tarde con red móvil real:** el criterio solo prueba los placeholders, y Fast 3G es un perfil, no un teléfono. → Si falla, precarga en `pre_live`. Cuando haya sprites reales hay que repetirlo, y el prerrequisito 8 sigue abierto.
- **`fights.roster_invalid` en `error` en el log del e2e.** → No vi ningún test que falle por eso, pero no corrí nada. Si falla, el estado conocido pasa a un roster de un solo personaje o a filtrar ese evento.
- **`connect-src 'self'` y WebSocket en WebKit.** → No hay prueba hoy de un WS real con la CSP en WebKit. `fights.spec.ts` la da. Si falla, es una ampliación que ya prevé ADR-0012, y vuelve a architect.
- **La sonda de `visibilitychange`:** un teléfono lento puede tardar más de 3 s y reconectar sin necesidad. → Solo cuesta una reconexión, con ventana nueva. Una por cada 2 s como máximo.
- **`fights.spec.ts` es lento y depende del reloj real:** en el peor caso, 113 + 113 + 8 + 90 + 5 s, unos **5,5 min**, no los 4 de la preocupación 9. → Una sola corrida para los 4 proyectos, con `globalTimeout` de 8 min y sin reintentos. Se mueve a un job propio, en el mismo PR, si alguna de 3 corridas del PR pasa de 16 min.
- **Dos clientes difieren más de 500 ms en CI:** con carga, los `server_time` pueden tardar. → La comparación normaliza con `data-t-at`, y el máximo elimina la muestra lenta. Si es inestable, se investiga y no se sube la tolerancia.
- **PR #73 cambia los mensajes o `FightsCurrent`.** → Se integra antes de implementar, y `svelte-check` lo detecta.
- **La página `/` con Pixi empeora la entrada móvil.** → Solo se carga en `pre_live` o en vivo. `check-renderer-split` y `just arena-bench` sin cambios. Los fps de `/` no son una medición nueva: es el mismo `FightStage`.

## Alternativas descartadas
- **Un método `onScene` o un evento del renderer para avisar del cierre.** Más preciso que leer cada 250 ms, pero toca el renderer para algo que la vista no necesita con más precisión. Al leer se reutiliza `snapshot()`, que ya es función de `t`.
- **Mostrar el resultado por `final` cuando llegue.** Más simple, y adelanta el resultado a un cliente atrasado.
- **`Date.now()` para el offset.** Más simple, y un cambio de hora del teléfono movería la pelea.
- **Cerrar y abrir `fight:{id}` para pedir un `fight_sync` nuevo.** Gasta DP-B7. La sonda con `schedule` y la reconexión bastan.
- **Stack de e2e aparte, con otra base y otros puertos, en paralelo con el principal.** Más puertos, otro proxy y otro egreso, sin ganar más que los minutos, y con más carga en CI.
- **Mockear todo sin peleas reales.** No prueba DP-B9, la sincronización ni los bytes del `log`, que pide la spec.
- **Copiar nombres y epítetos a `messages/`.** RF56 lo prohíbe.
- **Probar el rechazo de `extend` montando `FightStage` con Testing Library.** Suma una dependencia y un entorno de DOM que el repo no usa. `live-host` más e2e con `routeWebSocket` lo cubren.

## Pruebas (cómo se demuestra que funciona)
- [ ] `just check` y `just e2e` en verde, con la salida en el PR
- [ ] **RF53:** Vitest del `load` con API caída: 200 y estado de error. e2e con el server inalcanzable. `check-renderer-split`. Sin sesión.
- [ ] **RF54:** tabla de `view-state.test.ts`, una fila por prioridad y por cada caso de la decisión 4, incluidos `halted` nulo que nunca da detenida, `halted` por encima de `live`, y cuenta atrás a cero sin `live` que da "por comenzar", nunca un `0:00` congelado. `home.spec.ts`: un caso por estado, en es y en; `last` al entrar a mitad; cancelada ("no se disputó"); anulada, con mensaje simulado.
- [ ] **RF54 frente a RF50:** `home.spec.ts` entra dentro de la ventana (`t` más allá del cierre, antes de `Settled`), y el resultado aparece sin ningún cue. `fights.spec.ts` lo repite con una pelea real (paso F). Un cliente que ya mira ve el resultado como máximo 250 ms después del cierre.
- [ ] **Spoiler:** `end` recibido con `t < cierre`: nada en la vista, y un `final` `decided` en el tablero tampoco lo adelanta para la pelea mirada antes de `t ≥ max_duration_ms`. Uno `voided` se muestra de inmediato.
- [ ] **Pose:** "En vivo" desaparece al existir `ending`, y el lienzo sigue hasta `t ≥ end.t + 10 s`. Con `end.t` ya pasado al recibir los eventos, ningún `canvas` y tarjeta de resultado.
- [ ] **`pre_live`:** cartelera y cuenta atrás visibles hasta el primer frame, y el criterio de red limitada de la decisión 14.
- [ ] **Commit:** `<details>` cerrado, y al abrirlo, el commit completo, seleccionable y sin scroll horizontal a 375 px.
- [ ] **RF55:** Vitest de `round-clock`, y en e2e para al llegar el `ko` y el `time_up`. Sin barra. Oculto con `degraded`.
- [ ] **RF56:** `fighter-lore.test.ts`, respaldo por slug y `check-compose`.
- [ ] **RF57:** Vitest de `fight-socket` con temporizadores falsos: secuencia de backoff y su tope, jitter, aviso único tras 3 fallos, primer poll al entrar en `degraded` y luego con jitter, sonda sin respuesta, muestra de hora al volver a visible con el socket vivo, un `ws.rate_limited` tratado como reconexión. e2e con el WS bloqueado, y con `visibilitychange` y `online`.
- [ ] **DP-B7:** la prueba de la decisión 6, y `data-subscribes` en `fights.spec.ts`.
- [ ] **DP-B9:** Vitest de `event-buffer`: solape parcial, repetido, hueco, fuera de orden y tope. `fight_sync` no toca el buffer. `loadLive` no se llama sin `intro`, con `fight_sync` vacío en `live` antes de `live_started_at`.
- [ ] **Rechazo de `extend` y `fail`:** `live-host.test.ts`, los dos en entrega inmediata y posterior: estado de error y suscripción cancelada. `home.spec.ts` lo repite con golpes a menos de 600 ms, que pasan el adaptador y rechaza `validateTimeline`.
- [ ] **Ráfaga:** tras un corte, `home.spec.ts` entrega de golpe una tanda de eventos vencidos: la escena es la de `t`, sin errores y con un solo anuncio.
- [ ] **`webglcontextlost`:** e2e en Chromium con `WEBGL_lose_context`: vuelve a dibujar en su `t`.
- [ ] **RF58:** `check-i18n`, axe en cada estado, en dark y light, 375 px sin scroll horizontal, `aria-live` en el resultado y `role="status"` solo en `halted` y `degraded`.
- [ ] **375×667:** la cartelera, con el epíteto más largo, cabe entera en el viewport sin scroll, comprobado con su caja.
- [ ] **RNF de sincronización y estrategia de la spec:** la tabla de la decisión 17, en los 4 proyectos, sin `degraded` en toda la corrida.
- [ ] **Capturas** de cada estado a 375 px y en escritorio, en dark y light, revisadas y descritas en el PR, con el epíteto más largo y los placeholders a 375 px, en es y en
- [ ] **Linear:** criterios de TA-27 y delegación en TA-13 comprobados (spec, "Trabajo de cierre")

## Trabajo de cierre
Se aplica en el paso 8, cada pieza cuando ya existe.

| Documento | Cambio |
|---|---|
| `docs/architecture.md` | Ciclo con el final cancelado, canales y sincronización, prerrequisito 5 cerrado, y "más un pequeño buffer" reemplazado por `playback_delay_ms` en el cliente. Renderer: `prepareSceneFrom`. **Prerrequisito 8 (Android) se conserva abierto** |
| `docs/glossary.md` | `FightSnapshot`, `FightFinal`, `Cancelada`, `Anulada` (`voided`), `FightState`, parada segura, detención, retraso de reproducción. `SeedCommit` no cambia |
| Spec de 001 | Enmienda de RF-B13 y de su frase en la línea 277 ("ningún socket vive… más de 120 s sin mensajes aceptados"), por RF42: el cierre por inactividad solo corre sin suscripciones |
| Spec de 003 | RF22 resuelta con la opción (a), con enlace a 010 |
| `CLAUDE.md` | `dev-fights`, `FIGHT_SCRIPT_PATH`, `CONTENT_DIR`, variables `FIGHT_*`, y la segunda corrida de `just e2e` |
| `apps/server/.env.example` | `FIGHT_CADENCE_SECS`, `FIGHT_BETTING_WINDOW_SECS`, `FIGHT_PRE_LIVE_SECS` y `CONTENT_DIR`, comentadas, con sus valores por defecto |
| `apps/server/README.md` | Cómo verificar una pelea con datos públicos (`GET /api/fights/{id}`, `SHA-256(seed)`, RF2 y resimular) |
| `REVIEW.md` | Regla de RF6: cambiar o quitar una entrada de `ruleset_freeze.rs` es bloqueante |
| `plan-c.md` → `spec.md` | **Enmienda 4, redactada por el architect y ya aplicada, antes de implementar.** Recoge la "Enmienda propuesta a la spec" de `plan-c.md` (RF46, RF47, RF50, RNF "CPU" y tabla de componentes) y las desviaciones aceptadas de abajo. product-owner confirmó RF50, y el RNF de CPU solo como aproximación, con el prerrequisito 8 abierto. Falta que confirme el texto final de RF54. Mantiene escrito el plazo de Android |
| Linear | Criterios bloqueantes de TA-27 y delegación en TA-13 |

ADR-0002 no cambia en 010. El prerrequisito de RF6 y el de Postgres siguen abiertos, en TA-8.

## Decisiones de producto y diseño
Con las correcciones de product-owner en `## Revisión`.

1. **Tras el final, la pose dura hasta `t ≥ end.t + 10 s`** y el lienzo se desmonta. Después, `FightResultCard` con el resultado junto a la próxima pelea. Mantener Pixi los 8 minutos de espera gastaría batería, contra lo que pide la sección "UI". "En vivo" se quita en cuanto exista `ending`. Si al recibir los eventos ya pasó `end.t`, no se monta Pixi.
2. **Disposición.** A 375 px: título, escenario o cartelera, `home_lead` debajo, resultado anterior, en una columna, y con los personajes apilados. La cartelera se ve completa sin scroll a 375×667. En escritorio: escenario a la izquierda y panel a la derecha. El `h1` sigue siendo `home_title`, y `home_lead` se queda solo sin pelea en vivo.
3. **Cuenta atrás en `m:ss`,** y con `h:mm:ss` a partir de una hora. Con `Intl` para los dígitos y cifras tabulares.
4. **Commit:** en un `<details>` cerrado, legible y copiable completo, con el rótulo "Semilla sellada" y una línea de explicación. `title` no sirve en un teléfono.
5. **En vivo no se muestra la pelea siguiente.** Reduce el ruido a 375 px, y aparece al terminar.
6. **Con el socket caído a mitad de pelea** el lienzo sigue hasta donde tenga eventos, y el reloj de ronda se oculta. Si llega un `final` visible por el poll, se desmonta y se muestra el resultado.
7. **`halted`:** el lienzo se desmonta si no queda nada que reproducir.

## Desviaciones propuestas de la spec
_El architect aceptó las siete, y están aplicadas en la Enmienda 4 de la spec, salvo la 4, que solo se anota allí._

1. **RF54, por qué sale el resultado.** Texto aceptado: "El resultado se muestra por `snapshot().ending` del renderer o, sin WebGL, por `t ≥ end.t` en texto, nunca antes por llegar `end`. Para la pelea que se está mirando, un final `decided` no adelanta el resultado antes de `t ≥ max_duration_ms`."
2. **RF54, "Cargando".** Con SSR, "hasta el primer `schedule_update`" casi nunca se ve. Propongo definirlo como tablero todo nulo con `halted` nulo, es decir, el runner aún no cargó, y distinguirlo de "iniciando", con `halted` en `false`. Un SSR fallido es el estado de error de RF53.
3. **Preocupación 9.** El e2e tarda, en el peor caso, 5,5 min, no 4, y "en paralelo" exige la configuración aparte de la decisión 16, porque hoy `workers` es 1.
4. **`LiveSession.subscribe` suma `fail`.** Es interfaz de la fase C, no de la spec. Lo anoto porque `plan-c.md` la describe. `live-host.test.ts` cubre `fail` en entrega inmediata.
5. **RNF de sincronización, "reloj_local".** Se precisa como reloj monotónico (decisión 2), con la muestra por `GET` al volver a visible. No cambia la fórmula.
6. **RF57, "comprueba el socket".** La spec no dice cómo, y un navegador no puede enviar un ping. Se precisa con la sonda de `subscribe schedule` (decisión 5). No gasta DP-B7, pero sí un mensaje de los 10 por 10 s, con un máximo de una sonda cada 2 s.
7. **RF54, commit "truncado a 375 px, completo en un atributo".** product-owner lo cambia: `<details>` cerrado, con el commit completo y copiable.

## Preguntas abiertas
1. **Para product-owner:** confirmar el texto final de RF54, antes de implementar.
2. **Para security-reviewer, solo si cambia la decisión 6:** el límite de DP-B7 frente al cliente.

## Estimación de CI
`web` sube unos segundos por los Vitest nuevos. `e2e`, que hoy tarda de 6 min 48 s a 8 min 10 s, sube unos 6 a 7 min: hasta 5,5 min de peleas reales, más `home.spec.ts` en Chromium y WebKit. Quedaría cerca de 15 min, por debajo de los 20 del job. **Criterio:** si alguna de 3 corridas del PR pasa de 16 min, la segunda corrida se mueve a un job propio, en el mismo PR, y `check-workflows` lo exige en su tabla de permisos.

## Desviaciones durante la implementación
1. **`live-host.ts` vive en `lib/components/fight/`, no en `features/fight/live/`,** porque ADR-0009 D4 solo deja a los componentes importar tipos de `features`, y `FightStage` lo usa. `formatClock` pasó a `lib/i18n/format.ts` por la misma razón. Por eso la página compone: `FightHome` recibe todo por props.
2. **`FightStage`:** la prop de los nombres es `displayNames`, y suma `onpainted` y `onfailure`, que el plan no nombraba.
3. **La pelea con `degraded` esperando su primer frame es `live` sin lienzo.** La fila 6 daba `about_to_start` para siempre, porque en ese modo no llegan eventos. Lo encontró el e2e del WS bloqueado, y tiene su fila en `view-state.test.ts`.
4. **`followed-fights.ts` es un módulo aparte,** para que la prueba de DP-B7 use la misma regla que el store. También deja de seguir una mirada con un final que no es `decided`, porque no traerá `end`.
5. **`home.spec.ts` no usa `page.clock`.** Los tiempos son relativos al reloj real, y una pelea puede estar en cualquier `t` desde el inicio. Es igual de determinista, y más simple.
6. **`about_to_start` no se afirma en `fights.spec.ts` (paso A).** Con el primer frame a tiempo, de la cuenta atrás se pasa directo a `live`, así que en una pelea real ese estado puede no verse. Se afirma en `home.spec.ts`.
7. **Precarga de las hojas en `pre_live`, por el criterio de la decisión 14.** Sin ella, con Fast 3G y CPU a 4×, el primer frame llegó en `t = +188 ms`. Con ella, llega antes de `t = 0`.
8. **`fights.spec.ts`:** los contextos usan una sola IP, sin `x-e2e-client`, para probar el límite de upgrades real. El contexto `session` entra por `login`, así que cuenta su socket igual que los demás.

## Revisión
### architect — 2026-10-31 — APPROVE (segunda ronda, con correcciones antes de implementar)
Escrito por architect; lo aplica frontend-engineer, porque esta sesión no puede escribir en este worktree.
- Revisé leyendo el plan y el código, sin compilar ni correr nada, solo: los seis cambios de la primera ronda, los cambios de product-owner tal como los registra este plan, las dos notas de QA y la desviación 7.
  - **Cambios 2 a 6:** resueltos. La decisión 17 cubre flujo, slugs y entrada tarde, y su peor caso cabe en el `globalTimeout`. Los grupos de hasta 4 upgrades quedan bajo la ráfaga de 10. La muestra por `GET` y las decisiones de CI están escritas.
  - **Cambio 1:** resuelto en lo que pedí, pero al revisarlo aparecen las correcciones 1 y 2.
  - **QA, no suscribirse a `next` ni `last`:** correcto contra RF38 y DP-B7, con la corrección 3.
  - **QA, "al día" por `end` o 300 ms sin página:** correcto. No depende de `PAGE_EVENTS` (`hub.rs`), y un cliente lento solo monta el lienzo de más.
  - **Desviación 7, aceptada:** el `<details>` da el commit completo y legible en un teléfono, donde un atributo no se ve. ADR-0002 solo necesita que sea público.
  - **Cambios de producto:** no chocan con ningún ADR. Solo RF54 cambia de texto; el resto son precisiones de este plan.
- Correcciones que el frontend-engineer incorpora antes de implementar:
  1. **"Mirada terminada" corta la pose.** En una decisión o un empate, `end.t = max_duration_ms = 90 000`, y `Settled` llega con `t = end.t + 1500`: la pose duraría 1,5 s, no 10. En `log-ko-clutch` (`end.t = 86 866`) duraría 3,1 s. Un resultado con `end` tampoco necesita `final`. Reemplazar por: "**Mirada terminada:** se cumple una de estas: tiene `end` y `t ≥ end.t + 10 s`; al estar al día ya había pasado `end.t` (decisión 7, sin Pixi); no tiene `end` y tiene `final` visible; o tiene `final` visible que no es `decided`." El segundo caso también hace falta: sin él, quien entra con `end.t` ya pasado y `current` aún en `live` nunca tiene `painted`, y queda en `about_to_start` en la fila 6. Filas nuevas en `view-state.test.ts`: empate con `Settled` durante la pose, `log-ko-clutch` en `t = 90 000`, y entrada con `end.t` ya pasado y `current` en `live`.
  2. **`painted` no contempla el modo texto.** Sin WebGL nunca hay primer frame, así que la fila 6 daría `about_to_start` durante toda la pelea, también en WebKit de CI (preocupación 10). Definir: "`painted`: primer frame pintado, o modo texto activo (`webgl_unavailable`, 2 pérdidas de contexto, rechazo o `fail`)". Con una fila en `view-state.test.ts` y un caso en `home.spec.ts`.
  3. **Reconexión con una mirada sin terminar.** Si la pelea mirada ya es `last` y el socket se cae antes de `end`, "un único canal, el de `current`" la deja sin eventos, contra "al reconectar se resuscribe" (RF57). Sumar a la decisión 5: "Un socket nuevo se suscribe también a la pelea mirada que aún no tiene `end`". En la decisión 6, la prueba pasa a "nunca más de 2 en un socket nuevo (`current` y esa mirada), y 1 en los demás casos", bajo el límite de 3. Sigue sin nombrar a `next` ni a un `last` no mirado.
  4. **`pre_live`.** "Se monta oculto" significa sin `display: none`: con tamaño 0, el lienzo no da una medida válida y el primer frame no significa nada. Un test comprueba que el contenedor tiene tamaño. El caso de red limitada de `home.spec.ts` corre sin `page.clock`, porque con el reloj falso "antes de `t = 0`" no mide nada.
  5. **Decisión 16, logs:** los vacía `serve-with-log.mjs` al arrancar; `reset-run.mjs` solo limpia `egress.log` y los archivos de corrida. El efecto que anota el plan es el mismo.
- **La Enmienda 4 ya está lista,** y esta aprobación no la cubre. Según lo registrado en este plan a partir del resumen de quien coordina, product-owner ya confirmó RF50 y el RNF de CPU como aproximación, y aceptó las desviaciones 1 y 2. Falta su confirmación del texto final de RF54 antes del PR. No hay nada para security-reviewer mientras no cambie el límite de DP-B7.

### architect — 2026-10-31 — REQUEST_CHANGES
- Verifiqué leyendo el código de `557cf26`, sin compilar: `session.rs` comprueba el canal antes de gastar DP-B7; `playwright.config.ts` tiene `workers: 1`; vectores de `golden.rs` y `derive_seed` correctos; `too_few_characters` no activa `halted`; `_e2e-run` y `e2e-server` usan ambas features; rejilla de 113 s válida; 5,5 min = 113+113+8+90+5 s; el job `e2e` tarda de 6 min 48 s a 8 min 10 s. Notas de D de los planes B y C con decisión o prueba, salvo los cambios 1 y 5. Las decisiones de producto y diseño no chocan con la spec.
- Decisiones:
  1. La segunda corrida va en el job `e2e` actual, con `globalTimeout` de 8 min y `retries: 0` en `playwright.fights.config.ts`. Se mueve a un job propio, en el mismo PR, si alguna de 3 corridas del PR pasa de 16 min.
  2. La Enmienda 4 la escribe el architect en esta rama cuando el plan quede aprobado y antes de implementar; recoge la de `plan-c.md` y las desviaciones aceptadas. product-owner confirma RF50, el RNF de CPU y RF54 antes del PR.
  3. Desviaciones aceptadas: 1 (RF54: "El resultado se muestra por `snapshot().ending` del renderer o, sin WebGL, por `t ≥ end.t` en texto, nunca antes por llegar `end`. Para la pelea que se está mirando, un final `decided` no adelanta el resultado antes de `t ≥ max_duration_ms`."); 2; 3 ("en el peor caso"); 4 (con `live-host.test.ts` cubriendo `fail` en entrega inmediata); 5 (con el cambio 5); 6 (máximo una sonda cada 2 s).
- Cambios pedidos:
  1. **Tabla de `view-state`.** (a) Con `current` en `live` y `t < 0` se cae a la fila 7 y se ve la cuenta atrás de la siguiente; sin `next` ni `last`, ninguna fila aplica. La pelea por venir es `current` mientras `serverNow < live_started_at + delay`. (b) `watched` no aparece en ninguna condición: `Settled` llega en `end + 5000` y `current` cambia, así que un cliente atrasado o la pose caen a la fila 7 y se desmonta el lienzo. Hace falta una fila antes de la 6: "la pelea mirada no terminó, con su pose incluida, da `live`". Di también qué se ve si termina la pose con la liquidación atrasada: el resultado sale de los eventos. (c) Una mirada que pasa a `voided` no trae `end` y queda en vivo para siempre; igual con `ws.forbidden_channel`. Regla: `final` se muestra de inmediato si no es `decided`, y si es `decided`, desde `t ≥ max_duration_ms`. Una fila por caso en `view-state.test.ts` y un caso en `home.spec.ts`. (d) RF54 mantiene `last` en la fila 6 hasta que la siguiente esté en vivo.
  2. **`fights.spec.ts`:** tabla con qué pelea mira cada aserción, en qué contexto e idioma, con un solo flujo por proyecto y contextos simultáneos; `test.setTimeout`; qué pasa si se entra con una pelea empezada. La pelea se identifica por slugs, no por orden (RF45 cuenta filas previas).
  3. **Ráfaga de upgrades:** `WS_ANON_UPGRADE_IP` deja pasar 10 por IP y el e2e sale de una IP; el rechazo cuenta como fallo de RF57 y falsea aserciones. Cuenta los sockets, escalona las entradas sin subir el límite y comprueba que `degraded` nunca aparece en esa corrida.
  4. **`_e2e-run`** repite también `reset-run.mjs` entre corridas, no solo `db.mjs reset`. Mismo `globalTeardown`, `outputDir` propio. Anota que CI solo sube los logs de la última corrida y que un fallo en la primera se conserva porque `set -e` detiene la segunda.
  5. **La sonda no trae hora.** Al volver a visible con el socket vivo, toma una muestra con `GET /api/fights/current`. Test en `clock-sync` o `fight-socket`.
  6. Actualiza el plan con estas decisiones: preguntas abiertas 1 y 2, fila `plan-c.md` → `spec.md` del cierre, desviación 1, y estimación de CI con el criterio de 16 min.

### product-owner — 2026-10-31 — REQUEST_CHANGES
Registrado por frontend-engineer a partir del resumen de quien coordina.
- **Confirmó** RF50, y el RNF de CPU solo como aproximación: el prerrequisito 8 sigue abierto. **Aceptó** las desviaciones 1 y 2.
- **Cambios pedidos:**
  1. **Pose:** quitar "En vivo" en cuanto exista `ending`; la pose dura hasta `t ≥ end.t + 10 s`. Si al recibir los eventos ya pasó `end.t`, no se monta Pixi: tarjeta de resultado.
  2. **`pre_live`:** cartelera y cuenta atrás visibles hasta el primer frame pintado, para evitar unos 30 s de lienzo vacío. Comprobar con red móvil limitada que llega a tiempo; si no, precargar las hojas en `pre_live`.
  3. **Commit:** en un `<details>` cerrado, legible y copiable completo.
  4. **Degradado:** ocultar el reloj de ronda; el primer poll sale al entrar en ese modo.
  5. **375 px:** cartelera completa sin scroll a 375×667, `home_lead` debajo, personajes apilados, cuenta atrás con cifras tabulares.
  6. **Textos:** los de "Textos", más separador, etiqueta del reloj, HP en modo texto y aviso de error.
  7. **Recomendaciones aceptadas:** `halted` y `degraded` en `role="status"`; desmontar el lienzo con `halted` si no hay más que reproducir; capturas con el epíteto más largo y los placeholders a 375 px, en es y en.
