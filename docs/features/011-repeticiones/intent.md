---
id: "011"
title: Repeticiones y verificación en el navegador
type: intent
status: approved
risk: high
author: product-owner
reviewers: [architect]
approvals:
  - by: architect
    date: 2026-11-02
linear: TA-44
depends_on: ["002", "003", "005", "010"]
created: 2026-11-02
updated: 2026-11-02
---

# Intent: Repeticiones y verificación en el navegador

## Problema
Las repeticiones están en el MVP (`docs/vision.md`, líneas 27, 38 y 49), pero hoy no hay forma de ver una pelea pasada. Con una pelea cada 10 minutos, casi nadie está conectado justo cuando pasa lo bueno: quien se la pierde no tiene cómo verla ni nada que comentar al día siguiente.

Además, la promesa de `docs/architecture.md:62` no se cumple. Desde 010 los datos para verificar son públicos, pero comprobarlos exige leer un JSON y correr el motor por tu cuenta. "Incertidumbre real" es un pilar de la visión, y para un espectador solo es creíble si lo puede comprobar sin ser programador.

## Resultado propuesto
1. **Se puede ver lo que te perdiste.** Una lista de peleas pasadas lleva a una página donde cualquiera, con o sin sesión, ve una pelea `settled` con final `decided` (KO, decisión o empate), reproducida desde su log guardado. Puede verla completa, reiniciarla y adelantarla.
2. **Se puede comprobar.** En esa página, el navegador comprueba con `engine-wasm` si el log coincide con la semilla y el commit, y lo dice en lenguaje de espectador: coincide, no coincide o no se pudo comprobar.
3. **Toda pelea terminada se explica.** Una `cancelled` o `voided` aparece como tal, con su motivo, sin reproductor ni log, y no parece un error.
4. **El vivo sigue protegido.** Nada de esto muestra una pelea sin liquidar como repetición ni adelanta su resultado.

## Usuarios y sistemas afectados
- Usuarios: el espectador que se perdió una pelea, casi siempre en el celular y a menudo anónimo; quien desconfía y quiere comprobarla; el apostador, cuando exista TA-27, que querrá revisar la pelea que decidió su apuesta.
- Sistemas: `apps/web`, `crates/engine-wasm` y su entrega al navegador (el build y la imagen de `web`, que hoy no incluyen `crates/`), `apps/server` y `crates/protocol` con `packages/protocol-ts` regenerado si listar pide una ruta de lectura nueva, y `packages/renderer` solo si los controles lo requieren.
- No debería cambiar: `crates/engine`, el ledger y el ciclo de vida de la pelea.

## Restricciones
- **Riesgo alto:** relaja la CSP con `'wasm-unsafe-eval'`, y ADR-0012 B6 hace `risk: high` a todo lo que la relaje; además, toca la exposición de la semilla, aunque solo la de peleas ya reveladas.
- **ADR-0002 y spec de 010 (RF26 y RF27):** semilla y log nunca antes de `settled`, ni por WS. Toda ruta nueva entra en el test 12 y responde igual sin sesión, con sesión y como admin.
- **ADR-0012:** la CSP solo se amplía como dice B4.1 (`'wasm-unsafe-eval'` en `script-src`, `.wasm` del mismo origen) y siguiendo B6. Según A4, ni la semilla ni el contenido del log van a los logs, tampoco después de `Settled`.
- **Lo que se promete tiene que ser cierto (spec de 010, preocupaciones 1 y 2):** comprobar no demuestra que la semilla no se eligió antes del commit, ni detecta una base reescrita si nadie archivó los datos. El texto no puede decir que hacer trampa es imposible.
- **ADR-0007:** el server solo emite códigos, y todo texto visible existe en en y es.
- **ADR-0004:** se usan los placeholders actuales. **ADR-0005:** esta feature no mueve monedas.
- **Mobile-first y recursos:** cómoda a 375 px y con el pulgar, en un VPS de 1 vCPU sin servicios externos. Pixi y el WASM se cargan solo cuando hacen falta, nunca en `/` ni para leer la lista. Tiene sentido con pocas peleas y sin nadie más conectado.
- **Repo público:** nada de negocio en docs, tests ni textos.

## Criterios de éxito
- [ ] **Lista:** muestra solo peleas `settled`, de la más reciente a la más antigua, con cruce, fecha y tipo de final. Sin peleas, tiene un estado vacío con sentido.
- [ ] **Sin fugas:** una pelea en `scheduled`, `betting_open`, `betting_closed` o `live`, o detenida, no aparece en la lista, y su URL no da reproductor, resultado, semilla ni log. La prueba lo comprueba en cada estado, como anónimo, con sesión y como admin.
- [ ] **Reproducción:** una pelea por KO, una por decisión y un empate se pueden ver completas, reiniciar y adelantar, y terminan con el resultado publicado.
- [ ] **Sin apagones:** sin WebGL se sigue viendo el resultado en texto y se puede verificar; si el WASM falla, la pelea se sigue viendo y la verificación dice que no se pudo comprobar.
- [ ] **Verificación:** con una pelea real dice que coincide. Con el log, la semilla o el commit alterados dice que no coincide, y un fallo técnico nunca se presenta como ninguno de los dos. El resultado coincide con el de la comprobación nativa de RF7 de 010.
- [ ] **Vigencia:** una pelea sigue reproduciéndose y verificándose después de cambiar `content/`.
- [ ] **Finales sin log:** una `cancelled` y una `voided` muestran su motivo, sin reproductor. Una `voided` se prueba con datos simulados, porque ningún flujo real llega a ella hasta TA-27.
- [ ] **Logs:** en una corrida con reproducción y verificación, los canarios de ADR-0012 A4 no encuentran la semilla ni contenido del log.
- [ ] **CSP y carga:** sin violaciones en Chromium ni WebKit en las rutas nuevas, y solo con la ampliación de B4.1 en la allowlist. `/` y la lista no cargan Pixi ni el WASM al abrir.
- [ ] **Calidad:** e2e en en y es, a 375 px, en Chromium y WebKit y sin sesión. `check-i18n`, `just check` y `just e2e` pasan.

