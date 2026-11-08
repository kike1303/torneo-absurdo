---
id: "012"
title: "Apuestas, fase C: panel en la home, form actions, e2e y cierre"
type: plan
status: in-review
risk: high
author: frontend-engineer
reviewers: [reviewer, security-reviewer]
approvals: []
linear: TA-27
depends_on: ["008", "010"]
created: 2026-11-08
updated: 2026-11-08
---

# Plan: Apuestas, fase C (desde spec.md 2026-11-02)

> [intent](./intent.md) · [spec](./spec.md) · [plan-a](./plan-a.md) · [plan-b](./plan-b.md) · ADRs: [0005](../../decisions/0005-economia-sin-dinero-real.md), [0007](../../decisions/0007-i18n-desde-cero.md), [0008](../../decisions/0008-autenticacion.md) D5, [0009](../../decisions/0009-ui-design-system.md) · 010: RF53, RF54, RF59

**Alcance.** RF41 a RF46 y RF48, la parte web de RF36 y RF47, el trabajo de cierre de la spec, y las pruebas que security-reviewer pidió antes de C (abajo, paso 0).

**Punto de partida.** `main` en `0de3d75`, con B fusionada (PR #112), en la rama `ta-27-apuestas-c`. La API, los tipos y los códigos ya existen: C no cambia `crates/protocol` salvo que el paso 0 lo pida.

**Gate.** OK del owner al criterio en local: apostar con dos cuentas, ver pago y reembolso, bono y rescate en `just dev`. Sin ese OK en chat no se fusiona, aunque las revisiones aprueben.

## Decisiones de este plan
| # | Tema | Decisión |
|---|---|---|
| C1 | Pelea objetivo y lecturas de `load` | `targetOf(board)` es puro y lo usan `load` y el panel: `current` si está en `betting_open`, si no `next` si lo está. `load` lee, solo con sesión y en paralelo, `wallet` y `bets/mine` de la objetivo, de la `current` si es otra, y de `last`. Gasta como mucho 4 de los 60/min por usuario (D6 de plan-b). Un fallo deja ese dato en `null`, nunca rompe la página |
| C2 | `client_key` | La genera `load` con `crypto.randomUUID()` (minúsculas y guiones, lo que el server acepta) y va en un campo oculto. Una recarga da otra; reenviar el formulario sin recargar repite la misma, y el server responde 200 con la apuesta ya hecha (RF25 fila 3). Tras un 201, `use:enhance` invalida y la página trae una llave nueva |
| C3 | Pozo en el cliente | El SSR lee `GET /api/fights/{id}/pool` de la objetivo; después manda `pool_update`, que `FightStore` guarda por `fight_id` (el último valor, sin cola). El panel toma el más nuevo por `server_time` |
| C4 | `quote` | El navegador llama a `GET /api/fights/{id}/quote` directo, como ya hace con `/api/fights/current`: es lectura con la cookie de sesión. `quote-client.ts` es puro salvo `fetch` y el temporizador inyectados: 300 ms de espera, descarta respuestas de otra entrada, un `pool_update` la marca desactualizada sin pedir otra, y error, 429 o sin JS dan "no disponible" |
| C5 | Relectura del pago (RF36) | `reread.ts`: cuando `view.result` es de una pelea en la que el usuario apostó (según `mine` de `last` o de la `current`), una sola vez por pelea y tras 0 a 3 s al azar, `invalidate('app:bets')`. `load` declara `depends('app:bets')`, y el layout relee `me`, así que el header cambia con lo mismo. Hasta ese momento el panel muestra lo que trajo el SSR, que no tiene `result` si la vista aún no lo deja ver (RF45: el panel no muestra `mine.result` de una pelea cuyo resultado no está en la vista) |
| C6 | Dinero en la UI (RF43) | Un solo archivo, `lib/features/bets/presentation.ts`, divide `payout_bp` entre 10 000 para el multiplicador y saca la proporción del pozo. Ningún otro archivo de `lib/features/bets/` ni `BetPanel` usa `+ - * / %` sobre campos de monedas: lo comprueba un test de fuente |
| C7 | Estados (RF42) | `panelState(input)` puro en `lib/features/bets/panel-state.ts`, con las tres zonas (bono, resultado, pelea) y el rechazo. Los componentes solo dibujan lo que dice |
| C8 | e2e de apuestas | Una tercera corrida, `playwright.bets.config.ts`, con `dev-fights`, ventana de 45 s y cadencia de 153 s (la mínima de RF11 de 010 con esa ventana), el KO y el empate de `fight-script.mjs`. Va en `just e2e` y, como `e2e-fights`, no en CI (TA-56): monta el escenario 3D. Dos contextos por proyecto, uno por lado |
| C9 | Sin JS | Los tres formularios son `<form method="post">` con acción; `use:enhance` solo mejora. La estimación no existe sin JS ("no disponible") |

## Archivos que cambian
| Archivo | Cambio | Paso |
|---|---|---|
| `apps/server/tests/{bets_settle,bets_claims,fights_no_leaks}.rs`, `apps/server/src/bets/facts.rs` | Pruebas pendientes de B y el fragmento compartido de carencia | 0 |
| `apps/web/src/lib/server/api/{reads,mutations}.ts` | `getWallet`, `getMyBets`, `getPool`; `postBet`, `postBonusClaim`, `postBailoutClaim` | 1 |
| `apps/web/src/lib/features/bets/{target,panel-state,presentation,quote-client,reread,form}.ts` y tests | Nuevos, puros | 2 |
| `apps/web/src/lib/features/fight/live/live-store.svelte.ts` y test | Guarda `pool_update` | 3 |
| `apps/web/src/routes/+page.server.ts`, `+page.svelte` | `load` con C1 y C2, y las tres acciones | 4 |
| `apps/web/src/lib/components/bets/{BetPanel,BetForm,BetPool}.svelte`, `presentation.ts`, `apps/web/src/lib/components/fight/FightHome.svelte` | El panel en la segunda zona (E2, E3) | 5 |
| `apps/web/messages/{en,es}.json` | Textos del panel | 5 |
| `apps/web/playwright.bets.config.ts`, `apps/web/e2e/bets.spec.ts`, `justfile` | C8 | 6 |
| `docs/glossary.md`, `docs/features/010-*/spec.md`, `CLAUDE.md`, `REVIEW.md`, `apps/server/.env.example` | Trabajo de cierre de la spec | 7 |

## Orden de trabajo
### 0. Pruebas pendientes de B
- RF14: un gancho más lento que `attempt_timeout` de prueba: el intento se aborta, no queda nada escrito, y el siguiente intento liquida.
- Apuestas y liquidación en paralelo: la liquidación de una pelea mientras otros usuarios apuestan a la siguiente y reclaman el rescate, sin `40P01` ni bloqueo de más de `lock_timeout`.
- `fights_no_leaks` con apuestas sembradas: el pozo, el `quote` y `bets/mine` no cambian entre dos semillas.
- Nit del reviewer: `probation_of` y `FACTS_SQL` comparten el fragmento de edad y peleas.

### 1. Lecturas y mutaciones
Las seis funciones, con el `Api` de siempre. Las mutaciones solo en `actions` (test `no-mutation-in-load`).

### 2. Funciones puras
`target.ts` (C1), `panel-state.ts` (C7: una fila de Vitest por estado y por código de RF42, `balance < min_bet` con cada `bailout` y `balance ≥ min_bet` en el tope), `presentation.ts` (C6), `quote-client.ts` (C4: desactualizada, fuera de orden, no disponible, mínimo como ayuda, ninguna petición por `pool_update`), `reread.ts` (C5) y `form.ts` (lee y valida el `FormData`, sin calcular dinero).

### 3. Pozo en el store
`pool_update` se guarda; la vista no cambia por él (sigue el test de B, con otra aserción).

### 4. `load` y acciones
`?/bet`, `?/claimBonus` y `?/claimBailout`. Un 401 redirige a entrar; los demás fallos dan `fail(status, { code, amount, side })`, con el importe conservado. Sin sesión, el panel solo muestra el pozo y el enlace a entrar que vuelve a `/`.

### 5. Panel
Una columna a 375 px y debajo del escenario; en escritorio, la segunda zona de `FightHome`, sobre la tarjeta de resultado. Objetivos de 44 px, tokens del theme, `aria-live` para el resultado, errores asociados al campo. La explicación (RF48) es un `<details>` dentro del panel. Capturas a 375 px y 1280 px, claro y oscuro.

### 6. e2e
Por proyecto: dos cuentas en lados opuestos del KO (pago y pérdida, con el header), el empate (reembolso), un lado vacío (reembolso y su aviso), `bet.side_locked` y `bet.fight_cap_exceeded` con el importe conservado, carencia con su texto, bono con saldo, rescate, estimación con su texto, recarga a mitad de pelea, y sin JS reenviando el formulario. En y es, a 375 px y en escritorio.

### 7. Cierre
Lo que lista "Trabajo de cierre (fase C)" de la spec y aún falta tras B. Linear: comentario en TA-28 y TA-13, y la revisión de los 90 días de RF40 con fecha.

## Riesgos
- **Tiempo del e2e:** dos peleas de 153 s más espera: unos 6 min. Corre aparte y solo en local.
- **RF45:** el SSR trae `mine.result` de `last` aunque la vista lo retenga. Por eso el panel filtra por `view.result` (C5), y un e2e con cliente atrasado lo comprueba.
- **Cuota de lectura:** cada `invalidate` cuesta 1 a 4 lecturas. La relectura es una por pelea, y una acción, otra.

## Desviaciones durante la implementación
| # | Plan | Lo que se hizo | Por qué |
|---|---|---|---|
| E1 | C5: `invalidate('app:bets')` y `depends` | `refreshAll()` de Kit | El header lee el saldo del layout: una invalidación solo de la página lo dejaba viejo. Cuesta además la lectura del tablero, que es de memoria |
| E2 | Componentes en `lib/components/fight/` | `lib/components/bets/` | CLAUDE.md: los componentes de dominio van en `lib/components/<dominio>/` |
| E3 | C6: `presentation.ts` en `lib/features/bets/` | En `lib/components/bets/`, con `lostAmount` y `rakeShare` | ADR-0009 D4: un componente solo importa tipos de `features/`. Sigue siendo el único archivo con aritmética, y el test de RF43 lo comprueba |
| E4 | C1: releer al abrirse otra ventana | `load` devuelve `target`, y la página relee si la objetivo cambió | Las apuestas leídas antes de abrirse una ventana tienen `max_stake = 0`. Lo encontró el e2e |
| E5 | C2: la llave cambia con cada lectura | La página guarda la llave y solo toma la nueva tras una apuesta hecha o "Recargar" | Un rechazo relee `wallet` y `mine`, y una llave nueva rompería el reintento de `bet.unavailable` |
| E6 | RF45 e2e con cliente atrasado | Vitest de `panelState` (solo muestra el resultado que la vista muestra) y la relectura, que espera a la vista | Atrasar un cliente real exige cortar su socket a medida. Queda para cuando 010 tenga ese soporte en el e2e |
| E7 | Bono e2e | Un usuario sembrado con el bono de ayer (`seedYesterdayUser`) | El alta ya da el bono del día, así que una cuenta nueva nunca lo ve disponible |
| E8 | Ventana de 45 s, cadencia de 153 s | 40 s y 148 s, tres peleas (KO, empate, KO) | Un empate gana a un lado vacío como motivo de reembolso: el lado vacío pide una tercera pelea |
| E9 | Paso 0: carencia en un fragmento | `concat!` con dos macros en `facts.rs` | Una constante no puede formar parte de otra en `concat!` |
