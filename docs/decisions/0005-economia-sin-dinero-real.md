---
id: "0005"
title: Economía de monedas sin dinero real
type: adr
status: approved
risk: high
author: owner
approvals:
  - by: owner
    date: 2026-10-20
created: 2026-10-20
updated: 2026-10-21
---

# ADR-0005: Economía de monedas sin dinero real

## Decisión
- **Monedas ficticias** (`Coins`): no se compran con dinero real y no se retiran. Se obtienen con el bono diario, el rescate y las ganancias de apuestas.
- **Apuestas parimutuel**: las ganancias se reparten entre los ganadores en proporción a lo que apostó cada uno. La casa no arriesga nada. Puede haber una comisión (`rake`) que se destruye, como sumidero contra la inflación.
- **Ledger de doble entrada**: cada movimiento es un asiento inmutable (grant, bailout, bet_stake, bet_payout, refund, rake…). Los saldos se derivan del ledger, nunca se editan directamente. Las liquidaciones ocurren en una sola transacción e idempotente.
- Cualquier cambio a esta regla requiere un nuevo ADR y revisión legal.

## Por qué
Comprar monedas con dinero real para apostar entra en regulación de juegos de azar o "social casino" (en México, la Ley Federal de Juegos y Sorteos) y choca con las políticas de las tiendas de apps y los procesadores de pago.

## Consecuencias
- El security-reviewer revisa todo cambio que toque el ledger, las apuestas o la liquidación.
- Leaderboard de apostadores por **ganancia neta**: excluye bonos y rescates.
- Nota de trazabilidad (2026-10-21, con OK del owner): el leaderboard de temporada ordena por `RankedProfit`, la parte de la ganancia neta que viene de apuestas de cuentas fuera de carencia. Es un refinamiento de la regla anterior, no un cambio; ver ADR-0008 D8.