## Fuera de alcance
- Apuestas (TA-27), también mostrar en la repetición lo que apostaste. Leaderboards (TA-28).
- Arte nuevo, narrador y torneos.
- Filtros, búsqueda e historial por personaje; imágenes de vista previa al compartir. Si hacen falta, van en una fase posterior.
- Cualquier tema de negocio (repo público).
- Deploy (TA-8): todo funciona en local por ahora.

## Preguntas abiertas
Las resuelve la spec antes de aprobarse.

1. **Spoilers:** la lista, ¿muestra quién ganó o lo esconde para quien quiere verla sin saberlo? El apostador quiere el resultado y el espectador que se la perdió, la sorpresa. En la página, ¿una barra de progreso delata la duración y, con ella, un KO temprano, que es lo que RF55 evita en vivo?
2. **Controles:** ¿qué significa adelantar (barra, saltos o velocidad), con qué cambios en el renderer y cómo se usa a 375 px?
3. **Alcance de la verificación:** ¿basta con semilla, commit y log, o entra todo RF7 (snapshot, final, slugs y límites)? Si no entra el final, ¿cómo se sabe que el resultado que se muestra es el del log?
4. **Cómo se presenta:** ¿corre sola o al pedirla? ¿Cuánto pesa el WASM en un celular y cuándo se descarga? ¿Qué texto es comprensible y a la vez honesto con la restricción de arriba? ¿Se muestran la semilla y el commit, y se pueden copiar?
5. **Finales sin log:** RF7 dice que una `voided` comprueba su commit y una `cancelled` su snapshot. ¿La página lo verifica o solo explica el final? ¿Esta feature deja listo el estado "no verificable" de `voided(secret_lost)` que TA-27 lleva como criterio bloqueante (preocupación 13 de 010), o lo agrega TA-27?
6. **URL de una pelea sin liquidar:** ¿qué ve quien la abre, incluida una pelea detenida? ¿Se manda al vivo, o se muestra un "todavía no"?
7. **Datos incompletos o antiguos:** ¿qué ve el espectador con `log`, `spec_a` o `spec_b` nulos (fila corrupta, DP-10 de 010), con una `ruleset_version` que el build del navegador no conoce, o con un personaje retirado de `content/`, cuyo nombre cae al respaldo de RF56?
8. **Tamaño de la lista:** ¿hasta dónde llega y cómo se pagina, con unas 52 000 peleas al año y sin índice para `settled_at` (Enmienda 3 de 010)? ¿Las repeticiones se conservan para siempre? ¿Basta `GET /api/fights/{id}` para la página y hace falta una sola ruta nueva para listar?
9. **Cómo se llega y cómo se vuelve:** además de un acceso desde `/`, ¿el resultado de `last` enlaza a su repetición? ¿El aviso de "sin conexión en vivo" (RF57) enlaza a la lista, ya que 010 dejó la "pelea con retraso" para esta UI? ¿La página avisa cuando hay una pelea en vivo, para que la repetición no le quite público a "ver juntos"?

## Revisión
### architect — 2026-11-02 — APPROVE
- Verifiqué la viabilidad contra el código, sin implementar nada: `GET /api/fights/{id}` ya da todo lo que pide RF7 de 010 y `RF27` recorre `ROUTES`, así que una ruta nueva entra sola; `load`, `seek`, `play` y `pause` del renderer alcanzan para reproducir un log guardado; `engine-wasm` compila para `wasm32-unknown-unknown` y, en Node, reproduce dos dorados byte a byte en menos de 4 ms. El módulo pesa 144 KB (62 KB con gzip), sin efecto medible en un VPS de 1 vCPU porque se calcula en el navegador, y el server solo suma una lectura paginada.
- Comprobé que no contradice ADR-0001, 0002, 0004, 0005, 0007 ni 0012: la ampliación de CSP es exactamente B4.1, y la semilla solo sale de peleas ya reveladas. El frontmatter es válido y cada criterio se puede verificar.
- Las preguntas abiertas no bloquean: el propio intent las delega en la spec, y las resuelve la [spec](./spec.md). Una precisión para ella: "tipo de final" en la lista admite dos lecturas (`kind` o KO/decisión/empate), y la segunda es un spoiler (pregunta 1).
