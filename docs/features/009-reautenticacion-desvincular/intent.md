---
id: "009"
title: Reautenticación para desvincular y para admin
type: intent
status: done
risk: high
author: product-owner
reviewers: [architect, security-reviewer]
approvals:
  - by: security-reviewer
    date: 2026-10-30
  - by: architect
    date: 2026-10-30
linear: TA-29
depends_on: ["008"]
created: 2026-10-30
updated: 2026-10-30
---

# Intent: Reautenticación para desvincular y para admin

## Problema
security-reviewer lo encontró al revisar la feature 008, y está en TA-29. Con una sesión robada o un navegador desatendido, un atacante puede **quedarse con la cuenta para siempre**:

1. Vincula su propio proveedor a la cuenta de la víctima. Vincular rota la sesión, y la sesión nueva sale con `authenticated_at = now()`, así que cuenta como reautenticación.
2. Desvincula el proveedor de la víctima. ADR-0008 D1 solo exige que quede otro proveedor, y el del atacante cumple.
3. Desvincular revoca las demás sesiones (D10), así que la víctima queda fuera. No tiene forma de volver a entrar, porque no hay recuperación por email (ADR-0008, Consecuencias).

La víctima pierde su alias, su saldo de `Coins` y su historial. Las monedas son ficticias, pero perder la cuenta es exactamente lo que hace que alguien no regrese mañana. Con una sesión de admin robada, el mismo paso 1 da una sesión que pasa `RecentAdmin` (60 min, D9) sin que el admin haya vuelto a probar quién es.

Hay además un detalle de #55: `unlink` comprueba la sesión antes de abrir su transacción, pero no vuelve a comprobar dentro de ella que esa sesión siga viva. Un `logout-all` o una rotación que llegan en medio no lo frenan.

## Resultado propuesto
1. **Desvincular pide una autenticación reciente** (unos 10 min; la spec fija el número) hecha con un proveedor que **ya estaba vinculado antes**. Un proveedor vinculado en esta sesión o hace poco no cuenta. Si falta, el server responde con un código y no desvincula nada.
2. **Vincular deja de contar como reautenticación.** La sesión rotada por vincular conserva la autenticación anterior, para `RecentAdmin` y para desvincular. Solo renueva la autenticación un login con un proveedor que cumpla la misma regla de "ya estaba vinculado" del punto 1. `RecentAdmin` y desvincular usan esa regla.
3. **La sesión sabe con qué identidad se autenticó.** Guarda la cuenta exacta del proveedor con la que se hizo el login, no solo el nombre del proveedor. `unlink` evalúa si esa identidad sirve (que siga vinculada, su antigüedad y lo reciente de la autenticación) dentro de su transacción, con el usuario bloqueado.
4. **`unlink` vuelve a comprobar la sesión dentro de su transacción.** Si la cerraron entre la comprobación y la transacción, o si en medio se desvinculó la identidad de la reautenticación, no desvincula nada.
5. **La víctima puede echar al atacante.** Se reautentica con su propio proveedor, desvincula el del atacante, y con eso se revocan las sesiones del atacante y ese proveedor ya no entra.
6. **En `/account` se entiende qué pasa.** Cuando falta la autenticación reciente, el usuario lee por qué (proteger su cuenta si alguien más usa su sesión). Con un toque inicia sesión otra vez con un proveedor que ya tenía y vuelve a `/account` para terminar de desvincular. Todo en en y es, y cómodo con el pulgar a 375 px.

## Usuarios y sistemas afectados
- Usuarios: quien tiene dos proveedores y quiere quitar uno (caso poco frecuente, pero de alto daño si sale mal), y el owner como admin.
- Sistemas: `apps/server` (sesiones, login, vincular, desvincular, `RecentAdmin`, y una migración obligatoria para que la sesión guarde la identidad con la que se autenticó), `apps/web` (`/account` y sus textos), `crates/protocol` (un código nuevo o reusado, con `packages/protocol-ts` regenerado), y los tests de integración y e2e.
- No toca: `crates/engine`, `crates/sim`, `packages/renderer`, el ledger.

