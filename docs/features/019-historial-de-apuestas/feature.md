---
id: "019"
title: Historial de apuestas del usuario
type: feature
status: done
risk: high
author: claude
linear: TA-45
depends_on: ["012"]
created: 2026-11-08
updated: 2026-11-08
---

# Historial de apuestas del usuario

## Qué y por qué
La feature 012 solo muestra la apuesta de la pelea actual y la de la última. Quien vuelve horas después ve su saldo cambiado sin saber por qué. Esta feature agrega una página con sus apuestas pasadas: a qué pelea, a qué lado, cuánto puso y qué pasó (ganó, perdió o se le devolvió, y por qué). Es solo lectura: no mueve monedas ni cambia nada del ledger.

## Alcance
- Entra: `GET /api/bets/history`, sus tipos en `crates/protocol`, la página `/[locale]/bets` con paginación y un enlace a ella desde la cuenta y desde el panel de apuestas.
- No entra: bonos y rescates en la lista (el usuario los reclama a mano y ya ve su efecto), `ranked_profit` (lo decide TA-28), totales o estadísticas, filtros, exportar.

## Diseño
**Protocolo (`crates/protocol/src/bets.rs`).**
- `BetHistoryItem { fight_id: u32, a: String, b: String, side: Side, staked: CoinAmount, bets: u32, placed_at: TimestampMs, settled: bool, result: Option<MyFightResult> }`. `a` y `b` son los slugs; `placed_at` es la última apuesta del usuario en esa pelea. `result` reutiliza el de RF33 de 012 y solo existe si la pelea está liquidada.
- `BetHistoryPage { items: Vec<BetHistoryItem>, next_before: Option<u32> }`, igual que `FightsPage` (011 RF7).

**Server (`apps/server/src/bets/history.rs`).**
- `GET /api/bets/history?before=<id>`: con sesión, solo del usuario de la sesión y sin parámetro de usuario. Comparte a propósito el bucket de `wallet` y `bets/mine` (`WALLET_READ_IP` y `_USER`, 60/min por usuario): es una lectura más del wallet, y quien pagina rápido gasta la misma cuota que el panel. La misma caché (`private, no-store`, `Vary: Cookie`). Ruta en el manifiesto con `Effect::Read`.
- Una fila por pelea en la que el usuario apostó, del id más alto al más bajo, 20 por página. Una sola consulta parametrizada, con `statement_timeout` de 2 s: primero las 21 `fight_id` distintas por `bets_user_fight_idx` en orden descendente, y solo para esas el agregado (`LATERAL`) de sus `bets`, la suma de sus `bet_settlements` unida por `bet_id` (la llave atada al dueño, no `(user_id, fight_id)`) y `pool_settlements.outcome` para la razón del reembolso. Así una página nunca recorre todo el historial. La fila 21 solo dice si hay otra página.
- `before` usa el parser canónico `fights::canonical_id`. Sin `before`, desde arriba (`i32::MAX + 1` como `bigint`, como `settled_page`). Uno no canónico o repetido da 404 `fight.not_found`, como RF33. Uno bien escrito es solo un cursor: nunca se comprueba si la pelea existe ni quién apostó en ella, así que no hay oráculo de "¿alguien apostó aquí?".
- `settled` es `fights.state = 'settled'` (también las anuladas y canceladas). `result` solo con `settled`: `bet_settlements` se escribe en la misma transacción que liquida la pelea (RF18 de 012), así que no puede aparecer el ganador antes de que sea pública, y el estado es una segunda barrera. Una pelea `settled` con apuestas del usuario y sin filas en `bet_settlements` rompe esa invariante: la página entera da 503 `bet.unavailable`, con un log de solo `event` y `error_kind`, nunca `result: null` en silencio.
- Sin apuestas: `items` vacío y 200.

**Web (`apps/web`).**
- `routes/bets/+page.server.ts` (el locale lo pone `reroute`): sin sesión redirige al login con el `next` fijo `/${locale}/bets`, sin la query, como `/account`; con la sesión caída, estado `unavailable`. `before` pasa por `parseFightId` y llega a `getBetHistory(api, before)` (`lib/server/api/reads.ts`) solo como número. La página lleva `private, no-store` con un `setHeaders` explícito. Un fallo de la API es un 200 con el estado de error.
- `lib/features/bets/history.ts` (puro, con tests): convierte un item en una fila de vista (`won` / `lost` / `refunded` / `pending`, el neto con signo y la razón del reembolso).
- `lib/components/bets/BetHistory.svelte`: lista de tarjetas enlazadas a la repetición (`/fights/{id}`), con los nombres de los luchadores, el lado elegido, lo apostado, el resultado y la fecha (`Intl` vía los helpers existentes). Estados vacío, error y "cargar más" como `FightList`. Mobile-first, objetivos de 44 px, solo tokens del theme y el resultado no depende solo del color (lleva texto).
- Textos nuevos en `messages/es.json` y `en.json` con prefijo `bet_history_`.

## Riesgos
- **Fuga entre usuarios.** El `user_id` sale solo de la sesión; un test de integración prueba que B no ve nada de A, ni siquiera paginando con `before`.
- **Revelar el resultado antes de tiempo.** `result` exige `state = 'settled'`; un test con una pelea `live` con apuestas no trae `result`. El lado ganador no viaja por fuera de `result`.
- **Carga.** Una página son 21 grupos sobre el índice `bets_user_fight_idx (user_id, fight_id)`, con las cuotas de lectura del wallet.
- **Datos personales.** No se agrega ningún dato nuevo; nada de esto va a los logs más que el `event` y el `error_kind`.

## Pruebas
- [ ] `just check` en verde.
- [ ] Protocolo: la forma JSON exacta de `BetHistoryPage`.
- [ ] Integración (`apps/server/tests/bets_history.rs`): sin sesión 401; sin apuestas lista vacía; orden y paginación con 21+ peleas; ganada, perdida y reembolsada con sus montos; pelea `live` sin `result` aunque haya filas de `bet_settlements`; `settled` sin resultado da 503; aislamiento entre dos usuarios, también con `before` en una pelea del otro o inexistente (200); `before` no canónico o repetido 404; cabeceras `private, no-store`. Además entra en `fights_no_leaks` y en el test de logs de 012.
- [ ] Vitest de `history.ts` y del `load`.
- [ ] Capturas a 375 px y en escritorio, tema claro y oscuro.
