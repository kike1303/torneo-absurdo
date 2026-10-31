---
id: "aviso-privacidad"
title: Aviso de privacidad integral (borrador)
type: legal
status: in-review
risk: high
author: claude
reviewers: [security-reviewer, owner]
approvals:
  - by: security-reviewer
    date: 2026-10-21
linear: TA-5
depends_on: ["0005", "0006", "0007", "0008"]
created: 2026-10-21
updated: 2026-10-30
---

# Aviso de privacidad integral — Torneo Absurdo (borrador)

> **Notas para el equipo (no forman parte del aviso publicado):**
> - Es un borrador técnico armado a partir de ADR-0006 y ADR-0008. **Antes de publicarse necesita una revisión legal.** Se redactó con base en la Ley Federal de Protección de Datos Personales en Posesión de los Particulares (LFPDPPP) vigente desde marzo de 2025; un abogado debe confirmar los requisitos y los plazos.
> - Los campos `{{…}}` se llenan al publicar. La identidad, el domicilio y el correo de contacto del responsable **no se commitean** (el repo es público): viven en `private/` o en la configuración de despliegue.
> - El texto que se publique en la web va traducido a `es` y `en` (ADR-0007). Dónde vive (mensajes de Paraglide o `content/legal/`) se decide en la spec de la feature que publique la página.
> - Las decisiones abiertas que este aviso necesita están en "Pendiente", al final.

## 1. Responsable
`{{RESPONSABLE_NOMBRE}}`, con domicilio en `{{RESPONSABLE_DOMICILIO}}`, es responsable del tratamiento de tus datos personales en Torneo Absurdo (el "Servicio"). Para cualquier asunto de privacidad, escríbenos a `{{CONTACTO_PRIVACIDAD}}`.

## 2. Qué es el Servicio
Torneo Absurdo es un juego gratuito en el que personajes ficticios pelean en batallas simuladas y los usuarios apuestan **monedas ficticias**. Las monedas no se compran con dinero real, no se retiran y no tienen valor económico.

## 3. Datos personales que tratamos
Solo tratamos los datos necesarios para que el Servicio funcione. **No pedimos ni guardamos tu nombre real, tu correo electrónico, tu foto ni tu número de teléfono**, y no guardamos los tokens de acceso de Google ni de Discord.

| Categoría | Dato | Para qué |
|---|---|---|
| Identificación de cuenta | Identificador único que te asigna Google (`sub`) o Discord (`id`) al iniciar sesión | Reconocer tu cuenta cada vez que entras |
| Perfil público | Alias (`display_name`), generado al azar o elegido por ti | Mostrarte en el ranking y en el juego |
| Actividad en el juego | Saldo y movimientos de monedas ficticias, apuestas, resultados, posición en el ranking, preferencias de idioma y de tema | Operar el juego y el ranking |
| Sesiones | Fechas de inicio y último uso de cada sesión, el proveedor con el que iniciaste sesión o confirmaste tu identidad, y cuándo, familia de navegador y sistema operativo (p. ej. "Chrome en Android", sin versión exacta) y un seudónimo de tu IP | Mantener tu sesión segura y dejarte ver y cerrar tus sesiones |
| Historial de eventos de seguridad | Tipo de evento (inicio de sesión, vinculación de proveedor, desvinculación de proveedor, confirmación de identidad, bono, rescate, apuesta), el proveedor usado cuando aplica, fecha y un seudónimo de tu IP | Limitar abusos y relacionar cuentas entre sí para detectar granjas de cuentas |
| Datos temporales de inicio de sesión | Proveedor elegido, página a la que volver y, si vinculas un proveedor o confirmas tu identidad, tu identificador interno | Completar el inicio de sesión de forma segura; se borran en 10 minutos o al completarlo |
| Registros técnicos del servidor | Por cada petición: método, ruta genérica (por ejemplo `/api/health`, sin parámetros ni consulta), código de resultado, duración, fecha y hora, y un identificador aleatorio de la petición que no tiene relación contigo. **Sin IP, cookies, cabeceras, navegador ni identificadores de usuario** | Diagnosticar fallas y mantener el Servicio |
| Registros de moderación | Sanciones y acciones de administración que te afecten, con su motivo | Aplicar las reglas del juego y poder revisar decisiones |