## Restricciones
- **ADR-0008 cambia.** D1 (desvincular) y D9 (qué cuenta como reautenticación) ganan una regla, y D3 (la rotación al vincular) precisa qué conserva la sesión rotada. Es una enmienda: la escribe el architect y la aprueban security-reviewer y el owner. Los demás números fijados no cambian (60 min de admin, 12 h y 30 días).
- **ADR-0007:** el server responde solo códigos; todo texto nuevo vive en en y es y pasa `check-i18n`.
- **ADR-0012:** ningún log nuevo lleva lo que lista A4 (ni `provider_user_id`, ni `user_id`, ni la IP), y cada fuente nueva agrega su canario.
- **ADR-0005:** vincular, desvincular y reautenticarse no crean asientos.
- **Mobile-first:** volver a iniciar sesión y regresar a `/account` funciona en el ida y vuelta de OAuth en el celular (WebKit y Chromium a 375 px en el e2e).
- **Pocos usuarios:** la regla no puede dejar fuera a un usuario legítimo que solo quiere ordenar sus proveedores. Siempre debe existir un camino para desvincular, aunque tenga que esperar o volver a entrar.
- **Funciona en local:** `just check` y `just e2e` pasan sin red, con el proveedor `dev`, que sigue las mismas reglas que Google y Discord.
- Repo público: sin datos personales ni secretos en los tests ni en los docs.

## Criterios de éxito
- [ ] **El ataque de TA-29 falla.** Con solo la sesión de la víctima, el atacante vincula su proveedor X y luego intenta desvincular el de la víctima. Lo intenta enseguida, tras cerrar sesión y entrar con X, tras un `logout-all` y **después de esperar, sin importar cuánto** (entrando con X durante la espera). En ningún caso desvincula, y la víctima puede seguir entrando con su proveedor. Si la regla de la pregunta 1 no aguanta la espera, este criterio solo se puede relajar si el owner acepta ese riesgo por escrito (ver Riesgos aceptados).
- [ ] **El usuario legítimo puede desvincular.** Con dos proveedores ya vinculados, inicia sesión con uno y desvincula el otro. Con una autenticación vieja, `/account` le explica por qué y lo lleva a reautenticarse y de vuelta, en en y es, a 375 px (e2e en Chromium y WebKit).
- [ ] **La víctima echa al atacante.** Tras el paso 1 del ataque, la víctima se reautentica con su propio proveedor y desvincula X. Las sesiones del atacante quedan revocadas, sus sockets se cierran y un login con X ya no entra a la cuenta.
- [ ] **Admin:** una sesión de admin con `authenticated_at` de hace más de 60 min vincula un proveedor X. En las mismas variantes del criterio 1 (enseguida, tras cerrar sesión y entrar con X, tras `logout-all` y tras esperar), `RecentAdmin` sigue respondiendo 403 `auth.reauth_required`. Solo pasa un login con una identidad que cumpla la regla de la pregunta 1.
- [ ] **Carrera de `unlink`:** con una pausa controlada en el test (como el `LinkGate` de `link`), si en medio se cierra la sesión, o si desde otra sesión se desvincula la identidad con la que se reautenticó, no se borra el proveedor, no hay `auth_events` de `unlink` y no se revoca ninguna sesión.
- [ ] Vincular, desvincular y reautenticarse no crean asientos del ledger.
- [ ] Canarios: en una corrida de vincular, reautenticarse y desvincular, ningún log contiene lo que lista ADR-0012 A4.
- [ ] `check-i18n`, `just check` y `just e2e` pasan.

## Fuera de alcance
- **Período de espera** que impida desvincular durante un tiempo después de vincular (posible mejora futura; ver la pregunta 1).
- **Aviso de "proveedor vinculado hace poco"** en `/account` o por otro canal (posible mejora futura).
- Recuperación de cuenta, email o soporte para devolver una cuenta tomada.
- Pedir reautenticación para renombrar, `logout-all` o apostar. Vincular no se descarta: es la pregunta 7.
- Rutas de admin y peleas en vivo (TA-13), despliegue (TA-6, TA-8).

