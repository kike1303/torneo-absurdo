# Criterios de code review

Lo usan `reviewer` y `security-reviewer` (ver skill `review-pr`). Cada hallazgo se etiqueta con su pase.

## Pases
1. **Bugs**: errores de lógica, condiciones de carrera, manejo de errores, casos límite.
2. **Seguridad** (solo `security-reviewer`): auth y sesiones, validación de entrada, secretos, filtración de la semilla o de eventos futuros, manipulación de monedas o apuestas, rate limiting.
3. **Determinismo**: el motor sigue siendo puro y reproducible (misma semilla → mismo log), sin reloj, sin `HashMap` con orden no determinista, sin floats dependientes de la plataforma en la lógica del resultado.
4. **Cumplimiento**: el diff hace lo que dicen el `feature.md` o la descripción del PR; cualquier desvío está explicado.
5. **i18n y UI**: sin strings literales, los componentes usan tokens del theme, mobile-first, accesibilidad básica.
6. **Infra y CI**: minutos de CI, actions fijadas por SHA, imágenes y compose.

## Qué es "Important" aquí
Algo que rompe comportamiento, filtra datos o resultados, permite crear o duplicar monedas, rompe el determinismo o viola un ADR aceptado. El estilo y los nombres son **nits**.

## Regla de RF6 (spec de 010)
Cambiar o quitar una entrada de la tabla de `apps/server/tests/ruleset_freeze.rs` es **bloqueante**: rompería la verificación de las peleas pasadas de esa versión. Un cambio de reglas lleva una versión nueva.

## Regla de las liquidaciones (spec de 012)
Una liquidación nueva, o un cambio al gancho de `apps/server/src/bets/settle.rs`, nunca lee `users` ni `banned_at`: un baneo no cambia lo que se paga ni se reembolsa (RF14). Si lo hace, es **Important**.

## Límite de nits
Máximo 5 por review. El resto se resume como un conteo.

## No reportar
- Archivos generados (`packages/protocol-ts/**`, lockfiles).
- Lo que ya hacen CI, rustfmt, clippy o prettier.

## Firma
Cada comentario empieza con `**🤖 <agente>**` y termina con `Veredicto: APPROVE | REQUEST_CHANGES | COMMENT`.