**Sobre el seudónimo de tu IP:** no guardamos tu dirección IP. Guardamos un valor calculado a partir de ella con una clave secreta (`ip_hash`, un HMAC), que nos permite saber si dos eventos vienen de la misma red sin ver la IP. En IPv6 se calcula sobre el prefijo de red (`/64`), no sobre la dirección completa. Sin esa clave no podemos obtener tu IP a partir del seudónimo, y la clave nunca se entrega a nadie.

No tratamos datos personales sensibles.

## 4. Finalidades
**Finalidades necesarias** (sin ellas no podemos darte el Servicio):
1. Crear y mantener tu cuenta e identificarte cuando inicias sesión con Google o Discord.
2. Operar el juego: entregar el bono diario y el rescate, registrar apuestas, liquidar peleas y calcular el ranking.
3. Mostrar tu alias y tu posición en el ranking público.
4. Proteger el Servicio: mantener tu sesión segura, limitar peticiones excesivas y detectar granjas de cuentas o abusos del sistema de monedas. Para esto usamos el seudónimo de tu IP para **relacionar cuentas entre sí**. Un reporte automático puede señalar cuentas para revisión, pero **ninguna sanción se aplica sin la revisión de una persona**, y compartir red con otra cuenta no basta por sí solo para sancionar.
5. Cumplir obligaciones legales y atender requerimientos de autoridades competentes.

**Finalidades secundarias:** ninguna. No usamos tus datos para publicidad, perfiles comerciales ni venta a terceros.

## 5. Qué ven otras personas
Tu **alias** y tus resultados en el ranking (posición y ganancia de la temporada) son públicos. Nada más de tu cuenta es visible para otros usuarios.

## 6. Cookies y tecnologías similares
Usamos solo cookies **necesarias** para que el Servicio funcione. No usamos cookies de analítica, de publicidad ni de terceros.

| Cookie | Para qué | Duración |
|---|---|---|
| `__Host-session` | Mantener tu sesión iniciada | 30 días en tu navegador; tu sesión deja de valer antes si no la usas en 7 días o si cierras sesión |
| `__Host-oauth` | Proteger el inicio de sesión con Google o Discord | 10 minutos |
| `locale` | Recordar tu idioma | 1 año |
| `theme` | Recordar tema claro, oscuro o del sistema | 1 año |

Puedes borrar las cookies desde tu navegador. Si borras `__Host-session`, se cerrará tu sesión.

## 7. Con quién compartimos tus datos
**No transferimos tus datos personales a terceros.** Los siguientes proveedores los tratan **por cuenta nuestra**, como encargados, solo para prestarnos su servicio:

| Proveedor | Servicio | Datos |
|---|---|---|
| Hostinger | Servidor donde corre el Servicio | Todos los de la sección 3 |
| Cloudflare (R2) | Almacenamiento cifrado de copias de seguridad | Copias de la base de datos |

Cuando inicias sesión con **Google** o **Discord**, ese proveedor sabe que entraste a Torneo Absurdo. Su tratamiento se rige por sus propios avisos de privacidad.

Algunos de estos proveedores pueden almacenar datos fuera de México.

## 8. Cuánto tiempo conservamos tus datos

| Dato | Plazo |
|---|---|
| Cuenta, alias y actividad en el juego | Mientras tu cuenta exista |
| Sesiones, con su seudónimo de IP y familia de navegador | Hasta que caducan o cierras sesión (máximo 30 días) |
| Historial de eventos de seguridad | El mayor entre 30 días y la duración de la temporada más 7 días |
| Datos temporales de inicio de sesión | 10 minutos, o menos si completas el inicio de sesión |
| Registros técnicos del servidor | Se sobrescriben automáticamente por volumen; normalmente en menos de 3 semanas |
| Copias de seguridad | 7 días en el servidor y 30 días en Cloudflare R2 |
| Registros de moderación | `{{PLAZO_REGISTROS_MODERACION}}` |

La aplicación web también guarda registros técnicos, pero **solo de errores** (no registra cada visita) y sin datos tuyos: ni IP, ni cookies, ni cabeceras, ni la dirección completa de la página. Como escribe poco, esos registros pueden durar más que los del servidor antes de sobrescribirse.

La base de datos también guarda registros técnicos (arranque, mantenimiento y errores), configurados para que **no contengan datos tuyos**: ni el contenido de tus datos, ni las consultas, ni tu IP. Como escribe poco, esos registros pueden durar más de 3 semanas antes de sobrescribirse.