## Riesgos aceptados (pendientes del owner)
- **El atacante convive con la víctima hasta que ella note X.** Esta feature evita que se excluya a la víctima, pero no saca al atacante: con una sesión robada, X queda vinculado y le sirve para entrar (y `logout-all` no lo saca) hasta que la víctima lo vea en `/account` y lo desvincule. Con un admin, eso es acceso de admin permanente, y es el peor caso. Lo cubrirían el aviso de "vinculado hace poco" o un vincular que pida autenticación reciente (pregunta 7).
- **Si la regla de la pregunta 1 no aguanta la espera** (opción (a) sin aviso), el ataque vuelve tras el plazo. Solo vale si el owner lo acepta por escrito.

## Decisiones
- **Dirección (owner, 2026-10-30).** La sesión coordinadora le preguntó "¿Arranco la feature de TA-29 con la opción recomendada?", y el owner contestó en el chat: "Sí, arráncala". La opción recomendada: desvincular exige una autenticación reciente con un proveedor ya vinculado, y uno vinculado hace poco no cuenta; vincular no cuenta como reautenticación para `RecentAdmin`; `unlink` vuelve a comprobar la sesión en su transacción. El período de espera y el aviso quedan fuera.
- **Delegación (owner, 2026-10-29), textual:** "el merge tambien lo puedes hacer y puedes continuar con todo lo que falte sin pedirme permiso, haz todo de una vez, solo ve siguiendo los procesos de los agentes y ya. No importa por ahora que sea de alto riesgo, esto todavía esta en desarrollo y no hay nada montado asi que no pasa nada. Intenta hacer todo sin pedirme las aprobaciones directas a mi". "Todo lo que falte" es lo que la extiende a 009 y a la enmienda de ADR-0008. No cubre un tag `v*` ni un deploy.
- **Lo que sigue siendo del owner.** Aceptar un riesgo residual (ver Riesgos aceptados) o un período de espera es una decisión de producto. El coordinador se lo señala al owner en su reporte en lugar de bloquear el pipeline.

## Preguntas abiertas
Todas se resuelven **antes de aprobar la spec**. Deciden architect y security-reviewer en la spec, salvo donde se indica.

1. **¿Qué es "ya estaba vinculado antes"?** Vale igual para desvincular y para `RecentAdmin`. El criterio 1 exige que falle "vincular X, cerrar sesión, entrar con X y desvincular el original", también tras esperar. Por eso no basta con "vinculado antes de esta sesión". Opciones:
   - (a) La identidad de la reautenticación lleva vinculada más de cierto tiempo. En la práctica es un período de espera, que está fuera de alcance, y sin aviso no aguanta la espera del criterio 1: lo tiene que aceptar el owner (ver Riesgos aceptados).
   - (b) Estaba vinculada antes que el proveedor que se quiere quitar. Junto con la salida de la pregunta 2 (no se quita el proveedor con el que uno se reautenticó), el proveedor más antiguo **nunca** se puede quitar. Eso rompe la restricción "siempre debe existir un camino para desvincular".
   - (c) Otra regla, por ejemplo: quitar P exige una autenticación con P posterior al último vínculo. No obliga a esperar, pero choca con la pregunta 2 y con los proveedores deshabilitados por config.

   La spec elige y deja escrito qué cede: la espera, el camino para desvincular o la salida de la pregunta 2.
2. **La ventana justo después de un login real.** Si la víctima acaba de entrar con su proveedor y deja el navegador abierto, su propia autenticación es reciente. ¿Basta para que alguien en ese navegador desvincule ese mismo proveedor? Una salida es que el proveedor de la reautenticación no pueda ser el que se quita. La spec decide y deja escrito el riesgo que queda.
3. **Qué conserva la sesión rotada por vincular.** Además de la autenticación anterior, ¿conserva su expiración absoluta, para que vincular no alargue los 30 días ni las 12 h de admin?
4. **La ventana exacta** (unos 10 min propuestos) y si es la misma para desvincular que la de 60 min de admin o una aparte.
5. **El código de error y el flujo de vuelta:** reusar `auth.reauth_required` o crear uno propio, y cómo vuelve el usuario a `/account` después de reautenticarse. El flujo de reautenticación queda atado al usuario de la sesión, como `link_user_id` en D2 (¿con un `purpose` propio?). Nunca crea un usuario ni cambia a otro, y si la identidad es de otro usuario responde con un código y deja la sesión como estaba. El `next` de vuelta sigue la regla de D2.
6. **Sesiones que ya existen** al aplicar el cambio (hoy no hay nada desplegado): si cuentan como reautenticación vieja.
7. **¿Vincular también pide una autenticación reciente** con un proveedor ya vinculado? Cortaría el ataque en el paso 1 cuando la sesión robada es vieja, y achica el riesgo de convivencia. Decide la spec; si no entra, queda en Riesgos aceptados.

