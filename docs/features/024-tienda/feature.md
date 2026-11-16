---
id: "024"
title: Tienda de cosméticos
type: feature
status: draft
risk: high
author: claude
linear: TA-68
depends_on: ["012", "021", "022"]
created: 2026-11-16
updated: 2026-11-16
---

# Tienda de cosméticos

## Qué y por qué
Hoy los accesorios y las animaciones de una botarga solo salen en los sorteos por nivel, y nada de lo que se gana se puede gastar fuera de las apuestas. Con la tienda, el jugador compra directo lo que quiere con sus monedas: botargas completas como traje, animaciones de reposo y de victoria, y accesorios. Es el paso 6 del [backlog](../../backlog.md) y el primer sumidero de monedas mientras el rake siga en 0. Nada de dinero real (ADR-0005).

## Decisiones del owner (chat, 2026-11-16)
- **Moneda:** solo la de apuestas. La segunda moneda queda para después.
- **Se vende:**
  - **botargas completas como traje**, tanto disfraces de los personajes del torneo como diseños nuevos, solo como aspecto: no traen stats ni especiales;
  - **animaciones** de reposo y de victoria;
  - **accesorios con sus stats**, que son de suma cero: cambian el perfil y no dan ventaja neta.
- **Gratis:** los colores de la botarga.
- **Precios:** los propuso el game-designer y el owner los aprobó tal cual.
- **Requisito de nivel:** un accesorio solo se vende si el tier al que pertenece ya está al alcance del nivel del jugador, igual que en el sorteo. No hay otras reglas de protección en la v1: ni saldo mínimo, ni corte del rescate, ni carencia.
- **Sin cajas sorpresa:** cada artículo se compra directo. Tampoco hay regalos ni reventa: lo comprado no sale de la cuenta.

## Alcance
- **Entra:**
  - el catálogo de la tienda en `content/catalog/shop.json`;
  - la compra idempotente, con su asiento del ledger y su desbloqueo;
  - la lectura de la tienda;
  - el traje (`skin`) como parte equipable de la build y del `look` de las peleas;
  - la página `/shop` con vista previa 3D sobre mi botarga;
  - equipar el traje en `/botarga`.
- **No entra:**
  - la segunda moneda y cualquier pago;
  - los tipos de cuerpo;
  - regalos e intercambios;
  - el arte de cada botarga. Publicar cada traje en `content/models/skins/` es trabajo de arte que revisa el owner (un issue por tanda), y la tienda lista solo los trajes que ya están en `content/`.
  - `tournament_suit`, que hay que partir en dos accesorios (TA-91).

## Precios (aprobados, 2026-11-16)
| Artículo | Precio |
|---|---|
| Animación de reposo (`idle` a `idle_5`) | 400 |
| Animación de victoria (`win_1` a `win_3`) | 750 |
| Accesorio de tier 1 / 2 / 3 / 4 / 5 | 750 / 1000 / 1500 / 2000 / 2500 |
| Botarga de diseño nuevo | 5000 |
| Botarga de personaje del torneo | 7500 |

El precio de un accesorio va por tier, no por cuánto mueve sus stats: son de suma cero, y cobrar más por un ±10 diría que da más poder. El precio de cada artículo es un dato del catálogo. Cambiarlo no reembolsa lo que ya se compró.

## Diseño
**Catálogo.** `content/catalog/shop.json` tiene una lista de artículos `{ id, kind, price }`:
- `kind` es `accessory`, `cosmetic` (un clip) o `skin`;
- `id` es el del catálogo de accesorios, el código del clip o el slug del traje en `content/models/skins/`.

`crates/content` lo carga y lo valida al arrancar:
- cada `id` existe;
- no hay repetidos;
- `price` está entre 1 y 1 000 000;
- un `skin` tiene su carpeta.

Los nombres salen de `names.{es,en}.json`. Para los trajes se agrega una sección `skins`.

**Datos** (migración aditiva, códigos SQLSTATE fijos):
- **Ledger:**
  - una cuenta `shop_sink`, creada por la migración (`SHOP_SINK_ACCOUNT`);
  - un asiento `shop_purchase`, del usuario (−precio) al sumidero (+precio).
  
  El sumidero nunca devuelve monedas, como `rake_sink`.
- **Compras:** `shop_purchases (id, user_id, kind, item_id, price, ledger_entry_id, client_key, created_at)`, inmutable (guard). Tiene `UNIQUE (user_id, kind, item_id)`, porque nada se compra dos veces, y `UNIQUE (user_id, client_key)`.
- **Inventario:** `botarga_unlocks` acepta `kind = 'skin'` y `source = 'shop'`. Lo comprado entra al inventario como cualquier desbloqueo. Un sorteo nunca ofrece lo que ya se tiene, así que ese sorteo da otra cosa.
- **Build:** `botargas.skin text NULL` es el traje equipado. `null` es la botarga base.
- **Auditoría:** un `auth_events` de tipo `purchase` por compra, con su `ledger_entry_id` y el `ip_hash`, como `bet` y `grant`.