El registro de movimientos de monedas es inmutable por diseño. Si eliminas tu cuenta, borramos en ese momento tus identificadores de Google o Discord, tus sesiones, tu alias y tu historial de eventos de seguridad. Tus movimientos de monedas, apuestas y resultados se conservan **seudonimizados**: ligados a un identificador interno, sin tu alias ni tu cuenta de Google o Discord. Además se conservan:
- los registros de moderación que te afecten, durante su plazo;
- tus datos en las copias de seguridad, hasta que estas caducan (máximo 30 días);
- los registros técnicos, hasta que se sobrescriben.

## 9. Tus derechos (ARCO) y cómo ejercerlos
Tienes derecho a **acceder** a tus datos, **rectificarlos**, **cancelarlos** (eliminarlos) y **oponerte** a su tratamiento. También puedes **revocar tu consentimiento** o **limitar** el uso de tus datos.

- **Desde tu cuenta:** puedes cambiar tu alias, cerrar todas tus sesiones y desvincular proveedores. `{{SI_APLICA: eliminar tu cuenta}}`
- **Por correo:** escribe a `{{CONTACTO_PRIVACIDAD}}` con tu alias, el derecho que quieres ejercer y lo que pides.
- **Para proteger tu cuenta, siempre confirmamos tu identidad**: te pediremos que inicies sesión y confirmes la solicitud desde tu cuenta. Como tu alias es público, no basta con conocerlo.
- Si pides acceso a tus datos, te entregamos solo los tuyos, nunca datos de otras cuentas.
- Te responderemos en un máximo de `{{PLAZO_RESPUESTA}}` y, si procede, aplicaremos lo que pediste en un máximo de `{{PLAZO_EJECUCION}}`.

Si consideras que no atendimos tu solicitud, puedes acudir a la autoridad de protección de datos personales competente.

## 10. Edad mínima
El Servicio es solo para personas de `{{EDAD_MINIMA}}` años o más. No recopilamos a sabiendas datos de menores de esa edad. Si detectamos una cuenta de un menor, la eliminamos.

## 11. Cambios a este aviso
Publicaremos cualquier cambio en esta misma página con su fecha de actualización. Si el cambio es importante, también lo avisaremos dentro del Servicio.

Última actualización: `{{FECHA_PUBLICACION}}`.

---

## Pendiente (no se publica)
Decisiones que este aviso necesita antes de publicarse:
1. **Datos del responsable:** nombre o razón social, domicilio y correo de privacidad (owner). Van fuera del repo.
2. **Edad mínima:** recomiendo **18 años**. Aunque las monedas sean ficticias, las tiendas de apps y la regulación tratan las apuestas simuladas con criterios de juego de azar, y 18 evita discusiones (owner).
3. **Eliminación de cuenta:** ningún ADR define todavía cómo se borra una cuenta con un ledger inmutable. Este aviso asume una **seudonimización**: se borran en el acto `oauth_accounts`, `sessions`, el alias y las filas de `auth_events` del usuario, y los asientos del ledger, `bets` y `season_stats` quedan atados al `user_id` sin datos. Lo tiene que confirmar el architect en la spec de la feature de cuentas, junto con el flujo "eliminar mi cuenta" (security-reviewer). Esa spec también debe evitar que un usuario baneado se borre y vuelva con el mismo Google o Discord: por ejemplo, conservando para cuentas baneadas un HMAC de `(provider, provider_user_id)` como lista de bloqueo. Si se adopta, se declara en este aviso con su plazo.
4. **Plazos:** de respuesta a solicitudes ARCO y de conservación de los registros de moderación. Un abogado los debe confirmar contra la LFPDPPP vigente.
5. **Cookies de preferencia:** la cookie de idioma ya quedó fijada en la spec de 001 (`locale`, 1 año, solo por elección explícita en el selector). Falta confirmar con legal si las cookies de preferencia (`locale` y `theme`) cuentan como necesarias o se declaran aparte como "de preferencia".
6. **Revisión legal** del texto completo antes de publicarlo. Puntos concretos: la autoridad competente con la LFPDPPP de 2025, si hace falta consentimiento expreso o basta el aviso, los medios para limitar el uso y revocar el consentimiento, los plazos ARCO, los tratamientos automatizados (`farm-report`) y en qué países guardan datos Hostinger y R2.
7. **Retención de `auth_events`:** la spec de auth debe fijar si el job de retención borra la fila completa o solo el `ip_hash`. Recomendación: la fila completa. Este aviso asume eso.
8. **Logs de `web` y del server:** resuelto en la spec de 001. `web` no registra peticiones, solo arranque y errores, nunca `getClientAddress()`, `X-Forwarded-For`, cookies ni headers (RF-A19). Un error se registra por su código (`error_code`), o por su nombre (`error_name`) y los marcos del stack, y nunca por su mensaje: `error_message` ya no existe ([ADR-0012](../decisions/0012-logs-y-cabeceras-de-seguridad.md) A5). El server escribe una línea por petición con solo `ts`, `level`, `method`, `route` (la plantilla), `status`, `latency_ms` y `request_id` aleatorio (RF-B15), y lo prueban canarios contra el binario. **Plazo medido en la fase B** (2026-10-24, binario `--release`): la línea de `/api/health` mide 192 B, 290 B con el envoltorio de `json-file`; con 5,760 healthchecks al día son unos 1.67 MB/día, así que 3 × 10 MiB duran como máximo 18.8 días sin tráfico ni despliegues. Por la regla de `plan-b.md` (menos de 19 días), el aviso dice "menos de 3 semanas". Falta que la fase C lo confirme con `prod-local` (1 h en reposo) y que la feature de auth actualice la fila "Registros técnicos" si agrega un campo (por ejemplo `user_id`).
9. **Flujo ARCO** (spec de cuentas): la verificación desde la sesión es obligatoria. La respuesta de acceso incluye solo datos propios, nunca cuentas correlacionadas, el contenido de `farm-report` sobre terceros ni `IP_HASH_KEY`.