## Revisión

### security-reviewer — 2026-10-30 — REQUEST_CHANGES
**🤖 security-reviewer**

- Verifiqué: el intent completo contra ADR-0008 (D1, D2, D3, D9, D10 y Consecuencias), ADR-0005, ADR-0007 y ADR-0012 A4, y contra el código de `682a47f`. El problema está bien descrito. `sessions.authenticated_at` tiene `DEFAULT now()` en `20261028000000_auth.sql`, y la rotación al vincular hace un `INSERT INTO sessions` sin esa columna (`sessions/mod.rs`), así que la sesión rotada sale "recién autenticada". `auth/unlink.rs` llama a `session::current()` fuera de la transacción y, dentro de ella, solo bloquea `users`; no vuelve a mirar la fila de `sessions`. `sessions` no guarda con qué identidad se autenticó la sesión, y `oauth_accounts` tiene `created_at`, que alcanza para las opciones (a) y (b) de la pregunta 1. El frontmatter es válido según `docs/process.md`, usa los términos del glosario y no contradice ningún ADR `approved`: la enmienda de ADR-0008 está anunciada en Restricciones. La dirección del owner está citada con sus palabras. Su delegación del 2026-10-29 dice "todo lo que falte" y la nota de 008 la acota a "en 008"; la acepto para 009 como lo indica la sesión coordinadora (ver Minor 3).
- Cambios pedidos:
  1. **[Important] El criterio 1 no prueba el ataque cuando el atacante espera.** El criterio prueba "enseguida", "tras cerrar sesión y entrar con X" y "tras un `logout-all`", pero no "después de esperar". *Escenario:* la spec elige la opción (a) de la pregunta 1 con un umbral de N días. El atacante vincula X con la sesión robada, espera N días entrando con X y después desvincula el proveedor de la víctima. Los tres casos del criterio 1 pasan, pero la cuenta se pierde igual. Como el aviso está fuera de alcance, la víctima no recibe ninguna señal durante la espera: un período de espera sin aviso solo retrasa la toma. *Cambio:* que el criterio 1 diga "en ningún momento después de vincular, sin importar cuánto espere", o que la pregunta 1 diga explícitamente que con (a), sin aviso, el ataque queda abierto tras la espera y que el owner tiene que aceptar ese riesgo por escrito. Además, que la pregunta 1 anote el choque con la restricción "Siempre debe existir un camino para desvincular". Con (b) más la salida de la pregunta 2 (el proveedor de la reautenticación no puede ser el que se quita), el proveedor más antiguo nunca se puede quitar. La spec tiene que elegir cuál de las dos cosas cede y dejarlo escrito.
  2. **[Important] El resultado 2 deja abierto `RecentAdmin` con logout y login.** Dice: "Solo un login con un proveedor que ya estaba vinculado renueva la autenticación", y el criterio 3 dice: "Un login nuevo con un proveedor ya vinculado sí pasa". *Escenario:* con una sesión de admin robada, el atacante vincula X, cierra sesión y entra con X. Al momento del login, X "ya estaba vinculado", así que la sesión nueva tiene `authenticated_at = now()` y pasa `RecentAdmin`. Es el mismo bypass que el resultado 2 quiere cerrar, con un paso más. El criterio 1 cubre esta variante para desvincular, pero el criterio 3 no la cubre para admin. *Cambio:* que el criterio 3 incluya "vincular X, cerrar sesión, entrar con X → sigue en 403 `auth.reauth_required`", y que la pregunta 1 aplique a `RecentAdmin` la misma regla de "ya estaba vinculado". Si la spec decide otra cosa para admin (por ejemplo, que vincular exija `RecentAdmin`), que deje escrito el riesgo que queda.
  3. **[Important] Falta el camino de la víctima para echar al atacante, y no está escrito el riesgo que queda.** Aunque desvincular quede bien protegido, el atacante que vinculó X conserva el acceso: `logout-all` no lo saca, porque vuelve a entrar con X. Con el período de espera y el aviso fuera de alcance, el único remedio es que la víctima desvincule X. *Escenario:* la spec elige una regla que, por accidente, también bloquea a la víctima (por ejemplo, una ventana o un orden mal definidos). El atacante queda dentro para siempre y ningún criterio lo detecta. *Cambio:* (a) un criterio de éxito: la víctima se reautentica con su proveedor, desvincula X, las sesiones del atacante se revocan (y sus sockets se cierran) y X ya no entra; (b) anotar en el intent como riesgo aceptado que, con una sesión robada, el atacante mantiene el acceso hasta que la víctima note X en `/account`; (c) una pregunta abierta para la spec: si **vincular** también debe exigir una autenticación reciente con un proveedor ya vinculado. Eso corta el ataque en el paso 1, y "Fuera de alcance" no lo descarta (solo nombra renombrar, `logout-all` y apuestas).
  4. **[Minor] Pregunta 5: atar el flujo de reautenticación al usuario de la sesión.** Si la reautenticación es un login normal (`purpose = login`), el usuario que en el proveedor elige otra cuenta termina en otro usuario o crea uno nuevo, y pierde la sesión en la que estaba. Lo mismo pasa con un login CSRF en ese ida y vuelta. Que la pregunta 5 pregunte si va un `purpose` propio, atado a la sesión como `link_user_id` en D2, que nunca cree ni cambie de usuario y que responda con un código si la identidad es de otro usuario. El `next` de vuelta a `/account` sigue la regla de D2.
  5. **[Minor] Qué identidad se guarda y dónde se comprueba.** La sesión tiene que guardar la identidad exacta con la que se autenticó (la fila de `oauth_accounts`, no solo el nombre del proveedor). La elegibilidad (esa fila sigue existiendo, su antigüedad y la frescura de `authenticated_at`) se evalúa **dentro** de la transacción de `unlink`, con el bloqueo del usuario. *Escenario:* una regla por nombre de proveedor deja pasar a quien desvinculó y volvió a vincular el mismo proveedor con otra cuenta. Que el criterio de la carrera de `unlink` cubra también "el proveedor de la reautenticación se desvincula en medio, desde otra sesión".
  6. **[Minor] Alcance de la delegación.** La nota de la spec de 008 dice "las aprobaciones del owner en 008". Que la Decisión del intent cite también el texto original ("puedes continuar con todo lo que falte"), que es lo que la extiende a 009 y a la enmienda de ADR-0008. Así nadie tiene que deducirlo.