**Compra:** `POST /api/shop/buy { kind, item_id, client_key }`, con sesión, `Effect::Mutate`, `Origin` y la cuota `SHOP_USER`. En una transacción:
1. `lock_user` (exige `banned_at IS NULL`) y la botarga, con un `lock_timeout` de 1 s.
2. Si el `client_key` ya existe, se devuelve esa compra con 200, aunque el precio haya cambiado.
3. Se valida el artículo:
   - 404 `shop.not_found` si no está en el catálogo;
   - 409 `shop.owned` si ya lo tengo (comprado o ganado);
   - 422 `shop.level` si es un accesorio de un tier fuera de mi nivel.
4. El saldo se lee bajo el lock, y si no alcanza se responde 402 `shop.insufficient`.
5. Se escriben el asiento, la fila de `shop_purchases`, el desbloqueo y el evento.

La respuesta trae la compra, el saldo nuevo y la `BotargaView`. Los errores de base de datos y el `lock_timeout` siguen el patrón de amigos y PvP (`shop.busy` y `shop.unavailable`).

**Lectura:** `GET /api/shop`, con sesión. Devuelve:
- los artículos con su precio;
- si ya lo tengo;
- si mi nivel lo permite;
- mi saldo.

**Traje:**
- `BotargaBuild.skin: Option<String>`; `PUT /api/botarga/build` exige que esté desbloqueado (`botarga.skin_locked`).
- `PvpLook.skin` lleva el traje a las peleas, a los retos de amigos y a sus repeticiones.

El motor no cambia: el traje es solo cosmético y no entra en el `CharacterSpec`. El renderer ya sabe ponerlo (022): la web arma el `look` con el GLB de `content/models/skins/<slug>/`.

**Web:**
- `/shop`: los artículos agrupados por tipo, con el precio, "lo tienes" y "nivel N". Tiene una vista previa 3D sobre mi botarga con el artículo puesto, el saldo y una confirmación ("comprar X por N monedas") antes del `POST`, que va en una form action con su `client_key`.
- `/botarga`: elegir el traje entre los que tengo.

**Protocolo:**
- tipos nuevos: `ShopView`, `ShopItem`, `ShopItemKind`, `ShopBuyRequest`, `ShopPurchase` y `ShopErrorCode`;
- campos nuevos: `skin` en `BotargaBuild`, en `BotargaUnlocks` y en `PvpLook`.

## Fases
1. Server A: migración, catálogo, `GET /api/shop`, `POST /api/shop/buy`, la conservación del ledger y la auditoría.
2. Server B y web: `skin` en la build y en el `look`, `/shop` con la vista previa y el traje en `/botarga`, con su e2e.
3. Contenido: los trajes de la primera tanda que apruebe el owner, cada uno con `skin.json` (fit y zonas) y `provenance`, en sus propios PR de contenido.

## Riesgos
- **Doble cobro.** Lo evitan la idempotencia por `client_key`, la unicidad `(user, kind, item)` y el lock del usuario. Un test concurrente compra lo mismo dos veces a la vez y comprueba que hay un solo asiento.
- **Saldo negativo.** El saldo se lee y se descuenta bajo el lock del usuario, en la misma transacción. Un test comprueba que el ledger se conserva: la suma de todas las cuentas sigue en cero con compras.
- **Inflación.** La tienda absorbe monedas, pero cada artículo se compra una sola vez, así que el sumidero se agota para quien ya lo tiene todo. No reemplaza la revisión del rake (TA-59). Revisarlo en D30 con tres medidas: qué parte de lo emitido absorbe la tienda, el saldo mediano y cuántos se quedan sin saldo.
- **Granjas.** Lo comprado no se transfiere, así que una granja solo viste a su dueño. El daño es cosmético. Se vigila con `farm-report`.
- **Contenido.** Los disfraces del torneo son contenido propio (`LICENSE-CONTENT.md`). Cada traje nuevo pasa la lista de "¿demasiado parecido?" de `write-character` antes de publicarse.

## Pruebas
- [ ] `just check` en verde
- [ ] Integración:
  - [ ] una compra mueve monedas solo por un asiento y desbloquea el artículo;
  - [ ] el ledger se conserva;
  - [ ] repetir el `client_key` devuelve la misma compra;
  - [ ] comprar algo que ya tengo da 409 y no escribe nada;
  - [ ] un saldo insuficiente da 402 y no escribe nada;
  - [ ] un accesorio fuera de tier da 422;
  - [ ] un artículo inexistente da 404;
  - [ ] dos compras concurrentes de lo mismo dejan un solo asiento.
- [ ] `banned_writes.rs` y `fights_no_leaks.rs` cubren las rutas nuevas.
- [ ] Un traje no desbloqueado no se puede equipar. Uno equipado viaja en el `look` de la pelea y en su repetición.
- [ ] e2e: comprar una animación y un accesorio, ver el saldo bajar, equiparlos y verlos en la vista previa y en una pelea. Capturas a 375 px y en escritorio, claro y oscuro.