## Revisión
### security-reviewer — 2026-10-21 — REQUEST_CHANGES
**🤖 security-reviewer**

- Verifiqué: frontmatter válido según `docs/process.md` (`type: legal` existe; `risk: high`; `depends_on` correcto). Contrasté cada fila de las secciones 3, 6, 7 y 8 con ADR-0008 D1–D10 (columnas de `users`, `oauth_accounts`, `sessions`, `oauth_flows`, `auth_events`, `admin_audit`; scopes `openid`/`identify`; tokens de proveedor no persistidos; `ip_hash` como HMAC sobre `ip_key`; retención `max(30 días, temporada + 7 días)`; logs redactados de `tower-http::trace`), con ADR-0006 D1, D9 y D11 (Hostinger, logs de Docker 3 × 10 MB, backups de 7 días locales y 30 en R2 cifrado, logs de acceso de Traefik apagados global y por router, activación de diagnóstico de 7 días como máximo sin `ClientAddr`), con ADR-0005 (ledger inmutable) y con ADR-0007 y ADR-0009 D3 (cookie de idioma y cookie `theme`). Son correctos y coinciden con los ADRs: no se guarda email, nombre, foto ni tokens de proveedor; el alias no se copia del proveedor; `__Host-session` y `__Host-oauth` con sus duraciones; Hostinger y Cloudflare R2 como encargados; los plazos de los backups; que no hay analítica ni terceros (ADR-0009: fuentes autoalojadas, sin CDN). La afirmación "la IP en claro no se guarda" **se sostiene** para la BD, los logs del server y Traefik, siempre que D11 de ADR-0006 se aplique y pase su verificación 13 (ver punto 4 para `web`).
- Los placeholders `{{…}}` y la sección "Pendiente" no cuentan como hallazgos.

**Cambios pedidos (bloquean):**