- Preguntas abiertas: las 6 pueden quedar para la spec. Con los cambios 1, 2 y 3, cubren las variantes del ataque (inmediata, con logout y login, con `logout-all`, con espera, por admin y por carrera) y los riesgos que quedan.

### architect — 2026-10-30 — REQUEST_CHANGES
- Verifiqué:
  - **Viabilidad.** Cabe en el stack sin dependencias ni servicios nuevos y sin costo en el VPS: es una columna o dos en `sessions`, una condición más en el lookup de `sessions/mod.rs` (el `authenticated_at > now() - interval '60 minutes'` de `RecentAdmin` ya se calcula ahí), conservar `authenticated_at` y `expires_at` en el `sessions::issue` que llama `link::attempt`, y una comprobación más en `unlink`. No toca `crates/engine`, `crates/sim`, `packages/renderer` ni el ledger, como dice el intent.
  - **Resultado 3** es viable tal cual. `unlink` hoy usa `info` de `session::current()` y dentro de la transacción solo bloquea `users`. El patrón ya existe en `link::attempt` (`sessions::revoke(RevokeFilter::One)` y comparar con la sesión esperada), y el `LinkGate` sirve de molde para la pausa del test. El criterio de la carrera se puede verificar.
  - **ADRs.** No contradice ningún ADR `approved`. Nombra bien la enmienda de ADR-0008 (D1, D3, D9) y cita ADR-0005, ADR-0007 y ADR-0012 A4. La delegación del owner del 2026-10-29 existe, textual, en la spec de 008 ("Revisión → owner"), y cubre la aprobación de la enmienda mientras nada esté desplegado. La cita de "Decisiones" es la palabra del owner, así que vale como decisión.
  - **Pruebas sin red.** `apps/server/tests/support/oauth.rs` ya simula proveedores, así que el ataque con dos proveedores se puede probar en integración sin red. En el e2e solo hay `dev`, y Google se siembra en la base (`seedOauthAccount`). `UNIQUE (user_id, provider)` impide dos cuentas `dev`, así que el criterio 2 (entrar con `dev`, quitar el Google sembrado) es viable, pero no se puede entrar con el proveedor sembrado. Lo tengo en cuenta en la spec.
  - **Frontmatter** completo y válido. Las preguntas abiertas quedan arrastradas a la spec de forma explícita.
- Cambios pedidos:
  1. **[Major] El criterio de admin no cubre el ataque que sí cubre el criterio 1.** El criterio 3 solo prueba "vincular y pedir `RecentAdmin` enseguida". No prueba "vincular X, cerrar sesión, entrar con X y pedir `RecentAdmin`", que es la variante que el criterio 1 sí exige para desvincular, y que el Resultado 2 ("solo un login con un proveedor que ya estaba vinculado renueva la autenticación") promete para los dos. Pido que el criterio 3 repita las tres variantes del criterio 1 (enseguida, tras cerrar sesión y entrar con X, tras `logout-all`), con la regla que fije la pregunta 1.
  2. **[Major] Falta escribir el riesgo que queda.** Esta feature evita que se **excluya** a la víctima. No evita que el atacante **conviva** con ella: X sigue vinculado y es una forma de entrar para siempre, hasta que la víctima lo vea y lo quite, y el aviso queda fuera. Con un admin, eso es acceso de admin permanente. Con la regla (a) de la pregunta 1, el atacante solo tiene que esperar el plazo y entrar con X. Pido una de dos cosas. Puede ser una línea en "Fuera de alcance" que lo nombre como riesgo aceptado, con la nota de que lo cubriría el aviso o un vincular que pida autenticación reciente. O puede ser una pregunta abierta 7 para la spec: ¿vincular pide una autenticación reciente con un proveedor ya vinculado? Eso corta el paso 1 del ataque cuando la sesión robada es vieja. No pido decidirlo aquí.
- Notas para la spec (no bloquean, las resuelvo al escribirla):
  - **Pregunta 1, opción (b).** Hay dos proveedores reales. Con (b), quien entró con Google y vinculó Discord **nunca** puede quitar Google, porque Discord es más nuevo. Eso choca con la restricción "siempre debe existir un camino". Con (a), es un período de espera, y eso va al owner. Evaluaré también una regla (c): quitar P exige una autenticación con P, posterior al último vínculo. No obliga a esperar y el atacante no tiene P, pero choca con la salida que propone la pregunta 2. Tampoco sirve para un proveedor deshabilitado por config, que hoy `unlink` sí deja quitar.
  - El server no sabe hoy con qué proveedor se autenticó una sesión (`sessions` no tiene la columna). Cualquier regla de la pregunta 1 necesita una migración, no "si hace falta".