1. **Falta declarar `auth_events`.** ADR-0008 D8 guarda una fila con `ip_hash` por cada login, vinculación, bono, rescate y apuesta (con `bet_id`). El aviso solo menciona "fechas de inicio y último uso de tus sesiones", así que esconde que existe un historial por evento que liga cada apuesta a una huella de IP y que el admin usa para correlacionar cuentas. *Escenario:* un usuario sancionado por `farm-report` pide acceso (ARCO) y descubre que lo correlacionaron con un historial de IP por apuesta que el aviso nunca declaró. El tratamiento queda fuera del aviso y la sanción es impugnable. *Cambio:* en la sección 3, agregar "Historial de eventos de seguridad: tipo de evento (inicio de sesión, vinculación, bono, rescate, apuesta), fecha y huella de IP". En la sección 4, finalidad 4, decir que la huella se usa para **relacionar cuentas entre sí**.
2. **La sección 8 mezcla plazos distintos.** `ip_hash` y `user_agent_family` de `sessions` se borran con la sesión (30 días como máximo). El plazo `max(30 días, temporada + 7 días)` aplica al `ip_hash` de `auth_events`. ADR-0008 no pone `user_agent_family` en `auth_events`. *Escenario:* el aviso promete conservar la familia de navegador "temporada + 7 días", pero el sistema la guarda menos tiempo. Al revés pasa con el resto de la fila de `auth_events` (tipo, fecha, `bet_id`, `user_id`): ningún ADR fija su plazo, así que hoy se conservaría indefinidamente sin estar declarado. *Cambio:* separar las filas ("Sesiones, con su huella de IP y familia de navegador: hasta 30 días"; "Historial de eventos de seguridad: `max(30 días, temporada + 7 días)`") y agregar a "Pendiente" que la spec de auth fije si el job borra la fila completa de `auth_events` o solo el `ip_hash`. Mi recomendación: la fila completa.
3. **"Sin ningún dato que te identifique" es falso con el diseño actual.** Tras la seudonimización de "Pendiente" punto 3, el `user_id` sigue en `ledger_entries`, `bets`, `season_stats` y `admin_audit` (solo de inserción, con `target` y un `payload` que lista usuarios marcados), y `auth_events` lo enlaza con un `ip_hash` hasta que vence su plazo. Eso es un **dato seudonimizado**, que sigue siendo dato personal mientras exista cómo reidentificarlo; no es un dato anónimo. *Escenario:* un usuario elimina su cuenta y vuelve a entrar desde la misma red. Su nuevo `ip_hash` coincide con el de los eventos de la cuenta "anónima" y el admin puede reidentificarla mientras siga vigente la retención. *Cambio:* decir "se conservan seudonimizados (ligados a un identificador interno sin tu alias ni tu cuenta de Google o Discord)", listar lo que se conserva (`admin_audit` por su plazo, `auth_events` hasta que vence, backups hasta 30 días) y, en "Pendiente" punto 3, pedir que la eliminación también borre en el acto las filas de `auth_events` del usuario o su `ip_hash`.
4. **Faltan los registros técnicos (logs).** El server registra cada petición con `tower-http::trace` (ruta, método, estado, fecha; en rutas autenticadas, casi seguro el `user_id` o algo que lo derive). Docker los guarda en `json-file` con rotación **por tamaño** (3 × 10 MB, ADR-0006 D1), así que no hay plazo en días. *Escenario:* en horas valle, 30 MB cubren semanas de actividad por usuario que el aviso no declara y cuyo plazo no puede prometer. *Cambio:* en la sección 3, agregar "Registros técnicos del servidor (sin IP ni cookies)" y, en la sección 8, "rotación automática por volumen; normalmente `{{PLAZO_LOGS}}`". En "Pendiente", agregar que la spec de 001 fije que `web` (adapter-node, `handleError`) **nunca** registra `getClientAddress()` ni `X-Forwarded-For`. ADR-0008 solo lo garantiza para el server, y sin esa regla la promesa de "IP en claro no se guarda" depende de un `console.log`. Lo vuelvo a revisar en el pase de Seguridad del PR.
5. **ARCO por correo: la verificación tiene que ser obligatoria, no "podemos pedirte".** Como no hay email, el alias es el único dato con el que se identifica una solicitud, y es **público** (ranking). *Escenario:* alguien escribe "soy `Tоrneo_King`, cancelen mi cuenta" o "denme mis datos" con el alias de otra persona. Si la verificación es opcional, se borra una cuenta ajena o se entregan su historial y sus huellas de IP. *Cambio:* "Para proteger tu cuenta, **siempre** confirmamos tu identidad pidiéndote que inicies sesión y hagas la solicitud desde tu cuenta (o que confirmes desde ella la que enviaste por correo)". Agregar a "Pendiente", para la spec del flujo ARCO: la respuesta de acceso incluye solo datos propios, nunca cuentas correlacionadas ni el contenido de `farm-report` sobre terceros, y nunca la `IP_HASH_KEY`.

**No bloqueantes:**

6. **"Huella cifrada"** no es preciso: un HMAC no se cifra ni se descifra. Además, si se filtra `IP_HASH_KEY`, una IPv4 se recupera por fuerza bruta (2^32). Sugiero: "un seudónimo de tu IP calculado con una clave secreta; no podemos ver tu IP a partir de él sin esa clave".
7. **`oauth_flows`** guarda durante 10 minutos el proveedor, `next` y, si vinculas, tu `user_id`. Es mínimo, pero conviene una línea: "datos temporales del inicio de sesión, que se borran en 10 minutos o al completarlo".
8. **Moderación y decisiones automatizadas.** `farm-report` marca cuentas de forma automática (concentración de contrapartes > 50 %, `ip_hash` compartido), pero la sanción la decide una persona (ADR-0008 D8). Conviene decirlo en las secciones 3 y 4: "Un reporte automático puede señalar cuentas para revisión; ninguna sanción se aplica sin revisión humana". Confirmar con legal si la LFPDPPP vigente exige algo más sobre tratamientos automatizados.
9. **Cookies.** La cookie de tema ya está fijada: se llama `theme` y dura 1 año (ADR-0009 D3), así que ese placeholder puede llenarse. `__Host-session`: la cookie dura 30 días en el navegador; los 7 días de inactividad los aplica el server. Sugiero "tu sesión deja de valer si no la usas en 7 días". Confirmar con legal si las cookies de preferencia (idioma, tema) entran en "necesarias" o se declaran como "de preferencia".
10. **Confirmar con legal (no bloquea):** la autoridad a la que se acude (con la LFPDPPP de 2025 ya no es el INAI); si el tratamiento necesario para la relación jurídica requiere consentimiento expreso o basta el aviso; los medios para limitar uso y divulgación y para revocar el consentimiento; los plazos ARCO; y el país de almacenamiento de Hostinger y R2 para la frase sobre datos fuera de México.
11. **Para "Pendiente" punto 3 (diseño, no del aviso):** si eliminar la cuenta borra `oauth_accounts`, un usuario baneado puede borrarse y volver a registrarse con el mismo Google o Discord, y así evadir el ban. *Escenario:* lo banean por granja, elimina la cuenta y en un minuto tiene una cuenta nueva con el mismo `sub`. La spec de cuentas debe conservar, para cuentas baneadas, un HMAC de `(provider, provider_user_id)` como lista de bloqueo, y declararlo en el aviso con su plazo.

Cuando estén resueltos 1 a 5, apruebo sin otra ronda completa: verifico solo esos puntos.

### security-reviewer — 2026-10-21 — APPROVE (ronda 2)
**🤖 security-reviewer**

- Verifiqué los puntos 1 a 5 de la ronda 1 en el texto actual:
  1. **`auth_events`:** declarado. La sección 3 tiene la fila "Historial de eventos de seguridad", con el tipo de evento, la fecha y el seudónimo de IP, y la finalidad 4 dice explícitamente que ese seudónimo se usa para **relacionar cuentas entre sí**. Resuelto.
  2. **Plazos separados:** la sección 8 distingue las sesiones (seudónimo de IP y familia de navegador, máximo 30 días) del historial de eventos (`max(30 días, temporada + 7 días)`). El punto 7 de "Pendiente" pide fijar que se borre la fila completa. Resuelto.
  3. **Seudonimización, no anonimización:** la sección 8 dice "seudonimizados", explica a qué quedan ligados los datos y lista lo que se conserva (moderación, backups hasta 30 días, logs). El historial de eventos se borra en el acto, y lo mismo dice el punto 3 de "Pendiente". Así se cierra el escenario de reidentificar la cuenta por `ip_hash`. Resuelto.
  4. **Registros técnicos:** la sección 3 los declara "sin IP ni cookies", la sección 8 dice que la rotación es por volumen y usa `{{PLAZO_LOGS}}`, y el punto 8 de "Pendiente" cubre `web` (`getClientAddress()`, `X-Forwarded-For`, cookies) y lo que registra `tower-http::trace` sobre el usuario. Resuelto. Lo vuelvo a revisar en el pase de Seguridad del PR de 001.
  5. **Verificación ARCO obligatoria:** la sección 9 dice "siempre confirmamos tu identidad", pide iniciar sesión y confirmar desde la cuenta y aclara que conocer el alias no basta. La respuesta de acceso incluye solo datos propios. El punto 9 de "Pendiente" excluye las cuentas correlacionadas, `farm-report` sobre terceros e `IP_HASH_KEY`. Resuelto.
- Nota para la spec de cuentas (no bloquea): definir cómo ejerce sus derechos ARCO un usuario baneado o que perdió el acceso a su proveedor, porque si no puede iniciar sesión, no puede verificarse. Esto no debe abrir una vía de verificación más débil que la sesión.
- Cuando la spec de auth fije lo que registra `tower-http::trace` sobre el usuario, hay que actualizar la fila "Registros técnicos" si incluye el `user_id`.