### architect — 2026-10-30 — APPROVE (ronda 2)
- Verifiqué, solo lo que cambió:
  - **Mi cambio 1, resuelto.** El criterio de admin repite las cuatro variantes del criterio 1 (enseguida, tras cerrar sesión y entrar con X, tras `logout-all` y tras esperar), y el Resultado 2 aplica la regla de la pregunta 1 a `RecentAdmin`.
  - **Mi cambio 2, resuelto.** "Riesgos aceptados (pendientes del owner)" nombra la convivencia del atacante, con el admin como peor caso. La pregunta 7 (¿vincular pide autenticación reciente?) queda para la spec, y "Fuera de alcance" ya no la descarta.
  - **Viabilidad de lo nuevo.** Los Resultados 3 a 5 caben en el diseño que ya revisé: la columna de la identidad de la sesión, la comprobación dentro de la transacción de `unlink` con el usuario bloqueado, y la revocación que `unlink` ya hace con `RevokeFilter::OthersOfUser`. El criterio de "echar al atacante" se prueba en integración con los proveedores simulados de `tests/support/oauth.rs`. El criterio 1 "sin importar cuánto espere" se puede cumplir sin período de espera, con una regla del tipo (c) que pida probar la identidad que se quita, así que no deja la feature sin salida. La migración ya figura como obligatoria.
  - **Forma.** El frontmatter es válido, las 7 preguntas quedan arrastradas a la spec, y la delegación se cita textual con su alcance (sin tag `v*` ni deploy). Las aceptaciones de riesgo quedan señaladas como del owner y no bloquean este intent.
- Nota para la spec (no bloquea): una regla del tipo (c) es por proveedor que se quita, y no se traduce a `RecentAdmin`. Para admin, "tras esperar" probablemente exige la pregunta 7, por ejemplo que un admin solo pueda vincular con `RecentAdmin`. Si la spec usa una regla distinta para admin, deja escrito que se aparta de la letra del Resultado 2 ("la misma regla").

### security-reviewer — 2026-10-30 — APPROVE (ronda 2)
**🤖 security-reviewer**

- Verifiqué solo lo que cambió, contra mis seis pedidos de la ronda 1:
  1. **Espera: cerrado.** El criterio 1 agrega "después de esperar, sin importar cuánto (entrando con X durante la espera)", y relajarlo exige que el owner acepte el riesgo por escrito. La pregunta 1 explica por qué (a) no aguanta la espera sin aviso, que (b) junto con la pregunta 2 deja el proveedor más antiguo sin poder quitarse, y suma una (c). La spec tiene que escribir qué cede.
  2. **`RecentAdmin`: cerrado.** El resultado 2 aplica a `RecentAdmin` la misma regla del punto 1, la pregunta 1 dice que vale para los dos, y el criterio de admin repite las cuatro variantes (enseguida, logout y login con X, `logout-all` y espera).
  3. **Echar al atacante y el riesgo que queda: cerrado.** Están el resultado 5 y su criterio (sesiones revocadas, sockets cerrados y X que ya no entra). "Riesgos aceptados" nombra la convivencia, con el admin como peor caso, y la espera con (a). La pregunta 7 deja abierto que vincular pida autenticación reciente, y "Fuera de alcance" ya no lo descarta.
  4. **Flujo atado a la sesión: cerrado.** Lo dice la pregunta 5: nunca crea ni cambia de usuario, responde con un código si la identidad es de otro y deja la sesión como estaba, y `next` sigue D2.
  5. **Identidad exacta y comprobación en la transacción: cerrado.** Lo cubren los resultados 3 y 4, y la migración ahora es obligatoria. El criterio de la carrera cubre también la identidad desvinculada en medio desde otra sesión.
  6. **Delegación: cerrado.** La Decisión la cita textual y explica que "todo lo que falte" la extiende a 009 y a la enmienda de ADR-0008, sin cubrir tags ni deploys.
- No encontré problemas de seguridad nuevos en el texto revisado. Las 7 preguntas pueden quedar para la spec.
- Notas para la spec (no bloquean):
  - El criterio de admin no tiene la cláusula de relajación del criterio 1. Si la pregunta 1 termina en (a), la variante "tras esperar" de admin choca con "solo pasa un login con una identidad que cumpla la regla". La spec tiene que aplicar la misma cláusula, o una regla más estricta para admin.
  - Los "Riesgos aceptados" siguen pendientes del owner. Mientras no estén aceptados por escrito, firmo la spec solo si no relaja el criterio 1 ni el de admin. La convivencia sí puede seguir pendiente, porque esta feature no la empeora.
